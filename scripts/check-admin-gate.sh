#!/usr/bin/env bash
# MAPPS-1211 guard: a settings surface marked `StaffAdmin` refuses a non-admin
# in its own page, not only in the Settings hub.
#
# `SETTINGS_SURFACES` in src/pages/settings.rs carries a `visibility` per tile,
# and `SurfaceVisibility::StaffAdmin` governs ONE thing: whether the tile is
# drawn on the hub. It is not routing and it is not authorization. The route
# stays reachable, so a non-admin with a bookmark, a back button or a pasted
# link lands on the page itself, and what they are shown is whatever that page
# decided to render.
#
# Forty of the forty-one StaffAdmin pages decided to refuse, each by its own
# early return. Settings > Contact Roles never added one, and so showed a
# non-admin the full role list and the capability editor. The server's admin
# check on `/portal-roles` meant nothing could be saved, which is why this
# survived: the defect is invisible unless you hold a non-admin session and
# type the URL, and the page looks correct in every other condition.
#
# Nothing in the types connects a `visibility` to the page the `route` names,
# and nothing can: the registry is data in one file and the gate is control
# flow in another. That gap is mechanical, which is why a script can hold it.
#
# What this does NOT check: that the gate is correct, or that it is the right
# gate. The finance surfaces refuse on a finance capability rather than on
# admin (`NoFinancePermission`, `PermissionRequired`), which is stricter and
# deliberate. Accepting any of the four gate forms in use keeps this guard on
# the one question it can answer mechanically: does this page refuse anybody
# at all. A page that refuses nobody is the bug this file exists to catch.
#
# Usage: check-admin-gate.sh [ROOT | --self-test]   (ROOT defaults to .)
# SPEC-2.10.1: argument shape; SPEC-2.10.5: FAIL/OK output and exit codes.
set -u
cd "$(dirname "$0")/.." || exit 2

# How a page refuses. Four spellings because four exist: `use_is_admin()` is
# the settings helper most pages call, `is_admin()` is the same question asked
# straight off the auth signal (settings_branding.rs, sla.rs), and the two
# component names are the finance plane's own refusals.
# SPEC-2.10.4: the four accepted gate spellings.
GATES='use_is_admin|is_admin\(\)|NoFinancePermission|PermissionRequired'

# SPEC-2.10.2: every `Route::` variant registered with StaffAdmin visibility.
staff_routes() {
  awk '
    /SettingsSurface \{/ { block = ""; collecting = 1 }
    collecting { block = block $0 "\n" }
    collecting && /^    \},/ {
      collecting = 0
      if (block ~ /SurfaceVisibility::StaffAdmin/ &&
          match(block, /route:[[:space:]]*Route::[A-Za-z0-9_]+/)) {
        r = substr(block, RSTART, RLENGTH)
        sub(/.*Route::/, "", r)
        print r
      }
    }
  ' "$1" | sort -u
}

# The body of `fn <name>(` in <file>, by brace depth. Empty when absent.
fn_body() {
  awk -v name="$1" '
    !started && index($0, "fn " name "(") { started = 1 }
    started {
      print
      line = $0; n = gsub(/\{/, "{", line)
      line = $0; m = gsub(/\}/, "}", line)
      depth += n - m
      if (depth <= 0 && (n + m) > 0) exit
    }
  ' "$2"
}

# The file a module path names, `settings` -> src/pages/settings.rs.
module_file() {
  if [ -f "$1/src/pages/$2.rs" ]; then
    echo "$1/src/pages/$2.rs"
  elif [ -f "$1/src/pages/$2/mod.rs" ]; then
    echo "$1/src/pages/$2/mod.rs"
  fi
}

# Does <component> in <file> refuse somebody, or does the single component it
# delegates to? One hop, because the gate-then-body split IS the house pattern
# (MAPPS-602 forbids the early return sitting above the body's hooks), so the
# page component is often three lines that render a `*Body`.
component_gates() {
  local file="$1" name="$2" body hop
  body="$(fn_body "$name" "$file")"
  [ -z "$body" ] && return 2
  if printf '%s' "$body" | grep -qE "$GATES"; then
    return 0
  fi
  for hop in $(printf '%s' "$body" |
    grep -oE '[A-Z][A-Za-z0-9_]*[[:space:]]*\{' |
    sed -E 's/[[:space:]]*\{$//' | sort -u); do
    if printf '%s' "$(fn_body "$hop" "$file")" | grep -qE "$GATES"; then
      return 0
    fi
  done
  return 1
}

