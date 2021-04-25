FROM rust:1.47 as builder

WORKDIR /build

RUN apt-get update && apt-get install -y liboping-dev && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef

COPY Cargo.toml .
COPY Cargo.lock .
COPY recipe.json .

# Prepare dependencies in release configuration for Layer Caching using the cargo-chef plugin
RUN cargo chef cook --release

COPY . .

RUN cargo build --release

FROM ubuntu:20.04 as runtime

RUN apt-get update && apt-get install -y ca-certificates tzdata liboping-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/rust/bin/

COPY --from=builder /build/target/release/rusty-cloudflare-dns-balancer /opt/rust/bin/rusty-cloudflare-dns-balancer

RUN chmod +x /opt/rust/bin/rusty-cloudflare-dns-balancer

ENTRYPOINT ["/opt/rust/bin/rusty-cloudflare-dns-balancer"]