#!/usr/bin/env bash
# MAPPS-432 guard: /auth/callback must classify a failed exchange on the
# `FlowError` variant, never on its rendered message. The prose match this
# replaces (`e.starts_with("storage:")`) meant rewording an `#[error(...)]`
# attribute silently changed auth behaviour with no compile error.
set -u
cd "$(dirname "$0")/.." || exit 2

page="src/pages/auth_callback.rs"
status=0

if hits=$(grep -nE '(starts_with|ends_with)\(|\.contains\("' "$page"); then
  echo "auth-error-prose guard: FAIL (string match on an error message in $page)"
  echo "$hits"
  echo "Classify with modules::oidc::classify_flow_error on the FlowError variant instead."
  status=1
fi

if ! grep -q 'classify_flow_error' "$page"; then
  echo "auth-error-prose guard: FAIL ($page no longer calls classify_flow_error)"
  status=1
fi

[ "$status" -eq 0 ] && echo "auth-error-prose guard: clean"
exit "$status"
