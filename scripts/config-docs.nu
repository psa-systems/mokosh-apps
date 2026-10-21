#!/usr/bin/env nu

# Renders and checks docs/configuration.md from config/variables.toml and the
# `${MOKOSH_...}` reads in oci-build/entrypoint.sh (MAPPS-1027).
#
# Mirrors mokosh-server's scripts/config-docs.nu (PMS-1442) but mokosh-apps
# has no Rust `declare_keys!` registry: every variable the image reads is
# documented in `config/variables.toml`. Discovery compares that file
# against the actual `${VAR}` reads in the entrypoint and fails if either
# side names something the other does not. PC-89 tracks moving the
# shared logic into psa-systems/common; until then this copy is the
# whole script.
#
# Subcommands: generate | check | since <version> | release-notes <version> [--ref] | self-test.

const VARIABLES_FILE = "config/variables.toml"
const ENTRYPOINT_FILE = "oci-build/entrypoint.sh"
const DOC_FILE = "docs/configuration.md"
const REPO_URL = "https://dev.a8n.run/psa-systems/mokosh-apps"
const WINDOW = 5
const BEGIN = "<!-- BEGIN GENERATED: recently-added (just config-docs) -->"
const END = "<!-- END GENERATED: recently-added -->"
const LIST_HEADING = "## All variables"
const GROUPS = ["Added" "Changed" "Deprecated" "Removed"]
# Variables that predate the first tag are dated here; nothing upgrades
# into it.
const BASELINE = "v0.1.0"
const LEVELS = { "wont-start": "Breaking: won't start", "feature-off": "Breaking: feature off" }

# The discovery regex matches `${MOKOSH_...}` only, so entrypoint-computed
# names (`MOKOSH_API_ORIGIN`, `MOKOSH_OIDC_ORIGIN` as bare assignments,
# `window.__MOKOSH_CONFIG__` as a JS global) and names in prose comments
# never reach `guard-errors`. If a future regex form picks them up, add a
# NOT_OPERATOR_CONFIG allow-list here with per-name reasons.
const NOT_OPERATOR_CONFIG = {}

# -- Versions -------------------------------------------------------------------

def vnum [v: string] {
    let p = ($v | parse --regex '^v(?<a>\d+)\.(?<b>\d+)\.(?<c>\d+)$')
    if ($p | is-empty) { error make { msg: $"($v) is not a vX.Y.Z release" } }
    (($p.a.0 | into int) * 1000000) + (($p.b.0 | into int) * 1000) + ($p.c.0 | into int)
}

def is-version [v: any] {
    ($v | describe) == "string" and ($v =~ '^v\d+\.\d+\.\d+$')
}

def release-tags [] {
    ^git tag --list "v*" | lines | where {|t| is-version $t }
}

def slug [name: string] {
    $"var-($name | str downcase | str replace --all '_' '-')"
}

def release-anchor [v: string] {
    $"release-($v | str replace --all '.' '-')"
}

# -- Inputs ---------------------------------------------------------------------

# Build the full catalog from variables.toml. Unlike mokosh-server, mokosh-apps
# has no Rust registry, so the TOML is the full source of truth.
def build-catalog [meta: record] {
    let vars = ($meta.variable? | default [] | each {|v|
        {
            name: $v.name
            since: $v.since
            required: ($v.required? | default false)
            required_since: ($v.required_since? | default null)
            source: "variables.toml"
            deprecated: ($v.deprecated? | default null)
            removed: null
        }
    })
    let removed = ($meta.removed? | default [] | each {|v|
        {
            name: $v.name
            since: $v.since
            required: false
            required_since: null
            source: "removed"
            deprecated: null
            removed: { release: $v.removed, issue: ($v.issue? | default ""), action: ($v.action? | default ""), level: ($v.breaking? | default "") }
        }
    })
    $vars | append $removed
}

def catalog-errors [catalog: list] {
    mut errors = []
    let dupes = ($catalog | group-by name | items {|k, v| { name: $k, n: ($v | length) } } | where n > 1)
    for d in $dupes {
        $errors = ($errors | append $"($d.name) is declared ($d.n) times in ($VARIABLES_FILE)")
    }
    for v in $catalog {
        for field in [$v.since $v.required_since ($v.deprecated?.release?) ($v.removed?.release?)] {
            if $field != null and not (is-version $field) {
                $errors = ($errors | append $"($v.name): '($field)' is not a vX.Y.Z release")
            }
        }
        if $v.required_since != null and (is-version $v.required_since) and (is-version $v.since) {
            if not $v.required or (vnum $v.required_since) <= (vnum $v.since) {
                $errors = ($errors | append $"($v.name): required_since needs `required` and a release after since")
            }
        }
    }
    $errors
}

