#!/usr/bin/env bash
# MAPPS-446 guards: three class omissions, each one word away from the class
# every sibling site already uses, and none of them visible in the default
# theme. That is why they are enforced rather than reviewed.
#
# 1. A heading that sets a text size also sets a font weight, and every
#    /auth/callback heading uses the full-screen auth heading class
#    (`text-2xl font-semibold text-content`), as the other nine do.
# 2. A two-up form grid goes two-up at `sm:` (640px). `md:grid-cols-2` is only
#    legal as the middle step of a card ladder that continues to `lg:`.
# 3. A table name cell (a `span` opening a `TableCell`) names its colour token
#    instead of inheriting whatever the theme happens to set.
#
# Each check also fails if it stops matching anything, so a refactor that moves
# the shape out from under a pattern is loud instead of silently clean.
set -u
cd "$(dirname "$0")/.." || exit 2

status=0

# Colour classes that count as naming a colour: the theme's own tokens, as
# input.css declares them, plus a Tailwind palette shade (`text-red-600`).
tokens=$(grep -oE '^[[:space:]]*--color-[a-z0-9-]+' input.css \
  | sed 's/^[[:space:]]*--color-//' | sort -u | paste -sd '|' -)
if [ -z "$tokens" ]; then
  echo "class-omission guard: FAIL (no --color-* tokens found in input.css)"
  exit 1
fi
colour="text-($tokens)([^a-z0-9-]|\$)|text-[a-z]+-[0-9]"

# --- 1. headings ---------------------------------------------------------
# The class may sit on the `h1 {` line or on one of the next few lines, so
# track the open tag rather than grepping a single line.
heading_out=$(
  find src -name '*.rs' -print0 | xargs -0 awk '
    /h[1-6][ \t]*\{/ { pending = 1; look = 0 }
    pending && /class: "/ {
      match($0, /class: "[^"]*"/)
      cls = substr($0, RSTART + 8, RLENGTH - 9)
      pending = 0
      total++
      if (cls ~ /text-(xs|sm|base|lg|[0-9]?xl)([^a-z0-9-]|$)/ &&
          cls !~ /font-(medium|semibold|bold|extrabold|black)/)
        print FILENAME ":" FNR ": " cls
      next
    }
    pending { if (++look > 4) pending = 0 }
    END { print "TOTAL=" total }
  '
)
heading_total=${heading_out##*TOTAL=}
heading_hits=$(printf '%s\n' "$heading_out" | grep -v '^TOTAL=' || true)

if [ -n "$heading_hits" ]; then
  echo "heading-weight guard: FAIL (heading sets a text size but no font weight)"
  echo "It then renders at the browser default weight, which differs per element."
  printf '%s\n' "$heading_hits"
  status=1
fi
if [ "$heading_total" -lt 50 ]; then
  echo "heading-weight guard: FAIL (only $heading_total headings matched; the pattern stopped finding them)"
  status=1
fi

auth_page="src/pages/auth_callback.rs"
auth_headings=$(grep -cE 'h1 \{ class: "' "$auth_page" || true)
auth_canonical=$(grep -cE 'h1 \{ class: "text-2xl font-semibold text-content"' "$auth_page" || true)
if [ "$auth_headings" -eq 0 ] || [ "$auth_headings" -ne "$auth_canonical" ]; then
  echo "auth-heading guard: FAIL ($auth_canonical of $auth_headings headings in $auth_page"
  echo "use the full-screen auth heading class \`text-2xl font-semibold text-content\`)"
  status=1
fi

# --- 2. form grid breakpoint --------------------------------------------
grid_hits=$(grep -rnE 'md:grid-cols-2' src --include='*.rs' | grep -v 'lg:grid-cols-' || true)
if [ -n "$grid_hits" ]; then
  echo "form-grid guard: FAIL (two-up form grid gated at md: instead of sm:)"
  echo "Every other two-up form grid goes two-up at 640px; md: leaves it stacked to 768px."
  printf '%s\n' "$grid_hits"
  status=1
fi

# --- 3. table name cells -------------------------------------------------
cell_out=$(
  find src -name '*.rs' -print0 | xargs -0 awk -v colour="$colour" '
    /TableCell \{[ \t]*$/ { cell = 1; next }
    cell {
      cell = 0
      if ($0 ~ /span \{ class: "/) {
        match($0, /class: "[^"]*"/)
        cls = substr($0, RSTART + 8, RLENGTH - 9)
        total++
        if (cls !~ colour) print FILENAME ":" FNR ": " cls
      }
    }
    END { print "TOTAL=" total }
  '
)
cell_total=${cell_out##*TOTAL=}
cell_hits=$(printf '%s\n' "$cell_out" | grep -v '^TOTAL=' || true)

if [ -n "$cell_hits" ]; then
  echo "name-cell guard: FAIL (table cell span names no colour token)"
  echo "It inherits, so a theme that changes the inherited colour changes the cell."
  printf '%s\n' "$cell_hits"
  status=1
fi
if [ "$cell_total" -lt 15 ]; then
  echo "name-cell guard: FAIL (only $cell_total table cell spans matched; the pattern stopped finding them)"
  status=1
fi

[ "$status" -eq 0 ] && echo "class-omission guards: clean ($heading_total headings, $cell_total table cell spans)"
exit "$status"
