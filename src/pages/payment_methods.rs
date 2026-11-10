//! MAPPS-674: portal-contact saved payment methods.
//!
//! A contact holding `payment_methods:manage_own` (built-in Billing
//! Contact) reaches `/settings/payment-methods`, sees every card they have
//! saved, and can:
//!
//! - Add a card. The button POSTs to `/contact/payment-methods` and
//!   redirects the browser to the Stripe Checkout URL the server mints.
//!   Stripe collects the card data on their hosted page; the contact
//!   never types anything into mokosh.
//! - Set a card as default. `PUT /contact/payment-methods/{id}/default`,
//!   204 on success. The server flips the picked row and clears every
//!   other row of this contact in one transaction.
//! - Remove a card. `DELETE /contact/payment-methods/{id}`, 204 on
//!   success. The server detaches on Stripe FIRST, then deletes the row,
//!   so a failed detach keeps the row and the contact can retry.
//!
//! A contact without the cap sees the portal's explained state with Ask for
//! access (MAPPS-780), never the capability's internal name. Nothing links to
//! this page yet: there is no sidebar entry, so it is reached by URL, and the
//! staff-copy note that used to sit here described an entry that does not
//! exist.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Button, ButtonVariant, Card, ConfirmDialog, ErrorBanner, PageHeader,
};

/// One row of `GET /contact/payment-methods`. Serde defaults everywhere
/// so a server that predates this ticket decodes to a blank row and the
/// list simply renders nothing, matching the pattern the pay-invoice
/// readiness response already uses.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemotePaymentMethod {
    #[serde(default)]
    id: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    brand: String,
    #[serde(default)]
    last4: String,
    #[serde(default)]
    exp_month: u8,
    #[serde(default)]
    exp_year: u16,
    #[serde(default)]
    is_default: bool,
}

/// Request body for `POST /contact/payment-methods`. The server takes the
/// return URLs from the SPA so a route rename moves the redirects with it.
#[derive(Clone, Debug, PartialEq, Serialize)]
struct StartAddPaymentMethodBody {
    success_url: String,
    cancel_url: String,
}

/// Response from `POST /contact/payment-methods`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct StartAddPaymentMethodResponse {
    #[serde(default)]
    checkout_url: String,
}

/// MAPPS-780: the gate, ahead of anything that fetches.
///
/// The page told a customer without the capability to "Ask your MSP to grant
/// you the payment_methods:manage_own capability" - an internal identifier
/// they cannot act on, and no way to ask. It now shows the portal's own
/// explained state with Ask for access.
///
/// A wrapper rather than an early return inside the body, because the body's
/// MAPPS-602 shape calls every hook before its return, including the list
/// fetch: a contact who could not manage payment methods sent a request that
/// was refused on every visit. With the gate out here the body, and its fetch,
/// only mounts for a contact who can use it.
#[component]
pub fn ContactPaymentMethodsPage() -> Element {
    use_page_title("Payment Methods");
    let can_manage = crate::hooks::capabilities::use_capability("payment_methods:manage_own");
    if !can_manage {
        return rsx! {
            crate::components::PortalAccessRequired {
                title: "Payment Methods".to_string(),
                area: crate::components::PAYMENT_METHODS,
            }
        };
    }
    rsx! { ContactPaymentMethodsBody {} }
}

