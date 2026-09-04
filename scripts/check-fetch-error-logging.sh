#!/usr/bin/env bash
# MAPPS-695 guard: an awaited fetch that fails says why before it is dropped.
#
# Every page loads through `use_resource` closures that awaited a fetch and
# ended in `.ok()`. A 401, a 500, a decode mismatch and a genuinely empty tenant
# all reached the card as the same `None`, with nothing in the console to tell
# them apart. The rendered empty state was honest; the cause was gone.
#
# Banned shapes, every way an awaited `Result` is collapsed in this tree:
#   get_authed::<T>(&path).await.ok()                  the reason dropped at
#   get_all_authed::<T>(&path).await.unwrap_or_default()   the statement that
#   get_authed::<T>(&path).await.unwrap_or_else(|_| d)     decides what the
#   get_authed::<T>(&path).await.unwrap_or(d)              page renders as fact
#
# The fix is one combinator before the substitution:
#   .await.inspect_err(|e| tracing::error!("<what> load failed: {e}")).ok()
# at `warn` where the read is best-effort and the absent value changes nothing
# the user is told, at `error` where the substituted `None` decides what the
# page renders. Where an empty result and a failed read look identical on
# screen, `crate::hooks::fetch::list_or_empty` states both outcomes under
# distinct messages instead. A `unwrap_or_else` closure that logs the error
# itself is equally fine; its argument list is searched too.
#
# Scope: the collapse must be chained onto an awaited expression, within
# CHAIN_CHARS of the `.await` and in the same statement. `parse().ok()` and
# friends carry no server reason to report and their nearest `.await` is far
# away. `Response::ok()` in src/hooks/fetch.rs is an HTTP status test, not
# `Result::ok`, and is skipped by the receiver rule below: it is a bare binding
# (`response.ok()`), never a link in a method chain.
#
# Deliberate exceptions carry an allow marker with a reason, on any line of the
# statement:
#   // fetch-error-logging-allow: <why dropping the reason is correct here>
# A marker with no reason after the colon does not count.
#
# Usage: check-fetch-error-logging.sh [ROOT | --self-test]
#   ROOT defaults to `src`. `--self-test` re-runs the guard over generated
#   fixtures to prove it still catches a stripped log line, and still passes the
#   logged form, the allow marker and `Response::ok()`.
set -u
cd "$(dirname "$0")/.." || exit 2

# The longest logged chain in the tree is ~150 characters between `.await` and
# `.ok()`; anything wider is a different statement, not a dropped reason.
CHAIN_CHARS=200

