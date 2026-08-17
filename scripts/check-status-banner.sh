#!/usr/bin/env bash
# MAPPS-439 guard: every inline status banner is `components::StatusBanner`.
#
# MAPPS-418 extracted the red one as `ErrorBanner` and stopped there, so the
# success, warning and info states stayed hand-rolled: 11 banners in six
# recipes, splitting three ways on text treatment, three ways on the dark tint
# and two ways on padding, with two dropping the border and one dropping
# `role="alert"`. Every spelling compiles and renders, so nothing about the
# drift is visible in review. `StatusBanner { tone: BannerTone::* }` now owns
# all four; these two passes keep it that way.
#
# Pass 1: no state-hue banner fill (`bg-{red,green,amber,blue}-50`) in a page.
#   That fill is the tell of a hand-rolled banner. Pages render one through
#   `StatusBanner`, whose recipe lives in the component.
# Pass 2: the four inline recipes appear once each, and only in
#   `components/error_banner.rs`. This fails both when a recipe is copied back
#   out into another component and when a tone loses its arm, which is what
#   sent the other three states back to hand-rolling the first time.
#
# Opt out on a line with `theme-guard-allow`, the same escape hatch the sibling
# guards use (the marketing hero's `hover:bg-blue-50` on a brand gradient is not
# a banner). Whole-line comments are skipped, so a comment may name a class
# while explaining why it is not one.
#
# Usage: check-status-banner.sh [ROOT | --self-test]
#   ROOT defaults to `src`; the passes read `$ROOT/pages` and
#   `$ROOT/components/error_banner.rs`. `--self-test` re-runs the guard over
#   generated fixtures to prove it still rejects a hand-rolled banner, a copied
#   recipe and a dropped tone, and still accepts the migrated form, so a future
#   edit cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

# The recipe each tone must carry, keyed by hue. `BannerTone::class()` spells
# these out in full because Tailwind scans the source for literal class names.
hues="red green amber blue"
recipe() { # recipe <hue>
  printf 'rounded-md border border-%s-200 dark:border-%s-900 bg-%s-50 dark:bg-%s-950/30 px-3 py-2 text-sm text-%s-700 dark:text-%s-300' \
    "$1" "$1" "$1" "$1" "$1" "$1"
}

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  # A clean tree: pages carrying no banner fill, and a component file with all
  # four recipes. Each rejection case is this tree with one thing broken.
  build_clean() {
    rm -rf "$fixtures/src"
    mkdir -p "$fixtures/src/pages" "$fixtures/src/components"
    {
      printf '    StatusBanner { tone: BannerTone::Warning, class: "mb-3", "heads up" }\n'
      printf '    // a comment naming bg-green-50 is prose, not a class string\n'
      printf '    a { class: "hover:bg-blue-50", "hero" } // theme-guard-allow: marketing\n'
    } > "$fixtures/src/pages/ok.rs"
    : > "$fixtures/src/components/error_banner.rs"
    for hue in $hues; do
      printf '            Self::X => "%s",\n' "$(recipe "$hue")" \
        >> "$fixtures/src/components/error_banner.rs"
    done
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
  printf '    div { class: "rounded-md bg-green-50 p-3", "Saved." }\n' \
    > "$fixtures/src/pages/handrolled.rs"
  check_rejects "a hand-rolled banner in a page"

  build_clean
  printf '            class: "%s",\n' "$(recipe amber)" \
    > "$fixtures/src/components/other.rs"
  check_rejects "the recipe copied into another component"

  build_clean
  grep -v 'blue-200' "$fixtures/src/components/error_banner.rs" > "$fixtures/tone.rs"
  mv "$fixtures/tone.rs" "$fixtures/src/components/error_banner.rs"
  check_rejects "a tone dropped from BannerTone"

  build_clean
  out=$("$0" "$fixtures/src" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (the migrated form, a comment or the allow marker was rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: the migrated form, comments and the allow marker pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "status-banner guard self-test: clean"
  exit "$status"
fi

root="${1:-src}"
component="$root/components/error_banner.rs"
status=0

# --- Pass 1: no hand-rolled banner fill in a page ------------------------
hits=$(grep -rnE 'bg-(red|green|amber|blue)-50\b' "$root/pages" --include='*.rs' \
  | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' \
  | grep -vF 'theme-guard-allow')

if [ -n "$hits" ]; then
  count=$(printf '%s\n' "$hits" | grep -c .)
  echo "status-banner guard: FAIL ($count hand-rolled banner fill(s) in a page)"
  echo "Render components::StatusBanner instead, so all four states share one recipe:"
  echo '  div { class: "rounded-md bg-green-50 ...", "Saved." }'
  echo '  ->  StatusBanner { tone: BannerTone::Success, "Saved." }'
  printf '%s\n' "$hits"
  status=1
fi

# --- Pass 2: the four recipes live once each, in error_banner.rs ---------
if [ ! -f "$component" ]; then
  echo "status-banner guard: FAIL ($component is missing)"
  exit 1
fi

for hue in $hues; do
  want=$(recipe "$hue")
  here=$(grep -cF "$want" "$component" || true)
  if [ "$here" -ne 1 ]; then
    echo "status-banner guard: FAIL ($component carries the $hue recipe $here times, expected 1)"
    echo "Every BannerTone keeps its arm; dropping one is what sent the state back to hand-rolling."
    status=1
  fi

  elsewhere=$(grep -rlF "$want" "$root" --include='*.rs' | grep -vF "$component" || true)
  if [ -n "$elsewhere" ]; then
    echo "status-banner guard: FAIL (the $hue recipe is copied outside $component)"
    printf '%s\n' "$elsewhere"
    status=1
  fi
done

[ "$status" -eq 0 ] && echo "status-banner guard: clean"
exit "$status"
