# Mokosh Platform client - Dioxus WASM web bundle
# Build stage (trixie for glibc 2.41, required by pre-built dioxus-cli binary)
FROM rust:1.93-slim-trixie AS builder

WORKDIR /build

RUN apt-get update && apt-get install --yes --no-install-recommends \
    curl pkg-config libssl-dev unzip \
    && rm -rf /var/lib/apt/lists/*

# Install Bun (for Tailwind v4 CSS build)
RUN curl --location --silent --show-error --fail https://bun.sh/install \
    | env BUN_INSTALL=/usr/local bash

# Install cargo-binstall, then use it to fetch pre-built dioxus-cli
RUN curl --location --silent --show-error \
    https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-gnu.tgz \
    | tar --extract --gzip --directory /usr/local/cargo/bin
RUN cargo binstall dioxus-cli --no-confirm

# Install WASM target for client build
RUN rustup target add wasm32-unknown-unknown

# --- Dependency caching (no source) ---
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && echo "" > src/lib.rs \
    && cargo build --release --target wasm32-unknown-unknown \
    && rm -rf src

# --- Tailwind CSS build ---
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

# --- Full source ---
COPY . .

RUN bun x @tailwindcss/cli --input input.css --output assets/styles.css --minify

# Build fullstack client (WASM bundle + static assets)
RUN dx build --release

# Runtime stage - serve static assets via Caddy
FROM alpine:3.21

# Copy Caddy from official image
COPY --from=caddy:alpine /usr/bin/caddy /usr/bin/caddy

# Copy static bundle
COPY --from=builder /build/target/dx/mokosh-client/release/web/public /usr/share/caddy

# Default Caddyfile (override via volume mount)
COPY oci-build/Caddyfile /etc/caddy/Caddyfile

RUN adduser --disabled-password --uid 1001 appuser \
    && chown --recursive appuser:appuser /usr/share/caddy
USER appuser

ENV PORT=8080
EXPOSE 8080

LABEL org.opencontainers.image.source=https://dev.a8n.run/a8n-tools/mokosh-clients
LABEL org.opencontainers.image.description="Mokosh Platform Dioxus client"

CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile"]