run_guard() {
  find "$1" -name '*.rs' -type f -print0 | sort -z | while IFS= read -r -d '' file; do
    awk -v file="$file" -v chain="$CHAIN_CHARS" '
      # Strip line comments, but only outside a string literal: a URL or a
      # format path would otherwise truncate the line and hide the site.
      function strip(line,   i, n, c, out, inq, esc) {
        n = length(line); out = ""; inq = 0; esc = 0
        for (i = 1; i <= n; i++) {
          c = substr(line, i, 1)
          if (inq) {
            out = out c
            if (esc) esc = 0
            else if (c == "\\") esc = 1
            else if (c == "\"") inq = 0
            continue
          }
          if (c == "\"") { inq = 1; out = out c; continue }
          if (c == "/" && substr(line, i + 1, 1) == "/") break
          out = out c
        }
        return out
      }

      # Source line holding character position `pos` of the joined text.
      function line_at(pos,   i) {
        for (i = nl; i >= 1; i--) if (start[i] <= pos) return i
        return 1
      }

      # First line of the statement `lo` belongs to, plus the comment block
      # directly above it: where an allow marker is written.
      function stmt_top(lo,   i) {
        i = lo
        while (i > 1 && code_of[i - 1] != "" && code_of[i - 1] !~ /[;{}]$/) i--
        while (i > 1 && comment_only[i - 1]) i--
        return i
      }

      # An allow marker on the statement, or above it, covers the site.
      function allowed(from, to,   i) {
        for (i = stmt_top(from); i <= to; i++) if (allow[i]) return 1
        return 0
      }

      # `.ok()` is in scope only as a link in a method chain: after a call that
      # closed with `)`, or straight after `.await`. A bare binding receiver
      # (`response.ok()`) is `Response::ok`, an HTTP status test.
      function chained(p,   prev, i, id) {
        i = p - 1
        while (i >= 1 && substr(text, i, 1) == " ") i--
        prev = substr(text, i, 1)
        if (prev == ")" || prev == "?") return 1
        id = ""
        for (; i >= 1; i--) {
          prev = substr(text, i, 1)
          if (prev ~ /[A-Za-z0-9_]/) { id = prev id; continue }
          break
        }
        return (id == "await" && substr(text, i, 1) == ".")
      }

      {
        allow[FNR] = ($0 ~ /fetch-error-logging-allow:[ \t]*[^ \t]/) ? 1 : 0
        code = strip($0)
        gsub(/[ \t]+/, " ", code)
        sub(/^ +/, "", code); sub(/ +$/, "", code)
        code_of[FNR] = code
        comment_only[FNR] = (code == "" && $0 ~ /[^ \t]/) ? 1 : 0
        text = text " " code
        nl++
        start[nl] = length(text) - length(code) + 1
      }

      END {
        # Every `.await` position, in order, so each `.ok()` can take the
        # nearest one before it.
        na = 0; cur = 1
        while ((at = index(substr(text, cur), ".await")) > 0) {
          at = cur + at - 1
          na++; aw[na] = at
          cur = at + 6
        }
        # Each way an awaited `Result` is collapsed. `unwrap_or_else` and
        # `unwrap_or` may log inside the substitution itself, so their
        # argument list is searched too.
        scan(".ok()")
        scan(".unwrap_or_default()")
        scan(".unwrap_or_else(")
        scan(".unwrap_or(")
      }

      function scan(tok,   p, cur, last, a, between, lo, hi, n) {
        n = length(tok)
        cur = 1; last = 0
        while ((p = index(substr(text, cur), tok)) > 0) {
          p = cur + p - 1
          cur = p + n
          while (last < na && aw[last + 1] < p) last++
          if (last == 0) continue
          a = aw[last]
          between = substr(text, a + 6, p - a - 6)
          if (length(between) > chain) continue
          # A statement boundary in between means this collapse belongs to a
          # later expression, not to the awaited fetch: `let b = f().await?;
          # let n = b.parse().ok();` carries no server reason to report.
          if (between ~ /[;{}]/) continue
          if (!chained(p)) continue
          if (between ~ /tracing::/) continue
          if (args_of(p, n) ~ /tracing::/) continue
          lo = line_at(a); hi = line_at(p)
          if (allowed(lo, hi)) continue
          printf "%s:%d: .await%s%s drops the error instead of logging it first\n", \
            file, hi, between, tok
        }
      }

      # The argument list of the collapse call itself, paren-balanced: the
      # closure `unwrap_or_else` substitutes with may do the logging there.
      function args_of(p, n,   i, depth, out, c) {
        i = p + n - 1
        if (substr(text, i, 1) != "(") return ""
        depth = 0; out = ""
        for (; i <= length(text); i++) {
          c = substr(text, i, 1)
          out = out c
          if (c == "(") depth++
          else if (c == ")") { depth--; if (depth == 0) break }
        }
        return out
      }
    ' "$file"
  done
}

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp --directory) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  expect() { # expect <fail|pass> <label>
    local want="$1" label="$2" out
    out=$(run_guard "$fixtures" 2>&1)
    if [ "$want" = "fail" ] && [ -z "$out" ]; then
      echo "self-test: FAIL (expected the guard to reject: $label)"
      status=1
    elif [ "$want" = "pass" ] && [ -n "$out" ]; then
      echo "self-test: FAIL (expected the guard to accept: $label)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: $label"
    fi
  }

  # The log line stripped back out of a fixed site: the regression this exists
  # to catch.
  cat > "$fixtures/f.rs" <<'FIXTURE'
