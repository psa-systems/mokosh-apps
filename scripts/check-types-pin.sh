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
# MAPPS-537 narrowed what "moved" means. With no `rev` on the dependency, cargo
# resolves to the head of mokosh-server's default branch, so the original rule
# went red on ANY commit over there, not only one that touched the crate: over
# the window the stale pin covered, 214 server commits produced 6 that touched
# crates/mokosh-types, so ~97% of the failures named a revision pair whose crate
# diff was empty. Worse than the noise, it was a race - mokosh-server merges
# every couple of hours, so a reviewed, green mokosh-apps PR went red because
# another repository merged a docs typo, and the re-bump raced the same way.
# MAPPS-532 paid that tax twice inside an hour.
#
# So a lock move now fails only when crates/mokosh-types actually differs
# between the pinned revision and the head. A catch-up-only move passes with a
# note naming the distance. The rule this gives up - that the pin cannot
# silently accumulate distance - is kept by types-pin-drift.yml, a weekly
# scheduled run of `--strict`, which is allowed to be red without blocking
# anyone's PR.
#
# An unreadable mirror is NOT a pass. If the crate diff cannot be computed the
# guard fails, because "I could not tell" and "nothing changed" are the same
# green check, and this file exists because a guard that quietly stops guarding
# reports clean forever.
#
# Usage: check-types-pin.sh [--strict | --self-test | --compare OLD NEW [STATE]]
#   No argument runs the real check against Cargo.lock (needs network).
#   `--strict` restores the pre-MAPPS-537 rule: any lock move fails. The weekly
#   drift workflow runs this.
#   `--compare` is the pure comparison step, split out so `--self-test` can
#   prove offline that the guard still rejects a real crate change, still
#   accepts a catch-up-only move, and still refuses to guess. STATE is
#   `changed`, `unchanged` or `unknown` (default `unknown`), which is what the
#   real run reads off the cargo git mirror.
set -u
cd "$(dirname "$0")/.." || exit 2

crate="mokosh-types"

# MAPPS-537: 1 restores the pre-MAPPS-537 rule, where any lock move fails.
# Set by --strict, which only the weekly drift workflow passes.
strict=0

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

# The bare mirror cargo already fetched, or empty when there is none.
mirror_db() {
  ls -d "${CARGO_HOME:-$HOME/.cargo}"/git/db/mokosh-server-* 2>/dev/null | head -1
}

# MAPPS-537: does crates/mokosh-types actually differ between the two
# revisions? Prints `changed`, `unchanged`, or `unknown` when the mirror cannot
# answer. `unknown` is a failure upstream, never a pass: the whole point of the
# narrowed rule is that it can tell the two cases apart, so it has to admit
# when it cannot.
crate_state() {
  local old="$1" new="$2" db rc
  db=$(mirror_db)
  [ -z "$db" ] && { echo unknown; return; }
  git -C "$db" cat-file -e "${old}^{commit}" 2>/dev/null || { echo unknown; return; }
  git -C "$db" cat-file -e "${new}^{commit}" 2>/dev/null || { echo unknown; return; }
  git -C "$db" diff --quiet "$old" "$new" -- "crates/$crate" 2>/dev/null
  rc=$?
  # `git diff --quiet` says 0 for no difference and 1 for a difference;
  # anything else is git failing, which is not an answer.
  case "$rc" in
    0) echo unchanged ;;
    1) echo changed ;;
    *) echo unknown ;;
  esac
}

# How far behind the pin is, in server commits. Empty when unknowable.
catch_up_distance() {
  local old="$1" new="$2" db
  db=$(mirror_db)
  [ -z "$db" ] && return
  git -C "$db" rev-list --count "$old..$new" 2>/dev/null
}

