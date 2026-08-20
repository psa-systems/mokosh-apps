#!/usr/bin/env bash
# MAPPS-440 guards: three surfaces that had drifted off the shared kit, each
# invisible to the compiler because the offending markup still renders.
# MAPPS-483 adds a fourth, for the same reason.
#
# 1. No DaisyUI class names. `input input-bordered` is DaisyUI syntax and the
#    project has no DaisyUI, so the class emits nothing and the field falls
#    back to the bare-element style. Use `components::form::Input`.
# 2. A full-screen auth surface renders `components::layout::AuthLayout`, which
#    owns the `min-h-screen` shell, the wordmark and the version footer. Only
#    the marketing page, the 404 and the OIDC callback own their own shell.
# 3. The `file:` class recipe lives in `components::form::FileField` and
#    nowhere else, so the three file pickers cannot drift apart again.
# 4. A floating dropdown panel takes its surface from the `.dropdown-panel`
#    class in input.css. Hand-copying `bg-raised` plus `shadow-lg` is what let
#    six of the eleven panels drift onto `ring-1 ring-black/5`, an edge that is
#    invisible on the dark surface, so both spellings fail here.
#
# Usage: check-kit-adoption.sh [ROOT | --self-test]
#   ROOT defaults to `src`. `--self-test` re-runs the guard over generated
#   fixtures to prove it still rejects a hand-rolled dropdown surface and an
#   untokenized black ring, and still accepts the shared class, so a future
#   edit cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  # A clean tree: the shells, the auth pages and the file recipe the first
  # three checks demand, plus a dropdown on the shared class. Each rejection
  # case is this tree with one thing broken.
  build_clean() {
    rm -rf "$fixtures/src"
    mkdir -p "$fixtures/src/pages" "$fixtures/src/components"
    for page in home not_found auth_callback; do
      printf '    div { class: "min-h-screen bg-app", "shell" }\n' \
        > "$fixtures/src/pages/$page.rs"
    done
    for page in login onboarding portal_login portal_set_password request_form; do
      printf '    AuthLayout {\n        "form"\n    }\n' \
        > "$fixtures/src/pages/$page.rs"
    done
    {
      printf '    // wordmark: crate::branding::product_name()\n'
      printf '    VersionFooter {}\n'
      printf '    div { class: "dropdown-panel absolute right-0 mt-2 w-52 z-20 p-1", "menu" }\n'
    } > "$fixtures/src/components/layout.rs"
    {
      printf '    class: "file:mr-3 file:rounded-md",\n'
      printf '    r#type: "file",\n'
    } > "$fixtures/src/components/form.rs"
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
  printf '    div { class: "absolute right-0 rounded-md bg-raised shadow-lg p-2", "menu" }\n' \
    > "$fixtures/src/components/menu.rs"
  check_rejects "a hand-copied dropdown surface"

  build_clean
  printf '    div { class: "dropdown-panel absolute ring-1 ring-black/5", "menu" }\n' \
    > "$fixtures/src/components/menu.rs"
  check_rejects "an untokenized black ring"

  build_clean
  out=$("$0" "$fixtures/src" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (the shared class and the migrated tree were rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: the shared class and the migrated tree pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "kit-adoption guard self-test: clean"
  exit "$status"
fi

root="${1:-src}"
status=0

# --- 1. DaisyUI input classes -------------------------------------------
if hits=$(grep -rnE '\binput-bordered\b' "$root" --include='*.rs'); then
  echo "kit-input guard: FAIL (DaisyUI class the build has no plugin for)"
  echo "It emits no CSS, so the field renders unstyled. Use \`Input\` from components/form.rs."
  printf '%s\n' "$hits"
  status=1
fi

# --- 2. auth shell -------------------------------------------------------
# Pages allowed to own a full-screen shell: marketing, 404 and the OIDC
# callback. Every other full-screen surface goes through AuthLayout.
shell_allowed="$root/pages/home.rs $root/pages/not_found.rs $root/pages/auth_callback.rs"
shell_hits=$(grep -rln 'min-h-screen' "$root/pages" --include='*.rs' | sort)
shell_expected=$(printf '%s\n' $shell_allowed | sort)
if [ "$shell_hits" != "$shell_expected" ]; then
  echo "auth-shell guard: FAIL (min-h-screen owners in $root/pages/ are not the three allowed pages)"
  echo "Expected: $(printf '%s ' $shell_expected)"
  echo "Found:    $(printf '%s ' $shell_hits)"
  echo "A full-screen auth surface renders AuthLayout so it gets the wordmark and the version footer."
  status=1
fi

# Fail if AuthLayout stops supplying either piece, which would make the check
# above pass while the pages it points at lost the branding anyway. MAPPS-509:
# the wordmark is the deployment's brand from runtime config, so the needle is
# the helper call, not the product name it defaults to.
for needle in 'branding::product_name()' 'VersionFooter'; do
  if ! grep -q "$needle" "$root/components/layout.rs"; then
    echo "auth-shell guard: FAIL (AuthLayout no longer renders \`$needle\`)"
    status=1
  fi
done

auth_pages="$root/pages/login.rs $root/pages/onboarding.rs $root/pages/portal_login.rs $root/pages/portal_set_password.rs $root/pages/request_form.rs"
for page in $auth_pages; do
  if ! grep -q 'AuthLayout {' "$page"; then
    echo "auth-shell guard: FAIL ($page does not render AuthLayout)"
    status=1
  fi
done

# --- 3. file input recipe ------------------------------------------------
if hits=$(grep -rn 'file:mr-3' "$root/pages" --include='*.rs'); then
  echo "file-field guard: FAIL (file-input class recipe copied into a page)"
  echo "Use \`FileField\` from components/form.rs, which owns the recipe."
  printf '%s\n' "$hits"
  status=1
fi

recipe=$(grep -c 'file:mr-3' "$root/components/form.rs" || true)
if [ "$recipe" -ne 1 ]; then
  echo "file-field guard: FAIL (components/form.rs carries the file: recipe $recipe times, expected 1)"
  status=1
fi

file_sites=$(grep -rc 'r#type: "file"' "$root" --include='*.rs' | grep -v ':0$' || true)
if [ "$file_sites" != "$root/components/form.rs:1" ]; then
  echo "file-field guard: FAIL (raw \`r#type: \"file\"\` outside FileField)"
  printf '%s\n' "$file_sites"
  status=1
fi

# --- 4. dropdown panel recipe --------------------------------------------
# The surface lives once, in input.css. A class string pairing `bg-raised`
# with `shadow-lg` and no `dropdown-panel` is a hand-rolled copy of it.
panel=$(grep -c '^[[:space:]]*\.dropdown-panel[[:space:]]*{' input.css || true)
if [ "$panel" -ne 1 ]; then
  echo "dropdown-panel guard: FAIL (input.css declares .dropdown-panel $panel times, expected 1)"
  echo "Every floating dropdown takes its surface from that one rule."
  status=1
fi

hits=$(grep -rn 'bg-raised' "$root" --include='*.rs' \
  | grep -F 'shadow-lg' \
  | grep -vF 'dropdown-panel' \
  | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)
if [ -n "$hits" ]; then
  echo "dropdown-panel guard: FAIL (a dropdown surface is hand-copied instead of shared)"
  echo "Use \`dropdown-panel\` and keep only positioning, width, max-height, z-index and padding:"
  echo '  class: "absolute right-0 rounded-md bg-raised shadow-lg ring-1 ring-black/5 p-2"'
  echo '  ->  class: "dropdown-panel absolute right-0 p-2"'
  printf '%s\n' "$hits"
  status=1
fi

# `ring-black/5` is a black ring at 5% opacity: a faint edge on the light
# surface and no edge at all on the dark one. Edges come from `border-line`.
hits=$(grep -rn 'ring-black' "$root" --include='*.rs' \
  | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)
if [ -n "$hits" ]; then
  echo "dropdown-panel guard: FAIL (untokenized black ring; it is invisible on the dark surface)"
  echo "Use \`dropdown-panel\` on a dropdown, or \`border border-line\` on any other surface."
  printf '%s\n' "$hits"
  status=1
fi

[ "$status" -eq 0 ] && echo "kit-adoption guards: clean"
exit "$status"
