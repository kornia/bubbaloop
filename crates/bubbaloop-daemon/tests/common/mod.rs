//! Test helpers for Zenoh integration tests

#![allow(dead_code)]

use prost::Message;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use zenoh::Session;

/// Configuration for test Zenoh sessions
pub struct TestConfig {
    pub router_required: bool,
    pub timeout_secs: u64,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            router_required: true,
            timeout_secs: 5,
        }
    }
}

/// Handle to a running zenohd process for testing
pub struct ZenohdHandle {
    child: Option<Child>,
}

impl ZenohdHandle {
    /// Start a zenohd router for testing
    ///
    /// Note: This requires `zenohd` to be in PATH.
    /// Tests using this should be marked with `#[ignore]` if zenohd is not available.
    pub fn start() -> Result<Self, std::io::Error> {
        // Kill any existing zenohd to ensure clean state
        let _ = Command::new("pkill").arg("zenohd").output();

        // Give it a moment to fully terminate
        std::thread::sleep(Duration::from_millis(100));

        // Start zenohd with a unique port for testing
        let child = Command::new("zenohd")
            .arg("--no-multicast-scouting")
            .arg("--listen")
            .arg("tcp/127.0.0.1:17447") // Different port than production
            .spawn()?;

        // Give zenohd time to start up
        std::thread::sleep(Duration::from_millis(500));

        Ok(Self { child: Some(child) })
    }

    /// Stop the zenohd router
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ZenohdHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Setup a Zenoh session for testing
///
/// If `config.router_required` is true, this will connect to tcp/127.0.0.1:17447.
/// Otherwise, it creates a peer-to-peer session.
pub async fn setup_test_session(config: TestConfig) -> Result<Arc<Session>, zenoh::Error> {
    let mut zenoh_config = zenoh::Config::default();

    if config.router_required {
        // Connect to test router
        zenoh_config.insert_json5("mode", "\"peer\"").ok();
        zenoh_config
            .insert_json5("connect/endpoints", "[\"tcp/127.0.0.1:17447\"]")
            .ok();
        zenoh_config
            .insert_json5("scouting/multicast/enabled", "false")
            .ok();
    } else {
        // Peer mode for tests that don't need router
        zenoh_config.insert_json5("mode", "\"peer\"").ok();
        zenoh_config
            .insert_json5("scouting/multicast/enabled", "false")
            .ok();
    }

    let session = zenoh::open(zenoh_config).await?;
    Ok(Arc::new(session))
}

/// Setup a test session with default config and timeout
pub async fn setup_test_session_default() -> Result<Arc<Session>, zenoh::Error> {
    setup_test_session(TestConfig::default()).await
}

/// Encode a protobuf message to bytes
pub fn encode_proto<M: Message>(msg: &M) -> Vec<u8> {
    let mut buf = Vec::new();
    msg.encode(&mut buf).expect("Failed to encode protobuf");
    buf
}

/// Decode a protobuf message from bytes
pub fn decode_proto<M: Message + Default>(bytes: &[u8]) -> Result<M, prost::DecodeError> {
    M::decode(bytes)
}

/// Wait for a condition with timeout
pub async fn wait_for<F, Fut>(
    timeout_secs: u64,
    mut condition: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let result = timeout(Duration::from_secs(timeout_secs), async {
        loop {
            if condition().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Timeout waiting for condition".into()),
    }
}

/// Helper to create a test NodeCommand
pub fn create_test_command(
    command_type: i32,
    node_name: &str,
    node_path: &str,
) -> bubbaloop_daemon::proto::NodeCommand {
    bubbaloop_daemon::proto::NodeCommand {
        command: command_type,
        node_name: node_name.to_string(),
        node_path: node_path.to_string(),
        request_id: uuid::Uuid::new_v4().to_string(),
    }
}

/// Helper to create a test CommandResult
pub fn create_test_result(
    request_id: &str,
    success: bool,
    message: &str,
) -> bubbaloop_daemon::proto::CommandResult {
    bubbaloop_daemon::proto::CommandResult {
        request_id: request_id.to_string(),
        success,
        message: message.to_string(),
        output: String::new(),
        node_state: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_session_setup_peer_mode() {
        // Test creating a session in peer mode (no router required)
        let config = TestConfig {
            router_required: false,
            timeout_secs: 2,
        };

        let session = setup_test_session(config).await;
        assert!(session.is_ok(), "Should create peer session");
    }

    #[test]
    fn test_proto_encoding() {
        // Test encoding and decoding protobuf messages
        use bubbaloop_daemon::proto::{CommandType, NodeCommand};

        let cmd = create_test_command(CommandType::Start as i32, "test-node", "/path/to/node");

        let encoded = encode_proto(&cmd);
        assert!(!encoded.is_empty(), "Encoded message should not be empty");

        let decoded: NodeCommand = decode_proto(&encoded).expect("Should decode");
        assert_eq!(decoded.node_name, "test-node");
        assert_eq!(decoded.node_path, "/path/to/node");
        assert_eq!(decoded.command, CommandType::Start as i32);
    }
}
