# ---- Build Stage ----
FROM rustlang/rust:nightly AS builder
WORKDIR /app
COPY pillar ./pillar
COPY libs ./libs
WORKDIR /app/pillar
RUN cargo +nightly build --release

# ---- Runtime Stage ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/local/bin
COPY --from=builder /app/pillar/target/release/pillar /usr/local/bin/pillar

ENTRYPOINT ["/usr/local/bin/pillar"]
CMD []
