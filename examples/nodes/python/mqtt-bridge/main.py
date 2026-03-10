#!/usr/bin/env python3
"""mqtt-bridge node — subscribes to MQTT topics and republishes messages to Zenoh."""

# Real MQTT usage (requires: pip install paho-mqtt):
#
#   import paho.mqtt.client as mqtt
#
#   def on_connect(client, userdata, flags, rc):
#       log.info("Connected to MQTT broker (rc=%d)", rc)
#       client.subscribe(userdata["mqtt_topic"])
#
#   def on_message(client, userdata, msg):
#       payload_str = msg.payload.decode("utf-8", errors="replace")
#       try:
#           payload = json.loads(payload_str)
#       except json.JSONDecodeError:
#           payload = payload_str
#       envelope = {
#           "topic": msg.topic,
#           "payload": payload,
#           "timestamp_ms": int(time.time() * 1000),
#       }
#       userdata["zenoh_pub"].put(json.dumps(envelope).encode())
#
#   client = mqtt.Client(userdata={"mqtt_topic": mqtt_topic, "zenoh_pub": data_pub})
#   client.on_connect = on_connect
#   client.on_message = on_message
#   client.connect(broker_host, broker_port)
#   client.loop_start()
#   # ... then spin until shutdown ...
#   client.loop_stop()
#   client.disconnect()

import argparse
import json
import logging
import math
import os
import signal
import time

import yaml

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger("mqtt-bridge")


def load_config(path: str) -> dict:
    with open(path) as f:
        return yaml.safe_load(f)


_MOCK_ROOMS = ["living_room", "bedroom", "kitchen", "bathroom", "office"]
_MOCK_DEVICES = ["thermostat", "sensor_01", "sensor_02", "weather_station"]


def mock_mqtt_message(seq: int, base_topic: str) -> dict:
    """Generate a simulated MQTT message envelope."""
    room = _MOCK_ROOMS[seq % len(_MOCK_ROOMS)]
    device = _MOCK_DEVICES[seq % len(_MOCK_DEVICES)]

    # Replace MQTT wildcards (+, #) with concrete values for mock topics.
    concrete_topic = base_topic.replace("+", room).replace("#", f"{device}/temperature")

    temperature = 20.0 + 5.0 * math.sin(seq * 0.15) + (seq % 3) * 0.3
    humidity = 50.0 + 10.0 * math.cos(seq * 0.1)

    payload = {
        "temperature": round(temperature, 2),
        "humidity": round(humidity, 2),
        "unit": "C",
        "device": device,
    }
    return {
        "topic": concrete_topic,
        "payload": payload,
        "timestamp_ms": int(time.time() * 1000),
        "_mock": True,
    }


def start_real_mqtt(broker_host: str, broker_port: int, mqtt_topic: str, zenoh_pub):
    """Start a real paho-mqtt client. Returns the client object."""
    try:
        import paho.mqtt.client as mqtt  # type: ignore[import]
    except ImportError:
        raise ImportError(
            "paho-mqtt not installed. Run: pip install paho-mqtt\n"
            "Or use broker=mock in config for simulation mode."
        )

    def on_connect(client, userdata, flags, rc):
        if rc == 0:
            log.info("Connected to MQTT broker %s:%d", broker_host, broker_port)
            client.subscribe(userdata["mqtt_topic"])
            log.info("Subscribed to MQTT topic: %s", userdata["mqtt_topic"])
        else:
            log.error("MQTT connection refused (rc=%d)", rc)

    def on_disconnect(client, userdata, rc):
        if rc != 0:
            log.warning("Unexpected MQTT disconnect (rc=%d), will reconnect", rc)

    def on_message(client, userdata, msg):
        payload_str = msg.payload.decode("utf-8", errors="replace")
        try:
            payload = json.loads(payload_str)
        except json.JSONDecodeError:
            payload = payload_str
        envelope = {
            "topic": msg.topic,
            "payload": payload,
            "timestamp_ms": int(time.time() * 1000),
        }
        encoded = json.dumps(envelope).encode()
        userdata["zenoh_pub"].put(encoded)
        log.debug("Forwarded MQTT %s (%d bytes)", msg.topic, len(encoded))

    userdata = {"mqtt_topic": mqtt_topic, "zenoh_pub": zenoh_pub}
    client = mqtt.Client(userdata=userdata)
    client.on_connect = on_connect
    client.on_disconnect = on_disconnect
    client.on_message = on_message
    client.connect(broker_host, broker_port, keepalive=60)
    client.loop_start()
    return client


