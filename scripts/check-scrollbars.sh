#!/usr/bin/env bash
# MAPPS-970 guard: the auto-hiding scrollbar contract, ported from bunyip's
# scripts/check-scrollbars.nu (BUNYIP-848) so the two apps cannot drift.
#
# Before MAPPS-970 a global `* { scrollbar-width: thin; scrollbar-color: ... }`
# made Chromium 121+ ignore every `::-webkit-scrollbar*` rule (YOTUN-208), so
# the styled pill never rendered, and the sidebar used `scrollbar-hide`, which
# left it with no bar to grab at all.
#
# The invariant, per stylesheet:
#   - forbidden everywhere: `scrollbar-width: none`, and `display: none` on a
#     `::-webkit-scrollbar*` part;
#   - `scrollbar-width` / `scrollbar-color` only inside the Firefox
#     `@supports not selector(::-webkit-scrollbar)` block;
#   - no painted bar, track, track piece or corner: only the thumb is painted;
#   - required: a 14px `::-webkit-scrollbar` (the grab zone, both axes), a
#     transparent track, a thumb color carrying `var(--sb-alpha)`, the
#     `@property --sb-alpha` registration that fails visible, the Firefox block
#     with `thin` and an alpha thumb; `scrollbar-gutter: stable` is forbidden (MAPPS-981).
# And in the behavior script: exactly one `SCROLLBAR_IDLE_MS`, within 5000-7000.
#
# Usage: check-scrollbars.sh [--self-test]
#   Checks input.css and assets/scrollbar-autohide.js always. Also checks
#   assets/styles.css when the Tailwind build has produced it; CI stubs it as
#   an empty file, so an empty or absent build is not a failure.
set -u
cd "$(dirname "$0")/.." || exit 2

CSS_SOURCE='input.css'
CSS_BUILT='assets/styles.css'
JS_FILE='assets/scrollbar-autohide.js'
IDLE_MS_MIN=5000
IDLE_MS_MAX=7000

