# Development Dockerfile - dx serve with hot reload
FROM ghcr.io/niceguyit/rust-builder-glibc:v1.0.1-rust1.94-trixie

ARG UID=1000
ARG GID=1000

RUN apt-get update && apt-get install --yes --no-install-recommends \
    pkg-config libssl-dev curl unzip \
    && rm -rf /var/lib/apt/lists/*

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

# PMS-884: `--features web` for the same reason the release image passes it -
# dx substitutes its own feature list for this crate's defaults, so the
# renderer is named here rather than left to dx's platform-name heuristic.
CMD ["sh", "-c", "bun x @tailwindcss/cli --input input.css --output assets/styles.css && dx serve --features web --port 4301 --addr 0.0.0.0"]
