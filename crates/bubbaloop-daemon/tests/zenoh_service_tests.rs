//! Unit tests for Zenoh service layer
//!
//! These tests focus on the pure functions and key generation logic
//! without requiring a running zenohd instance.

use bubbaloop_daemon::proto::{
    CommandResult, CommandType, HealthStatus, NodeCommand, NodeEvent, NodeList, NodeState,
    NodeStatus,
};
use bubbaloop_daemon::zenoh_service::keys;
use prost::Message;

#[test]
fn test_node_state_key_generates_correct_format() {
    let key = keys::node_state_key("test-machine", "my-node");
    assert_eq!(key, "bubbaloop/test-machine/daemon/nodes/my-node/state");

    let key2 = keys::node_state_key("test-machine", "rtsp-camera");
    assert_eq!(
        key2,
        "bubbaloop/test-machine/daemon/nodes/rtsp-camera/state"
    );
}

#[test]
fn test_node_state_key_handles_special_characters() {
    // Test with underscores
    let key = keys::node_state_key("test-machine", "my_test_node");
    assert_eq!(
        key,
        "bubbaloop/test-machine/daemon/nodes/my_test_node/state"
    );

    // Test with numbers
    let key2 = keys::node_state_key("test-machine", "node123");
    assert_eq!(key2, "bubbaloop/test-machine/daemon/nodes/node123/state");
}

#[test]
fn test_node_state_key_handles_empty_string() {
    let key = keys::node_state_key("test-machine", "");
    assert_eq!(key, "bubbaloop/test-machine/daemon/nodes//state");
}

#[test]
fn test_key_constants_are_valid() {
    // These should be valid Zenoh key expressions (no wildcards needed for tests)
    assert!(!keys::NODES_LIST_LEGACY.is_empty());
    assert!(!keys::NODE_STATE_PREFIX_LEGACY.is_empty());
    assert!(!keys::COMMAND_LEGACY.is_empty());
    assert!(!keys::EVENTS_LEGACY.is_empty());

    // Verify structure
    assert_eq!(keys::NODES_LIST_LEGACY, "bubbaloop/daemon/nodes");
    assert_eq!(keys::NODE_STATE_PREFIX_LEGACY, "bubbaloop/daemon/nodes/");
    assert_eq!(keys::COMMAND_LEGACY, "bubbaloop/daemon/command");
    assert_eq!(keys::EVENTS_LEGACY, "bubbaloop/daemon/events");
}

#[test]
fn test_key_prefix_matches_list() {
    // NODE_STATE_PREFIX should be a prefix of NODES_LIST
    assert!(keys::NODE_STATE_PREFIX_LEGACY.starts_with(keys::NODES_LIST_LEGACY));
}

#[test]
fn test_proto_encoding_node_state() {
    let state = NodeState {
        name: "test-node".to_string(),
        path: "/path/to/node".to_string(),
        status: NodeStatus::Running as i32,
        installed: true,
        autostart_enabled: false,
        version: "0.1.0".to_string(),
        description: "Test node".to_string(),
        node_type: "rust".to_string(),
        is_built: true,
        last_updated_ms: 1234567890,
        build_output: vec!["line1".to_string(), "line2".to_string()],
        health_status: HealthStatus::Healthy as i32,
        last_health_check_ms: 1234567890,
        machine_id: String::new(),
        machine_hostname: String::new(),
    };

    // Encode
    let mut buf = Vec::new();
    state.encode(&mut buf).expect("Failed to encode");

    // Decode
    let decoded = NodeState::decode(&buf[..]).expect("Failed to decode");

    // Verify all fields
    assert_eq!(decoded.name, "test-node");
    assert_eq!(decoded.path, "/path/to/node");
    assert_eq!(decoded.status, NodeStatus::Running as i32);
    assert!(decoded.installed);
    assert!(!decoded.autostart_enabled);
    assert_eq!(decoded.version, "0.1.0");
    assert_eq!(decoded.description, "Test node");
    assert_eq!(decoded.node_type, "rust");
    assert!(decoded.is_built);
    assert_eq!(decoded.last_updated_ms, 1234567890);
    assert_eq!(decoded.build_output, vec!["line1", "line2"]);
    assert_eq!(decoded.health_status, HealthStatus::Healthy as i32);
    assert_eq!(decoded.last_health_check_ms, 1234567890);
}

