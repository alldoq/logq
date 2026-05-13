# syntax=docker/dockerfile:1.7

FROM rust:1.86-bookworm AS builder
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends \
      cmake \
    && rm -rf /var/lib/apt/lists/*

# Cache dependency build
COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && cargo build --release --locked && rm -rf src target/release/deps/logq*

COPY . .
RUN cargo build --release --locked && strip target/release/logq

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      openssh-client ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/logq /usr/local/bin/logq

# Expose the default port and bind 0.0.0.0 by default inside the container.
EXPOSE 7777
ENV LOGQ_HOST=0.0.0.0
WORKDIR /data
ENTRYPOINT ["logq"]
CMD ["--host", "0.0.0.0", "--port", "7777", "--no-open", "/data"]
