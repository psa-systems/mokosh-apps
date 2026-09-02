#!/usr/bin/env bash
# MAPPS-477 link-preview check, end to end and without a browser.
#
# The tags exist so a chat client or social platform renders a branded card for
# a pasted Mokosh link. That claim is only true if a crawler - which fetches the
# URL over HTTP and never runs the WASM app - receives them in the response
# body. Reading oci-build/entrypoint.sh cannot show that: the page still has to
# pass through Caddy's `try_files ... /index.html` SPA fallback, which is what
# serves every client-side route a real pasted link points at.
#
# So this runs the actual serving stack (oci-build/entrypoint.sh + Caddyfile in
# a caddy:2-alpine container over the repo's index.html) and fetches it with
# curl, which is exactly a no-JS crawler. It uses the repo index.html rather
# than a `dx build` output because the injection keys off `</head>`, which both
# carry; `just check-docker` builds the real image.
#
# Scenarios, matching the acceptance criteria:
#   1. branded  - every og:/twitter: tag carries the operator's values, on the
#                 root URL and on a deep client-side route, with og:image
#                 absolute.
#   2. default  - no branding env: built-in title/description, no image tags,
#                 twitter:card downgraded to `summary`.
#   3. relative - a root-relative logo with no MOKOSH_PUBLIC_URL: image tags
#                 omitted (a crawler cannot resolve one) and the reason logged.
#
# Usage: check-link-preview.sh [--self-test]
#   --self-test serves the un-injected index.html and requires the scenario-1
#   assertions to FAIL, so a check that stopped checking cannot report clean.
set -u
cd "$(dirname "$0")/.." || exit 2

CADDY_IMAGE="caddy:2-alpine"
CRAWLER_UA="facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)"
CONTAINER="mokosh-link-preview-$$"
WORKDIR=""
QUIET_FAIL="no"
failures=0

# Tear the test container down. Stop-then-remove rather than `rm --force`, and
# the suppressed output is the "no such container" case before the first
# scenario starts one; nothing downstream reads the result.
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
  [ "$QUIET_FAIL" = "no" ] && printf 'FAIL: %s\n' "$1" >&2
  failures=$((failures + 1))
}

# Start the serving stack over a fresh copy of index.html and print the host
# port it listens on. `--entrypoint ""` starts Caddy directly, skipping the
# injection, which is the --self-test baseline.
serve() {
  local use_entrypoint="$1"
  shift
  cp index.html "$WORKDIR/index.html" || return 1
  remove_container

  local entry=(--entrypoint /usr/local/bin/entrypoint.sh)
  [ "$use_entrypoint" = "no" ] && entry=(--entrypoint "")

  # Non-root, like the real image's `appuser`: it keeps the rewritten
  # index.html owned by the invoking user so the next scenario can replace it.
  docker run --detach --name "$CONTAINER" \
    --user "$(id -u):$(id -g)" \
    --env XDG_CONFIG_HOME=/tmp --env XDG_DATA_HOME=/tmp \
    --volume "$WORKDIR:/usr/share/caddy" \
    --volume "$PWD/oci-build/entrypoint.sh:/usr/local/bin/entrypoint.sh:ro" \
    --volume "$PWD/oci-build/Caddyfile:/etc/caddy/Caddyfile:ro" \
    --publish 127.0.0.1::8080 \
    --env PORT=8080 \
    "$@" \
    "${entry[@]}" \
    "$CADDY_IMAGE" caddy run --config /etc/caddy/Caddyfile >/dev/null || return 1

  local port
  port="$(docker port "$CONTAINER" 8080/tcp | head -1 | sed 's/.*://')"
  [ -n "$port" ] || return 1

  local i
  for i in $(seq 1 60); do
    if curl --silent --fail --output /dev/null "http://127.0.0.1:$port/"; then
      printf '%s' "$port"
      return 0
    fi
    sleep 0.25
  done
  return 1
}

# Fetch as a crawler does: plain HTTP GET, no JavaScript engine.
crawl() {
  curl --silent --show-error --user-agent "$CRAWLER_UA" "http://127.0.0.1:$1$2"
}

expect_tag() {
  local body="$1" tag="$2" where="$3"
  case "$body" in
  *"$tag"*) ;;
  *) fail "$where: crawler did not see $tag" ;;
  esac
}

reject_tag() {
  local body="$1" tag="$2" where="$3"
  case "$body" in
  *"$tag"*) fail "$where: $tag was served but should have been omitted" ;;
  *) ;;
  esac
}