#[test]
fn test_proto_encoding_node_list() {
    let list = NodeList {
        nodes: vec![
            NodeState {
                name: "node1".to_string(),
                status: NodeStatus::Running as i32,
                ..Default::default()
            },
            NodeState {
                name: "node2".to_string(),
                status: NodeStatus::Stopped as i32,
                ..Default::default()
            },
        ],
        timestamp_ms: 9876543210,
        machine_id: String::new(),
    };

    // Encode
    let mut buf = Vec::new();
    list.encode(&mut buf).expect("Failed to encode");

    // Decode
    let decoded = NodeList::decode(&buf[..]).expect("Failed to decode");

    // Verify
    assert_eq!(decoded.nodes.len(), 2);
    assert_eq!(decoded.nodes[0].name, "node1");
    assert_eq!(decoded.nodes[1].name, "node2");
    assert_eq!(decoded.timestamp_ms, 9876543210);
}

#[test]
fn test_proto_encoding_node_command() {
    let cmd = NodeCommand {
        command: CommandType::Start as i32,
        node_name: "test-node".to_string(),
        node_path: "".to_string(),
        request_id: "req-123".to_string(),
        timestamp_ms: 0,
        source_machine: String::new(),
        target_machine: String::new(),
    };

    // Encode
    let mut buf = Vec::new();
    cmd.encode(&mut buf).expect("Failed to encode");

    // Decode
    let decoded = NodeCommand::decode(&buf[..]).expect("Failed to decode");

    // Verify
    assert_eq!(decoded.command, CommandType::Start as i32);
    assert_eq!(decoded.node_name, "test-node");
    assert_eq!(decoded.request_id, "req-123");
}

#[test]
fn test_proto_encoding_command_result() {
    let result = CommandResult {
        request_id: "req-123".to_string(),
        success: true,
        message: "Command executed successfully".to_string(),
        output: "Some output".to_string(),
        node_state: Some(NodeState {
            name: "test-node".to_string(),
            ..Default::default()
        }),
        timestamp_ms: 0,
        responding_machine: String::new(),
    };

    // Encode
    let mut buf = Vec::new();
    result.encode(&mut buf).expect("Failed to encode");

    // Decode
    let decoded = CommandResult::decode(&buf[..]).expect("Failed to decode");

    // Verify
    assert_eq!(decoded.request_id, "req-123");
    assert!(decoded.success);
    assert_eq!(decoded.message, "Command executed successfully");
    assert_eq!(decoded.output, "Some output");
    assert!(decoded.node_state.is_some());
    assert_eq!(decoded.node_state.unwrap().name, "test-node");
}

#[test]
fn test_proto_encoding_node_event() {
    let event = NodeEvent {
        event_type: "state_changed".to_string(),
        node_name: "test-node".to_string(),
        state: Some(NodeState {
            name: "test-node".to_string(),
            status: NodeStatus::Running as i32,
            ..Default::default()
        }),
        timestamp_ms: 1234567890,
    };

    // Encode
    let mut buf = Vec::new();
    event.encode(&mut buf).expect("Failed to encode");

    // Decode
    let decoded = NodeEvent::decode(&buf[..]).expect("Failed to decode");

    // Verify
    assert_eq!(decoded.event_type, "state_changed");
    assert_eq!(decoded.node_name, "test-node");
    assert_eq!(decoded.timestamp_ms, 1234567890);
    assert!(decoded.state.is_some());
    assert_eq!(decoded.state.unwrap().status, NodeStatus::Running as i32);
}

