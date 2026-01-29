//! Coordination Scenario Tests
//!
//! Simulates inter-Jetson coordination scenarios using multiple Zenoh sessions
//! within a single process. Each "Jetson" is represented by a separate Zenoh
//! session and tokio task to simulate distributed behavior.

#![allow(clippy::while_let_loop)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex, RwLock};
use zenoh::bytes::ZBytes;
use zenoh::Session;

// Import protobuf messages if needed
// For now, we'll define simple coordination messages

/// Simple protobuf-like message for testing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RecordingCommand {
    command: String,
    timestamp_ms: i64,
    jetson_id: String,
}

impl RecordingCommand {
    fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }

    fn decode(data: &[u8]) -> Self {
        serde_json::from_slice(data).unwrap()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Acknowledgment {
    jetson_id: String,
    command: String,
    timestamp_ms: i64,
    success: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FrameCapture {
    jetson_id: String,
    frame_id: u64,
    timestamp_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Heartbeat {
    jetson_id: String,
    timestamp_ms: i64,
    status: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CalibrationResult {
    jetson_id: String,
    sync_quality: f64,
    timestamp_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct QueryResponse {
    request_id: String,
    from_jetson: String,
    data: String,
}

/// Helper to create a test Zenoh session
async fn create_test_session() -> Arc<Session> {
    let mut config = zenoh::Config::default();
    config.insert_json5("mode", "\"peer\"").ok();

    // Connect to local zenohd if available, otherwise use peer-to-peer
    config
        .insert_json5("connect/endpoints", "[\"tcp/127.0.0.1:7447\"]")
        .ok();

    // Enable shared memory for local communication
    config
        .insert_json5("transport/shared_memory/enabled", "true")
        .ok();

    let session = zenoh::open(config).await.unwrap();
    Arc::new(session)
}

/// Get current timestamp in milliseconds
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Simulated Jetson node
struct JetsonNode {
    id: String,
    session: Arc<Session>,
    state: Arc<RwLock<JetsonState>>,
}

#[derive(Debug, Default)]
struct JetsonState {
    recording: bool,
    last_command: Option<String>,
    last_command_timestamp: i64,
    frame_count: u64,
    heartbeat_count: u64,
}

impl JetsonNode {
    async fn new(id: String, session: Arc<Session>) -> Self {
        Self {
            id,
            session,
            state: Arc::new(RwLock::new(JetsonState::default())),
        }
    }

    /// Start listening for recording commands
    async fn listen_recording_commands(
        &self,
        ack_tx: mpsc::Sender<Acknowledgment>,
    ) -> tokio::task::JoinHandle<()> {
        let subscriber = self
            .session
            .declare_subscriber("bubbaloop/coordination/recording/start")
            .await
            .unwrap();

        let jetson_id = self.id.clone();
        let state = self.state.clone();

        tokio::spawn(async move {
            loop {
                match subscriber.recv_async().await {
                    Ok(sample) => {
                        let data = sample.payload().to_bytes().to_vec();
                        let cmd = RecordingCommand::decode(&data);

                        // Update state
                        {
                            let mut s = state.write().await;
                            s.recording = true;
                            s.last_command = Some(cmd.command.clone());
                            s.last_command_timestamp = cmd.timestamp_ms;
                        }

                        // Send acknowledgment
                        let ack = Acknowledgment {
                            jetson_id: jetson_id.clone(),
                            command: cmd.command,
                            timestamp_ms: now_ms(),
                            success: true,
                        };

                        ack_tx.send(ack).await.ok();
                    }
                    Err(_) => break,
                }
            }
        })
    }

    /// Publish heartbeat
    async fn publish_heartbeat(&self) {
        let heartbeat = Heartbeat {
            jetson_id: self.id.clone(),
            timestamp_ms: now_ms(),
            status: "healthy".to_string(),
        };

        let data = serde_json::to_vec(&heartbeat).unwrap();
        self.session
            .put(
                format!("bubbaloop/heartbeat/{}", self.id),
                ZBytes::from(data),
            )
            .await
            .ok();

        let mut state = self.state.write().await;
        state.heartbeat_count += 1;
    }

    /// Capture a frame for calibration
    async fn capture_frame(&self) -> FrameCapture {
        let mut state = self.state.write().await;
        state.frame_count += 1;

        FrameCapture {
            jetson_id: self.id.clone(),
            frame_id: state.frame_count,
            timestamp_ms: now_ms(),
        }
    }
}

/// Coordinator that manages multiple Jetsons
struct Coordinator {
    session: Arc<Session>,
}

impl Coordinator {
    async fn new(session: Arc<Session>) -> Self {
        Self { session }
    }

    /// Broadcast start recording command
    async fn start_recording(&self) {
        let cmd = RecordingCommand {
            command: "start".to_string(),
            timestamp_ms: now_ms(),
            jetson_id: "dashboard".to_string(),
        };

        let data = cmd.encode();
        self.session
            .put("bubbaloop/coordination/recording/start", ZBytes::from(data))
            .await
            .ok();
    }

    /// Collect acknowledgments
    async fn collect_acks(
        &self,
        ack_rx: &mut mpsc::Receiver<Acknowledgment>,
        expected_count: usize,
        timeout: Duration,
    ) -> Vec<Acknowledgment> {
        let mut acks = Vec::new();
        let deadline = tokio::time::Instant::now() + timeout;

        while acks.len() < expected_count {
            match tokio::time::timeout_at(deadline, ack_rx.recv()).await {
                Ok(Some(ack)) => acks.push(ack),
                Ok(None) => break,
                Err(_) => break, // Timeout
            }
        }

        acks
    }

    /// Monitor heartbeats
    async fn monitor_heartbeats(&self, duration: Duration) -> HashMap<String, Vec<Heartbeat>> {
        let subscriber = self
            .session
            .declare_subscriber("bubbaloop/heartbeat/*")
            .await
            .unwrap();

        let heartbeats: Arc<Mutex<HashMap<String, Vec<Heartbeat>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let heartbeats_clone = heartbeats.clone();

        let handle = tokio::spawn(async move {
            loop {
                match subscriber.recv_async().await {
                    Ok(sample) => {
                        let data = sample.payload().to_bytes().to_vec();
                        if let Ok(hb) = serde_json::from_slice::<Heartbeat>(&data) {
                            let mut hbs = heartbeats_clone.lock().await;
                            hbs.entry(hb.jetson_id.clone()).or_default().push(hb);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        tokio::time::sleep(duration).await;
        handle.abort();

        let result = heartbeats.lock().await.clone();
        result
    }

    /// Initiate calibration
    async fn calibrate(
        &self,
        frame_rx: &mut mpsc::Receiver<FrameCapture>,
        expected_count: usize,
        timeout: Duration,
    ) -> CalibrationResult {
        // Signal calibration start
        self.session
            .put(
                "bubbaloop/coordination/calibration/start",
                ZBytes::from(vec![1]),
            )
            .await
            .ok();

        // Collect frame captures
        let mut captures = Vec::new();
        let deadline = tokio::time::Instant::now() + timeout;

        while captures.len() < expected_count {
            match tokio::time::timeout_at(deadline, frame_rx.recv()).await {
                Ok(Some(capture)) => captures.push(capture),
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // Calculate sync quality based on timestamp variance
        if captures.is_empty() {
            return CalibrationResult {
                jetson_id: "coordinator".to_string(),
                sync_quality: 0.0,
                timestamp_ms: now_ms(),
            };
        }

        let timestamps: Vec<i64> = captures.iter().map(|c| c.timestamp_ms).collect();
        let mean = timestamps.iter().sum::<i64>() / timestamps.len() as i64;
        let variance: f64 = timestamps
            .iter()
            .map(|&t| {
                let diff = (t - mean) as f64;
                diff * diff
            })
            .sum::<f64>()
            / timestamps.len() as f64;

        let std_dev = variance.sqrt();
        // Lower variance = better sync quality (inverted and normalized)
        let sync_quality = 1.0 / (1.0 + std_dev / 1000.0);

        CalibrationResult {
            jetson_id: "coordinator".to_string(),
            sync_quality,
            timestamp_ms: now_ms(),
        }
    }
}

//
// TEST SCENARIOS
//

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_synchronized_recording() {
    // Create sessions for 3 Jetsons + 1 coordinator
    let jetson1_session = create_test_session().await;
    let jetson2_session = create_test_session().await;
    let jetson3_session = create_test_session().await;
    let coordinator_session = create_test_session().await;

    // Create Jetson nodes
    let jetson1 = JetsonNode::new("jetson1".to_string(), jetson1_session).await;
    let jetson2 = JetsonNode::new("jetson2".to_string(), jetson2_session).await;
    let jetson3 = JetsonNode::new("jetson3".to_string(), jetson3_session).await;

    // Create acknowledgment channel
    let (ack_tx, mut ack_rx) = mpsc::channel(10);

    // Start listening on all Jetsons
    let _h1 = jetson1.listen_recording_commands(ack_tx.clone()).await;
    let _h2 = jetson2.listen_recording_commands(ack_tx.clone()).await;
    let _h3 = jetson3.listen_recording_commands(ack_tx.clone()).await;

    // Give subscribers time to set up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create coordinator and send command
    let coordinator = Coordinator::new(coordinator_session).await;
    coordinator.start_recording().await;

    // Collect acknowledgments with timeout
    let acks = coordinator
        .collect_acks(&mut ack_rx, 3, Duration::from_secs(2))
        .await;

    // Assertions
    assert_eq!(acks.len(), 3, "Should receive 3 acknowledgments");

    let jetson_ids: Vec<String> = acks.iter().map(|a| a.jetson_id.clone()).collect();
    assert!(jetson_ids.contains(&"jetson1".to_string()));
    assert!(jetson_ids.contains(&"jetson2".to_string()));
    assert!(jetson_ids.contains(&"jetson3".to_string()));

    // Verify all acks are successful
    assert!(acks.iter().all(|a| a.success));

    // Verify state changes
    assert!(jetson1.state.read().await.recording);
    assert!(jetson2.state.read().await.recording);
    assert!(jetson3.state.read().await.recording);

    // Verify message ordering (all acks should be within reasonable time)
    let timestamps: Vec<i64> = acks.iter().map(|a| a.timestamp_ms).collect();
    let max_timestamp = *timestamps.iter().max().unwrap();
    let min_timestamp = *timestamps.iter().min().unwrap();
    assert!(
        max_timestamp - min_timestamp < 1000,
        "All acknowledgments should arrive within 1 second"
    );
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_multi_camera_calibration() {
    // Create sessions
    let jetson1_session = create_test_session().await;
    let jetson2_session = create_test_session().await;
    let jetson3_session = create_test_session().await;
    let coordinator_session = create_test_session().await;

    // Create Jetson nodes
    let jetson1 = Arc::new(JetsonNode::new("jetson1".to_string(), jetson1_session).await);
    let jetson2 = Arc::new(JetsonNode::new("jetson2".to_string(), jetson2_session).await);
    let jetson3 = Arc::new(JetsonNode::new("jetson3".to_string(), jetson3_session).await);

    // Create frame capture channel
    let (frame_tx, mut frame_rx) = mpsc::channel(10);

    // Spawn tasks to capture frames when calibration starts
    let jetsons = vec![jetson1.clone(), jetson2.clone(), jetson3.clone()];
    for jetson in jetsons {
        let frame_tx = frame_tx.clone();
        let sub = coordinator_session
            .declare_subscriber("bubbaloop/coordination/calibration/start")
            .await
            .unwrap();

        tokio::spawn(async move {
            if let Ok(_sample) = sub.recv_async().await {
                // Small random delay to simulate real capture timing variance
                let delay_ms = (jetson.id.len() % 10) as u64 * 5;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                let capture = jetson.capture_frame().await;
                let data = serde_json::to_vec(&capture).unwrap();

                // Publish frame capture
                jetson
                    .session
                    .put(
                        format!("bubbaloop/coordination/calibration/frame/{}", jetson.id),
                        ZBytes::from(data.clone()),
                    )
                    .await
                    .ok();

                frame_tx.send(capture).await.ok();
            }
        });
    }

    // Give subscribers time to set up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create coordinator and initiate calibration
    let coordinator = Coordinator::new(coordinator_session).await;
    let result = coordinator
        .calibrate(&mut frame_rx, 3, Duration::from_secs(2))
        .await;

    // Assertions
    assert!(
        result.sync_quality > 0.5,
        "Sync quality should be reasonable: {}",
        result.sync_quality
    );

    // Verify all Jetsons captured frames
    assert_eq!(jetson1.state.read().await.frame_count, 1);
    assert_eq!(jetson2.state.read().await.frame_count, 1);
    assert_eq!(jetson3.state.read().await.frame_count, 1);
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_health_monitoring() {
    // Create sessions
    let jetson1_session = create_test_session().await;
    let jetson2_session = create_test_session().await;
    let jetson3_session = create_test_session().await;
    let coordinator_session = create_test_session().await;

    // Create Jetson nodes
    let jetson1 = Arc::new(JetsonNode::new("jetson1".to_string(), jetson1_session).await);
    let jetson2 = Arc::new(JetsonNode::new("jetson2".to_string(), jetson2_session).await);
    let jetson3 = Arc::new(JetsonNode::new("jetson3".to_string(), jetson3_session).await);

    // Create coordinator
    let coordinator = Coordinator::new(coordinator_session).await;

    // Jetson1 and Jetson2 publish heartbeats regularly
    let j1 = jetson1.clone();
    let j1_task = tokio::spawn(async move {
        for _ in 0..5 {
            j1.publish_heartbeat().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let j2 = jetson2.clone();
    let j2_task = tokio::spawn(async move {
        for _ in 0..5 {
            j2.publish_heartbeat().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    // Jetson3 publishes 2 heartbeats then goes offline
    let j3 = jetson3.clone();
    let j3_task = tokio::spawn(async move {
        for _ in 0..2 {
            j3.publish_heartbeat().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // Simulate going offline (stop publishing)
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    // Monitor heartbeats
    let heartbeats = coordinator
        .monitor_heartbeats(Duration::from_millis(600))
        .await;

    // Wait for tasks to complete
    j1_task.await.ok();
    j2_task.await.ok();
    j3_task.await.ok();

    // Assertions
    assert!(
        heartbeats.contains_key("jetson1"),
        "Should receive heartbeats from jetson1"
    );
    assert!(
        heartbeats.contains_key("jetson2"),
        "Should receive heartbeats from jetson2"
    );
    assert!(
        heartbeats.contains_key("jetson3"),
        "Should receive heartbeats from jetson3"
    );

    // Jetson1 and Jetson2 should have ~5 heartbeats
    assert!(
        heartbeats.get("jetson1").unwrap().len() >= 4,
        "Jetson1 should have sent multiple heartbeats"
    );
    assert!(
        heartbeats.get("jetson2").unwrap().len() >= 4,
        "Jetson2 should have sent multiple heartbeats"
    );

    // Jetson3 should have only 2 heartbeats (went offline)
    assert_eq!(
        heartbeats.get("jetson3").unwrap().len(),
        2,
        "Jetson3 should have only 2 heartbeats before going offline"
    );

    // Verify state consistency
    assert_eq!(jetson1.state.read().await.heartbeat_count, 5);
    assert_eq!(jetson2.state.read().await.heartbeat_count, 5);
    assert_eq!(jetson3.state.read().await.heartbeat_count, 2);
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_command_relay() {
    // Create sessions
    let jetson1_session = create_test_session().await;
    let jetson2_session = create_test_session().await;
    let dashboard_session = create_test_session().await;

    // Create response channel
    let (resp_tx, mut resp_rx) = mpsc::channel(10);

    // Jetson2 provides data via queryable
    let j2_queryable = jetson2_session
        .declare_queryable("bubbaloop/query/jetson2/data")
        .await
        .unwrap();

    let resp_tx_clone = resp_tx.clone();
    tokio::spawn(async move {
        if let Ok(query) = j2_queryable.recv_async().await {
            let response = QueryResponse {
                request_id: "req1".to_string(),
                from_jetson: "jetson2".to_string(),
                data: "sensor_data_xyz".to_string(),
            };

            let data = serde_json::to_vec(&response).unwrap();
            query
                .reply(query.key_expr(), ZBytes::from(data.clone()))
                .await
                .ok();

            resp_tx_clone.send(response).await.ok();
        }
    });

    // Give queryable time to set up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Jetson1 receives command from dashboard, needs to query Jetson2
    let j1_subscriber = jetson1_session
        .declare_subscriber("bubbaloop/command/jetson1/process")
        .await
        .unwrap();

    let j1_session = jetson1_session.clone();
    let resp_tx_clone = resp_tx.clone();
    tokio::spawn(async move {
        if let Ok(_sample) = j1_subscriber.recv_async().await {
            // Query Jetson2 for data
            let query = j1_session
                .get("bubbaloop/query/jetson2/data")
                .await
                .unwrap();

            // Wait for reply
            if let Ok(reply) = query.recv_async().await {
                if let Ok(sample) = reply.result() {
                    let data = sample.payload().to_bytes();
                    let response: QueryResponse = serde_json::from_slice(&data).unwrap();

                    // Combine results and send back to dashboard
                    let combined = QueryResponse {
                        request_id: response.request_id,
                        from_jetson: "jetson1".to_string(),
                        data: format!("processed_{}", response.data),
                    };

                    let data = serde_json::to_vec(&combined).unwrap();
                    j1_session
                        .put("bubbaloop/response/dashboard", ZBytes::from(data.clone()))
                        .await
                        .ok();

                    resp_tx_clone.send(combined).await.ok();
                }
            }
        }
    });

    // Dashboard subscribes to responses
    let dashboard_subscriber = dashboard_session
        .declare_subscriber("bubbaloop/response/dashboard")
        .await
        .unwrap();

    let resp_tx_clone = resp_tx.clone();
    tokio::spawn(async move {
        if let Ok(sample) = dashboard_subscriber.recv_async().await {
            let data = sample.payload().to_bytes().to_vec();
            let response: QueryResponse = serde_json::from_slice(&data).unwrap();
            resp_tx_clone.send(response).await.ok();
        }
    });

    // Give subscribers time to set up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Dashboard sends command to Jetson1
    dashboard_session
        .put("bubbaloop/command/jetson1/process", ZBytes::from(vec![1]))
        .await
        .unwrap();

    // Wait for responses with timeout
    let mut responses = Vec::new();
    let timeout = Duration::from_secs(2);
    let deadline = tokio::time::Instant::now() + timeout;

    while responses.len() < 3 {
        match tokio::time::timeout_at(deadline, resp_rx.recv()).await {
            Ok(Some(resp)) => responses.push(resp),
            Ok(None) => break,
            Err(_) => break,
        }
    }

    // Assertions
    assert_eq!(responses.len(), 3, "Should receive 3 responses");

    // Verify query chain: Jetson2 -> Jetson1 -> Dashboard
    let j2_response = responses
        .iter()
        .find(|r| r.from_jetson == "jetson2")
        .unwrap();
    assert_eq!(j2_response.data, "sensor_data_xyz");

    let j1_response = responses
        .iter()
        .find(|r| r.from_jetson == "jetson1")
        .unwrap();
    assert_eq!(j1_response.data, "processed_sensor_data_xyz");

    // Verify message ordering - Jetson2 should respond before Jetson1
    let j2_idx = responses
        .iter()
        .position(|r| r.from_jetson == "jetson2")
        .unwrap();
    let j1_idx = responses
        .iter()
        .position(|r| r.from_jetson == "jetson1")
        .unwrap();
    assert!(
        j2_idx < j1_idx,
        "Jetson2 should respond before Jetson1 in the query chain"
    );
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_timeout_handling() {
    // Create sessions
    let coordinator_session = create_test_session().await;
    let jetson1_session = create_test_session().await;

    // Create Jetson that will timeout (doesn't respond)
    let _jetson1 = JetsonNode::new("jetson1".to_string(), jetson1_session).await;

    // Create acknowledgment channel
    let (_ack_tx, mut ack_rx) = mpsc::channel::<Acknowledgment>(10);

    // Create coordinator and send command
    let coordinator = Coordinator::new(coordinator_session).await;
    coordinator.start_recording().await;

    // Try to collect acknowledgments with short timeout
    let acks = coordinator
        .collect_acks(&mut ack_rx, 3, Duration::from_millis(500))
        .await;

    // Assertions - should timeout with no acks
    assert_eq!(
        acks.len(),
        0,
        "Should receive no acknowledgments due to timeout"
    );
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_error_propagation() {
    // Create sessions
    let coordinator_session = create_test_session().await;
    let jetson1_session = create_test_session().await;

    // Create Jetson that will fail
    let jetson1 = JetsonNode::new("jetson1".to_string(), jetson1_session).await;

    // Create acknowledgment channel
    let (ack_tx, mut ack_rx) = mpsc::channel(10);

    // Subscribe and send failure acknowledgment
    let subscriber = jetson1
        .session
        .declare_subscriber("bubbaloop/coordination/recording/start")
        .await
        .unwrap();

    let jetson_id = jetson1.id.clone();
    tokio::spawn(async move {
        if let Ok(sample) = subscriber.recv_async().await {
            let data = sample.payload().to_bytes().to_vec();
            let cmd = RecordingCommand::decode(&data);

            // Send failure acknowledgment
            let ack = Acknowledgment {
                jetson_id: jetson_id.clone(),
                command: cmd.command,
                timestamp_ms: now_ms(),
                success: false, // Indicate failure
            };

            ack_tx.send(ack).await.ok();
        }
    });

    // Give subscriber time to set up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create coordinator and send command
    let coordinator = Coordinator::new(coordinator_session).await;
    coordinator.start_recording().await;

    // Collect acknowledgments
    let acks = coordinator
        .collect_acks(&mut ack_rx, 1, Duration::from_secs(1))
        .await;

    // Assertions
    assert_eq!(acks.len(), 1);
    assert!(!acks[0].success, "Should receive failure acknowledgment");
    assert_eq!(acks[0].jetson_id, "jetson1");
}

#[tokio::test]
#[ignore] // Requires zenohd running
async fn test_state_consistency_under_concurrent_updates() {
    // Create session
    let session = create_test_session().await;
    let jetson = Arc::new(JetsonNode::new("jetson1".to_string(), session.clone()).await);

    // Spawn multiple tasks that concurrently update state
    let mut handles = Vec::new();

    // Task 1: Publish heartbeats
    let j = jetson.clone();
    handles.push(tokio::spawn(async move {
        for _ in 0..10 {
            j.publish_heartbeat().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }));

    // Task 2: Capture frames
    let j = jetson.clone();
    handles.push(tokio::spawn(async move {
        for _ in 0..10 {
            j.capture_frame().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }));

    // Task 3: Update recording state
    let j = jetson.clone();
    handles.push(tokio::spawn(async move {
        for i in 0..10 {
            let mut state = j.state.write().await;
            state.recording = i % 2 == 0;
            drop(state);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }));

    // Wait for all tasks
    for handle in handles {
        handle.await.ok();
    }

    // Verify state consistency
    let state = jetson.state.read().await;
    assert_eq!(state.heartbeat_count, 10);
    assert_eq!(state.frame_count, 10);
    assert!(!state.recording); // Last update was i=9 (odd)
}
