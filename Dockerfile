FROM rust:latest AS builder

WORKDIR /app

# only build dependencies to leverage docker cache
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release --bin rust-arb-bot
RUN rm -rf src

COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/rust-arb-bot .
COPY --from=builder /app/config/ ./config/

CMD ["./rust-arb-bot"] 