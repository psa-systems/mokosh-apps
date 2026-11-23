#!/usr/bin/env bash
# MAPPS-545 Markdown link guard.
#
# MAPPS-540 took docs/ from 49 broken relative links to zero. Nothing stopped
# the count climbing back, and it did not climb through carelessness the first
# time: every one of the 49 came from a file move. `docs/dev-docs/` was
# relocated without repointing the links inside it, so every `](../src/...)`
# resolved to `docs/src/...`, one directory short, and the four inbound links to
# `client-server-integration.md` had the same defect.
#
# A broken link fails silently. The reader clicks, lands on nothing, and
# concludes the docs are abandoned - which is roughly what had happened. Three
# doc audits reported stale content in those files without anyone noticing the
# links underneath were dead.
#
# This resolves a path against the tree, not prose wording: a doc link is a
# cross-reference to a file, and only the filesystem can say whether it still
# exists. mokosh-server's PMS-850 does the same thing in Nushell; this is bash
# because every other guard here is, and one script is not worth a second guard
# language on the runner.
#
# Deliberately NOT checked:
#   - Anchor fragments. `](codebase-state.md#proposed-fixes)` verifies the file,
#     not the heading: checking headings means parsing slugification rules, and
#     a renamed heading is a far smaller harm than a missing file.
#   - Cross-repo pointers. A link to `mokosh-server/...` cannot resolve in a
#     checkout of THIS repository, so failing it is the intended behaviour. The
#     convention both repos settled on is a code span, not a link.
#
# SPEC-2.1.1: no-argument invocation, default root, and the clean/FAIL stdout
# line contract.
#
# Usage: check-doc-links.sh [--self-test | ROOT]
#   No argument checks every tracked Markdown file. ROOT limits the walk to one
#   directory, which is how --self-test points it at fixtures.
set -u
cd "$(dirname "$0")/.." || exit 2

