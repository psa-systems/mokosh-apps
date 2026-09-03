#!/usr/bin/env bash
# MAPPS-438 guard: a page renders only rows the backend returned. A failed
# fetch renders ErrorBanner over the table's empty state, never seeded rows.
#
# Tickets and Tenants each carried a `*Source { Backend, Demo }` enum whose
# Demo arm was taken on a 4xx and rendered five invented tickets (TKT-1234,
# "Acme Corp", live links into /tickets/{1..5} that resolve to nothing) under
# an amber "showing demo rows" banner. Both spellings compile and the demo
# path only appears when a fetch fails, so nothing about it shows up in review
# or in a normal run. MAPPS-403/414 removed the same pattern from the portal.
#
# What fails, on a non-comment line: a standalone `Demo` token (the enum arm or
# a `Source::Demo` match), or rendered copy admitting the rows are demo rows.
# `SeedDemoPanel` and friends do not match: the token has to stand alone.
#
# Usage: check-no-demo-rows.sh [FILE...| --self-test]
#   Defaults to every page in src/pages.
#   `--self-test` re-runs the guard over generated fixtures to prove it still
#   rejects each violation and still accepts the migrated form, so a future
#   edit cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  check_rejects() {
    local name="$1" file="$2" out rc
    out=$("$0" "$file" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
      echo "self-test: FAIL ($name did not fail the guard)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: $name fails the guard (exit $rc)"
    fi
  }

  cat > "$fixtures/enum.rs" <<'EOF'
#[derive(Clone, Copy, Debug, PartialEq)]
enum TicketSource {
    Backend,
    Demo,
}
EOF
  check_rejects "a Backend/Demo source enum" "$fixtures/enum.rs"

  cat > "$fixtures/branch.rs" <<'EOF'
    let rows = match fetch().await {
        Ok(page) => (page.data, TicketSource::Backend),
        Err(_) => (Vec::new(), TicketSource::Demo),
    };
EOF
  check_rejects "a demo arm on a failed fetch" "$fixtures/branch.rs"

  cat > "$fixtures/banner.rs" <<'EOF'
        if fetch_failed {
            StatusBanner {
                tone: BannerTone::Warning,
                class: "mb-3",
                "Backend tickets API not reachable - showing demo rows."
            }
        }
EOF
  check_rejects "a banner admitting fabricated rows" "$fixtures/banner.rs"

  cat > "$fixtures/clean.rs" <<'EOF'
    // The page used to fall back to demo rows here; MAPPS-438 removed them.
    let fetch_failed = matches!(*resource_snapshot, Some(None));

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load tickets. Refresh the page to retry." }
        }
EOF
  out=$("$0" "$fixtures/clean.rs" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (the migrated error-banner form was rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: the migrated form and a comment about the old fallback pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "demo-row guard self-test: clean"
  exit "$status"
fi

if [ "$#" -gt 0 ]; then
  files=("$@")
else
  mapfile -t files < <(find src/pages -name '*.rs' | sort)
fi

hits=$(
  awk '
    {
      code = $0
      sub(/^[ \t]*/, "", code)
      if (code ~ /^\/\//) next
      if ($0 ~ /(^|[^A-Za-z0-9_])Demo([^A-Za-z0-9_]|$)/)
        print FILENAME ":" FNR ": demo row source: " code
      else if (tolower($0) ~ /demo rows/)
        print FILENAME ":" FNR ": copy admits the rows are fabricated: " code
    }
  ' "${files[@]}"
)

if [ -n "$hits" ]; then
  echo "demo-row guard: FAIL (a page renders rows the backend did not return)"
  echo "Render the failure instead of inventing rows:"
  echo '  Err(_) => (Vec::new(), TicketSource::Demo)'
  echo '  ->  let fetch_failed = matches!(*snapshot, Some(None));'
  echo '      if fetch_failed { ErrorBanner { class: "mb-3", "Could not load ..." } }'
  printf '%s\n' "$hits"
  exit 1
fi

echo "demo-row guard: clean"