#[component]
fn ContactPaymentMethodsBody() -> Element {
    // MAPPS-602: every hook fires BEFORE any early return.
    let mut reload_tick = use_signal(|| 0u64);
    let methods_resource = use_resource(move || {
        let _ = reload_tick.read();
        async {
            #[cfg(feature = "app")]
            {
                crate::hooks::fetch::api::get_contact_authed::<Vec<RemotePaymentMethod>>(
                    "/contact/payment-methods",
                )
                .await
            }
            #[cfg(not(feature = "app"))]
            {
                Err::<Vec<RemotePaymentMethod>, crate::hooks::fetch::api::ApiError>(
                    crate::hooks::fetch::api::ApiError::Network("non-app build".into()),
                )
            }
        }
    });
    let mut action_error = use_signal(String::new);
    let mut add_saving = use_signal(|| false);
    // MAPPS-788: `pending_remove` carries the (id, label) of the row whose
    // Remove button was clicked; the dialog renders with the same brand +
    // last4 label the row itself shows. `removing` gates the dialog's
    // loading state so a click during the in-flight DELETE does not
    // double-fire.
    let mut pending_remove = use_signal::<Option<(String, String)>>(|| None);
    let mut removing = use_signal(|| false);
    // MAPPS-1231: the card whose Set default is in flight, so a repeat click
    // is ignored like Add and Remove ignore theirs.
    let default_saving = use_signal(|| None::<String>);

    let snap = methods_resource.read_unchecked();
    let load_error = match &*snap {
        Some(Err(e)) => Some(e.user_message()),
        _ => None,
    };

    let action_err = action_error.read().clone();

    rsx! {
        PageHeader {
            title: "Payment Methods",
            subtitle: "Save a card for future invoices and pick which one is your default. Card data is typed into Stripe's own page and never touches your MSP.",
        }

        // MAPPS-966: the banners and the card are gapped, not stacked.
        div { class: "space-y-6",
            if let Some(msg) = load_error {
                ErrorBanner { "{msg}" }
            }
            if !action_err.is_empty() {
                ErrorBanner { "{action_err}" }
            }

            Card {
                title: "Saved cards".to_string(),
                // MAPPS-967: the heading-row control moves to `actions`, which is
                // what keeps it on the header's baseline now that the heading is
                // the card's own rather than a flex row in the body.
                actions: rsx! {
                        Button {
                            variant: ButtonVariant::Primary,
                            disabled: *add_saving.read(),
                            loading: *add_saving.read(),
                            r#type: "button".to_string(),
                            onclick: move |_| {
                                if *add_saving.read() {
                                    return;
                                }
                                add_saving.set(true);
                                action_error.set(String::new());
                                spawn(async move {
                                    #[cfg(feature = "app")]
                                    {
                                        // MAPPS-674: return to this same page after
                                        // Stripe finishes or the customer cancels. The
                                        // page's own resource re-fetches on mount, so
                                        // the new card appears once the webhook has
                                        // landed.
                                        let base = Route::ContactPaymentMethods {}.to_string();
                                        // MAPPS-1231: without an origin the URLs were
                                        // relative, which Stripe refuses.
                                        let (success_url, cancel_url) = match return_urls(
                                            crate::platform::location::origin(),
                                            &base,
                                        ) {
                                            Ok(urls) => urls,
                                            Err(e) => {
                                                tracing::error!("add card: {e}");
                                                action_error.set(e);
                                                add_saving.set(false);
                                                return;
                                            }
                                        };
                                        let body = StartAddPaymentMethodBody {
                                            success_url,
                                            cancel_url,
                                        };
                                        match crate::hooks::fetch::api::post_contact_authed_typed::<
                                            StartAddPaymentMethodResponse,
                                            _,
                                        >("/contact/payment-methods", &body)
                                        .await
                                        {
                                            Ok(resp) if !resp.checkout_url.is_empty() => {
                                                #[cfg(target_arch = "wasm32")]
                                                {
                                                    if let Some(win) = web_sys::window() {
                                                        // MAPPS-1231: a refused redirect used to
                                                        // return silently; say it instead.
                                                        match win.location().replace(&resp.checkout_url) {
                                                            Ok(()) => return,
                                                            Err(e) => {
                                                                tracing::error!("add card: redirect to the payment provider failed: {e:?}");
                                                                action_error.set(
                                                                    "Could not open the payment provider's page. Try again.".to_string(),
                                                                );
                                                                add_saving.set(false);
                                                                return;
                                                            }
                                                        }
                                                    }
                                                }
                                                #[cfg(not(target_arch = "wasm32"))]
                                                {
                                                    let _ = &resp;
                                                }
                                                action_error.set(
                                                    "Adding a card is only available in the web portal. Open your portal in a browser to add a card.".to_string(),
                                                );
                                            }
                                            Ok(_) => {
                                                action_error.set(
                                                    "The payment provider returned an empty response. Try again.".to_string(),
                                                );
                                            }
                                            Err(err) => {
                                                action_error.set(format!(
                                                    "Could not start adding a card: {}",
                                                    err.user_message()
                                                ));
                                            }
                                        }
                                    }
                                    add_saving.set(false);
                                });
                            },
                            "Add a card"
                        }
                },

                match &*snap {
                    None => rsx! {
                        div { class: "space-y-4 py-2",
                            div { class: "h-4 w-full bg-surface-2 rounded animate-pulse" }
                            div { class: "h-4 w-full bg-surface-2 rounded animate-pulse" }
                            div { class: "h-4 w-2/3 bg-surface-2 rounded animate-pulse" }
                        }
                    },
                    Some(Ok(v)) if v.is_empty() => rsx! {
                        p { class: "text-sm text-muted py-6",
                            "You have not saved any cards yet. Click Add a card to save one for future invoices."
                        }
                    },
                    Some(Ok(v)) => rsx! {
                        ul { class: "divide-y divide-line",
                            for method in v.iter() {
                                {render_method_row(method.clone(), reload_tick, action_error, default_saving, pending_remove)}
                            }
                        }
                    },
                    Some(Err(_)) => rsx! {},
                }
            }
        }

        {
            let pending = pending_remove.read().clone();
            let open = pending.is_some();
            let label = pending.as_ref().map(|(_, l)| l.clone()).unwrap_or_default();
            let message = format!("Remove {label}?");
            let on_confirm = move |_: ()| {
                let Some((id, _)) = pending_remove.read().clone() else {
                    return;
                };
                if removing() {
                    return;
                }
                removing.set(true);
                action_error.set(String::new());
                spawn(async move {
                    #[cfg(feature = "app")]
                    {
                        match crate::hooks::fetch::api::delete_contact_authed_no_content(&format!(
                            "/contact/payment-methods/{id}"
                        ))
                        .await
                        {
                            Ok(()) => {
                                reload_tick.with_mut(|t| *t += 1);
                            }
                            Err(err) => {
                                action_error.set(format!(
                                    "Could not remove card: {}",
                                    err.user_message()
                                ));
                            }
                        }
                    }
                    #[cfg(not(feature = "app"))]
                    {
                        let _ = id;
                    }
                    removing.set(false);
                    pending_remove.set(None);
                });
            };
            rsx! {
                ConfirmDialog {
                    open,
                    title: "Remove card".to_string(),
                    message,
                    confirm_text: "Remove".to_string(),
                    cancel_text: "Cancel".to_string(),
                    destructive: true,
                    loading: removing(),
                    onconfirm: on_confirm,
                    oncancel: move |_| {
                        if !removing() {
                            pending_remove.set(None);
                        }
                    },
                }
            }
        }
    }
}

