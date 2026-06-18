//! S3-compatible backend over `aws-sdk-s3` (spec §8) — R2 / AWS S3 / GCS / MinIO.
//!
//! Behind the optional `s3` feature (the SDK is a large dependency tree). The
//! client is built with explicit static credentials (never the ambient AWS
//! credential chain, §8), `region("auto")` for R2, an explicit `endpoint_url`,
//! and a rustls+ring TLS stack (so the build needs no `aws-lc`/cmake).
//!
//! Per-chunk integrity uses `x-amz-checksum-sha256` (base64 of the SHA-256): set
//! on PUT so the server validates server-side, and requested on HEAD/GET so
//! [`reconcile`](crate::storage::reconcile) can compare the full digest. Errors
//! are mapped to the [`BackendError`] variants the sync retry policy understands
//! (§3.4.3): `Forbidden`/`NoSuchBucket` terminal, `BadDigest` terminal, the rest
//! retryable `Io`.

use async_trait::async_trait;

use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{ChecksumAlgorithm, ChecksumMode};
use aws_sdk_s3::Client;
use aws_smithy_runtime_api::client::result::SdkError;

use super::{validate_key, BackendError, ObjectMeta, PutResult, StorageBackend};
use crate::storage::integrity::{self, Sha256Digest};

/// An S3-compatible [`StorageBackend`].
#[derive(Debug, Clone)]
pub struct S3Compat {
    client: Client,
    bucket: String,
}

impl S3Compat {
    /// Build a client for `bucket` at `endpoint` with explicit credentials.
    ///
    /// `path_style` should be `true` for MinIO-style deployments and `false` for
    /// R2/AWS (virtual-hosted, the documented default, §8).
    pub fn new(
        endpoint: &str,
        region: &str,
        bucket: &str,
        access_key_id: &str,
        secret_access_key: &str,
        path_style: bool,
    ) -> Self {
        // rustls + ring HTTP stack — no aws-lc / cmake build dependency.
        let http_client = aws_smithy_http_client::Builder::new()
            .tls_provider(aws_smithy_http_client::tls::Provider::Rustls(
                aws_smithy_http_client::tls::rustls_provider::CryptoMode::Ring,
            ))
            .build_https();

        let creds = Credentials::new(
            access_key_id,
            secret_access_key,
            None,
            None,
            "bubbaloop-storage",
        );

        let conf = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .endpoint_url(endpoint)
            .credentials_provider(creds)
            .http_client(http_client)
            .force_path_style(path_style)
            .build();

        Self {
            client: Client::from_conf(conf),
            bucket: bucket.to_string(),
        }
    }
}

#[async_trait]
impl StorageBackend for S3Compat {
    async fn put(
        &self,
        key: &str,
        bytes: &[u8],
        checksum_sha256: Option<&Sha256Digest>,
    ) -> Result<PutResult, BackendError> {
        validate_key(key)?;
        let mut req = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(bytes.to_vec()));
        if let Some(digest) = checksum_sha256 {
            // R2/S3 validates this server-side and returns BadDigest on mismatch.
            req = req
                .checksum_algorithm(ChecksumAlgorithm::Sha256)
                .checksum_sha256(integrity::to_base64(digest));
        }
        let resp = req.send().await.map_err(|e| map_err(key, e))?;
        Ok(PutResult {
            etag: resp.e_tag().map(|s| s.to_string()),
        })
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, BackendError> {
        validate_key(key)?;
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .checksum_mode(ChecksumMode::Enabled)
            .send()
            .await
            .map_err(|e| map_err(key, e))?;
        let data = resp
            .body
            .collect()
            .await
            .map_err(|e| BackendError::Io {
                key: key.to_string(),
                detail: format!("reading body: {e}"),
            })?
            .into_bytes();
        Ok(data.to_vec())
    }

