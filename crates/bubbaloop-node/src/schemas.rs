//! Protobuf schemas shared across bubbaloop nodes.
//!
//! Contains the `Header` message type and the `MessageTypeName` trait
//! for encoding type metadata in Zenoh samples.

pub mod header {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/bubbaloop.header.v1.rs"));
    }
}

// Re-export commonly used types
pub use header::v1::Header;

/// Trait for protobuf types to provide their fully-qualified type name.
/// Used for Zenoh encoding schema suffix and descriptor lookup.
///
/// Implementations return the proto package + message name, e.g.
/// `"bubbaloop.header.v1.Header"` or `"bubbaloop.camera.v1.CompressedImage"`.
pub trait MessageTypeName {
    fn type_name() -> &'static str;
}

impl MessageTypeName for Header {
    fn type_name() -> &'static str {
        "bubbaloop.header.v1.Header"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn test_header_roundtrip() {
        let header = Header {
            acq_time: 1000,
            pub_time: 2000,
            sequence: 42,
            frame_id: "cam0".into(),
            machine_id: "jetson1".into(),
            scope: "default".into(),
        };
        let bytes = header.encode_to_vec();
        let decoded = Header::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.frame_id, "cam0");
        assert_eq!(decoded.scope, "default");
    }

    #[test]
    fn test_header_default() {
        let header = Header::default();
        assert_eq!(header.scope, "");
        assert_eq!(header.sequence, 0);
    }

    #[test]
    fn test_header_serde_json_roundtrip() {
        let header = Header {
            sequence: 99,
            frame_id: "test".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&header).unwrap();
        let decoded: Header = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.sequence, 99);
        assert_eq!(decoded.frame_id, "test");
    }

    #[test]
    fn test_message_type_name() {
        assert_eq!(Header::type_name(), "bubbaloop.header.v1.Header");
    }
}