/// MAPPS-1231: Stripe's return URLs, absolute, or why they cannot be built.
fn return_urls(origin: Option<String>, base: &str) -> Result<(String, String), String> {
    match origin.map(|o| o.trim().trim_end_matches('/').to_string()).filter(|o| !o.is_empty()) {
        Some(origin) => Ok((format!("{origin}{base}?added=1"), format!("{origin}{base}"))),
        None => Err("Could not work out this page's address to return to after adding a card. Reload the page and try again.".to_string()),
    }
}

/// One card row. Split out so the closures do not fight for `move`
/// captures at the callsite in the `for` above.
fn render_method_row(
    method: RemotePaymentMethod,
    mut reload_tick: Signal<u64>,
    mut action_error: Signal<String>,
    mut default_saving: Signal<Option<String>>,
    mut pending_remove: Signal<Option<(String, String)>>,
) -> Element {
    let id_for_default = method.id.clone();
    let id_for_remove = method.id.clone();
    let brand = humanise_brand(&method.brand);
    let last4 = method.last4.clone();
    let exp = format!("expires {:02}/{}", method.exp_month, method.exp_year);
    let is_default = method.is_default;
    let remove_label = format!("{brand} \u{2022}\u{2022}\u{2022}\u{2022} {last4}");

    rsx! {
        li { class: "flex items-center justify-between py-4",
            div {
                div { class: "font-medium text-content", "{brand} \u{2022}\u{2022}\u{2022}\u{2022} {last4}" }
                div { class: "text-sm text-muted", "{exp}" }
                if is_default {
                    div { class: "text-xs text-accent mt-1", "Default" }
                }
            }
            div { class: "flex items-center gap-2",
                if !is_default {
                    Button {
                        variant: ButtonVariant::Secondary,
                        r#type: "button".to_string(),
                        disabled: default_saving.read().is_some(),
                        loading: default_saving.read().as_deref() == Some(id_for_default.as_str()),
                        onclick: move |_| {
                            if default_saving.read().is_some() {
                                return;
                            }
                            let id = id_for_default.clone();
                            default_saving.set(Some(id.clone()));
                            action_error.set(String::new());
                            spawn(async move {
                                #[cfg(feature = "app")]
                                {
                                    match crate::hooks::fetch::api::put_contact_authed_no_content(&format!(
                                        "/contact/payment-methods/{id}/default"
                                    ))
                                    .await
                                    {
                                        Ok(()) => {
                                            reload_tick.with_mut(|t| *t += 1);
                                        }
                                        Err(err) => {
                                            action_error.set(format!(
                                                "Could not set default card: {}",
                                                err.user_message()
                                            ));
                                        }
                                    }
                                }
                                default_saving.set(None);
                            });
                        },
                        "Set default"
                    }
                }
                Button {
                    variant: ButtonVariant::Danger,
                    r#type: "button".to_string(),
                    onclick: move |_| {
                        pending_remove.set(Some((id_for_remove.clone(), remove_label.clone())));
                    },
                    "Remove"
                }
            }
        }
    }
}

