#!/usr/bin/env bash
# MAPPS-624 guard: the page width cap lives on the page, never back in the shell.
#
# `AppShell` used to wrap its `Outlet` in `max-w-7xl mx-auto`, so all 92 routed
# pages were 1280px wide whether that suited them or not, and the KB reading
# view (tree rail + article + right rail) could not use a wide monitor. The cap
# moved onto each route component in `src/lib.rs`, which makes width a per-page
# choice: a page opts out by omitting its wrapper, as `KBArticleDetail` does.
#
# That arrangement is invisible in a diff. Putting `max-w-7xl` back on `main`
# looks like a tidy-up and silently re-caps the page that opted out, and a route
# added without its wrapper is full width by accident rather than by decision.
# Both are the same failure: nobody chose the width.
#
# Usage: check-page-width.sh [--self-test | --files LIB LAYOUT]
set -u
cd "$(dirname "$0")/.." || exit 2

CAP='max-w-7xl mx-auto'

# Routes that deliberately fill the window. Adding a name here is the record of
# that decision; the guard fails if a listed route carries the cap anyway, so
# the list cannot drift into a stale comment.
#
# MAPPS-652 added the two KB article editor routes. An editing surface is the
# one place where the content, not the chrome, should get the width, and the
# cap was also what made collapsing the sidebar useless there: `mx-auto` turns
# the reclaimed 12rem into two 6rem margins instead of writing area.
FULL_WIDTH='KBArticleDetail KBArticleNew KBArticleEdit'

# The route component names between `#[layout(AppShell)]` and its `#[end_layout]`.
appshell_routes() {
  awk '
    /^[ \t]*#\[layout\(AppShell\)\]/ { inside = 1; next }
    inside && /^[ \t]*#\[end_layout\]/ { exit }
    inside && match($0, /^[ \t]*[A-Z][A-Za-z0-9_]*[ \t]*\{.*\},[ \t]*$/) {
      name = $1
      sub(/\{.*$/, "", name)
      print name
    }
  ' "$1"
}

# The body of a top-level `fn NAME(...)`, from its signature to the closing
# brace, minus comment lines: a comment naming a class is prose about the code,
# and reading it as the code is how a guard passes a page it never checked.
fn_body() {
  awk -v name="$2" '
    $0 ~ "^(pub )?fn " name "\\(" { inside = 1 }
    inside && /^[ \t]*\/\// { next }
    inside { print }
    inside && /^\}/ { exit }
  ' "$1"
}

run_guard() {
  local lib="$1" layout="$2" fail=0 shell_body routes name body listed

  for f in "$lib" "$layout"; do
    if [ ! -f "$f" ]; then
      echo "page-width guard: FAIL (no such file: $f)"
      return 2
    fi
  done

  # 1. The shell caps nothing. Its `main` carries the shared padding instead.
  shell_body=$(fn_body "$layout" AppShell)
  if [ -z "$shell_body" ]; then
    echo "page-width guard: FAIL ($layout has no AppShell component)"
    return 1
  fi
  if printf '%s\n' "$shell_body" | grep -q 'max-w-'; then
    echo "page-width guard: FAIL (AppShell in $layout constrains its width again)"
    echo "  A max-w-* in the shell caps every routed page at once, including the"
    echo "  ones that opted out. The cap belongs on the route component."
    fail=1
  fi
  if ! printf '%s\n' "$shell_body" | grep -qF 'px-4 sm:px-6 lg:px-8'; then
    echo "page-width guard: FAIL (AppShell in $layout no longer supplies the shared padding)"
    echo "  \`main\` owns \`px-4 sm:px-6 lg:px-8\`; without it every page renders"
    echo "  flush against the sidebar."
    fail=1
  fi

  # 2. Every AppShell route caps itself, except the ones listed above.
  routes=$(appshell_routes "$lib")
  if [ -z "$routes" ]; then
    echo "page-width guard: FAIL (no routes found under #[layout(AppShell)] in $lib)"
    return 1
  fi
  for name in $routes; do
    body=$(fn_body "$lib" "$name")
    if [ -z "$body" ]; then
      echo "page-width guard: FAIL (route $name has no component in $lib)"
      fail=1
      continue
    fi
    listed=0
    for allowed in $FULL_WIDTH; do
      [ "$name" = "$allowed" ] && listed=1
    done
    if [ "$listed" -eq 1 ]; then
      if printf '%s\n' "$body" | grep -qF "$CAP"; then
        echo "page-width guard: FAIL ($name is listed as full width but caps itself)"
        echo "  Either drop the \`$CAP\` wrapper or take $name out of FULL_WIDTH"
        echo "  in this script, so the list still says what the code does."
        fail=1
      fi
    elif ! printf '%s\n' "$body" | grep -qF "$CAP"; then
      echo "page-width guard: FAIL ($name renders with no width of its own in $lib)"
      echo "  Wrap its top-level element in \`div { class: \"$CAP\", ... }\` to match"
      echo "  every other page, or add $name to FULL_WIDTH in this script to say the"
      echo "  full width is deliberate."
      fail=1
    fi
  done

  return "$fail"
}

