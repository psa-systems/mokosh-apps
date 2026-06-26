#!/bin/sh
# Container entrypoint: render a small JS shim that exposes runtime
# config to the SPA, then exec the CMD (Caddy by default).
#
# Why this exists: the Mokosh SPA is a static WASM bundle. Operators
# self-hosting on a custom hostname need to point it at their own
# API/OIDC endpoints without rebuilding the image. The shim writes
# `window.__MOKOSH_CONFIG__` from env vars on each container start;
# the SPA reads it before falling through to its compile-time defaults
# and the built-in `msp.<tld>` host-prefix derivation.
#
# Only env vars that are set and non-empty are emitted. In dev (where
# no entrypoint runs) or when no env vars are set, `_mokosh_config.js`
# is still served but contains an empty object, and the SPA falls
# through to its existing behaviour.

set -eu

CONFIG_JS="/usr/share/caddy/_mokosh_config.js"
INDEX="/usr/share/caddy/index.html"
INCLUDE_TAG='<script src="/_mokosh_config.js"></script>'

# JSON-escape backslash and double-quote so a value containing either
# does not break the emitted JS object literal. Operators set these
# env vars themselves so this is not an attacker boundary, but
# silently corrupting the page on a stray quote is worse than escaping.
escape_js() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

emit_field() {
    name="$1"
    val="$2"
    if [ -n "${val:-}" ]; then
        if [ "${first:-1}" -eq 0 ]; then
            printf ', '
        fi
        printf '"%s": "%s"' "$name" "$(escape_js "$val")"
        first=0
    fi
}

if ! {
    echo "// Generated at container start by oci-build/entrypoint.sh."
    echo "// Operators override these via env vars on the mokosh-www container."
    printf 'window.__MOKOSH_CONFIG__ = {'
    first=1
    emit_field api_base "${MOKOSH_API_BASE:-}"
    emit_field oidc_issuer "${MOKOSH_OIDC_ISSUER:-}"
    emit_field oidc_client_id "${MOKOSH_OIDC_CLIENT_ID:-}"
    emit_field hub_base_url "${MOKOSH_HUB_BASE_URL:-}"
    # BUNYIP-142: requested scope string for /oauth2/authorize. Default
    # compile-time value is "openid email offline_access"; operators
    # opting in to bunyip's profile/phone claim emission set this to
    # e.g. "openid email offline_access profile" without rebuilding the
    # SPA image.
    emit_field oidc_scopes "${MOKOSH_OIDC_SCOPES:-}"
    # MAPPS-329: Team admin nav feature flag. Locked off by default; set
    # `MOKOSH_TEAM_ENABLED=true` (or `=1`) per deployment to expose the
    # Team item under the Admin nav section. Route::Team and its API stay
    # reachable by direct URL regardless of the flag.
    emit_field team_enabled "${MOKOSH_TEAM_ENABLED:-}"
    # build_sha is the git revision the WASM bundle was built from.
    # Baked into the image at build time via Dockerfile's GIT_SHA build
    # arg. The SPA polls `_mokosh_config.js` and reloads when this
    # changes, so a fresh deploy automatically propagates to open tabs
    # without users having to Ctrl+Shift+R. Emitted even when other
    # config fields are empty (operator-overridable fields stay opt-in,
    # but the version field is always-on).
    emit_field build_sha "${GIT_SHA:-}"
    echo '};'
} > "$CONFIG_JS" 2>/dev/null; then
    echo "[entrypoint] WARN: could not write ${CONFIG_JS} (read-only fs?); SPA will fall back to compile-time config" >&2
fi

# Inject the script tag into <head> if not already present. Idempotent
# across restarts (only injects once, even if the image layer's
# index.html is the canonical artifact between runs).
#
# This is best-effort: if the rootfs is read-only (e.g. operator runs
# the container with `read_only: true`) the sed -i write fails. We
# explicitly do not let that abort startup - Caddy can still serve
# the un-injected index.html, and the SPA falls through to its
# compile-time defaults. Log a warning so operators see the cause.
if ! grep -q -F "$INCLUDE_TAG" "$INDEX"; then
    if ! sed -i "s|</head>|    ${INCLUDE_TAG}\\n</head>|" "$INDEX" 2>/dev/null; then
        echo "[entrypoint] WARN: could not patch ${INDEX} (read-only fs?); SPA will fall back to compile-time config" >&2
    fi
fi

exec "$@"
