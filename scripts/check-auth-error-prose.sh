#!/usr/bin/env bash
# MAPPS-432 guard: /auth/callback must classify a failed exchange on the
# `FlowError` variant, never on its rendered message. The prose match this
# replaces (`e.starts_with("storage:")`) meant rewording an `#[error(...)]`
# attribute silently changed auth behaviour with no compile error.
# SPEC-2.1.1: no-arg invocation, exit 0 clean line / nonzero FAIL line.
# SPEC-2.1.3: intentionally no --self-test mode - there is no synthetic
# fixture to construct for this guard, so this is not a gap.
set -u
cd "$(dirname "$0")/.." || exit 2

page="src/pages/auth_callback.rs"
status=0

# SPEC-2.5.3 (1/2): independently fail on a raw string match on an error message.
if hits=$(grep -nE '(starts_with|ends_with)\(|\.contains\("' "$page"); then
  echo "auth-error-prose guard: FAIL (string match on an error message in $page)"
  echo "$hits"
  echo "Classify with modules::oidc::classify_flow_error on the FlowError variant instead."
  status=1
fi

# SPEC-2.5.3 (2/2): independently fail on the absence of a classify_flow_error call.
if ! grep -q 'classify_flow_error' "$page"; then
  echo "auth-error-prose guard: FAIL ($page no longer calls classify_flow_error)"
  status=1
fi

[ "$status" -eq 0 ] && echo "auth-error-prose guard: clean"
exit "$status"
