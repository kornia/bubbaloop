"""Zenoh client bridge - connects to Zenoh network, buffers topic data, queries daemon API."""

import asyncio
import json
import logging
import os
import re
import socket
import time
from collections import defaultdict, deque
from dataclasses import dataclass, field
from typing import Any

import zenoh

logger = logging.getLogger(__name__)

TOPIC_PATTERN = re.compile(r"^[a-zA-Z0-9/_\-\.]+$")


@dataclass
class TopicSample:
    """A single sample from a topic."""
    timestamp: float
    payload: bytes
    key: str


class ZenohBridge:
    """Zenoh client that subscribes to topics and buffers latest data."""

    def __init__(self, config: dict):
        self.endpoint = config.get("zenoh", {}).get("endpoint", "tcp/127.0.0.1:7447")
        self.scope = os.environ.get("BUBBALOOP_SCOPE", "local")
        self.machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", socket.gethostname())
        self.topic_mappings = config.get("topics", {})
        self.buffer_size = 10  # Keep latest N samples per topic

        self._session: zenoh.Session | None = None
        self._subscribers: dict[str, Any] = {}
        self._topic_buffer: dict[str, deque[TopicSample]] = defaultdict(
            lambda: deque(maxlen=self.buffer_size)
        )
        self._callbacks: dict[str, list] = defaultdict(list)

    @property
    def session(self) -> zenoh.Session:
        if self._session is None:
            raise RuntimeError("Zenoh session not open. Call open() first.")
        return self._session

    def open(self):
        """Open the Zenoh session."""
        zenoh_config = zenoh.Config()
        zenoh_config.insert_json5("connect/endpoints", json.dumps([self.endpoint]))
        zenoh_config.insert_json5("scouting/multicast/enabled", "false")
        zenoh_config.insert_json5("scouting/gossip/enabled", "false")

        self._session = zenoh.open(zenoh_config)
        logger.info(f"Zenoh connected to {self.endpoint} (scope={self.scope}, machine={self.machine_id})")

    def close(self):
        """Close the Zenoh session and all subscribers."""
        for sub in self._subscribers.values():
            sub.undeclare()
        self._subscribers.clear()
        if self._session is not None:
            self._session.close()
            self._session = None
        logger.info("Zenoh session closed")

    def scoped_topic(self, suffix: str) -> str:
        """Build a fully scoped topic from a suffix."""
        return f"bubbaloop/{self.scope}/{self.machine_id}/{suffix}"

    def subscribe(self, topic_suffix: str, callback=None):
        """Subscribe to a scoped topic and buffer incoming data."""
        if not TOPIC_PATTERN.match(topic_suffix):
            raise ValueError(f"Invalid topic name: {topic_suffix}")

        full_topic = self.scoped_topic(topic_suffix)
        if full_topic in self._subscribers:
            logger.debug(f"Already subscribed to {full_topic}")
            return

        if callback:
            self._callbacks[full_topic].append(callback)

        def _on_sample(sample):
            ts = TopicSample(
                timestamp=time.time(),
                payload=bytes(sample.payload),
                key=str(sample.key_expr),
            )
            self._topic_buffer[full_topic].append(ts)
            for cb in self._callbacks.get(full_topic, []):
                try:
                    cb(ts)
                except Exception as e:
                    logger.error(f"Callback error on {full_topic}: {e}")

        sub = self.session.declare_subscriber(full_topic, _on_sample)
        self._subscribers[full_topic] = sub
        logger.info(f"Subscribed to {full_topic}")

    def subscribe_pattern(self, pattern: str, callback=None):
        """Subscribe to a wildcard pattern (e.g., 'camera/*/compressed')."""
        full_pattern = self.scoped_topic(pattern)

        if callback:
            self._callbacks[full_pattern].append(callback)

        def _on_sample(sample):
            key = str(sample.key_expr)
            ts = TopicSample(
                timestamp=time.time(),
                payload=bytes(sample.payload),
                key=key,
            )
            self._topic_buffer[key].append(ts)
            for cb in self._callbacks.get(full_pattern, []):
                try:
                    cb(ts)
                except Exception as e:
                    logger.error(f"Callback error on {key}: {e}")

        sub = self.session.declare_subscriber(full_pattern, _on_sample)
        self._subscribers[full_pattern] = sub
        logger.info(f"Subscribed to pattern {full_pattern}")

    def get_latest(self, topic_suffix: str) -> TopicSample | None:
        """Get the latest sample from a topic buffer."""
        full_topic = self.scoped_topic(topic_suffix)
        buf = self._topic_buffer.get(full_topic)
        if buf and len(buf) > 0:
            return buf[-1]
        return None

    def get_latest_by_full_key(self, full_key: str) -> TopicSample | None:
        """Get the latest sample by full key expression."""
        buf = self._topic_buffer.get(full_key)
        if buf and len(buf) > 0:
            return buf[-1]
        return None

    def get_recent(self, topic_suffix: str, n: int = 5) -> list[TopicSample]:
        """Get the N most recent samples from a topic."""
        full_topic = self.scoped_topic(topic_suffix)
        buf = self._topic_buffer.get(full_topic)
        if buf:
            return list(buf)[-n:]
        return []

    def get_all_buffered_topics(self) -> dict[str, int]:
        """Get all topics with buffered data and their sample counts."""
        return {k: len(v) for k, v in self._topic_buffer.items() if len(v) > 0}

    async def query(self, key: str, payload: str | None = None, timeout_sec: float = 5.0) -> str:
        """Query a Zenoh key expression (for daemon API calls)."""
        try:
            if payload:
                replies = self.session.get(
                    key,
                    payload=payload.encode(),
                    timeout=timeout_sec,
                )
            else:
                replies = self.session.get(key, timeout=timeout_sec)

            results = []
            for reply in replies:
                if reply.ok:
                    results.append(bytes(reply.ok.payload).decode("utf-8", errors="replace"))
                else:
                    results.append(f"Error: {reply.err}")

            if not results:
                return "No response (timeout or no responders)"
            return "\n".join(results)
        except Exception as e:
            return f"Query failed: {e}"

    async def query_daemon(self, endpoint: str, payload: str | None = None) -> str:
        """Query the daemon API via Zenoh.

        The daemon uses: bubbaloop/{machine_id}/daemon/api/{endpoint}
        (no scope in daemon API paths).
        """
        key = f"bubbaloop/{self.machine_id}/daemon/api/{endpoint}"
        return await self.query(key, payload)

    def publish(self, topic_suffix: str, data: bytes):
        """Publish data to a scoped topic."""
        if not TOPIC_PATTERN.match(topic_suffix):
            raise ValueError(f"Invalid topic name: {topic_suffix}")
        full_topic = self.scoped_topic(topic_suffix)
        self.session.put(full_topic, data)

    def publish_health(self, node_name: str):
        """Publish a health heartbeat."""
        key = f"bubbaloop/{self.scope}/{self.machine_id}/health/{node_name}"
        self.session.put(key, node_name.encode())

    def decode_sample(self, sample: TopicSample, topic_suffix: str | None = None) -> dict | str:
        """Decode a topic sample to a human-readable format.

        Tries proto decoding based on topic mappings, falls back to string/JSON.
        """
        payload = sample.payload

        # Try JSON first (many messages are JSON-encoded)
        try:
            return json.loads(payload.decode("utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError):
            pass

        # Try proto decoding based on topic mapping
        if topic_suffix and topic_suffix in self.topic_mappings:
            proto_type = self.topic_mappings[topic_suffix]
            decoded = self._decode_proto(payload, proto_type)
            if decoded is not None:
                return decoded

        # Try to infer proto type from full key
        for suffix, proto_type in self.topic_mappings.items():
            if sample.key.endswith(suffix):
                decoded = self._decode_proto(payload, proto_type)
                if decoded is not None:
                    return decoded

        # Fall back to raw string
        try:
            return payload.decode("utf-8")
        except UnicodeDecodeError:
            return f"<binary data, {len(payload)} bytes>"

    def _decode_proto(self, payload: bytes, proto_type: str) -> dict | None:
        """Try to decode protobuf payload by type name."""
        try:
            # Import the generated pb2 modules dynamically
            import importlib
            for module_name in [
                "header_pb2", "daemon_pb2", "weather_pb2",
                "network_monitor_pb2", "agent_pb2",
                "system_telemetry_pb2",
            ]:
                try:
                    mod = importlib.import_module(module_name)
                    if hasattr(mod, proto_type):
                        msg_class = getattr(mod, proto_type)
                        msg = msg_class()
                        msg.ParseFromString(payload)
                        return self._proto_to_dict(msg)
                except ImportError:
                    continue
        except Exception as e:
            logger.debug(f"Proto decode failed for {proto_type}: {e}")
        return None

    def _proto_to_dict(self, msg) -> dict:
        """Convert a protobuf message to a dict."""
        from google.protobuf.json_format import MessageToDict
        return MessageToDict(msg, preserving_proto_field_name=True)
