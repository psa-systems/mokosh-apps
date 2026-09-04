#!/usr/bin/env bash
# MAPPS-659 guard: the first-paint theme applier and the app must name the same
# localStorage key.
#
# `assets/theme-init.js` read `localStorage.theme` while `src/hooks/theme.rs`
# has always written `mokosh_theme`, so the pre-hydration guard never found a
# stored preference and every load fell through to `prefers-color-scheme`. A
# user who chose Light under a Dark OS (or the reverse) got a first frame in the
# wrong mode until the WASM app booted and corrected it. Nothing could catch it:
# the two names live in different languages, in files no shared test reads, and
# both sides work perfectly on their own.
#
# The invariant: the key literal in the JS applier equals the `THEME_KEY`
# constant in the Rust hook, and the applier reads no other localStorage key
# (the original bug was a bare `localStorage.theme` property read, not a
# mismatched constant).
#
# Usage: check-theme-storage-key.sh [--self-test | --compare RUST_FILE JS_FILE]
#   No argument checks this repository's own pair.
#   `--compare` is the pure comparison, split out so `--self-test` can prove
#   offline that the guard still rejects a drifted key. A guard that quietly
#   matches nothing reports clean forever.
set -u
cd "$(dirname "$0")/.." || exit 2

RUST_FILE='src/hooks/theme.rs'
JS_FILE='assets/theme-init.js'

# `const THEME_KEY: &str = "mokosh_theme";`
rust_key() {
  sed -nE 's/^[[:space:]]*(pub[[:space:]]+)?const THEME_KEY:[[:space:]]*&.*str[[:space:]]*=[[:space:]]*"([^"]*)".*/\2/p' "$1"
}

# `var THEME_KEY = 'mokosh_theme';`
js_key() {
  sed -nE "s/^[[:space:]]*(var|let|const)[[:space:]]+THEME_KEY[[:space:]]*=[[:space:]]*['\"]([^'\"]*)['\"].*/\2/p" "$1"
}

# Any localStorage access in the JS that does not go through the constant:
# a property read (`localStorage.theme`, `'theme' in localStorage`) or a
# getItem/setItem call with a string literal instead of THEME_KEY. Line
# comments are skipped, so the file can still describe the bug it fixes.
stray_reads() {
  awk '{ line = $0; sub(/^[[:space:]]*/, "", line); if (line !~ /^\/\//) print NR ":" $0 }' "$1" |
    grep -E "localStorage\.[A-Za-z_$][A-Za-z0-9_$]*|in[[:space:]]+localStorage|(get|set|remove)Item\([[:space:]]*['\"]" |
    grep -vE "localStorage\.(getItem|setItem|removeItem)\([[:space:]]*THEME_KEY"
}

compare() {
  local rust="$1" js="$2" rkey jkey strays fail=0

  for f in "$rust" "$js"; do
    if [ ! -f "$f" ]; then
      echo "theme-storage-key guard: FAIL (no such file: $f)"
      return 2
    fi
  done

  rkey=$(rust_key "$rust")
  jkey=$(js_key "$js")

  if [ -z "$rkey" ] || [ "$(printf '%s\n' "$rkey" | wc -l)" -ne 1 ]; then
    echo "theme-storage-key guard: FAIL ($rust does not declare exactly one THEME_KEY constant)"
    echo "  The guard could not read what it is meant to compare, which is not a pass."
    return 2
  fi
  if [ -z "$jkey" ] || [ "$(printf '%s\n' "$jkey" | wc -l)" -ne 1 ]; then
    echo "theme-storage-key guard: FAIL ($js does not declare exactly one THEME_KEY variable)"
    echo "  The first-paint applier must name its key once, in a constant the"
    echo "  guard can compare against $rust."
    return 2
  fi

  if [ "$rkey" != "$jkey" ]; then
    echo "theme-storage-key guard: FAIL (the two sides name different localStorage keys)"
    echo "  $rust: \"$rkey\""
    echo "  $js: \"$jkey\""
    echo "  The applier then never finds a stored preference and every load paints"
    echo "  from the OS match, so an explicit theme choice loses the first frame."
    fail=1
  fi

  strays=$(stray_reads "$js")
  if [ -n "$strays" ]; then
    echo "theme-storage-key guard: FAIL ($js reads localStorage outside THEME_KEY)"
    printf '%s\n' "$strays" | sed 's/^/  /'
    echo "  Read the preference as localStorage.getItem(THEME_KEY); a literal key"
    echo "  here is the exact shape of the MAPPS-659 bug."
    fail=1
  fi

  if [ "$fail" -eq 0 ]; then
    echo "theme-storage-key guard: OK (both sides use \"$rkey\")"
  fi
  return "$fail"
}

