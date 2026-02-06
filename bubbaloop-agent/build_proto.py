#!/usr/bin/env python3
"""Compile protobuf definitions into Python modules."""

import subprocess
import sys
from pathlib import Path

# Try bubbaloop-schemas first, fall back to local protos
BUBBALOOP_PROTO_DIR = (
    Path(__file__).parent.parent
    / "bubbaloop"
    / "crates"
    / "bubbaloop-schemas"
    / "protos"
)
LOCAL_PROTO_DIR = Path(__file__).parent / "protos"
OUTPUT_DIR = Path(__file__).parent


def main():
    # Collect proto files from both locations
    proto_dirs = []
    proto_files = []

    if BUBBALOOP_PROTO_DIR.exists():
        proto_dirs.append(BUBBALOOP_PROTO_DIR)
        proto_files.extend(BUBBALOOP_PROTO_DIR.glob("*.proto"))
        print(f"Found bubbaloop-schemas protos at {BUBBALOOP_PROTO_DIR}")

    if LOCAL_PROTO_DIR.exists():
        proto_dirs.append(LOCAL_PROTO_DIR)
        for f in LOCAL_PROTO_DIR.glob("*.proto"):
            # Don't duplicate files already found in bubbaloop-schemas
            if not any(f.name == existing.name for existing in proto_files):
                proto_files.append(f)
        print(f"Found local protos at {LOCAL_PROTO_DIR}")

    if not proto_files:
        print("ERROR: No .proto files found", file=sys.stderr)
        sys.exit(1)

    print(f"Compiling {len(proto_files)} proto files")

    # Build protoc command with all proto paths
    cmd = [
        sys.executable,
        "-m",
        "grpc_tools.protoc",
        f"--python_out={OUTPUT_DIR}",
    ]
    for d in proto_dirs:
        cmd.append(f"--proto_path={d}")
    cmd.extend(str(f) for f in proto_files)

    result = subprocess.run(cmd, capture_output=True, text=True)

    if result.returncode != 0:
        print(f"ERROR: protoc failed:\n{result.stderr}", file=sys.stderr)
        sys.exit(1)

    for proto in proto_files:
        pb2_file = OUTPUT_DIR / f"{proto.stem}_pb2.py"
        if pb2_file.exists():
            print(f"  Generated: {pb2_file.name}")
        else:
            print(f"  WARNING: Expected {pb2_file.name} not found", file=sys.stderr)

    print("Proto compilation complete.")


if __name__ == "__main__":
    main()
