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

use crate::components::{use_page_title, Button, ButtonVariant, Card, ErrorBanner, PageHeader};

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
    let reload_tick = use_signal(|| 0u64);
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

    let snap = methods_resource.read_unchecked();
    let methods: Vec<RemotePaymentMethod> = match &*snap {
        Some(Ok(v)) => v.clone(),
        _ => Vec::new(),
    };
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

        if let Some(msg) = load_error {
            ErrorBanner { "{msg}" }
        }
        if !action_err.is_empty() {
            ErrorBanner { "{action_err}" }
        }

        Card {
            div { class: "flex items-center justify-between mb-4",
                h2 { class: "text-lg font-semibold text-content", "Saved cards" }
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
                                let origin = crate::platform::location::origin().unwrap_or_default();
                                // MAPPS-674: return to this same page after
                                // Stripe finishes or the customer cancels. The
                                // page's own resource re-fetches on mount, so
                                // the new card appears once the webhook has
                                // landed.
                                let base = Route::ContactPaymentMethods {}.to_string();
                                let body = StartAddPaymentMethodBody {
                                    success_url: format!("{origin}{base}?added=1"),
                                    cancel_url: format!("{origin}{base}"),
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
                                                let _ = win.location().replace(&resp.checkout_url);
                                                return;
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
            }
            if methods.is_empty() {
                p { class: "text-sm text-muted py-6",
                    "You have not saved any cards yet. Click Add a card to save one for future invoices."
                }
            } else {
                ul { class: "divide-y divide-line",
                    for method in methods.iter() {
                        {render_method_row(method.clone(), reload_tick, action_error)}
                    }
                }
            }
        }
    }
}

/// One card row. Split out so the closures do not fight for `move`
/// captures at the callsite in the `for` above.
fn render_method_row(
    method: RemotePaymentMethod,
    mut reload_tick: Signal<u64>,
    mut action_error: Signal<String>,
) -> Element {
    let id_for_default = method.id.clone();
    let id_for_remove = method.id.clone();
    let brand = humanise_brand(&method.brand);
    let last4 = method.last4.clone();
    let exp = format!("expires {:02}/{}", method.exp_month, method.exp_year);
    let is_default = method.is_default;

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
                        onclick: move |_| {
                            let id = id_for_default.clone();
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
                            });
                        },
                        "Set default"
                    }
                }
                Button {
                    variant: ButtonVariant::Danger,
                    r#type: "button".to_string(),
                    onclick: move |_| {
                        let id = id_for_remove.clone();
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
                        });
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