if [ "${1:-}" = "--files" ]; then
  run_guard "$2" "$3"
  exit "$?"
fi

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d) || exit 2
  trap 'rm -r "$tmp"' EXIT
  status=0

  # The shape that shipped before MAPPS-624: the cap back around the Outlet.
  sed 's|main { class: "flex-1 overflow-y-auto overscroll-contain py-6 px-4 sm:px-6 lg:px-8",|main { class: "flex-1 overflow-y-auto overscroll-contain py-6",\n                    div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",|' \
    src/components/layout.rs > "$tmp/recapped-layout.rs"
  if run_guard src/lib.rs "$tmp/recapped-layout.rs" >/dev/null 2>&1; then
    echo "page-width guard: SELF-TEST FAIL (a shell that caps every page passed)"
    status=1
  fi

  # `main` stripped of the padding the wrapper div used to carry.
  sed 's| py-6 px-4 sm:px-6 lg:px-8"| py-6"|' src/components/layout.rs > "$tmp/unpadded-layout.rs"
  if run_guard src/lib.rs "$tmp/unpadded-layout.rs" >/dev/null 2>&1; then
    echo "page-width guard: SELF-TEST FAIL (a shell with no shared padding passed)"
    status=1
  fi

  # A route component that forgot its own wrapper.
  awk '
    /^fn Dashboard\(\) -> Element \{/ { skip = 1 }
    skip && /max-w-7xl mx-auto/ { next }
    skip && /^\}/ { skip = 0 }
    { print }
  ' src/lib.rs > "$tmp/uncapped-lib.rs"
  if run_guard "$tmp/uncapped-lib.rs" src/components/layout.rs >/dev/null 2>&1; then
    echo "page-width guard: SELF-TEST FAIL (a route with no width of its own passed)"
    status=1
  fi

  # The opt-out re-capped, which is the regression the KB article page exists to avoid.
  awk '
    /^fn KBArticleDetail\(id: String\) -> Element \{/ { print; print "    rsx! { div { class: \"max-w-7xl mx-auto\" } }"; next }
    { print }
  ' src/lib.rs > "$tmp/recapped-lib.rs"
  if run_guard "$tmp/recapped-lib.rs" src/components/layout.rs >/dev/null 2>&1; then
    echo "page-width guard: SELF-TEST FAIL (a full-width route that caps itself passed)"
    status=1
  fi

  # And the real pair still passes, so the guard is not failing everything.
  if ! run_guard src/lib.rs src/components/layout.rs >/dev/null 2>&1; then
    echo "page-width guard: SELF-TEST FAIL (the real src/lib.rs + layout.rs do not pass)"
    status=1
  fi

  [ "$status" -eq 0 ] && echo "page-width guard: self-test OK"
  exit "$status"
fi

if run_guard src/lib.rs src/components/layout.rs; then
  echo "page-width guard: OK"
  exit 0
fi
exit 1