# Splits the comment-stripped CSS into blocks, each with its own declarations,
# its effective selector (the nearest non-at-rule prelude, as CSS nesting reads
# it) and whether it sits inside the Firefox block, then judges them.
# shellcheck disable=SC2016
CSS_AWK='
function trim(s) { gsub(/^[[:space:]]+|[[:space:]]+$/, "", s); return s }
function flat(s) { gsub(/[[:space:]]+/, " ", s); return trim(s) }
function report(ln, msg) { if (ln) printf "%s:%d: %s\n", path, ln, msg; else printf "%s: %s\n", path, msg }
BEGIN { RS = "\001"; S = "[[:space:]]*"
  CLEAR = "(transparent|#0000|#00000000)"
  ALPHA = "color-mix\\(" S "in[[:space:]]+srgb" S "," S "var\\(--[a-z0-9-]+\\)[[:space:]]+calc\\(" S "var\\(--sb-alpha\\)" S "\\*" S "100%" S "\\)" S "," S CLEAR S "\\)"
}
{ text = text $0 }
END {
  n = length(text); line = 1; depth = 0; seg = ""; segline = 0; q = ""; k = 0
  eff[0] = ""; ff[0] = 0
  for (i = 1; i <= n; i++) {
    c = substr(text, i, 1)
    if (q == "" && c == "/" && substr(text, i + 1, 1) == "*") {
      rest = substr(text, i + 2); j = index(rest, "*/")
      com = j ? substr(rest, 1, j - 1) : rest
      line += gsub(/\n/, "", com)
      i += j ? j + 2 : n
      seg = seg " "
      continue
    }
    if (c == "\n") line++
    if (q != "") { seg = seg c; if (c == "\\") { i++; seg = seg substr(text, i, 1) } else if (c == q) q = ""; continue }
    if (c == "\"" || c == "\047") q = c
    if (c == "{") {
      depth++; pre[depth] = flat(seg); body[depth] = ""; bline[depth] = segline ? segline : line
      eff[depth] = (pre[depth] ~ /^@/) ? eff[depth - 1] : pre[depth]
      ff[depth] = ff[depth - 1] || (pre[depth] ~ ("^@supports[[:space:]]+not[[:space:]]+selector\\(" S "::-webkit-scrollbar" S "\\)"))
      seg = ""; segline = 0; continue
    }
    if (c == ";") { if (depth > 0) body[depth] = body[depth] seg ";"; seg = ""; segline = 0; continue }
    if (c == "}" && depth > 0) {
      body[depth] = body[depth] seg
      k++; rsel[k] = flat(eff[depth]); rown[k] = pre[depth]; rbody[k] = body[depth]; rff[k] = ff[depth]; rline[k] = bline[depth]
      depth--; seg = ""; segline = 0; continue
    }
    if (segline == 0 && c !~ /[[:space:]]/) segline = line
    seg = seg c
  }

  bar_w = bar_h = track = thumb = prop = ff_thin = ff_color = 0
  for (r = 1; r <= k; r++) {
    sel = rsel[r]; b = rbody[r]; nd = split(b, decls, ";")
    for (d = 1; d <= nd; d++) {
      decl = flat(decls[d])
      if (decl == "") continue
      if (decl ~ ("^scrollbar-width" S ":" S "none"))
        report(rline[r], "\047" decl "\047 - hides the Firefox bar.")
      if (sel ~ /::-webkit-scrollbar/ && decl ~ ("^display" S ":" S "none"))
        report(rline[r], "\047" sel " { " decl " }\047 - hides the WebKit / Chromium bar.")
      if (!rff[r] && decl ~ ("^scrollbar-(width|color)" S ":"))
        report(rline[r], "\047" decl "\047 outside the `@supports not selector(::-webkit-scrollbar)` block - Chromium then drops the webkit styling (YOTUN-208).")
      if (sel ~ /::-webkit-scrollbar(-track|-track-piece|-corner)?([^a-z-]|$)/ && decl ~ ("^background(-color|-image)?" S ":")) {
        v = decl; sub(/^[^:]*:[[:space:]]*/, "", v)
        if (v !~ ("^(" CLEAR "|none)$"))
          report(rline[r], "\047" sel " { " decl " }\047 paints a scrollbar background - only the thumb may be painted.")
      }
      if (decl ~ ("^scrollbar-gutter" S ":" S "stable"))
        report(rline[r], "\047" decl "\047 reserves an empty strip on containers that never scroll (MAPPS-981).")
      if (sel ~ ("::-webkit-scrollbar" S "(,|$)")) {
        if (decl ~ ("^width" S ":" S "14px$")) bar_w = 1
        if (decl ~ ("^height" S ":" S "14px$")) bar_h = 1
      }
      if (sel ~ ("::-webkit-scrollbar-track" S "(,|$)") && decl ~ ("^background(-color)?" S ":" S CLEAR "$")) track = 1
      if (sel ~ ("::-webkit-scrollbar-thumb" S "(,|$)") && decl ~ ("^background(-color)?" S ":" S ALPHA)) thumb = 1
      if (rff[r] && decl ~ ("^scrollbar-width" S ":" S "thin$")) ff_thin = 1
      if (rff[r] && decl ~ ("^scrollbar-color" S ":" S ALPHA "[[:space:]]+" CLEAR "$")) ff_color = 1
    }
    if (rown[r] == "@property --sb-alpha" && b ~ ("syntax" S ":" S "\"<number>\"") && b ~ ("inherits" S ":" S "true") && b ~ ("initial-value" S ":" S "1" S "(;|$)")) prop = 1
  }
  if (!bar_w) report(0, "missing a 14px `::-webkit-scrollbar` width, the grab zone.")
  if (!bar_h) report(0, "missing a 14px `::-webkit-scrollbar` height, the horizontal grab zone.")
  if (!track) report(0, "missing a transparent `::-webkit-scrollbar-track`.")
  if (!thumb) report(0, "missing a `::-webkit-scrollbar-thumb` color from a theme token carrying `var(--sb-alpha)` (color-mix in srgb over transparent).")
  if (!prop) report(0, "missing `@property --sb-alpha` registered as an inherited <number> with initial-value 1, which is what fails visible.")
  if (!ff_thin || !ff_color) report(0, "missing the Firefox `@supports not selector(::-webkit-scrollbar)` block with `scrollbar-width: thin` and a `var(--sb-alpha)` thumb over a transparent track.")
}
'

css_problems() {
  if [ ! -r "$1" ]; then
    echo "$1: missing or not readable - the guard cannot prove the scrollbar contract."
    return
  fi
  awk -v path="$1" "$CSS_AWK" "$1"
}

