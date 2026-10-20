#!/bin/sh
# Container entrypoint: render a small JS shim (and its JSON sibling)
# that expose runtime config to the SPA, then exec the CMD (Caddy by
# default).
#
# Why this exists: the Mokosh SPA is a static WASM bundle. Operators
# self-hosting on a custom hostname need to point it at their own
# API/OIDC endpoints without rebuilding the image. The JS shim writes
# `window.__MOKOSH_CONFIG__` from env vars on each container start;
# the SPA reads it before falling through to its compile-time defaults
# and the built-in `msp.<tld>` host-prefix derivation.
#
# MAPPS-812: the served CSP allows `WebAssembly.instantiate` only, not
# `eval()`, so the SPA's update probe (`fetch_live_build_sha` in
# `src/hooks/update_check.rs`) cannot re-evaluate `_mokosh_config.js`
# as script to read the live `build_sha`. `_mokosh_config.json` is the
# same field set rendered as data instead of code, from the same
# `build_config_fields` list below, so the probe can `serde_json` it.
#
# Only env vars that are set and non-empty are emitted. In dev (where
# no entrypoint runs) or when no env vars are set, both files are
# still served but contain an empty object, and the SPA falls through
# to its existing behaviour.
#
# MAPPS-963: the generated config and the patched index.html land in
# a per-run directory under /tmp so the image works as any non-root
# uid rather than hard-coding ownership of /usr/share/caddy. The
# Caddyfile is matched to serve those three paths from here before
# falling back to the compile-time bundle; a failure to prepare the
# directory exits non-zero so a half-wired deploy fails loud at start
# rather than silently serving the compile-time fallback.

set -eu

RUNTIME_DIR="${MOKOSH_RUNTIME_DIR:-/tmp/mokosh-runtime}"
BUNDLE_INDEX="/usr/share/caddy/index.html"
CONFIG_JS="${RUNTIME_DIR}/_mokosh_config.js"
CONFIG_JSON="${RUNTIME_DIR}/_mokosh_config.json"
INDEX="${RUNTIME_DIR}/index.html"
INCLUDE_TAG='<script src="/_mokosh_config.js"></script>'

# MAPPS-963: prepare the runtime directory and refresh the index copy
# every start. The directory lives under /tmp so any uid can write
# it; a failure here is fatal because the Caddyfile's matched root
# below is this path and a missing file means the shim is unreachable.
fail_runtime_dir() {
    echo "[entrypoint] FATAL: ${1}: ${RUNTIME_DIR}. Mount a writable path at this location or set MOKOSH_RUNTIME_DIR to one." >&2
    exit 1
}

mkdir -p "$RUNTIME_DIR" 2>/dev/null || fail_runtime_dir "cannot create runtime directory"
if [ ! -w "$RUNTIME_DIR" ]; then
    fail_runtime_dir "runtime directory is not writable"
fi
# Start every run from the pristine image copy of index.html so the
# patches below are idempotent and a prior restart's edits cannot
# accumulate.
cp "$BUNDLE_INDEX" "$INDEX" 2>/dev/null \
    || fail_runtime_dir "cannot copy index.html into runtime directory"

# Caddy's XDG and HOME paths. Caddy 2 writes autosaves to
# $XDG_CONFIG_HOME/caddy and tries $HOME/.config / $HOME/.local
# otherwise; those paths are not writable for a non-root uid without
# this redirect, and the fallback warnings fill the log.
export HOME="$RUNTIME_DIR"
export XDG_CONFIG_HOME="${RUNTIME_DIR}/.config"
export XDG_DATA_HOME="${RUNTIME_DIR}/.local/share"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" 2>/dev/null || true

# MAPPS-831: MOKOSH_DOCS_URL was renamed to MOKOSH_DOCS_BASE_URL to match the
# desktop build's env var for the same field. Still honoured as a fallback,
# but warn so operators migrate.
if [ -z "${MOKOSH_DOCS_BASE_URL:-}" ] && [ -n "${MOKOSH_DOCS_URL:-}" ]; then
    echo "[entrypoint] WARN: MOKOSH_DOCS_URL is deprecated; set MOKOSH_DOCS_BASE_URL instead" >&2
