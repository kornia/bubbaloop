//! Integration tests for Zenoh pub/sub communication patterns
//!
//! These tests verify the Zenoh publish/subscribe patterns used in Bubbaloop:
//! - Basic pub/sub communication
//! - Wildcard subscriptions
//! - Protobuf message encoding/decoding
//! - Node state publishing
//! - Event publishing
//!
//! ## Running Tests
//!
//! These tests are marked with `#[ignore]` because they require a running zenohd router.
//!
//! To run them:
//!
//! 1. Start a Zenoh router in a separate terminal:
//!    ```bash
//!    zenohd
//!    ```
//!
//! 2. Run the tests:
//!    ```bash
//!    cargo test --test pubsub_integration -- --ignored --test-threads=1
//!    ```
//!
//! Note: Use `--test-threads=1` to avoid port conflicts if tests spawn their own routers.

#![allow(dead_code)]
#![allow(clippy::needless_borrow)]

use bubbaloop_daemon::proto::{
    CommandResult, CommandType, HealthStatus, NodeCommand, NodeEvent, NodeList, NodeState,
    NodeStatus,
};
use prost::Message;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use zenoh::bytes::ZBytes;

/// Test fixture that manages Zenoh sessions for testing
struct ZenohFixture {
    sessions: Vec<Arc<zenoh::Session>>,
}

impl ZenohFixture {
    /// Create a new fixture with the specified number of sessions
    async fn new(num_sessions: usize) -> Self {
        let mut sessions = Vec::new();

        for i in 0..num_sessions {
            let session = Self::create_test_session().await;
            log::info!("Created test session {}/{}", i + 1, num_sessions);
            sessions.push(session);
        }

        // Give Zenoh time to establish connections
        tokio::time::sleep(Duration::from_millis(500)).await;

        Self { sessions }
    }

    /// Create a single Zenoh session configured for testing
    async fn create_test_session() -> Arc<zenoh::Session> {
        let mut config = zenoh::Config::default();

        // Run in peer mode for tests
        config.insert_json5("mode", "\"peer\"").ok();

        // Connect to local zenohd
        config
            .insert_json5("connect/endpoints", "[\"tcp/127.0.0.1:7447\"]")
            .ok();

        // Disable scouting to ensure we only connect to the explicit endpoint
        config
            .insert_json5("scouting/multicast/enabled", "false")
            .ok();
        config.insert_json5("scouting/gossip/enabled", "false").ok();

        let session = zenoh::open(config)
            .await
            .expect("Failed to open Zenoh session - is zenohd running?");

        Arc::new(session)
    }

    /// Get a session by index
    fn session(&self, index: usize) -> Arc<zenoh::Session> {
        self.sessions[index].clone()
    }

    /// Get the first session (convenience method)
    fn first(&self) -> Arc<zenoh::Session> {
        self.session(0)
    }
}

/// Helper to encode protobuf messages to ZBytes
fn encode_proto<M: Message>(msg: &M) -> ZBytes {
    let mut buf = Vec::new();
    msg.encode(&mut buf).expect("Failed to encode protobuf");
    ZBytes::from(buf)
}