# -- Changelog entries -----------------------------------------------------------

def default-action [e: record] {
    if $e.group != "Added" { return "" }
    if $e.required { "Required: set it before the container starts." } else { "Optional. Leave it unset to keep the default." }
}

def build-entries [catalog: list, meta: record] {
    mut auto = []
    for v in $catalog {
        let at_add = ($v.required and $v.required_since == null)
        let level = if $at_add and $v.since != $BASELINE { "wont-start" } else { "" }
        $auto = ($auto | append { release: $v.since, group: "Added", var: $v.name, required: $at_add, level: $level, feature: "", issue: "", action: "" })
        if $v.required_since != null {
            $auto = ($auto | append { release: $v.required_since, group: "Changed", var: $v.name, required: true, level: "wont-start", feature: "", issue: "", action: "" })
        }
        if $v.deprecated != null {
            let d = $v.deprecated
            $auto = ($auto | append { release: $d.release, group: "Deprecated", var: $v.name, required: false, level: "", feature: "", issue: ($d.issue? | default ""), action: $"Renamed to `($d.replacement)`. The old name still works and logs a warning naming the new one; rename it at your next compose edit." })
        }
        if $v.removed != null {
            let r = $v.removed
            $auto = ($auto | append { release: $r.release, group: "Removed", var: $v.name, required: false, level: $r.level, feature: "", issue: $r.issue, action: $r.action })
        }
    }

    mut errors = []
    mut explicit = []
    let names = ($catalog | get name)
    for c in ($meta.change? | default []) {
        if not ($c.group in $GROUPS) {
            $errors = ($errors | append $"change record for ($c.release): group '($c.group)' is not one of ($GROUPS | str join ', ')")
            continue
        }
        let level = ($c.breaking? | default "")
        if not ($level in ["" "wont-start" "feature-off"]) {
            $errors = ($errors | append $"change record ($c.release) ($c.group): breaking '($level)' is not wont-start or feature-off")
            continue
        }
        for var in ($c.variables? | default []) {
            if not ($var in $names) {
                $errors = ($errors | append $"change record ($c.release) ($c.group) names ($var), which is not in ($VARIABLES_FILE)")
                continue
            }
            let hit = ($auto | enumerate | where {|e| $e.item.release == $c.release and $e.item.group == $c.group and $e.item.var == $var })
            if ($hit | is-empty) {
                if $c.group != "Changed" {
                    $errors = ($errors | append $"change record ($c.release) ($c.group) ($var) does not match the facts: check its since / deprecated / removed release")
                    continue
                }
                let v = ($catalog | where name == $var | first)
                let req = ($v.required and ($v.required_since == null or (vnum $v.required_since) <= (vnum $c.release)))
                $explicit = ($explicit | append { release: $c.release, group: "Changed", var: $var, required: $req, level: $level, feature: ($c.feature? | default ""), issue: ($c.issue? | default ""), action: ($c.action? | default "") })
            } else {
                let i = $hit.0.index
                let e = $hit.0.item
                $auto = ($auto | update $i ($e | merge {
                    issue: ($c.issue? | default $e.issue)
                    action: ($c.action? | default $e.action)
                    level: (if ($level | is-empty) { $e.level } else { $level })
                    feature: ($c.feature? | default $e.feature)
                }))
            }
        }
    }

    let all = ($auto | append $explicit | each {|e|
        if ($e.action | is-empty) { $e | update action (default-action $e) } else { $e }
    })
    for e in $all {
        if ($e.level | is-not-empty) and (($e.action | is-empty) or not ($e.issue =~ '^[A-Z]+-\d+$')) {
            $errors = ($errors | append $"($LEVELS | get $e.level) ($e.group) ($e.var) in ($e.release) needs a [[change]] record in ($VARIABLES_FILE) with the operator action and the issue that introduced it")
        }
        if $e.level == "feature-off" and ($e.feature | str trim | is-empty) {
            $errors = ($errors | append $"Breaking: feature off ($e.group) ($e.var) in ($e.release) needs `feature` naming what stops working")
        }
        if ($e.issue | is-not-empty) and not ($e.issue =~ '^[A-Z]+-\d+$') {
            $errors = ($errors | append $"($e.var) in ($e.release): issue '($e.issue)' is not an issue id")
        }
    }
    { entries: $all, errors: $errors }
}

