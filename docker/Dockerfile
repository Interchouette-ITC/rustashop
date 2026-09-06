# Multi-stage Actix API image. Build tools stay in the builder.
FROM rust:slim-trixie AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    binutils \
    ca-certificates \
    git \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY db ./db

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

RUN cargo build --release -p rustashop-api -p rustashop-persist-sqlx \
    && strip target/release/rustashop-api target/release/rustashop-migrate

FROM gcr.io/distroless/cc-debian13:nonroot AS runtime

WORKDIR /app

COPY --from=builder /src/target/release/rustashop-api /app/rustashop-api
COPY --from=builder /src/target/release/rustashop-migrate /app/rustashop-migrate
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY config /app/config

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV RUSTASHOP_BIND=0.0.0.0:8080
ENV RUSTASHOP_ROOT=/app

EXPOSE 8080

USER nonroot:nonroot

CMD ["/app/rustashop-api"]
