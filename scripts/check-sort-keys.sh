#!/usr/bin/env bash
# MAPPS-527 sort-key guard: no page hardcodes a `?sort=` value.
#
# mokosh-server validates `sort` against a per-endpoint allow-list inside
# `PaginationParams::order_by` and silently drops anything else, answering in
# its default order. A key the SPA invents therefore paints a sorted column
# header over rows that never moved, and nothing errors. That shape survived
# three parity audits (`company_type`, `company_name`, `-updated_at`), so the
# allow-lists are mirrored in `src/utils/sort_keys.rs`, every query fragment is
# a const there, and the unit tests in that module assert each const against
# its endpoint's list. This guard is what keeps a new page from reintroducing
# a bare literal that no test covers.
#
# What fails: a `sort=<value>` string literal on a non-comment line under
# `src/`, outside `src/utils/sort_keys.rs`.
#
# What passes: `&sort={field}` and the like, where the value is interpolated
# from a checked const or a mapping function; `sort=` inside a `//` comment;
# and a line marked `sort-key-guard-allow`.
#
# Usage: check-sort-keys.sh [DIR | --self-test]
#   DIR defaults to `src`. `--self-test` re-runs the guard over generated
#   fixtures to prove it still rejects a hardcoded key and still accepts the
#   interpolated form, a comment and the allow marker, so a future edit cannot
#   quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

# The one file allowed to spell a sort value out: it is the mirror of the
# server's allow-lists and its tests check every const against them.
mirror="src/utils/sort_keys.rs"

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  mkdir -p "$fixtures/dirty"
  printf '    let path = "/tickets?per_page=5&sort=-updated_at";\n' > "$fixtures/dirty/page.rs"
  out=$("$0" "$fixtures/dirty" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (a hardcoded sort key did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a hardcoded sort key fails the guard (exit $rc)"
  fi

  mkdir -p "$fixtures/clean"
  {
    printf '    path.push_str(&format!("&sort={field}&sort_dir={dir}"));\n'
    printf '    let p = format!("/tickets?per_page=5&{TICKETS_RECENT_SORT}");\n'
    printf '    // the old query sent sort=-updated_at, which the server dropped\n'
    printf '    let legacy = "?sort=name"; // sort-key-guard-allow\n'
  } > "$fixtures/clean/page.rs"
  out=$("$0" "$fixtures/clean" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (an interpolated key, a comment or the allow marker were rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: interpolated keys, comments and the allow marker pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "sort-key guard self-test: clean"
  exit "$status"
fi

dir="${1:-src}"

if [ ! -d "$dir" ]; then
  echo "sort-key guard: FAIL (no such directory: $dir)"
  exit 2
fi

files=$(find "$dir" -name '*.rs' -type f ! -path "*/${mirror#src/}" | sort)
find_rc=$?
if [ "$find_rc" -ne 0 ]; then
  echo "sort-key guard: FAIL (find exited $find_rc scanning $dir)"
  exit 2
fi
if [ -z "$files" ]; then
  echo "sort-key guard: FAIL (no .rs files under $dir)"
  exit 2
fi

# shellcheck disable=SC2086
hits=$(
  awk '
    /sort-key-guard-allow/ { next }
    {
      # Drop the comment tail so prose naming an old key is not a value.
      line = $0
      sub(/\/\/.*$/, "", line)
      # `sort=` followed by anything other than `{` is a spelled-out key;
      # `sort={field}` interpolates a value the Rust tests already check.
      if (line ~ /sort=[^{"]/)
        print FILENAME ":" FNR ": " $0
    }
  ' $files
)
awk_rc=$?
if [ "$awk_rc" -ne 0 ]; then
  echo "sort-key guard: FAIL (awk exited $awk_rc scanning $dir)"
  exit 2
fi

if [ -n "$hits" ]; then
  echo "sort-key guard: FAIL (hardcoded ?sort= value outside $mirror)"
  echo "The server drops a sort key it does not allow-list and answers in its"
  echo "default order, so the header lies. Add the fragment as a const in"
  echo "$mirror, where a test checks it against the endpoint's allow-list:"
  echo '  "/tickets?per_page=5&sort=-updated_at"'
  echo '  ->  format!("/tickets?per_page=5&{TICKETS_RECENT_SORT}")'
  printf '%s\n' "$hits"
  exit 1
fi

echo "sort-key guard: clean"
