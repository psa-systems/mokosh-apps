#!/usr/bin/env bash
# MAPPS-423 guards: Cancel destinations, and the global pointer-cursor rule.
#
# 1. A create/edit form shared by both modes must send Cancel back to the
#    record being edited (a mode-derived `cancel_route`), never to a fixed
#    list route. Create-only pages legitimately cancel to their list, so the
#    compliant set is pinned below: a new Cancel-in-Link site, or a shared
#    form regressing to a literal route, fails this guard.
# 2. Tailwind v4's preflight sets `button { cursor: default }`. The base-layer
#    rule in input.css is the app's only compensation; fail if it is dropped.
set -u
cd "$(dirname "$0")/.." || exit 2

status=0

# Every `Link { to: <route>, ... "Cancel" }` site, as "file:<route expression>".
# `cancel_route` = mode-aware (shared create/edit form). A literal Route::*List
# = create-only page with no edit route (verified against the Route enum in
# src/lib.rs).
expected=$(
  cat <<'EOF'
src/pages/assets.rs:Route::AssetList {}
src/pages/billing.rs:Route::InvoiceList {}
src/pages/contacts.rs:cancel_route.clone()
src/pages/contacts.rs:cancel_route.clone()
src/pages/contracts.rs:cancel_route.clone()
src/pages/knowledge_base.rs:cancel_route.clone()
src/pages/portal.rs:Route::PortalTicketList {}
src/pages/projects.rs:Route::ProjectList {}
src/pages/quotes.rs:cancel_route.clone()
src/pages/tickets.rs:Route::TicketList {}
src/pages/time.rs:Route::TimeEntryList {}
EOF
)

# Match the `to:` line of a Link whose body reaches "Cancel" within a few
# lines, which is the shape every cancel-navigation site uses.
actual=$(
  for f in src/pages/*.rs; do
    awk -v file="$f" '
      /to: / { pending = $0; sub(/^[ \t]*to: /, "", pending); sub(/,$/, "", pending); count = 0; next }
      pending != "" {
        if ($0 ~ /"Cancel"/) { print file ":" pending; pending = ""; next }
        if (++count > 4) pending = ""
      }
    ' "$f"
  done | sort
)

if [ "$actual" != "$(printf '%s\n' "$expected" | sort)" ]; then
  echo "cancel-route guard: FAIL (Cancel destinations changed)"
  echo "--- expected (pinned) / +++ found:"
  diff <(printf '%s\n' "$expected" | sort) <(printf '%s\n' "$actual") || true
  echo "A shared create/edit form must cancel to the record (cancel_route);"
  echo "update scripts/check-cancel-routes.sh when adding a create-only form."
  status=1
fi

if ! grep -qE '^\s*button:not\(:disabled\),' input.css ||
  ! grep -qE '^\s*cursor: pointer;' input.css; then
  echo "cursor-pointer guard: FAIL (input.css lost the base-layer pointer rule)"
  echo "Tailwind v4 preflight sets button { cursor: default }; the rule restores it."
  status=1
fi

[ "$status" -eq 0 ] && echo "cancel-route + cursor-pointer guards: clean"
exit "$status"
