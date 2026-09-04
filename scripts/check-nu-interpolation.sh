#!/usr/bin/env bash
# MAPPS-680 Nushell interpolation guard: inside a `$"..."` string, every `(`
# opens a subexpression. A literal parenthesis in prose must be escaped `\(`.
#
# `print $"... public registry (MAPPS-421)."` in build-oci-image.yml made the
# publish job run `MAPPS-421` as an external command and die - after the image
# had already been built and pushed. Nothing could catch it: `nu --ide-check`
# parses the line clean, because an unknown external command is a RUN-time
# error, and the branch that reaches it only runs on a branch push.
#
# What fails: a `(` inside an interpolated string that is neither escaped nor
# followed by `$`. Subexpressions in this repo are all variable reads
# (`($tag)`, `($env.FOO)`), so requiring the `$` keeps the rule lexical and the
# false-positive count at zero. A command subexpression (`$"(date now)"`) is
# rejected too: bind it with `let` first, or escape the parenthesis.
#
# Scope: `*.nu`, the `justfile`, and the `run:` block of every `shell: nu` step
# in `.forgejo/workflows/*.yml`. Opt out on a line with `nu-interp-guard-allow`.
#
# Usage: check-nu-interpolation.sh [ROOT | --self-test]
#   ROOT defaults to the repo root. `--self-test` re-runs the guard over
#   generated fixtures to prove it still rejects a literal parenthesis and
#   still accepts an escaped one, so a future edit cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