fi

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

# The one list of runtime-config fields (MAPPS-812): every field the SPA
# can read at runtime, as "name<TAB>value" lines. `render_config_fields`
# below is the only consumer, and it drives both `_mokosh_config.js` and
# `_mokosh_config.json`, so the two files can never drift apart in which
# fields they carry.
build_config_fields() {
    printf 'api_base\t%s\n' "${MOKOSH_API_BASE:-}"
    printf 'oidc_issuer\t%s\n' "${MOKOSH_OIDC_ISSUER:-}"
    printf 'oidc_client_id\t%s\n' "${MOKOSH_OIDC_CLIENT_ID:-}"
    # MAPPS-831: runtime lever for the redirect URI, matching every other
    # OIDC field above. Without this, a path-prefixed or otherwise
    # non-default deployment had no way to override redirect_uri short of
    # rebuilding the image with MOKOSH_OIDC_REDIRECT_URI baked in at
    # compile time.
    printf 'oidc_redirect_uri\t%s\n' "${MOKOSH_OIDC_REDIRECT_URI:-}"
    printf 'hub_base_url\t%s\n' "${MOKOSH_HUB_BASE_URL:-}"
    # MAPPS-649: the single host the portal is served from (e.g.
    # `portal.psa.systems`). The SPA reads this to (a) decide whether
    # the current host is the portal host (`on_portal_host()` in
    # `src/hooks/fetch.rs`) and (b) derive the API base when the SPA
    # is running there (same fn's `api_base()`). Empty (default) turns
    # both off and the SPA falls back to its `msp.<tld>` agent-only
    # derivation. Retires the per-MSP `MOKOSH_PORTAL_HOST_SUFFIX` env;
    # see docs/dev-docs/portal-single-host-cutover.md in mokosh-server.
    printf 'portal_host\t%s\n' "${MOKOSH_PORTAL_HOST:-}"
    # MAPPS-453: documentation subdomain base URL (e.g. https://docs.a8n.systems).
    # Unset hides the Documentation menu entry and every contextual help link,
    # unless the SPA can derive it from a `msp.<tld>` host (MAPPS-999).
    # MAPPS-831: renamed from MOKOSH_DOCS_URL to MOKOSH_DOCS_BASE_URL to match
    # the desktop build's env var for the same field (see
    # src/modules/runtime_config.rs, which derives MOKOSH_<FIELD> from the
    # runtime-config field name). MOKOSH_DOCS_URL is read as a deprecated
    # fallback for operators who have not migrated yet.
    printf 'docs_base_url\t%s\n' "${MOKOSH_DOCS_BASE_URL:-${MOKOSH_DOCS_URL:-}}"
    # PMS-1337: organizations (the tenant switcher and the create-team action
    # beside it) are hidden unless this says 1, true or yes. The feature is not
    # built out and its charging model is unresolved, so the default is off and
    # turning it on is a deployment decision rather than a rebuild. Hidden rather
    # than greyed out because it is unbuilt rather than refused; see
    # src/modules/feature_flags.rs.
    printf 'organizations_enabled\t%s\n' "${MOKOSH_ORGANIZATIONS_ENABLED:-}"
    # BUNYIP-142: requested scope string for /oauth2/authorize. Default
    # compile-time value is "openid email offline_access"; operators
    # opting in to bunyip's profile/phone claim emission set this to
    # e.g. "openid email offline_access profile" without rebuilding the
    # SPA image.
    printf 'oidc_scopes\t%s\n' "${MOKOSH_OIDC_SCOPES:-}"
    # PMS-1337: `team_enabled` (MAPPS-329) is gone from here. PMS-791 phase 2
    # made Teams core and retired the flag in the SPA, but this line and its row
    # in docs/self-hosting.md stayed, so an operator could set
    # MOKOSH_TEAM_ENABLED, see it land in `_mokosh_config.js`, and get no
    # behaviour at all - a knob that looks supported and reads nothing, which is
    # exactly what makes somebody reintroduce it.
    # MAPPS-509: operator branding. Unset means the SPA keeps its built-in
    # name and artwork, so a deployment that sets none of these renders
    # exactly as before. The logo and hero URLs must resolve on the SPA
    # origin (mount the file into /usr/share/caddy) or on the API origin:
    # the Caddyfile CSP is `img-src 'self' data: {API origin}`. Everything
    # outside /assets/* and /wasm/* is served no-cache, so a remounted
    # file propagates on the next load. See docs/deployment-branding.md.
    # MAPPS-880: the server's KB attachment size cap
    # (`KB_ATTACHMENT_MAX_BYTES`, operator-tunable there too). Unset means
    # the SPA keeps its compile-time 5 MiB default, so a deployment that
    # sets neither renders exactly as before. Set this to the SAME value
    # as the server's `KB_ATTACHMENT_MAX_BYTES` env var, or the client's
    # pre-upload check and the server's actual limit drift apart again.
    printf 'kb_attachment_max_bytes\t%s\n' "${MOKOSH_KB_ATTACHMENT_MAX_BYTES:-}"
    printf 'brand_name\t%s\n' "${MOKOSH_BRAND_NAME:-}"
    printf 'brand_logo_url\t%s\n' "${MOKOSH_BRAND_LOGO_URL:-}"
    printf 'brand_hero_url\t%s\n' "${MOKOSH_BRAND_HERO_URL:-}"
    # build_sha is the git revision the WASM bundle was built from.
    # Baked into the image at build time via Dockerfile's GIT_SHA build
    # arg. The SPA polls `_mokosh_config.json` and reloads when this
    # changes, so a fresh deploy automatically propagates to open tabs
    # without users having to Ctrl+Shift+R. Emitted even when other
    # config fields are empty (operator-overridable fields stay opt-in,
    # but the version field is always-on).
    # MAPPS-813: truncated to 12 characters to match build.rs:29's
    # `.take(12)`, which is what the desktop build bakes as its baseline.
    # `fetch_live_build_sha` compares this value against that baseline
    # directly, so the two sides must carry the same-length sha or the
    # comparison is never equal.
    printf 'build_sha\t%.12s\n' "${GIT_SHA:-}"
}

