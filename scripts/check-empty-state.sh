#!/usr/bin/env bash
# MAPPS-442 guard: every settings type-editor list renders the rich three-part
# EmptyState - `title`, `description`, and the page's "New <thing>" Button as
# `actions` - never `TableEmpty`'s bare-message mode.
#
# The bare mode is documented (components/table.rs) as being for sub-resource
# tables, error rows and filtered-out states with no CTA. A routed settings page
# with its own PageHeader and primary action is none of those, but all 14 of
# them used it anyway, writing the call to action as prose ("Click New Work Type
# to add one") that names a button the reader then has to go find. Both
# spellings compile, so nothing about the regression is visible in review.
#
# What fails, per `TableEmpty { .. }` block in the checked file: a `message:`
# prop, a missing `title:`/`description:`/`actions:`, an `actions:` block with
# no `Button {` and no "New <thing>" label, or copy naming a button in prose.
#
# Usage: check-empty-state.sh [FILE | --self-test]
#   FILE defaults to `src/pages/settings.rs`, the file the invariant covers.
#   `--self-test` re-runs the guard over generated fixtures to prove it still
#   rejects each violation and still accepts the rich form, so a future edit
#   cannot quietly neuter it.
set -u
cd "$(dirname "$0")/.." || exit 2

if [ "${1:-}" = "--self-test" ]; then
  fixtures=$(mktemp -d) || exit 2
  trap 'rm -rf "$fixtures"' EXIT
  status=0

  rich() {
    cat <<'EOF'
                    TableEmpty {
                        columns: 4,
                        title: "No work types yet".to_string(),
                        description: "Work types are the billable categories you pick.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                onclick: move |_| editing.set(Some(WorkTypeFormState::new())),
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "New Work Type"
                            }
                        },
                    }
EOF
  }

  check_rejects() {
    local name="$1" file="$2" out rc
    out=$("$0" "$file" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
      echo "self-test: FAIL ($name did not fail the guard)"
      printf '%s\n' "$out"
      status=1
    else
      echo "self-test: $name fails the guard (exit $rc)"
    fi
  }

  cat > "$fixtures/bare.rs" <<'EOF'
                    TableEmpty {
                        columns: 4,
                        message: "No work types yet. Click New Work Type to add one.".to_string(),
                    }
EOF
  check_rejects "the bare-message mode" "$fixtures/bare.rs"

  cat > "$fixtures/no_actions.rs" <<'EOF'
                    TableEmpty {
                        columns: 4,
                        title: "No work types yet".to_string(),
                        description: "Work types are the billable categories you pick.".to_string(),
                    }
EOF
  check_rejects "a rich empty state with no actions" "$fixtures/no_actions.rs"

  rich | sed 's/Work types are the billable categories you pick./Click New Work Type to add one./' \
    > "$fixtures/prose.rs"
  check_rejects "copy naming a button in prose" "$fixtures/prose.rs"

  rich > "$fixtures/clean.rs"
  out=$("$0" "$fixtures/clean.rs" 2>&1) && rc=0 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "self-test: FAIL (the rich three-part empty state was rejected)"
    printf '%s\n' "$out"
    status=1
  else
    echo "self-test: the rich three-part empty state passes the guard"
  fi

  [ "$status" -eq 0 ] && echo "empty-state guard self-test: clean"
  exit "$status"
fi

file="${1:-src/pages/settings.rs}"

hits=$(
  awk -v file="$file" '
    function flush(   where) {
      where = file ":" start ": "
      if (has_message)  print where "TableEmpty uses the bare `message:` mode"
      if (!has_title)   print where "TableEmpty has no `title:`"
      if (!has_desc)    print where "TableEmpty has no `description:`"
      if (!has_actions) print where "TableEmpty has no `actions:` CTA"
      if (has_actions && !(has_button && has_new)) \
                        print where "TableEmpty `actions:` passes no \"New <thing>\" Button"
      if (prose)        print where "TableEmpty copy names a button in prose"
    }
    {
      match($0, /^[ \t]*/)
      lead = RLENGTH
    }
    !in_block {
      if ($0 ~ /TableEmpty[ \t]*\{/) {
        in_block = 1; indent = lead; start = FNR
        has_message = 0; has_title = 0; has_desc = 0; has_actions = 0
        has_button = 0; has_new = 0; prose = 0
      }
      next
    }
    {
      if (lead == indent && $0 ~ /^[ \t]*\}/) { flush(); in_block = 0; next }
      if ($0 ~ /Click [A-Z]/) prose = 1
      if ($0 ~ /Button[ \t]*\{/) has_button = 1
      if ($0 ~ /"New /) has_new = 1
      if (lead != indent + 4) next
      if ($0 ~ /^[ \t]*message:/)     has_message = 1
      if ($0 ~ /^[ \t]*title:/)       has_title = 1
      if ($0 ~ /^[ \t]*description:/) has_desc = 1
      if ($0 ~ /^[ \t]*actions:/)     has_actions = 1
    }
    END { if (in_block) flush() }
  ' "$file"
)

if [ -n "$hits" ]; then
  echo "empty-state guard: FAIL (a settings list page is not on the rich EmptyState)"
  echo "Pass title + description + the page's \"New <thing>\" Button as actions:"
  echo '  message: "No work types yet. Click New Work Type to add one."'
  echo '  ->  title: "No work types yet", description: "...", actions: rsx! { Button { .. } }'
  printf '%s\n' "$hits"
  exit 1
fi

echo "empty-state guard: clean"
