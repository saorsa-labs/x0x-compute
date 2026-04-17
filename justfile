set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Show available recipes
@default:
    just --list

fmt:
    cargo fmt --all

lint:
    cargo clippy --all-targets --all-features -- -D warnings -D clippy::panic -D clippy::unwrap_used -D clippy::expect_used

test:
    cargo nextest run --all-features

test-verbose:
    cargo nextest run --all-features --no-capture

build:
    cargo build --all-features

build-release:
    cargo build --release --all-features

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

quick-check: fmt lint test

check: fmt lint test build doc

clean:
    cargo clean