def window [entries: list, tags: list] {
    let sorted = ($tags | sort-by {|t| vnum $t } --reverse)
    let latest = if ($sorted | is-empty) { 0 } else { vnum $sorted.0 }
    let unreleased = ($entries | get release | uniq | where {|r| (vnum $r) > $latest } | sort-by {|r| vnum $r } --reverse)
    { unreleased: $unreleased, released: ($sorted | first ([$WINDOW ($sorted | length)] | math min)) }
}

def level-rank [level: string] {
    match $level { "wont-start" => 0, "feature-off" => 1, _ => 2 }
}

def entry-sort [rows: list] {
    $rows | sort-by {|e| $"(level-rank $e.level)(if $e.required { 0 } else { 1 })($e.var)" } # nu-interp-guard-allow
}

def render-entry [e: record] {
    let mark = if ($e.level | is-empty) { "" } else { $"**($LEVELS | get $e.level):** " }
    let req = if $e.required { "yes" } else { "no" }
    let feature = if $e.level == "feature-off" { $"Feature: ($e.feature). " } else { "" }
    let issue = if ($e.issue | is-empty) { "" } else { $" \(($e.issue))" }
    $"- ($mark)[`($e.var)`]\(#(slug $e.var)) \(Required: ($req)) - ($feature)($e.action)($issue)" # nu-interp-guard-allow
}

def render-recent [entries: list, tags: list] {
    let w = (window $entries $tags)
    mut out = [
        $BEGIN
        ""
        $"Configuration changes in the last ($WINDOW) releases, newest first, rendered by `just config-docs` from `($VARIABLES_FILE)` \(edit that, not this section). **Breaking: won't start** marks a change after which the container refuses to start until the operator acts; apply it before restarting. **Breaking: feature off** marks a change after which the image runs but the named feature stops working or changes until the operator acts; review it before restarting. Both name the operator action and the issue that introduced them. An unmarked entry needs no action unless you relied on the old behavior. Upgrading across more releases than this? Run `just config-since <running version>`."
    ]
    let versions = ($w.unreleased | each {|v| { v: $v, label: $"Unreleased \(($v))" } } | append ($w.released | each {|v| { v: $v, label: $v } }))
    for r in $versions {
        $out = ($out | append ["" $"<a id=\"(release-anchor $r.v)\"></a>" "" $"### ($r.label)"]) # nu-interp-guard-allow
        let rows = ($entries | where release == $r.v)
        if ($rows | is-empty) {
            $out = ($out | append ["" "No configuration changes."])
            continue
        }
        for g in $GROUPS {
            let group_rows = (entry-sort ($rows | where group == $g))
            if ($group_rows | is-empty) { continue }
            $out = ($out | append ["" $"#### ($g)" ""] | append ($group_rows | each {|e| render-entry $e }))
        }
    }
    $out | append ["" $END] | str join "\n"
}

# -- The full list ----------------------------------------------------------------

def expected-required [v: record] {
    if $v.required { "yes" } else { "no" }
}

def expected-added [v: record] {
    mut cell = $v.since
    if $v.required_since != null { $cell = $"($cell); required since ($v.required_since)" }
    if $v.deprecated != null { $cell = $"($cell); deprecated in ($v.deprecated.release), use `($v.deprecated.replacement)`" }
    if $v.removed != null { $cell = $"($cell); removed in ($v.removed.release)" }
    $cell
}

const ROW_RE = '^\| <a id="(?<anchor>[^"]*)"></a>`(?<var>[A-Z][A-Z0-9_]*)` \| (?<req>[^|]*?) \| (?<added>[^|]*?) \| (?<rest>.*)$'

def list-bounds [lines: list] {
    let start = ($lines | enumerate | where item == $LIST_HEADING | get 0?.index)
    if $start == null { return null }
    let after = ($lines | skip ($start + 1) | enumerate | where {|r| $r.item starts-with "## " } | get 0?.index)
    let stop = if $after == null { $lines | length } else { $start + 1 + $after }
    { start: $start, stop: $stop }
}

def recent-bounds [lines: list] {
    let b = ($lines | enumerate | where item == $BEGIN | get index)
    let e = ($lines | enumerate | where item == $END | get index)
    if ($b | length) != 1 or ($e | length) != 1 or $b.0 > $e.0 { return null }
    { start: $b.0, stop: $e.0 }
}

# -- Check ----------------------------------------------------------------------

