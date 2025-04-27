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

lint:
    @echo "Running format..."
    just format
    @echo "Running clippy..."
    just clippy
    @echo "Running check..."
    just check

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

start-pipeline NAME HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} pipeline start -n {{NAME}}

stop-pipeline NAME HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} pipeline stop -n {{NAME}}

list-pipelines HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} pipeline list

start-recording HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} recording start

stop-recording HOST="0.0.0.0" PORT="3000":
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} recording stop
