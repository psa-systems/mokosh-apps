//! PMS-588: user-facing role labels.
//!
//! Decision (PMS-588 Option A): the server identifier stays "admin" to
//! keep the RBAC enum and every DB row stable; the SPA labels every
//! surface that displays the role as "Owner" instead. Everywhere a
//! user sees the role goes through [`display_role_label`]; the server
//! identifier (what gets sent on the wire) is unchanged.
//!
//! The term "Owner" was chosen in the 2026-06-29 standup to match the
//! Cloudflare / Bitwarden convention where the person who signs up to
//! an account is the Owner. It also removes the ambiguity between the
//! Mokosh admin and the Bunyip admin: any surface where both appear
//! reads "Mokosh Owner" vs "Bunyip admin".
//!
//! Any new role display site must call this helper rather than
//! rendering the raw string. The exhaustive match on `UserRole` lives
//! on [`crate::pages::profile::role_display_name`] still; this helper
//! covers the string paths (invitations, members, badges) that the
//! typed path does not reach.

/// Map a server role identifier to its user-facing label.
///
/// `"admin"` renders as `"Owner"`. Every other role title-cases its
/// server string; an unknown role falls back to its own identifier so
/// the UI stays legible on a server that adds a role the SPA has not
/// learned yet.
pub fn display_role_label(role: &str) -> String {
    match role {
        "super_admin" => "Super admin".to_string(),
        "admin" => "Owner".to_string(),
        "manager" => "Manager".to_string(),
        "technician" => "Technician".to_string(),
        "dispatcher" => "Dispatcher".to_string(),
        "sales" => "Sales".to_string(),
        "finance" => "Finance".to_string(),
        "read_only" | "readonly" => "Read-only".to_string(),
        other if other.is_empty() => "Unknown".to_string(),
        other => {
            // Title-case the server string so a new role still renders
            // readably until the match arm catches up. "service_delivery"
            // -> "Service delivery".
            let mut out = String::new();
            for (i, word) in other.split('_').enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    out.extend(first.to_uppercase());
                    out.push_str(&chars.as_str().to_lowercase());
                }
            }
            if out.is_empty() {
                other.to_string()
            } else {
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::display_role_label;

    #[test]
    fn admin_renders_as_owner_per_pms_588() {
        assert_eq!(display_role_label("admin"), "Owner");
    }

    #[test]
    fn super_admin_stays_super_admin_not_super_owner() {
        // The Bunyip-platform super admin is a separate persona
        // (MAPPS-513); keeping its name distinct makes the Mokosh
        // Owner vs Bunyip admin boundary visible in the one surface
        // where both can appear.
        assert_eq!(display_role_label("super_admin"), "Super admin");
    }

    #[test]
    fn every_known_role_has_a_human_label() {
        for role in [
            "admin",
            "manager",
            "technician",
            "dispatcher",
            "sales",
            "finance",
            "read_only",
            "super_admin",
        ] {
            let label = display_role_label(role);
            assert!(!label.is_empty(), "{role} rendered empty");
            assert!(
                label.chars().next().unwrap().is_uppercase(),
                "{role} -> {label} does not start with an uppercase letter"
            );
        }
    }

    #[test]
    fn an_unknown_role_falls_back_to_a_title_cased_server_string() {
        assert_eq!(display_role_label("service_delivery"), "Service delivery");
        assert_eq!(display_role_label("ops"), "Ops");
    }

    #[test]
    fn an_empty_role_reads_as_unknown() {
        assert_eq!(display_role_label(""), "Unknown");
    }
}
