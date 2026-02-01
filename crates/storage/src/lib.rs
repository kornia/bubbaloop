//! Bubbaloop Storage Service
//!
//! A generic, message-type agnostic storage service that records Zenoh messages
//! to LanceDB backed by GCP Cloud Storage. Extracts Header metadata from any
//! protobuf message following the bubbaloop convention (Header at field 1).

pub mod config;
pub mod error;
pub mod header;
pub mod lancedb_client;
pub mod recorder;