def check-list [lines: list, catalog: list] {
    let bounds = (list-bounds $lines)
    if $bounds == null { return [$"($DOC_FILE): no '($LIST_HEADING)' section"] }
    mut errors = []
    mut seen = []
    for line in ($lines | skip ($bounds.start + 1) | first ($bounds.stop - $bounds.start - 1)) {
        if not ($line starts-with "| ") or ($line starts-with "| Variable") or ($line starts-with "| ---") { continue }
        let row = ($line | parse --regex $ROW_RE)
        if ($row | is-empty) {
            $errors = ($errors | append $"($DOC_FILE): a row in '($LIST_HEADING)' is not `| <a id=...></a>`NAME` | Required | Added in | ...`: ($line | str substring 0..80)")
            continue
        }
        let r = $row.0
        $seen = ($seen | append $r.var)
        let found = ($catalog | where name == $r.var)
        if ($found | is-empty) {
            $errors = ($errors | append $"($DOC_FILE): row ($r.var) names a variable ($VARIABLES_FILE) does not list")
            continue
        }
        let v = $found.0
        if $r.anchor != (slug $r.var) {
            $errors = ($errors | append $"($DOC_FILE): row ($r.var) has anchor '($r.anchor)', expected '(slug $r.var)'") # nu-interp-guard-allow
        }
        let req = ($r.req | parse --regex '^(?<t>yes|no)\b' | get 0?.t)
        if $req != (expected-required $v) {
            $errors = ($errors | append $"($DOC_FILE): row ($r.var) says Required '($r.req)', the facts say '(expected-required $v)'") # nu-interp-guard-allow
        }
        if $r.added != (expected-added $v) {
            $errors = ($errors | append $"($DOC_FILE): row ($r.var) says Added in '($r.added)', the facts say '(expected-added $v)'") # nu-interp-guard-allow
        }
    }
    for v in $catalog {
        let n = ($seen | where {|s| $s == $v.name } | length)
        if $n == 0 { $errors = ($errors | append $"($DOC_FILE): ($v.name) \(($v.source)) has no row in '($LIST_HEADING)'; run `just config-docs`, then describe it") }
        if $n > 1 { $errors = ($errors | append $"($DOC_FILE): ($v.name) has ($n) rows") }
    }
    $errors
}

const ENTRY_RE = r#'^- (?:\*\*Breaking: (?<b>won't start|feature off):\*\* )?\[`(?<var>[A-Z][A-Z0-9_]*)`\]\(#(?<anchor>[a-z0-9-]+)\) \(Required: (?<req>yes|no)\) - (?<text>.+)$'#

def check-recent [lines: list, entries: list, tags: list] {
    let bounds = (recent-bounds $lines)
    if $bounds == null { return [$"($DOC_FILE): the Recently added markers are missing or out of order; run `just config-docs`"] }
    let anchors = ($lines | str join "\n" | parse --regex '<a id="(?<id>[^"]+)"></a>' | get id)
    mut errors = []
    mut found = []
    mut release: any = null
    mut group: any = null
    for line in ($lines | skip ($bounds.start + 1) | first ($bounds.stop - $bounds.start - 1)) {
        if ($line starts-with "### ") {
            $release = ($line | parse --regex '(?<v>v\d+\.\d+\.\d+)' | get 0?.v)
            $group = null
            continue
        }
        if ($line starts-with "#### ") {
            $group = ($line | str replace "#### " "" | str trim)
            continue
        }
        if not ($line starts-with "- ") { continue }
        let p = ($line | parse --regex $ENTRY_RE)
        if ($p | is-empty) or $release == null or not ($group in $GROUPS) {
            $errors = ($errors | append $"($DOC_FILE): malformed Recently added entry: ($line | str substring 0..80)")
            continue
        }
        let d = $p.0
        let exp = ($entries | where {|e| $e.release == $release and $e.group == $group and $e.var == $d.var })
        if ($exp | is-empty) {
            $errors = ($errors | append $"($DOC_FILE): ($release) ($group) ($d.var) is not a change ($VARIABLES_FILE) records")
            continue
        }
        let e = $exp.0
        $found = ($found | append $"($release)|($group)|($d.var)")
        let req = if $e.required { "yes" } else { "no" }
        if $d.req != $req {
            $errors = ($errors | append $"($DOC_FILE): ($release) ($group) ($d.var) says Required: ($d.req), the facts say ($req)")
        }
        let want = if ($e.level | is-empty) { "" } else { $LEVELS | get $e.level | str replace "Breaking: " "" }
        if ($d.b | default "") != $want {
            $errors = ($errors | append $"($DOC_FILE): ($release) ($group) ($d.var) mark is '($d.b | default "none")', the facts say '(if ($want | is-empty) { "none" } else { $want })'") # nu-interp-guard-allow
        }
        if ($e.level | is-not-empty) and not ($d.text | str contains $e.issue) {
            $errors = ($errors | append $"($DOC_FILE): ($release) Breaking ($d.var) does not name its issue ($e.issue)")
        }
        if $e.level == "feature-off" and not ($d.text | str starts-with $"Feature: ($e.feature). ") {
            $errors = ($errors | append $"($DOC_FILE): ($release) feature-off ($d.var) does not name its feature '($e.feature)'")
        }
        if $d.anchor != (slug $d.var) or not ($d.anchor in $anchors) {
            $errors = ($errors | append $"($DOC_FILE): ($release) ($d.var) links to #($d.anchor), which is not that variable's row anchor")
        }
    }
    let w = (window $entries $tags)
    let covered = ($w.unreleased | append $w.released)
    for e in ($entries | where {|e| $e.release in $covered }) {
        if not ($"($e.release)|($e.group)|($e.var)" in $found) {
            let what = if ($e.level | is-empty) { "" } else { $"($LEVELS | get $e.level) " }
            $errors = ($errors | append $"($DOC_FILE): ($what)($e.group) ($e.var) in ($e.release) is missing from Recently added; run `just config-docs`")
        }
    }
    $errors
}

