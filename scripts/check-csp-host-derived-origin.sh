#!/usr/bin/env bash
# MAPPS-814 CSP host-derived origin check, end to end and without a browser.
#
# `src/hooks/fetch.rs::api_base()` and `src/modules/oidc/config.rs::resolve()`
# derive an API/OIDC origin from the browser's `msp.<tld>` host when no
# MOKOSH_API_BASE / MOKOSH_OIDC_ISSUER is configured. Reading the Caddyfile
# cannot show whether the served CSP actually names that same origin: the
# `map` directives it relies on only evaluate inside a running Caddy, keyed
# off the real request's Host header.
#
# So this runs the actual serving stack (oci-build/entrypoint.sh + Caddyfile
# in a caddy:2-alpine container) and reads the Content-Security-Policy header
# back with curl, exactly as a browser would receive it.
#
# Scenarios:
#   1. zero-config, msp.<tld> host   - connect-src/img-src name the derived
#                                      API/OIDC origins (the acceptance
#                                      criterion).
#   2. zero-config, non-msp host     - no derived origin is added (nothing
#                                      for the SPA to have derived either).
#   3. explicit MOKOSH_API_BASE/MOKOSH_OIDC_ISSUER on an msp.<tld> host - the
#                                      operator's origin wins, not the
#                                      host-derived guess.
#
# SPEC-2.1.1: invocable with no arguments, exiting 0/nonzero with an
# `ok`/`<failures> failure(s)` line naming the reason.
# SPEC-2.1.3: this script carries no --self-test mode; the spec treats these
# three live HTTP scenarios run against the real stack as the guard's own
# test, since there is no synthetic fixture that stands in for a served CSP.
set -u
cd "$(dirname "$0")/.." || exit 2

CADDY_IMAGE="caddy:2-alpine"
CONTAINER="mokosh-csp-origin-$$"
WORKDIR=""
failures=0

# SPEC-2.7.1: stop and remove every container this script starts, regardless
# of outcome.
remove_container() {
  docker stop --timeout 2 "$CONTAINER" >/dev/null 2>&1
  docker rm "$CONTAINER" >/dev/null 2>&1
  return 0
}

cleanup() {
  remove_container
  [ -n "$WORKDIR" ] && rm --recursive "$WORKDIR"
}
trap cleanup EXIT

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  failures=$((failures + 1))
}

# SPEC-2.7.1: starts the real caddy:2-alpine serving stack in a container.
# SPEC-2.3.3: MOKOSH_API_BASE/MOKOSH_OIDC_ISSUER (when passed via "$@") are
# set only as env vars into this container, never read from the host shell.
# Start the serving stack with a bare index.html and print the host port.
serve() {
  cp index.html "$WORKDIR/index.html" || return 1
  remove_container

  docker run --detach --name "$CONTAINER" \
    --user "$(id -u):$(id -g)" \
    --env XDG_CONFIG_HOME=/tmp --env XDG_DATA_HOME=/tmp \
    --volume "$WORKDIR:/usr/share/caddy" \
    --volume "$PWD/oci-build/entrypoint.sh:/usr/local/bin/entrypoint.sh:ro" \
    --volume "$PWD/oci-build/Caddyfile:/etc/caddy/Caddyfile:ro" \
    --publish 127.0.0.1::8080 \
    --env PORT=8080 \
    "$@" \
    --entrypoint /usr/local/bin/entrypoint.sh \
    "$CADDY_IMAGE" caddy run --config /etc/caddy/Caddyfile >/dev/null || return 1

  local port
  port="$(docker port "$CONTAINER" 8080/tcp | head -1 | sed 's/.*://')"
  [ -n "$port" ] || return 1

  local i
  for i in $(seq 1 60); do
    if curl --silent --fail --output /dev/null --header "Host: probe.invalid" "http://127.0.0.1:$port/"; then
      printf '%s' "$port"
      return 0
    fi
    sleep 0.25
  done
  return 1
}

csp() {
  curl --silent --show-error --header "Host: $2" "http://127.0.0.1:$1/" \
    --dump-header - --output /dev/null | tr -d '\r' | grep -i '^content-security-policy:'
}

# SPEC-2.5.26: require Docker to be present, failing with exit 2 and a
# message naming Docker rather than letting its absence surface further down.
command -v docker >/dev/null 2>&1 || {
  echo "check-csp-host-derived-origin: docker is required (it runs the real Caddy serving stack)" >&2
  exit 2
}
docker image inspect "$CADDY_IMAGE" >/dev/null 2>&1 || docker pull "$CADDY_IMAGE" >/dev/null || {
  echo "check-csp-host-derived-origin: could not obtain $CADDY_IMAGE" >&2
  exit 2
}
WORKDIR="$(mktemp --directory)"

# SPEC-2.4.3: on a zero-config msp.<tld> host, connect-src/img-src must name
# the API/OIDC origins the client itself derives from that host.
# --- Scenario 1: zero-config, msp.<tld> host ---------------------------------
port="$(serve)" || {
  echo "check-csp-host-derived-origin: could not start the zero-config server" >&2
  exit 2
}
header="$(csp "$port" "msp.example.com")"
case "$header" in
*"connect-src 'self' https://api.msp.example.com https://api.example.com"*) ;;
*) fail "zero-config msp.example.com: connect-src did not name the derived origins ($header)" ;;
esac
case "$header" in
*"img-src 'self' data: https://api.msp.example.com"*) ;;
*) fail "zero-config msp.example.com: img-src did not name the derived API origin ($header)" ;;
esac
remove_container

# SPEC-2.4.3: on a non-msp.<tld> host with no explicit origin, the policy
# must not add any derived API origin at all.
# --- Scenario 2: zero-config, non-msp host -----------------------------------
port="$(serve)" || {
  echo "check-csp-host-derived-origin: could not start the zero-config server" >&2
  exit 2
}
header="$(csp "$port" "example.com")"
case "$header" in
*"https://api."*) fail "zero-config example.com: connect-src/img-src named an origin the SPA never derives ($header)" ;;
*) ;;
esac
remove_container

# SPEC-2.4.3: with MOKOSH_API_BASE/MOKOSH_OIDC_ISSUER explicitly set, the
# policy must show the operator's origin and not fall back to the
# host-derived one.
# --- Scenario 3: explicit origin on an msp.<tld> host ------------------------
explicit_env=(
  --env "MOKOSH_API_BASE=https://api.example.org/api/v1"
  --env "MOKOSH_OIDC_ISSUER=https://issuer.example.org"
)
port="$(serve "${explicit_env[@]}")" || {
  echo "check-csp-host-derived-origin: could not start the explicit-origin server" >&2
  exit 2
}
header="$(csp "$port" "msp.example.com")"
case "$header" in
*"https://api.example.org"*"https://issuer.example.org"*) ;;
*) fail "explicit config on msp.example.com: connect-src did not keep the operator's origin ($header)" ;;
esac
case "$header" in
*"api.msp.example.com"*) fail "explicit config on msp.example.com: connect-src fell back to the host-derived origin instead of the operator's ($header)" ;;
*) ;;
esac
remove_container

if [ "$failures" -gt 0 ]; then
  echo "check-csp-host-derived-origin: ${failures} failure(s) - the served CSP does not track the SPA's own derived origin" >&2
  exit 1
fi
echo "check-csp-host-derived-origin: ok (the served CSP names the SPA's msp.<tld>-derived origin, and an operator's explicit origin still wins)"
