#FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:edge-centos
FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:0.2.5
#FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu@sha256:b4eff900bf2007cbcb54335a5826dedde6082f484bc8be7499d5ed071608ecf3

RUN apt-get update && apt-get install --assume-yes \
    cmake \
    curl \
    libglib2.0-dev \
    pkg-config \
    && \
    apt-get clean

RUN dpkg --add-architecture arm64

RUN apt-get update && apt-get install --assume-yes \
    nasm \
    libgstreamer1.0-dev:arm64 \
    libgstreamer-plugins-base1.0-dev:arm64 \
    libssl-dev:arm64 \
    libglib2.0-dev:arm64 \
    libudev-dev:arm64 \
    && \
    apt-get clean
