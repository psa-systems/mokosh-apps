#!/usr/bin/env bash
# MAPPS-525 mokosh-types pin guard: the shared-DTO crate is a bare git
# dependency with no `rev`, `tag` or `branch`, so the only thing holding it
# still is the resolved commit in Cargo.lock. Nothing advances that commit,
# and nothing notices when the server moves: a wire-format change merges in
# mokosh-server, this client keeps compiling against the old shape, CI stays
# green, and the divergence only ever surfaces as user-visible behaviour.
# The 2026-08-07 audit measured the pin 92 commits stale, 2026-08-14 measured
# 138, and MAPPS-525 measured 211. This guard turns "the client is behind"
# from invisible into a red check.
#
# What fails: `cargo update --package mokosh-types` moves Cargo.lock. The
# failure prints the pinned revision, the revision on the server's default
# branch, and the mokosh-types commits between them.
#
# What passes: the update is a no-op, i.e. the lock already names the head of
# the server's default branch.
#
# Fixing a failure means committing the bump (run `cargo update --package
# mokosh-types`) together with whatever source changes the new DTOs require,
# so the bump is a reviewed edit rather than a silent one.
#
# Known cost, tracked in MAPPS-537: with no `rev` on the dependency, cargo
# resolves to the head of mokosh-server's default branch, so this fails on any
# commit over there, not only on one that touched the crate. Over the window
# the stale pin covered that was 214 commits, 6 of which touched
# crates/mokosh-types. The output below distinguishes the two cases; MAPPS-537
# is the decision about whether the catch-up case should still fail.
#
# Usage: check-types-pin.sh [--self-test | --compare OLD_LOCK NEW_LOCK]
#   No argument runs the real check against Cargo.lock (needs network).
#   `--compare` is the pure comparison step, split out so `--self-test` can
#   prove offline that the guard still rejects a moved pin and still accepts
#   an unmoved one. A guard that quietly stopped guarding reports clean
#   forever, which is the failure mode this whole file exists to prevent.
set -u
cd "$(dirname "$0")/.." || exit 2

crate="mokosh-types"

# The commit Cargo.lock resolves the crate to, read off the `source` line of
# its `[[package]]` block.
pinned_rev() {
  awk -v crate="$crate" '
    $0 == "name = \"" crate "\"" { found = 1; next }
    found && /^source = "git\+/ {
      line = $0
      sub(/^.*#/, "", line)
      sub(/".*$/, "", line)
      print line
      exit
    }
    found && /^\[\[package\]\]/ { exit }
  ' "$1"
}

# Best-effort commit list, read from the bare mirror cargo already fetched.
# Absence is reported, never swallowed: the revisions above are the finding,
# and this is the detail that makes it readable.
print_commits() {
  local old="$1" new="$2" db
  db=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/git/db/mokosh-server-* 2>/dev/null | head -1)
  if [ -z "$db" ]; then
    echo "  (no cargo git mirror for mokosh-server; commit list unavailable)"
    return
  fi
  local log
  log=$(git -C "$db" log --oneline "$old..$new" -- "crates/$crate" 2>&1)
  if [ $? -ne 0 ]; then
    echo "  (git log over $db failed; commit list unavailable)"
    printf '  %s\n' "$log"
    return
  fi
  if [ -z "$log" ]; then
    echo "  (no commits touched crates/$crate between the two revisions)"
    return
  fi
  printf '%s\n' "$log" | sed 's/^/  /'
}

compare() {
  local old="$1" new="$2" old_rev new_rev

  for f in "$old" "$new"; do
    if [ ! -f "$f" ]; then
      echo "$crate pin guard: FAIL (no such lock file: $f)"
      return 2
    fi
  done

  old_rev=$(pinned_rev "$old")
  new_rev=$(pinned_rev "$new")
  if [ -z "$old_rev" ] || [ -z "$new_rev" ]; then
    echo "$crate pin guard: FAIL (no git revision for $crate in the lock file)"
    echo "The dependency is meant to be a git dependency on mokosh-server."
    echo "  before: ${old_rev:-<none>}"
    echo "  after:  ${new_rev:-<none>}"
    return 2
  fi

  if cmp -s "$old" "$new"; then
    echo "$crate pin guard: clean (pinned at $old_rev, which is the server head)"
    return 0
  fi

  echo "$crate pin guard: FAIL (cargo update --package $crate moved Cargo.lock)"
  echo "  pinned revision: $old_rev"
  echo "  server head:     $new_rev"
  if [ "$old_rev" = "$new_rev" ]; then
    echo "The pin itself did not move, so the change is elsewhere in the lock:"
    diff -u "$old" "$new" | sed 's/^/  /'
  else
    echo "crates/$crate changed in between:"
    print_commits "$old_rev" "$new_rev"
    echo "Commit the bump with this PR:"
    echo "  cargo update --package $crate"
  fi
  return 1
}

