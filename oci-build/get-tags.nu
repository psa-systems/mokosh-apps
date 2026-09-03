#!/usr/bin/env nu

# Resolve the single publish tag and train for the build workflow.
#
# The publish MODE is derived from the workflow TRIGGER (not from `git
# describe`) and passed in via --mode:
# - release: an exact `v*` tag push. Publishes ONLY the immutable <version> artifact.
# - latest:  a push to main. Publishes ONLY the rolling `latest` artifact.
# - branch:  a push to an allow-listed feature branch (MAPPS-421). Publishes
#            ONLY a tag derived from the branch name (`/` -> `-`), so staging can
#            run the branch while `:latest` stays a main-only artifact. The
#            allow-list is the workflow's `on: push: branches:` filter.
# - dry-run: a workflow_dispatch. The caller builds and prints but mutates nothing.
#
# Deriving the mode from the trigger is what removes the twin-publish race
# (governance GOV-13 / claude-run CLAUDE-122). The release commit is
# simultaneously a push to `main` AND the `v*` tag push, so both workflow
# events fire. With `git describe` as the source of truth, both runs resolved
# the identical `[vX.Y.Z, latest]` set and raced to write the same destination
# (a 409 on immutable generic-package files, an overwrite race on the OCI
# `latest` tag). Trigger-derived modes return DISJOINT tag sets: the tag-push
# run publishes only `vX.Y.Z`, the branch-push run publishes only `latest`, so
# the two runs never write the same destination.
#
# A dry-run still resolves to one of the two real publish trains so the caller
# can exercise either path:
# - --simulate-tag v9.9.9  -> resolve the `release` train (prints the exact
#                             <version> URLs a tag build would write).
# - --simulate-tag "" (default) -> resolve the `latest` train.
#
# Returns a record { mode, train, tag, describe }:
# - train: release | latest | branch (the effective publish train)
# - tag:   <version> for release (e.g. v1.2.3), `latest` for latest, or the
#          sanitized branch name for branch
# - describe: `git describe --tags --always`, kept for build-metadata /
#             diagnostics only; it no longer decides the train.
#
# When used as a module (`use get-tags.nu`) it returns the record. When run as a
# script for a workflow step, pass --json to serialize the record for capture
# (e.g. `^nu oci-build/get-tags.nu --mode latest --json | from json`).
export def main [
    --mode: string                  # release | latest | branch | dry-run (from the trigger)
    --ref-name: string = ""         # ref name: the version for release mode (e.g. v1.2.3), the branch for branch mode
    --simulate-tag: string = ""      # dry-run only: simulate a release of this version
    --json(-j)                       # Serialize the record to JSON for shell capture
] {
    use std log
    let describe = (^git describe --tags --always | str trim)
    log info $"[get-tags] mode: ($mode) ref-name: ($ref_name) simulate-tag: ($simulate_tag) describe: ($describe)"

    # Resolve the effective publish train and its version (if any). A dry-run
    # maps onto a real train so both publish paths can be exercised without a
    # registry mutation.
    let effective = if $mode == "release" {
        { train: "release", version: $ref_name }
    } else if $mode == "latest" {
        { train: "latest", version: "" }
    } else if $mode == "branch" {
        { train: "branch", version: "" }
    } else if $mode == "dry-run" {
        if ($simulate_tag | is-not-empty) {
            { train: "release", version: $simulate_tag }
        } else {
            { train: "latest", version: "" }
        }
    } else {
        error make { msg: $"[get-tags] Unknown mode: '($mode)'. Expected release|latest|branch|dry-run." }
    }

    if $effective.train == "release" and ($effective.version | is-empty) {
        error make { msg: "[get-tags] release train requires a non-empty version (--ref-name for a tag build, or --simulate-tag for a dry-run)." }
    }

    if $effective.train == "branch" and ($ref_name | is-empty) {
        error make { msg: "[get-tags] branch train requires a non-empty --ref-name (the pushed branch name)." }
    }

    let tag = if $effective.train == "release" {
        $effective.version
    } else if $effective.train == "branch" {
        ($ref_name | str replace --all "/" "-")
    } else {
        "latest"
    }

    # A branch tag must never collide with the main train: `latest` is published
    # from main only, so a branch that resolves to it is a config error.
    if $effective.train == "branch" and $tag == "latest" {
        error make { msg: $"[get-tags] branch '($ref_name)' resolves to the reserved tag 'latest', which is published from main only." }
    }

    # `/` is handled above, but a branch name can still hold characters an OCI
    # tag cannot. Reject it here rather than deep inside buildx.
    if not ($tag =~ '^[a-zA-Z0-9_][a-zA-Z0-9._-]{0,127}$') {
        error make { msg: $"[get-tags] Resolved tag '($tag)' is not a valid OCI tag: it must start with an alphanumeric or underscore and hold only alphanumerics, dot, dash and underscore, up to 128 characters." }
    }

    log info $"[get-tags] Resolved train: ($effective.train) tag: ($tag)"

    let resolved = {
        mode: $mode
        train: $effective.train
        tag: $tag
        describe: $describe
    }

    if $json { $resolved | to json --raw } else { $resolved }
}
