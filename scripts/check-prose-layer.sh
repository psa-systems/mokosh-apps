#!/usr/bin/env bash
# MAPPS-584 guard: the Markdown corrections must outrank the plugin they correct.
#
# `@tailwindcss/typography` emits every `.prose` rule into `@layer utilities`.
# The MAPPS-573 corrections were written into `@layer components`, and a cascade
# layer beats specificity and source order both, so for every declaration the
# two of them shared the plugin won and the correction did nothing. Inline code
# kept the plugin's literal backticks and its 600 weight; fenced blocks kept the
# plugin's background. The whole block shipped inert and no test noticed,
# because the tests that shipped with MAPPS-573 test the Rust renderer and the
# stylesheet had none.
#
# What makes it hard to see by eye is that the declarations the two did NOT
# share still applied. The inline-code pill had its background and its border,
# so the fix looked live and half-broken rather than dead.
#
# The invariant: the corrections live in a layer whose FIRST mention comes after
# `utilities`, which for a cascade layer is what "later" means. That requires a
# bare `@layer <name>;` statement after the Tailwind import, because a layer
# first named by its block would sort wherever that block happens to sit.
#
# Usage: check-prose-layer.sh [--self-test]
#   Checks input.css always. Also checks assets/styles.css when the Tailwind
#   build has been run, since that is the artifact the browser loads; CI does
#   not build the CSS, so its absence is not a failure.
set -u
cd "$(dirname "$0")/.." || exit 2

# A rule that only the corrections carry, used to find which layer they are in.
MARKER='content: none;'
SELECTOR='.prose :where(code)'
BUILT_LAYERS='theme base components utilities'

run_guard() {
  local css="$1" built="$2" fail=0 layer name

  # The layer whose block contains the inline-code correction.
  layer=$(awk -v sel="$SELECTOR" '
    /^@layer [a-z-]+ \{/ { name = $2; next }
    /^\}/ { name = ""; next }
    index($0, sel) && name != "" { print name; exit }
  ' "$css")

  if [ -z "$layer" ]; then
    echo "prose-layer guard: FAIL ($css has no @layer block containing \`$SELECTOR\`)"
    echo "  The Markdown corrections must sit in a named layer so their order is stated."
    return 1
  fi

  for name in $BUILT_LAYERS; do
    if [ "$layer" = "$name" ]; then
      echo "prose-layer guard: FAIL (the corrections are in \`@layer $layer\`)"
      echo "  Tailwind declares \`@layer theme, base, components, utilities;\` and the"
      echo "  typography plugin writes .prose into \`utilities\`, so a correction in any"
      echo "  of those four either loses to the plugin or fights the app's own styles."
      echo "  Put them in a layer of their own, declared after the import."
      return 1
    fi
  done

  # The bare declaration is what fixes the order. Without it the layer is first
  # named by its block, which sorts it wherever the block sits rather than last.
  if ! grep -qE "^@layer $layer;" "$css"; then
    echo "prose-layer guard: FAIL (\`@layer $layer;\` is never declared in $css)"
    echo "  A layer first named by its block sorts by position, not by intent."
    fail=1
  elif [ "$(grep -nE "^@layer $layer;" "$css" | head -1 | cut -d: -f1)" \
         -lt "$(grep -n '^@import "tailwindcss";' "$css" | head -1 | cut -d: -f1)" ]; then
    echo "prose-layer guard: FAIL (\`@layer $layer;\` is declared before the Tailwind import)"
    echo "  Tailwind's own layers are declared by the import; anything named earlier"
    echo "  sorts earlier and loses to them."
    fail=1
  fi

  # The artifact. Layer order there is the order of first mention across every
  # `@layer a, b;` statement, so `utilities` must be named before our layer is.
  #
  # Only when there IS an artifact. check.yml `touch`es an empty
  # assets/styles.css to satisfy the `asset!()` macro without installing bun, so
  # the file existing does not mean the CSS was built. A stylesheet that never
  # names `utilities` was not built by Tailwind, and reporting that as a
  # cascade-order failure would fail every CI run for the wrong reason.
  local theirs=""
  [ -f "$built" ] &&
    theirs=$(grep -nE '^@layer [a-z, -]*\butilities\b' "$built" | head -1 | cut -d: -f1)
  if [ -n "$theirs" ]; then
    local ours
    ours=$(grep -nE "^@layer [a-z, -]*\b$layer\b" "$built" | head -1 | cut -d: -f1)
    if [ -z "$ours" ]; then
      echo "prose-layer guard: FAIL ($built declares \`utilities\` but never \`$layer\`)"
      echo "  The corrections were not emitted into a layer of their own."
      fail=1
    elif [ "$ours" -lt "$theirs" ]; then
      echo "prose-layer guard: FAIL (in $built, \`$layer\` is ordered before \`utilities\`)"
      echo "  The built stylesheet is what the browser cascades; a correction ordered"
      echo "  before the plugin is a correction that does nothing."
      fail=1
    fi
  fi

  if ! grep -qF "$MARKER" "$css"; then
    echo "prose-layer guard: FAIL ($css no longer neutralises the plugin's backticks)"
    fail=1
  fi

  return "$fail"
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d) || exit 2
  trap 'rm -r "$tmp"' EXIT
  status=0

  # The exact shape that shipped: the corrections back in `components`.
  sed 's/^@layer prose;/@layer components-placeholder;/; s/^@layer prose {/@layer components {/' \
    input.css > "$tmp/regressed.css"
  if run_guard "$tmp/regressed.css" "$tmp/missing.css" >/dev/null 2>&1; then
    echo "prose-layer guard: SELF-TEST FAIL (a correction in @layer components passed)"
    status=1
  fi

  # The layer named only by its block, so its order is wherever it happens to be.
  grep -v '^@layer prose;' input.css > "$tmp/undeclared.css"
  if run_guard "$tmp/undeclared.css" "$tmp/missing.css" >/dev/null 2>&1; then
    echo "prose-layer guard: SELF-TEST FAIL (an undeclared layer passed)"
    status=1
  fi

  # A built stylesheet that orders the layer before utilities.
  cp input.css "$tmp/ok.css"
  printf '@layer prose;\n@layer theme, base, components, utilities;\n' > "$tmp/bad-built.css"
  if run_guard "$tmp/ok.css" "$tmp/bad-built.css" >/dev/null 2>&1; then
    echo "prose-layer guard: SELF-TEST FAIL (a built stylesheet ordering prose first passed)"
    status=1
  fi

  # check.yml's empty stub is not a built stylesheet and must not be read as one.
  : > "$tmp/stub.css"
  if ! run_guard input.css "$tmp/stub.css" >/dev/null 2>&1; then
    echo "prose-layer guard: SELF-TEST FAIL (an empty CSS stub was treated as a build)"
    status=1
  fi

  # And the real file still passes, so the guard is not failing everything.
  if ! run_guard input.css assets/styles.css >/dev/null 2>&1; then
    echo "prose-layer guard: SELF-TEST FAIL (the real input.css does not pass)"
    status=1
  fi

  [ "$status" -eq 0 ] && echo "prose-layer guard: self-test OK"
  exit "$status"
fi

if run_guard input.css assets/styles.css; then
  echo "prose-layer guard: OK"
  exit 0
fi
exit 1
