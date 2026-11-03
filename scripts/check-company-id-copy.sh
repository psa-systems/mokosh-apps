#!/usr/bin/env bash
# MAPPS-650 / PMS-946: fail CI if a fresh "Portal ID" string lands
# under src/pages/contact_portal/ (the customer-facing surface).
# David explicitly asked for "Company ID" as the user-facing name;
# the wire/schema field name `portal_id` legitimately appears in
# comments elsewhere in the tree, so the guard is scoped narrowly.

# SPEC-2.1.1: no-arg invocation, exit 0 clean line / nonzero FAIL on hit.
# SPEC-2.1.3: intentionally no --self-test mode here - there is no synthetic
# fixture to construct for this guard, so this is not a gap.
set -euo pipefail

HITS=$(grep -rn --include='*.rs' -E 'Portal ID|portal ID' src/pages/contact_portal/ || true)

if [ -n "$HITS" ]; then
    echo "ERROR: src/pages/contact_portal/ references 'Portal ID' - use 'Company ID' instead (PMS-946):" >&2
    echo "$HITS" >&2
    exit 1
fi

echo "Company ID copy guard: clean (no 'Portal ID' hits in src/pages/contact_portal/)"
