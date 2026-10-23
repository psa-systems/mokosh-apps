#!/usr/bin/env nu

# PMS-1335 guard (ported to Nushell for MAPPS-1137): no em-dash (U+2014) or
# en-dash (U+2013) anywhere under src/, tests/ or docs/. Catches the
# character in rendered copy, in comments, and in markdown prose, which is
# the full scope of the global style rule.
#
# Nushell matches Unicode without depending on `grep -P` or the caller's
# locale, unlike the Bash original. The two dash characters are built from
# their codepoints at runtime (`char --unicode`), so this file contains
# neither and cannot trip itself.
#
# What fails: any line that contains U+2014 or U+2013 in one of the scanned
# roots. A file is skipped (and counted) as binary only if it contains a NUL
# byte; a file with no NUL byte that is not valid UTF-8 is an error, not a
# skip.
#
# Opt out on a line with `em-dash-guard-allow`, used by docs/copy-style.md's
# definition section (which has to spell the character to describe the
# rule).
#
# Usage: nu scripts/check-no-em-dash.nu [--self-test]
#   `--self-test` runs the guard over generated fixtures to prove it still
#   fails on an em-dash and still passes once the character is replaced.

const ROOTS = [src tests docs]
const ALLOW_MARKER = "em-dash-guard-allow"

def dash-chars [] {
    [(char --unicode "2014") (char --unicode "2013")]
}

# Scans one file. Returns {hits: [...], binary: bool}; raises an error for a
# non-UTF-8 file that has no NUL byte, instead of swallowing the decode
# failure.
def scan-file [path: string] {
    let raw = (open --raw $path | into binary)
    if ($raw | bytes index-of 0x[00]) != -1 {
        return {hits: [], binary: true}
    }

    let decoded = ($raw | decode utf-8)
    if ($decoded | into binary) != $raw {
        error make {msg: $"not valid UTF-8: ($path)"}
    }

    let dashes = (dash-chars)
    let hits = (
        $decoded
            | lines
            | enumerate
            | where {|row| not ($row.item | str contains $ALLOW_MARKER) and ($dashes | any {|d| $row.item | str contains $d }) }
            | each {|row| $"($path):($row.index + 1): ($row.item)" }
    )
    {hits: $hits, binary: false}
}

# Scans every file under the given roots. Returns {hits, binary_count}.
def scan [roots: list<string>] {
    mut hits = []
    mut binary_count = 0
    for root in $roots {
        for path in (glob --no-dir $"($root)/**/*") {
            let result = (scan-file $path)
            $hits = ($hits | append $result.hits)
            if $result.binary {
                $binary_count += 1
            }
        }
    }
    {hits: $hits, binary_count: $binary_count}
}

def run-self-test [] {
    let fixtures = (mktemp -d)
    mut failed = 0
    let em = (char --unicode "2014")

    $"let msg = \"stale ($em)refresh\"\n" | save -f ($fixtures | path join "bad.rs")
    if (scan-file ($fixtures | path join "bad.rs")).hits == [] {
        print "self-test: FAIL (an em-dash in a Rust string did not fail the guard)"
        $failed += 1
    } else {
        print "self-test: an em-dash in a Rust string fails the guard"
    }

    "let msg = \"stale, refresh\"\n" | save -f ($fixtures | path join "good.rs")
    if (scan-file ($fixtures | path join "good.rs")).hits != [] {
        print "self-test: FAIL (a line with no em-dash was flagged)"
        $failed += 1
    } else {
        print "self-test: a clean line passes the guard"
    }

    $"see rule: em-dash \(U+2014\) ($em) em-dash-guard-allow\n" | save -f ($fixtures | path join "allow.rs")
    if (scan-file ($fixtures | path join "allow.rs")).hits != [] {
        print "self-test: FAIL (the opt-out marker did not exempt the line)"
        $failed += 1
    } else {
        print "self-test: the em-dash-guard-allow marker exempts its line"
    }

    rm -rf $fixtures

    if $failed > 0 {
        print --stderr $"self-test: FAIL \(($failed) case\(s\) failed\)"
        exit 1
    }
    print "no-em-dash guard self-test: clean"
}

def main [--self-test] {
    if $self_test {
        run-self-test
        return
    }

    let result = (scan $ROOTS)
    print $"no-em-dash guard: skipped ($result.binary_count) binary file\(s\)"
    if ($result.hits | is-not-empty) {
        print --stderr "em-dash or en-dash found. Replace with a hyphen, a colon, or a period."
        for hit in $result.hits {
            print --stderr $hit
        }
        exit 1
    }
}
