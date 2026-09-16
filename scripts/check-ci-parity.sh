#!/usr/bin/env bash
# MAPPS-534 CI-parity guard.
#
# `.forgejo/workflows/check.yml` says it "Mirrors the local `just check` +
# `just test` recipes". It had drifted from `just check` in four places, and
# nothing in the repo could tell:
#
#   - check-email-affordance was absent entirely;
#   - check-desktop was absent, so CI had never once compiled the desktop
#     feature combination;
#   - check-kit-adoption and check-theme-tokens ran without their --self-test,
#     dropping "so a guard that stopped guarding fails loudly" in the one place
#     that is the enforcement boundary.
#
# All four accumulated while every sibling guard was added correctly, so the
# cause is not carelessness a reminder fixes: adding a guard means remembering
# two files, and the workflow is 170-odd lines of hand-maintained steps.
#
# This compares COMMAND LINES, not recipe names. A name comparison cannot see
# the third and fourth cases, where the recipe is present in CI but invoked
# differently, and half of what was wrong here was exactly that.
#
# What fails: a command line in a `just check` recipe that appears in no `run:`
# block of check.yml. What passes: every one of them appears.
#
# Not in scope: `just test`, and recipes outside the `check` dependency list.
# Steps CI runs that no recipe asks for are fine and are not reported - CI does
# checkout, caching and a CSS stub that a local run has no need of.
#
# Usage: check-ci-parity.sh [--self-test | --compare JUSTFILE WORKFLOW]
#   No argument checks this repository's own pair.
#   `--compare` is the pure comparison, split out so `--self-test` can prove
#   offline that the guard still catches a dropped command and still accepts a
#   complete workflow. A parity guard that quietly matches nothing reports
#   clean forever, which is the failure mode this file exists to prevent.
set -u
cd "$(dirname "$0")/.." || exit 2

# The recipes `just check` depends on, read off its dependency line.
check_recipes() {
  awk '
    /^check:/ {
      sub(/^check:/, "")
      n = split($0, parts, " ")
      for (i = 1; i <= n; i++) {
        if (parts[i] ~ /^check-/) print parts[i]
      }
      exit
    }
  ' "$1"
}

# Every command line in a recipe body: the indented lines between the recipe
# header and the next unindented line, minus comments and blanks. A body
# opening with a shebang (`#!/usr/bin/env nu`, etc.) is a script, not a
# sequence of shell commands, so its lines cannot be matched against
# `workflow`'s `run:` blocks one for one; what CI must run instead is the
# recipe itself, so this reports a single synthetic `just <recipe>` command.
recipe_commands_in() {
  local file="$1" recipe="$2" first
  first=$(awk -v recipe="$recipe" '
    $0 == recipe ":" { found = 1; next }
    found && /^[ \t]*$/ { next }
    found { sub(/^[ \t]+/, ""); print; exit }
  ' "$file")
  if [ "${first#\#!}" != "$first" ]; then
    printf 'just %s\n' "$recipe"
    return
  fi
  awk -v recipe="$recipe" '
    $0 == recipe ":" { found = 1; next }
    found && /^[^ \t]/ { exit }
    found && /^[ \t]*#/ { next }
    found && /^[ \t]*$/ { next }
    found {
      sub(/^[ \t]+/, "")
      print
    }
  ' "$file"
}

# Same, but falls back to the files the justfile imports (one level:
# common.just does not nest further) when the recipe is not defined locally.
# check-justfile is common-owned and lives in common.just, not the root
# justfile, so a recipe search scoped to the root file alone would report it
# missing and fail the guard on every repo that depends on it.
recipe_commands() {
  local justfile="$1" recipe="$2" dir imp out
  out=$(recipe_commands_in "$justfile" "$recipe")
  if [ -n "$out" ]; then
    printf '%s\n' "$out"
    return
  fi
  dir=$(dirname "$justfile")
  for imp in $(grep "^import " "$justfile" 2>/dev/null | sed "s/^import '\(.*\)'/\1/"); do
    out=$(recipe_commands_in "$dir/$imp" "$recipe")
    if [ -n "$out" ]; then
      printf '%s\n' "$out"
      return
    fi
  done
}

compare() {
  local justfile="$1" workflow="$2" recipes missing=0 total=0

  for f in "$justfile" "$workflow"; do
    if [ ! -f "$f" ]; then
      echo "CI-parity guard: FAIL (no such file: $f)"
      return 2
    fi
  done

  recipes=$(check_recipes "$justfile")
  if [ -z "$recipes" ]; then
    echo "CI-parity guard: FAIL (no 'check:' recipe with check-* dependencies in $justfile)"
    echo "The guard could not read what it is meant to compare, which is not a pass."
    return 2
  fi

  for recipe in $recipes; do
    local commands
    commands=$(recipe_commands "$justfile" "$recipe")
    if [ -z "$commands" ]; then
      echo "CI-parity guard: FAIL (recipe $recipe has no commands in $justfile)"
      echo "Either the recipe is missing or this guard has stopped parsing the justfile."
      return 2
    fi
    while IFS= read -r cmd; do
      total=$((total + 1))
      if ! grep -qF -- "$cmd" "$workflow"; then
        if [ "$missing" -eq 0 ]; then
          echo "CI-parity guard: FAIL ($workflow does not run everything 'just check' runs)"
        fi
        missing=$((missing + 1))
        echo "  [$recipe] $cmd"
      fi
    done <<< "$commands"
  done

  if [ "$missing" -gt 0 ]; then
    echo "Add a step to $workflow running each line above. CI is the enforcement"
    echo "boundary; a check that only runs locally is opt-in."
    return 1
  fi

  echo "CI-parity guard: clean ($total command(s) across $(printf '%s\n' "$recipes" | wc -l | tr -d ' ') recipes, all present in $workflow)"
  return 0
}

case "${1:-}" in
  --compare)
    if [ $# -ne 3 ]; then
      echo "CI-parity guard: FAIL (--compare needs JUSTFILE and WORKFLOW)"
      exit 2
    fi
    compare "$2" "$3"
    exit $?
    ;;
  --self-test)
    fixtures=$(mktemp -d) || exit 2
    trap 'rm -rf "$fixtures"' EXIT
    status=0

    cat > "$fixtures/justfile" <<'EOF'
