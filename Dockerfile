# Development Dockerfile - dx serve with hot reload
FROM ghcr.io/niceguyit/rust-builder-glibc:v1.0.0-rust1.94-trixie

ARG UID=1000
ARG GID=1000

RUN apt-get update && apt-get install --yes --no-install-recommends \
    pkg-config libssl-dev curl unzip \
    && rm -rf /var/lib/apt/lists/*

# WASM target

# Install Bun (Tailwind v4 CSS)
RUN curl --location --silent --show-error --fail https://bun.sh/install \
    | env BUN_INSTALL=/usr/local bash

# Install dioxus-cli via pre-built binary
RUN curl --location --silent --show-error \
    https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-gnu.tgz \
    | tar --extract --gzip --directory /usr/local/cargo/bin

# Create non-root user matching host UID/GID so bind-mounted files stay host-owned
RUN groupadd --gid ${GID} dev \
    && useradd --uid ${UID} --gid ${GID} --create-home --shell /bin/bash dev \
    && mkdir --parents /app /data /config \
    && chown --recursive ${UID}:${GID} /app /data /config /usr/local/cargo

USER dev

WORKDIR /app

# Copy manifests (dependency cache layer)
COPY --chown=${UID}:${GID} Cargo.toml Cargo.lock ./
COPY --chown=${UID}:${GID} package.json bun.lock ./

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