case "${1:-}" in
  --compare)
    if [ $# -ne 3 ]; then
      echo "theme-storage-key guard: FAIL (--compare needs RUST_FILE and JS_FILE)"
      exit 2
    fi
    compare "$2" "$3"
    exit $?
    ;;
  --self-test)
    tmp=$(mktemp -d) || exit 2
    trap 'rm -r "$tmp"' EXIT
    status=0

    cp "$RUST_FILE" "$tmp/theme.rs"

    # The shape that shipped: the applier reading a key nothing writes.
    cat > "$tmp/property-read.js" <<'EOF'
(function () {
  var stored = localStorage.theme;
  if (stored === 'dark' || !('theme' in localStorage)) {
    document.documentElement.classList.add('dark');
  }
})();
EOF

    # A constant, but a drifted one.
    sed "s/THEME_KEY = 'mokosh_theme'/THEME_KEY = 'theme'/" "$JS_FILE" > "$tmp/drifted.js"

    # The constant matches, but a second read sneaks a literal key back in.
    sed "s/var stored = localStorage.getItem(THEME_KEY);/var stored = localStorage.getItem('theme');/" \
      "$JS_FILE" > "$tmp/literal-getitem.js"

    # Neither side declares anything the guard can read.
    printf 'not-the-hook\n' > "$tmp/empty.rs"
    printf '(function () {})();\n' > "$tmp/empty.js"

    out=$(bash "$0" --compare "$tmp/theme.rs" "$JS_FILE" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "self-test: FAIL (the real pair was rejected, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: the repository's own pair passes"
    fi

    out=$(bash "$0" --compare "$tmp/theme.rs" "$tmp/drifted.js" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (a drifted key did not fail the guard, exit $rc)"
      printf '%s\n' "$out"
      status=1
    elif ! printf '%s' "$out" | grep -q 'mokosh_theme'; then
      echo "self-test: FAIL (the failure did not name the two keys)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a drifted key fails the guard and names both sides"
    fi

    out=$(bash "$0" --compare "$tmp/theme.rs" "$tmp/property-read.js" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
      echo "self-test: FAIL (the original localStorage.theme applier passed)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: the original localStorage.theme applier fails the guard"
    fi

    out=$(bash "$0" --compare "$tmp/theme.rs" "$tmp/literal-getitem.js" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (a literal getItem key did not fail the guard, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a literal getItem key fails the guard"
    fi

    out=$(bash "$0" --compare "$tmp/empty.rs" "$JS_FILE" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 2 ]; then
      echo "self-test: FAIL (a hook with no THEME_KEY did not error, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a hook declaring no THEME_KEY is an error, not a pass"
    fi

    out=$(bash "$0" --compare "$tmp/theme.rs" "$tmp/empty.js" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 2 ]; then
      echo "self-test: FAIL (an applier with no THEME_KEY did not error, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: an applier declaring no THEME_KEY is an error, not a pass"
    fi

    [ "$status" -eq 0 ] && echo "theme-storage-key guard self-test: clean"
    exit "$status"
    ;;
  "")
    ;;
  *)
    echo "theme-storage-key guard: FAIL (unknown argument: $1)"
    exit 2
    ;;
esac

compare "$RUST_FILE" "$JS_FILE"
exit $?
