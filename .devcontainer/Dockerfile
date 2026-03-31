# Bubbaloop dev container — Linux aarch64 / x86_64
# Matches the Linux target environment (Jetson, Raspberry Pi, Ubuntu server)
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV LANG=C.UTF-8

# System dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Build essentials
    build-essential \
    pkg-config \
    cmake \
    curl \
    git \
    unzip \
    # Protobuf compiler (required by prost-build)
    protobuf-compiler \
    # GStreamer (camera/video pipeline)
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    # zlib (required by various crates)
    zlib1g-dev \
    # D-Bus (for systemd integration — available on Linux unlike macOS)
    libdbus-1-dev \
    dbus \
    # Debugging
    lldb \
    # Utilities
    ca-certificates \
    openssh-client \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable

# Pixi package manager (used by project tasks and environments)
RUN curl -fsSL https://pixi.sh/install.sh | sh

ENV PATH="/root/.pixi/bin:/root/.cargo/bin:${PATH}"

# Node.js 20 (for dashboard)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

CMD ["sleep", "infinity"]
