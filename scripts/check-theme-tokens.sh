#!/usr/bin/env bash
# MAPPS-259 theme-token guard, plus the MAPPS-444 dark-pair pass.
#
# After the semantic token migration, SPA components must express
# surfaces, text, borders, and the brand/primary accent through the
# token utilities defined in input.css (bg-app, bg-surface, bg-surface-2,
# bg-raised, text-content, text-muted, text-subtle, border-line,
# bg-accent, text-on-accent, text-accent, border-accent, ring-accent,
# bg-accent-{50..950}, ...). Hardcoded neutral grays / brand blue defeat
# theming, so this guard fails the build when they reappear.
#
# Allowed (NOT flagged by the first pass): semantic STATE colors red/rose
# (danger), green/emerald (success), yellow/amber/orange (warning), and blue (a
# valid INFO/status hue, e.g. AlertType::Info, BadgeVariant::Blue - the brand
# blue was already migrated to the accent tokens); `text-white` (it sits
# on colored fills); the theme source itself. The first pass targets only the
# neutral grays/white, which are never semantic and must be tokenized.
#
# The state hues are allowed but not unconditional: a `text-red-*` or
# `text-green-*` with no `dark:` sibling keeps its light-mode value on the dark
# surface, where it goes under AA. The second pass enforces the pair, judging
# every state hue: red, green, yellow, amber, orange, and blue - no family is
# exempt just because the guard doesn't name it.
#
# Blue carries one more hole the other state hues don't: unlike a `ring-red-*`
# or `ring-green-*`, which mark a deliberate danger/success confirm action, a
# `ring-blue-*` names no such state - every focus ring in this design system
# is the brand accent, so a raw `ring-blue-*` is always unmigrated brand blue,
# never a genuine INFO treatment. The third pass fails any `ring-blue-*` that
# isn't paired with `dark:` or marked `theme-guard-allow`, closing that
# specific hole without re-litigating blue's other, legitimate paired fills
# (`bg-blue-50 dark:bg-blue-900/20`, `border-blue-200 dark:border-blue-900`,
# ...), which pass validates separately via the text pairing above.
#
# Usage: check-theme-tokens.sh [ROOT | --self-test]
#   ROOT defaults to `src`. `--self-test` re-runs all three passes over
#   generated fixtures to prove they still reject a hardcoded gray, an
#   unpaired state hue, and an unpaired blue ring, so a future edit cannot
#   quietly neuter them.
set -u
cd "$(dirname "$0")/.." || exit 2

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  printf '    p { class: "text-sm text-gray-500 bg-white", "hi" }\n' > "$fixtures/gray.rs"
  out=$("$0" "$fixtures" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (a hardcoded neutral did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a hardcoded neutral fails the guard (exit $rc)"
  fi
  rm -f "$fixtures/gray.rs"

  {
    printf '    p { class: "text-sm text-red-600", "boom" }\n'
    printf '    button { class: "text-content hover:text-green-600", "vote" }\n'
    printf '    span { class: "text-sm text-blue-600", "info" }\n'
  } > "$fixtures/unpaired.rs"
  out=$("$0" "$fixtures" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (an unpaired state hue did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: an unpaired state hue fails the guard (exit $rc)"
  fi
  rm -f "$fixtures/unpaired.rs"

  printf '    button { class: "focus:outline-none focus:ring-2 focus:ring-blue-500", "go" }\n' > "$fixtures/unpaired-blue-ring.rs"
  out=$("$0" "$fixtures" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "self-test: FAIL (an unpaired blue ring did not fail the guard)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: an unpaired blue ring fails the guard (exit $rc)"
  fi
  rm -f "$fixtures/unpaired-blue-ring.rs"

  {
    printf '    p { class: "text-sm text-red-600 dark:text-red-400", "boom" }\n'
    printf '    button { class: "hover:text-green-600 dark:hover:text-green-400", "v" }\n'
    printf '    span { class: "text-content", "%s" }\n' 'plain'
    printf '    // a comment naming text-red-600 is prose, not a class string\n'
    printf '    let icon = "text-red-400"; // theme-guard-allow\n'
    printf '    p { class: "bg-red-50 border-red-200", "fills need no text pair" }\n'
    printf '    p { class: "border border-blue-200 dark:border-blue-900 bg-blue-50 dark:bg-blue-950/30 text-blue-700 dark:text-blue-300", "info" }\n'
    printf '    let icon = "focus:ring-blue-500"; // theme-guard-allow\n'
  } > "$fixtures/clean.rs"
  out=$("$0" "$fixtures" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (paired hues, comments, non-text hues or the allow marker were rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: paired hues, comments, non-text hues and the allow marker pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "theme-token guard self-test: clean"
  exit "$status"
fi

root="${1:-src}"
status=0

# Pass 1 (MAPPS-259): hardcoded neutrals.
# dark: optional prefix; bg-white, or {bg,text,border,ring,divide,placeholder}-gray-NNN.
pattern='(dark:)?(bg-white\b|(bg|text|border|ring|divide|placeholder)-gray-[0-9]{2,3})'

hits=$(grep -rnE "$pattern" "$root" --include='*.rs' \
  | grep -vE 'src/modules/theme/|src/components/theme_picker\.rs' \
  | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' \
  | grep -vF 'theme-guard-allow')   # drop full-line comments + opted-out lines

if [ -n "$hits" ]; then
  count=$(printf '%s\n' "$hits" | grep -c .)
  echo "theme-token guard: FAIL ($count hardcoded color class line(s))"
  echo "Replace with semantic tokens from input.css (see MAPPS-259). Semantic state colors red/green/yellow/amber/orange are allowed."
  printf '%s\n' "$hits"
  status=1
fi

# Pass 2 (MAPPS-444, extended by MAPPS-797): every state-hue TEXT utility
# carries a dark-mode pair.
#
# Tailwind v4's red-600 is #e7000b: 3.07:1 on the dark `--surface` #1e293b,
# under the 4.5:1 AA floor, so an unpaired light-mode red is unreadable in dark
# mode. `text-red-600 dark:text-red-400` (5.06:1) is the canonical spelling, and
# `dark:text-green-400` its success twin. Only `text-*` is judged: a fill
# (`bg-red-50`) or a border is contrast-checked against what sits on it, not
# against the surface. Every state hue is judged, not just red/green: yellow,
# amber, orange, and blue keep the same AA floor and the same escape hatch.
#
# Line-based, like its siblings, after joining Rust string continuations so a
# class split over `\` is read as one string. The pair may carry any variant
# chain (`dark:hover:text-red-400` pairs `hover:text-red-600`). Two separate
# class strings on one physical line would satisfy each other; no such site
# exists, and `theme-guard-allow` is the escape hatch either way.
#
# Pass 3 (MAPPS-797): every `ring-blue-*` carries a dark-mode pair (or the
# allow marker). Rings are singled out, not blue's other utilities, because
# every legitimate focus ring in this design system is the brand accent
# (`focus:ring-accent`): a raw `ring-blue-*` is always unmigrated brand blue,
# never a genuine INFO treatment, so it gets no unconditional pass the way
# `bg-blue-50`/`border-blue-200` INFO fills do.
hits=$(
  find "$root" -name '*.rs' -print0 | sort -z | xargs -0 awk '
    # A non-dark `<prefix>-<fam>-NNN` with no `dark:...<prefix>-<fam>-NNN`
    # anywhere in the joined line.
    function unpaired(s, prefix, fam,   pat, rest, chain, pre) {
      if (s ~ ("dark:([a-z-]+:)*" prefix "-" fam "-[0-9]{2,3}")) return 0
      pat = prefix "-" fam "-[0-9][0-9]?[0-9]?"
      rest = s
      while (match(rest, pat)) {
        pre = substr(rest, 1, RSTART - 1)
        rest = substr(rest, RSTART + RLENGTH)
        chain = match(pre, /[A-Za-z0-9:_-]+$/) ? substr(pre, RSTART) : ""
        if (index(chain, "dark:") == 0) return 1
      }
      return 0
    }
    # Rust continues a string literal over a trailing backslash; join those so
    # the pair may sit on the following physical line.
    {
      if (pending != "") { line = pending " " $0; lineno = pendingno }
      else { line = $0; lineno = FNR }
      pending = ""
      if (line ~ /\\[[:space:]]*$/) {
        sub(/\\[[:space:]]*$/, "", line)
        pending = line; pendingno = lineno; next
      }
      if (line ~ /^[[:space:]]*\/\//) next
      if (index(line, "theme-guard-allow")) next
      hit = 0
      n = split("red green yellow amber orange blue", fams, " ")
      for (i = 1; i <= n; i++) if (unpaired(line, "text", fams[i])) hit = 1
      if (unpaired(line, "ring", "blue")) hit = 1
      if (hit) print FILENAME ":" lineno ": " line
    }
    END { pending = "" }
  '
)

if [ -n "$hits" ]; then
  count=$(printf '%s\n' "$hits" | grep -c .)
  echo "dark-pair guard: FAIL ($count state-hue class(es) with no dark: sibling)"
  echo "Pair every state hue, as components/error_banner.rs and components/card.rs do:"
  echo '  text-red-600  ->  text-red-600 dark:text-red-400'
  echo '  text-green-600  ->  text-green-600 dark:text-green-400'
  echo '  ring-blue-500  ->  ring-blue-500 dark:ring-blue-400  (or, for the brand accent, focus:ring-accent)'
  printf '%s\n' "$hits"
  status=1
fi

[ "$status" -eq 0 ] && echo "theme-token guard: clean"
exit "$status"
