#!/usr/bin/env bash
# MAPPS-967 guard: a `Card`'s heading comes from its `title` prop, never from an
# `h2`/`h3` written inside the body.
#
# MAPPS-966 wrote the rule down in `docs/form-conventions.md` ("Card headings use
# `title` / `subtitle`, never an inner `p-6`") and converted the profile page.
# The rest of the app kept 22 cards that wrote their own heading, which is how two
# heading styles coexisted: one with the CardHeader rule and inset, one without,
# differing in size and weight per page. MAPPS-967 converted them, and this is
# what stops the next one appearing.
#
# The rule it enforces: no `h2` or `h3` in the heading slot of a `Card {` block,
# which is the first rendered element, reached through any number of plain
# wrapper `div`s because `Card { div { h2 { ... } } }` is the same mistake one
# level deeper. Narrow on purpose beyond that. It does NOT try to judge a heading
# written as a styled `p`, or one further down a card body past real content,
# because a scan that guesses at those produces false positives and a guard
# people silence is worse than no guard.
#
# ALLOW MARKER: a card whose heading genuinely belongs in the body carries
#
#     // card-heading-allow: <reason>
#
# on the line before the heading. Four shapes legitimately need it, all found
# while converting:
#   - a centred empty state, where an icon comes first and the heading is part of
#     a centred stack (converting it strands the icon below the header rule);
#   - a document preview, where the heading is the DOCUMENT's title (an invoice
#     or credit note), not the card's label;
#   - a heading that is a focus target, carrying `tabindex="-1"` and an
#     `onmounted` that moves focus to it after a step change (the `title` prop is
#     a string and cannot hold either);
#   - an empty-state message whose first line is styled as a heading but is prose.
#
# Usage: check-card-headings.sh [--self-test]
set -euo pipefail

scan() {
    local root="$1"
    local violations=0
    local file
    while IFS= read -r file; do
        local n=0 in_card=0 allow=0 prefix
        while IFS= read -r line; do
            n=$((n + 1))
            local trimmed="${line#"${line%%[![:space:]]*}"}"
            case "$trimmed" in
                "// card-heading-allow:"*) allow=1; continue ;;
                "Card {"*|"Card{"*) in_card=1; continue ;;
            esac
            if [ "$in_card" = 1 ]; then
                # Blank lines and comments do not end the heading-slot window.
                case "$trimmed" in
                    ""|"//"*) continue ;;
                esac
                # The heading check comes FIRST. An element line carries
                # attributes (`h2 { class: ... }`), so a "looks like a prop"
                # test placed before it matches the heading too and the guard
                # silently passes everything, which is how the first version of
                # this script failed its own self-test.
                case "$trimmed" in
                    h2\ \{*|h2\{*|h3\ \{*|h3\{*)
                        if [ "$allow" = 0 ]; then
                            echo "  $file:$n: $trimmed"
                            violations=$((violations + 1))
                        fi
                        in_card=0
                        allow=0
                        continue
                        ;;
                esac
                # A prop on the Card itself (`title:`, `class:`, `padding:`,
                # `actions: rsx! {`) has a colon before its first brace, which
                # an element line never has.
                prefix="${trimmed%%\{*}"
                case "$prefix" in
                    *:*) continue ;;
                esac
                # A plain wrapper div delegates the heading slot to its own first
                # child, so keep looking inside it.
                case "$trimmed" in
                    div\ \{*|div\{*) continue ;;
                esac
                in_card=0
            fi
            allow=0
        done < "$file"
    done < <(find "$root" -name '*.rs' -type f | sort)
    return "$violations"
}

self_test() {
    local tmp
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    mkdir -p "$tmp/src"

    cat > "$tmp/src/good.rs" <<'EOF'
rsx! {
    Card {
        title: "Fine".to_string(),
        p { class: "text-sm", "body" }
    }
}
EOF
    if scan "$tmp/src"; then
        echo "self-test: a compliant card passes"
    else
        echo "self-test FAILED: a compliant card was reported" >&2
        return 1
    fi

    cat > "$tmp/src/bad.rs" <<'EOF'
rsx! {
    Card {
        h2 { class: "text-lg font-semibold", "Hand rolled" }
    }
}
EOF
    if scan "$tmp/src" > /dev/null; then
        echo "self-test FAILED: an in-body h2 was not reported" >&2
        return 1
    else
        echo "self-test: an in-body h2 fails the guard"
    fi

    cat > "$tmp/src/bad.rs" <<'EOF'
rsx! {
    Card {
        div { class: "space-y-4",
            h3 { class: "text-base font-medium", "Hand rolled, one div down" }
        }
    }
}
EOF
    if scan "$tmp/src" > /dev/null; then
        echo "self-test FAILED: a heading inside a wrapper div was not reported" >&2
        return 1
    else
        echo "self-test: a heading inside a wrapper div fails the guard"
    fi

    cat > "$tmp/src/bad.rs" <<'EOF'
rsx! {
    Card {
        p { class: "text-sm", "real content first" }
        h2 { class: "text-lg", "a later section, not the card's label" }
    }
}
EOF
    if scan "$tmp/src"; then
        echo "self-test: a heading past real content is left alone"
    else
        echo "self-test FAILED: a heading past real content was reported" >&2
        return 1
    fi

    cat > "$tmp/src/bad.rs" <<'EOF'
rsx! {
    Card {
        // card-heading-allow: a document preview's own title
        h2 { class: "text-2xl font-bold", "INVOICE" }
    }
}
EOF
    if scan "$tmp/src"; then
        echo "self-test: the allow marker exempts a card"
    else
        echo "self-test FAILED: the allow marker did not exempt" >&2
        return 1
    fi
    echo "card-heading guard self-test: clean"
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit 0
fi

if scan src; then
    echo "card headings: clean (every Card heading comes from its title prop)"
else
    count=$?
    echo "card-heading guard: FAIL ($count card(s) opening with an in-body h2/h3)" >&2
    echo "Move the text to the Card's \`title\` (and \`subtitle\`, and \`actions\` for a" >&2
    echo "heading-row control), per docs/form-conventions.md. If the heading genuinely" >&2
    echo "belongs in the body, put \`// card-heading-allow: <reason>\` on the line above." >&2
    exit 1
fi
