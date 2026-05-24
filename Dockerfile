FROM rust:1-slim AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock* ./
COPY src ./src

RUN cargo build --release

FROM debian:stable-slim

WORKDIR /app

COPY --from=builder /app/target/release/vps-rust /usr/local/bin/vps-rust

VOLUME ["/app/docs", "/app/keys"]

EXPOSE 8000

CMD ["vps-rust"]