def main():
    parser = argparse.ArgumentParser(description="MQTT to Zenoh bridge node")
    parser.add_argument("-c", "--config", default="config.yaml", help="Config file path")
    parser.add_argument(
        "-e",
        "--endpoint",
        default=os.environ.get("ZENOH_ENDPOINT", "tcp/127.0.0.1:7447"),
        help="Zenoh endpoint (default: tcp/127.0.0.1:7447)",
    )
    args = parser.parse_args()

    config = load_config(args.config)
    name = config.get("name", "mqtt-bridge")
    broker = config.get("broker", "mock")
    broker_port = int(config.get("broker_port", 1883))
    mqtt_topic = config.get("mqtt_topic", "home/+/temperature")
    interval_secs = float(config.get("interval_secs", 5.0))
    scope = os.environ.get("BUBBALOOP_SCOPE", "local")
    machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", "localhost").replace("-", "_")
    mock_mode = broker == "mock"

    import zenoh

    conf = zenoh.Config()
    conf.insert_json5("mode", '"client"')
    conf.insert_json5("connect/endpoints", json.dumps([args.endpoint]))
    session = zenoh.open(conf)

    data_topic = f"bubbaloop/{scope}/{machine_id}/{name}/data"
    health_topic = f"bubbaloop/{scope}/{machine_id}/health/{name}"

    data_pub = session.declare_publisher(data_topic)
    health_pub = session.declare_publisher(health_topic)

    log.info("mqtt-bridge started (broker=%s, mqtt_topic=%s)", "mock" if mock_mode else f"{broker}:{broker_port}", mqtt_topic)
    log.info("Publishing to: %s", data_topic)

    running = True
    last_health = 0.0
    seq = 0
    mqtt_client = None

    def handle_signal(signum, frame):
        nonlocal running
        log.info("Shutdown signal received")
        running = False

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    if not mock_mode:
        try:
            mqtt_client = start_real_mqtt(broker, broker_port, mqtt_topic, data_pub)
        except Exception as e:
            log.error("Failed to start MQTT client: %s", e)
            session.close()
            import sys
            sys.exit(1)

    while running:
        tick_start = time.monotonic()

        # Publish health every 5 seconds.
        if tick_start - last_health >= 5.0:
            health_pub.put(b"ok")
            last_health = tick_start

        if mock_mode:
            envelope = mock_mqtt_message(seq, mqtt_topic)
            encoded = json.dumps(envelope).encode()
            data_pub.put(encoded)
            log.info("Mock MQTT [%s] payload=%s", envelope["topic"], json.dumps(envelope["payload"]))
            seq += 1

            elapsed = time.monotonic() - tick_start
            sleep_time = max(0.0, interval_secs - elapsed)
            if sleep_time > 0:
                time.sleep(sleep_time)
        else:
            # Real mode: paho loop_start() handles incoming messages in a background thread.
            # We just sleep and keep the health heartbeat alive.
            time.sleep(1.0)

    log.info("mqtt-bridge stopping")
    if mqtt_client is not None:
        mqtt_client.loop_stop()
        mqtt_client.disconnect()
    data_pub.undeclare()
    health_pub.undeclare()
    session.close()
    log.info("mqtt-bridge stopped")


if __name__ == "__main__":
    main()
