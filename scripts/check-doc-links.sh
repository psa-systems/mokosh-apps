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
# Usage: check-doc-links.sh [--self-test | ROOT]
#   No argument checks every tracked Markdown file. ROOT limits the walk to one
#   directory, which is how --self-test points it at fixtures.
set -u
cd "$(dirname "$0")/.." || exit 2

# Every link target in one file, as `line<TAB>target`.
#
# The awk pass carries the state a regex cannot: whether we are inside a fenced
# code block or an HTML comment, neither of which renders a link.
# `docs/dev-docs/kb-ui-overhaul-plan.md` holds a literal `javascript:alert(1`
# inside a fence as example text, and a naive scan reports it.
targets_in() {
  awk '
    # Fence toggling. A closing fence must match the opener s length or longer,
    # but three backticks is what this repo writes, so the simple form is
    # enough - and being generous here only ever skips more, never less.
    /^[[:space:]]*(```|~~~)/ { fence = !fence; next }
    fence { next }

    { line = $0 }

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
      # Inline links and images: `](target)`.
      s = line
      while (match(s, /\]\([^)]*\)/)) {
        t = substr(s, RSTART + 2, RLENGTH - 3)
        if (t != "") print NR "\t" t
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
  case "$target" in
    '#'*) return ;;      # same-document anchor
    '//'*) return ;;     # protocol-relative URL
  esac
  # Any scheme (http:, https:, mailto:, tel:, javascript:) resolves outside the
  # tree.
  if printf '%s' "$target" | grep -qE '^[a-zA-Z][a-zA-Z0-9+.-]*:'; then
    return
  fi
  local path="${target%%#*}"
  path="${path%%\?*}"
  path="${path//%20/ }"
  printf '%s' "$path"
}

run_over() { # run_over <root>
  local root="$1" status=0 checked=0 files=0
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
    while IFS=$'\t' read -r lineno target; do
      [ -n "${target:-}" ] || continue
      local path
      path=$(path_of "$target")
      [ -n "$path" ] || continue
      checked=$((checked + 1))
      local resolved
      case "$path" in
        /*) resolved=".${path}" ;;
        *) resolved="$dir/$path" ;;
      esac
      if [ ! -e "$resolved" ]; then
        if [ "$status" -eq 0 ]; then
          echo "doc-link guard: FAIL (a Markdown link names a path that is not there)"
        fi
        echo "  $file:$lineno: $target -> $resolved"
        status=1
      fi
    done < <(targets_in "$file")
  done <<< "$list"

  if [ "$status" -ne 0 ]; then
    echo "Repoint it at the file's current location, or drop the link. A link to"
    echo "another repository cannot resolve in a checkout of this one: write it as"
    echo "a code span instead."
    return 1
  fi
  echo "doc-link guard: clean ($checked relative links across $files Markdown files all resolve)"
  return 0
}

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

  [ "$status" -eq 0 ] && echo "doc-link guard self-test: clean"
  exit "$status"
fi

run_over "${1:-.}"
exit $?