# -- Guard: entrypoint reads vs variables.toml -----------------------------------

def discover-entrypoint [text: string] {
    # `${MOKOSH_FOO}`, `${MOKOSH_FOO:-}`, `${MOKOSH_FOO:-default}`. The
    # entrypoint's `window.__MOKOSH_CONFIG__ = { ... }` writes the field
    # names of the global, not an env read, so the regex deliberately
    # requires the `${` prefix.
    $text
        | parse --regex '\$\{(?<n>MOKOSH_[A-Z0-9_]+)(?::-[^}]*)?\}'
        | get n
        | uniq
        | sort
}

def guard-errors [discovered: list, catalog: list] {
    let listed = ($catalog | where source == "variables.toml" | get name)
    let removed = ($catalog | where source == "removed" | get name)
    mut errors = []
    for n in $discovered {
        if ($n in $listed) or ($n in $removed) or ($n in ($NOT_OPERATOR_CONFIG | columns)) { continue }
        $errors = ($errors | append $"($n) is read in ($ENTRYPOINT_FILE) but is not in ($VARIABLES_FILE); add a [[variable]] with since and required, or a NOT_OPERATOR_CONFIG reason in scripts/config-docs.nu")
    }
    for n in $listed {
        if not ($n in $discovered) {
            $errors = ($errors | append $"($VARIABLES_FILE): ($n) is listed but ($ENTRYPOINT_FILE) does not read it; move it to [[removed]] or drop it")
        }
    }
    $errors
}

# The NOT_OPERATOR_CONFIG dead-entry check runs against the real
# entrypoint only (not the self-test fixture), because the fixture
# deliberately omits those entrypoint-computed names: a NOT entry that
# the fixture does not reference is not a defect.
def not-operator-config-dead [discovered: list] {
    mut errors = []
    for n in ($NOT_OPERATOR_CONFIG | columns) {
        if not ($n in $discovered) {
            $errors = ($errors | append $"NOT_OPERATOR_CONFIG: ($n) is no longer read in ($ENTRYPOINT_FILE); drop the entry")
        }
    }
    $errors
}

# -- Generate -------------------------------------------------------------------

def generate-doc [doc: string, catalog: list, entries: list, tags: list] {
    mut lines = ($doc | lines)
    let rb = (recent-bounds $lines)
    if $rb == null { error make { msg: $"($DOC_FILE): add the two marker lines under '## Recently added' first:\n($BEGIN)\n($END)" } }
    let recent = (render-recent $entries $tags | lines)
    $lines = ($lines | first $rb.start | append $recent | append ($lines | skip ($rb.stop + 1)))

    let lb = (list-bounds $lines)
    if $lb == null { error make { msg: $"($DOC_FILE): no '($LIST_HEADING)' section" } }
    mut seen = []
    mut last_row: any = null
    for i in ($lb.start + 1)..<($lb.stop) {
        let line = ($lines | get $i)
        let row = ($line | parse --regex $ROW_RE)
        if ($row | is-empty) { continue }
        let r = $row.0
        $last_row = $i
        $seen = ($seen | append $r.var)
        let found = ($catalog | where name == $r.var)
        if ($found | is-empty) { continue }
        let v = $found.0
        let req = ($r.req | str replace --regex '^(yes|no)\b' (expected-required $v))
        let req = if ($req =~ '^(yes|no)\b') { $req } else { expected-required $v }
        $lines = ($lines | update $i $"| <a id=\"(slug $r.var)\"></a>`($r.var)` | ($req) | (expected-added $v) | ($r.rest)") # nu-interp-guard-allow
    }
    let missing = ($catalog | where {|v| not ($v.name in $seen) })
    if ($missing | is-not-empty) {
        if $last_row == null { error make { msg: $"($DOC_FILE): '($LIST_HEADING)' has no table to add rows to" } }
        let new_rows = ($missing | each {|v| $"| <a id=\"(slug $v.name)\"></a>`($v.name)` | (expected-required $v) | (expected-added $v) | compose variable | Not described yet: say what it does. |" }) # nu-interp-guard-allow
        $lines = ($lines | first ($last_row + 1) | append $new_rows | append ($lines | skip ($last_row + 1)))
    }
    $"($lines | str join "\n")\n"
}

