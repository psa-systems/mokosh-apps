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

{
    echo "// Generated at container start by oci-build/entrypoint.sh."
    echo "// Operators override these via env vars on the mokosh-www container."
    printf 'window.__MOKOSH_CONFIG__ = {'
    first=1
    emit_field api_base "${MOKOSH_API_BASE:-}"
    emit_field oidc_issuer "${MOKOSH_OIDC_ISSUER:-}"
    emit_field oidc_client_id "${MOKOSH_OIDC_CLIENT_ID:-}"
    emit_field hub_base_url "${MOKOSH_HUB_BASE_URL:-}"
    echo '};'
} > "$CONFIG_JS"

# Inject the script tag into <head> if not already present. Idempotent
# across restarts (only injects once, even if the image layer's
# index.html is the canonical artifact between runs).
if ! grep -q -F "$INCLUDE_TAG" "$INDEX"; then
    sed -i "s|</head>|    ${INCLUDE_TAG}\\n</head>|" "$INDEX"
fi

exec "$@"
