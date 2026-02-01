use bubbaloop_schemas::Header;
use prost::Message;

/// Wrapper for extracting Header from any protobuf message where Header is field 1.
///
/// This works because protobuf ignores unknown fields during decode. A message
/// with only field 1 declared will successfully decode from any protobuf message
/// that has a Header at field 1, silently discarding all other fields.
#[derive(prost::Message)]
struct HeaderWrapper {
    #[prost(message, optional, tag = "1")]
    pub header: Option<Header>,
}

/// Extract Header metadata from raw protobuf bytes.
///
/// Returns `None` if the bytes cannot be decoded or don't have a Header at field 1.
pub fn extract_header(raw_bytes: &[u8]) -> Option<Header> {
    HeaderWrapper::decode(raw_bytes).ok()?.header
}

/// Metadata extracted from a Zenoh sample's Header + topic info.
#[derive(Debug, Clone)]
pub struct MessageMeta {
    /// Acquisition timestamp from Header (nanoseconds since epoch).
    pub timestamp_ns: i64,
    /// Publication timestamp from Header (nanoseconds since epoch).
    pub pub_time_ns: i64,
    /// Sequence number from Header.
    pub sequence: u32,
    /// Source identifier from Header.
    pub frame_id: String,
    /// Machine hostname from Header.
    pub machine_id: String,
    /// Zenoh key expression the message was published on.
    pub topic: String,
    /// Fully qualified protobuf type name (from ros-z or schema_hints).
    pub message_type: String,
    /// Raw payload size in bytes.
    pub data_size: u64,
}

impl MessageMeta {
    /// Build metadata from raw bytes and topic context.
    ///
    /// Falls back to wall clock time if Header extraction fails.
    pub fn from_raw(raw_bytes: &[u8], topic: &str, message_type: &str) -> Self {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        match extract_header(raw_bytes) {
            Some(header) if is_valid_header(&header) => Self {
                timestamp_ns: header.acq_time as i64,
                pub_time_ns: header.pub_time as i64,
                sequence: header.sequence,
                frame_id: header.frame_id,
                machine_id: header.machine_id,
                topic: topic.to_string(),
                message_type: message_type.to_string(),
                data_size: raw_bytes.len() as u64,
            },
            _ => Self {
                timestamp_ns: now_ns,
                pub_time_ns: 0,
                sequence: 0,
                frame_id: String::new(),
                machine_id: String::new(),
                topic: topic.to_string(),
                message_type: message_type.to_string(),
                data_size: raw_bytes.len() as u64,
            },
        }
    }
}

/// Basic sanity check on extracted header to catch garbage data.
/// A valid header should have a reasonable acquisition timestamp.
fn is_valid_header(header: &Header) -> bool {
    // Timestamp should be after 2020-01-01 (1577836800 seconds)
    // and before 2100-01-01 (4102444800 seconds) in nanoseconds.
    let min_ns: u64 = 1_577_836_800_000_000_000;
    let max_ns: u64 = 4_102_444_800_000_000_000;
    header.acq_time >= min_ns && header.acq_time <= max_ns
}

#[cfg(test)]
mod tests {
    use super::*;
    use bubbaloop_schemas::{CompressedImage, CurrentWeather, Header};

    #[test]
    fn test_extract_header_from_compressed_image() {
        let img = CompressedImage {
            header: Some(Header {
                acq_time: 1_700_000_000_000_000_000,
                pub_time: 1_700_000_000_100_000_000,
                sequence: 42,
                frame_id: "cam0".into(),
                machine_id: "jetson1".into(),
                scope: "default".into(),
            }),
            format: "h264".into(),
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        let bytes = img.encode_to_vec();
        let header = extract_header(&bytes).unwrap();
        assert_eq!(header.sequence, 42);
        assert_eq!(header.frame_id, "cam0");
        assert_eq!(header.machine_id, "jetson1");
    }

    #[test]
    fn test_extract_header_from_weather() {
        let weather = CurrentWeather {
            header: Some(Header {
                acq_time: 1_700_000_000_000_000_000,
                sequence: 7,
                frame_id: "weather".into(),
                machine_id: "central".into(),
                ..Default::default()
            }),
            temperature_2m: 22.5,
            relative_humidity_2m: 65.0,
            ..Default::default()
        };
        let bytes = weather.encode_to_vec();
        let header = extract_header(&bytes).unwrap();
        assert_eq!(header.sequence, 7);
        assert_eq!(header.frame_id, "weather");
    }

    #[test]
    fn test_extract_header_from_garbage_returns_none_or_invalid() {
        let garbage = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let meta = MessageMeta::from_raw(&garbage, "/test", "unknown");
        // Should fall back to wall clock time (sequence 0)
        assert_eq!(meta.sequence, 0);
        assert_eq!(meta.topic, "/test");
    }

    #[test]
    fn test_extract_header_from_empty_returns_none() {
        let empty = vec![];
        let result = extract_header(&empty);
        // Empty bytes decode to default (all zeros), which fails validity
        assert!(result.is_none() || !is_valid_header(&result.unwrap()));
    }

    #[test]
    fn test_message_meta_from_raw_with_valid_header() {
        let img = CompressedImage {
            header: Some(Header {
                acq_time: 1_700_000_000_000_000_000,
                pub_time: 1_700_000_000_100_000_000,
                sequence: 99,
                frame_id: "cam1".into(),
                machine_id: "edge1".into(),
                scope: "prod".into(),
            }),
            format: "h264".into(),
            data: vec![1, 2, 3],
        };
        let bytes = img.encode_to_vec();
        let meta = MessageMeta::from_raw(
            &bytes,
            "/camera/entrance/compressed",
            "bubbaloop.camera.v1.CompressedImage",
        );
        assert_eq!(meta.sequence, 99);
        assert_eq!(meta.frame_id, "cam1");
        assert_eq!(meta.machine_id, "edge1");
        assert_eq!(meta.topic, "/camera/entrance/compressed");
        assert_eq!(
            meta.message_type,
            "bubbaloop.camera.v1.CompressedImage"
        );
    }

    #[test]
    fn test_is_valid_header() {
        let valid = Header {
            acq_time: 1_700_000_000_000_000_000,
            ..Default::default()
        };
        assert!(is_valid_header(&valid));

        let too_old = Header {
            acq_time: 0,
            ..Default::default()
        };
        assert!(!is_valid_header(&too_old));

        let too_new = Header {
            acq_time: 5_000_000_000_000_000_000,
            ..Default::default()
        };
        assert!(!is_valid_header(&too_new));
    }
}
