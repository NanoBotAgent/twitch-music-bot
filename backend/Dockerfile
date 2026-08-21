# Build stage
FROM rust:1.79-slim-bookworm AS builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libpq-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/Cargo.toml
COPY shared/Cargo.toml shared/Cargo.toml

# Create dummy source files to cache dependencies
RUN mkdir -p backend/src shared/src && \
    echo "fn main() {}" > backend/src/main.rs && \
    echo "" > shared/src/lib.rs

# Build dependencies
RUN cargo build --release --workspace

# Copy actual source
COPY backend/src backend/src
COPY shared/src shared/src
COPY backend/migrations backend/migrations

# Build application
RUN cargo build --release --workspace

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    libpq5 \
    ca-certificates \
    curl \
    ffmpeg \
    yt-dlp \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/release/twitch-music-bot /app/twitch-music-bot

# Copy migrations
COPY --from=builder /app/backend/migrations /app/migrations

# Create non-root user
RUN useradd -r -s /bin/false -u 1000 appuser && \
    chown -R appuser:appuser /app

USER appuser

EXPOSE 8080 9090

ENV RUST_LOG=info
ENV CONFIG_PATH=/app/config

CMD ["/app/twitch-music-bot"]