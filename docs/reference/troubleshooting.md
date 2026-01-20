# Troubleshooting

Common issues and solutions for Bubbaloop.

## Installation Issues

### "pixi: command not found"

Pixi is not in your PATH.

**Solution:**

```bash
# Restart terminal or source shell config
source ~/.bashrc  # or ~/.zshrc
```

### GStreamer plugin errors

Missing GStreamer plugins.

**Solution:**

```bash
# Verify plugins
pixi run gst-inspect-1.0 rtspsrc
pixi run gst-inspect-1.0 h264parse

# Reinstall dependencies
pixi install --force
```

### Build failures

Compilation errors.

**Solution:**

```bash
# Clean build cache
pixi run cargo clean
pixi run build

# Check for missing system dependencies
pixi install
```

## Camera Issues

### Camera not connecting

RTSP connection fails.

**Checklist:**

1. Verify RTSP URL is correct
2. Test with VLC: `vlc rtsp://...`
3. Check firewall allows RTSP (port 554)
4. Verify credentials are correct
5. Ensure camera is on the same network

**Debug:**

```bash
RUST_LOG=debug pixi run cameras
```

### "Waiting for keyframe" in dashboard

Stream is running but video not displaying.

**Causes:**

- Waiting for H264 keyframe (IDR frame)
- Stream is H265/HEVC (not supported)
- Incorrect topic subscription

**Solutions:**

1. Wait a few seconds for the next keyframe
2. Verify camera streams H264 (not H265)
3. Check topic pattern is correct

### Low FPS / stuttering

Video is choppy or low frame rate.

**Checklist:**

1. Check network bandwidth
2. Use sub-stream instead of main stream
3. Reduce number of cameras
4. Increase `latency` in config

```yaml
cameras:
  - name: "camera"
    url: "rtsp://..."
    latency: 500  # Increase buffer
```

### High latency

Video is significantly delayed.

**Solutions:**

1. Decrease `latency` in camera config
2. Use wired connection instead of WiFi
3. Use sub-stream for lower bandwidth

```yaml
cameras:
  - name: "camera"
    url: "rtsp://.../stream2"  # Sub-stream
    latency: 100  # Lower buffer
```

## Dashboard Issues

### "WebSocket disconnected"

Dashboard can't connect to Zenoh bridge.

**Checklist:**

1. Verify bridge is running: `pixi run bridge`
2. Check bridge listening on port 10000
3. Try refreshing the page
4. Check browser console for errors

### "WebCodecs not supported"

Browser doesn't support H264 decoding.

**Solution:**

- Use Chrome 94+, Edge 94+, or Safari 16.4+
- Firefox is NOT supported
- Access via `localhost` or HTTPS

### Black screen / no video

Video element shows nothing.

**Checklist:**

1. Verify camera is connected (check `pixi run cameras` logs)
2. Verify topic is correct in panel settings
3. Check browser console for decoder errors
4. Try refreshing the page

### Certificate error (remote access)

Browser shows security warning.

**Solution:**

1. This is expected for self-signed certificates
2. Click "Advanced" → "Proceed to site"
3. Certificate is per-browser, repeat for each browser

## Zenoh Issues

### Can't connect to Zenoh router

Services can't connect to `tcp/127.0.0.1:7447`.

**Checklist:**

1. Start Zenoh router first: `zenohd -c zenoh.json5`
2. Or use `pixi run up` to start everything
3. Check port 7447 is not in use

### Topics not appearing

Services running but no data in dashboard.

**Debug steps:**

1. Check `/topics` in TUI
2. Verify services are publishing
3. Check Zenoh connectivity

```bash
pixi run bubbaloop
# /connect → /topics
```

### Remote connection fails

Can't connect to Zenoh on another machine.

**Checklist:**

1. Verify server IP is correct
2. Check firewall allows port 7447
3. Ensure router config listens on `0.0.0.0`:

```json5
{
  listen: {
    endpoints: ["tcp/0.0.0.0:7447"],  // Not 127.0.0.1
  },
}
```

## Weather Issues

### No weather data

OpenMeteo service not publishing.

**Checklist:**

1. Check internet connectivity
2. Verify service is running: `pixi run weather`
3. Check service logs for API errors
4. Verify Open-Meteo API is accessible

```bash
curl https://api.open-meteo.com/v1/forecast
```

### Wrong location

Weather data for incorrect location.

**Solutions:**

1. Set explicit coordinates in config
2. Disable `auto_discover`:

```yaml
location:
  auto_discover: false
  latitude: 41.4167
  longitude: 1.9667
```

## Performance Issues

### High CPU usage

System running slowly.

**Possible causes:**

- Too many cameras
- Main streams instead of sub-streams
- Debug logging enabled

**Solutions:**

1. Use sub-streams (lower resolution)
2. Reduce number of cameras
3. Use `RUST_LOG=info` (not debug)

### High memory usage

Memory consumption growing.

**Possible causes:**

- Memory leak in long-running session
- Too many cameras
- Large video buffers

**Solutions:**

1. Restart services periodically
2. Reduce camera count
3. Lower `latency` values

## Browser Console Errors

### "Failed to execute 'decode' on 'VideoDecoder'"

WebCodecs decoder error.

**Causes:**

- Invalid H264 data
- Missing SPS/PPS headers
- Stream corruption

**Solutions:**

1. Refresh the page
2. Wait for next keyframe
3. Check camera is outputting valid H264

### "WebSocket connection failed"

Can't establish WebSocket.

**Causes:**

- Bridge not running
- Proxy misconfiguration
- Network issues

**Solutions:**

1. Verify `pixi run bridge` is running
2. Check port 10000 is accessible
3. Check browser dev tools Network tab

## Logging

### Enable debug logging

```bash
RUST_LOG=debug pixi run cameras
RUST_LOG=debug pixi run weather
```

### View specific module logs

```bash
RUST_LOG=bubbaloop::h264_capture=trace pixi run cameras
```

### Browser console

1. Open browser Developer Tools (F12)
2. Go to Console tab
3. Look for errors and warnings

## Getting Help

If issues persist:

1. Check [GitHub Issues](https://github.com/kornia/bubbaloop/issues)
2. Join [Discord](https://discord.com/invite/HfnywwpBnD)
3. Include:
   - OS and version
   - Browser and version
   - Error messages
   - Steps to reproduce

## Next Steps

- [CLI Commands](cli.md) — Command reference
- [Configuration](../getting-started/configuration.md) — Configuration options
- [Architecture](../concepts/architecture.md) — System design