scan() {
  awk '
    function skipstr(line, i, quote,   n, c) {
      n = length(line)
      while (i <= n) {
        c = substr(line, i, 1)
        if (c == "\\") { i += 2; continue }
        if (c == quote) return i + 1
        i++
      }
      return n + 1
    }
    # Walk an interpolated string body from i, reporting the first literal
    # parenthesis. A `($...)` subexpression is skipped whole, so a nested
    # paren inside one is never mistaken for prose.
    function interp(line, i, file, lno,   n, c, depth) {
      n = length(line)
      while (i <= n) {
        c = substr(line, i, 1)
        if (c == "\\") { i += 2; continue }
        if (c == "\"") return i + 1
        if (c == "(") {
          if (substr(line, i + 1, 1) != "$") {
            print file ":" lno ": " line
            return n + 1
          }
          depth = 0
          while (i <= n) {
            c = substr(line, i, 1)
            if (c == "(") depth++
            else if (c == ")") { depth--; if (depth == 0) { i++; break } }
            i++
          }
          continue
        }
        i++
      }
      return n + 1
    }
    function scanline(line, file, lno,   n, i, c) {
      n = length(line)
      i = 1
      while (i <= n) {
        c = substr(line, i, 1)
        if (c == "#") return                      # comment to end of line
        if (c == "'"'"'") { i = skipstr(line, i + 1, "'"'"'"); continue }
        if (c == "$" && substr(line, i + 1, 1) == "\"") { i = interp(line, i + 2, file, lno); continue }
        if (c == "\"") { i = skipstr(line, i + 1, "\""); continue }
        i++
      }
    }
    function indent(line) { match(line, /^ */); return RLENGTH }
    # A step is buffered whole and scanned at its end, so `shell:` and `run:`
    # may appear in either order without the block being silently skipped.
    function emit(   i) {
      if (step_nu) for (i = 1; i <= nbuf; i++) scanline(buf[i], buffile[i], bufln[i])
      nbuf = 0; step_nu = 0; in_run = 0
    }

    FNR == 1 { emit(); yaml = (FILENAME ~ /\.ya?ml$/) }
    /nu-interp-guard-allow/ { next }

    # A YAML file is scanned only inside the run: block of a `shell: nu` step.
    yaml {
      if (in_run && ($0 ~ /^[[:space:]]*$/ || indent($0) > base)) {
        nbuf++; buf[nbuf] = $0; buffile[nbuf] = FILENAME; bufln[nbuf] = FNR
        next
      }
      in_run = 0
      if ($0 ~ /^[[:space:]]*-[[:space:]]*[A-Za-z_]+:/) emit()   # next step
      if ($0 ~ /^[[:space:]]*shell:[[:space:]]*nu([[:space:]]|$)/) step_nu = 1
      if ($0 ~ /^[[:space:]]*run:[[:space:]]*[|>]/) { in_run = 1; base = indent($0) }
      next
    }

    { scanline($0, FILENAME, FNR) }
    END { emit() }
  ' "$@"
}

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  printf 'print $"pushed ($image) but not mirrored (MAPPS-421)."\n' > "$fixtures/bad.nu"
  printf -- '---\njobs:\n  a:\n    steps:\n      - name: s\n        shell: nu {0}\n        run: |\n          print $"note (MAPPS-421)."\n' > "$fixtures/bad.yml"
  # Same step with the keys the other way round: the block must still be scanned.
  printf -- '---\njobs:\n  a:\n    steps:\n      - name: s\n        run: |\n          print $"note (MAPPS-421)."\n        shell: nu {0}\n' > "$fixtures/bad-reordered.yml"
  for bad in bad.nu bad.yml bad-reordered.yml; do
    out=$("$0" "$fixtures/$bad" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
      echo "self-test: FAIL (a literal parenthesis in $bad did not fail the guard)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: a literal parenthesis in $bad fails the guard (exit $rc)"
    fi
  done
  rm -f "$fixtures/bad.nu" "$fixtures/bad.yml" "$fixtures/bad-reordered.yml"

  {
    printf 'print $"pushed ($image):($tag) not mirrored \\(MAPPS-421\\)."\n'
    printf 'let x = $"($env.A)/($env.B)"\n'
    printf '# a comment mentioning $"(MAPPS-421)" is prose, not nu code\n'
    printf 'let s = "a plain (parenthesised) string is not interpolated"\n'
    printf "let re = '(^v[0-9]+)'\n"
    printf 'print $"allowed (MAPPS-421)" # nu-interp-guard-allow\n'
  } > "$fixtures/clean.nu"
  # A bash step is not nu and is not scanned, including the one right after a
  # nu step: the step boundary must end the nu block.
  printf -- '---\njobs:\n  a:\n    steps:\n      - name: nu step\n        shell: nu {0}\n        run: |\n          print $"ok ($tag) \\(MAPPS-421\\)"\n      - name: bash step\n        run: |\n          echo "note (MAPPS-421)."\n' > "$fixtures/clean.yml"
  out=$("$0" "$fixtures" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (an escaped paren, a subexpression, a comment, a plain string, the allow marker or a bash step were rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: escaped parens, subexpressions, comments, plain strings, the allow marker and bash steps pass the guard"
  fi

  [ "$status" -eq 0 ] && echo "nu-interpolation guard self-test: clean"
  exit "$status"
fi

root="${1:-.}"

if [ -f "$root" ]; then
  files=("$root")
else
  mapfile -d '' -t files < <(
    # `common/` is a submodule: another repository's nu, not fixable from here.
    find "$root" \( -path "$root/target" -o -path "$root/node_modules" -o -path "$root/common" \) -prune -o \
      \( -name '*.nu' -o -name 'justfile' -o -path '*/.forgejo/workflows/*.yml' \) -print0 | sort -z
  )
  if [ "${#files[@]}" -eq 0 ]; then
    echo "nu-interpolation guard: FAIL (no nu sources found under $root)"
    exit 1
  fi
fi

# An awk failure must not read as "no hits": report it instead of going green.
hits=$(scan "${files[@]}") || {
  echo "nu-interpolation guard: FAIL (the scanner itself errored, so nothing was checked)"
  exit 2
}

if [ -n "$hits" ]; then
  echo "nu-interpolation guard: FAIL (literal parenthesis inside a Nushell interpolated string)"
  echo "Nushell runs it as a subexpression. Escape it, as the justfile already does:"
  echo '  $"... registry (MAPPS-421)."  ->  $"... registry \(MAPPS-421\)."'
  printf '%s\n' "$hits"
  exit 1
fi

echo "nu-interpolation guard: clean"
