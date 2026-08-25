#!/usr/bin/env bash
# MAPPS-574 guard: a destructive delete never discards the server's answer.
#
# `delete_authed` returns `Result<(), String>` and that `String` is the server's
# own refusal ("Cannot delete company with existing tickets", the PMS-170
# related-records list, ...). Five call sites ran `.is_ok()` on it and threw it
# away, so a refused delete stopped the spinner, closed the dialog, and said
# nothing. On the reported company that reason was only ever visible in
# devtools, and the user filed the button as broken.
#
# Banned shapes:
#   delete_authed(...).await.is_ok()     the result reduced to a bool
#   let _ = delete_authed(...).await     the result dropped outright
#
# The fix is to `match` the result: `Ok(())` reports success, `Err(err)` puts
# `err` in front of the user (`ConfirmDialog`'s `error` prop). Aggregating shapes
# stay legal - a bulk delete that collects results and reports how many failed
# has not discarded anything. See docs/destructive-actions.md.
#
# Scope: the call and the discard must fall within WINDOW lines of each other,
# which covers every formatting this codebase uses (the call is one line, or the
# receiver wraps onto the next). A discard deliberately spread wider than that,
# or laundered through an intermediate binding, is out of reach; this guard
# stops the shape from being reintroduced by copy-paste, which is how all five
# sites got there.
#
# Usage: check-delete-result.sh [ROOT | --self-test]
#   ROOT defaults to `src`. `--self-test` re-runs the guard over generated
#   fixtures to prove it still catches both banned shapes (on one line and
#   wrapped), and still passes the `match` form and the aggregating form.
set -u
cd "$(dirname "$0")/.." || exit 2

WINDOW=3

run_guard() {
  find "$1" -name '*.rs' -print0 | sort -z | xargs -0 awk -v window="$WINDOW" '
    # Line comments only. A `//` inside a string literal would truncate the
    # line early, which can only ever hide a call from the scan, and the
    # codebase has no delete call sitting behind one.
    function strip(line) { sub(/\/\/.*$/, "", line); return line }
    function window_text(i,   k, t) {
      t = ""
      for (k = i; k < i + window && k <= n_lines; k++) t = t " " src[k]
      gsub(/[ \t]+/, "", t)
      return t
    }
    function process(   i, t) {
      for (i = 1; i <= n_lines; i++) {
        if (src[i] !~ /delete_authed[A-Za-z_]*\(/) continue
        t = window_text(i)
        if (t ~ /delete_authed[A-Za-z_]*\(.*\)\.await\.is_ok\(\)/) {
          printf "%s:%d: the delete result is reduced to .is_ok(), discarding the server%s reason\n", file, i, "\x27s"
          bad = 1
        } else if (t ~ /let_=[^;]*delete_authed[A-Za-z_]*\(/) {
          printf "%s:%d: the delete result is dropped with `let _ =`\n", file, i
          bad = 1
        }
      }
    }
    FNR == 1 { process(); n_lines = 0; file = FILENAME }
    { n_lines++; src[n_lines] = strip($0) }
    END { process(); if (bad) exit 1 }
  '
}

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  expect() { # expect <want_rc:fail|pass> <label>
    local want="$1" label="$2" out rc
    out=$(run_guard "$fixtures" 2>&1) && rc=0 || rc=$?
    if [ "$want" = "fail" ] && [ "$rc" -eq 0 ]; then
      echo "self-test: FAIL ($label did not fail the guard)"
      printf '%s\n' "$out"
      status=1
    elif [ "$want" = "pass" ] && [ "$rc" -ne 0 ]; then
      echo "self-test: FAIL ($label was rejected)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: $label"
    fi
  }

  cat > "$fixtures/f.rs" <<'FIXTURE'
fn danger() {
    if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
        navigator.push(Route::CompanyList {});
    }
}
FIXTURE
  expect fail "a delete reduced to .is_ok() fails the guard"

  # The same discard, wrapped the way rustfmt breaks a long receiver.
  cat > "$fixtures/f.rs" <<'FIXTURE'
fn danger() {
    if crate::hooks::fetch::api::delete_authed(&path)
        .await
        .is_ok()
    {
        navigator.push(Route::CompanyList {});
    }
}
FIXTURE
  expect fail "a wrapped .is_ok() discard fails the guard"

  cat > "$fixtures/f.rs" <<'FIXTURE'
fn danger() {
    let _ = crate::hooks::fetch::api::delete_authed(&path).await;
}
FIXTURE
  expect fail 'a "let _ =" discard fails the guard'

  # The shape the fix uses, plus the aggregating shape a bulk delete needs.
  # A guard that rejects these guards nothing.
  cat > "$fixtures/f.rs" <<'FIXTURE'
fn reported() {
    match crate::hooks::fetch::api::delete_authed(&path).await {
        Ok(()) => {
            confirming_delete.set(false);
            navigator.push(Route::CompanyList {});
        }
        Err(err) => delete_error.set(err),
    }
}

fn aggregated() {
    let futs = ids.iter().map(|id| {
        let path = format!("/tickets/{id}");
        async move { crate::hooks::fetch::api::delete_authed(&path).await }
    });
    let results = join_all(futs).await;
    let failures = results.iter().filter(|r| r.is_err()).count();
    report(failures);
}
FIXTURE
  expect pass "the match form and the aggregating form pass the guard"

  [ "$status" -eq 0 ] && echo "delete-result guard self-test: clean"
  exit "$status"
fi

report=$(run_guard "${1:-src}") || {
  echo "delete-result guard: FAIL"
  echo "$report"
  echo "Match on the result instead: Ok(()) reports success, Err(err) puts the"
  echo "server's own message in front of the user (ConfirmDialog's \`error\` prop)."
  echo "See docs/destructive-actions.md."
  exit 1
}

echo "delete-result guard: clean"