    async fn head(&self, key: &str) -> Result<Option<ObjectMeta>, BackendError> {
        validate_key(key)?;
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .checksum_mode(ChecksumMode::Enabled)
            .send()
            .await
        {
            Ok(resp) => Ok(Some(ObjectMeta {
                key: key.to_string(),
                size_bytes: resp.content_length().unwrap_or(0).max(0) as u64,
                etag: resp.e_tag().map(|s| s.to_string()),
                sha256: resp.checksum_sha256().and_then(decode_b64_digest),
            })),
            Err(e) => {
                let mapped = map_err(key, e);
                if matches!(mapped, BackendError::NotFound { .. }) {
                    Ok(None)
                } else {
                    Err(mapped)
                }
            }
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>, BackendError> {
        let mut out = Vec::new();
        let mut continuation: Option<String> = None;
        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);
            if let Some(token) = &continuation {
                req = req.continuation_token(token);
            }
            let resp = req.send().await.map_err(|e| map_err(prefix, e))?;
            for obj in resp.contents() {
                if let Some(key) = obj.key() {
                    out.push(ObjectMeta {
                        key: key.to_string(),
                        size_bytes: obj.size().unwrap_or(0).max(0) as u64,
                        etag: obj.e_tag().map(|s| s.to_string()),
                        // ListObjectsV2 does not return checksums; HEAD does.
                        sha256: None,
                    });
                }
            }
            if resp.is_truncated().unwrap_or(false) {
                continuation = resp.next_continuation_token().map(|s| s.to_string());
                if continuation.is_none() {
                    break;
                }
            } else {
                break;
            }
        }
        out.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(out)
    }

    async fn delete(&self, key: &str) -> Result<(), BackendError> {
        validate_key(key)?;
        // S3 DeleteObject is idempotent (204 even if the key is absent).
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| map_err(key, e))?;
        Ok(())
    }
}

/// Decode a base64 `x-amz-checksum-sha256` header into a 32-byte digest.
fn decode_b64_digest(b64: &str) -> Option<Sha256Digest> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    bytes.try_into().ok()
}

/// Map an SDK error to a [`BackendError`], honoring the retryable/terminal split
/// (§3.4.3). Service errors are classified by their S3 error code; transport /
/// timeout / construction failures are retryable `Io`.
fn map_err<E, R>(key: &str, err: SdkError<E, R>) -> BackendError
where
    E: ProvideErrorMetadata + std::fmt::Debug,
    R: std::fmt::Debug,
{
    let code = err.code().unwrap_or_default().to_string();
    match code.as_str() {
        "NoSuchBucket" => BackendError::NoSuchBucket {
            bucket: key.to_string(),
        },
        "AccessDenied" | "InvalidAccessKeyId" | "SignatureDoesNotMatch" | "Forbidden" => {
            BackendError::Forbidden {
                key: key.to_string(),
            }
        }
        "NoSuchKey" | "NotFound" => BackendError::NotFound {
            key: key.to_string(),
        },
        "BadDigest"
        | "InvalidDigest"
        | "XAmzContentSHA256Mismatch"
        | "BadChecksum"
        | "InvalidChecksum" => BackendError::BadDigest {
            key: key.to_string(),
            expected: "server-validated checksum".to_string(),
            actual: "mismatch".to_string(),
        },
        // Everything else (5xx, throttling, DNS, timeouts, dispatch) is retryable.
        other => BackendError::Io {
            key: key.to_string(),
            detail: if other.is_empty() {
                format!("{err:?}")
            } else {
                format!("{other}: {err:?}")
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_digest_roundtrips() {
        let digest: Sha256Digest = [7u8; 32];
        let b64 = integrity::to_base64(&digest);
        assert_eq!(decode_b64_digest(&b64), Some(digest));
        assert_eq!(decode_b64_digest("not base64!!!"), None);
        // Wrong length decodes but fails the 32-byte conversion.
        assert_eq!(decode_b64_digest("YWJj"), None); // "abc"
    }

    #[test]
    fn client_builds_without_panicking() {
        // Construction must not require network; it just configures the client.
        let _ = S3Compat::new(
            "https://acct.r2.cloudflarestorage.com",
            "auto",
            "kornia-recordings",
            "AKID",
            "SECRET",
            false,
        );
    }
}
