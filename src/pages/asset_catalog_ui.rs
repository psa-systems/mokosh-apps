//! Phase 3 asset-catalog SPA surfaces (MAPPS-xxxx asset catalog +
//! bulk order; docs/mokosh-assets/04-templates-and-bulk-order.md).
//!
//! Two new affordances on the Assets list:
//!
//! 1. [`SignatureBanner`] - reads `GET /asset-models/signature-detector`
//!    and renders "You have 7 Dell Latitude 5450s. Save as a template?"
//!    on top of the list when the server has >= 5 identical creates in
//!    the last 7 days + no authored catalog row yet. Dismiss hides the
//!    banner for 30 days per tenant via `platform::prefs`.
//!
//! 2. [`AssetBulkCreateModal`] - the "Bulk create" action from the
//!    list header. Picks a catalog model via typeahead, picks a
//!    Company, pastes serials one per line, submits a single
//!    `POST /assets/bulk-create` which fans the batch out server-side.
//!
//! Both components are additive and gate on the server's `assets_catalog`
//! toggle: the model-picker typeahead and the signature banner 404 when
//! the toggle is off, in which case the signature banner stays hidden
//! and the bulk modal shows an inline "feature not enabled" message.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{
    BannerTone, Button, ButtonVariant, ErrorBanner, Modal, ModalSize, Select, SelectOption,
    StatusBanner, Textarea,
};
use crate::hooks::fetch::api::ApiError;

// -- Shared DTOs mirroring the server wire shape ---------------------

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CatalogModelRow {
    pub id: Uuid,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub asset_type_id: Option<Uuid>,
    #[serde(default)]
    pub uses_count: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SignatureCandidate {
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub asset_type_id: Option<Uuid>,
    #[serde(default)]
    pub recent_count: i64,
    #[serde(default)]
    pub existing_model_id: Option<Uuid>,
    #[serde(default)]
    pub template_suggestion: SignatureTemplateSuggestion,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SignatureTemplateSuggestion {
    #[serde(default)]
    pub name_hint: String,
    #[serde(default)]
    pub common_fields: serde_json::Value,
}

// -- Signature banner ----------------------------------------------

const DISMISS_PREFS_KEY: &str = "mokosh:assets:signature_dismissed:v1";

/// Local-storage sentinel: once set, the banner hides until the stored
/// timestamp passes (30 days from the dismiss click).
fn is_dismiss_fresh() -> bool {
    let raw = crate::platform::prefs::get(DISMISS_PREFS_KEY).unwrap_or_default();
    let Ok(until) = raw.parse::<i64>() else {
        return false;
    };
    let now_s = chrono::Utc::now().timestamp();
    until > now_s
}

fn mark_dismissed_for_30_days() {
    let until = chrono::Utc::now().timestamp() + 60 * 60 * 24 * 30;
    crate::platform::prefs::set(DISMISS_PREFS_KEY, &until.to_string());
}

#[derive(Props, Clone, PartialEq)]
pub struct SignatureBannerProps {
    /// Called when the operator clicks "Save as template" after the
    /// server returns a candidate. The parent opens the save-template
    /// modal; this component does not own that flow.
    pub onsave: EventHandler<SignatureCandidate>,
}

#[component]
pub fn SignatureBanner(props: SignatureBannerProps) -> Element {
    let mut dismissed = use_signal(is_dismiss_fresh);
    let signature = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed_any::<SignatureCandidate>(
            "/asset-models/signature-detector",
        )
        .await
        .inspect_err(|e| tracing::warn!("asset signature detector load failed: {e}"))
        .ok()
    });
    if dismissed() {
        return rsx! {};
    }
    let snap = signature.read_unchecked().clone().flatten();
    let Some(cand) = snap else {
        return rsx! {};
    };
    if cand.recent_count < 1 {
        return rsx! {};
    }
    let count = cand.recent_count;
    let make = cand.manufacturer.clone();
    let model = cand.model.clone();
    let candidate = cand.clone();
    rsx! {
        StatusBanner {
            tone: BannerTone::Warning,
            class: "mb-4".to_string(),
            div { class: "flex flex-wrap items-center justify-between gap-3",
                div {
                    span { class: "font-semibold", "You have {count} {make} {model}s." }
                    " Save as a template to add more in one step."
                }
                div { class: "flex items-center gap-2",
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| props.onsave.call(candidate.clone()),
                        "Save as template"
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| {
                            mark_dismissed_for_30_days();
                            dismissed.set(true);
                        },
                        "Dismiss"
                    }
                }
            }
        }
    }
}

