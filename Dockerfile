# ---- Build Stage ----
FROM rustlang/rust:nightly AS builder
WORKDIR /app
COPY blockchain ./blockchain
COPY libs ./libs
WORKDIR /app/blockchain
RUN cargo +nightly build --release

# ---- Runtime Stage ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/local/bin
COPY --from=builder /app/blockchain/target/release/blockchain /usr/local/bin/blockchain

ENTRYPOINT ["/usr/local/bin/blockchain"]
CMD []
