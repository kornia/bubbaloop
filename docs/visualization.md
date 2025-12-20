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

## Panel Layout

### Single Camera

Use a single Image panel fullscreen for monitoring one camera.

### Multi-Camera Grid

1. Split the layout into multiple panes (drag panel edges)
2. Add an Image panel to each pane
3. Assign different camera topics to each panel

Example 2x2 layout:

```
┌─────────────────┬─────────────────┐
│   entrance      │    backyard     │
│                 │                 │
├─────────────────┼─────────────────┤
│    garage       │    driveway     │
│                 │                 │
└─────────────────┴─────────────────┘
```

## Topic Reference

Each camera publishes to a topic based on its configured name:

| Camera Name | Topic |
|-------------|-------|
| `entrance` | `/camera/entrance/compressed` |
| `backyard` | `/camera/backyard/compressed` |
| `garage` | `/camera/garage/compressed` |

Message type: `foxglove.CompressedVideo`

## Troubleshooting

### No video appears

1. **Check connection**: Look for "Connected" status in Foxglove
2. **Verify topics**: Open the Topics panel to see available topics
3. **Check logs**: Look for "publishing frame" messages in Bubbaloop output

### Connection refused

- Ensure Bubbaloop is running
- Check firewall allows port 8765
- Verify the IP address is correct

### Video stuttering

- Increase `latency` in camera config
- Use sub-stream (`stream2`) instead of main stream
- Check network bandwidth

### Blank/black video

- The Image panel may not decode H264 directly
- Try the **Raw Image** panel type
- Verify camera is streaming (check with VLC first)

## Saving Layouts

1. Configure your preferred panel layout
2. File → Save layout as...
3. Give it a name like "4-camera-grid"

Load saved layouts from File → Open layout.

## Recording

Foxglove can record sessions to MCAP files:

1. Click the record button (red circle)
2. Perform your session
3. Click stop
4. Save the MCAP file

Replay recordings anytime for review.

