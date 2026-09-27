//! Operator-flipped switches for work that is in the tree but not finished
//! (PMS-1337).
//!
//! The first one is organizations. The feature is not built out, its charging
//! model is unresolved, and the control for it rendered inconsistently: the
//! switcher trigger appears only for an identity holding two or more
//! memberships, while "Create new team" sits in the user menu so a
//! single-membership identity can still start one, so the control came and went
//! as memberships loaded and came back on a hard refresh. What the operator saw
//! was a control for a feature that does not work, behaving differently between
//! refreshes.
//!
//! HIDDEN rather than greyed out, which is the opposite of what PMS-1334 did to
//! the invoice actions, and the difference is the point. A greyed control with a
//! reason is right for an action that exists and is refused in this state: the
//! operator learns what would make it work. Organizations is not refused, it is
//! unbuilt, and "greyed because we have not written it yet" is an announcement
//! rather than an explanation. So the flag removes it, and the flag itself is
//! where a reader finds out why.
//!
//! Default off. Turning it on is a deployment decision, not a code change:
//!
//! * Browser: `window.__MOKOSH_CONFIG__.organizations_enabled`, which the
//!   `mokosh-www` image's entrypoint writes from `MOKOSH_ORGANIZATIONS_ENABLED`.
//! * Desktop: `MOKOSH_ORGANIZATIONS_ENABLED` in the environment, or
//!   `organizations_enabled` in the per-user `config.json`.
//!
//! Both read through [`crate::modules::runtime_config`], which treats an absent
//! value and an empty one the same way, so an operator who leaves the variable
//! blank gets the default rather than a parse error.
//!
//! Workspace sharing is in flight separately and will want this decision
//! revisited when it lands; the flag is the thing it revisits.

/// Values that mean yes. Anything else, including nothing at all, means no,
/// because the default has to be off for an unfinished feature and a typo must
/// not be the thing that ships it.
const TRUTHY: [&str; 3] = ["1", "true", "yes"];

/// Whether the organizations surface (the tenant switcher and the create-team
/// action beside it) is offered at all.
pub fn organizations_enabled() -> bool {
    crate::modules::runtime_config::get("organizations_enabled")
        .map(|raw| is_truthy(&raw))
        .unwrap_or(false)
}

/// The parse, split out so it can be tested without a browser or an
/// environment: `runtime_config::get` needs one or the other.
fn is_truthy(raw: &str) -> bool {
    let value = raw.trim().to_ascii_lowercase();
    TRUTHY.contains(&value.as_str())
}

#[cfg(test)]
mod tests {
    use super::is_truthy;

    /// PMS-1337: every way into the organizations surface is behind the flag.
    ///
    /// Two entry points exist and the bug was that they were gated differently
    /// from each other: the switcher on `memberships >= 2`, the create action on
    /// nothing at all. So this counts the gate rather than trusting a reader to
    /// notice a third one arriving: `TenantSwitcher {}` is rendered once, the
    /// create-team signal is set from one place outside the switcher's own file,
    /// and each of those lines is inside an `organizations_enabled()` check.
    #[test]
    fn both_entry_points_sit_behind_the_flag() {
        let layout = include_str!("../components/layout.rs");
        for needle in ["TenantSwitcher {}", "SHOW_CREATE_ORG.write() = true"] {
            let at = layout
                .find(needle)
                .unwrap_or_else(|| panic!("{needle} is rendered by the layout"));
            let before = &layout[..at];
            let gate = before
                .rfind("feature_flags::organizations_enabled()")
                .unwrap_or_else(|| panic!("{needle} is not behind the organizations flag"));
            // The gate has to be the nearest condition, not one further up the
            // file guarding something else: no closing brace of its own block
            // may sit between them.
            assert!(
                !layout[gate..at].contains("\n                }\n"),
                "{needle} is after an organizations gate that has already closed"
            );
            assert_eq!(
                layout.matches(needle).count(),
                1,
                "{needle} appears more than once, so one copy may be ungated"
            );
        }
    }

    /// The retired `team_enabled` flag is not coming back by accident. PMS-791
    /// made Teams core and the SPA stopped reading the key, but the container
    /// entrypoint kept writing it and the self-hosting table kept documenting
    /// it, so an operator could set it and get nothing.
    #[test]
    fn the_retired_team_flag_is_gone_from_the_operator_surface() {
        let entrypoint = include_str!("../../oci-build/entrypoint.sh");
        assert!(
            !entrypoint.contains("printf 'team_enabled"),
            "the entrypoint is writing a config key nothing reads"
        );
        let self_hosting = include_str!("../../docs/self-hosting.md");
        assert!(
            !self_hosting.contains("MOKOSH_TEAM_ENABLED"),
            "the self-hosting table documents a variable nothing reads"
        );
    }

    /// PMS-1337: the three spellings an operator is likely to write, in any
    /// case, with whitespace they did not mean to leave.
    #[test]
    fn the_spellings_that_mean_yes() {
        for raw in [
            "1", "true", "TRUE", "True", "yes", "YES", " true ", "\tyes\n",
        ] {
            assert!(is_truthy(raw), "{raw:?} should enable the feature");
        }
    }

    /// Everything else is off, and that includes the near misses. A flag
    /// guarding an unfinished feature has to fail closed: the cost of reading
    /// "flase" as off is that somebody retypes it, and the cost of reading it as
    /// on is that customers see a half-built organizations surface.
    #[test]
    fn everything_else_is_off() {
        for raw in [
            "", " ", "0", "false", "no", "off", "flase", "ture", "enabled", "y", "t", "2",
        ] {
            assert!(!is_truthy(raw), "{raw:?} must not enable the feature");
        }
    }
}
