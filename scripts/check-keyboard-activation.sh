#!/usr/bin/env bash
# MAPPS-443 guard: nothing in the SPA announces itself as a button without
# being one. `role="button"` plus `tabindex="0"` on a div puts the element in
# the tab order and makes a screen reader call it a button, but a div gets no
# implicit Enter/Space activation, so the only handler (`onclick`) never fires
# from the keyboard. The three calendar create-targets shipped that way.
# Use a `button { r#type: "button", ... }` element, which is keyboard-operable
# for free; where a click target already contains buttons, cover the area with
# an absolutely-positioned button instead of wrapping it (a button cannot nest
# interactive content).
set -u
cd "$(dirname "$0")/.." || exit 2

status=0

if hits=$(grep -rn 'role: "button"' src/); then
  echo "keyboard-activation guard: FAIL (role=\"button\" on a non-button element)"
  echo "$hits"
  echo "Render a real button element instead; drop role and tabindex, which it carries implicitly."
  status=1
fi

if hits=$(grep -rn 'tabindex: "0"' src/); then
  echo "keyboard-activation guard: FAIL (tabindex=\"0\" hand-rolls the tab order)"
  echo "$hits"
  echo "A button, a link and a form control are focusable already; nothing in this app needs the override."
  status=1
fi

[ "$status" -eq 0 ] && echo "keyboard-activation guard: clean"
exit "$status"
