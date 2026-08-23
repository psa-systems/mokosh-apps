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
# What counts as present is the RENDERED element (`MailIcon {`), not the bare
# name: until MAPPS-539 a bare `grep MailIcon` was satisfied by the import line,
# so a file could import the icon, render none, and pass.
#
# The granularity is the FILE, not the call site, and it is worth knowing what
# that does and does not promise. `src/pages/billing.rs` is 3,700 lines with
# Send, Edit and Void in it; one `MailIcon` anywhere in the file satisfies the
# check, and this script cannot tell which button it sits on. It catches the
# case it was written for - a send trigger added with no email affordance
# anywhere near it - and not a misplaced icon. Making it call-site aware means
# a proximity rule, and the fetch usually sits in a handler while the icon sits
# in the `rsx!`, far apart; that is a separate change with its own false
# positives.
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
#
# MAPPS-539: an entry does not have to be a URL. The invoice send is
# `PUT /invoices/{id}` with `{"status": "sent"}`, a path shared with Edit, Void
# and the invoice delete in src/pages/contacts.rs, none of which send anything -
# so keying on the URL shape would demand these affordances of a file that has
# nothing to do with email. It keys on `invoice_send_path(` instead, a helper
# that exists because the call sends mail.
EMAIL_PATHS=(
  '"/form-request-links|POST /form-request-links (emails a client a request-form link)'
  '"/invitations"|POST /invitations (emails a colleague an invitation)'
  '"/quotes/\{[A-Za-z_]+\}/send"|POST /quotes/{id}/send (emails the billing contact a sign-off link)'
  'invoice_send_path\(|PUT /invoices/{id} status=sent (emails the billing contact a pay-now link)'
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
    # MAPPS-539. Two files, because the point of keying on the helper rather
    # than on `PUT /invoices/{id}` is that the OTHER user of that URL is left
    # alone: contacts.rs calls it to delete an invoice and must not be asked
    # for an email affordance.
    {
      printf '    let path = invoice_send_path(&id_for_send);\n'
      printf '    MailIcon { size: IconSize::Small, class: "mr-2".to_string() }\n'
      printf '    EmailPreview { event_type: "billing.invoice_pay_now".to_string(), context: ctx }\n'
    } > "$fixtures/src/pages/billing.rs"
    {
      printf '    let delete_path = format!("/invoices/{id}");\n'
    } > "$fixtures/src/pages/contacts.rs"
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

  # MAPPS-539: the invoice send is keyed on a helper, so it is guarded like any
  # other entry.
  build_clean
  grep -v 'MailIcon' "$fixtures/src/pages/billing.rs" > "$fixtures/billing.tmp"
  mv "$fixtures/billing.tmp" "$fixtures/src/pages/billing.rs"
  check_rejects "an invoice send with no MailIcon"

  build_clean
  out=$("$0" "$fixtures/src" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (a compliant tree was rejected)"
    printf '%s\n' "$out"
    status=1
  else
    # The clean tree includes a contacts.rs that calls `PUT /invoices/{id}` to
    # DELETE an invoice and carries neither affordance. Passing proves the
    # invoice entry keys on the send helper and not on the URL, which is the
    # whole reason the helper exists (MAPPS-539).
    echo "self-test: a compliant tree passes, and the invoice URL's other caller is left alone"
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
    # MAPPS-539: match the RENDERED element, not the string. `grep -q MailIcon`
    # was satisfied by the `use crate::components::{... MailIcon ...}` import
    # line, so a file could import the icon, render none, and pass.
    if ! grep -qE 'MailIcon[[:space:]]*\{' "$file"; then
      echo "email-affordance guard: FAIL ($file calls $label but renders no MailIcon)"
      echo "Put \`MailIcon { size: IconSize::Small, class: \"mr-2\".to_string() }\` inside the button, before its label."
      status=1
    fi
    if ! grep -qE 'EmailPreview[[:space:]]*\{' "$file"; then
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
