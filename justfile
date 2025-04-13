@_default:
    just --list

build:
    cargo build --release

test:
    cargo test --release

format:
    cargo fmt

clippy:
    cargo clippy

check:
    cargo check

install_deps:
    ./scripts/install_deps.sh

help:
    cargo run --release --bin bubbaloop -- --help

serve HOST="0.0.0.0" PORT="3000" FEATURES="":
    RUST_LOG=debug cargo run --release --bin serve {{FEATURES}} -- -h {{HOST}} -p {{PORT}}

whoami HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} stats whoami

sysinfo HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} stats sysinfo

compute-mean-std HOST PORT PATH:
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} compute mean-std -i {{PATH}}

inference HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} inference

start-pipeline ID HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} pipeline start -i {{ID}}

stop-pipeline ID HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} pipeline stop -i {{ID}}

list-pipelines HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} pipeline list

config-pipeline HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} pipeline config