#!/usr/bin/env bash
# MAPPS-259 theme-token guard.
#
# After the semantic token migration, SPA components must express
# surfaces, text, borders, and the brand/primary accent through the
# token utilities defined in input.css (bg-app, bg-surface, bg-surface-2,
# bg-raised, text-content, text-muted, text-subtle, border-line,
# bg-accent, text-on-accent, text-accent, border-accent, ring-accent,
# bg-accent-{50..950}, ...). Hardcoded neutral grays / brand blue defeat
# theming, so this guard fails the build when they reappear.
#
# Allowed (NOT flagged): semantic STATE colors red/rose (danger),
# green/emerald (success), yellow/amber/orange (warning); blue (a valid
# INFO/status hue, e.g. AlertType::Info, BadgeVariant::Blue - the brand
# blue was already migrated to the accent tokens); `text-white` (it sits
# on colored fills); the theme source itself. The guard targets only the
# neutral grays/white, which are never semantic and must be tokenized.
set -u
cd "$(dirname "$0")/.." || exit 2

# dark: optional prefix; bg-white, or {bg,text,border,ring,divide,placeholder}-gray-NNN.
pattern='(dark:)?(bg-white\b|(bg|text|border|ring|divide|placeholder)-gray-[0-9]{2,3})'

hits=$(grep -rnE "$pattern" src --include='*.rs' \
  | grep -vE 'src/modules/theme/|src/components/theme_picker\.rs' \
  | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' \
  | grep -vF 'theme-guard-allow')   # drop full-line comments + opted-out lines

if [ -n "$hits" ]; then
  count=$(printf '%s\n' "$hits" | grep -c .)
  echo "theme-token guard: FAIL ($count hardcoded color class line(s))"
  echo "Replace with semantic tokens from input.css (see MAPPS-259). Semantic state colors red/green/yellow/amber/orange are allowed."
  printf '%s\n' "$hits"
  exit 1
fi
echo "theme-token guard: clean"
