//! Bubbaloop Launch System
//!
//! A ROS2-inspired launch system for managing bubbaloop services.
//!
//! # Overview
//!
//! The launch system allows you to:
//! - Define services and their dependencies in YAML files
//! - Launch services in the correct order based on dependencies
//! - Gracefully shutdown services in reverse order
//! - Override arguments at launch time
//!
//! # Example Launch File
//!
//! ```yaml
//! version: "1.0"
//!
//! args:
//!   camera_config:
//!     default: "configs/default.yaml"
//!
//! nodes:
//!   bridge:
//!     executable: "target/release/zenoh-bridge"
//!     group: core
//!
//!   cameras:
//!     package: rtsp_camera
//!     binary: cameras_node
//!     args:
//!       config: "$(arg camera_config)"
//!     depends_on:
//!       - bridge
//! ```

pub mod cli;
pub mod config;
pub mod runtime;

pub use cli::LaunchArgs;
pub use config::{LaunchFile, LaunchFileError, SubstitutionContext, SubstitutionError};
pub use runtime::{
    DependencyError, DependencyGraph, Executor, ExecutorConfig, ExecutorError, LaunchPlan,
    ManagedProcess, ProcessConfig, ProcessError, ProcessEvent, ProcessStatus,
};
