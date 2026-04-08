# Remote Access

Access the Bubbaloop dashboard from other machines on your network or over the internet.

## Overview

The dashboard supports remote access via:

- **HTTPS** — Self-signed certificate for secure connections
- **Single-port** — Zenoh WebSocket proxied through dashboard port
- **Multi-device** — Access from laptops, phones, tablets

## Quick Setup

### On the Server (Robot/Jetson)

```bash
# Start all services
pixi run up
```

### On the Client (Laptop/Phone)

Open in browser:

```
https://<server-ip>:5173
```

Accept the self-signed certificate warning.

## HTTPS Configuration

### Self-Signed Certificate

The dashboard automatically generates and uses a self-signed SSL certificate for HTTPS.

**Benefits:**

- Secure WebSocket connection required by browsers
- WebCodecs API requires secure context
- No certificate authority needed

**Limitations:**

- Browser shows security warning
- Must accept certificate manually
- Not suitable for public deployment

### Accepting the Certificate

1. Navigate to `https://<server-ip>:5173`
2. Browser shows "Your connection is not private"
3. Click "Advanced" or "Show Details"
4. Click "Proceed to \<ip\> (unsafe)" or "Visit this website"
5. Certificate is remembered for the session

## Network Architecture

### Single-Port Access

The Vite development server proxies WebSocket connections:

```
┌────────────────────────────────────────────────────────────┐
│                      Server                                 │
│                                                             │
│  ┌──────────────┐    ┌──────────────────────────────────┐  │
│  │  Dashboard   │    │  Zenoh Bridge                     │  │
│  │  :5173       │───▶│  :10001 (WebSocket)               │  │
│  │  (Vite)      │    │                                   │  │
│  └──────────────┘    └──────────────────────────────────┘  │
│         │                                                   │
│         │ HTTPS :5173                                       │
└─────────┼───────────────────────────────────────────────────┘
          │
          │
    ┌─────▼─────┐
    │  Client   │
    │  Browser  │
    └───────────┘
```

**Benefit:** Only one port (5173) needs to be accessible from the client.

## Distributed Setup

### Robot + Laptop Configuration

For accessing cameras from a laptop connected to a robot:

```
┌─────────────────────────────────────────────────────────────┐
│                    Server (Robot)                            │
│  ┌────────────────┐         ┌──────────────────────────┐    │
│  │  rtsp-camera   │──TCP───▶│  zenohd                  │    │
│  │  (-e :7447)    │  :7447  │  - tcp :7447 (router)    │    │
│  └────────────────┘         │  - ws :10001 (API)       │    │
│  ┌────────────────┐         └───────────┬──────────────┘    │
│  │  openmeteo     │──TCP────────────────┘                   │
│  └────────────────┘                     │                   │
└─────────────────────────────────────────┼───────────────────┘
                                          │ TCP :7447
┌─────────────────────────────────────────┼───────────────────┐
│                   Client (Laptop)       │                   │
│                           ┌─────────────▼──────────────┐    │
│  ┌───────────────┐   WS   │  zenohd                    │    │
│  │  Dashboard    │◀───────│  - connects to server:7447 │    │
│  │  Browser      │ :10001 │  - ws :10001 (local)       │    │
│  └───────────────┘        └────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### Server Setup

On the robot:

```bash
# Start Zenoh router
zenohd -c zenoh.json5

# Start camera node
bubbaloop node start rtsp-camera
```

**zenoh.json5:**

```json5
{
  mode: "router",
  listen: {
    endpoints: ["tcp/0.0.0.0:7447"],
  },
  plugins: {
    remote_api: {
      websocket_port: 10001,
    },
  },
}
```

### Client Setup

On the laptop:

```bash
# Configure server endpoint (one-time)
pixi run bubbaloop
# Use /server command to set robot IP

# Start local router
pixi run zenohd-client

# Start dashboard
pixi run dashboard
```

## Firewall Configuration

### Required Ports

| Port | Protocol | Direction | Purpose |
|------|----------|-----------|---------|
| 5173 | TCP | Inbound | Dashboard HTTPS |
| 7447 | TCP | Inbound | Zenoh router (if distributed) |
| 10001 | TCP | Inbound | WebSocket (if direct access) |

### UFW (Ubuntu)

```bash
# Dashboard only (single port)
sudo ufw allow 5173/tcp

# Distributed setup
sudo ufw allow 7447/tcp
sudo ufw allow 10001/tcp
```

### iptables

```bash
# Dashboard only
sudo iptables -A INPUT -p tcp --dport 5173 -j ACCEPT

# Distributed setup
sudo iptables -A INPUT -p tcp --dport 7447 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 10001 -j ACCEPT
```

## Mobile Access

### Browser Requirements

Mobile browsers that support WebCodecs:

| Browser | Platform | Support |
|---------|----------|---------|
| Chrome | Android | Yes |
| Safari | iOS 16.4+ | Yes |
| Edge | Android | Yes |

### Mobile Considerations

- Use WiFi for better performance
- Sub-streams recommended for lower bandwidth
- Dashboard is responsive for mobile screens
- Accept certificate on first visit

## Security Considerations

### Local Network

For local network access:

- Self-signed certificates are acceptable
- Ensure WiFi network is trusted
- Use strong router password

### Internet Access

For internet access (not recommended without additional security):

- Use a VPN instead of exposing ports
- Consider reverse proxy with proper certificates
- Implement authentication (not currently built-in)

### Best Practices

1. **Use VPN** for remote access over internet
2. **Limit exposure** — Only open necessary ports
3. **Update regularly** — Keep software up to date
4. **Monitor access** — Check logs for unauthorized access

## Troubleshooting

### Can't connect remotely

1. Verify server IP is correct
2. Check firewall allows port 5173
3. Ensure server is on same network or routable

### Certificate error

1. This is expected for self-signed certificates
2. Click through the browser warning
3. Certificate must be accepted on each browser/device

### WebSocket disconnected

1. Check Zenoh bridge is running
2. Verify proxy configuration in Vite
3. Check browser console for errors

### Slow performance

1. Check network bandwidth
2. Use sub-streams for cameras
3. Reduce number of simultaneous streams

## Next Steps

- [Dashboard Overview](index.md) — Dashboard features
- [Configuration](../getting-started/configuration.md) — Zenoh configuration
- [Troubleshooting](../reference/troubleshooting.md) — Common issues
