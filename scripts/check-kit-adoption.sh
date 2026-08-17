#!/usr/bin/env bash
# MAPPS-440 guards: three surfaces that had drifted off the shared kit, each
# invisible to the compiler because the offending markup still renders.
#
# 1. No DaisyUI class names. `input input-bordered` is DaisyUI syntax and the
#    project has no DaisyUI, so the class emits nothing and the field falls
#    back to the bare-element style. Use `components::form::Input`.
# 2. A full-screen auth surface renders `components::layout::AuthLayout`, which
#    owns the `min-h-screen` shell, the wordmark and the version footer. Only
#    the marketing page, the 404 and the OIDC callback own their own shell.
# 3. The `file:` class recipe lives in `components::form::FileField` and
#    nowhere else, so the three file pickers cannot drift apart again.
set -u
cd "$(dirname "$0")/.." || exit 2

status=0

# --- 1. DaisyUI input classes -------------------------------------------
if hits=$(grep -rnE '\binput-bordered\b' src --include='*.rs'); then
  echo "kit-input guard: FAIL (DaisyUI class the build has no plugin for)"
  echo "It emits no CSS, so the field renders unstyled. Use \`Input\` from components/form.rs."
  printf '%s\n' "$hits"
  status=1
fi

# --- 2. auth shell -------------------------------------------------------
# Pages allowed to own a full-screen shell: marketing, 404 and the OIDC
# callback. Every other full-screen surface goes through AuthLayout.
shell_allowed="src/pages/home.rs src/pages/not_found.rs src/pages/auth_callback.rs"
shell_hits=$(grep -rln 'min-h-screen' src/pages --include='*.rs' | sort)
shell_expected=$(printf '%s\n' $shell_allowed | sort)
if [ "$shell_hits" != "$shell_expected" ]; then
  echo "auth-shell guard: FAIL (min-h-screen owners in src/pages/ are not the three allowed pages)"
  echo "Expected: $(printf '%s ' $shell_expected)"
  echo "Found:    $(printf '%s ' $shell_hits)"
  echo "A full-screen auth surface renders AuthLayout so it gets the wordmark and the version footer."
  status=1
fi

# Fail if AuthLayout stops supplying either piece, which would make the check
# above pass while the pages it points at lost the branding anyway.
for needle in 'Mokosh Platform' 'VersionFooter'; do
  if ! grep -q "$needle" src/components/layout.rs; then
    echo "auth-shell guard: FAIL (AuthLayout no longer renders \`$needle\`)"
    status=1
  fi
done

auth_pages="src/pages/login.rs src/pages/onboarding.rs src/pages/portal_login.rs src/pages/portal_set_password.rs src/pages/request_form.rs"
for page in $auth_pages; do
  if ! grep -q 'AuthLayout {' "$page"; then
    echo "auth-shell guard: FAIL ($page does not render AuthLayout)"
    status=1
  fi
done

# --- 3. file input recipe ------------------------------------------------
if hits=$(grep -rn 'file:mr-3' src/pages --include='*.rs'); then
  echo "file-field guard: FAIL (file-input class recipe copied into a page)"
  echo "Use \`FileField\` from components/form.rs, which owns the recipe."
  printf '%s\n' "$hits"
  status=1
fi

recipe=$(grep -c 'file:mr-3' src/components/form.rs || true)
if [ "$recipe" -ne 1 ]; then
  echo "file-field guard: FAIL (components/form.rs carries the file: recipe $recipe times, expected 1)"
  status=1
fi

file_sites=$(grep -rc 'r#type: "file"' src --include='*.rs' | grep -v ':0$' || true)
if [ "$file_sites" != "src/components/form.rs:1" ]; then
  echo "file-field guard: FAIL (raw \`r#type: \"file\"\` outside FileField)"
  printf '%s\n' "$file_sites"
  status=1
fi

[ "$status" -eq 0 ] && echo "kit-adoption guards: clean"
exit "$status"