// -- Bulk create modal ---------------------------------------------

#[derive(Serialize)]
struct BulkCreateBody {
    asset_model_id: Uuid,
    company_id: Uuid,
    serials: Vec<BulkSerialBody>,
}

#[derive(Serialize)]
struct BulkSerialBody {
    serial_number: String,
}

#[derive(Props, Clone, PartialEq)]
pub struct AssetBulkCreateModalProps {
    pub open: bool,
    pub onclose: EventHandler<()>,
    /// Fires after a successful POST so the list refetches.
    pub oncreated: EventHandler<usize>,
    /// Pre-filled company list from the parent (so the modal does not
    /// fetch the same list the parent already holds).
    pub companies: Vec<CompanyOpt>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CompanyOpt {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
}

#[component]
pub fn AssetBulkCreateModal(props: AssetBulkCreateModalProps) -> Element {
    let mut model_query = use_signal(String::new);
    let mut selected_model: Signal<Option<CatalogModelRow>> = use_signal(|| None);
    let mut selected_company_id = use_signal(String::new);
    let mut serials_raw = use_signal(String::new);
    let mut err = use_signal(String::new);
    let mut submitting = use_signal(|| false);

    // Typeahead against /asset-models?q=. Returns an empty vec on 404
    // (feature off) so the modal fails gracefully with a message.
    let model_options = use_resource(move || {
        let q = model_query.read().trim().to_string();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            if q.is_empty() {
                return (Vec::<CatalogModelRow>::new(), false);
            }
            let path = format!(
                "/asset-models?q={}&limit=10",
                crate::utils::url::encode_uri_component(&q)
            );
            match crate::hooks::fetch::api::get_authed_any_typed::<
                crate::utils::Paginated<CatalogModelRow>,
            >(&path)
            .await
            {
                Ok(p) => (p.data, true),
                Err(ApiError::Status { code: 404, .. }) => (Vec::new(), false),
                Err(_) => (Vec::new(), true),
            }
        }
    });

    let serials_vec: Vec<String> = serials_raw
        .read()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let (serials_dedup, serials_duplicates) = dedup_preserving_order(&serials_vec);
    let serials_raw_count = serials_vec.len();
    let serials_dedup_count = serials_dedup.len();
    let serials_count_str = format!("{serials_dedup_count}");
    // The 500-cap applies to what the server will see (post-dedup), so
    // a paste of 501 lines with one duplicate is 500 unique and submits.
    let serials_over_cap = serials_dedup_count > 500;
    let dup_count = serials_duplicates.len();
    let duplicates_preview = {
        let shown: Vec<String> = serials_duplicates.iter().take(5).cloned().collect();
        let more = serials_duplicates.len().saturating_sub(5);
        if more > 0 {
            format!("{} and {more} more", shown.join(", "))
        } else {
            shown.join(", ")
        }
    };
    let model_picked = selected_model.read().is_some();
    let model_typed = !model_query.read().trim().is_empty();
    let company_picked = selected_company_id.read().parse::<Uuid>().is_ok();
    // The submit button is enabled only when every field is valid.
    // The on-submit guards below stay as a safety net for races (the
    // click lands before the latest signal read), but the operator
    // never sees a confident "Create 501 assets" that cannot fire.
    let can_submit = model_picked && company_picked && serials_dedup_count > 0 && !serials_over_cap;

    let companies = props.companies.clone();
    let company_options: Vec<SelectOption> = companies
        .iter()
        .map(|c| SelectOption::new(c.id.to_string(), c.name.clone()))
        .collect();

    let oncreated = props.oncreated;
    let onclose = props.onclose;

