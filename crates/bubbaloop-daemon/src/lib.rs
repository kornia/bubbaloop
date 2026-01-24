//! Bubbaloop Daemon
//!
//! Central service for node management via Zenoh and HTTP.

pub mod http_server;
pub mod node_manager;
pub mod registry;
pub mod systemd;
pub mod zenoh_service;

/// Protobuf schemas for the daemon
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/bubbaloop.daemon.v1.rs"));
}

pub use node_manager::NodeManager;
pub use zenoh_service::ZenohService;
