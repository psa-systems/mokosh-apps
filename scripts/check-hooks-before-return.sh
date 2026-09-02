#!/usr/bin/env bash
# MAPPS-602 guard: every hook in a component runs before every early return.
#
# Dioxus identifies a hook by the ORDER it was called in. A `use_*` call that
# sits after a `return` runs on some renders and not others, so the render that
# takes the early exit leaves the component a hook short and the NEXT render
# panics in dioxus-core with "Unable to retrieve the hook that was initialized
# at this index".
#
# What makes it expensive is what happens after the panic. WASM does not unwind,
# so the runtime is poisoned: no later render lands, the page stops responding
# to clicks, and a save that reaches the database goes on rendering the old
# value. That reads as a stale-data bug, and on the ticket detail page it cost
# three wrong fixes to a resource read that was never at fault.
#
# The rule is positional and mechanical, which is why a script can hold it and
# a reviewer reliably cannot: `use_auth()` in the middle of 1200 lines of a
# component looks like a local, not like a hook.
#
# Usage: check-hooks-before-return.sh [--self-test]
set -u
cd "$(dirname "$0")/.." || exit 2

# Report `<file>:<line>: <hook>` for every hook call that follows a
# function-body-level `return` inside a `#[component]` function.
run_guard() {
  local root="$1"
  find "$root" -name '*.rs' -type f -print0 | while IFS= read -r -d '' file; do
    awk -v file="$file" '
      # A component starts at #[component] and ends at the first column-0 "}".
      /^#\[component\]/ { in_comp = 1; seen_return = 0; next }
      in_comp && /^\}/  { in_comp = 0; seen_return = 0; next }
      !in_comp { next }

      # Strip line comments so a `return` or `use_x(` inside prose does not count.
      { line = $0; sub(/[[:space:]]*\/\/.*$/, "", line) }

      # Indentation is what separates the component body from a closure inside
      # it. An early return is written at the body (4) or inside an `if` at the
      # body (8); a `return` deeper than that is inside a `use_memo`, an
      # `EventHandler` or an async block, where it exits the closure and not the
      # render. `dirty = use_memo(|| { if x { return false; } ... })` in the KB
      # editor is exactly that, and is not a bug.
      { indent = match(line, /[^ ]/) - 1 }

      indent <= 8 && line ~ /(^|[^[:alnum:]_])return([[:space:]]|;)/ { seen_return = 1; next }

      # And the hook has to be at that level too: a `use_*` nested deeper is
      # inside a closure, which is its own (different) bug and not this one.
      seen_return && indent <= 8 && match(line, /(^|[^[:alnum:]_.])(use_[a-z_]+)[[:space:]]*\(/, m) {
        printf "%s:%d: %s\n", file, FNR, m[2]
      }
    ' "$file"
  done
}

TARGET="${1:-src}"

if [ "${1:-}" = "--self-test" ]; then
  status=0
  tmp="$(mktemp --directory)"
  trap 'rm -rf "$tmp"' EXIT

  cat > "$tmp/bad.rs" <<'EOF'
#[component]
fn Broken(props: Props) -> Element {
    let a = use_signal(|| 0);
    if props.missing {
        return rsx! { div { "gone" } };
    }
    let auth = crate::hooks::use_auth();
    rsx! { div { "{a} {auth:?}" } }
}
EOF
  if [ -z "$(run_guard "$tmp")" ]; then
    echo "hook-order guard: SELF-TEST FAIL (a hook after an early return was not caught)"
    status=1
  fi

  cat > "$tmp/good.rs" <<'EOF'
#[component]
fn Fine(props: Props) -> Element {
    let a = use_signal(|| 0);
    let auth = crate::hooks::use_auth();
    if props.missing {
        return rsx! { div { "gone" } };
    }
    rsx! { div { "{a} {auth:?}" } }
}
EOF
  rm -f "$tmp/bad.rs"
  if [ -n "$(run_guard "$tmp")" ]; then
    echo "hook-order guard: SELF-TEST FAIL (the corrected order was rejected)"
    status=1
  fi

  if [ -n "$(run_guard src)" ]; then
    echo "hook-order guard: SELF-TEST FAIL (src does not pass)"
    status=1
  fi

  [ "$status" -eq 0 ] && echo "hook-order guard: self-test OK"
  exit "$status"
fi

hits="$(run_guard "$TARGET")"
if [ -z "$hits" ]; then
  echo "hook-order guard: OK"
  exit 0
fi
echo "hook-order guard: FAIL"
echo "$hits"
echo
echo "A hook after an early return runs on some renders and not others, so the"
echo "render that takes the exit leaves the component a hook short and the next"
echo "one panics in dioxus-core. WASM does not unwind, so that panic poisons the"
echo "runtime and the page stops responding entirely. Move the hook above every"
echo "\`return\` in the component. MAPPS-602."
exit 1
