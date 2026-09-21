#!/usr/bin/env bash
# MAPPS-917 guard: the portal and contact refresh tokens are never written to
# localStorage. They live in sessionStorage (tab-scoped) until an HttpOnly
# cookie flow exists. Usage: check-refresh-token-storage.sh [--self-test]
set -u
cd "$(dirname "$0")/.." || exit 2

FILE='src/hooks/fetch.rs'


# A localStorage handle is only acceptable in the functions that do not touch
# a refresh key, so flag any `local_storage()` within 6 lines of a key constant.
check_file() {
    awk '
        /local_storage\(\)/ { ls[NR] = 1 }
        /(PORTAL|CONTACT)_REFRESH_STORAGE_KEY/ && !/const / { key[NR] = 1 }
        END {
            bad = 0
            for (k in key) for (l in ls) if (l + 0 >= k - 6 && l + 0 <= k + 6) { print "refresh key near local_storage() at line " l; bad = 1 }
            exit bad
        }' "$1"
}

if [ "${1:-}" = "--self-test" ]; then
    tmp=$(mktemp)
    trap 'rm -f "$tmp"' EXIT
    printf 'let s = win.local_storage();\ns.set_item(PORTAL_REFRESH_STORAGE_KEY, v);\n' >"$tmp"
    if check_file "$tmp" >/dev/null; then
        echo "self-test failed: guard accepted a localStorage refresh write" >&2
        exit 1
    fi
    echo "self-test ok"
    exit 0
fi

if ! check_file "$FILE"; then
    echo "MAPPS-917: refresh tokens must use sessionStorage, not localStorage ($FILE)" >&2
    exit 1
fi
echo "refresh-token storage guard ok"