    let on_submit = move |_| {
        let model = selected_model.read().clone();
        let company_raw = selected_company_id.read().clone();
        let raw = serials_raw.read().clone();
        if submitting() {
            return;
        }
        let Some(model) = model else {
            err.set("Pick a model first.".to_string());
            return;
        };
        let Ok(company_id) = company_raw.parse::<Uuid>() else {
            err.set("Pick a company first.".to_string());
            return;
        };
        let raw_lines: Vec<String> = raw
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        // Preserve first-occurrence order so any future per-row overrides
        // (hostname, mac, ip, asset_tag; see MAPPS-xxxx follow-up) tied to
        // a given line survive the dedup. The server also enforces the
        // unique constraint; this is the UX layer.
        let (unique, _dupes) = dedup_preserving_order(&raw_lines);
        let serials: Vec<BulkSerialBody> = unique
            .into_iter()
            .map(|serial_number| BulkSerialBody { serial_number })
            .collect();
        if serials.is_empty() {
            err.set("Paste at least one serial.".to_string());
            return;
        }
        if serials.len() > 500 {
            err.set("Up to 500 serials per batch; split into two.".to_string());
            return;
        }
        let count = serials.len();
        let body = BulkCreateBody {
            asset_model_id: model.id,
            company_id,
            serials,
        };
        submitting.set(true);
        err.set(String::new());
        spawn(async move {
            match crate::hooks::fetch::api::post_authed_typed::<serde_json::Value, _>(
                "/assets/bulk-create",
                &body,
            )
            .await
            {
                Ok(_) => {
                    submitting.set(false);
                    oncreated.call(count);
                    onclose.call(());
                    // Reset the modal state so the next open starts clean.
                    model_query.set(String::new());
                    selected_model.set(None);
                    selected_company_id.set(String::new());
                    serials_raw.set(String::new());
                }
                Err(e) => {
                    submitting.set(false);
                    err.set(e.user_message());
                }
            }
        });
    };

    let selected_label = selected_model
        .read()
        .as_ref()
        .map(|m| format!("{} {}", m.manufacturer, m.model))
        .unwrap_or_default();
    let model_rows: Vec<CatalogModelRow> = model_options
        .read_unchecked()
        .clone()
        .map(|(rows, _)| rows)
        .unwrap_or_default();
    let feature_on = model_options
        .read_unchecked()
        .clone()
        .map(|(_, on)| on)
        .unwrap_or(true);

