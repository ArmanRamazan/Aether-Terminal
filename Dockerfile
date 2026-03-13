# Stage 1: Build frontend
FROM node:20-alpine AS frontend
WORKDIR /app/crates/aether-web/frontend
COPY crates/aether-web/frontend/package*.json ./
RUN npm ci
COPY crates/aether-web/frontend/ ./
RUN npm run build

# Stage 2: Build Rust
FROM rust:1.82-bookworm AS builder
WORKDIR /app
COPY . .
COPY --from=frontend /app/crates/aether-web/frontend/dist crates/aether-web/frontend/dist
RUN cargo build --release -p aether-terminal

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/aether-terminal /usr/local/bin/
EXPOSE 8080 3000 50051
ENTRYPOINT ["aether-terminal"]
