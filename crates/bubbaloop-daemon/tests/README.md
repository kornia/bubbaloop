# Bubbaloop Daemon Integration Tests

Comprehensive integration tests for Zenoh communication patterns used in the Bubbaloop daemon.

## Test Structure

```
tests/
├── common/
│   └── mod.rs                   # Shared test helpers and utilities
├── queryable_integration.rs     # Queryable (request/reply) pattern tests
├── coordination_scenarios.rs    # Multi-node coordination scenarios
└── README.md                    # This file
```

## Running Tests

### Unit Tests (No zenohd required)

Some tests run in peer-to-peer mode without requiring a router:

```bash
# Run common module tests
cargo test --package bubbaloop-daemon --test queryable_integration common::tests
```

### Integration Tests (Require zenohd)

Most tests require a running Zenoh router and are marked with `#[ignore]`:

#### Start zenohd on test port

```bash
# Terminal 1: Start test router
zenohd --no-multicast-scouting --listen tcp/127.0.0.1:17447

# Terminal 2: Run integration tests
cargo test --package bubbaloop-daemon --test queryable_integration -- --ignored
cargo test --package bubbaloop-daemon --test coordination_scenarios -- --ignored
```

#### Or use default port

```bash
# Terminal 1: Start router on default port
zenohd

# Terminal 2: Run all tests
cargo test --package bubbaloop-daemon --tests -- --ignored
```

## Test Files

### queryable_integration.rs

Tests the Zenoh **queryable (request/reply)** pattern used for daemon API commands.

#### Test Categories

1. **Basic Queryable Tests**
   - `test_basic_queryable_reply` - Declare queryable, send query, receive reply
   - `test_query_timeout_no_queryable` - Query timeout when no queryable exists
   - `test_multiple_queryables_different_keys` - Multiple queryables on different keys

2. **Command Execution Flow Tests**
   - `test_command_encoding_and_sending` - NodeCommand encoding and sending
   - `test_command_result_decoding` - CommandResult receiving and decoding
   - `test_various_command_types` - Test all command types (start, stop, build, etc.)

3. **Error Handling Tests**
   - `test_invalid_payload_handling` - Invalid protobuf payload
   - `test_malformed_command_handling` - Command validation errors
   - `test_queryable_errors` - Queryable error responses

4. **Advanced Patterns**
   - `test_wildcard_queryable` - Wildcard queryable (like daemon API: `api/**`)
   - `test_concurrent_queries` - Concurrent query handling

#### Example: Basic Queryable

```rust
// Declare queryable
let queryable = session.declare_queryable("test/ping").await?;

// Handle queries
tokio::spawn(async move {
    if let Ok(query) = queryable.recv_async().await {
        query.reply(query.key_expr(), ZBytes::from("pong")).await?;
    }
});

// Send query
let replies = session.get("test/ping").await?;
let reply = replies.recv_async().await?;
```

---

### coordination_scenarios.rs

Tests **multi-node coordination** patterns simulating distributed Jetson deployments.

#### Test Scenarios

1. **Synchronized Recording** (`test_synchronized_recording`)
   - Dashboard broadcasts start command to all Jetsons
   - All Jetsons acknowledge receipt
   - Tests: Broadcast pub/sub, acknowledgment collection

2. **Multi-Camera Calibration** (`test_multi_camera_calibration`)
   - Synchronized frame capture across cameras
   - Timestamp analysis for sync quality
   - Tests: Timing coordination, data aggregation

3. **Health Monitoring** (`test_health_monitoring`)
   - Periodic heartbeat publishing
   - Wildcard subscription monitoring
   - Offline node detection
   - Tests: Heartbeat patterns, wildcard subscriptions

4. **Command Relay** (`test_command_relay`)
   - Multi-hop request/reply chains
   - Jetson1 queries Jetson2, processes data, replies to Dashboard
   - Tests: Queryable chaining, data transformation

5. **Timeout Handling** (`test_timeout_handling`)
   - Command sent with no listeners
   - Graceful timeout handling
   - Tests: Timeout logic, non-blocking waits

6. **Error Propagation** (`test_error_propagation`)
   - Node fails to execute command
   - Error acknowledgment propagation
   - Tests: Failure handling, negative cases

7. **State Consistency** (`test_state_consistency_under_concurrent_updates`)
   - Concurrent state updates from multiple tasks
   - Lock-free state management
   - Tests: Race conditions, RwLock correctness

#### Message Flow Examples

**Synchronized Recording:**
```
Dashboard                  Jetson1             Jetson2             Jetson3
    |                         |                   |                   |
    |-- start_recording ----->|<------ pub/sub ---|                   |
    |                         |                   |<------ pub/sub ---|
    |<----- ack ------------- |                   |                   |
    |<----- ack ------------------------------ |                   |
    |<----- ack -------------------------------------------- |
```

