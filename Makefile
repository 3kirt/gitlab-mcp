LISTEN_ADDR ?= 127.0.0.1:8080

# Clippy strictness (the pedantic, nursery, and cargo lint groups) is configured
# in Cargo.toml under [lints.clippy], so every `cargo clippy` — `make lint`,
# rust-analyzer, and the release gate — enforces it with no extra flags.

.PHONY: build release test lint secrets-scan fmt fmt-check audit check run-stdio run-http live-test help

build:
	cargo build

release:
	cargo build --release

test:
	cargo test --all --locked

# Strictest lint: pedantic/nursery/cargo come from Cargo.toml; this escalates
# every warning to an error across all targets (incl. tests) and both feature
# sets. This is the command the release gate runs.
lint: secrets-scan
	cargo clippy --all-targets --locked -- -D warnings
	cargo clippy --all-targets --features live-tests --locked -- -D warnings

# Same check as CI's "Scan for committed GitLab tokens" step, so a test
# fixture that looks like a PAT fails locally instead of after the push.
# Fixtures must use a placeholder (glpat-xxxxxxxxxx…) or a non-glpat value.
secrets-scan:
	@if git grep -nE 'glpat-[A-Za-z0-9_-]{10,}' -- . | grep -vE 'glpat-x{10,}'; then \
		echo "A GitLab personal access token appears to be committed"; exit 1; \
	fi

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

audit:
	cargo audit

# Full pre-release gate, mirroring the release process minus the live suite
# (which needs credentials — run `make live-test` with GITLAB_URL/GITLAB_TOKEN set).
check: fmt-check lint test release

# Live integration suite against a real GitLab instance. Requires GITLAB_URL and
# GITLAB_TOKEN (and optionally GITLAB_TEST_PROJECT) in the environment; runs
# serially since it creates and deletes real resources.
live-test:
	cargo test --features live-tests --locked live:: -- --test-threads=1 --nocapture

run-stdio:
	cargo run

run-http:
	cargo run -- --listen $(LISTEN_ADDR)

help:
	cargo run -- --help