fn load() {
    let r = use_resource(|| async {
        crate::hooks::fetch::api::get_authed::<RemoteContact>(&path)
            .await
            .ok()
    });
}
FIXTURE
  expect fail "a stripped log line fails the guard"

  cat > "$fixtures/f.rs" <<'FIXTURE'
fn load() {
    let r = use_resource(|| async {
        crate::hooks::fetch::api::get_authed::<RemoteContact>(&path).await.ok()
    });
}
FIXTURE
  expect fail "the same discard on one line fails the guard"

  # The sibling collapses: a default substituted for the error with nothing
  # said, which is the same defect with a different operator.
  cat > "$fixtures/f.rs" <<'FIXTURE'
fn load() {
    let r = use_resource(|| async {
        crate::hooks::fetch::api::get_all_authed::<Row>(&path)
            .await
            .unwrap_or_default()
    });
}
FIXTURE
  expect fail "an unlogged .unwrap_or_default() fails the guard"

  cat > "$fixtures/f.rs" <<'FIXTURE'
fn load() {
    let r = use_resource(|| async {
        crate::hooks::fetch::api::get_authed::<Row>(&path)
            .await
            .unwrap_or_else(|_| Row::default())
    });
}
FIXTURE
  expect fail "an unlogged .unwrap_or_else() fails the guard"

  # Every shape that must keep passing. A guard that rejects these guards
  # nothing: the fixed form, the `map_err` variant, an allow marker with a
  # reason, `Response::ok()`, and a `parse().ok()` far from its `.await`.
  cat > "$fixtures/f.rs" <<'FIXTURE'
fn logged() {
    crate::hooks::fetch::api::get_authed::<RemoteContact>(&path)
        .await
        .inspect_err(|e| tracing::error!("contact detail load failed: {e}"))
        .ok()
}

fn mapped() {
    crate::hooks::fetch::api::get_authed::<Row>(&path)
        .await
        .map_err(|e| tracing::warn!("row load failed: {e}"))
        .ok()
}

fn substitution_logs_itself() {
    crate::hooks::fetch::api::get_all_authed::<Row>(&path)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("row load failed: {e}");
            Vec::new()
        })
}

fn excused() {
    // fetch-error-logging-allow: the caller reports the failure itself.
    crate::hooks::fetch::api::get_authed::<Row>(&path).await.ok()
}

async fn status() {
    let response = Request::get(&url).send().await.map_err(transport_err)?;
    if response.ok() {
        response.json::<T>().await.map_err(|e| e.to_string())
    } else {
        Err(status_error(response).await)
    }
}

async fn far() {
    let body = fetch(&url).await?;
    let n = body.trim().parse::<i64>().ok();
}
FIXTURE
  expect pass "the logged, mapped, self-logging, excused, Response::ok and parse forms pass"

  # An allow marker with nothing after the colon excuses nothing.
  cat > "$fixtures/f.rs" <<'FIXTURE'
fn empty_reason() {
    // fetch-error-logging-allow:
    crate::hooks::fetch::api::get_authed::<Row>(&path).await.ok()
}
FIXTURE
  expect fail "an allow marker with no reason fails the guard"

  [ "$status" -eq 0 ] && echo "fetch-error-logging guard self-test: clean"
  exit "$status"
fi

report=$(run_guard "${1:-src}")
if [ -n "$report" ]; then
  echo "fetch-error-logging guard: FAIL"
  printf '%s\n' "$report"
  echo
  echo "A 401, a 500, a decode mismatch and an empty tenant all reach the page as"
  echo "the same \`None\`. Log the cause before the substitution:"
  echo "  .await.inspect_err(|e| tracing::error!(\"<what> load failed: {e}\")).ok()"
  echo "\`warn\` for a best-effort read, \`error\` where the \`None\` decides what the"
  echo "page renders. If dropping the reason is genuinely right, say why on the"
  echo "statement: // fetch-error-logging-allow: <reason>. MAPPS-695."
  exit 1
fi

echo "fetch-error-logging guard: clean"