**Command Relay:**
```
Dashboard             Jetson1               Jetson2
    |                    |                      |
    |-- command -------->|                      |
    |                    |-- query (get) ------>|
    |                    |<----- reply ---------|
    |<----- response ----|   (processed data)   |
```

---

## Test Helpers (common/mod.rs)

### Core Utilities

```rust
// Create test Zenoh session
let session = setup_test_session(TestConfig::default()).await?;

// Encode/decode protobuf messages
let bytes = encode_proto(&command);
let decoded: NodeCommand = decode_proto(&bytes)?;
```

### Process Management

```rust
// Auto-start zenohd for test (auto-cleanup on drop)
let _router = ZenohdHandle::start()?;
tokio::time::sleep(Duration::from_millis(500)).await;
```

### Test Data Builders

```rust
// Build test commands
let cmd = create_test_command(CommandType::Start as i32, "node", "/path");
let result = create_test_result("req-123", true, "Success");
```

### Utilities

```rust
// Wait for async condition with timeout
wait_for(5, || async {
    counter.load(Ordering::SeqCst) >= 10
}).await?;
```

---

## Configuration

### Test Router Port

- **Production**: `tcp/127.0.0.1:7447`
- **Test**: `tcp/127.0.0.1:17447` (avoid conflicts)

### Runtime Requirements

All async tests use **multi-threaded tokio runtime** (required by Zenoh):

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn my_test() { ... }
```

Single-threaded runtime will panic with:
> "Zenoh runtime doesn't support Tokio's current thread scheduler"

---

## Troubleshooting

### Tests hang waiting for replies

- Ensure zenohd is running on correct port
- Check firewall settings
- Verify no port conflicts

### "Address already in use"

```bash
pkill zenohd
# Then restart tests
```

### "Zenoh runtime doesn't support current thread scheduler"

Use multi-threaded runtime:
```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
```

---

## Adding New Tests

### 1. Import test helpers

```rust
use common::{
    setup_test_session, encode_proto, decode_proto,
    TestConfig, ZenohdHandle
};
```

### 2. Mark tests requiring zenohd

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires zenohd at tcp/127.0.0.1:17447"]
async fn my_integration_test() {
    let _router = ZenohdHandle::start().unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let session = setup_test_session(TestConfig::default()).await.unwrap();
    // ... test logic
}
```

### 3. Follow test patterns

- **Queryable tests**: One session for queryable, one for query
- **Coordination tests**: One session per simulated node
- **Always use timeouts**: `tokio::time::timeout()`
- **Clean shutdown**: Resources auto-drop, but explicit cleanup is clearer

---

## Architecture Validation

These tests validate the following distributed patterns:

| Pattern | Test File | Tests |
|---------|-----------|-------|
| **Pub/Sub Broadcast** | coordination_scenarios | Synchronized recording, Calibration |
| **Queryable (RPC)** | queryable_integration | All queryable tests, Command relay |
| **Wildcard Subscriptions** | coordination_scenarios | Health monitoring (`bubbaloop/heartbeat/*`) |
| **Acknowledgment Collection** | coordination_scenarios | Synchronized recording |
| **Multi-hop Messaging** | coordination_scenarios | Command relay |
| **Timeout Handling** | Both | All tests with timeouts |
| **Error Propagation** | Both | Error handling tests |
| **Concurrent Updates** | coordination_scenarios | State consistency test |
| **Protobuf Encoding** | queryable_integration | Command encoding/decoding |

---

## Performance Considerations

### Current Focus

Tests prioritize **correctness** over performance:
- Message delivery reliability
- State consistency
- Error handling
- Timeout behavior

### Future Performance Tests

Potential benchmarks to add:
- Message latency (pub → sub)
- Queryable response time
- Acknowledgment round-trip time
- Maximum sustainable message rate
- Memory usage under load
- Shared memory transport performance

---

## Debugging

### Enable logging

```bash
RUST_LOG=debug cargo test --package bubbaloop-daemon --test queryable_integration -- --ignored --nocapture
```

### Run specific test

```bash
cargo test --package bubbaloop-daemon --test queryable_integration test_basic_queryable_reply -- --ignored --nocapture
```

### Check test isolation

Each test should be independent. If tests fail when run together but pass individually:
- Check for shared state (shouldn't exist)
- Verify unique topic namespaces
- Ensure proper cleanup (drop handles)

---

## CI/CD Integration

### GitHub Actions Example

```yaml
- name: Start Zenoh Router
  run: |
    zenohd --no-multicast-scouting --listen tcp/127.0.0.1:17447 &
    sleep 2

- name: Run Integration Tests
  run: |
    cargo test --package bubbaloop-daemon --tests -- --ignored
```

### Local Pre-commit Hook

```bash
#!/bin/bash
zenohd --no-multicast-scouting --listen tcp/127.0.0.1:17447 &
ZENOHD_PID=$!
sleep 2

cargo test --package bubbaloop-daemon --tests -- --ignored

kill $ZENOHD_PID
```
