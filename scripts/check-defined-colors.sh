#!/usr/bin/env bash
# MAPPS-433 defined-colour guard.
#
# Tailwind v4 emits a utility only when it can resolve the name. A class built
# on a colour nobody defined - `text-danger`, `border-default`, `divide-border`
# - therefore produces NO CSS at all: no error, no fallback, just an element
# rendering with whatever it inherited. It reads as deliberate in the source and
# is invisible in the browser, which is how the request-form builder shipped
# field rows with no border and a failed-save message in body colour (PMS-760),
# and how a client-facing error banner lost its colour entirely.
#
# `check-theme-tokens.sh` hunts the OPPOSITE mistake (hardcoded neutrals that
# defeat theming) and passes an undefined semantic name cleanly, so this is its
# own pass.
#
# How it decides:
#   1. Read the colour names the theme actually defines out of input.css
#      (`--color-<name>`), so adding or renaming a token needs no edit here.
#   2. Collect every colour-shaped utility used in src/**/*.rs.
#   3. Ignore anything carrying a digit or ending mid-word: that is the Tailwind
#      palette (`text-red-600`), an arbitrary value, or a class completed by
#      interpolation (`border-l-{n}`), none of which depend on our tokens.
#      `text-[10px]` never matches in the first place, because a bracket is not
#      a colour name.
#   4. What is left is a bare word. It is either a theme colour, a non-colour
#      Tailwind keyword (`border-dashed`, `text-center`, `ring-inset`), or a
#      name nobody defined. Only the third fails.
#
# Opt out on a line with `theme-guard-allow`, the same escape hatch the sibling
# guard uses. Whole-line comments are skipped, so a comment may name a dead
# class while explaining why it is dead - which is also why this reads whole
# lines rather than `grep -o` matches: the match alone cannot tell you it came
# out of a sentence.
set -u
cd "$(dirname "$0")/.." || exit 2

# Non-colour values these prefixes legitimately take, plus the two palette
# colours that carry no shade. Names with a digit never reach here, so
# `text-2xl`, `ring-2` and `border-l-4` need no entry.
KEYWORDS='none|current|inherit|transparent|auto|initial|unset|white|black'
KEYWORDS="$KEYWORDS"'|dashed|dotted|solid|double|hidden|groove|ridge|inset|outset'
KEYWORDS="$KEYWORDS"'|center|left|right|justify|start|end|top|bottom'
# Bare sides and offsets: `border-b`, `divide-y`, `ring-offset`.
KEYWORDS="$KEYWORDS"'|b|t|l|r|x|y|s|e|offset'
KEYWORDS="$KEYWORDS"'|base|sm|md|lg|xl|xs'
KEYWORDS="$KEYWORDS"'|wrap|nowrap|balance|pretty|ellipsis|clip|truncate'
KEYWORDS="$KEYWORDS"'|cover|contain|fixed|local|scroll|repeat|no-repeat|origin|blend'
KEYWORDS="$KEYWORDS"'|gradient-to-t|gradient-to-tr|gradient-to-r|gradient-to-br'
KEYWORDS="$KEYWORDS"'|gradient-to-b|gradient-to-bl|gradient-to-l|gradient-to-tl'

# The theme's own vocabulary, as input.css declares it.
defined=$(grep -oE '^[[:space:]]*--color-[a-z0-9-]+' input.css \
  | sed 's/^[[:space:]]*--color-//' | sort -u)
if [ -z "$defined" ]; then
  echo "defined-colour guard: FAIL (no --color-* tokens found in input.css)"
  exit 1
fi
defined_pattern=$(printf '%s' "$defined" | paste -sd '|' -)

prefixes='bg|text|border|ring|divide|fill|stroke|outline|placeholder|caret|decoration|accent'
# `ring-offset-surface` and `border-l-line` name a colour after a side or an
# offset, so those come off before the name is judged.
sides='offset|l|t|r|b|x|y|s|e'

hits=$(grep -rnE "($prefixes)-[a-z]" src --include='*.rs' \
  | awk -v pat="($prefixes)-[a-z][a-z0-9-]*" \
        -v defined="^($defined_pattern)$" \
        -v keywords="^($KEYWORDS)$" \
        -v prefixes="^($prefixes)-" \
        -v sides="^($sides)-" '
      {
        if (!match($0, /^[^:]+:[0-9]+:/)) next
        loc = substr($0, 1, RLENGTH - 1)
        rest = substr($0, RLENGTH + 1)
        if (rest ~ /^[[:space:]]*\/\//) next
        if (index($0, "theme-guard-allow")) next

        while (match(rest, pat)) {
          utility = substr(rest, RSTART, RLENGTH)
          before = RSTART > 1 ? substr(rest, RSTART - 1, 1) : " "
          after = substr(rest, RSTART + RLENGTH, 1)
          rest = substr(rest, RSTART + RLENGTH)

          # Mid-word, so it is part of some longer identifier or sentence.
          if (before ~ /[a-zA-Z0-9]/) continue
          # A shade, an arbitrary value, or a name finished by interpolation
          # (`border-l-{n}`), which resolves to a width rather than a colour.
          if (utility ~ /[0-9]/ || utility ~ /-$/) continue
          if (after == "-" || after == "[") continue

          name = utility
          sub(prefixes, "", name)
          sub(sides, "", name)
          if (name ~ defined || name ~ keywords) continue
          print loc ": " utility
        }
      }' \
  | sort -u)

if [ -n "$hits" ]; then
  count=$(printf '%s\n' "$hits" | grep -c .)
  echo "defined-colour guard: FAIL ($count use(s) of a colour input.css does not define)"
  echo "Tailwind emits nothing for these, so they render as no styling at all."
  echo "Use a token from input.css (line, surface, content, muted, accent, ...) or"
  echo "a semantic state colour (red/green/yellow), which the theme guard allows."
  printf '%s\n' "$hits"
  exit 1
fi
echo "defined-colour guard: clean"
