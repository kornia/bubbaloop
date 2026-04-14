---
description: "Bubbaloop vs ROS1 comparison. Why teams migrating from ROS1 should consider a modern Hardware AI agent instead of ROS2 for edge AI and IoT projects."
---

# Bubbaloop vs ROS1

ROS1 Noetic reached end-of-life on May 31, 2025. If you're evaluating what comes next, here's how Bubbaloop compares.

---

## At a Glance

| | Bubbaloop | ROS1 (Noetic) |
|---|---|---|
| **Status** | Active development | End-of-life (May 2025) |
| **Architecture** | Single binary (~13 MB) | roscore + catkin workspace |
| **Transport** | Zenoh | TCPROS (custom TCP sockets) |
| **Discovery** | Decentralized (Zenoh scouting) | Centralized (rosmaster — single point of failure) |
| **Languages** | Rust, Python, TypeScript | C++, Python |
| **AI agents** | Built-in multi-agent runtime | None |
| **Security** | RBAC, bearer tokens, localhost binding | None (any network peer can publish to any topic) |
| **Jetson Orin** | Native ARM64 binary | Not supported on JetPack 6 (Ubuntu 22.04) |
| **Install** | 30 seconds | APT on Ubuntu 20.04 only |
| **Real-time** | No | No |

---

## Why Move Off ROS1?

ROS1 Noetic is **unmaintained** — no security patches, no bug fixes, no updates. The specific issues:

- **Security**: No encryption, no authentication. Any process on the network can publish to any topic or call any service.
- **rosmaster**: Single point of failure. If it crashes, no new nodes can connect.
- **Ubuntu 20.04 lock-in**: Noetic only runs on Ubuntu 20.04. Jetson Orin (JetPack 6) ships Ubuntu 22.04 — Noetic packages return 404.
- **Python 2 legacy**: Code from Kinetic/Melodic required manual porting. The ecosystem still has unmigrated packages.
- **No lifecycle management**: Nodes start and crash. No standard state machine for configured/inactive/active transitions.

---

## If Your Project Is an Edge Device

Many ROS1 users aren't building multi-joint robots — they're running cameras, sensors, and ML inference on Jetson or Raspberry Pi. For these use cases, ROS1 was always overkill, and ROS2 may be too.

**Bubbaloop is designed for exactly this:**

```bash
# Install and run on Jetson
curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
bubbaloop up
bubbaloop node install rtsp-camera
bubbaloop agent chat "Start monitoring the entrance camera"
```

No roscore. No catkin workspace. No DDS configuration. One binary, one command.

---

## Migration Comparison

| From ROS1 to... | Bubbaloop | ROS2 |
|---|---|---|
| **Effort** | Rewrite nodes using Node SDK (~50 lines each) | Port catkin to ament/colcon, rewrite launch files, new API |
| **Transport** | Zenoh (simpler than DDS) | DDS (complex, multicast issues) |
| **Time to first node** | Minutes | Hours to days |
| **Ecosystem gap** | Small ecosystem, focused on edge AI | Large ecosystem, Nav2/MoveIt2/Gazebo |
| **AI integration** | Built-in | Bolt-on |

---

## Feature Comparison

### Messaging

| | Bubbaloop | ROS1 |
|---|---|---|
| **Protocol** | Zenoh pub/sub | TCPROS/UDPROS |
| **Discovery** | Decentralized, automatic | rosmaster (must be running first) |
| **Serialization** | Protobuf | Custom ROS serialization |
| **QoS** | Best-effort + reliable | None (no QoS controls) |
| **Cross-machine** | Works with router peering | Requires ROS_MASTER_URI on every machine |
| **Shared memory** | Zenoh SHM | Not supported |

### Node Development

**Bubbaloop Node SDK (Rust):**
```rust
use bubbaloop_node::{Node, NodeContext, run_node};

struct MySensor;

#[async_trait]
impl Node for MySensor {
    async fn init(ctx: &NodeContext) -> anyhow::Result<Self> { Ok(Self) }
    async fn run(self, ctx: NodeContext) -> anyhow::Result<()> {
        let pub_json = ctx.publisher_json("data").await?;
        // ... publish sensor data
        Ok(())
    }
}

#[tokio::main]
async fn main() { run_node::<MySensor>().await.unwrap(); }
```

**ROS1 (Python):**
```python
import rospy
from std_msgs.msg import String

rospy.init_node('my_sensor')  # requires roscore running
pub = rospy.Publisher('data', String, queue_size=10)
rate = rospy.Rate(1)
while not rospy.is_shutdown():
    pub.publish("sensor data")
    rate.sleep()
```

The ROS1 version is simpler for trivial cases, but lacks: health heartbeats, schema queryable, config loading, shutdown handling, and self-description — all of which Bubbaloop's SDK provides automatically.

---

## Summary

If you're on ROS1 because you're building a complex robot with navigation and manipulation, **migrate to ROS2** — you need the ecosystem.

If you're on ROS1 because you're running cameras and sensors on edge devices, **consider Bubbaloop** — it's designed for exactly this use case, with AI agents and MCP built in, and none of the complexity you don't need.
