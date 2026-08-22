#!/usr/bin/env bash
# MAPPS-530 dev-SSO scheme guard: the dev-SSO overlay hands the browser https
# URLs only, never plain http.
#
# The overlay routes the SPA at Host(`${USER}-mokosh.a8n.run`) over TLS, so the
# page is an https origin. Any absolute http:// URL it bakes in (the hub, the
# issuer, a redirect_uri) is a mixed-content request the browser blocks or
# upgrades depending on policy, and the one dev stack built to mirror the
# deployed shape stops mirroring it. MOKOSH_HUB_BASE_URL was http for exactly
# this reason and nothing in review caught it, so it is enforced.
#
# What fails: `http://` on a non-comment line of the overlay.
#
# What passes: `https://`, and `http://` inside a `#` comment, where it is prose
# about the base compose.yml's localhost binding rather than a value the SPA
# receives. Opt out on a line with `dev-sso-scheme-guard-allow`, the same escape
# hatch the sibling guards use.
#
# Usage: check-dev-sso-scheme.sh [FILE | --self-test]
#   FILE defaults to `compose.dev-sso.yml`. `--self-test` re-runs the guard over
#   generated fixtures to prove it still rejects a plain-http value and still
#   accepts https, a comment and the allow marker, so a future edit cannot
#   quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  printf '      MOKOSH_HUB_BASE_URL: http://${USER}-bunyip.a8n.run\n' > "$fixtures/dirty.yml"
  out=$("$0" "$fixtures/dirty.yml" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (a plain-http URL did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a plain-http URL fails the guard (exit $rc)"
  fi

  {
    printf '      MOKOSH_HUB_BASE_URL: https://${USER}-bunyip.a8n.run\n'
    printf '      # the base compose.yml binds http://localhost:4301 for Google OAuth\n'
    printf '      MOKOSH_LEGACY_URL: http://example.test # dev-sso-scheme-guard-allow\n'
  } > "$fixtures/clean.yml"
  out=$("$0" "$fixtures/clean.yml" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (https, a comment or the allow marker were rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: https, comments and the allow marker pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "dev-SSO scheme guard self-test: clean"
  exit "$status"
fi

file="${1:-compose.dev-sso.yml}"

if [ ! -f "$file" ]; then
  echo "dev-SSO scheme guard: FAIL (no such file: $file)"
  exit 2
fi

hits=$(
  awk '
    /dev-sso-scheme-guard-allow/ { next }
    {
      # Drop the comment tail so a `#` explanation naming an http URL is prose,
      # not a value. A `#` inside a value would be a comment to YAML too.
      line = $0
      sub(/#.*$/, "", line)
      if (line ~ /http:\/\//)
        print FILENAME ":" FNR ": " $0
    }
  ' "$file"
)
# A failed awk yields no hits, which would read as "clean". Fail loud instead.
awk_rc=$?
if [ "$awk_rc" -ne 0 ]; then
  echo "dev-SSO scheme guard: FAIL (awk exited $awk_rc scanning $file)"
  exit 2
fi

if [ -n "$hits" ]; then
  echo "dev-SSO scheme guard: FAIL (plain-http URL in the TLS-routed overlay)"
  echo "The overlay serves the SPA over TLS, so every absolute URL it hands the"
  echo "browser must be https or the request is mixed content:"
  echo '  http://${USER}-bunyip.a8n.run  ->  https://${USER}-bunyip.a8n.run'
  printf '%s\n' "$hits"
  exit 1
fi

echo "dev-SSO scheme guard: clean"