/// Helper to decode protobuf messages from ZBytes
fn decode_proto<M: Message + Default>(bytes: &ZBytes) -> M {
    let data = bytes.to_bytes();
    M::decode(&data[..]).expect("Failed to decode protobuf")
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_basic_pubsub() {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixture = ZenohFixture::new(2).await;
    let publisher = fixture.session(0);
    let subscriber_session = fixture.session(1);

    let test_key = "bubbaloop/test/basic";
    let test_message = "Hello, Zenoh!";

    // Create subscriber with channel
    let (tx, mut rx) = mpsc::channel(10);
    let subscriber = subscriber_session
        .declare_subscriber(test_key)
        .callback(move |sample| {
            let payload = sample.payload().to_bytes();
            let message = String::from_utf8(payload.to_vec()).unwrap();
            tx.blocking_send(message).ok();
        })
        .await
        .expect("Failed to create subscriber");

    // Give subscriber time to be ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish message
    publisher
        .put(test_key, test_message)
        .await
        .expect("Failed to publish");

    // Verify message received
    let received = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Timeout waiting for message")
        .expect("Channel closed");

    assert_eq!(received, test_message);

    // Cleanup
    drop(subscriber);
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_multiple_subscribers() {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixture = ZenohFixture::new(4).await;
    let publisher = fixture.session(0);

    let test_key = "bubbaloop/test/multi";
    let test_message = "Broadcast message";

    // Create 3 subscribers
    let mut receivers = Vec::new();
    for i in 0..3 {
        let session = fixture.session(i + 1);
        let (tx, rx) = mpsc::channel(10);
        receivers.push(rx);

        let _subscriber = session
            .declare_subscriber(test_key)
            .callback(move |sample| {
                let payload = sample.payload().to_bytes();
                let message = String::from_utf8(payload.to_vec()).unwrap();
                tx.blocking_send(message).ok();
            })
            .await
            .expect("Failed to create subscriber");
    }

    // Give subscribers time to be ready
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Publish message
    publisher
        .put(test_key, test_message)
        .await
        .expect("Failed to publish");

    // Verify all subscribers received the message
    for (i, rx) in receivers.iter_mut().enumerate() {
        let received = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("Timeout waiting for subscriber {}", i))
            .unwrap_or_else(|| panic!("Channel {} closed", i));

        assert_eq!(
            received, test_message,
            "Subscriber {} didn't receive message",
            i
        );
    }
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_wildcard_subscription() {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixture = ZenohFixture::new(2).await;
    let publisher = fixture.session(0);
    let subscriber_session = fixture.session(1);

    // Subscribe to wildcard pattern
    let wildcard_pattern = "bubbaloop/nodes/*/state";
    let (tx, mut rx) = mpsc::channel(10);

    let _subscriber = subscriber_session
        .declare_subscriber(wildcard_pattern)
        .callback(move |sample| {
            let key = sample.key_expr().to_string();
            tx.blocking_send(key).ok();
        })
        .await
        .expect("Failed to create wildcard subscriber");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish to different matching keys
    let keys = vec![
        "bubbaloop/nodes/camera/state",
        "bubbaloop/nodes/weather/state",
        "bubbaloop/nodes/recorder/state",
    ];

    for key in &keys {
        publisher
            .put(*key, "test")
            .await
            .expect("Failed to publish");
    }

    // Verify we receive all messages
    let mut received_keys = Vec::new();
    for _ in 0..keys.len() {
        let key = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("Timeout waiting for message")
            .expect("Channel closed");
        received_keys.push(key);
    }

    for expected_key in keys {
        assert!(
            received_keys.contains(&expected_key.to_string()),
            "Missing key: {}",
            expected_key
        );
    }
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_node_state_publishing() {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixture = ZenohFixture::new(2).await;
    let publisher = fixture.session(0);
    let subscriber_session = fixture.session(1);

    let test_key = "bubbaloop/daemon/nodes/test-node/state";

    // Create test NodeState
    let original_state = NodeState {
        name: "test-node".to_string(),
        path: "/home/user/test-node".to_string(),
        status: NodeStatus::Running as i32,
        installed: true,
        autostart_enabled: false,
        version: "0.1.0".to_string(),
        description: "Test node for integration tests".to_string(),
        node_type: "rust".to_string(),
        is_built: true,
        last_updated_ms: 1234567890,
        build_output: vec!["Build successful".to_string()],
        health_status: HealthStatus::Healthy as i32,
        last_health_check_ms: 1234567890,
    };

    // Set up subscriber
    let (tx, mut rx) = mpsc::channel(10);
    let _subscriber = subscriber_session
        .declare_subscriber(test_key)
        .callback(move |sample| {
            let bytes = sample.payload();
            let state: NodeState = decode_proto(bytes);
            tx.blocking_send(state).ok();
        })
        .await
        .expect("Failed to create subscriber");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish NodeState
    let encoded = encode_proto(&original_state);
    publisher
        .put(test_key, encoded)
        .await
        .expect("Failed to publish");

    // Verify received state
    let received_state = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Timeout waiting for NodeState")
        .expect("Channel closed");

    assert_eq!(received_state.name, original_state.name);
    assert_eq!(received_state.path, original_state.path);
    assert_eq!(received_state.status, original_state.status);
    assert_eq!(received_state.installed, original_state.installed);
    assert_eq!(received_state.version, original_state.version);
    assert_eq!(received_state.node_type, original_state.node_type);
    assert_eq!(received_state.is_built, original_state.is_built);
    assert_eq!(received_state.health_status, original_state.health_status);
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_node_list_publishing() {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixture = ZenohFixture::new(2).await;
    let publisher = fixture.session(0);
    let subscriber_session = fixture.session(1);

    let test_key = "bubbaloop/daemon/nodes";

    // Create test NodeList with multiple nodes
    let nodes = vec![
        NodeState {
            name: "camera".to_string(),
            path: "/path/to/camera".to_string(),
            status: NodeStatus::Running as i32,
            installed: true,
            autostart_enabled: true,
            version: "1.0.0".to_string(),
            description: "Camera node".to_string(),
            node_type: "rust".to_string(),
            is_built: true,
            last_updated_ms: 1000,
            build_output: vec![],
            health_status: HealthStatus::Healthy as i32,
            last_health_check_ms: 1000,
        },
        NodeState {
            name: "weather".to_string(),
            path: "/path/to/weather".to_string(),
            status: NodeStatus::Stopped as i32,
            installed: true,
            autostart_enabled: false,
            version: "0.5.0".to_string(),
            description: "Weather node".to_string(),
            node_type: "python".to_string(),
            is_built: false,
            last_updated_ms: 2000,
            build_output: vec![],
            health_status: HealthStatus::Unknown as i32,
            last_health_check_ms: 0,
        },
    ];

    let original_list = NodeList {
        nodes: nodes.clone(),
        timestamp_ms: 1234567890,
    };

    // Set up subscriber
    let (tx, mut rx) = mpsc::channel(10);
    let _subscriber = subscriber_session
        .declare_subscriber(test_key)
        .callback(move |sample| {
            let bytes = sample.payload();
            let list: NodeList = decode_proto(bytes);
            tx.blocking_send(list).ok();
        })
        .await
        .expect("Failed to create subscriber");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish NodeList
    let encoded = encode_proto(&original_list);
    publisher
        .put(test_key, encoded)
        .await
        .expect("Failed to publish");

    // Verify received list
    let received_list = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Timeout waiting for NodeList")
        .expect("Channel closed");

    assert_eq!(received_list.nodes.len(), 2);
    assert_eq!(received_list.timestamp_ms, original_list.timestamp_ms);

    // Verify first node
    assert_eq!(received_list.nodes[0].name, "camera");
    assert_eq!(received_list.nodes[0].status, NodeStatus::Running as i32);

    // Verify second node
    assert_eq!(received_list.nodes[1].name, "weather");
    assert_eq!(received_list.nodes[1].status, NodeStatus::Stopped as i32);
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_event_publishing() {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixture = ZenohFixture::new(2).await;
    let publisher = fixture.session(0);
    let subscriber_session = fixture.session(1);

    let test_key = "bubbaloop/daemon/events";

    // Create test NodeEvent
    let node_state = NodeState {
        name: "test-node".to_string(),
        path: "/path/to/test-node".to_string(),
        status: NodeStatus::Running as i32,
        installed: true,
        autostart_enabled: true,
        version: "1.0.0".to_string(),
        description: "Test node".to_string(),
        node_type: "rust".to_string(),
        is_built: true,
        last_updated_ms: 1000,
        build_output: vec![],
        health_status: HealthStatus::Healthy as i32,
        last_health_check_ms: 1000,
    };

    let original_event = NodeEvent {
        event_type: "started".to_string(),
        node_name: "test-node".to_string(),
        state: Some(node_state),
        timestamp_ms: 1234567890,
    };

    // Set up subscriber
    let (tx, mut rx) = mpsc::channel(10);
    let _subscriber = subscriber_session
        .declare_subscriber(test_key)
        .callback(move |sample| {
            let bytes = sample.payload();
            let event: NodeEvent = decode_proto(bytes);
            tx.blocking_send(event).ok();
        })
        .await
        .expect("Failed to create subscriber");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish NodeEvent
    let encoded = encode_proto(&original_event);
    publisher
        .put(test_key, encoded)
        .await
        .expect("Failed to publish");

    // Verify received event
    let received_event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Timeout waiting for NodeEvent")
        .expect("Channel closed");

    assert_eq!(received_event.event_type, original_event.event_type);
    assert_eq!(received_event.node_name, original_event.node_name);
    assert_eq!(received_event.timestamp_ms, original_event.timestamp_ms);
    assert!(received_event.state.is_some());

    let state = received_event.state.unwrap();
    assert_eq!(state.name, "test-node");
    assert_eq!(state.status, NodeStatus::Running as i32);
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_command_queryable() {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixture = ZenohFixture::new(2).await;
    let queryable_session = fixture.session(0);
    let client_session = fixture.session(1);

    let command_key = "bubbaloop/test/command";

    // Create a queryable that echoes commands back as results
    let (tx, mut rx) = mpsc::channel(10);
    let _queryable = queryable_session
        .declare_queryable(command_key)
        .await
        .expect("Failed to create queryable");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Spawn task to handle queries
    let queryable_session_clone = queryable_session.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        // Use subscriber pattern instead of callback for async handling
        let subscriber = queryable_session_clone
            .declare_queryable(command_key)
            .await
            .expect("Failed to create queryable");

        while let Ok(query) = subscriber.recv_async().await {
            // Decode command
            let payload = query.payload().expect("No payload");
            let cmd: NodeCommand = decode_proto(payload);

            // Create result
            let result = CommandResult {
                request_id: cmd.request_id.clone(),
                success: true,
                message: format!("Executed command {:?}", cmd.command),
                output: format!("Node: {}", cmd.node_name),
                node_state: None,
            };

            // Send reply
            let encoded = encode_proto(&result);
            query.reply(query.key_expr(), encoded).await.ok();

            // Notify test
            tx_clone.send(cmd.node_name).await.ok();
        }
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send a command query
    let command = NodeCommand {
        command: CommandType::Start as i32,
        node_name: "test-node".to_string(),
        node_path: "/path/to/node".to_string(),
        request_id: "req-123".to_string(),
    };

    let encoded_command = encode_proto(&command);
    let replies = client_session
        .get(command_key)
        .payload(encoded_command)
        .await
        .expect("Failed to send query");

    // Wait for queryable to process
    let processed_node = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Timeout waiting for queryable processing")
        .expect("Channel closed");

    assert_eq!(processed_node, "test-node");

    // Verify reply
    let reply = timeout(Duration::from_secs(2), replies.recv_async())
        .await
        .expect("Timeout waiting for reply")
        .expect("No reply received");
    let sample = reply.result().expect("Reply should be OK");
    let result: CommandResult = decode_proto(&sample.payload());

    assert_eq!(result.request_id, "req-123");
    assert!(result.success);
    assert!(result.message.contains("Executed command"));
    assert!(result.output.contains("test-node"));
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_health_status_values() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Test all health status enum values
    let statuses = vec![
        HealthStatus::Unknown,
        HealthStatus::Healthy,
        HealthStatus::Unhealthy,
    ];

    for status in statuses {
        let state = NodeState {
            name: "test".to_string(),
            path: "/test".to_string(),
            status: NodeStatus::Running as i32,
            installed: true,
            autostart_enabled: false,
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            node_type: "rust".to_string(),
            is_built: true,
            last_updated_ms: 1000,
            build_output: vec![],
            health_status: status as i32,
            last_health_check_ms: 1000,
        };

        // Verify encoding/decoding preserves health status
        let encoded = encode_proto(&state);
        let decoded: NodeState = decode_proto(&encoded);
        assert_eq!(decoded.health_status, status as i32);
    }
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_command_types() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Test all command type enum values
    let commands = vec![
        CommandType::Start,
        CommandType::Stop,
        CommandType::Restart,
        CommandType::Install,
        CommandType::Uninstall,
        CommandType::Build,
        CommandType::Clean,
        CommandType::EnableAutostart,
        CommandType::DisableAutostart,
        CommandType::AddNode,
        CommandType::RemoveNode,
        CommandType::Refresh,
        CommandType::GetLogs,
    ];

    for cmd_type in commands {
        let cmd = NodeCommand {
            command: cmd_type as i32,
            node_name: "test".to_string(),
            node_path: "/test".to_string(),
            request_id: "req-1".to_string(),
        };

        // Verify encoding/decoding preserves command type
        let encoded = encode_proto(&cmd);
        let decoded: NodeCommand = decode_proto(&encoded);
        assert_eq!(decoded.command, cmd_type as i32);
    }
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_concurrent_publishers() {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixture = ZenohFixture::new(4).await;
    let subscriber_session = fixture.session(0);

    let test_key = "bubbaloop/test/concurrent";

    // Set up subscriber to collect all messages
    let (tx, mut rx) = mpsc::channel(100);
    let _subscriber = subscriber_session
        .declare_subscriber(test_key)
        .callback(move |sample| {
            let payload = sample.payload().to_bytes();
            let message = String::from_utf8(payload.to_vec()).unwrap();
            tx.blocking_send(message).ok();
        })
        .await
        .expect("Failed to create subscriber");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Spawn multiple concurrent publishers
    let mut handles = Vec::new();
    for i in 0..3 {
        let session = fixture.session(i + 1);
        let handle = tokio::spawn(async move {
            for j in 0..5 {
                let message = format!("Publisher-{}-Message-{}", i, j);
                session
                    .put(test_key, message)
                    .await
                    .expect("Failed to publish");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all publishers to complete
    for handle in handles {
        handle.await.expect("Publisher task failed");
    }

    // Collect messages (should receive 15 total: 3 publishers × 5 messages)
    let mut messages = Vec::new();
    for _ in 0..15 {
        let msg = timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("Timeout waiting for message")
            .expect("Channel closed");
        messages.push(msg);
    }

    assert_eq!(messages.len(), 15);

    // Verify each publisher sent all its messages
    for i in 0..3 {
        for j in 0..5 {
            let expected = format!("Publisher-{}-Message-{}", i, j);
            assert!(
                messages.contains(&expected),
                "Missing message: {}",
                expected
            );
        }
    }
}