# Emit every field in `build_config_fields` via `emit_field`, so a
# caller wrapping this in `window.__MOKOSH_CONFIG__ = { ... };` (JS) or
# bare `{ ... }` (JSON) always gets the same field set. Resets `first`
# so callers do not need to manage the comma state themselves.
render_config_fields() {
    first=1
    while IFS="$(printf '\t')" read -r name val; do
        emit_field "$name" "$val"
    done <<EOF
$(build_config_fields)
EOF
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

# MAPPS-477: resolve a branding image URL to the ABSOLUTE form og:image and
# twitter:image require. A link-preview crawler fetches the image from its own
# servers, with no page to resolve a relative path against, so it drops a
# root-relative value - and root-relative is exactly what
# docs/deployment-branding.md recommends for the logo, because the browser CSP
# is `img-src 'self'`. Join such a value onto MOKOSH_PUBLIC_URL's origin. With
# no MOKOSH_PUBLIC_URL there is no correct absolute form, so this yields empty
# and the caller omits the image tags instead of emitting an unfetchable one.
absolute_image_url() {
    case "$1" in
        "")
            :
            ;;
        *://*)
            printf '%s' "$1"
            ;;
        *)
            image_base="$(origin_of "${MOKOSH_PUBLIC_URL:-}")"
            if [ -n "$image_base" ]; then
                printf '%s/%s' "$image_base" "${1#/}"
            fi
            ;;
    esac
}

{
    echo "// Generated at container start by oci-build/entrypoint.sh."
    echo "// Operators override these via env vars on the mokosh-www container."
    printf 'window.__MOKOSH_CONFIG__ = {'
    render_config_fields
    echo '};'
} > "$CONFIG_JS" || fail_runtime_dir "cannot write ${CONFIG_JS#$RUNTIME_DIR/} to"

# MAPPS-812: the JSON sibling of `_mokosh_config.js`, same field set
# (`render_config_fields`), read by `fetch_live_build_sha` in
# `src/hooks/update_check.rs` since the served CSP forbids the `eval()`
# that reading the JS shim's `build_sha` at runtime would require.
{
    printf '{'
    render_config_fields
    printf '}'
} > "$CONFIG_JSON" || fail_runtime_dir "cannot write ${CONFIG_JSON#$RUNTIME_DIR/} to"

