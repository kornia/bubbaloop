//! Protobuf schemas shared across bubbaloop nodes.
//!
//! Re-exports types from `bubbaloop-schemas` — the single source of truth
//! for all protobuf definitions.

pub use bubbaloop_schemas::header;
pub use bubbaloop_schemas::Header;
pub use bubbaloop_schemas::MessageTypeName;

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
        };
        let bytes = header.encode_to_vec();
        let decoded = Header::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.frame_id, "cam0");
        assert_eq!(decoded.machine_id, "jetson1");
    }

    #[test]
    fn test_header_default() {
        let header = Header::default();
        assert_eq!(header.sequence, 0);
        assert_eq!(header.frame_id, "");
    }

    #[test]
    fn test_message_type_name() {
        assert_eq!(Header::type_name(), "bubbaloop.header.v1.Header");
    }
}
