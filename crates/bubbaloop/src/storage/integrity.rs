//! SHA-256 integrity primitives for the storage subsystem.
//!
//! Integrity is **mandatory** (spec §11): every finalized chunk carries a
//! SHA-256 digest embedded in its filename, recorded in `manifest.json`, sent
//! to R2 via `x-amz-checksum-sha256`, and re-verified on download. This module
//! provides the streaming digest used during chunk writes (no second pass over
//! the bytes) plus the helpers that format and verify those digests.

use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::Path;

/// A 32-byte SHA-256 digest.
pub type Sha256Digest = [u8; 32];

/// Streaming SHA-256 hasher.
///
/// Feed bytes incrementally with [`Hasher::update`] as chunk data is written,
/// then call [`Hasher::finalize`]. This yields the same digest as hashing the
/// full buffer in one shot, which is what lets the recorder compute the chunk
/// hash during the write rather than re-reading the finalized file (spec §3.3.6).
#[derive(Clone, Default)]
pub struct Hasher {
    inner: Sha256,
}

impl Hasher {
    /// Create a fresh hasher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed more bytes into the digest.
    pub fn update(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }

    /// Consume the hasher and return the final digest.
    pub fn finalize(self) -> Sha256Digest {
        self.inner.finalize().into()
    }
}

impl std::fmt::Debug for Hasher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hasher(sha256, in-progress)")
    }
}

/// Compute the SHA-256 of an in-memory buffer in one shot.
pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    let mut h = Hasher::new();
    h.update(bytes);
    h.finalize()
}

/// Stream a reader to completion and return its SHA-256 without buffering the
/// whole input in memory. Used by `sync`/`reconcile` to re-verify a chunk file
/// against its manifest digest before upload (spec §3.4.4 step 2).
pub fn sha256_reader<R: Read>(mut reader: R) -> io::Result<Sha256Digest> {
    let mut hasher = Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize())
}

/// Compute the SHA-256 of a file on disk, streaming it block by block.
pub fn sha256_file(path: impl AsRef<Path>) -> io::Result<Sha256Digest> {
    let file = std::fs::File::open(path)?;
    sha256_reader(std::io::BufReader::new(file))
}

/// Lowercase 64-char hex encoding of a digest (the form stored in `manifest.json`).
pub fn to_hex(digest: &Sha256Digest) -> String {
    hex::encode(digest)
}

/// The first 8 hex characters of a digest — the prefix embedded in chunk
/// filenames (`chunk-{idx:06d}-{sha256_prefix8}.mcap`, spec §3.3.6).
pub fn hex_prefix8(digest: &Sha256Digest) -> String {
    let full = to_hex(digest);
    full[..8].to_string()
}

/// Base64 (standard, padded) encoding of a digest — the value for the
/// `x-amz-checksum-sha256` upload header (spec §8).
pub fn to_base64(digest: &Sha256Digest) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// Parse a 64-char hex string back into a digest.
pub fn from_hex(s: &str) -> Result<Sha256Digest, IntegrityError> {
    let bytes = hex::decode(s).map_err(|_| IntegrityError::InvalidHex {
        value: s.to_string(),
    })?;
    bytes
        .try_into()
        .map_err(|_| IntegrityError::WrongLength { hex: s.to_string() })
}

/// Verify that `actual` equals `expected`, returning a typed mismatch error
/// carrying both hex digests so callers can surface a precise corruption report.
pub fn verify(expected: &Sha256Digest, actual: &Sha256Digest) -> Result<(), IntegrityError> {
    if expected == actual {
        Ok(())
    } else {
        Err(IntegrityError::Mismatch {
            expected: to_hex(expected),
            actual: to_hex(actual),
        })
    }
}

/// Errors raised by integrity checks.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IntegrityError {
    /// A digest did not match its expected value (corruption / tampering).
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    Mismatch { expected: String, actual: String },
    /// A string was not valid hexadecimal.
    #[error("invalid hex digest: {value}")]
    InvalidHex { value: String },
    /// A hex digest decoded to the wrong number of bytes (not 32).
    #[error("hex digest has wrong length (expected 32 bytes): {hex}")]
    WrongLength { hex: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // Known SHA-256 vectors (NIST / RFC 6234).
    const EMPTY_HEX: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC_HEX: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn known_vectors_one_shot() {
        assert_eq!(to_hex(&sha256(b"")), EMPTY_HEX);
        assert_eq!(to_hex(&sha256(b"abc")), ABC_HEX);
    }

    #[test]
    fn streaming_equals_one_shot() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let one_shot = sha256(&data);

        // Feed in irregular chunks to exercise the incremental path.
        let mut h = Hasher::new();
        for piece in data.chunks(7) {
            h.update(piece);
        }
        assert_eq!(h.finalize(), one_shot);

        // Reader path must agree too.
        let via_reader = sha256_reader(std::io::Cursor::new(&data)).unwrap();
        assert_eq!(via_reader, one_shot);
    }

    #[test]
    fn file_digest_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chunk.bin");
        let data = b"the quick brown fox jumps over the lazy dog";
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(data).unwrap();
        f.sync_all().unwrap();

        assert_eq!(sha256_file(&path).unwrap(), sha256(data));
    }

    #[test]
    fn hex_roundtrip_and_prefix() {
        let d = sha256(b"abc");
        assert_eq!(from_hex(&to_hex(&d)).unwrap(), d);
        assert_eq!(hex_prefix8(&d), "ba7816bf");
        assert_eq!(hex_prefix8(&d).len(), 8);
    }

    #[test]
    fn base64_checksum_header() {
        // Empty-input SHA-256 in base64 is the canonical x-amz-checksum-sha256
        // value AWS docs use for an empty object.
        assert_eq!(
            to_base64(&sha256(b"")),
            "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
        );
    }

    #[test]
    fn from_hex_rejects_bad_input() {
        assert_eq!(
            from_hex("zz"),
            Err(IntegrityError::InvalidHex {
                value: "zz".to_string()
            })
        );
        // Valid hex but wrong length.
        assert!(matches!(
            from_hex("ab"),
            Err(IntegrityError::WrongLength { .. })
        ));
    }

    #[test]
    fn verify_detects_corruption() {
        let good = sha256(b"payload");
        let mut bad = good;
        bad[0] ^= 0xff; // flip a bit
        assert!(verify(&good, &good).is_ok());
        assert!(matches!(
            verify(&good, &bad),
            Err(IntegrityError::Mismatch { .. })
        ));
    }
}
