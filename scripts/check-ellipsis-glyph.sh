#!/usr/bin/env bash
# MAPPS-445 ellipsis guard: rendered text uses the single ellipsis character
# (U+2026, "Loading…"), never three ASCII periods ("Loading...").
#
# MAPPS-410 chose U+2026 and 80-odd sites follow it, but the two spellings look
# alike in a diff, so later files kept reintroducing the ASCII one - far enough
# that `components/layout.rs` had to compare a page title against BOTH before
# this guard landed. Nothing about it is visible in review, which is why it is
# enforced rather than noticed.
#
# What fails: a string literal, on a line that is not a comment, in which the
# three periods follow a letter or digit ("Loading...", "Sending...").
#
# What passes: `...` as elision, where the periods stand in for omitted code or
# text rather than trailing a word - `"..."`, `": [...], "`, `rsx!(...)`,
# `{"error":"..."}`. Those are never rendered, and they read correctly as ASCII.
# Comments are skipped for the same reason; a comment quoting a UI string
# should still use the glyph, but that is prose review, not a guard.
#
# Opt out on a line with `ellipsis-guard-allow`, the same escape hatch the
# sibling guards use.
#
# Usage: check-ellipsis-glyph.sh [ROOT | --self-test]
#   ROOT defaults to `src`. `--self-test` re-runs the guard over generated
#   fixtures to prove it still rejects a three-dot ellipsis and still accepts
#   the glyph and an elision, so a future edit cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  printf '    p { class: "text-sm", "Loading..." }\n' > "$fixtures/ascii.rs"
  out=$("$0" "$fixtures" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (a three-dot ellipsis in a rendered string did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a three-dot ellipsis fails the guard (exit $rc)"
  fi
  rm -f "$fixtures/ascii.rs"

  {
    printf '    p { class: "text-sm", "Loading\xe2\x80\xa6" }\n'
    printf '    // a comment quoting "Loading..." is prose, not a rendered string\n'
    printf '    panic!("a NavItem without an `icon: rsx!(...)`");\n'
    printf '    let elided = "...";\n'
    printf '    let allowed = "Loading..."; // ellipsis-guard-allow\n'
  } > "$fixtures/clean.rs"
  out=$("$0" "$fixtures" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (the glyph, a comment, an elision or an allow marker were rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: the glyph, comments, elisions and the allow marker pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "ellipsis guard self-test: clean"
  exit "$status"
fi

root="${1:-src}"

hits=$(
  find "$root" -name '*.rs' -print0 | sort -z | xargs -0 awk '
    # Everything outside a double-quoted literal, dropped, so a `//` inside a
    # URL string is not mistaken for a comment and a `...` outside one (a range
    # pattern, a rest pattern) is never inspected. Escapes are removed first so
    # `\"` cannot close a literal.
    function strings(line,   out, rest, pre, body) {
      out = ""
      gsub(/\\./, "", line)
      rest = line
      while (match(rest, /"[^"]*"/)) {
        pre = substr(rest, 1, RSTART - 1)
        body = substr(rest, RSTART + 1, RLENGTH - 2)
        rest = substr(rest, RSTART + RLENGTH)
        # `pre` is code between literals, so a `//` in it opens a real comment
        # and the rest of the line is prose. A `//` inside an earlier literal
        # (a URL) never reaches here.
        if (index(pre, "//") > 0) break
        out = out " " body
      }
      return out
    }
    /ellipsis-guard-allow/ { next }
    {
      if (strings($0) ~ /[A-Za-z0-9]\.\.\./)
        print FILENAME ":" FNR ": " $0
    }
  '
)

if [ -n "$hits" ]; then
  echo "ellipsis guard: FAIL (three-dot ellipsis in a rendered string)"
  echo "Use the single character U+2026 instead, as the rest of the SPA does:"
  echo '  "Loading..."  ->  "Loading…"'
  printf '%s\n' "$hits"
  exit 1
fi

echo "ellipsis guard: clean"
