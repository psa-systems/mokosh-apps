//! Formatting an audit entry for the change-history panes (MAPPS-596).
//!
//! Four surfaces render an entity's change history: the project and task
//! panes (`pages/projects.rs`), the asset pane (`pages/assets.rs`) and the
//! ticket journal (`pages/tickets.rs`). Every one of them turns the same
//! audit row into the same words, and before this module every one of them
//! carried its own copy of these functions: `title_field`, `looks_like_uuid`
//! and `fmt_change_value` were byte-identical across assets and tickets, and
//! the projects copy differed only by a date branch the other two never got.
//!
//! They live here rather than in one page and get imported by the others,
//! because none of the four owns the concept. `single_definition` at the
//! bottom of this file fails if a copy comes back.

use chrono::{DateTime, Utc};

/// The point past which a value is cut. Long enough to recognise a sentence,
/// short enough that a full description does not land in the pane.
///
/// MAPPS-601 will replace both-sides-truncated with a word-level diff, at
/// which point this cap moves onto the diff rather than onto each side.
const VALUE_CHARS: usize = 160;

/// Humanize an audit `action` code.
///
/// Covers both code sets in use. Tickets and projects write `create` /
/// `update` / `delete`; assets write `created` / `updated` plus its own
/// `status_changed`, `credential_*` and `*_revealed` events. An unknown code
/// falls back to sentence case, which is what the assets copy did and is
/// strictly better than the projects copy's flat `"Changed"`: a new server
/// action reads as itself instead of as nothing in particular.
pub fn action_label(action: &str) -> String {
    match action {
        "create" | "created" => "Created".to_string(),
        "update" | "updated" => "Updated".to_string(),
        "delete" | "deleted" => "Deleted".to_string(),
        other => sentence_case(other),
    }
}

/// `"warranty_expiry"` to `"Warranty expiry"` for a single field name.
///
/// PMS-370: a foreign-key column is recorded under its raw name, which ends
/// in `_id` (`project_manager_id`, `asset_type_id`, `status_id`). Without
/// trimming the suffix the pane reads "Project manager id". Stripping it
/// keeps future FK fields readable with no per-column allow-list.
pub fn title_field(f: &str) -> String {
    sentence_case(f.strip_suffix("_id").unwrap_or(f))
}

/// `"description, status"` to `"Description, Status"` for a change summary.
pub fn fields_label(fields: &[String]) -> String {
    fields
        .iter()
        .map(|f| title_field(f))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `snake_case` to `Sentence case`.
fn sentence_case(s: &str) -> String {
    let mut out = s.replace('_', " ");
    if let Some(first) = out.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    out
}

/// A 36-char hyphenated UUID, not worth showing as before/after text.
pub fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.as_bytes().iter().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Render an audit value for display: `"(empty)"` for null or blank, the
/// trimmed text (truncated) for strings, a coarse marker for a reference or
/// an object.
///
/// PMS-317's date branch used to exist only in the projects copy, so an asset
/// warranty date or a ticket due date rendered as the raw `2026-03-01` the
/// audit log stores while a project's rendered as `Mar 1, 2026`. Sharing the
/// function shares the branch.
pub fn fmt_change_value(v: &Option<serde_json::Value>) -> String {
    match v {
        None | Some(serde_json::Value::Null) => "(empty)".to_string(),
        Some(serde_json::Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                "(empty)".to_string()
            } else if let Ok(d) = chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d") {
                d.format("%b %-d, %Y").to_string()
            } else if looks_like_uuid(t) {
                "(reference)".to_string()
            } else if t.chars().count() > VALUE_CHARS {
                format!("{}…", t.chars().take(VALUE_CHARS).collect::<String>())
            } else {
                t.to_string()
            }
        }
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(_) => "(updated)".to_string(),
    }
}

/// The headline for a change-history entry: what happened, and to what.
///
/// "Updated: Description". An entry with no named columns (a create, a delete,
/// or an action the server records without a field list) is the action alone.
pub fn headline(action: &str, changed_fields: &[String]) -> String {
    let label = action_label(action);
    if changed_fields.is_empty() {
        label
    } else {
        format!("{label}: {}", fields_label(changed_fields))
    }
}

