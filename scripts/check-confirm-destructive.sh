#!/usr/bin/env bash
# MAPPS-436 guard: a destructive mutation never fires from a button `onclick`.
#
# docs/destructive-actions.md: "the Delete/Remove button's only job is to open
# the dialog ... Never issue the mutation straight from the button `onclick`."
# MAPPS-189 wired ConfirmDialog across the app; row-level Deletes added
# afterwards shipped with no confirmation at all, which is what this guard
# stops recurring.
#
# It tracks brace depth per file (with string, raw-string, char-literal and
# comment text removed, so `format!("/x/{id}")` does not skew it), then:
#   pass A: collects every `fn NAME` / `let NAME =` in that file whose body
#           issues a DELETE,
#   pass B: fails on any `onclick:` handler whose body issues a DELETE or calls
#           one of those names.
# A confirmed delete lives under `onconfirm:` / `ondelete:`, so it is invisible
# to pass B by construction. Indirection through a helper in ANOTHER file is
# out of reach here; a file that cannot be parsed fails loudly rather than
# reporting clean.
set -u
cd "$(dirname "$0")/.." || exit 2

report=$(
  find src -name '*.rs' -print0 | sort -z | xargs -0 awk '
    function clean(line,   pre, rest, q, n) {
      if (in_raw) {
        if (index(line, "\"#") > 0) { sub(/^.*"#/, "", line); in_raw = 0 }
        else { return "" }
      }
      gsub(/\\./, "", line)
      # Leading space so a raw-string opener at column 1 still has a delimiter
      # to match on; the class keeps `...Color"` from reading as `r"`.
      line = " " line
      while (match(line, /[^A-Za-z0-9_]r#*"/)) {
        pre = substr(line, 1, RSTART)
        rest = substr(line, RSTART + RLENGTH)
        if (match(rest, /"#*/)) { line = pre substr(rest, RSTART + RLENGTH) }
        else { line = pre; in_raw = 1; break }
      }
      gsub(SQ "[{}\"]" SQ, "", line)
      if (in_str) {
        if (index(line, "\"") > 0) { sub(/^[^"]*"/, "", line); in_str = 0 }
        else { return "" }
      }
      gsub(/"[^"]*"/, "", line)
      sub(/\/\/.*$/, "", line)
      n = split(line, q, "\"")
      if (n > 0 && n % 2 == 0) { in_str = 1; sub(/"[^"]*$/, "", line) }
      return line
    }
    function depth_delta(line,   i, c, d) {
      d = 0
      for (i = 1; i <= length(line); i++) {
        c = substr(line, i, 1)
        if (c == "{") d++
        else if (c == "}") d--
      }
      return d
    }
    # End of the block opened at line `i`: the first line whose closing depth is
    # back at the depth before `i`. A complete one-line attribute (no brace,
    # trailing comma) is its own region.
    function region_end(i,   k, seen) {
      if (clean_line[i] !~ /{/ && clean_line[i] ~ /,[ \t]*$/) return i
      seen = 0
      for (k = i; k <= n_lines; k++) {
        if (clean_line[k] ~ /{/) seen = 1
        if (seen && depth_after[k] <= depth_before[i]) return k
      }
      return n_lines
    }
    function region_text(i, j,   k, t) {
      t = ""
      for (k = i; k <= j; k++) t = t " " clean_line[k]
      return t
    }
    function process(   i, j, name, txt, sym, pass, t, lhs, rhs) {
      if (n_lines == 0) return
      if (depth != 0) {
        printf "%s: guard cannot parse this file (brace depth ended at %d)\n", file, depth
        bad = 1
        return
      }
      split("", deleters)
      for (i = 1; i <= n_lines; i++) {
        # Only real definitions: `fn NAME` and `let NAME = |closure|`. A plain
        # `let path = ...` or a `let Some(x) = ...` binding is not a callee.
        name = ""
        if (clean_line[i] ~ /^[ \t]*(pub[ \t]+)?(async[ \t]+)?fn[ \t]+/) {
          name = clean_line[i]
          sub(/^[ \t]*(pub[ \t]+)?(async[ \t]+)?fn[ \t]+/, "", name)
        } else if (clean_line[i] ~ /^[ \t]*let[ \t]+(mut[ \t]+)?[A-Za-z_][A-Za-z0-9_]*[ \t]*=[ \t]*(move[ \t]+)?[|{]/) {
          name = clean_line[i]
          sub(/^[ \t]*let[ \t]+(mut[ \t]+)?/, "", name)
        }
        if (name == "" || !match(name, /^[A-Za-z_][A-Za-z0-9_]*/)) continue
        name = substr(name, 1, RLENGTH)
        if (region_text(i, region_end(i)) ~ DEL) deleters[name] = 1
      }
      # `let on_del = on_delete;` renames a handler per row; follow the alias so
      # the rename does not hide the call. Repeated for short chains.
      for (pass = 0; pass < 3; pass++) {
        for (i = 1; i <= n_lines; i++) {
          t = clean_line[i]
          if (t !~ /^[ \t]*let[ \t]+(mut[ \t]+)?[A-Za-z_][A-Za-z0-9_]*[ \t]*=[ \t]*[A-Za-z_][A-Za-z0-9_]*[ \t]*;[ \t]*$/) continue
          sub(/^[ \t]*let[ \t]+(mut[ \t]+)?/, "", t)
          match(t, /^[A-Za-z_][A-Za-z0-9_]*/)
          lhs = substr(t, 1, RLENGTH)
          sub(/^[A-Za-z_][A-Za-z0-9_]*[ \t]*=[ \t]*/, "", t)
          match(t, /^[A-Za-z_][A-Za-z0-9_]*/)
          rhs = substr(t, 1, RLENGTH)
          if (rhs in deleters) deleters[lhs] = 1
        }
      }
      for (i = 1; i <= n_lines; i++) {
        if (clean_line[i] !~ /onclick:/) continue
        j = region_end(i)
        txt = region_text(i, j)
        if (txt ~ DEL) {
          printf "%s:%d: onclick issues a DELETE directly\n", file, i
          bad = 1
          continue
        }
        for (sym in deleters) {
          if (txt ~ ("[^A-Za-z0-9_]" sym "[^A-Za-z0-9_]")) {
            printf "%s:%d: onclick calls %s(), which issues a DELETE\n", file, i, sym
            bad = 1
            break
          }
        }
      }
    }
    BEGIN {
      SQ = sprintf("%c", 39)
      DEL = "delete_authed\\(|delete_authed_typed\\(|delete_lookup\\("
    }
    FNR == 1 {
      process()
      n_lines = 0; depth = 0; in_raw = 0; in_str = 0; file = FILENAME
    }
    {
      n_lines++
      c = clean($0)
      clean_line[n_lines] = c
      depth_before[n_lines] = depth
      depth += depth_delta(c)
      depth_after[n_lines] = depth
    }
    END { process(); if (bad) exit 1 }
  '
) || {
  echo "confirm-destructive guard: FAIL"
  echo "$report"
  echo "Route the mutation through crate::components::ConfirmDialog: the button"
  echo "sets a pending/confirming signal, the DELETE fires from onconfirm."
  echo "See docs/destructive-actions.md."
  exit 1
}

echo "confirm-destructive guard: clean"