case "${1:-}" in
  --compare)
    if [ $# -ne 3 ]; then
      echo "$crate pin guard: FAIL (--compare needs OLD_LOCK and NEW_LOCK)"
      exit 2
    fi
    compare "$2" "$3"
    exit $?
    ;;
  --self-test)
    fixtures=$(mktemp -d) || exit 2
    trap 'rm -rf "$fixtures"' EXIT
    status=0

    lock_at() {
      cat > "$2" <<EOF
[[package]]
name = "mokosh-apps"
version = "0.13.0"
dependencies = [
 "$crate",
]

[[package]]
name = "$crate"
version = "0.1.0"
source = "git+https://dev.a8n.run/psa-systems/mokosh-server.git#$1"
dependencies = [
 "serde",
]
EOF
    }

    lock_at 30cde300a32cb754605a4cca3be38654946a9104 "$fixtures/stale.lock"
    lock_at ded6b047c0ffee0000000000000000000000000d "$fixtures/head.lock"
    lock_at 30cde300a32cb754605a4cca3be38654946a9104 "$fixtures/same.lock"
    printf 'name = "unrelated"\n' > "$fixtures/nopin.lock"

    out=$("$0" --compare "$fixtures/stale.lock" "$fixtures/head.lock" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (a moved pin did not fail the guard, exit $rc)"
      printf '%s\n' "$out"
      status=1
    elif ! printf '%s' "$out" | grep -q 30cde300 || ! printf '%s' "$out" | grep -q ded6b047; then
      echo "self-test: FAIL (the failure did not name both revisions)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a moved pin fails the guard and names both revisions"
    fi

    out=$("$0" --compare "$fixtures/stale.lock" "$fixtures/same.lock" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "self-test: FAIL (an unmoved pin was rejected, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: an unmoved pin passes the guard"
    fi

    out=$("$0" --compare "$fixtures/nopin.lock" "$fixtures/head.lock" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 2 ]; then
      echo "self-test: FAIL (a lock with no $crate git pin did not error, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a lock with no $crate git pin is an error, not a pass"
    fi

    [ "$status" -eq 0 ] && echo "$crate pin guard self-test: clean"
    exit "$status"
    ;;
  "")
    ;;
  *)
    echo "$crate pin guard: FAIL (unknown argument: $1)"
    exit 2
    ;;
esac

if [ ! -f Cargo.lock ]; then
  echo "$crate pin guard: FAIL (no Cargo.lock at the repository root)"
  exit 2
fi

restore() {
  # Always put the committed lock back: this guard reports, it never bumps.
  # A failed restore leaves a bumped lock in the tree, so it is loud.
  if [ -f "$work/Cargo.lock.orig" ] && ! cp "$work/Cargo.lock.orig" Cargo.lock; then
    echo "$crate pin guard: FAIL (could not restore Cargo.lock from $work/Cargo.lock.orig)"
    echo "The working tree may now hold a bumped lock this guard wrote. Check git status."
    rm -rf "${work:?}"
    exit 2
  fi
  rm -rf "${work:?}"
}

work=$(mktemp -d) || exit 2
trap restore EXIT
cp Cargo.lock "$work/Cargo.lock.orig" || exit 2

if ! cargo update --package "$crate" > "$work/update.log" 2>&1; then
  echo "$crate pin guard: FAIL (cargo update --package $crate errored)"
  sed 's/^/  /' "$work/update.log"
  exit 2
fi
cp Cargo.lock "$work/Cargo.lock.new" || exit 2

compare "$work/Cargo.lock.orig" "$work/Cargo.lock.new"
exit $?