# The scenario-1 assertions, factored out so --self-test can run the identical
# set against an un-injected page and require it to report failures.
assert_branded() {
  local port="$1" path="$2" where="crawler GET $2"
  local body
  body="$(crawl "$port" "$path")"

  expect_tag "$body" '<meta property="og:type" content="website">' "$where"
  expect_tag "$body" '<meta property="og:title" content="Acme &quot;PSA&quot; &amp; Co">' "$where"
  expect_tag "$body" '<meta property="og:site_name" content="Acme &quot;PSA&quot; &amp; Co">' "$where"
  expect_tag "$body" '<meta property="og:description" content="Ticketing &amp; billing">' "$where"
  expect_tag "$body" '<meta property="og:image" content="https://psa.example.com/branding/logo.svg">' "$where"
  expect_tag "$body" '<meta name="twitter:card" content="summary_large_image">' "$where"
  expect_tag "$body" '<meta name="twitter:title" content="Acme &quot;PSA&quot; &amp; Co">' "$where"
  expect_tag "$body" '<meta name="twitter:description" content="Ticketing &amp; billing">' "$where"
  expect_tag "$body" '<meta name="twitter:image" content="https://psa.example.com/branding/logo.svg">' "$where"
}

branded_env=(
  --env "MOKOSH_BRAND_NAME=Acme \"PSA\" & Co"
  --env "MOKOSH_BRAND_DESCRIPTION=Ticketing & billing"
  --env "MOKOSH_BRAND_LOGO_URL=/branding/logo.svg"
  --env "MOKOSH_PUBLIC_URL=https://psa.example.com/"
)

command -v docker >/dev/null 2>&1 || {
  echo "check-link-preview: docker is required (it runs the real Caddy serving stack)" >&2
  exit 2
}
docker image inspect "$CADDY_IMAGE" >/dev/null 2>&1 || docker pull "$CADDY_IMAGE" >/dev/null || {
  echo "check-link-preview: could not obtain $CADDY_IMAGE" >&2
  exit 2
}
WORKDIR="$(mktemp --directory)"

if [ "${1:-}" = "--self-test" ]; then
  QUIET_FAIL="yes"
  port="$(serve no)" || {
    echo "check-link-preview: --self-test could not start the un-injected server" >&2
    exit 2
  }
  assert_branded "$port" "/"
  remove_container
  if [ "$failures" -eq 0 ]; then
    echo "check-link-preview: SELF-TEST FAILED - the assertions passed against a page with no og:/twitter: tags, so they are not checking anything" >&2
    exit 1
  fi
  echo "check-link-preview: self-test ok (${failures} assertions correctly failed on an un-injected page)"
  exit 0
fi

# --- Scenario 1: branded, root URL and a deep client-side route -------------
port="$(serve yes "${branded_env[@]}")" || {
  echo "check-link-preview: could not start the branded server" >&2
  exit 2
}
assert_branded "$port" "/"
# A pasted link usually points at a route with no file on disk, which Caddy
# answers through `try_files ... /index.html`. The crawler must see the tags
# there too, else only a bare root link ever unfurls.
assert_branded "$port" "/tickets/12345"
code="$(curl --silent --output /dev/null --write-out '%{http_code}' --user-agent "$CRAWLER_UA" "http://127.0.0.1:$port/tickets/12345")"
[ "$code" = "200" ] || fail "crawler GET /tickets/12345: expected HTTP 200, got $code"
remove_container

# --- Scenario 2: no branding env at all ------------------------------------
port="$(serve yes)" || {
  echo "check-link-preview: could not start the default server" >&2
  exit 2
}
body="$(crawl "$port" "/")"
expect_tag "$body" '<meta property="og:title" content="Mokosh Platform">' "default"
expect_tag "$body" '<meta property="og:site_name" content="Mokosh Platform">' "default"
expect_tag "$body" '<meta property="og:description" content="Mokosh Platform - Professional Services Automation for MSPs">' "default"
expect_tag "$body" '<meta name="twitter:card" content="summary">' "default"
reject_tag "$body" 'og:image' "default"
reject_tag "$body" 'twitter:image' "default"
remove_container

# --- Scenario 3: relative logo with no MOKOSH_PUBLIC_URL --------------------
port="$(serve yes --env "MOKOSH_BRAND_LOGO_URL=/branding/logo.svg")" || {
  echo "check-link-preview: could not start the relative-logo server" >&2
  exit 2
}
body="$(crawl "$port" "/")"
reject_tag "$body" 'og:image' "relative logo"
reject_tag "$body" 'twitter:image' "relative logo"
expect_tag "$body" '<meta name="twitter:card" content="summary">' "relative logo"
# Omitting the operator's logo silently would look like the branding never
# applied. The entrypoint must say why on stderr.
logs="$(docker logs "$CONTAINER" 2>&1)"
case "$logs" in
*"MOKOSH_PUBLIC_URL is unset"*) ;;
*) fail "relative logo: entrypoint dropped og:image without logging a reason" ;;
esac
remove_container

if [ "$failures" -gt 0 ]; then
  echo "check-link-preview: ${failures} failure(s) - a pasted Mokosh link would not render the expected preview" >&2
  exit 1
fi
echo "check-link-preview: ok (a no-JS crawler sees the branded og:/twitter: tags on the root URL and on a deep route)"