#[test]
fn test_proto_encoding_empty_messages() {
    // Test that empty messages can be encoded and decoded
    let state = NodeState::default();
    let mut buf = Vec::new();
    state.encode(&mut buf).expect("Failed to encode");
    let decoded = NodeState::decode(&buf[..]).expect("Failed to decode");
    assert_eq!(decoded.name, "");

    let list = NodeList::default();
    let mut buf = Vec::new();
    list.encode(&mut buf).expect("Failed to encode");
    let decoded = NodeList::decode(&buf[..]).expect("Failed to decode");
    assert_eq!(decoded.nodes.len(), 0);
}

#[test]
fn test_proto_encoding_large_build_output() {
    // Test with many build output lines
    let build_output: Vec<String> = (0..1000).map(|i| format!("Line {}", i)).collect();

    let state = NodeState {
        name: "test-node".to_string(),
        build_output: build_output.clone(),
        ..Default::default()
    };

    // Encode
    let mut buf = Vec::new();
    state.encode(&mut buf).expect("Failed to encode");

    // Decode
    let decoded = NodeState::decode(&buf[..]).expect("Failed to decode");

    // Verify
    assert_eq!(decoded.build_output.len(), 1000);
    assert_eq!(decoded.build_output[0], "Line 0");
    assert_eq!(decoded.build_output[999], "Line 999");
}

#[test]
fn test_node_status_enum_values() {
    // Verify enum values match proto definition
    assert_eq!(NodeStatus::Unknown as i32, 0);
    assert_eq!(NodeStatus::Stopped as i32, 1);
    assert_eq!(NodeStatus::Running as i32, 2);
    assert_eq!(NodeStatus::Failed as i32, 3);
    assert_eq!(NodeStatus::Installing as i32, 4);
    assert_eq!(NodeStatus::Building as i32, 5);
    assert_eq!(NodeStatus::NotInstalled as i32, 6);
}

#[test]
fn test_health_status_enum_values() {
    // Verify enum values match proto definition
    assert_eq!(HealthStatus::Unknown as i32, 0);
    assert_eq!(HealthStatus::Healthy as i32, 1);
    assert_eq!(HealthStatus::Unhealthy as i32, 2);
}

#[test]
fn test_command_type_enum_values() {
    // Verify enum values match proto definition
    assert_eq!(CommandType::Start as i32, 0);
    assert_eq!(CommandType::Stop as i32, 1);
    assert_eq!(CommandType::Restart as i32, 2);
    assert_eq!(CommandType::Install as i32, 3);
    assert_eq!(CommandType::Uninstall as i32, 4);
    assert_eq!(CommandType::Build as i32, 5);
    assert_eq!(CommandType::Clean as i32, 6);
    assert_eq!(CommandType::EnableAutostart as i32, 7);
    assert_eq!(CommandType::DisableAutostart as i32, 8);
    assert_eq!(CommandType::AddNode as i32, 9);
    assert_eq!(CommandType::RemoveNode as i32, 10);
    assert_eq!(CommandType::Refresh as i32, 11);
    assert_eq!(CommandType::GetLogs as i32, 12);
}

#[test]
fn test_proto_roundtrip_preserves_all_fields() {
    // Create a fully populated state
    let original = NodeState {
        name: "full-test-node".to_string(),
        path: "/full/path/to/node".to_string(),
        status: NodeStatus::Running as i32,
        installed: true,
        autostart_enabled: true,
        version: "1.2.3".to_string(),
        description: "A fully populated test node".to_string(),
        node_type: "python".to_string(),
        is_built: true,
        last_updated_ms: 9999999999,
        build_output: vec![
            "Building target".to_string(),
            "Linking".to_string(),
            "Success".to_string(),
        ],
        health_status: HealthStatus::Healthy as i32,
        last_health_check_ms: 8888888888,
        machine_id: "test-machine".to_string(),
        machine_hostname: "test-host".to_string(),
    };

    // Encode and decode
    let mut buf = Vec::new();
    original.encode(&mut buf).expect("Failed to encode");
    let decoded = NodeState::decode(&buf[..]).expect("Failed to decode");

    // Verify all fields match exactly
    assert_eq!(decoded.name, original.name);
    assert_eq!(decoded.path, original.path);
    assert_eq!(decoded.status, original.status);
    assert_eq!(decoded.installed, original.installed);
    assert_eq!(decoded.autostart_enabled, original.autostart_enabled);
    assert_eq!(decoded.version, original.version);
    assert_eq!(decoded.description, original.description);
    assert_eq!(decoded.node_type, original.node_type);
    assert_eq!(decoded.is_built, original.is_built);
    assert_eq!(decoded.last_updated_ms, original.last_updated_ms);
    assert_eq!(decoded.build_output, original.build_output);
    assert_eq!(decoded.health_status, original.health_status);
    assert_eq!(decoded.last_health_check_ms, original.last_health_check_ms);
}