fn humanise_brand(raw: &str) -> String {
    match raw {
        "visa" => "Visa".to_string(),
        "mastercard" => "Mastercard".to_string(),
        "amex" | "american_express" => "American Express".to_string(),
        "discover" => "Discover".to_string(),
        "diners" => "Diners".to_string(),
        "jcb" => "JCB".to_string(),
        "unionpay" => "UnionPay".to_string(),
        "" => "Card".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => "Card".to_string(),
            }
        }
    }
}

use crate::Route;

/// MAPPS-1231: the payment-methods findings from the specification pass.
#[cfg(test)]
mod mapps1231_payment_method_tests {
    use super::return_urls;

    fn head() -> &'static str {
        let src = include_str!("payment_methods.rs");
        &src[..src
            .find("mod mapps1231_payment_method_tests")
            .expect("this module")]
    }

    /// No origin sent relative URLs, which Stripe refuses.
    #[test]
    fn return_urls_are_absolute_or_refused() {
        assert!(return_urls(None, "/portal/payment-methods").is_err());
        assert!(return_urls(Some("  ".into()), "/portal/payment-methods").is_err());
        assert_eq!(
            return_urls(
                Some("https://msp.example/".into()),
                "/portal/payment-methods"
            ),
            Ok((
                "https://msp.example/portal/payment-methods?added=1".to_string(),
                "https://msp.example/portal/payment-methods".to_string()
            ))
        );
    }

    /// A refused redirect returned silently.
    #[test]
    fn a_failed_redirect_is_said() {
        let head = head();
        assert!(!head.contains("let _ = win.location().replace(&resp.checkout_url);"));
        assert!(head.contains("Could not open the payment provider's page. Try again."));
    }

    /// Repeated Set default clicks sent repeated requests.
    #[test]
    fn set_default_ignores_a_repeat() {
        let head = head();
        let compact: String = head.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(compact.contains("if default_saving.read().is_some() { return; }"));
        assert!(compact.contains("default_saving.set(Some(id.clone()));"));
        assert!(compact.contains("default_saving.set(None); });"));
    }
}
