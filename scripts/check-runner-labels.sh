#!/usr/bin/env bash
# MAPPS-398 runner-label guard.
#
# check.yml compiles Rust on the runner, so it must request the dev label
# (its image ships cc/gcc/ld) instead of installing a C toolchain at run
# time on the base image. This guard fails if either half regresses.
set -u
cd "$(dirname "$0")/.." || exit 2

workflow='.forgejo/workflows/check.yml'
status=0

if ! grep -qF 'vars.RUNS_ON_OPENSUSE_DEV_LATEST' "$workflow"; then
  echo "runner-label guard: FAIL ($workflow must run on RUNS_ON_OPENSUSE_DEV_LATEST)"
  status=1
fi

installs=$(grep -nE '\b(zypper|apt-get|dnf|yum)\b.*install' "$workflow")
if [ -n "$installs" ]; then
  echo "runner-label guard: FAIL ($workflow installs packages at run time)"
  echo "The dev runner image already provides the C toolchain; do not reinstall it."
  printf '%s\n' "$installs"
  status=1
fi

[ "$status" -eq 0 ] && echo "runner-label guard: clean"
exit "$status"
