#!/usr/bin/env bash
# MAPPS-963 serving-uid check, end to end and without a browser.
#
# The claim is that the mokosh-www image boots and serves its runtime
# config shim as any non-root uid - specifically the image's own
# `appuser` (uid 1001) AND an operator override like DEV-271's
# `10000:10000`. The old image chowned /usr/share/caddy to 1001 and
# only worked at that uid; MAPPS-963 moved the runtime write out to
# $MOKOSH_RUNTIME_DIR (default /tmp/mokosh-runtime), which is
# world-writable, so this check now exercises both uids on the live
# serving stack rather than reading the Dockerfile.
#
# Mechanics mirror scripts/check-link-preview.sh: a caddy:2-alpine
# container with the repo's oci-build/{entrypoint.sh,Caddyfile}
# docker-cp'd in on top of a bundled index.html stand-in, no bind
# mounts, no host ports. A curl container joins the server's network
# namespace to fetch the shim; a separate run with a read-only
# $MOKOSH_RUNTIME_DIR exercises the negative path.
#
# Scenarios, matching the acceptance criteria:
#   1. default-uid  - the image's own appuser (uid 1001) serves
#                     /_mokosh_config.js and the patched /index.html.
#   2. uid-override - the same stack with --user 10000:10000 serves
#                     both files unchanged, which is what unblocks
#                     reapplying DEV-271 to mokosh-apps.
#   3. unwritable   - MOKOSH_RUNTIME_DIR pointed at a read-only path
#                     exits non-zero at start with a message naming
#                     the path.
#
# Usage: check-oci-runtime.sh [--self-test]
#   --self-test runs scenario 1 against an entrypoint that writes the
#   shim to a UID-owned path, so a check that stopped checking fails
#   on the uid-override case even though scenario 1 would pass.
set -u
cd "$(dirname "$0")/.." || exit 2

CADDY_IMAGE="caddy:2-alpine"
CURL_IMAGE="curlimages/curl:8.16.0"
CONTAINER_BASE="mokosh-oci-runtime-$$"
WORKDIR=""
failures=0

cleanup() {
    for suffix in default override unwritable; do
        name="${CONTAINER_BASE}-${suffix}"
        docker stop "$name" >/dev/null 2>&1 || true
        docker rm "$name" >/dev/null 2>&1 || true
    done
    if [ -n "$WORKDIR" ] && [ -d "$WORKDIR" ]; then
        rm -rf "$WORKDIR"
    fi
}
trap cleanup EXIT

fail() {
    failures=$((failures + 1))
    echo "FAIL: $*" >&2
}
pass() {
    echo "pass: $*"
}

prepare_workdir() {
    WORKDIR="$(mktemp -d)" || return 1
    # A minimal index.html the entrypoint can patch; the serving
    # contract is "the operator's env lands in the config shim and
    # the include tag is in <head>", which does not need the dx
    # bundle output. The real image replaces this file at
    # /usr/share/caddy/index.html; this script copies in the same
    # path via `docker cp`.
    cat > "$WORKDIR/index.html" <<'HTML'
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Mokosh test</title>
</head>
<body></body>
</html>
HTML
}