# Every link target in one file, as `line<TAB>target<TAB>text`. `text` is the
# bracketed link text for an inline link, empty for a reference definition or
# raw HTML href/src (neither has one).
#
# The awk pass carries the state a regex cannot: whether we are inside a fenced
# code block or an HTML comment, neither of which renders a link.
# `docs/dev-docs/kb-ui-overhaul-plan.md` holds a literal `javascript:alert(1`
# inside a fence as example text, and a naive scan reports it.
targets_in() {
  awk '
    # SPEC-2.5.19 (skip condition): a target inside a fenced code block must
    # not be evaluated.
    # Fence toggling. A closing fence must match the opener s length or longer,
    # but three backticks is what this repo writes, so the simple form is
    # enough - and being generous here only ever skips more, never less.
    /^[[:space:]]*(```|~~~)/ { fence = !fence; next }
    fence { next }

    { line = $0 }

    # SPEC-2.5.19 (skip condition): a target inside an HTML comment must not
    # be evaluated.
    # HTML comments, single- and multi-line.
    comment {
      if (line ~ /-->/) { sub(/^.*-->/, "", line); comment = 0 } else { next }
    }
    {
      while (match(line, /<!--/)) {
        rest = substr(line, RSTART)
        if (rest ~ /-->/) {
          sub(/<!--.*-->/, "", line)
        } else {
          sub(/<!--.*$/, "", line)
          comment = 1
          break
        }
      }
    }

    {
      # Inline links and images: `[text](target)`. The text is captured too,
      # for the text/target basename check below; `[^][]*` deliberately does
      # not handle a `[` nested inside link text, which this repo does not write.
      s = line
      while (match(s, /\[[^][]*\]\([^)]*\)/)) {
        full = substr(s, RSTART, RLENGTH)
        p = index(full, "](")
        txt = substr(full, 2, p - 2)
        t = substr(full, p + 2, length(full) - p - 2)
        if (t != "") print NR "\t" t "\t" txt
        s = substr(s, RSTART + RLENGTH)
      }

      # Reference definitions: `[label]: target`.
      if (line ~ /^[[:space:]]*\[[^]]+\]:[[:space:]]*[^[:space:]]/) {
        t = line
        sub(/^[[:space:]]*\[[^]]+\]:[[:space:]]*/, "", t)
        sub(/[[:space:]].*$/, "", t)
        if (t != "") print NR "\t" t
      }

      # Raw HTML: href="target" / src="target", single or double quoted.
      s = line
      while (match(s, /(href|src)[[:space:]]*=[[:space:]]*("[^"]*"|'"'"'[^'"'"']*'"'"')/)) {
        t = substr(s, RSTART, RLENGTH)
        sub(/^(href|src)[[:space:]]*=[[:space:]]*./, "", t)
        sub(/.$/, "", t)
        if (t != "") print NR "\t" t
        s = substr(s, RSTART + RLENGTH)
      }
    }
  ' "$1"
}

# The path a target names in this repository, or empty when it names none.
path_of() {
  local target="$1"
  # SPEC-2.5.19 (skip conditions): anchor-only and protocol-relative targets
  # must not be evaluated.
  case "$target" in
    '#'*) return ;;      # same-document anchor
    '//'*) return ;;     # protocol-relative URL
  esac
  # SPEC-2.5.19 (skip condition, cont.): a scheme-qualified target (http:,
  # https:, mailto:, tel:, javascript:, etc.) must not be evaluated.
  if printf '%s' "$target" | grep -qE '^[a-zA-Z][a-zA-Z0-9+.-]*:'; then
    return
  fi
  # SPEC-2.5.19 (resolution logic): strip any fragment or query before the
  # target is resolved.
  local path="${target%%#*}"
  path="${path%%\?*}"
  path="${path//%20/ }"
  printf '%s' "$path"
}

run_over() { # run_over <root>
  local root="$1" status=0 checked=0 files=0 broken=0 mismatched=0
  local list
  if [ "$root" = "." ]; then
    list=$(git ls-files '*.md' 2>/dev/null)
  else
    list=$(find "$root" -type f -name '*.md' | sort)
  fi
  if [ -z "$list" ]; then
    echo "doc-link guard: FAIL (no Markdown files under $root)"
    echo "The guard found nothing to check, which is not a pass."
    return 2
  fi

  while IFS= read -r file; do
    [ -n "$file" ] || continue
    files=$((files + 1))
    local dir
    dir=$(dirname "$file")
    while IFS=$'\t' read -r lineno target text; do
      [ -n "${target:-}" ] || continue
      local path
      path=$(path_of "$target")
      [ -n "$path" ] || continue
      checked=$((checked + 1))
      # SPEC-2.5.19 (resolution logic, cont.): resolve relative to the file's
      # own directory, not the repository root, and the target must exist.
      local resolved
      case "$path" in
        /*) resolved=".${path}" ;;
        *) resolved="$dir/$path" ;;
      esac
      if [ ! -e "$resolved" ]; then
        if [ "$broken" -eq 0 ]; then
          echo "doc-link guard: FAIL (a Markdown link names a path that is not there)"
        fi
        echo "  $file:$lineno: $target -> $resolved"
        status=1
        broken=1
      fi
      # A link's text may name the file it points at, e.g. `` [`foo.md`](foo.md) ``.
      # A rename that repoints the target without updating that text leaves a
      # reader unable to tell the stale name from an intentional one, which is
      # the defect MAPPS-1286 fixed; this keeps it from coming back.
      if [ -n "${text:-}" ]; then
        local stripped="${text#\`}"
        stripped="${stripped%\`}"
        if printf '%s' "$stripped" | grep -qE '^[A-Za-z0-9._-]+\.md$'; then
          local target_base="${path##*/}"
          if [ "$stripped" != "$target_base" ]; then
            if [ "$mismatched" -eq 0 ]; then
              echo "doc-link guard: FAIL (a Markdown link's text names a different file than its target)"
            fi
            echo "  $file:$lineno: link text '$stripped' -> target names '$target_base'"
            status=1
            mismatched=1
          fi
        fi
      fi
    done < <(targets_in "$file")
  done <<< "$list"

  if [ "$broken" -ne 0 ]; then
    echo "Repoint it at the file's current location, or drop the link. A link to"
    echo "another repository cannot resolve in a checkout of this one: write it as"
    echo "a code span instead."
  fi
  if [ "$mismatched" -ne 0 ]; then
    echo "Change the link text to match the target's filename, or the target to"
    echo "match the text, whichever one is correct."
  fi
  if [ "$status" -ne 0 ]; then
    return 1
  fi
  echo "doc-link guard: clean ($checked relative links across $files Markdown files all resolve)"
  return 0
}

# SPEC-2.1.3: self-test mode runs detection against generated fixtures only,
# proving it still rejects a broken link and the depth defect, and still
# accepts valid links, skip-cases, and fences/comments.
if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  mkdir -p "$fixtures/docs/sub"
  : > "$fixtures/docs/real.md"
  : > "$fixtures/docs/sub/nested.md"

  build_ok() {
    rm -f "$fixtures/docs/sub/page.md"
    {
      # Resolves from the file's own directory, which is the defect that
      # produced all 49 of MAPPS-540's broken links.
      printf 'See [real](../real.md) and [nested](nested.md).\n'
      printf 'An [anchor](#section), a [url](https://example.com), a [mail](mailto:a@b.c).\n'
      printf '[ref]: ../real.md\n'
      printf '<a href="../real.md">raw html</a>\n'
      printf '<!-- [commented](../gone.md) -->\n'
      printf '```\n[fenced](../gone.md) and javascript:alert(1\n```\n'
    } > "$fixtures/docs/sub/page.md"
  }

  build_ok
  out=$("$0" "$fixtures/docs" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (a tree of valid links was rejected, exit $rc)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: valid links pass, and anchors, URLs, comments and fences are skipped"
  fi

  build_ok
  printf '[gone](../nowhere.md)\n' >> "$fixtures/docs/sub/page.md"
  out=$("$0" "$fixtures/docs" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 1 ]; then
    echo "self-test: FAIL (a broken target did not fail the guard, exit $rc)"
    printf '%s\n' "$out"
    status=1
  elif ! printf '%s' "$out" | grep -q 'nowhere.md'; then
    echo "self-test: FAIL (the failure did not name the target)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a broken target fails the guard and names it"
  fi

  # The depth defect itself: a target that exists at the repo root but not
  # relative to the file that names it.
  build_ok
  printf '[wrong depth](real.md)\n' >> "$fixtures/docs/sub/page.md"
  out=$("$0" "$fixtures/docs" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 1 ]; then
    echo "self-test: FAIL (a wrong-depth target did not fail the guard, exit $rc)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a target one directory short fails the guard"
  fi

  # MAPPS-1286: link text naming a `.md` filename must match its target's
  # basename, the defect left behind by a file move that repointed the target
  # but not the text naming it.
  build_ok
  printf '[`nested.md`](../real.md)\n' >> "$fixtures/docs/sub/page.md"
  out=$("$0" "$fixtures/docs" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 1 ]; then
    echo "self-test: FAIL (a text/target basename mismatch did not fail the guard, exit $rc)"
    printf '%s\n' "$out"
    status=1
  elif ! printf '%s' "$out" | grep -q "'nested.md'.*'real.md'"; then
    echo "self-test: FAIL (the mismatch failure did not name both filenames)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: a link text naming a different file than its target fails the guard"
  fi

  [ "$status" -eq 0 ] && echo "doc-link guard self-test: clean"
  exit "$status"
fi

run_over "${1:-.}"
exit $?
