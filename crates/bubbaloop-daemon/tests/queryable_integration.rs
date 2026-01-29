//! Integration tests for Zenoh queryable (request/reply) patterns
//!
//! These tests verify the Zenoh queryable API used in Bubbaloop daemon.
//!
//! Tests marked with `#[ignore]` require `zenohd` to be running:
//! ```bash
//! zenohd --no-multicast-scouting --listen tcp/127.0.0.1:17447 &
//! cargo test --test queryable_integration -- --ignored
//! ```
//!
//! Note: All tests use multi-threaded tokio runtime as required by Zenoh.

#![allow(clippy::while_let_loop)]

mod common;

use bubbaloop_daemon::proto::{CommandResult, CommandType, NodeCommand};
use common::{
    create_test_command, create_test_result, decode_proto, encode_proto, setup_test_session,
    TestConfig, ZenohdHandle,
};
use prost::Message;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use zenoh::bytes::ZBytes;

// ============================================================================
// Basic Queryable Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_basic_queryable_reply() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session 1");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session 2");

    // Session 1: Declare queryable
    let queryable = session1
        .declare_queryable("test/ping")
        .await
        .expect("Failed to declare queryable");

    // Spawn task to handle queries
    let handle = tokio::spawn(async move {
        if let Ok(query) = queryable.recv_async().await {
            let reply = ZBytes::from("pong");
            query
                .reply(query.key_expr(), reply)
                .await
                .expect("Failed to send reply");
        }
    });

    // Give queryable time to be ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Session 2: Send query
    let replies = session2
        .get("test/ping")
        .await
        .expect("Failed to send query");

    // Collect replies with timeout
    let mut reply_count = 0;
    while let Ok(reply) = tokio::time::timeout(Duration::from_secs(1), replies.recv_async()).await {
        match reply {
            Ok(reply) => {
                let sample = reply.into_result().expect("Reply should be OK");
                let payload = sample.payload().to_bytes();
                assert_eq!(payload.as_ref(), b"pong");
                reply_count += 1;
            }
            Err(_) => break,
        }
    }

    assert_eq!(reply_count, 1, "Should receive exactly one reply");
    handle.await.expect("Handler task failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_query_timeout_no_queryable() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    // Query a key that has no queryable
    let replies = session
        .get("test/nonexistent")
        .timeout(Duration::from_millis(500))
        .await
        .expect("Failed to send query");

    // Should get no replies
    let result = tokio::time::timeout(Duration::from_millis(1000), replies.recv_async()).await;

    match result {
        Err(_) => {
            // Timeout is expected when no queryable exists
        }
        Ok(Ok(reply)) => {
            // Got a reply - check if it's an error or success
            match reply.into_result() {
                Ok(_) => panic!("Should not receive successful reply when no queryable exists"),
                Err(_) => {
                    // Error reply is acceptable
                }
            }
        }
        Ok(Err(_)) => {
            // Channel closed is also acceptable
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_multiple_queryables_different_keys() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    // Declare two queryables on different keys
    let queryable1 = session1
        .declare_queryable("test/service1")
        .await
        .expect("Failed to declare queryable 1");

    let queryable2 = session1
        .declare_queryable("test/service2")
        .await
        .expect("Failed to declare queryable 2");

    // Handlers
    let handle1 = tokio::spawn(async move {
        if let Ok(query) = queryable1.recv_async().await {
            query
                .reply(query.key_expr(), ZBytes::from("service1"))
                .await
                .expect("Failed to send reply");
        }
    });

    let handle2 = tokio::spawn(async move {
        if let Ok(query) = queryable2.recv_async().await {
            query
                .reply(query.key_expr(), ZBytes::from("service2"))
                .await
                .expect("Failed to send reply");
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Query both services
    let replies1 = session2.get("test/service1").await.unwrap();
    let replies2 = session2.get("test/service2").await.unwrap();

    // Check service1 reply
    let reply1 = tokio::time::timeout(Duration::from_secs(1), replies1.recv_async())
        .await
        .expect("Timeout")
        .expect("No reply")
        .into_result()
        .expect("Reply error");
    assert_eq!(reply1.payload().to_bytes().as_ref(), b"service1");

    // Check service2 reply
    let reply2 = tokio::time::timeout(Duration::from_secs(1), replies2.recv_async())
        .await
        .expect("Timeout")
        .expect("No reply")
        .into_result()
        .expect("Reply error");
    assert_eq!(reply2.payload().to_bytes().as_ref(), b"service2");

    handle1.await.expect("Handler 1 failed");
    handle2.await.expect("Handler 2 failed");
}

// ============================================================================
// Command Execution Flow Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_command_encoding_and_sending() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    // Create a command
    let command = create_test_command(CommandType::Start as i32, "test-node", "/path/to/node");
    let request_id = command.request_id.clone();

    // Declare queryable to handle commands
    let queryable = session1
        .declare_queryable("bubbaloop/daemon/command")
        .await
        .expect("Failed to declare queryable");

    let received_command = Arc::new(Mutex::new(None));
    let received_command_clone = received_command.clone();

    let handle = tokio::spawn(async move {
        if let Ok(query) = queryable.recv_async().await {
            // Decode the command from the query payload
            if let Some(payload) = query.payload() {
                let bytes = payload.to_bytes();
                if let Ok(cmd) = NodeCommand::decode(bytes.as_ref()) {
                    *received_command_clone.lock().await = Some(cmd.clone());

                    // Send back a success result
                    let result = create_test_result(&cmd.request_id, true, "Command executed");
                    let reply_bytes = encode_proto(&result);
                    query
                        .reply(query.key_expr(), ZBytes::from(reply_bytes))
                        .await
                        .expect("Failed to send reply");
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send command as query with encoded payload
    let command_bytes = encode_proto(&command);
    let replies = session2
        .get("bubbaloop/daemon/command")
        .payload(command_bytes)
        .await
        .expect("Failed to send query");

    // Receive the result
    let reply = tokio::time::timeout(Duration::from_secs(1), replies.recv_async())
        .await
        .expect("Timeout")
        .expect("No reply")
        .into_result()
        .expect("Reply error");

    let result_bytes = reply.payload().to_bytes();
    let result: CommandResult = decode_proto(result_bytes.as_ref()).expect("Failed to decode");

    assert_eq!(result.request_id, request_id);
    assert!(result.success);
    assert_eq!(result.message, "Command executed");

    handle.await.expect("Handler failed");

    // Verify the command was received correctly
    let received = received_command.lock().await;
    assert!(received.is_some());
    let received_cmd = received.as_ref().unwrap();
    assert_eq!(received_cmd.node_name, "test-node");
    assert_eq!(received_cmd.node_path, "/path/to/node");
    assert_eq!(received_cmd.command, CommandType::Start as i32);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_various_command_types() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    let queryable = session1
        .declare_queryable("bubbaloop/daemon/command")
        .await
        .expect("Failed to declare queryable");

    let received_commands = Arc::new(Mutex::new(Vec::new()));
    let received_commands_clone = received_commands.clone();

    let handle = tokio::spawn(async move {
        while let Ok(query) = queryable.recv_async().await {
            if let Some(payload) = query.payload() {
                let bytes = payload.to_bytes();
                if let Ok(cmd) = NodeCommand::decode(bytes.as_ref()) {
                    received_commands_clone.lock().await.push(cmd.clone());

                    let result = create_test_result(&cmd.request_id, true, "OK");
                    let reply_bytes = encode_proto(&result);
                    query
                        .reply(query.key_expr(), ZBytes::from(reply_bytes))
                        .await
                        .ok();
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test various command types
    let command_types = vec![
        CommandType::Start,
        CommandType::Stop,
        CommandType::Restart,
        CommandType::Build,
        CommandType::Install,
        CommandType::Uninstall,
        CommandType::EnableAutostart,
        CommandType::DisableAutostart,
    ];

    for cmd_type in command_types {
        let command = create_test_command(cmd_type as i32, "test-node", "/path");
        let command_bytes = encode_proto(&command);

        let replies = session2
            .get("bubbaloop/daemon/command")
            .payload(command_bytes)
            .await
            .expect("Failed to send query");

        // Just verify we get a reply
        let _ = tokio::time::timeout(Duration::from_secs(1), replies.recv_async())
            .await
            .expect("Timeout");
    }

    // Give time for all commands to be received
    tokio::time::sleep(Duration::from_millis(100)).await;

    let received = received_commands.lock().await;
    assert_eq!(received.len(), 8, "Should receive all 8 commands");

    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_command_result_decoding() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    let queryable = session1
        .declare_queryable("test/command")
        .await
        .expect("Failed to declare queryable");

    let handle = tokio::spawn(async move {
        if let Ok(query) = queryable.recv_async().await {
            // Create a detailed result with all fields
            let result = CommandResult {
                request_id: "test-123".to_string(),
                success: true,
                message: "Build completed successfully".to_string(),
                output: "Compiled 42 files\nTests passed".to_string(),
                node_state: Some(bubbaloop_daemon::proto::NodeState {
                    name: "test-node".to_string(),
                    path: "/path/to/node".to_string(),
                    status: bubbaloop_daemon::proto::NodeStatus::Running as i32,
                    installed: true,
                    autostart_enabled: true,
                    version: "1.0.0".to_string(),
                    description: "Test node".to_string(),
                    node_type: "rust".to_string(),
                    is_built: true,
                    last_updated_ms: 1234567890,
                    build_output: vec!["line1".to_string(), "line2".to_string()],
                    health_status: bubbaloop_daemon::proto::HealthStatus::Healthy as i32,
                    last_health_check_ms: 1234567890,
                    machine_id: String::new(),
                    machine_hostname: String::new(),
                }),
                timestamp_ms: 0,
                responding_machine: String::new(),
            };

            let reply_bytes = encode_proto(&result);
            query
                .reply(query.key_expr(), ZBytes::from(reply_bytes))
                .await
                .ok();
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let replies = session2.get("test/command").await.unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(1), replies.recv_async())
        .await
        .expect("Timeout")
        .expect("No reply")
        .into_result()
        .expect("Reply error");

    let result_bytes = reply.payload().to_bytes();
    let result: CommandResult = decode_proto(result_bytes.as_ref()).expect("Failed to decode");

    assert_eq!(result.request_id, "test-123");
    assert!(result.success);
    assert_eq!(result.message, "Build completed successfully");
    assert!(result.output.contains("Compiled 42 files"));
    assert!(result.node_state.is_some());

    let state = result.node_state.unwrap();
    assert_eq!(state.name, "test-node");
    assert_eq!(state.version, "1.0.0");
    assert!(state.is_built);
    assert_eq!(state.build_output.len(), 2);

    handle.await.expect("Handler failed");
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_invalid_payload_handling() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    let queryable = session1
        .declare_queryable("test/command")
        .await
        .expect("Failed to declare queryable");

    let handle = tokio::spawn(async move {
        if let Ok(query) = queryable.recv_async().await {
            if let Some(payload) = query.payload() {
                let bytes = payload.to_bytes();

                // Try to decode as NodeCommand
                match NodeCommand::decode(bytes.as_ref()) {
                    Ok(_cmd) => {
                        // Should not succeed with invalid data
                        panic!("Should not decode invalid payload");
                    }
                    Err(_) => {
                        // Send error result
                        let result = CommandResult {
                            request_id: "unknown".to_string(),
                            success: false,
                            message: "Invalid command payload".to_string(),
                            output: String::new(),
                            node_state: None,
                            timestamp_ms: 0,
                            responding_machine: String::new(),
                        };
                        let reply_bytes = encode_proto(&result);
                        query
                            .reply(query.key_expr(), ZBytes::from(reply_bytes))
                            .await
                            .ok();
                    }
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send invalid protobuf data
    let invalid_data = vec![0xFF, 0xFF, 0xFF, 0xFF];
    let replies = session2
        .get("test/command")
        .payload(invalid_data)
        .await
        .unwrap();

    let reply = tokio::time::timeout(Duration::from_secs(1), replies.recv_async())
        .await
        .expect("Timeout")
        .expect("No reply")
        .into_result()
        .expect("Reply error");

    let result_bytes = reply.payload().to_bytes();
    let result: CommandResult = decode_proto(result_bytes.as_ref()).expect("Failed to decode");

    assert!(!result.success);
    assert!(result.message.contains("Invalid"));

    handle.await.expect("Handler failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_malformed_command_handling() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    let queryable = session1
        .declare_queryable("test/command")
        .await
        .expect("Failed to declare queryable");

    let handle = tokio::spawn(async move {
        if let Ok(query) = queryable.recv_async().await {
            if let Some(payload) = query.payload() {
                let bytes = payload.to_bytes();
                if let Ok(cmd) = NodeCommand::decode(bytes.as_ref()) {
                    // Validate command fields
                    let result = if cmd.node_name.is_empty() {
                        CommandResult {
                            request_id: cmd.request_id,
                            success: false,
                            message: "Node name is required".to_string(),
                            output: String::new(),
                            node_state: None,
                            timestamp_ms: 0,
                            responding_machine: String::new(),
                        }
                    } else {
                        CommandResult {
                            request_id: cmd.request_id,
                            success: true,
                            message: "OK".to_string(),
                            output: String::new(),
                            node_state: None,
                            timestamp_ms: 0,
                            responding_machine: String::new(),
                        }
                    };

                    let reply_bytes = encode_proto(&result);
                    query
                        .reply(query.key_expr(), ZBytes::from(reply_bytes))
                        .await
                        .ok();
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send command with empty node name
    let command = NodeCommand {
        command: CommandType::Start as i32,
        node_name: String::new(), // Empty name
        node_path: String::new(),
        request_id: "req-123".to_string(),
        timestamp_ms: 0,
        source_machine: String::new(),
        target_machine: String::new(),
    };

    let command_bytes = encode_proto(&command);
    let replies = session2
        .get("test/command")
        .payload(command_bytes)
        .await
        .unwrap();

    let reply = tokio::time::timeout(Duration::from_secs(1), replies.recv_async())
        .await
        .expect("Timeout")
        .expect("No reply")
        .into_result()
        .expect("Reply error");

    let result_bytes = reply.payload().to_bytes();
    let result: CommandResult = decode_proto(result_bytes.as_ref()).expect("Failed to decode");

    assert!(!result.success);
    assert!(result.message.contains("required"));

    handle.await.expect("Handler failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_queryable_errors() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    let queryable = session1
        .declare_queryable("test/error")
        .await
        .expect("Failed to declare queryable");

    let handle = tokio::spawn(async move {
        if let Ok(query) = queryable.recv_async().await {
            // Simulate an error by replying with error code
            // In Zenoh, we can't directly send an error reply, but we can send
            // a result that indicates failure
            let result = CommandResult {
                request_id: "error-test".to_string(),
                success: false,
                message: "Internal server error".to_string(),
                output: "Stack trace here".to_string(),
                node_state: None,
                timestamp_ms: 0,
                responding_machine: String::new(),
            };

            let reply_bytes = encode_proto(&result);
            query
                .reply(query.key_expr(), ZBytes::from(reply_bytes))
                .await
                .ok();
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let replies = session2.get("test/error").await.unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(1), replies.recv_async())
        .await
        .expect("Timeout")
        .expect("No reply")
        .into_result()
        .expect("Reply error");

    let result_bytes = reply.payload().to_bytes();
    let result: CommandResult = decode_proto(result_bytes.as_ref()).expect("Failed to decode");

    assert!(!result.success);
    assert_eq!(result.message, "Internal server error");
    assert!(result.output.contains("Stack trace"));

    handle.await.expect("Handler failed");
}

// ============================================================================
// Wildcard Queryable Tests (like the daemon API)
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_wildcard_queryable() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    // Declare queryable with wildcard (like daemon API)
    let queryable = session1
        .declare_queryable("api/**")
        .complete(true)
        .await
        .expect("Failed to declare queryable");

    let received_keys = Arc::new(Mutex::new(Vec::new()));
    let received_keys_clone = received_keys.clone();

    let handle = tokio::spawn(async move {
        loop {
            match queryable.recv_async().await {
                Ok(query) => {
                    let key = query.key_expr().as_str().to_string();
                    received_keys_clone.lock().await.push(key.clone());

                    let response = format!("Response for {}", key);
                    query
                        .reply(query.key_expr(), ZBytes::from(response))
                        .await
                        .ok();
                }
                Err(_) => break,
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Query multiple endpoints under the wildcard
    let endpoints = vec!["api/health", "api/nodes", "api/nodes/foo"];

    for endpoint in &endpoints {
        let replies = session2.get(*endpoint).await.unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(1), replies.recv_async())
            .await
            .expect("Timeout")
            .expect("No reply")
            .into_result()
            .expect("Reply error");

        let payload = reply.payload().to_bytes();
        let response = String::from_utf8_lossy(payload.as_ref());
        assert!(response.contains(endpoint));
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    let received = received_keys.lock().await;
    assert_eq!(received.len(), 3);
    assert!(received.contains(&"api/health".to_string()));
    assert!(received.contains(&"api/nodes".to_string()));
    assert!(received.contains(&"api/nodes/foo".to_string()));

    handle.abort();
}

// ============================================================================
// Concurrent Query Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn test_concurrent_queries() {
    let _router = ZenohdHandle::start().expect("Failed to start zenohd");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session1 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");
    let session2 = setup_test_session(TestConfig::default())
        .await
        .expect("Failed to create session");

    let queryable = session1
        .declare_queryable("test/concurrent")
        .await
        .expect("Failed to declare queryable");

    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let handle = tokio::spawn(async move {
        loop {
            match queryable.recv_async().await {
                Ok(query) => {
                    let mut count = counter_clone.lock().await;
                    *count += 1;
                    let response = format!("Reply {}", *count);
                    drop(count); // Release lock before replying

                    query
                        .reply(query.key_expr(), ZBytes::from(response))
                        .await
                        .ok();
                }
                Err(_) => break,
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send 10 concurrent queries
    let mut handles = Vec::new();
    for _ in 0..10 {
        let session = session2.clone();
        let h = tokio::spawn(async move {
            let replies = session.get("test/concurrent").await.unwrap();
            tokio::time::timeout(Duration::from_secs(1), replies.recv_async())
                .await
                .expect("Timeout")
                .expect("No reply")
                .into_result()
                .expect("Reply error");
        });
        handles.push(h);
    }

    // Wait for all queries to complete
    for h in handles {
        h.await.expect("Query failed");
    }

    // All 10 queries should have been handled
    let final_count = *counter.lock().await;
    assert_eq!(final_count, 10, "Should handle all 10 concurrent queries");

    handle.abort();
}