[group: 'check']
check: check-alpha check-beta

# A comment inside the body, which is not a command.
[group: 'check']
check-alpha:
    bash scripts/check-alpha.sh --self-test
    bash scripts/check-alpha.sh

check-beta:
    cargo check --features desktop
EOF

    cat > "$fixtures/complete.yml" <<'EOF'
      - name: Alpha guard
        run: |
          bash scripts/check-alpha.sh --self-test
          bash scripts/check-alpha.sh
      - name: Beta
        run: cargo check --features desktop
EOF

    # The real shape of the MAPPS-534 drift: the guard is there, its self-test
    # is not.
    cat > "$fixtures/no-selftest.yml" <<'EOF'
      - name: Alpha guard
        run: bash scripts/check-alpha.sh
      - name: Beta
        run: cargo check --features desktop
EOF

    cat > "$fixtures/missing.yml" <<'EOF'
      - name: Alpha guard
        run: |
          bash scripts/check-alpha.sh --self-test
          bash scripts/check-alpha.sh
EOF

    printf 'not-a-justfile:\n' > "$fixtures/empty.justfile"

    # check-justfile's real shape: a shebang-script recipe defined in an
    # imported file, not the root justfile.
    cat > "$fixtures/imported.just" <<'EOF'
check-gamma:
    #!/usr/bin/env nu
    let x = 1
    print $x
EOF

    cat > "$fixtures/import.justfile" <<'EOF'
import 'imported.just'

[group: 'check']
check: check-gamma
EOF

    cat > "$fixtures/imported-complete.yml" <<'EOF'
      - name: Gamma guard
        run: just check-gamma
EOF

    cat > "$fixtures/imported-missing.yml" <<'EOF'
      - name: Nothing relevant
        run: echo hi
EOF

    out=$("$0" --compare "$fixtures/justfile" "$fixtures/complete.yml" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "self-test: FAIL (a complete workflow was rejected, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a workflow running everything passes"
    fi

    out=$("$0" --compare "$fixtures/justfile" "$fixtures/missing.yml" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (a dropped recipe did not fail the guard, exit $rc)"
      printf '%s\n' "$out"
      status=1
    elif ! printf '%s' "$out" | grep -q 'cargo check --features desktop'; then
      echo "self-test: FAIL (the failure did not name the missing command)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a dropped recipe fails the guard and names the command"
    fi

    out=$("$0" --compare "$fixtures/justfile" "$fixtures/no-selftest.yml" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (a dropped --self-test did not fail the guard, exit $rc)"
      printf '%s\n' "$out"
      status=1
    elif ! printf '%s' "$out" | grep -q -- '--self-test'; then
      echo "self-test: FAIL (the failure did not name the dropped --self-test)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a recipe run without its --self-test fails the guard"
    fi

    out=$("$0" --compare "$fixtures/empty.justfile" "$fixtures/complete.yml" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 2 ]; then
      echo "self-test: FAIL (an unparseable justfile did not error, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: an unparseable justfile is an error, not a pass"
    fi

    out=$("$0" --compare "$fixtures/import.justfile" "$fixtures/imported-complete.yml" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "self-test: FAIL (a shebang recipe defined in an imported file was rejected, exit $rc)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a shebang recipe in an imported file matches 'just <recipe>'"
    fi

    out=$("$0" --compare "$fixtures/import.justfile" "$fixtures/imported-missing.yml" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "self-test: FAIL (a missing 'just check-gamma' step did not fail the guard, exit $rc)"
      printf '%s\n' "$out"
      status=1
    elif ! printf '%s' "$out" | grep -q 'just check-gamma'; then
      echo "self-test: FAIL (the failure did not name the missing 'just check-gamma')"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a dropped 'just check-gamma' step fails the guard and names it"
    fi

    [ "$status" -eq 0 ] && echo "CI-parity guard self-test: clean"
    exit "$status"
    ;;
  "")
    ;;
  *)
    echo "CI-parity guard: FAIL (unknown argument: $1)"
    exit 2
    ;;
esac

compare justfile .forgejo/workflows/check.yml
exit $?