    rsx! {
        Modal {
            open: props.open,
            title: "Bulk create assets".to_string(),
            size: ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| onclose.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: submitting() || !can_submit,
                    onclick: on_submit,
                    if submitting() { "Creating…" } else { "Create {serials_count_str} assets" }
                }
            },
            div { class: "space-y-4",
                if !feature_on {
                    ErrorBanner {
                        "Asset catalog is not enabled on this deployment. Ask an admin to flip the `assets_catalog` toggle."
                    }
                }
                if !err.read().is_empty() {
                    ErrorBanner { "{err}" }
                }
                div {
                    label { class: "block text-xs font-medium text-muted mb-1", "Model" }
                    input {
                        r#type: "text",
                        class: "block w-full rounded-md border border-line bg-surface text-content \
                                px-2 py-1 text-sm shadow-sm focus:border-accent focus:ring-accent",
                        value: "{model_query}",
                        placeholder: "Start typing: Latitude, EliteBook, ThinkPad…",
                        oninput: move |e: FormEvent| {
                            model_query.set(e.value());
                            selected_model.set(None);
                        },
                    }
                    if !selected_label.is_empty() {
                        p { class: "mt-1 text-xs text-subtle",
                            "Picked: "
                            span { class: "font-mono text-content", "{selected_label}" }
                        }
                    } else if model_typed && model_rows.is_empty() && feature_on {
                        p { class: "mt-1 text-xs text-red-600 dark:text-red-400",
                            "Pick a result from the dropdown to continue."
                        }
                    } else if !model_rows.is_empty() {
                        ul { class: "mt-1 max-h-40 overflow-y-auto rounded-md border border-line bg-surface",
                            for m in model_rows.iter() {
                                {
                                    let row = m.clone();
                                    let display = format!("{} {}", row.manufacturer, row.model);
                                    rsx! {
                                        li { key: "{row.id}",
                                            button {
                                                r#type: "button",
                                                class: "block w-full text-left px-2 py-1 text-sm hover:bg-surface-2",
                                                onclick: move |_| {
                                                    model_query.set(display.clone());
                                                    selected_model.set(Some(row.clone()));
                                                },
                                                "{display} (used {row.uses_count}×)"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Select {
                    name: "bulk-company",
                    label: "Company".to_string(),
                    required: true,
                    options: company_options,
                    value: selected_company_id.read().clone(),
                    onchange: move |e: FormEvent| selected_company_id.set(e.value()),
                }
                div {
                    label { class: "block text-xs font-medium text-muted mb-1",
                        "Serial numbers ("
                        if serials_over_cap {
                            span { class: "font-mono text-red-600 dark:text-red-400", "{serials_count_str} of 500" }
                        } else {
                            span { class: "font-mono", "{serials_count_str} of 500" }
                        }
                        ")"
                    }
                    Textarea {
                        name: "bulk-serials",
                        value: serials_raw.read().clone(),
                        placeholder: "One serial per line".to_string(),
                        rows: 8,
                        oninput: move |e: FormEvent| serials_raw.set(e.value()),
                    }
                    if dup_count > 0 {
                        p { class: "mt-1 text-xs text-amber-700 dark:text-amber-400",
                            "{serials_raw_count} lines pasted, {dup_count} duplicate(s) dropped: "
                            span { class: "font-mono", "{duplicates_preview}" }
                            ". {serials_dedup_count} will be created."
                        }
                    }
                    if serials_over_cap {
                        p { class: "mt-1 text-xs text-red-600 dark:text-red-400",
                            "Up to 500 serials per batch; split into two."
                        }
                    } else if dup_count == 0 {
                        p { class: "mt-1 text-xs text-subtle",
                            "The server names each asset "
                            span { class: "font-mono", "{{manufacturer}} {{model}} #{{serial}}" }
                            " by default."
                        }
                    }
                }
            }
        }
    }
}

/// Return `(unique, duplicate_values)` from `input`, preserving the first
/// occurrence of each value and reporting each duplicated serial exactly
/// once. Exact-match comparison on purpose: serial numbers are case- and
/// whitespace-sensitive for some vendors; the caller trims whitespace
/// before handing them in.
fn dedup_preserving_order(input: &[String]) -> (Vec<String>, Vec<String>) {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut unique = Vec::<String>::new();
    let mut dupes = Vec::<String>::new();
    for s in input {
        if seen.insert(s.clone()) {
            unique.push(s.clone());
        } else if !dupes.contains(s) {
            dupes.push(s.clone());
        }
    }
    (unique, dupes)
}

#[cfg(test)]
mod dedup_tests {
    use super::dedup_preserving_order;

    #[test]
    fn no_duplicates_returns_input_order() {
        let (u, d) = dedup_preserving_order(&["A".into(), "B".into(), "C".into()]);
        assert_eq!(u, vec!["A", "B", "C"]);
        assert!(d.is_empty());
    }

    #[test]
    fn duplicates_dropped_once_in_preview() {
        let (u, d) =
            dedup_preserving_order(&["A".into(), "B".into(), "A".into(), "C".into(), "A".into()]);
        assert_eq!(u, vec!["A", "B", "C"]);
        assert_eq!(d, vec!["A"]);
    }

    #[test]
    fn case_sensitive_match() {
        let (u, d) = dedup_preserving_order(&["abc".into(), "ABC".into()]);
        assert_eq!(u, vec!["abc", "ABC"]);
        assert!(d.is_empty());
    }

    #[test]
    fn empty_input_returns_empty_pair() {
        let (u, d) = dedup_preserving_order(&[]);
        assert!(u.is_empty());
        assert!(d.is_empty());
    }

    #[test]
    fn each_duplicate_reported_once_even_if_repeated_more() {
        let (_u, d) = dedup_preserving_order(&["A".into(), "A".into(), "A".into(), "A".into()]);
        assert_eq!(d, vec!["A"]);
    }
}
