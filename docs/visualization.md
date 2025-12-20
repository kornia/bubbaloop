# 📊 Visualization

Bubbaloop streams video to [Foxglove Studio](https://foxglove.dev/studio) for real-time visualization.

## Foxglove Studio Setup

### Desktop App (Recommended)

1. Download from [foxglove.dev/download](https://foxglove.dev/download)
2. Install and launch

### Web App

Visit [app.foxglove.dev](https://app.foxglove.dev) (requires account)

## Connecting to Bubbaloop

### 1. Start Bubbaloop

```bash
cd bubbaloop
pixi run multicam
```

Wait for the server to start:

```
[INFO  foxglove::websocket::server] Started server on 0.0.0.0:8765
```

### 2. Connect Foxglove

1. Open Foxglove Studio
2. Click **Open connection** (or File → Open connection)
3. Select **Foxglove WebSocket**
4. Enter the URL:

```
ws://localhost:8765
```

!!! tip "Remote connection"
    If Bubbaloop runs on a different machine (e.g., Jetson), use its IP:
    ```
    ws://192.168.1.100:8765
    ```

5. Click **Open**

### 3. Add Video Panels

1. Click the **+** button to add a panel
2. Select **Image** panel
3. In the panel settings, select a topic:
   - `/camera/entrance/compressed`
   - `/camera/backyard/compressed`

Repeat for each camera you want to view.