# -- Repo plumbing ----------------------------------------------------------------

def load-repo [] {
    let meta = (open --raw $VARIABLES_FILE | decode utf-8 | from toml)
    let catalog = (build-catalog $meta)
    let built = (build-entries $catalog $meta)
    { catalog: $catalog, entries: $built.entries, errors: ((catalog-errors $catalog) | append $built.errors), tags: (release-tags) }
}

def entrypoint-text [] {
    open --raw $ENTRYPOINT_FILE | decode utf-8
}

def fail-on [errors: list, what: string] {
    if ($errors | is-empty) { return }
    print --stderr $"ERROR: ($what)"
    for e in $errors { print --stderr $"  ($e)" }
    exit 1
}

def main [] {
    print "usage: nu scripts/config-docs.nu <generate|check|since <version>|release-notes <version>|self-test>"
}

def "main generate" [] {
    let repo = (load-repo)
    fail-on $repo.errors "the configuration facts are inconsistent"
    let doc = (open --raw $DOC_FILE | decode utf-8)
    let next = (generate-doc $doc $repo.catalog $repo.entries $repo.tags)
    if $next == $doc { print $"($DOC_FILE) is current"; return }
    $next | save --force $DOC_FILE
    print $"rewrote the generated parts of ($DOC_FILE)"
}

def "main check" [] {
    let repo = (load-repo)
    let lines = (open --raw $DOC_FILE | decode utf-8 | lines)
    let discovered = (discover-entrypoint (entrypoint-text))
    let errors = (
        $repo.errors
        | append (guard-errors $discovered $repo.catalog)
        | append (not-operator-config-dead $discovered)
        | append (check-list $lines $repo.catalog)
        | append (check-recent $lines $repo.entries $repo.tags)
    )
    fail-on $errors $"($DOC_FILE) disagrees with the configuration facts \(MAPPS-1027)"
    print $"config docs OK: ($repo.catalog | length) variables, ($repo.entries | length) changelog entries"
}

def "main since" [
    version: string
    --json
] {
    let repo = (load-repo)
    let after = ($repo.entries | where {|e| (vnum $e.release) > (vnum $version) })
    let sorted = ($after | sort-by {|e| $"((vnum $e.release) + 1000000000)(level-rank $e.level)($e.var)" }) # nu-interp-guard-allow
    if $json {
        print ($sorted | each {|e| { release: $e.release, group: $e.group, variable: $e.var, required: $e.required, level: (if ($e.level | is-empty) { "none" } else { $e.level }), feature: $e.feature, issue: $e.issue, action: $e.action } } | to json)
        return
    }
    if ($after | is-empty) { print $"No configuration changes after ($version)."; return }
    let rows = ($sorted | each {|e|
        {
            release: $e.release
            change: $e.group
            variable: $e.var
            required: (if $e.required { "REQUIRED" } else { "no" })
            level: (match $e.level { "wont-start" => "WON'T START", "feature-off" => $"FEATURE OFF: ($e.feature)", _ => "" })
            issue: $e.issue
            action: $e.action
        }
    })
    print ($rows | table --expand --width 200)
    let wont = ($after | where level == "wont-start" | length)
    let off = ($after | where level == "feature-off" | length)
    print $"($rows | length) changes after ($version): ($wont) WON'T START \(apply before restarting), ($off) FEATURE OFF \(review and acknowledge before restarting)."
}