/// `"Feb 28, 2025 15:04"` for a history timestamp.
pub fn fmt_history_dt(dt: DateTime<Utc>) -> String {
    dt.format("%b %-d, %Y %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn both_action_code_sets_are_covered() {
        for created in ["create", "created"] {
            assert_eq!(action_label(created), "Created");
        }
        for updated in ["update", "updated"] {
            assert_eq!(action_label(updated), "Updated");
        }
        for deleted in ["delete", "deleted"] {
            assert_eq!(action_label(deleted), "Deleted");
        }
    }

    /// The asset-specific events have no entry of their own and do not need
    /// one: sentence case is already what they should read as. Pinned so a
    /// future rewrite of the fallback does not quietly turn them into
    /// "Changed", which is what the projects copy would have done.
    #[test]
    fn an_unknown_action_reads_as_itself() {
        assert_eq!(action_label("credential_revealed"), "Credential revealed");
        assert_eq!(action_label("status_changed"), "Status changed");
        assert_eq!(action_label("some_future_action"), "Some future action");
    }

    #[test]
    fn a_foreign_key_column_loses_its_id_suffix() {
        assert_eq!(title_field("project_manager_id"), "Project manager");
        assert_eq!(title_field("due_date"), "Due date");
        assert_eq!(title_field("description"), "Description");
    }

    #[test]
    fn fields_label_titles_each_name() {
        let fields = vec!["description".to_string(), "status_id".to_string()];
        assert_eq!(fields_label(&fields), "Description, Status");
    }

    /// PMS-317's branch, which only the projects copy had. Assets and tickets
    /// inherit it by sharing the function.
    #[test]
    fn a_date_renders_the_way_the_rest_of_the_app_writes_one() {
        assert_eq!(fmt_change_value(&Some(json!("2026-03-01"))), "Mar 1, 2026");
    }

    #[test]
    fn an_absent_or_blank_value_is_empty_not_a_gap() {
        assert_eq!(fmt_change_value(&None), "(empty)");
        assert_eq!(fmt_change_value(&Some(json!(null))), "(empty)");
        assert_eq!(fmt_change_value(&Some(json!("   "))), "(empty)");
    }

    #[test]
    fn a_uuid_is_a_reference_and_an_object_is_an_update() {
        assert_eq!(
            fmt_change_value(&Some(json!("3f2504e0-4f89-41d3-9a0c-0305e82c3301"))),
            "(reference)"
        );
        assert_eq!(fmt_change_value(&Some(json!({"a": 1}))), "(updated)");
    }

    #[test]
    fn a_long_value_is_cut_and_says_so() {
        let long = "x".repeat(VALUE_CHARS + 40);
        let out = fmt_change_value(&Some(json!(long)));
        assert!(out.ends_with('…'), "{out}");
        assert_eq!(out.chars().count(), VALUE_CHARS + 1);
    }

    #[test]
    fn a_headline_names_the_action_and_the_columns() {
        let fields = vec!["description".to_string()];
        assert_eq!(headline("update", &fields), "Updated: Description");
        assert_eq!(headline("create", &[]), "Created");
    }

    /// MAPPS-596: these five lived in three pages at once. `title_field`,
    /// `looks_like_uuid` and `fmt_change_value` were byte-identical between
    /// assets and tickets, and the projects copy differed only by the date
    /// branch above, so a fix to one silently left the other two behind. That
    /// is exactly how assets and tickets came to render a raw `yyyy-mm-dd`.
    #[test]
    fn single_definition() {
        // The three pages that each carried a copy. Read as source rather
        // than imported, so this fails on a copy that is never called.
        const PAGES: [(&str, &str); 3] = [
            ("pages/projects.rs", include_str!("../../pages/projects.rs")),
            ("pages/assets.rs", include_str!("../../pages/assets.rs")),
            ("pages/tickets.rs", include_str!("../../pages/tickets.rs")),
        ];
        for needle in [
            "fn action_label(",
            "fn title_field(",
            "fn fields_label(",
            "fn fmt_change_value(",
            "fn looks_like_uuid(",
            "fn fmt_history_dt(",
        ] {
            for (path, src) in PAGES {
                assert!(
                    !src.contains(needle),
                    "{path} defines `{needle}` again; it belongs in \
                     modules/audit/format.rs alone"
                );
            }
        }
    }
}
