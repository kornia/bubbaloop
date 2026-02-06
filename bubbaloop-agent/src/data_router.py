"""Data router - captures data from Zenoh topics and writes to files."""

import csv
import io
import json
import logging
import os
import time
import uuid
from dataclasses import dataclass, field
from pathlib import Path

logger = logging.getLogger(__name__)


@dataclass
class Capture:
    """An active data capture from a topic to files."""
    id: str
    topic: str
    output_path: str
    format: str
    max_files: int
    started_at: float = field(default_factory=time.time)
    files_written: int = 0
    bytes_written: int = 0
    samples_received: int = 0
    active: bool = True

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "topic": self.topic,
            "output_path": self.output_path,
            "format": self.format,
            "max_files": self.max_files,
            "started_at": self.started_at,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "Capture":
        return cls(
            id=data["id"],
            topic=data["topic"],
            output_path=data["output_path"],
            format=data.get("format", "json"),
            max_files=data.get("max_files", 0),
            started_at=data.get("started_at", time.time()),
        )


class DataRouter:
    """Routes data from Zenoh topics to file storage."""

    def __init__(self, zenoh_bridge, config: dict, data_dir: Path):
        self.zenoh = zenoh_bridge
        self.config = config
        self.data_dir = data_dir
        self.captures_file = data_dir / "captures.json"

        self.captures: dict[str, Capture] = {}
        self._csv_writers: dict[str, tuple] = {}  # capture_id -> (file, writer)

        allowed = config.get("safety", {}).get("allowed_data_paths", ["/data/", "/tmp/bubbaloop/"])
        self._allowed_paths = [os.path.realpath(p) for p in allowed]

        self._load_captures()

    def _load_captures(self):
        """Load persisted captures."""
        if self.captures_file.exists():
            try:
                data = json.loads(self.captures_file.read_text())
                for c_data in data:
                    capture = Capture.from_dict(c_data)
                    self.captures[capture.id] = capture
                    logger.info(f"Loaded capture: {capture.id} ({capture.topic})")
            except (json.JSONDecodeError, KeyError) as e:
                logger.error(f"Failed to load captures: {e}")

    def _save_captures(self):
        """Save captures to persistence file."""
        data = [c.to_dict() for c in self.captures.values() if c.active]
        self.captures_file.write_text(json.dumps(data, indent=2))

    def _validate_path(self, path: str) -> bool:
        """Check if the output path is within allowed directories."""
        real_path = os.path.realpath(path)
        return any(real_path.startswith(allowed) for allowed in self._allowed_paths)

    async def start_capture(
        self,
        topic: str,
        output_path: str,
        format: str = "json",
        max_files: int = 0,
    ) -> str:
        """Start capturing data from a topic to files."""
        # Validate path
        if not self._validate_path(output_path):
            return f"Error: Path '{output_path}' is not in allowed data paths: {self._allowed_paths}"

        if format not in ("json", "csv", "raw", "h264"):
            return f"Error: Unknown format '{format}'. Use: json, csv, raw, h264."

        # Resolve and reject path traversal
        real_path = os.path.realpath(output_path)
        if ".." in output_path:
            return "Error: Path traversal not allowed."

        # Create output directory
        os.makedirs(real_path, exist_ok=True)

        capture_id = f"cap-{uuid.uuid4().hex[:8]}"
        capture = Capture(
            id=capture_id,
            topic=topic,
            output_path=real_path,
            format=format,
            max_files=max_files,
        )
        self.captures[capture_id] = capture
        self._save_captures()

        # Subscribe to topic with capture callback
        self.zenoh.subscribe(topic, callback=lambda sample: self._on_sample(capture_id, sample))

        logger.info(f"Started capture {capture_id}: {topic} -> {real_path} ({format})")
        return (
            f"Capture started.\n"
            f"  ID: {capture_id}\n"
            f"  Topic: {topic}\n"
            f"  Output: {real_path}\n"
            f"  Format: {format}"
        )

    async def stop_capture(self, capture_id: str) -> str:
        """Stop a data capture."""
        capture = self.captures.get(capture_id)
        if not capture:
            return f"Capture '{capture_id}' not found."

        capture.active = False
        self._save_captures()

        # Clean up CSV writer if exists
        if capture_id in self._csv_writers:
            fh, _ = self._csv_writers.pop(capture_id)
            fh.close()

        duration = time.time() - capture.started_at
        logger.info(f"Stopped capture {capture_id}")
        return (
            f"Capture '{capture_id}' stopped.\n"
            f"  Duration: {duration:.0f}s\n"
            f"  Files written: {capture.files_written}\n"
            f"  Bytes written: {capture.bytes_written}\n"
            f"  Samples: {capture.samples_received}"
        )

    def _on_sample(self, capture_id: str, sample):
        """Handle an incoming data sample for a capture."""
        capture = self.captures.get(capture_id)
        if not capture or not capture.active:
            return

        capture.samples_received += 1

        try:
            if capture.format == "json":
                self._write_json(capture, sample)
            elif capture.format == "csv":
                self._write_csv(capture, sample)
            elif capture.format == "raw":
                self._write_raw(capture, sample)
            elif capture.format == "h264":
                self._write_raw(capture, sample)  # h264 is raw binary
        except Exception as e:
            logger.error(f"Capture {capture_id} write error: {e}")

    def _write_json(self, capture: Capture, sample):
        """Write a sample as a JSON line."""
        decoded = self.zenoh.decode_sample(sample, capture.topic)
        record = {
            "timestamp": sample.timestamp,
            "key": sample.key,
            "data": decoded,
        }

        filepath = os.path.join(capture.output_path, "data.jsonl")
        with open(filepath, "a") as f:
            f.write(json.dumps(record, default=str) + "\n")

        capture.files_written = 1  # Single file, growing
        capture.bytes_written += len(json.dumps(record, default=str)) + 1

        self._enforce_max_files(capture)

    def _write_csv(self, capture: Capture, sample):
        """Write a sample as a CSV row."""
        decoded = self.zenoh.decode_sample(sample, capture.topic)

        filepath = os.path.join(capture.output_path, "data.csv")

        if capture.id not in self._csv_writers:
            fh = open(filepath, "a", newline="")
            writer = csv.writer(fh)
            # Write header if file is empty
            if os.path.getsize(filepath) == 0 and isinstance(decoded, dict):
                writer.writerow(["timestamp", "key"] + list(decoded.keys()))
            self._csv_writers[capture.id] = (fh, writer)

        fh, writer = self._csv_writers[capture.id]

        if isinstance(decoded, dict):
            writer.writerow([sample.timestamp, sample.key] + list(decoded.values()))
        else:
            writer.writerow([sample.timestamp, sample.key, str(decoded)])

        fh.flush()
        capture.files_written = 1
        capture.bytes_written = os.path.getsize(filepath)

    def _write_raw(self, capture: Capture, sample):
        """Write raw binary data with sequence number."""
        seq = capture.samples_received
        filepath = os.path.join(capture.output_path, f"{seq:08d}.bin")

        with open(filepath, "wb") as f:
            f.write(sample.payload)

        capture.files_written += 1
        capture.bytes_written += len(sample.payload)
        self._enforce_max_files(capture)

    def _enforce_max_files(self, capture: Capture):
        """Remove oldest files if max_files limit is exceeded."""
        if capture.max_files <= 0:
            return

        output = Path(capture.output_path)
        files = sorted(output.iterdir(), key=lambda f: f.stat().st_mtime)
        while len(files) > capture.max_files:
            oldest = files.pop(0)
            oldest.unlink()
            capture.files_written -= 1

    def describe_all(self) -> str:
        """Describe all active captures (for system prompt)."""
        active = [c for c in self.captures.values() if c.active]
        if not active:
            return "No active captures."

        lines = []
        for c in active:
            duration = time.time() - c.started_at
            lines.append(
                f"- [{c.id}] {c.topic} -> {c.output_path} "
                f"({c.format}, {c.samples_received} samples, {duration:.0f}s)"
            )
        return "\n".join(lines)

    def start_persisted_captures(self):
        """Re-subscribe to topics for persisted captures."""
        for capture in self.captures.values():
            if capture.active:
                self.zenoh.subscribe(
                    capture.topic,
                    callback=lambda sample, cid=capture.id: self._on_sample(cid, sample),
                )
                logger.info(f"Resumed capture {capture.id}: {capture.topic}")
