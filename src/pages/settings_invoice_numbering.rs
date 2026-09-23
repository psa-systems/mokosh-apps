//! Settings > Billing & SLA > Invoice Numbering (MAPPS-938).
//!
//! Which shape a new invoice's number takes, the tenant setting
//! `billing_prefs/invoice_numbering` (PMS-979). Until this it was reachable
//! only through `PUT /api/v1/settings` with an admin bearer, so an MSP whose
//! customers now read invoice numbers in the portal had no way here to move
//! off numbers that say nothing about whose invoice they are.
//!
//! Admin only, like the write it drives, and the two schemes are written out
//! here because the server refuses anything outside that set; what is fetched
//! is which one is in force. A tenant with no row set is on
//! `tenant_sequence`, the server's own default, and the page says so rather
//! than showing an empty select.
//!
//! Unlike the note-editing and segment-editing pages this follows, the change
//! goes through a confirm step. Those decide who may edit something and are
//! undone by setting them back; this decides what goes on documents a customer
//! keeps, and switching back does not undo the numbers issued in between.
//! Which is also why most of this page is the consequences rather than the
//! choice: that existing invoices are never renumbered, and that each customer
//! restarts at one under a prefix they do not have yet.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, Card, ConfirmDialog, ErrorBanner, PageHeader, Select, SelectOption,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

/// One row of `GET /api/v1/settings` (`TenantSettingResponse`), narrowed to
/// what this page needs to find its own.
#[derive(Clone, Debug, Deserialize)]
struct TenantSetting {
    #[serde(default)]
    category: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: serde_json::Value,
}

const CATEGORY: &str = "billing_prefs";
const KEY: &str = "invoice_numbering";

/// The server's default when no row exists (`NumberScheme::from_setting`).
const DEFAULT_SCHEME: &str = "tenant_sequence";

/// The closed set `validate_setting_value` accepts: name, label, an example
/// of what it produces, and what choosing it means. Written out because the
/// server refuses anything else, so a value invented here would be a 422 the
/// administrator cannot act on.
const SCHEMES: &[(&str, &str, &str, &str)] = &[
    (
        DEFAULT_SCHEME,
        "One sequence for the whole business",
        "INV-000042",
        "The default. Every invoice you issue takes the next number, whoever it is for. Simple to follow internally, but a customer reading their invoice number learns how many invoices you have issued in total, and two invoices to the same customer look unrelated.",
    ),
    (
        "company_prefix",
        "A sequence per customer",
        "A7QF-000001",
        "Each customer gets a short prefix of their own and their own numbering from one. Their invoices are visibly theirs and read consecutively, and the number no longer carries your total volume. The prefix is assigned on their first invoice, is not taken from their name, and does not change if you rename them.",
    ),
];

/// The scheme in force in `settings`, or the server's default when no row is
/// set. A stored value this build does not know reads as the default too,
/// which is what the server does with one.
fn scheme_in(settings: &[TenantSetting]) -> &'static str {
    settings
        .iter()
        .find(|s| s.category == CATEGORY && s.key == KEY)
        .and_then(|s| s.value.as_str())
        .and_then(|stored| SCHEMES.iter().find(|(name, _, _, _)| *name == stored))
        .map(|(name, _, _, _)| *name)
        .unwrap_or(DEFAULT_SCHEME)
}

/// What the confirm dialog says about moving from one scheme to the other.
/// Separate from the rendering so the wording is testable, and because the two
/// directions are not the same act: one starts every customer's numbering
/// over, the other returns to a single sequence that carries on from where it
/// was left.
fn switch_warning(to: &str) -> &'static str {
    match to {
        "company_prefix" => "Invoices you have already issued keep their numbers: nothing is ever renumbered. From now on each customer starts at 000001 under a new prefix of their own, so the next invoice will not continue from the last one you sent them. Switching back later is possible, and does not undo the numbers issued in between.",
        _ => "Invoices you have already issued keep their numbers, including the per-customer ones. From now on every invoice takes the next number in your single sequence, continuing from where that sequence was left.",
    }
}