# SPEC-2.10.3: resolve each route to its page component, then SPEC-2.10.4 and SPEC-2.10.5 per route.
run_guard() {
  local root="$1" route body ref mod file comp target
  staff_routes "$root/src/pages/settings.rs" | while read -r route; do
    body="$(fn_body "$route" "$root/src/lib.rs")"
    if [ -z "$body" ]; then
      echo "$route: no router component in src/lib.rs"
      continue
    fi
    # The page the router wrapper renders, as `module::Component {`.
    ref="$(printf '%s' "$body" |
      grep -oE '[a-z_]+::[A-Z][A-Za-z0-9_]*[[:space:]]*\{' | head -1 |
      sed -E 's/[[:space:]]*\{$//')"
    if [ -z "$ref" ]; then
      # The wrapper IS the page.
      file="$root/src/lib.rs"
      comp="$route"
    else
      mod="${ref%%::*}"
      comp="${ref##*::}"
      file="$(module_file "$root" "$mod")"
      if [ -z "$file" ]; then
        echo "$route: $mod is not a file under src/pages"
        continue
      fi
    fi
    component_gates "$file" "$comp"
    case "$?" in
      0) ;;
      2) echo "$route: $comp is not defined in ${file#"$root/"}" ;;
      *) target="${file#"$root/"}"
         echo "$route: $target::$comp renders its body to a non-admin" ;;
    esac
  done
}

# SPEC-2.10.6: --self-test fixtures and exit status.
if [ "${1:-}" = "--self-test" ]; then
  status=0
  tmp="$(mktemp --directory)"
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/src/pages"

  cat > "$tmp/src/pages/settings.rs" <<'EOF'
const SETTINGS_SURFACES: &[SettingsSurface] = &[
    SettingsSurface {
        route: Route::SettingsOpen {},
        title: "Open",
        visibility: SurfaceVisibility::Always,
    },
    SettingsSurface {
        route: Route::SettingsGated {},
        title: "Gated",
        visibility: SurfaceVisibility::StaffAdmin,
    },
];
EOF
  cat > "$tmp/src/lib.rs" <<'EOF'
#[component]
fn SettingsOpen() -> Element {
    rsx! { page_open::OpenPage {} }
}

#[component]
fn SettingsGated() -> Element {
    rsx! { page_gated::GatedPage {} }
}
EOF
  # A page that renders its body to anybody, reached through the split the
  # house pattern uses: the hop has to be followed to see there is no gate.
  cat > "$tmp/src/pages/page_gated.rs" <<'EOF'
#[component]
pub fn GatedPage() -> Element {
    use_page_title("Gated");
    rsx! { GatedBody {} }
}

#[component]
fn GatedBody() -> Element {
    rsx! { div { "secrets" } }
}
EOF
  cat > "$tmp/src/pages/page_open.rs" <<'EOF'
#[component]
pub fn OpenPage() -> Element {
    rsx! { div { "anybody" } }
}
EOF
  if [ -z "$(run_guard "$tmp")" ]; then
    echo "admin-gate guard: SELF-TEST FAIL (an ungated StaffAdmin page was not caught)"
    status=1
  fi
  # An `Always` surface is not this guard's business even ungated.
  if [ -n "$(run_guard "$tmp" | grep SettingsOpen)" ]; then
    echo "admin-gate guard: SELF-TEST FAIL (a non-admin surface was reported)"
    status=1
  fi

  # The same page, gated past the hop, is accepted.
  cat > "$tmp/src/pages/page_gated.rs" <<'EOF'
#[component]
pub fn GatedPage() -> Element {
    use_page_title("Gated");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Gated" } };
    }
    rsx! { GatedBody {} }
}

#[component]
fn GatedBody() -> Element {
    rsx! { div { "secrets" } }
}
EOF
  if [ -n "$(run_guard "$tmp")" ]; then
    echo "admin-gate guard: SELF-TEST FAIL (a gated page was rejected)"
    run_guard "$tmp"
    status=1
  fi

  if [ -n "$(run_guard .)" ]; then
    echo "admin-gate guard: SELF-TEST FAIL (this repository does not pass)"
    status=1
  fi

  [ "$status" -eq 0 ] && echo "admin-gate guard: self-test OK"
  exit "$status"
fi

hits="$(run_guard "${1:-.}")"
if [ -z "$hits" ]; then
  echo "admin-gate guard: OK"
  exit 0
fi
echo "admin-gate guard: FAIL"
echo "$hits"
echo
echo "A surface marked SurfaceVisibility::StaffAdmin is only HIDDEN from a"
echo "non-admin's hub. The route stays reachable, so the page needs its own"
echo "early return: \`if !crate::pages::settings::use_is_admin() { return rsx! {"
echo "AdminOnlyNotice { title: \"...\" } }; }\`, above the body it would"
echo "otherwise render and below every hook (MAPPS-602). MAPPS-1211."
exit 1
