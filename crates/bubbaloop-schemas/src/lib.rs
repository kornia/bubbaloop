//! Protobuf schemas for bubbaloop nodes
//!
//! Single source of truth for all protobuf definitions shared across
//! bubbaloop nodes. Rust nodes depend on this crate directly;
//! Python nodes compile from the proto sources in `protos/`.

pub mod header {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/bubbaloop.header.v1.rs"));
    }
}

pub mod system_telemetry {
    pub mod v1 {
        include!(concat!(
            env!("OUT_DIR"),
            "/bubbaloop.system_telemetry.v1.rs"
        ));
    }
}

pub mod network_monitor {
    pub mod v1 {
        include!(concat!(
            env!("OUT_DIR"),
            "/bubbaloop.network_monitor.v1.rs"
        ));
    }
}

// Re-export commonly used types
pub use header::v1::Header;
pub use network_monitor::v1::{
    CheckStatus, CheckType, HealthCheck, NetworkStatus, Summary,
};
pub use system_telemetry::v1::{
    CpuMetrics, DiskMetrics, LoadMetrics, MemoryMetrics, NetworkMetrics, SystemMetrics,
};

// ros-z type info implementations (enables ZPub/ZSub with ProtobufSerdes)
#[cfg(feature = "ros-z")]
mod rosz_impls {
    use ros_z::{MessageTypeInfo, TypeHash, WithTypeInfo};

    impl MessageTypeInfo for crate::SystemMetrics {
        fn type_name() -> &'static str {
            "bubbaloop.system_telemetry.v1.SystemMetrics"
        }
        fn type_hash() -> TypeHash {
            TypeHash::zero()
        }
    }
    impl WithTypeInfo for crate::SystemMetrics {}

    impl MessageTypeInfo for crate::Header {
        fn type_name() -> &'static str {
            "bubbaloop.header.v1.Header"
        }
        fn type_hash() -> TypeHash {
            TypeHash::zero()
        }
    }
    impl WithTypeInfo for crate::Header {}

    impl MessageTypeInfo for crate::NetworkStatus {
        fn type_name() -> &'static str {
            "bubbaloop.network_monitor.v1.NetworkStatus"
        }
        fn type_hash() -> TypeHash {
            TypeHash::zero()
        }
    }
    impl WithTypeInfo for crate::NetworkStatus {}
}
