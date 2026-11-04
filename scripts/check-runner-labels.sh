#!/usr/bin/env bash
# MAPPS-398 / DEV-769 runner-label guard.
#
# check.yml compiles Rust on the runner, so it must request the heavy label
# (dev image with cc/gcc/ld) instead of installing a C toolchain at run time.
# No workflow may use the retired *_LATEST labels. This guard fails on either.
# SPEC-2.1.1: no-arg invocation, exit 0 clean line / nonzero FAIL line.
# SPEC-2.1.3: intentionally no --self-test mode - there is no synthetic
# fixture to construct for this guard, so this is not a gap.
set -u
cd "$(dirname "$0")/.." || exit 2

workflow='.forgejo/workflows/check.yml'
status=0

# SPEC-2.5.4 (1/3): the Rust-build job in check.yml must request the heavy runner label.
if ! grep -qF 'vars.RUNS_ON_OPENSUSE_BASE_HEAVY' "$workflow"; then
  echo "runner-label guard: FAIL ($workflow must run on RUNS_ON_OPENSUSE_BASE_HEAVY)"
  status=1
fi

# SPEC-2.5.4 (2/3): no workflow may reference a retired *_LATEST runner label.
retired=$(grep -nE 'RUNS_ON_OPENSUSE_(BASE|DEV)_LATEST' .forgejo/workflows/*.yml)
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
exit "$status"
