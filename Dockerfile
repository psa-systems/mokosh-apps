# Development Dockerfile - dx serve with hot reload
FROM rust:1-slim-trixie

RUN apt-get update && apt-get install --yes --no-install-recommends \
    pkg-config libssl-dev curl unzip \
    && rm -rf /var/lib/apt/lists/*

# WASM target
RUN rustup target add wasm32-unknown-unknown

# Install Bun (Tailwind v4 CSS)
RUN curl --location --silent --show-error --fail https://bun.sh/install \
    | env BUN_INSTALL=/usr/local bash

# Install dioxus-cli via pre-built binary
RUN curl --location --silent --show-error \
    https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-gnu.tgz \
    | tar --extract --gzip --directory /usr/local/cargo/bin
RUN cargo binstall dioxus-cli --no-confirm

RUN mkdir --parents /app /data /config

WORKDIR /app

# Copy manifests (dependency cache layer)
COPY Cargo.toml Cargo.lock ./
COPY package.json bun.lock ./

RUN bun install --frozen-lockfile

# Pre-build dependencies
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && echo "" > src/lib.rs \
    && cargo build --target wasm32-unknown-unknown \
    && rm -rf src

# Source code is mounted via volumes in compose

EXPOSE 4301

CMD ["sh", "-c", "bun x @tailwindcss/cli --input input.css --output assets/styles.css && dx serve --port 4301 --addr 0.0.0.0"]