#[test]
fn test_node_list_with_multiple_states() {
    let list = NodeList {
        nodes: vec![
            NodeState {
                name: "node1".to_string(),
                status: NodeStatus::Running as i32,
                health_status: HealthStatus::Healthy as i32,
                ..Default::default()
            },
            NodeState {
                name: "node2".to_string(),
                status: NodeStatus::Stopped as i32,
                health_status: HealthStatus::Unknown as i32,
                ..Default::default()
            },
            NodeState {
                name: "node3".to_string(),
                status: NodeStatus::Failed as i32,
                health_status: HealthStatus::Unhealthy as i32,
                ..Default::default()
            },
        ],
        timestamp_ms: 1234567890123,
        machine_id: String::new(),
    };

    let mut buf = Vec::new();
    list.encode(&mut buf).expect("Failed to encode");
    let decoded = NodeList::decode(&buf[..]).expect("Failed to decode");

    assert_eq!(decoded.nodes.len(), 3);
    assert_eq!(decoded.nodes[0].status, NodeStatus::Running as i32);
    assert_eq!(decoded.nodes[1].status, NodeStatus::Stopped as i32);
    assert_eq!(decoded.nodes[2].status, NodeStatus::Failed as i32);
    assert_eq!(decoded.nodes[0].health_status, HealthStatus::Healthy as i32);
    assert_eq!(
        decoded.nodes[2].health_status,
        HealthStatus::Unhealthy as i32
    );
}

#[test]
fn test_command_result_with_no_state() {
    let result = CommandResult {
        request_id: "req-456".to_string(),
        success: false,
        message: "Command failed".to_string(),
        output: "Error details".to_string(),
        node_state: None,
        timestamp_ms: 0,
        responding_machine: String::new(),
    };

    let mut buf = Vec::new();
    result.encode(&mut buf).expect("Failed to encode");
    let decoded = CommandResult::decode(&buf[..]).expect("Failed to decode");

    assert!(!decoded.success);
    assert!(decoded.node_state.is_none());
}

// Tests that would require a running Zenoh session are marked with #[ignore]
// and documented for manual testing

#[test]
#[ignore = "Requires running zenohd"]
fn test_create_session_with_default_endpoint() {
    // This test would require:
    // 1. A running zenohd instance on tcp/127.0.0.1:7447
    // 2. Async runtime
    //
    // Manual test:
    // 1. Start zenohd
    // 2. Run: cargo test --test zenoh_service_tests test_create_session_with_default_endpoint -- --ignored
}

#[test]
#[ignore = "Requires running zenohd"]
fn test_create_session_with_custom_endpoint() {
    // This test would require:
    // 1. A running zenohd instance on a custom port
    // 2. Async runtime
    //
    // Manual test:
    // 1. Start zenohd on custom port
    // 2. Run: cargo test --test zenoh_service_tests test_create_session_with_custom_endpoint -- --ignored
}

#[test]
#[ignore = "Requires environment variable setup"]
fn test_create_session_reads_env_var() {
    // This test would require:
    // 1. Setting BUBBALOOP_ZENOH_ENDPOINT environment variable
    // 2. A running zenohd instance at that endpoint
    //
    // Manual test:
    // 1. export BUBBALOOP_ZENOH_ENDPOINT="tcp/127.0.0.1:8447"
    // 2. Start zenohd on port 8447
    // 3. Run: BUBBALOOP_ZENOH_ENDPOINT="tcp/127.0.0.1:8447" cargo test --test zenoh_service_tests test_create_session_reads_env_var -- --ignored
}