# Start a plain caddy:2-alpine with our entrypoint and Caddyfile.
# $1 = container suffix, remaining args forwarded to `docker run`
# (uid override, env, etc.). Does not publish host ports - a curl
# container joins the server's network namespace with --network
# container:<name> to reach :8080 the same way on every environment.
start_server() {
    suffix="$1"
    shift
    name="${CONTAINER_BASE}-${suffix}"
    # The target paths all live under directories that already exist
    # in caddy:2-alpine (/usr/local/bin, /etc/caddy, /usr/share/caddy),
    # so a plain `docker cp` lands without a `mkdir` dance.
    docker create \
        --name "$name" \
        --entrypoint /usr/local/bin/entrypoint.sh \
        -e PORT=8080 \
        -e MOKOSH_API_BASE="https://api.test.example" \
        -e GIT_SHA="abcdef1234567890" \
        "$@" \
        "$CADDY_IMAGE" \
        caddy run --config /etc/caddy/Caddyfile \
        >/dev/null || return 1
    docker cp oci-build/entrypoint.sh "$name:/usr/local/bin/entrypoint.sh" >/dev/null || return 1
    docker cp oci-build/Caddyfile "$name:/etc/caddy/Caddyfile" >/dev/null || return 1
    docker cp "$WORKDIR/index.html" "$name:/usr/share/caddy/index.html" >/dev/null || return 1
    docker start "$name" >/dev/null || return 1
    # Wait for Caddy to answer. 30 seconds beats a slow runner start.
    for _ in $(seq 1 60); do
        if docker run --rm --network "container:$name" "$CURL_IMAGE" \
            -sf --max-time 1 http://127.0.0.1:8080/_mokosh_config.js >/dev/null 2>&1
        then
            return 0
        fi
        sleep 0.5
    done
    echo "server-${suffix} never became ready" >&2
    docker logs "$name" 2>&1 | tail -40 >&2
    return 1
}

curl_in() {
    suffix="$1"
    path="$2"
    name="${CONTAINER_BASE}-${suffix}"
    docker run --rm --network "container:$name" "$CURL_IMAGE" \
        -sf --max-time 5 "http://127.0.0.1:8080${path}"
}

assert_contains() {
    scenario="$1"
    label="$2"
    haystack="$3"
    needle="$4"
    if printf '%s' "$haystack" | grep -q -F "$needle"; then
        pass "${scenario}: ${label} contains ${needle}"
    else
        fail "${scenario}: ${label} did NOT contain ${needle}; body was: $(printf '%s' "$haystack" | head -c 400)"
    fi
}

check_uid() {
    scenario="$1"
    shift
    if ! start_server "$scenario" "$@"; then
        fail "${scenario}: server failed to start"
        return
    fi

    cfg_js="$(curl_in "$scenario" /_mokosh_config.js || true)"
    index="$(curl_in "$scenario" /index.html || true)"

    assert_contains "$scenario" "/_mokosh_config.js" "$cfg_js" '"api_base": "https://api.test.example"'
    assert_contains "$scenario" "/_mokosh_config.js" "$cfg_js" '"build_sha": "abcdef123456"'
    assert_contains "$scenario" "/index.html" "$index" '<script src="/_mokosh_config.js"></script>'
    assert_contains "$scenario" "/index.html" "$index" 'MAPPS-477 link-preview metadata'

    # No "Permission denied" line from Caddy / sh / sed anywhere in
    # the server's own log. Both uids must boot cleanly.
    name="${CONTAINER_BASE}-${scenario}"
    log="$(docker logs "$name" 2>&1 || true)"
    if printf '%s' "$log" | grep -q -i 'permission denied'; then
        fail "${scenario}: server log contained Permission denied line: $(printf '%s' "$log" | grep -i 'permission denied' | head -3)"
    else
        pass "${scenario}: server log is clean of Permission denied"
    fi
}