# Best-effort commit list, read from the bare mirror cargo already fetched.
# Absence is reported, never swallowed: the revisions above are the finding,
# and this is the detail that makes it readable.
print_commits() {
  local old="$1" new="$2" db
  db=$(mirror_db)
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
  local old="$1" new="$2" state="${3:-unknown}" old_rev new_rev distance

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

  if [ "$old_rev" = "$new_rev" ]; then
    echo "$crate pin guard: FAIL (cargo update --package $crate moved Cargo.lock)"
    echo "  pinned revision: $old_rev"
    echo "  server head:     $new_rev"
    echo "The pin itself did not move, so the change is elsewhere in the lock:"
    diff -u "$old" "$new" | sed 's/^/  /'
    return 1
  fi

  distance=$(catch_up_distance "$old_rev" "$new_rev")

  # MAPPS-537: the pin is behind, but only on commits that cannot reach this
  # build. Say so and pass. The weekly types-pin-drift.yml run is what stops
  # the distance below from growing back into the 214 commits MAPPS-525 found.
  if [ "$state" = "unchanged" ] && [ "$strict" -eq 0 ]; then
    echo "$crate pin guard: clean (catch-up only, crates/$crate is unchanged)"
    echo "  pinned revision: $old_rev"
    echo "  server head:     $new_rev"
    echo "  behind by:       ${distance:-unknown} mokosh-server commit(s), none touching crates/$crate"
    return 0
  fi

  echo "$crate pin guard: FAIL (cargo update --package $crate moved Cargo.lock)"
  echo "  pinned revision: $old_rev"
  echo "  server head:     $new_rev"
  echo "  behind by:       ${distance:-unknown} mokosh-server commit(s)"

  case "$state" in
    changed)
      echo "crates/$crate changed in between:"
      print_commits "$old_rev" "$new_rev"
      ;;
    unchanged)
      # Only reachable under --strict: the weekly drift run, whose job is to
      # notice accumulated distance that no PR was ever obliged to close.
      echo "crates/$crate is unchanged; this is the scheduled catch-up check,"
      echo "not a wire-format change. It does not gate any pull request."
      ;;
    *)
      echo "Whether crates/$crate changed could not be determined from the cargo"
      echo "git mirror, so this fails rather than guessing: an unreadable mirror"
      echo "and an unchanged crate must not produce the same green check."
      ;;
  esac
  echo "Commit the bump with this PR:"
  echo "  cargo update --package $crate"
  return 1
}

if [ "${1:-}" = "--strict" ]; then
  strict=1
  shift
fi

case "${1:-}" in
  --compare)
    if [ $# -lt 3 ] || [ $# -gt 4 ]; then
      echo "$crate pin guard: FAIL (--compare needs OLD_LOCK NEW_LOCK [STATE])"
      exit 2
    fi
    compare "$2" "$3" "${4:-unknown}"
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

    out=$("$0" --compare "$fixtures/stale.lock" "$fixtures/head.lock" changed 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (a crate change did not fail the guard, exit $rc)"
      printf '%s\n' "$out"
      status=1
    elif ! printf '%s' "$out" | grep -q 30cde300 || ! printf '%s' "$out" | grep -q ded6b047; then
      echo "self-test: FAIL (the failure did not name both revisions)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a crate change fails the guard and names both revisions"
    fi

    # MAPPS-537: the narrowed rule. A pin that is behind on commits which
    # cannot reach this build passes, and says how far behind it is.
    out=$("$0" --compare "$fixtures/stale.lock" "$fixtures/head.lock" unchanged 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "self-test: FAIL (a catch-up-only move was rejected, exit $rc)"
      printf '%s\n' "$out"
      status=1
    elif ! printf '%s' "$out" | grep -q "behind by"; then
      echo "self-test: FAIL (the catch-up pass did not report the distance)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a catch-up-only move passes and reports the distance"
    fi

    # The half that keeps the narrowed rule honest: an edit that neuters the
    # crate-diff check leaves `unknown` behind, and `unknown` must never pass.
    out=$("$0" --compare "$fixtures/stale.lock" "$fixtures/head.lock" unknown 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (an undeterminable crate diff did not fail, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: an undeterminable crate diff fails rather than guessing"
    fi

    # --strict is the weekly drift run: the pre-MAPPS-537 rule, where even a
    # catch-up-only move is a finding.
    out=$("$0" --strict --compare "$fixtures/stale.lock" "$fixtures/head.lock" unchanged 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (--strict passed a catch-up-only move, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: --strict still fails a catch-up-only move"
    fi

    out=$("$0" --strict --compare "$fixtures/stale.lock" "$fixtures/same.lock" unchanged 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "self-test: FAIL (an unmoved pin was rejected, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: an unmoved pin passes the guard, even under --strict"
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

# Read the crate diff off the mirror `cargo update` just refreshed, so the
# comparison below can tell a wire-format change from a catch-up (MAPPS-537).
state=$(crate_state \
  "$(pinned_rev "$work/Cargo.lock.orig")" \
  "$(pinned_rev "$work/Cargo.lock.new")")

compare "$work/Cargo.lock.orig" "$work/Cargo.lock.new" "$state"
exit $?
