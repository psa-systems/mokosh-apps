#!/usr/bin/env bash
# PMS-1335 guard: no em-dash (U+2014) or en-dash (U+2013) anywhere under src/,
# tests/ or docs/. Catches the character in rendered copy, in comments, and in
# markdown prose, which is the full scope of the global style rule.
#
# What fails: any line that contains U+2014 or U+2013 in one of the scanned
# roots.
#
# Opt out on a line with `em-dash-guard-allow`, used by docs/copy-style.md's
# definition section (which has to spell the character to describe the rule).
#
# Usage: check-no-em-dash.sh [--self-test]
#   `--self-test` runs the guard over generated fixtures to prove it still
#   fails on an em-dash and still passes once the character is replaced.
set -u
cd "$(dirname "$0")/.." || exit 2

roots=(src tests docs)

scan() {
  local target_roots=("$@")
  # grep -P understands \x{HEX}; mokosh-apps uses GNU grep. Scan only text
  # files under the targeted roots; binary assets are explicitly excluded via
  # --binary-files=without-match.
  grep -rnP --binary-files=without-match '[\x{2014}\x{2013}]' "${target_roots[@]}" 2>/dev/null \
    | grep -v 'em-dash-guard-allow'
}

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  printf 'let msg = "stale — refresh"\n' > "$fixtures/bad.rs"
  if ! grep -q 'bad.rs' <(cd "$fixtures" && scan .); then
    echo "self-test: FAIL (an em-dash in a Rust string did not fail the guard)"
    status=1
  fi

  printf 'let msg = "stale, refresh"\n' > "$fixtures/good.rs"
  if (cd "$fixtures" && scan good.rs) | grep -q .; then
    echo "self-test: FAIL (a line with no em-dash was flagged)"
    status=1
  fi

  printf 'see rule: em-dash (U+2014) em-dash-guard-allow\n' > "$fixtures/allow.rs"
  if (cd "$fixtures" && scan allow.rs) | grep -q .; then
    echo "self-test: FAIL (the opt-out marker did not exempt the line)"
    status=1
  fi

  if [ "$status" -eq 0 ]; then
    echo "self-test: ok"
  fi
  exit "$status"
fi

hits=$(scan "${roots[@]}")
if [ -n "$hits" ]; then
  echo "em-dash or en-dash found. Replace with a hyphen, a colon, or a period." >&2
  echo "$hits" >&2
  exit 1
fi