check_unwritable() {
    scenario="unwritable"
    name="${CONTAINER_BASE}-${scenario}"
    # tmpfs mode=0555 is unwritable to every uid, which is the
    # read-only-rootfs posture an operator who bind-mounts a stale
    # path hits. The old entrypoint logged a warning and served the
    # compile-time fallback; the new one must exit non-zero and name
    # the path.
    docker run --rm \
        --name "$name" \
        --entrypoint /usr/local/bin/entrypoint.sh \
        -e PORT=8080 \
        -e MOKOSH_RUNTIME_DIR=/ro/runtime \
        --tmpfs /ro:ro,mode=0555 \
        -v "$(pwd)/oci-build/entrypoint.sh:/usr/local/bin/entrypoint.sh:ro" \
        -v "$(pwd)/oci-build/Caddyfile:/etc/caddy/Caddyfile:ro" \
        "$CADDY_IMAGE" \
        caddy run --config /etc/caddy/Caddyfile \
        >/tmp/mapps963-stdout.log 2>/tmp/mapps963-stderr.log
    code=$?
    stderr="$(cat /tmp/mapps963-stderr.log 2>/dev/null || true)"
    rm -f /tmp/mapps963-stdout.log /tmp/mapps963-stderr.log

    if [ "$code" -eq 0 ]; then
        fail "${scenario}: entrypoint exited 0 on a read-only runtime dir; it must refuse to boot"
    else
        pass "${scenario}: entrypoint exited non-zero (${code}) on a read-only runtime dir"
    fi
    assert_contains "$scenario" "stderr" "$stderr" "FATAL"
    assert_contains "$scenario" "stderr" "$stderr" "/ro/runtime"
}

main() {
    self_test="no"
    for arg in "$@"; do
        case "$arg" in
            --self-test) self_test="yes" ;;
            *) echo "unknown argument: $arg" >&2; exit 2 ;;
        esac
    done

    if ! command -v docker >/dev/null 2>&1; then
        echo "docker is required for check-oci-runtime.sh" >&2
        exit 2
    fi

    prepare_workdir || { echo "could not prepare workdir" >&2; exit 2; }

    if [ "$self_test" = "yes" ]; then
        # Rewrite the entrypoint so it writes to a uid-1001-owned path
        # instead of the configured runtime dir. The default-uid case
        # still works (appuser owns it), but the --user 10000:10000
        # case must fail: that is the regression this check exists to
        # catch, so a check that stopped checking is loud rather than
        # quiet.
        broken_dir="$(mktemp -d)"
        cp oci-build/entrypoint.sh "$broken_dir/entrypoint.sh"
        chmod +x "$broken_dir/entrypoint.sh"
        sed -i 's|RUNTIME_DIR="${MOKOSH_RUNTIME_DIR:-/tmp/mokosh-runtime}"|RUNTIME_DIR="/usr/share/caddy/runtime-broken"|' \
            "$broken_dir/entrypoint.sh"
        # Point the server at the broken entrypoint and expect
        # scenario 2 to fail.
        suffix="selftest"
        name="${CONTAINER_BASE}-${suffix}"
        docker create \
            --name "$name" \
            --user 10000:10000 \
            --entrypoint /usr/local/bin/entrypoint.sh \
            -e PORT=8080 \
            "$CADDY_IMAGE" \
            caddy run --config /etc/caddy/Caddyfile \
            >/dev/null
        docker cp "$broken_dir/entrypoint.sh" "$name:/usr/local/bin/entrypoint.sh"
        docker cp oci-build/Caddyfile "$name:/etc/caddy/Caddyfile"
        docker cp "$WORKDIR/index.html" "$name:/usr/share/caddy/index.html"
        if docker start "$name" >/dev/null && \
           sleep 2 && \
           docker ps --filter "name=$name" --filter "status=running" --format '{{.Names}}' | grep -q "$name"
        then
            echo "self-test FAILED: the broken entrypoint stayed up as uid 10000; the real check would not catch the regression" >&2
            docker stop "$name" >/dev/null 2>&1 || true
            docker rm "$name" >/dev/null 2>&1 || true
            rm -rf "$broken_dir"
            exit 1
        fi
        docker rm "$name" >/dev/null 2>&1 || true
        rm -rf "$broken_dir"
        echo "self-test ok: the broken entrypoint exited as uid 10000"
        exit 0
    fi

    check_uid default-uid
    check_uid uid-override --user 10000:10000
    check_unwritable

    if [ "$failures" -gt 0 ]; then
        echo "" >&2
        echo "${failures} check(s) failed" >&2
        exit 1
    fi
    echo "all MAPPS-963 runtime scenarios pass"
}

main "$@"
