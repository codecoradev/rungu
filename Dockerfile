# ── Stage 1: Build frontend ────────────────────────────────────────────
FROM node:22-slim AS frontend
WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ .
RUN npm run build

# ── Stage 2: Build static binary (musl) ───────────────────────────────
FROM rust:1.86-alpine AS builder
RUN apk add --no-cache musl-dev curl

WORKDIR /app

# Cache dependencies — copy only Cargo files first
COPY Cargo.toml Cargo.lock ./
COPY crates/rungu-proto/Cargo.toml crates/rungu-proto/Cargo.toml
COPY crates/rungu-core/Cargo.toml crates/rungu-core/Cargo.toml
COPY crates/rungu-auth/Cargo.toml crates/rungu-auth/Cargo.toml
COPY crates/rungu-api/Cargo.toml crates/rungu-api/Cargo.toml
COPY crates/rungu-mcp/Cargo.toml crates/rungu-mcp/Cargo.toml
COPY crates/rungud/Cargo.toml crates/rungud/Cargo.toml

# Create dummy source files for dependency caching
RUN mkdir -p crates/rungu-proto/src && echo "" > crates/rungu-proto/src/lib.rs && \
    mkdir -p crates/rungu-core/src && echo "" > crates/rungu-core/src/lib.rs && \
    mkdir -p crates/rungu-auth/src && echo "" > crates/rungu-auth/src/lib.rs && \
    mkdir -p crates/rungu-api/src && echo "" > crates/rungu-api/src/lib.rs && \
    mkdir -p crates/rungu-mcp/src && echo "" > crates/rungu-mcp/src/lib.rs && \
    mkdir -p crates/rungud/src && echo "fn main() {}" > crates/rungud/src/main.rs
RUN cargo build --release --bin rungu 2>/dev/null || true

# Copy real source and rebuild
COPY . .
COPY --from=frontend /app/web/build web/build
RUN touch crates/*/src/*.rs && cargo build --release --bin rungu

# ── Stage 3: Scratch runtime (zero OS overhead) ───────────────────────
FROM scratch

# Copy CA certs for HTTPS (reqwest needs this for OAuth calls)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

WORKDIR /app
COPY --from=builder /app/target/release/rungu /rungu
COPY --from=frontend /app/web/build /app/web/build

ENV RUNGU_LISTEN=0.0.0.0:3000
ENV RUST_LOG=rungu=info
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

VOLUME /data
EXPOSE 3000
ENTRYPOINT ["/rungu"]
CMD ["serve"]