# The idle hold lives in the behavior script alone: declared once, in the agreed 5-7 s range.
js_problems() {
  local decls count ms
  if [ ! -r "$1" ]; then
    echo "$1: missing or not readable - the guard cannot prove the scrollbar idle hold."
    return
  fi
  decls=$(sed -nE 's/^[[:space:]]*(var|let|const)[[:space:]]+SCROLLBAR_IDLE_MS[[:space:]]*=[[:space:]]*([0-9_]+)[[:space:]]*;.*/\2/p' "$1")
  count=$(printf '%s' "$decls" | grep -c .)
  if [ "$count" -eq 0 ]; then
    echo "$1: missing the \`SCROLLBAR_IDLE_MS\` declaration that times the auto-hide."
  elif [ "$count" -gt 1 ]; then
    echo "$1: \`SCROLLBAR_IDLE_MS\` is declared $count times - declare it once."
  else
    ms=$((10#${decls//_/}))
    if [ "$ms" -lt "$IDLE_MS_MIN" ] || [ "$ms" -gt "$IDLE_MS_MAX" ]; then
      echo "$1: \`SCROLLBAR_IDLE_MS = $ms\` is outside the agreed $IDLE_MS_MIN-${IDLE_MS_MAX}ms hold (MAPPS-970)."
    fi
  fi
}

self_test() {
  local status=0 name expect why out
  tmp=$(mktemp -d) || exit 2
  trap 'rm -r "$tmp"' EXIT

  local alpha='color-mix(in srgb, var(--line-strong) calc(var(--sb-alpha) * 100%), transparent)'
  local p_property=$'@property --sb-alpha {\n  syntax: "<number>";\n  inherits: true;\n  initial-value: 1;\n}\n'
  local p_bar=$'::-webkit-scrollbar {\n  width: 14px;\n  height: 14px;\n  background-color: transparent;\n}\n'
  local p_track=$'::-webkit-scrollbar-track,\n::-webkit-scrollbar-corner {\n  background-color: transparent;\n}\n'
  local p_thumb=$'::-webkit-scrollbar-thumb {\n  border: 4px solid transparent;\n  background-clip: padding-box;\n  background-color: '"$alpha"$';\n}\n'
  local p_firefox=$'@supports not selector(::-webkit-scrollbar) {\n  html,\n  pre {\n    scrollbar-width: thin;\n    scrollbar-color: '"$alpha"$' transparent;\n  }\n}\n'
  local compliant="$p_property$p_bar$p_track$p_thumb$p_firefox"
  # What Tailwind emits unminified: a plain-token fallback plus a nested `@supports` for color-mix.
  local tw_thumb=$'::-webkit-scrollbar-thumb {\n  background-clip: padding-box;\n  background-color: var(--line-strong);\n  @supports (color: color-mix(in lab, red, red)) {\n    background-color: '"$alpha"$';\n  }\n}\n'
  local tw_firefox=$'@supports not selector(::-webkit-scrollbar) {\n  html, pre {\n    scrollbar-width: thin;\n    scrollbar-color: var(--line-strong) transparent;\n    @supports (color: color-mix(in lab, red, red)) {\n      scrollbar-color: '"$alpha"$' transparent;\n    }\n  }\n}\n'
  local tailwind="$p_property$p_bar$p_track$tw_thumb$tw_firefox"
  # The --minify form: one line, `: ` squeezed, `transparent` as `#0000`, the fallback hoisted out.
  local minified
  minified=$(printf '%s' "$p_property$p_bar$p_track" | sed -E 's/^[[:space:]]+//; s/: /:/g; s/transparent/#0000/g' | tr -d '\n')
  minified+='::-webkit-scrollbar-thumb{background-clip:padding-box;background-color:var(--line-strong)}@supports (color:color-mix(in lab, red, red)){::-webkit-scrollbar-thumb{background-color:color-mix(in srgb, var(--line-strong) calc(var(--sb-alpha) * 100%), transparent)}}'
  minified+='@supports not selector(::-webkit-scrollbar){html,pre{scrollbar-width:thin;scrollbar-color:var(--line-strong) transparent}@supports (color:color-mix(in lab, red, red)){html,pre{scrollbar-color:color-mix(in srgb, var(--line-strong) calc(var(--sb-alpha) * 100%), transparent) transparent}}}'

  local -a names expects whys
  case_css() {
    names+=("$1"); expects+=("$2"); whys+=("$3")
    printf '%s' "$4" > "$tmp/$1.css"
  }
  case_css compliant 0 "the authored auto-hiding block" "$compliant"
  case_css tailwind 0 "Tailwind's unminified output with its nested color-mix fallback" "$tailwind"
  case_css minified 0 "the --minify output, \`transparent\` as \`#0000\`" "$minified"
  case_css unrelated-hide 0 "an unrelated \`display: none\` rule" "$compliant"$'.hidden {\n  display: none;\n}\n'
  case_css commented-rules 0 "a comment naming the forbidden rules" $'/* replaces `* { scrollbar-color: var(--line-strong) transparent }`,\n   `::-webkit-scrollbar { display: none }` and a painted track */\n'"$compliant"
  case_css hover-thumb 0 "a painted thumb hover state" "$compliant"$'::-webkit-scrollbar-thumb:hover {\n  background-color: color-mix(in srgb, var(--muted) calc(var(--sb-alpha) * 100%), transparent);\n}\n'
  case_css quoted-brace 0 "a brace inside a quoted string" "$compliant"$'.x::after {\n  content: "}";\n}\n'
  case_css hidden-webkit 1 "a re-added \`::-webkit-scrollbar { display: none }\`" "$compliant"$'::-webkit-scrollbar {\n  display: none;\n}\n'
  case_css hidden-thumb 1 "a hidden \`::-webkit-scrollbar-thumb\`" "$compliant"$'::-webkit-scrollbar-thumb{display:none}\n'
  case_css hidden-utility 1 "the old \`.scrollbar-hide\` utility" "$compliant"$'.scrollbar-hide::-webkit-scrollbar {\n  display: none;\n}\n'
  case_css hidden-firefox 1 "\`scrollbar-width: none\` inside the Firefox block" "${compliant/scrollbar-width: thin/scrollbar-width: none}"
  case_css global-color 1 "a re-added global \`* { scrollbar-color }\` (YOTUN-208)" "$compliant"$'* {\n  scrollbar-color: var(--line-strong) transparent;\n}\n'
  case_css global-width 1 "a \`scrollbar-width\` outside the Firefox block" "$compliant"$'* {\n  scrollbar-width: auto;\n}\n'
  case_css thin-outside 1 "\`scrollbar-width: thin\` outside the Firefox block" "$compliant"$'.pane {\n  scrollbar-width: thin;\n}\n'
  case_css layered-color 1 "a \`scrollbar-color\` nested in \`@layer base\`" "$compliant"$'@layer base {\n  * {\n    scrollbar-color: var(--line-strong) transparent;\n  }\n}\n'
  case_css painted-track 1 "a painted \`::-webkit-scrollbar-track\`" "$compliant"$'::-webkit-scrollbar-track {\n  background-color: var(--surface-2);\n}\n'
  case_css painted-track-hover 1 "a painted track hover state" "$compliant"$'::-webkit-scrollbar-track:hover{background:#eee}\n'
  case_css painted-track-piece 1 "a painted \`::-webkit-scrollbar-track-piece\`" "$compliant"$'::-webkit-scrollbar-track-piece {\n  background: var(--line);\n}\n'
  case_css painted-bar 1 "a painted \`::-webkit-scrollbar\`" "$p_property${p_bar/background-color: transparent/background-color: var(--line)}$p_track$p_thumb$p_firefox"
  case_css painted-corner 1 "a painted \`::-webkit-scrollbar-corner\`" "$compliant"$'::-webkit-scrollbar-corner{background:#fff}\n'
  case_css narrow-bar 1 "a \`::-webkit-scrollbar\` narrower than the 14px zone" "${compliant/width: 14px/width: 5px}"
  case_css short-bar 1 "a horizontal bar lower than the 14px zone" "${compliant/height: 14px/height: 5px}"
  case_css no-track 1 "no transparent \`::-webkit-scrollbar-track\`" "$p_property$p_bar$p_thumb$p_firefox"
  case_css static-thumb 1 "a thumb color without \`var(--sb-alpha)\`" "$p_property$p_bar$p_track${p_thumb/"$alpha"/var(--line-strong)}$p_firefox"
  case_css literal-thumb 1 "a literal thumb color" "$p_property$p_bar$p_track${p_thumb/"$alpha"/#888}$p_firefox"
  case_css no-property 1 "no \`@property --sb-alpha\` registration" "$p_bar$p_track$p_thumb$p_firefox"
  case_css uninherited 1 "an uninherited \`--sb-alpha\`" "${compliant/inherits: true/inherits: false}"
  case_css hidden-initial 1 "\`--sb-alpha\` starting at 0, which would fail hidden" "${compliant/initial-value: 1/initial-value: 0}"
  case_css no-firefox 1 "no Firefox \`@supports\` block" "$p_property$p_bar$p_track$p_thumb"
  case_css firefox-auto 1 "a Firefox block without \`thin\`" "${compliant/scrollbar-width: thin/scrollbar-width: auto}"
  case_css firefox-static 1 "a Firefox thumb color without \`var(--sb-alpha)\`" "$p_property$p_bar$p_track$p_thumb${p_firefox/"$alpha"/var(--line-strong)}"
  case_css gutter-stable 1 "a re-added \`scrollbar-gutter: stable\` (MAPPS-981)" "$compliant"$'html {\n  scrollbar-gutter: stable;\n}\n'
  case_css unstyled 1 "a stylesheet with the scrollbar styling stripped out" $'body {\n  color: red;\n}\n'
  case_css commented-styling 1 "the styling present only inside a comment" "/* $compliant */"$'\nbody {\n  color: red;\n}\n'

  local i
  for i in "${!names[@]}"; do
    name=${names[$i]}; expect=${expects[$i]}; why=${whys[$i]}
    out=$(css_problems "$tmp/$name.css")
    if { [ "$expect" -eq 0 ] && [ -z "$out" ]; } || { [ "$expect" -eq 1 ] && [ -n "$out" ]; }; then
      echo "self-test: ok, the guard handles $why"
    else
      echo "self-test: FAIL, the guard mis-handles $why"
      printf '%s\n' "${out:-  (no problems reported)}" | sed 's/^/  /'
      status=1
    fi
  done

  local -a js_cases=(
    '0|  var SCROLLBAR_IDLE_MS = 6000;|a 6000ms hold'
    '0|  const SCROLLBAR_IDLE_MS = 5000;|the 5000ms lower bound'
    '0|  let SCROLLBAR_IDLE_MS = 7_000;|the 7000ms upper bound'
    '1|  var SCROLLBAR_IDLE_MS = 1000;|a 1000ms hold, too short to grab'
    '1|  var SCROLLBAR_IDLE_MS = 4999;|a hold just under 5000ms'
    '1|  var SCROLLBAR_IDLE_MS = 7001;|a hold just over 7000ms'
    '1|  var TOAST_SHORT_MS = 5000;|no `SCROLLBAR_IDLE_MS`'
    '1|  // var SCROLLBAR_IDLE_MS = 6000;|a commented-out declaration only'
    '1|  var SCROLLBAR_IDLE_MS = 6000;\n  var SCROLLBAR_IDLE_MS = 6500;|two declarations'
  )
  local entry js n=0
  for entry in "${js_cases[@]}"; do
    n=$((n + 1))
    expect=${entry%%|*}; js=${entry#*|}; why=${js#*|}; js=${js%|*}
    printf '%b\n' "$js" > "$tmp/case-$n.js"
    out=$(js_problems "$tmp/case-$n.js")
    if { [ "$expect" -eq 0 ] && [ -z "$out" ]; } || { [ "$expect" -eq 1 ] && [ -n "$out" ]; }; then
      echo "self-test: ok, the guard handles $why"
    else
      echo "self-test: FAIL, the guard mis-handles $why"
      printf '%s\n' "${out:-  (no problems reported)}" | sed 's/^/  /'
      status=1
    fi
  done

  if [ -z "$(css_problems "$tmp/absent.css")" ]; then
    echo "self-test: FAIL, a missing stylesheet passed"; status=1
  else
    echo "self-test: ok, a missing stylesheet is an error, not a pass"
  fi
  if [ -z "$(js_problems "$tmp/absent.js")" ]; then
    echo "self-test: FAIL, a missing behavior script passed"; status=1
  else
    echo "self-test: ok, a missing behavior script is an error, not a pass"
  fi

  [ "$status" -eq 0 ] && echo "scrollbars guard self-test: clean"
  return "$status"
}

case "${1:-}" in
  --self-test)
    self_test
    exit $?
    ;;
  "")
    ;;
  *)
    echo "scrollbars guard: FAIL (unknown argument: $1)"
    exit 2
    ;;
esac

files=("$CSS_SOURCE")
[ -s "$CSS_BUILT" ] && files+=("$CSS_BUILT")
problems=""
for f in "${files[@]}"; do
  problems+=$(css_problems "$f")$'\n'
done
problems+=$(js_problems "$JS_FILE")
problems=$(printf '%s\n' "$problems" | sed '/^$/d')

if [ -n "$problems" ]; then
  echo "scrollbars guard: FAIL"
  printf '%s\n' "$problems" | sed 's/^/  /'
  echo "  Scrollbars are a 5px thumb with no painted track in a 14px grab zone, hidden at rest only"
  echo "  once $JS_FILE tags <html>, and shown for SCROLLBAR_IDLE_MS (5-7 s) after use (MAPPS-970)."
  echo "  \`scrollbar-width\` / \`scrollbar-color\` belong in the Firefox @supports block alone."
  exit 1
fi
echo "scrollbars guard: OK (${files[*]} and $JS_FILE keep the auto-hiding scrollbar contract)"
