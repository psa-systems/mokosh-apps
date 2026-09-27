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
