---
description: Get the Zenoh topic and connection params for a node's data stream
argument-hint: <node-name>
---

Call `get_stream_info(node_name="$ARGUMENTS")` on the `bubbaloop` MCP server.

Report the result as:

```
topic:    <zenoh_topic>
encoding: <encoding>
endpoint: <endpoint>
```

Then provide a copy-pasteable subscriber snippet for both Python (`zenoh-python`) and Rust (`zenoh` crate) using the returned topic. Remind the user that streaming data MUST go through Zenoh, not repeated MCP calls (rate limit: 100 burst / 1 rps).
