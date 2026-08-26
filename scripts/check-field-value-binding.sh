#!/usr/bin/env bash
# MAPPS-585 guard: a form field's value is an attribute, never a child.
#
# `Textarea` rendered its value as the text child of `<textarea>`:
#
#     textarea { id: "...", oninput: ..., "{props.value}" }
#
# A `<textarea>`'s text child is its DEFAULT value. The browser copies it into
# `.value` only while the element is still "clean"; the first keystroke sets the
# dirty-value flag and from then on writing the child changes `textContent` and
# nothing the user can see. `Input` never had this - it has always passed
# `value: "{props.value}"`, which dioxus maps onto the `.value` PROPERTY.
#
# The cost was the whole Markdown toolbar. Bold, headings, lists, code block,
# link and image all work by rewriting the source and handing it back, so every
# one of them worked on a freshly loaded article and silently stopped the moment
# the author typed a character - which, in an editor, is always. Worse than
# silent: the transform landed in the signal, so the preview showed it, and the
# next keystroke sent the stale DOM text back up through `oninput` and overwrote
# it. Verified in a browser before and after; see MAPPS-585.
#
# What fails: a `textarea { ... }` element in the shared form components whose
# value is passed as a trailing string child, or one with no `value:` attribute
# at all. What passes: `value: "{...}"` among its attributes.
#
# Scope: src/components/form.rs, the shared fields every page uses. A one-off
# `textarea` elsewhere with no programmatic writer is not what broke.
#
# Usage: check-field-value-binding.sh [--self-test]
set -u
cd "$(dirname "$0")/.." || exit 2

TARGET="src/components/form.rs"

run_guard() {
  awk '
    # Track the element we are inside and its brace depth.
    /textarea[[:space:]]*\{/ { in_ta = 1; depth = 0; has_value = 0; child = 0; start = FNR }
    in_ta {
      n = gsub(/\{/, "{"); depth += n
      n = gsub(/\}/, "}"); depth -= n
      if ($0 ~ /^[[:space:]]*value:/) has_value = 1
      # A bare string literal on its own line inside the element is a child.
      if ($0 ~ /^[[:space:]]*"[^"]*"[[:space:]]*$/) child = FNR
      if (depth <= 0) {
        if (child) {
          printf "%s:%d: the textarea passes its value as a CHILD (line %d), which is only the default value\n", FILE, start, child
          bad = 1
        }
        if (!has_value) {
          printf "%s:%d: the textarea has no `value:` attribute, so nothing can set it programmatically\n", FILE, start
          bad = 1
        }
        in_ta = 0
      }
    }
    END { exit (bad ? 1 : 0) }
  ' FILE="$1" "$1"
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d) || exit 2
  trap 'rm -r "$tmp"' EXIT
  status=0

  # The shape that shipped: value as a trailing child.
  cat > "$tmp/bad.rs" <<'EOF'
            textarea {
                id: "{props.name}",
                oninput: move |e: FormEvent| props.oninput.call(sanitized(e)),
                "{props.value}"
            }
EOF
  if run_guard "$tmp/bad.rs" >/dev/null 2>&1; then
    echo "field-value guard: SELF-TEST FAIL (a value passed as a child passed)"
    status=1
  fi

  # No binding at all: nothing can drive the field.
  cat > "$tmp/none.rs" <<'EOF'
            textarea {
                id: "{props.name}",
                oninput: move |e: FormEvent| props.oninput.call(sanitized(e)),
            }
EOF
  if run_guard "$tmp/none.rs" >/dev/null 2>&1; then
    echo "field-value guard: SELF-TEST FAIL (a textarea with no value binding passed)"
    status=1
  fi

  # The fixed shape.
  cat > "$tmp/good.rs" <<'EOF'
            textarea {
                id: "{props.name}",
                value: "{props.value}",
                oninput: move |e: FormEvent| props.oninput.call(sanitized(e)),
            }
EOF
  if ! run_guard "$tmp/good.rs" >/dev/null 2>&1; then
    echo "field-value guard: SELF-TEST FAIL (the corrected shape was rejected)"
    status=1
  fi

  if ! run_guard "$TARGET" >/dev/null 2>&1; then
    echo "field-value guard: SELF-TEST FAIL ($TARGET does not pass)"
    status=1
  fi

  [ "$status" -eq 0 ] && echo "field-value guard: self-test OK"
  exit "$status"
fi

if run_guard "$TARGET"; then
  echo "field-value guard: OK"
  exit 0
fi
echo
echo "A textarea's text child is its DEFAULT value: once the user types, writing"
echo "it changes textContent and nothing visible. Pass \`value: \"{...}\"\` instead,"
echo "the way Input does. MAPPS-585."
exit 1
