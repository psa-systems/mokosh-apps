#!/usr/bin/env bash
# MAPPS-398 / DEV-769 runner-label guard.
#
# check.yml compiles Rust on the runner, so it must request the heavy label
# (dev image with cc/gcc/ld) instead of installing a C toolchain at run time.
# No workflow may use the retired *_LATEST labels. This guard fails on either.
# SPEC-2.1.1: no-arg invocation, exit 0 clean line / nonzero FAIL line.
# SPEC-2.1.3: --self-test runs the checks against generated workflows.
#
# Usage: check-runner-labels.sh [--self-test | WORKFLOW_DIR]
#   WORKFLOW_DIR defaults to .forgejo/workflows; the self-test points it at
#   fixtures.
set -u
cd "$(dirname "$0")/.." || exit 2

# The `runs-on:` line of the job named <job> in <workflow>, or nothing.
job_runs_on() {
  awk -v want="$2" '
    /^[A-Za-z]/ { in_jobs = ($0 ~ /^jobs:/); next }
    in_jobs && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { job = $1; sub(/:$/, "", job); next }
    in_jobs && job == want && /^    runs-on:/ { print; exit }
  ' "$1"
}

run_guard() {
  local dir="$1" workflow="$1/check.yml" status=0 heavy retired installs
  if [ ! -f "$workflow" ]; then
    echo "runner-label guard: CANNOT EVALUATE ($workflow does not exist)"
    return 2
  fi

  # SPEC-2.5.4 (1/3): the Rust-build job in check.yml, `cargo`, must request
  # the heavy runner label. Scoped to that job: a grep of the whole file
  # passed as long as the string appeared anywhere, in any job (MAPPS-1224).
  heavy="$(job_runs_on "$workflow" cargo)"
  if [ -z "$heavy" ]; then
    echo "runner-label guard: CANNOT EVALUATE ($workflow has no \`cargo\` job with a runs-on)"
    return 2
  fi
  if ! printf '%s' "$heavy" | grep -qF 'vars.RUNS_ON_OPENSUSE_BASE_HEAVY'; then
    echo "runner-label guard: FAIL ($workflow job \`cargo\` must run on RUNS_ON_OPENSUSE_BASE_HEAVY: $heavy)"
    status=1
  fi

  # SPEC-2.5.4 (2/3): no workflow may reference a retired *_LATEST runner label.
  retired=$(grep -nE 'RUNS_ON_OPENSUSE_(BASE|DEV)_LATEST' "$dir"/*.yml)
  if [ -n "$retired" ]; then
    echo "runner-label guard: FAIL (retired *_LATEST label; use BASE_HEAVY or BASE_MEDIUM)"
    printf '%s\n' "$retired"
    status=1
  fi

  # SPEC-2.5.4 (3/3): check.yml must not install OS packages at run time.
  installs=$(grep -nE '\b(zypper|apt-get|dnf|yum)\b.*install' "$workflow")
  if [ -n "$installs" ]; then
    echo "runner-label guard: FAIL ($workflow installs packages at run time)"
    echo "The dev runner image already provides the C toolchain; do not reinstall it."
    printf '%s\n' "$installs"
    status=1
  fi

  [ "$status" -eq 0 ] && echo "runner-label guard: clean"
  return "$status"
}

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  # <case> <expected exit> <cargo job's label> <another job's label>
  check_case() {
    rm -f "$fixtures"/*.yml
    cat > "$fixtures/check.yml" <<EOF
name: Check
on:
  push:
jobs:
  guards:
    runs-on: \${{ vars.$4 }}
  cargo:
    runs-on: \${{ vars.$3 }}
EOF
    run_guard "$fixtures" >/dev/null
    local got=$?
    if [ "$got" -eq "$2" ]; then
      echo "runner-label guard: self-test case ok ($1)"
    else
      echo "runner-label guard: SELF-TEST FAIL ($1: exit $got, expected $2)"
      status=1
    fi
  }
  check_case "cargo on heavy" 0 RUNS_ON_OPENSUSE_BASE_HEAVY RUNS_ON_OPENSUSE_BASE_MEDIUM
  check_case "heavy only on another job" 1 RUNS_ON_OPENSUSE_BASE_MEDIUM RUNS_ON_OPENSUSE_BASE_HEAVY
  check_case "retired label" 1 RUNS_ON_OPENSUSE_BASE_HEAVY RUNS_ON_OPENSUSE_BASE_LATEST

  sed -i 's/^  cargo:/  compile:/' "$fixtures/check.yml"
  run_guard "$fixtures" >/dev/null
  if [ "$?" -eq 2 ]; then
    echo "runner-label guard: self-test case ok (no cargo job)"
  else
    echo "runner-label guard: SELF-TEST FAIL (no cargo job: expected exit 2)"
    status=1
  fi
  rm -f "$fixtures/check.yml"
  run_guard "$fixtures" >/dev/null
  if [ "$?" -eq 2 ]; then
    echo "runner-label guard: self-test case ok (no check.yml)"
  else
    echo "runner-label guard: SELF-TEST FAIL (no check.yml: expected exit 2)"
    status=1
  fi

  [ "$status" -eq 0 ] && echo "runner-label guard: self-test OK"
  exit "$status"
fi

run_guard "${1:-.forgejo/workflows}"
exit $?
