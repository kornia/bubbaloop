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

serve HOST PORT:
    RUST_LOG=debug cargo run --release --bin serve -- -h {{HOST}} -p {{PORT}}

whoami HOST PORT:
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} stats whoami

compute-mean-std HOST PORT PATH:
    RUST_LOG=info cargo run --release --bin bubbaloop -- -h {{HOST}} -p {{PORT}} compute mean-std -i {{PATH}}
