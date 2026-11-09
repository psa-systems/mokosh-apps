#!/usr/bin/env bash
# MAPPS-1212 guard: a checkbox that asks for confirmation renders the user's
# intent, not only the server's answer.
#
# A checkbox is not a controlled input. The click toggles the DOM node's own
# `checked` property immediately, whatever the vdom carries, so a flow that
# opens a dialog before writing starts from a node that already shows the
# unconfirmed value. If the user cancels, nothing has been written and the box
# is wrong.
#
# Re-rendering does not fix it. `dioxus-html` marks `value` and `selected`
# `volatile`, which makes dioxus rewrite them every render even when unchanged;
# `checked` is not on that list, so the diff takes its `!value_changed &&
# !volatile` early return and emits nothing. Two of these pages were written
# around the opposite belief, one of them with a comment saying that re-reading
# the server would re-tick the box. It cannot.
# `components::form::mapps1212_checkbox_resync_tests` measures both halves.
#
# What works is rendering the intent (`checked: wanted().unwrap_or(<server>)`)
# and dropping it on cancel, so every path has a value transition for the diff
# to carry.
#
# What fails: a `Checkbox { .. }` block that opens a confirmation
# (`confirm_*.set(true)`) and whose `checked:` does not read `wanted()`.
# What passes: that same block once it does, and every checkbox that writes
# straight through, which has nothing to put back.
#
# Usage: check-confirmed-toggle.sh [ROOT | --self-test]   (ROOT defaults to src)
set -u
cd "$(dirname "$0")/.." || exit 2

# Report `<file>:<line>` for every confirming Checkbox block with no intent in
# its `checked:`. Blocks are delimited by the indentation of the `Checkbox {`
# line, which is what rustfmt guarantees about an rsx! child.
# SPEC-2.5.1: the reason the invariant cannot be evaluated under <root>, or
# nothing when it can. `find` failing on a missing root happens inside the
# pipe below, where its status is lost and the empty output read as a pass
# (MAPPS-1224).
cannot_evaluate() {
  if [ ! -d "$1" ]; then
    echo "$1 is not a directory"
  elif [ -z "$(find "$1" -name '*.rs' -type f -print -quit)" ]; then
    echo "$1 holds no .rs file to scan"
  fi
}

run_guard() {
  local root="$1"
  find "$root" -name '*.rs' -type f -print0 | while IFS= read -r -d '' file; do
    awk -v file="$file" '
      # Entering a Checkbox block: remember where, and at what indentation.
      !in_block && /(^|[^A-Za-z0-9_])Checkbox[[:space:]]*\{/ {
        in_block = 1
        start = FNR
        indent = match($0, /[^ ]/) - 1
        confirms = 0
        has_intent = 0
        next
      }
      !in_block { next }

      # The close is a line at the block opener own indentation. rsx! children
      # close that way; anything deeper belongs to a nested element.
      {
        this_indent = match($0, /[^ ]/) - 1
      }
      this_indent == indent && /^[[:space:]]*\}/ {
        if (confirms && !has_intent) {
          printf "%s:%d\n", file, start
        }
        in_block = 0
        next
      }

      # Strip line comments so prose about confirming does not count.
      { line = $0; sub(/[[:space:]]*\/\/.*$/, "", line) }

      line ~ /confirm[A-Za-z0-9_]*\.set\(true\)/ { confirms = 1 }
      line ~ /checked:/ && line ~ /wanted\(\)/ { has_intent = 1 }
    ' "$file"
  done
}

if [ "${1:-}" = "--self-test" ]; then
  status=0
  tmp="$(mktemp --directory)"
  trap 'rm -rf "$tmp"' EXIT

  # The defect: confirmation asked for, server value rendered.
  cat > "$tmp/bad.rs" <<'EOF'
fn page() -> Element {
    rsx! {
        Card {
            Checkbox {
                name: "enabled",
                label: "Allow this",
                checked: data.enabled,
                onchange: move |e: FormEvent| {
                    if e.checked() {
                        write_enabled(true);
                    } else {
                        confirm_off.set(true);
                    }
                },
            }
        }
    }
}
EOF
  if [ -z "$(run_guard "$tmp")" ]; then
    echo "confirmed-toggle guard: SELF-TEST FAIL (a confirming checkbox bound to the server was not caught)"
    status=1
  fi

  # The same block, rendering the intent.
  cat > "$tmp/bad.rs" <<'EOF'
fn page() -> Element {
    rsx! {
        Card {
            Checkbox {
                name: "enabled",
                label: "Allow this",
                checked: wanted().unwrap_or(data.enabled),
                onchange: move |e: FormEvent| {
                    wanted.set(Some(e.checked()));
                    if e.checked() {
                        write_enabled(true);
                    } else {
                        confirm_off.set(true);
                    }
                },
            }
        }
    }
}
EOF
  if [ -n "$(run_guard "$tmp")" ]; then
    echo "confirmed-toggle guard: SELF-TEST FAIL (an intent-rendering checkbox was rejected)"
    status=1
  fi

  # A checkbox that writes straight through has nothing to put back, and a
  # comment that merely talks about confirming is not a confirmation.
  cat > "$tmp/bad.rs" <<'EOF'
fn page() -> Element {
    rsx! {
        Checkbox {
            name: "enabled",
            label: "Allow this",
            checked: data.enabled,
            // No dialog here: nothing calls confirm_off.set(true).
            onchange: move |e: FormEvent| write_enabled(e.checked()),
        }
    }
}
EOF
  if [ -n "$(run_guard "$tmp")" ]; then
    echo "confirmed-toggle guard: SELF-TEST FAIL (a write-through checkbox was rejected)"
    status=1
  fi

  if [ -n "$(run_guard src)" ]; then
    echo "confirmed-toggle guard: SELF-TEST FAIL (src does not pass)"
    run_guard src
    status=1
  fi

  # SPEC-2.5.1: nothing to check is exit 2, never a pass.
  bash "$0" "$tmp/missing" >/dev/null 2>&1
  if [ "$?" -ne 2 ]; then
    echo "confirmed-toggle guard: SELF-TEST FAIL (a missing root did not exit 2)"
    status=1
  fi
  mkdir -p "$tmp/empty"
  bash "$0" "$tmp/empty" >/dev/null 2>&1
  if [ "$?" -ne 2 ]; then
    echo "confirmed-toggle guard: SELF-TEST FAIL (a root with no .rs file did not exit 2)"
    status=1
  fi

  [ "$status" -eq 0 ] && echo "confirmed-toggle guard: self-test OK"
  exit "$status"
fi

why="$(cannot_evaluate "${1:-src}")"
if [ -n "$why" ]; then
  echo "confirmed-toggle guard: CANNOT EVALUATE ($why)"
  exit 2
fi
hits="$(run_guard "${1:-src}")"
if [ -z "$hits" ]; then
  echo "confirmed-toggle guard: OK"
  exit 0
fi
echo "confirmed-toggle guard: FAIL"
echo "$hits"
echo
echo "This checkbox opens a confirmation, so the browser has already toggled the"
echo "node by the time the dialog appears, and a cancel leaves it showing a value"
echo "nothing wrote. Dioxus does not mark \`checked\` volatile, so a re-render that"
echo "computes the same value writes nothing and cannot put it back. Render the"
echo "intent instead: \`let mut wanted = use_signal(|| None::<bool>);\`,"
echo "\`checked: wanted().unwrap_or(<server value>)\`, set it in \`onchange\`, and"
echo "drop it on cancel and on a failed save. MAPPS-1212."
exit 1