#[component]
pub fn InvoiceNumberingSettingsPage() -> Element {
    use_page_title("Invoice Numbering");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Invoice Numbering" } };
    }
    rsx! { InvoiceNumberingSettingsBody {} }
}

#[component]
fn InvoiceNumberingSettingsBody() -> Element {
    let mut settings = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_all_authed::<TenantSetting>("/settings")
            .await
            .inspect_err(|e| tracing::error!("tenant settings load failed: {e}"))
            .ok()
    });
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    // The scheme the administrator picked in the select and has not confirmed
    // yet. `None` closes the dialog, so cancelling leaves the select showing
    // what is actually in force.
    let mut pending = use_signal(|| None::<String>);
    let can_mutate = crate::hooks::use_can_mutate();
    let snap = settings.read_unchecked().clone();

    let on_confirm = move |_: ()| {
        let Some(next) = pending() else {
            return;
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = serde_json::json!({
                    "category": CATEGORY,
                    "key": KEY,
                    "value": next,
                });
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                    "/settings",
                    &body,
                )
                .await
                {
                    // Re-read rather than trusting the local value: the
                    // server is what decides which scheme is in force.
                    Ok(_) => {
                        pending.set(None);
                        settings.restart();
                    }
                    Err(e) => error.set(format!("Could not change the numbering scheme: {e}")),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = next;
                pending.set(None);
            }
            saving.set(false);
        });
    };

    rsx! {
        PageHeader {
            title: "Invoice Numbering",
            subtitle: "What a new invoice's number looks like. Invoices you have already issued keep the numbers they were issued with.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsInvoiceNumbering {} }
            },
        }
        if !error().is_empty() {
            ErrorBanner { "{error}" }
        }
        ConfirmDialog {
            open: pending().is_some(),
            title: "Change invoice numbering".to_string(),
            message: switch_warning(pending().as_deref().unwrap_or(DEFAULT_SCHEME)).to_string(),
            confirm_text: "Change numbering".to_string(),
            cancel_text: "Keep current".to_string(),
            loading: saving(),
            onconfirm: on_confirm,
            oncancel: move |_| {
                if !saving() {
                    pending.set(None);
                }
            },
        }
        match snap {
            None => rsx! { crate::components::DetailSkeleton {} },
            Some(None) => rsx! {
                Card {
                    div { class: "p-6", ErrorBanner { "Could not load the invoice numbering scheme." } }
                }
            },
            Some(Some(rows)) => {
                let current = scheme_in(&rows);
                let described = SCHEMES
                    .iter()
                    .find(|(name, _, _, _)| *name == current)
                    .map(|(_, _, example, meaning)| (*example, *meaning))
                    .unwrap_or_default();
                rsx! {
                    Card {
                        div { class: "p-6 space-y-4",
                            Select {
                                name: "invoice_numbering",
                                label: "How new invoices are numbered",
                                value: current.to_string(),
                                disabled: saving() || !can_mutate,
                                options: SCHEMES
                                    .iter()
                                    .map(|(name, label, _, _)| SelectOption::new(*name, *label))
                                    .collect::<Vec<_>>(),
                                onchange: move |e: FormEvent| {
                                    let next = e.value();
                                    // Nothing is written from the select: the
                                    // dialog is what writes, so cancelling
                                    // leaves the scheme where it was.
                                    if next != current {
                                        pending.set(Some(next));
                                    }
                                },
                            }
                            p { class: "text-sm text-muted", "For example: {described.0}" }
                            p { class: "text-sm text-muted", "{described.1}" }
                            p { class: "text-sm text-subtle",
                                "Changing this never renumbers an invoice. Each invoice keeps the number it was issued with, and records which scheme produced it, so your history stays exactly as your customers received it."
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{scheme_in, switch_warning, TenantSetting, DEFAULT_SCHEME, SCHEMES};

    fn rows(json: &str) -> Vec<TenantSetting> {
        serde_json::from_str(json).expect("deserialise settings")
    }

    /// A tenant that never opened this page is on the server's default, and
    /// the select has to show that: the server numbers invoices whether or
    /// not a row exists.
    #[test]
    fn an_unset_tenant_reads_as_the_default() {
        assert_eq!(scheme_in(&[]), DEFAULT_SCHEME);
        assert_eq!(
            scheme_in(&rows(
                r#"[{"category":"billing_prefs","key":"currency","value":"USD"}]"#
            )),
            DEFAULT_SCHEME,
            "and another key in the same category is not this one"
        );
    }

    /// The stored value is what the select shows.
    #[test]
    fn the_stored_scheme_is_the_one_shown() {
        for (name, _, _, _) in SCHEMES {
            let stored = format!(
                r#"[{{"category":"billing_prefs","key":"invoice_numbering","value":"{name}"}}]"#
            );
            assert_eq!(scheme_in(&rows(&stored)), *name);
        }
    }

    /// A value this build does not know reads as the default, the same answer
    /// the server gives one, rather than a select with nothing selected.
    #[test]
    fn an_unknown_stored_value_reads_as_the_default() {
        assert_eq!(
            scheme_in(&rows(
                r#"[{"category":"billing_prefs","key":"invoice_numbering","value":"per_year"}]"#
            )),
            DEFAULT_SCHEME
        );
        assert_eq!(
            scheme_in(&rows(
                r#"[{"category":"billing_prefs","key":"invoice_numbering","value":42}]"#
            )),
            DEFAULT_SCHEME,
            "and so does one of the wrong type"
        );
    }

    /// Both directions say what actually happens, and neither claims the
    /// change is undoable in the sense that matters: the numbers issued in
    /// between stay issued.
    #[test]
    fn each_direction_says_what_it_does() {
        let to_per_customer = switch_warning("company_prefix");
        assert!(to_per_customer.contains("keep their numbers"));
        assert!(
            to_per_customer.contains("starts at 000001"),
            "the restart is the surprise, so it is stated: {to_per_customer}"
        );
        assert!(to_per_customer.contains("does not undo"));

        let back = switch_warning(DEFAULT_SCHEME);
        assert!(back.contains("keep their numbers"));
        assert!(
            back.contains("continuing from where that sequence was left"),
            "going back is not a restart, and says so: {back}"
        );
    }

    /// Every run of whitespace as one space, so an assertion below is about
    /// what the code does rather than about where rustfmt chose to break a
    /// line. The first version of this test matched a two-line call verbatim
    /// and went red the next time the file was formatted, which is a test
    /// failing for a reason that has nothing to do with the behaviour it
    /// guards.
    fn squeezed(source: &str) -> String {
        source.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The page is admin only, writes the server's body shape, confirms
    /// before writing, and re-reads afterwards.
    #[test]
    fn the_page_confirms_before_it_writes() {
        let src = include_str!("settings_invoice_numbering.rs");
        let head = squeezed(&src[..src.find("mod tests").expect("this module")]);
        assert!(head.contains("if !crate::pages::settings::use_is_admin() {"));
        assert!(head.contains("\"category\": CATEGORY,"));
        assert!(head.contains("put_authed::<serde_json::Value, _>( \"/settings\", &body,"));
        assert!(
            head.contains("settings.restart();"),
            "a saved scheme re-reads rather than trusting the local value"
        );
        // The select opens the dialog and writes nothing itself: the whole
        // reason this page differs from the other two policy pages.
        let onchange = head
            .split("onchange: move |e: FormEvent| {")
            .nth(1)
            .expect("the select handler");
        let body = &onchange[..onchange.find("},").expect("handler end")];
        assert!(body.contains("pending.set(Some(next));"));
        assert!(
            !body.contains("put_authed"),
            "the select must not write: {body}"
        );
    }
}