# Inject the script tag into <head>. The runtime copy of index.html is
# refreshed from the pristine bundle on every start (see cp above), so
# this always edits the un-patched version; no idempotence check needed.
if ! sed -i "s|</head>|    ${INCLUDE_TAG}\\n</head>|" "$INDEX"; then
    fail_runtime_dir "cannot patch index.html in"
fi

# MAPPS-477: link-preview (OpenGraph / Twitter) metadata. A link-preview
# crawler does not run the WASM app, so these tags must live in the served
# HTML. They are stamped from the branding env here, at container start, the
# same way _mokosh_config.js is; the SPA never sets them.
#
# MAPPS-963: the runtime index.html is fresh every start (see cp above),
# so the strip-stale-tags dance the previous in-place patch needed is
# gone - there are no stale tags to find.
OG_MARKER='<!-- MAPPS-477 link-preview metadata -->'
og_title="$(escape_html "${MOKOSH_BRAND_NAME:-Mokosh Platform}")"
og_desc="$(escape_html "${MOKOSH_BRAND_DESCRIPTION:-Mokosh Platform - Professional Services Automation for MSPs}")"
og_image_raw="$(absolute_image_url "${MOKOSH_BRAND_LOGO_URL:-}")"
if [ -n "${MOKOSH_BRAND_LOGO_URL:-}" ] && [ -z "$og_image_raw" ]; then
    echo "[entrypoint] WARN: MOKOSH_BRAND_LOGO_URL='${MOKOSH_BRAND_LOGO_URL}' is not absolute and MOKOSH_PUBLIC_URL is unset; og:image/twitter:image omitted because a link-preview crawler cannot resolve a relative image URL. Set MOKOSH_PUBLIC_URL to the site's public base URL." >&2
fi

og_blockfile="${RUNTIME_DIR}/.og-block"
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
} > "$og_blockfile" || fail_runtime_dir "cannot stage link-preview metadata in"

og_tmp="${RUNTIME_DIR}/.index.ogtmp"
if ! awk 'FNR==NR{b=b $0 ORS; next} !ins && /<\/head>/{printf "%s",b; ins=1} {print}' "$og_blockfile" "$INDEX" > "$og_tmp"; then
    rm -f "$og_tmp"
    fail_runtime_dir "cannot inject link-preview metadata into index.html in"
fi
mv "$og_tmp" "$INDEX"
rm -f "$og_blockfile"

# MAPPS-369: derive origin-scoped CSP sources from the operator-facing base
# URLs and export them so the Caddyfile's connect-src (read by the `caddy run`
# exec'd below) allows the API / OIDC origins without their paths.
#
# MAPPS-814: with no MOKOSH_API_BASE / MOKOSH_OIDC_ISSUER, the SPA still
# derives an origin at runtime from the browser's `msp.<tld>` host
# (`src/hooks/fetch.rs::api_base()`, `src/modules/oidc/config.rs::resolve()`),
# but entrypoint.sh runs once at container start and does not know that host
# (one image serves every deployment). Export the Caddyfile's `map`
# placeholder name instead of an empty string in that case, so the CSP header
# picks up the SAME derivation from the actual request's Host header.
if [ -n "${MOKOSH_API_BASE:-}" ]; then
    MOKOSH_API_ORIGIN="$(origin_of "${MOKOSH_API_BASE}")"
else
    MOKOSH_API_ORIGIN='{mokosh_api_origin_fallback}'
fi
if [ -n "${MOKOSH_OIDC_ISSUER:-}" ]; then
    MOKOSH_OIDC_ORIGIN="$(origin_of "${MOKOSH_OIDC_ISSUER}")"
else
    MOKOSH_OIDC_ORIGIN='{mokosh_oidc_origin_fallback}'
fi
export MOKOSH_API_ORIGIN MOKOSH_OIDC_ORIGIN MOKOSH_RUNTIME_DIR="$RUNTIME_DIR"

exec "$@"
