# Build stage
FROM rust:1-slim-bookworm AS builder

WORKDIR /build

# Cache dependency compilation separately from application code
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY src ./src
# Touch main.rs so cargo knows to relink even if source mtimes are older
RUN touch src/main.rs \
    && cargo build --release --locked

# Runtime stage
FROM debian:trixie-slim

COPY --from=builder /build/target/release/gitlab-mcp /usr/local/bin/gitlab-mcp

ENTRYPOINT ["gitlab-mcp"]
