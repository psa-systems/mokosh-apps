#!/usr/bin/env bash
# MAPPS-789 guard: a busy surface renders the shared `TableLoading` or
# `DetailSkeleton` skeleton, never a hand-rolled "Loading…" string.
#
# This is the fourth report of the class (PMS-353, the prior audit window,
# MAPPS-789 itself), and scoping the fix to a named sibling group let it keep
# regrowing: two new settings pages copied `settings_modules.rs`'s
# `p { ..., "Loading…" }` shape verbatim before this guard existed. So rather
# than only fixing the sites this run found, every `"Loading` string literal
# anywhere in `src` is checked, either against a hard zero (a file with no
# prior debt) or a baseline (a file that already carried debt when this guard
# landed, capped at that count so it cannot grow, `tickets.rs` and friends).
# Every "Loading" site this run touched (`settings_modules.rs`,
# `settings_note_editing.rs`, `settings_timesheet_editing.rs`,
# `payment_methods.rs`) is a hard zero, same as any file this guard has never
# seen before.
#
# Two files are excluded outright rather than baselined: `components/
# skeleton.rs`, the skeletons themselves, and the two documented boot-time
# exceptions that render before the component tree or the theme shell exists
# (`src/lib.rs`, `contact_portal/portal_branding.rs`) - neither can reach for
# `DetailSkeleton`, which needs both.
#
# A "Loading" hit is a `"Loading` substring inside a double-quoted string
# literal, on a line that is not a comment (mirrors `check-ellipsis-glyph.sh`'s
# string extraction so a `// see "Loading…" above` note cannot trip it).
#
# Opt out on a line with `loading-guard-allow`, the same escape hatch the
# sibling guards use.
#
# Usage: check-loading-recipe.sh [ROOT | --self-test]
#   ROOT defaults to `src`. `--self-test` re-runs the guard over generated
#   fixtures to prove it still rejects a fresh hand-rolled "Loading" line and
#   still accepts a skeleton component, a comment, and the allow marker, so a
#   future edit cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

# Files that already carried "Loading" string literals when this guard
# landed, capped at that count. A file not listed here starts at zero.
baseline_file() { # baseline_file <path>
  case "$1" in
  src/components/layout.rs) echo 1 ;;
  src/pages/admin.rs) echo 2 ;;
  src/pages/approvals.rs) echo 1 ;;
  src/pages/assets.rs) echo 3 ;;
  src/pages/button_showcase.rs) echo 2 ;;
  src/pages/calendar.rs) echo 3 ;;
  src/pages/contacts.rs) echo 3 ;;
  src/pages/credit_notes.rs) echo 1 ;;
  src/pages/dashboards_view.rs) echo 1 ;;
  src/pages/kb_activity.rs) echo 1 ;;
  src/pages/knowledge_base.rs) echo 3 ;;
  src/pages/projects.rs) echo 2 ;;
  src/pages/quotes.rs) echo 2 ;;
  src/pages/request_form.rs) echo 1 ;;
  src/pages/settings_branding.rs) echo 1 ;;
  src/pages/tickets.rs) echo 7 ;;
  src/pages/time.rs) echo 3 ;;
  *) echo 0 ;;
  esac
}

is_exception() { # is_exception <path>
  case "$1" in
  src/components/skeleton.rs) return 0 ;;
  src/lib.rs) return 0 ;;
  src/pages/contact_portal/portal_branding.rs) return 0 ;;
  *) return 1 ;;
  esac
}

# Per-file count of `"Loading` string-literal hits, one per line.
hits_in() { # hits_in <file>
  awk '
    function strings(line,   out, rest, pre, body) {
      out = ""
      gsub(/\\./, "", line)
      rest = line
      while (match(rest, /"[^"]*"/)) {
        pre = substr(rest, 1, RSTART - 1)
        body = substr(rest, RSTART + 1, RLENGTH - 2)
        rest = substr(rest, RSTART + RLENGTH)
        if (index(pre, "//") > 0) break
        out = out "\x01" body
      }
      return out
    }
    /loading-guard-allow/ { next }
    { if (strings($0) ~ /Loading/) print FILENAME ":" FNR ": " $0 }
  ' "$1"
}

run() { # run <root>
  local root="$1" status=0
  while IFS= read -r -d '' file; do
    is_exception "$file" && continue
    local hits
    hits=$(hits_in "$file")
    [ -z "$hits" ] && continue
    local count cap
    count=$(printf '%s\n' "$hits" | wc -l)
    cap=$(baseline_file "$file")
    if [ "$count" -gt "$cap" ]; then
      echo "loading guard: FAIL ($file has $count \"Loading\" string literal(s), cap is $cap)"
      printf '%s\n' "$hits"
      status=1
    fi
  done < <(find "$root" -name '*.rs' -print0 | sort -z)
  return "$status"
}

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  mkdir -p "$fixtures/src/pages"
  printf '    p { class: "p-6 text-sm text-subtle", "Loading…" }\n' \
    >"$fixtures/src/pages/new_page.rs"
  out=$(run "$fixtures/src" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (a fresh hand-rolled 'Loading' line did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a fresh hand-rolled 'Loading' line fails the guard (exit $rc)"
  fi
  rm -f "$fixtures/src/pages/new_page.rs"

  {
    printf '    crate::components::DetailSkeleton {}\n'
    printf '    // a comment quoting "Loading…" is prose, not a rendered string\n'
    printf '    let allowed = "Loading…"; // loading-guard-allow\n'
  } >"$fixtures/src/pages/clean.rs"
  out=$(run "$fixtures/src" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (a skeleton, a comment and the allow marker were rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a skeleton, a comment and the allow marker pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "loading guard self-test: clean"
  exit "$status"
fi

root="${1:-src}"
if run "$root"; then
  echo "loading guard: clean"
else
  echo "Replace the hand-rolled line with TableLoading (a table body) or"
  echo "DetailSkeleton (a record or card-shaped surface):"
  echo '  p { "Loading…" }  ->  crate::components::DetailSkeleton {}'
  exit 1
fi
