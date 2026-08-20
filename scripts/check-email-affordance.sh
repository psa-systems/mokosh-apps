#!/usr/bin/env bash
# MAPPS-482 guard: every action that makes the server email someone is marked
# as one, and offers the operator a preview before they commit to it.
#
# docs/email-actions.md: a trigger whose click mails a client or a colleague
# renders `MailIcon` inside the button and an `EmailPreview` beside it. Three
# such triggers existed when this landed and none of them looked any different
# from an action that writes a row. The fourth, added six months from now,
# would not either - which is what this guard stops.
#
# The list of email-sending API paths below is maintained BY HAND. There is no
# way to derive it: whether an endpoint sends mail is a fact about mokosh-server,
# not about this repo's source. Adding an email-sending endpoint means adding it
# here, and docs/email-actions.md says so.
#
# Two checks per path:
#   1. every file that calls it renders `MailIcon`,
#   2. every file that calls it renders `EmailPreview`.
# Plus a third that keeps the list itself honest: a path nothing calls any more
# fails loudly rather than sitting in the list guarding nothing.
#
# Usage: check-email-affordance.sh [ROOT | --self-test]
#   ROOT defaults to `src`. `--self-test` re-runs the guard over generated
#   fixtures to prove it still rejects a call site missing the icon and one
#   missing the preview, and still accepts a compliant one, so a future edit
#   cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

# "<extended regex matching the path in Rust source>|<human description>"
#
# The patterns carry the literal opening quote of the Rust string so they match
# a call and not a route or a redirect target that happens to share the prefix
# (`/invitations/accept?token=...` in src/lib.rs is the SPA's own route).
EMAIL_PATHS=(
  '"/form-request-links|POST /form-request-links (emails a client a request-form link)'
  '"/invitations"|POST /invitations (emails a colleague an invitation)'
  '"/quotes/\{[A-Za-z_]+\}/send"|POST /quotes/{id}/send (emails the billing contact a sign-off link)'
)

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  # A clean tree: one file per path, each with both affordances. Every
  # rejection case is this tree with one thing taken away.
  build_clean() {
    rm -rf "$fixtures/src"
    mkdir -p "$fixtures/src/pages"
    {
      printf '    post_authed_typed::<RequestLink, _>("/form-request-links", &req)\n'
      printf '    MailIcon { size: IconSize::Small, class: "mr-2".to_string() }\n'
      printf '    EmailPreview { event_type: "forms.request_link".to_string(), context: ctx }\n'
    } > "$fixtures/src/pages/request_links.rs"
    {
      printf '    post_authed::<Created, _>("/invitations", &body)\n'
      printf '    MailIcon { size: IconSize::Small, class: "mr-2".to_string() }\n'
      printf '    EmailPreview { event_type: "invitations.created".to_string(), context: ctx }\n'
    } > "$fixtures/src/pages/team.rs"
    {
      printf '    post_authed::<QuoteResponse, _>(&format!("/quotes/{quote_id}/send"), &empty)\n'
      printf '    MailIcon { size: IconSize::Small, class: "mr-2".to_string() }\n'
      printf '    EmailPreview { event_type: "quote.sent".to_string(), context: ctx }\n'
    } > "$fixtures/src/pages/quotes.rs"
  }

  check_rejects() { # check_rejects <name>
    local out rc
    out=$("$0" "$fixtures/src" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
      echo "self-test: FAIL ($1 did not fail the guard)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: $1 fails the guard (exit $rc)"
    fi
  }

  build_clean
  grep -v 'MailIcon' "$fixtures/src/pages/quotes.rs" > "$fixtures/quotes.tmp"
  mv "$fixtures/quotes.tmp" "$fixtures/src/pages/quotes.rs"
  check_rejects "a send button with no MailIcon"

  build_clean
  grep -v 'EmailPreview' "$fixtures/src/pages/team.rs" > "$fixtures/team.tmp"
  mv "$fixtures/team.tmp" "$fixtures/src/pages/team.rs"
  check_rejects "a send action with no EmailPreview"

  build_clean
  rm -f "$fixtures/src/pages/request_links.rs"
  check_rejects "a listed path nothing calls any more"

  build_clean
  out=$("$0" "$fixtures/src" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (a compliant tree was rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a compliant tree passes the guard"
  fi

  [ "$status" -eq 0 ] && echo "email-affordance guard self-test: clean"
  exit "$status"
fi

root="${1:-src}"
status=0

for entry in "${EMAIL_PATHS[@]}"; do
  pattern="${entry%%|*}"
  label="${entry#*|}"

  # Whole-line comments are prose about the path, not a call to it.
  callers=$(grep -rnE "$pattern" "$root" --include='*.rs' \
    | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' \
    | cut -d: -f1 | sort -u)

  if [ -z "$callers" ]; then
    echo "email-affordance guard: FAIL (nothing under $root/ calls $label)"
    echo "The path list in $(basename "$0") has drifted. Remove the entry, or fix the pattern if the call site was renamed."
    status=1
    continue
  fi

  for file in $callers; do
    if ! grep -q 'MailIcon' "$file"; then
      echo "email-affordance guard: FAIL ($file calls $label but renders no MailIcon)"
      echo "Put \`MailIcon { size: IconSize::Small, class: \"mr-2\".to_string() }\` inside the button, before its label."
      status=1
    fi
    if ! grep -q 'EmailPreview' "$file"; then
      echo "email-affordance guard: FAIL ($file calls $label but offers no EmailPreview)"
      echo "Render \`crate::components::EmailPreview\` beside the send trigger, with the event type and the context the form holds."
      status=1
    fi
  done
done

if [ "$status" -ne 0 ]; then
  echo "See docs/email-actions.md."
else
  echo "email-affordance guard: clean"
fi
exit "$status"