def "main release-notes" [
    version: string
    --ref: string
] {
    let repo = (load-repo)
    let ref = ($ref | default $version)
    let rows = ($repo.entries | where release == $version)
    let link = $"($REPO_URL)/src/tag/($ref)/($DOC_FILE)#(release-anchor $version)" # nu-interp-guard-allow
    let link = if $ref == $version { $link } else { $"($REPO_URL)/src/branch/($ref)/($DOC_FILE)#(release-anchor $version)" } # nu-interp-guard-allow
    print "## Configuration changes"
    print ""
    if ($rows | is-empty) {
        print $"No configuration changes in ($version). Full list: [docs/configuration.md]\(($link))."
        return
    }
    let breaking = ($rows | where {|e| $e.level | is-not-empty } | sort-by {|e| $"(level-rank $e.level)($e.var)" }) # nu-interp-guard-allow
    let wont = ($breaking | where level == "wont-start" | length)
    print $"[Configuration changes in ($version)]\(($link)): ($rows | length) entries, ($wont) won't start, (($breaking | length) - $wont) feature off." # nu-interp-guard-allow
    if ($breaking | is-not-empty) {
        print ""
        for e in $breaking {
            let feature = if $e.level == "feature-off" { $"Feature: ($e.feature). " } else { "" }
            print $"- **($LEVELS | get $e.level):** `($e.var)` - ($feature)($e.action) \(($e.issue))"
        }
    }
}

# -- Self-test --------------------------------------------------------------------

def fixture [] {
    let meta = {
        variable: [
            { name: "MOKOSH_OLD_KEY", since: "v0.1.0", required: true, reader: $ENTRYPOINT_FILE }
            { name: "MOKOSH_OPT_KEY", since: "v9.1.0", required: false, reader: $ENTRYPOINT_FILE }
            { name: "MOKOSH_LATE_REQ", since: "v9.0.0", required: true, required_since: "v9.1.0", reader: $ENTRYPOINT_FILE }
            { name: "MOKOSH_BOOT_KEY", since: "v9.1.0", required: false, reader: $ENTRYPOINT_FILE }
        ]
        removed: [
            { name: "MOKOSH_GONE_KEY", since: "v9.0.0", removed: "v9.1.0", issue: "MAPPS-2", action: "Delete it." }
        ]
        change: [
            { release: "v9.1.0", group: "Changed", variables: ["MOKOSH_LATE_REQ"], issue: "MAPPS-1", breaking: "wont-start", action: "Set it." }
            { release: "v9.1.0", group: "Changed", variables: ["MOKOSH_OLD_KEY"], issue: "MAPPS-4", breaking: "feature-off", feature: "Widget export", action: "Move it." }
        ]
    }
    let entrypoint_text = '
value="${MOKOSH_OLD_KEY:-default}"
echo "${MOKOSH_OPT_KEY}"
echo "${MOKOSH_LATE_REQ:-}"
echo "${MOKOSH_BOOT_KEY:-}"
'
    { meta: $meta, tags: ["v0.1.0" "v9.0.0" "v9.1.0"], entrypoint_text: $entrypoint_text }
}

def fixture-doc [] {
    [
        "# Configuration"
        ""
        "## Recently added"
        ""
        $BEGIN
        $END
        ""
        $LIST_HEADING
        ""
        "| Variable | Required | Added in | Where set | Purpose |"
        "| --- | --- | --- | --- | --- |"
        '| <a id="var-mokosh-old-key"></a>`MOKOSH_OLD_KEY` | yes | v0.1.0 | compose variable | Old. |'
        '| <a id="var-mokosh-opt-key"></a>`MOKOSH_OPT_KEY` | no | v9.1.0 | compose variable | Opt. |'
        '| <a id="var-mokosh-late-req"></a>`MOKOSH_LATE_REQ` | yes | v9.0.0; required since v9.1.0 | compose variable | Late. |'
        '| <a id="var-mokosh-boot-key"></a>`MOKOSH_BOOT_KEY` | no | v9.1.0 | compose variable | Boot. |'
        '| <a id="var-mokosh-gone-key"></a>`MOKOSH_GONE_KEY` | no | v9.0.0; removed in v9.1.0 | compose variable | Gone. |'
        ""
    ] | str join "\n"
}

def all-errors [meta: record, tags: list, doc: string, entrypoint_text: string] {
    let catalog = (build-catalog $meta)
    let built = (build-entries $catalog $meta)
    let lines = ($doc | lines)
    (catalog-errors $catalog)
        | append $built.errors
        | append (guard-errors (discover-entrypoint $entrypoint_text) $catalog)
        | append (check-list $lines $catalog)
        | append (check-recent $lines $built.entries $tags)
}

def generated [f: record] {
    let catalog = (build-catalog $f.meta)
    let built = (build-entries $catalog $f.meta)
    generate-doc (fixture-doc) $catalog $built.entries $f.tags
}

