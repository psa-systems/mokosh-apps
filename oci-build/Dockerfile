# Mokosh Platform client - Dioxus WASM web bundle.
# Base image bundles Rust 1.93 + WASM target + bun + cargo-binstall + dioxus-cli
# (pinned). See niceguyit/oci-images/rust-builder.
#
# BUILDPLATFORM pins the builder stage to the runner's native arch.
# The WASM bundle it produces is arch-agnostic, so it is built once
# and copied into per-arch runtime stages. Building cargo under arm64
# emulation would be slow and gains nothing for a WASM artifact.
FROM --platform=$BUILDPLATFORM ghcr.io/niceguyit/rust-builder-glibc:v1.0.0-rust1.94-trixie AS builder

WORKDIR /build

# Cap concurrent rustc / build-script processes. CI sets this to the
# full host nproc since this is the only build on the runner. The
# default keeps ad-hoc local builds from oversubscribing a small
# laptop. Overridable via `--build-arg CARGO_BUILD_JOBS=N`.
ARG CARGO_BUILD_JOBS=2
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
RUN echo "build parallelism: CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS} (host nproc=$(nproc))"

# --- Dependency caching (no source) ---
COPY Cargo.toml Cargo.lock ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    mkdir src \
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
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    dx build --release

# Caddy binary, declared as its own stage so the Caddy image is
# resolved at the build target platform (i.e. per-arch) instead of
# the default-for-`COPY --from=image:tag`, which resolves at the
# build platform.
FROM caddy:alpine AS caddy-bin

# Runtime stage - serve static assets via Caddy. Each output image
# in a multi-arch build picks up the matching alpine variant.
FROM alpine:3.21

# Copy Caddy from the target-arch stage above
COPY --from=caddy-bin /usr/bin/caddy /usr/bin/caddy

# Copy static bundle
COPY --from=builder /build/target/dx/mokosh-client/release/web/public /usr/share/caddy

# Pre-create /etc/caddy with 0755 because `COPY --chmod=` propagates
# the file mode to implicitly-created parent directories. The
# Caddyfile COPY below intentionally does NOT pin --chmod (git tracks
# the file at 100644 already); creating the parent dir here keeps
# its mode independent of whatever the file's mode ends up at.
RUN mkdir --parents /etc/caddy

# Default Caddyfile (override via volume mount). Mode comes from
# git's 100644 tracked mode; if a dev's host umask drops the read bit
# they need to `chmod 0644 oci-build/Caddyfile` once - the file is in
# the source tree and would surface the issue at build time.
COPY oci-build/Caddyfile /etc/caddy/Caddyfile

# Container entrypoint: renders /usr/share/caddy/_mokosh_config.js
# from env vars at start so operators can point the SPA at their own
# API/OIDC endpoints without rebuilding. Injects a <script> tag into
# index.html on first run. /usr/local/bin already exists in alpine so
# --chmod=0755 here is safe (no implicit parent dir creation).
COPY --chmod=0755 oci-build/entrypoint.sh /usr/local/bin/entrypoint.sh

RUN adduser --disabled-password --uid 1001 appuser \
    && chown --recursive appuser:appuser /usr/share/caddy
USER appuser

ENV PORT=8080
EXPOSE 8080

ARG GIT_SHA=""
LABEL org.opencontainers.image.source=https://dev.a8n.run/psa-systems/mokosh-clients
LABEL org.opencontainers.image.description="Mokosh Platform Dioxus client"
LABEL org.opencontainers.image.revision="${GIT_SHA}"

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile"]
