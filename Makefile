LISTEN_ADDR ?= 127.0.0.1:8080

.PHONY: build release test lint fmt fmt-check run-stdio run-http help

build:
	cargo build

release:
	cargo build --release

test:
	cargo test --all --locked

lint:
	cargo clippy --locked -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

run-stdio:
	cargo run

run-http:
	cargo run -- --listen $(LISTEN_ADDR)

help:
	cargo run -- --help