def "main self-test" [] {
    let f = (fixture)
    let good = (generated $f)
    let new_req_meta = ($f.meta
        | update variable ($f.meta.variable | append { name: "MOKOSH_NEW_REQ", since: "v9.1.0", required: true, reader: $ENTRYPOINT_FILE })
        | update change ($f.meta.change | append { release: "v9.1.0", group: "Added", variables: ["MOKOSH_NEW_REQ"], issue: "MAPPS-3", action: "Set it first." }))
    let new_req_entry = ($f.entrypoint_text + "\necho \"${MOKOSH_NEW_REQ:-}\"\n")
    let new_req_doc = (generated { meta: $new_req_meta, tags: $f.tags, entrypoint_text: $new_req_entry })
    let cases = [
        [name, errors, expect];
        ["a generated doc passes", (all-errors $f.meta $f.tags $good $f.entrypoint_text), 0]
        ["generating twice changes nothing", (do {
            let catalog = (build-catalog $f.meta)
            let again = (generate-doc $good $catalog (build-entries $catalog $f.meta).entries $f.tags)
            if $again == $good { [] } else { ["the second run rewrote the doc"] }
        }), 0]
        ["a key without a row fails", (all-errors $f.meta $f.tags ($good | lines | where {|l| not ($l | str contains '`MOKOSH_OPT_KEY` |') } | str join "\n") $f.entrypoint_text), 1]
        ["a Required mismatch fails", (all-errors $f.meta $f.tags ($good | str replace '`MOKOSH_OLD_KEY` | yes' '`MOKOSH_OLD_KEY` | no') $f.entrypoint_text), 1]
        ["an Added in mismatch fails", (all-errors $f.meta $f.tags ($good | str replace '`MOKOSH_OPT_KEY` | no | v9.1.0' '`MOKOSH_OPT_KEY` | no | v9.0.0') $f.entrypoint_text), 1]
        ["a new required key passes with its Breaking entry", (all-errors $new_req_meta $f.tags $new_req_doc $new_req_entry), 0]
        ["a new required key missing from the changelog fails", (all-errors $new_req_meta $f.tags ($new_req_doc | lines | where {|l| not (($l starts-with "- ") and ($l | str contains "MOKOSH_NEW_REQ")) } | str join "\n") $new_req_entry), 1]
        ["a new required key without its Breaking mark fails", (all-errors $new_req_meta $f.tags ($new_req_doc | str replace "- **Breaking: won't start:** [`MOKOSH_NEW_REQ`]" '- [`MOKOSH_NEW_REQ`]') $new_req_entry), 1]
        ["a feature-off entry whose doc line drops the feature fails", (all-errors $f.meta $f.tags ($good | str replace "Feature: Widget export. " "") $f.entrypoint_text), 1]
        ["an entrypoint read not in variables.toml fails", (guard-errors ["MOKOSH_OLD_KEY" "MOKOSH_SURPRISE_KEY"] (build-catalog $f.meta)), 1]
        ["a listed variable the entrypoint does not read fails", (guard-errors ["MOKOSH_OLD_KEY" "MOKOSH_OPT_KEY" "MOKOSH_LATE_REQ"] (build-catalog $f.meta)), 1]
        ["discover-entrypoint finds every \${MOKOSH_...} form", (do {
            let found = (discover-entrypoint '${MOKOSH_A:-} and ${MOKOSH_B} and ${MOKOSH_C:-x}')
            ["MOKOSH_A" "MOKOSH_B" "MOKOSH_C"] | where {|n| not ($n in $found) }
        }), 0]
        ["an unknown breaking level fails", (build-entries (build-catalog $f.meta) ($f.meta | update change ($f.meta.change | update 0 {|c| $c | update breaking "maybe" })) | get errors), 1]
        ["a feature-off record without a feature fails", (build-entries (build-catalog $f.meta) ($f.meta | update change ($f.meta.change | update 1 {|c| $c | reject feature })) | get errors), 1]
    ]
    mut failed = 0
    for c in $cases {
        let n = ($c.errors | length)
        if ($c.expect == 0 and $n == 0) or ($c.expect > 0 and $n > 0) {
            print $"self-test: ($c.name)"
        } else {
            let want = if $c.expect == 0 { "no errors" } else { "an error" }
            print --stderr $"self-test: FAIL \(($c.name): expected ($want), got ($n))"
            for e in $c.errors { print --stderr $"    ($e)" }
            $failed += 1
        }
    }
    if $failed > 0 { exit 1 }
    print "config-docs self-test: clean"
}
