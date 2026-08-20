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

# HTML-escape for an attribute context (og:/twitter: content="..."). Distinct
# from escape_js: the link-preview tags below live in HTML, not the JS object.
escape_html() {
    printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g'
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

# MAPPS-369: reduce a URL to its origin (scheme://host[:port]), dropping any
# path/query. The Caddy CSP connect-src must be origin-scoped: a source that
# keeps a path (e.g. ".../api/v1") is an exact-path match and blocks every API
# sub-path (/api/v1/auth/login, ...). A relative or empty value yields empty -
# the request is same-origin and already covered by connect-src 'self'.
origin_of() {
    case "$1" in
        *://*)
            scheme=${1%%://*}
            rest=${1#*://}
            printf '%s://%s' "$scheme" "${rest%%/*}"
            ;;
        *)
            # relative path or empty: same-origin, 'self' covers it
            :
            ;;
    esac
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
    # MAPPS-453: documentation subdomain base URL (e.g. https://docs.n.niceguyit.biz).
    # Unset hides the Documentation menu entry and every contextual help link.
    emit_field docs_base_url "${MOKOSH_DOCS_URL:-}"
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
    # MAPPS-509: operator branding. Unset means the SPA keeps its built-in
    # name and artwork, so a deployment that sets none of these renders
    # exactly as before. The logo and hero URLs must resolve on the SPA
    # origin (mount the file into /usr/share/caddy) or on the API origin:
    # the Caddyfile CSP is `img-src 'self' data: {API origin}`. Everything
    # outside /assets/* and /wasm/* is served no-cache, so a remounted
    # file propagates on the next load. See docs/deployment-branding.md.
    emit_field brand_name "${MOKOSH_BRAND_NAME:-}"
    emit_field brand_logo_url "${MOKOSH_BRAND_LOGO_URL:-}"
    emit_field brand_hero_url "${MOKOSH_BRAND_HERO_URL:-}"
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

# MAPPS-477: link-preview (OpenGraph / Twitter) metadata. A link-preview
# crawler does not run the WASM app, so these tags must live in the served
# HTML. They are stamped from the branding env here, at container start, the
# same way _mokosh_config.js is; the SPA never sets them. Idempotent (skips if
# already injected) and best-effort (a read-only rootfs is not fatal).
OG_MARKER='<!-- MAPPS-477 link-preview metadata -->'
if ! grep -q -F "$OG_MARKER" "$INDEX"; then
    og_title="$(escape_html "${MOKOSH_BRAND_NAME:-Mokosh Platform}")"
    og_desc="$(escape_html "${MOKOSH_BRAND_DESCRIPTION:-Mokosh Platform - Professional Services Automation for MSPs}")"
    og_image_raw="${MOKOSH_BRAND_LOGO_URL:-}"

    og_blockfile="$(mktemp 2>/dev/null || echo "${INDEX}.ogblock")"
    {
        printf '    %s\n' "$OG_MARKER"
        printf '    <meta property="og:type" content="website">\n'
        printf '    <meta property="og:title" content="%s">\n' "$og_title"
        printf '    <meta property="og:site_name" content="%s">\n' "$og_title"
        printf '    <meta property="og:description" content="%s">\n' "$og_desc"
        if [ -n "$og_image_raw" ]; then
            og_image="$(escape_html "$og_image_raw")"
            printf '    <meta property="og:image" content="%s">\n' "$og_image"
            printf '    <meta name="twitter:card" content="summary_large_image">\n'
        else
            printf '    <meta name="twitter:card" content="summary">\n'
        fi
        printf '    <meta name="twitter:title" content="%s">\n' "$og_title"
        printf '    <meta name="twitter:description" content="%s">\n' "$og_desc"
        if [ -n "$og_image_raw" ]; then
            printf '    <meta name="twitter:image" content="%s">\n' "$og_image"
        fi
    } > "$og_blockfile" 2>/dev/null

    og_tmp="$(mktemp 2>/dev/null || echo "${INDEX}.ogtmp")"
    if [ -s "$og_blockfile" ] \
        && awk 'FNR==NR{b=b $0 ORS; next} !ins && /<\/head>/{printf "%s",b; ins=1} {print}' "$og_blockfile" "$INDEX" > "$og_tmp" 2>/dev/null \
        && mv "$og_tmp" "$INDEX" 2>/dev/null; then
        :
    else
        echo "[entrypoint] WARN: could not inject link-preview metadata into ${INDEX} (read-only fs?); a pasted link shows the built-in defaults or nothing" >&2
        rm -f "$og_tmp" 2>/dev/null
    fi
    rm -f "$og_blockfile" 2>/dev/null
fi

# MAPPS-369: derive origin-scoped CSP sources from the operator-facing base
# URLs and export them so the Caddyfile's connect-src (read by the `caddy run`
# exec'd below) allows the API / OIDC origins without their paths.
MOKOSH_API_ORIGIN="$(origin_of "${MOKOSH_API_BASE:-}")"
MOKOSH_OIDC_ORIGIN="$(origin_of "${MOKOSH_OIDC_ISSUER:-}")"
export MOKOSH_API_ORIGIN MOKOSH_OIDC_ORIGIN

exec "$@"
