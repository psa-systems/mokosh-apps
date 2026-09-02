#!/usr/bin/env bash
# MAPPS-528 per_page guard: no call site asks for a page at or above the cap.
#
# mokosh-server caps `per_page` at `PaginationParams::MAX_PER_PAGE` (100) and
# CLAMPS anything larger instead of rejecting it, so a page that asked for 200
# got 100 rows, no error, and no sign that the rest existed. Fifteen sites did
# exactly that (`per_page=200`, `per_page=500`) and read `resp.data` once, so
# every one of those lists silently stopped at 100 rows.
#
# Asking for exactly 100 is the same defect one row later, so the rule is:
# a whole-collection read goes through the `get_all_*` helpers in
# src/hooks/fetch.rs, which page until a short page arrives; a list with its
# own pager asks for a page size BELOW the cap.
#
# What fails: `per_page=<n>` with n >= 100 on a non-comment line under `src/`,
# outside src/hooks/fetch.rs; and `per_page={CONST}` where the same file
# defines that const as >= 100.
#
# What passes: `per_page={PER_PAGE}` where the const is under the cap; a
# `per_page=` inside a `//` comment; and a line marked `per-page-guard-allow`.
#
# Usage: check-per-page-cap.sh [DIR | --self-test]
#   DIR defaults to `src`. `--self-test` re-runs the guard over generated
#   fixtures to prove it still rejects an over-cap literal and an over-cap
#   const, and still accepts an under-cap page size, a comment and the allow
#   marker, so a future edit cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

# The one file allowed to spell the cap: it defines MAX_PER_PAGE and the
# `get_all_*` helpers that every whole-collection read goes through.
helpers="src/hooks/fetch.rs"

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  mkdir -p "$fixtures/dirty"
  printf '    let path = format!("/invoices?company_id={id}&per_page=200");\n' > "$fixtures/dirty/page.rs"
  out=$("$0" "$fixtures/dirty" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (an over-cap per_page literal did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: an over-cap per_page literal fails the guard (exit $rc)"
  fi

  mkdir -p "$fixtures/dirty-const"
  {
    printf 'const SUBLIST_PER_PAGE: usize = 100;\n'
    printf '    let path = format!("/contracts/{id}/items?per_page={SUBLIST_PER_PAGE}");\n'
  } > "$fixtures/dirty-const/page.rs"
  out=$("$0" "$fixtures/dirty-const" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (an over-cap per_page const did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: an over-cap per_page const fails the guard (exit $rc)"
  fi

  mkdir -p "$fixtures/clean"
  {
    printf 'const PER_PAGE: usize = 25;\n'
    printf '    let path = format!("/contacts/contacts?page={page}&per_page={PER_PAGE}");\n'
    printf '    let recent = "/audit-log?page=1&per_page=5";\n'
    printf '    // the old query sent per_page=200, which the server clamped to 100\n'
    printf '    let legacy = "?per_page=200"; // per-page-guard-allow\n'
  } > "$fixtures/clean/page.rs"
  out=$("$0" "$fixtures/clean" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (an under-cap page size, a comment or the allow marker were rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: under-cap page sizes, comments and the allow marker pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "per_page cap guard self-test: clean"
  exit "$status"
fi

dir="${1:-src}"

if [ ! -d "$dir" ]; then
  echo "per_page cap guard: FAIL (no such directory: $dir)"
  exit 2
fi

files=$(find "$dir" -name '*.rs' -type f ! -path "*/${helpers#src/}" | sort)
find_rc=$?
if [ "$find_rc" -ne 0 ]; then
  echo "per_page cap guard: FAIL (find exited $find_rc scanning $dir)"
  exit 2
fi
if [ -z "$files" ]; then
  echo "per_page cap guard: FAIL (no .rs files under $dir)"
  exit 2
fi

# shellcheck disable=SC2086
hits=$(
  awk '
    function report(file, line, text, why) {
      printf "%s:%d: %s\n         (%s)\n", file, line, text, why
    }
    /per-page-guard-allow/ { next }
    {
      # Drop the comment tail so prose naming an old page size is not a value.
      line = $0
      sub(/\/\/.*$/, "", line)

      # `const NAME: usize = 200;` - remembered per file, checked at END
      # against the interpolations, since a const may be declared after use.
      if (match(line, /const [A-Z_]+[[:space:]]*:[[:space:]]*[iu](size|8|16|32|64)[[:space:]]*=[[:space:]]*[0-9]+/)) {
        decl = substr(line, RSTART, RLENGTH)
        split(decl, parts, /[[:space:]]+/)
        name = parts[2]
        sub(/:$/, "", name)
        sub(/:.*/, "", name)
        value = decl
        sub(/^.*=[[:space:]]*/, "", value)
        const_value[FILENAME, name] = value + 0
      }

      # A spelled-out page size at or above the cap.
      rest = line
      while (match(rest, /per_page=[0-9]+/)) {
        n = substr(rest, RSTART + 9, RLENGTH - 9) + 0
        if (n >= 100)
          report(FILENAME, FNR, $0, "per_page=" n " is at or above the server cap of 100")
        rest = substr(rest, RSTART + RLENGTH)
      }

      # An interpolated page size, resolved at END against the consts
      # declared in the same file.
      rest = line
      while (match(rest, /per_page=\{[A-Z_]+\}/)) {
        ref = substr(rest, RSTART + 10, RLENGTH - 11)
        used[FILENAME, ref] = FNR
        used_text[FILENAME, ref] = $0
        rest = substr(rest, RSTART + RLENGTH)
      }
    }
    END {
      for (key in used) {
        if (!((key) in const_value))
          continue
        if (const_value[key] >= 100) {
          split(key, k, SUBSEP)
          report(k[1], used[key], used_text[key], k[2] " is " const_value[key] ", at or above the server cap of 100")
        }
      }
    }
  ' $files
)
awk_rc=$?
if [ "$awk_rc" -ne 0 ]; then
  echo "per_page cap guard: FAIL (awk exited $awk_rc scanning $dir)"
  exit 2
fi

if [ -n "$hits" ]; then
  echo "per_page cap guard: FAIL (a page size at or above the server cap outside $helpers)"
  echo "The server clamps per_page to 100 and answers with a full page, so the"
  echo "caller cannot tell a complete list from a truncated one. Read the whole"
  echo "collection through the paging helpers, which stop on a short page:"
  echo '  get_authed::<Paginated<T>>("/things?per_page=200").await.map(|p| p.data)'
  echo '  ->  get_all_authed::<T>("/things").await'
  echo 'A list with its own pager keeps page={n} and a per_page BELOW the cap.'
  printf '%s\n' "$hits"
  exit 1
fi

echo "per_page cap guard: clean"
