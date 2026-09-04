//! Reusable product picker (MAPPS-640).
//!
//! Mirror of [`crate::components::AssetPicker`]: hits
//! `GET /products?is_active=true&q=...&per_page=20` on each keystroke, renders
//! the matches in a click-to-select dropdown, and reports the picked product
//! back through [`PickedProduct`]. Used on the invoice create form, the invoice
//! edit modal (as "Add from catalog") and the contract item form.
//!
//! Only ACTIVE products are offered, because the server refuses a retired one
//! on a new document (`assert_product_sellable`) and a picker that offered it
//! would be offering a 400. Only this tenant's, because the server scopes the
//! list; the picker never composes an id itself.
//!
//! What a pick hands back is the product's price AT THE MOMENT OF PICKING. The
//! caller writes it onto the line, and that copy is what the document keeps:
//! a price is never read through the product reference again (PMS-955), so
//! editing the catalog later cannot re-price anything already sold.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{Button, ButtonSize, ButtonVariant, ErrorBanner, Input};
use crate::hooks::use_dropdown_nav;
use crate::utils::url::urlencoding_minimal;

#[derive(Clone, Debug, Deserialize)]
struct PickerProduct {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    sku: Option<String>,
    #[serde(default)]
    description: Option<String>,
    /// The server's decimal string.
    #[serde(default)]
    unit_price: String,
    #[serde(default)]
    unit: String,
}

#[derive(Clone, Debug, Deserialize)]
struct PickerPage {
    data: Vec<PickerProduct>,
}

/// What the caller receives on a pick: enough to fill a line without a second
/// request, and the price as it stood when picked.
#[derive(Clone, Debug, PartialEq)]
pub struct PickedProduct {
    pub id: String,
    pub name: String,
    pub sku: Option<String>,
    pub description: Option<String>,
    /// Decimal string, as the server sends it and as a price field holds it.
    pub unit_price: String,
    pub unit: String,
}

impl PickedProduct {
    fn from_row(row: &PickerProduct) -> Self {
        Self {
            id: row.id.to_string(),
            name: row.name.clone(),
            sku: row.sku.clone().filter(|s| !s.trim().is_empty()),
            description: row.description.clone().filter(|s| !s.trim().is_empty()),
            unit_price: row.unit_price.clone(),
            unit: row.unit.clone(),
        }
    }
}

/// The text a result row shows beside the name: the SKU when there is one,
/// then the price per unit. Pure so the tests can pin the shape.
pub fn product_row_detail(sku: Option<&str>, unit_price: &str, unit: &str) -> String {
    let price = crate::utils::money::format_money_str(unit_price);
    let per_unit = if unit.trim().is_empty() {
        price
    } else {
        format!("{price} / {}", unit.trim())
    };
    match sku.map(str::trim).filter(|s| !s.is_empty()) {
        Some(sku) => format!("{sku} · {per_unit}"),
        None => per_unit,
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ProductPickerProps {
    /// Currently selected product name (or empty if none selected).
    pub value: String,
    /// When `Some`, the picker renders as a selected chip with Change and
    /// Clear rather than the search input.
    pub selected_id: Option<String>,
    #[props(default = String::from("Product"))]
    pub label: String,
    #[props(default)]
    pub required: bool,
    /// Placeholder for the search input.
    #[props(default = String::from("Search the price list…"))]
    pub placeholder: String,
    /// Reset the search box after a pick rather than showing the picked name,
    /// for a caller that uses the picker as an "add one more" control and
    /// never shows a selection.
    #[props(default)]
    pub clear_on_select: bool,
    /// Fires once when the user picks a row.
    pub onselect: EventHandler<PickedProduct>,
    /// Fires when the user clears the selection from the chip.
    pub onclear: EventHandler<()>,
}

#[component]
pub fn ProductPicker(props: ProductPickerProps) -> Element {
    let mut query = use_signal(String::new);
    // MAPPS-503: open / highlight state and the shared keyboard contract.
    // MAPPS-653: the field has to end up holding a product, so Enter takes the
    // first match when the user has not arrowed onto one.
    let mut nav = use_dropdown_nav("product-picker").enter_takes_first_match();
    let mut editing = use_signal(|| false);
    // PMS-371: read the query INSIDE the resource closure so the fetch
    // subscribes to it.
    let query_text = query.read().trim().to_string();
    let results = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let q = query.read().trim().to_string();
        let path = if q.is_empty() {
            "/products?is_active=true&per_page=20".to_string()
        } else {
            format!(
                "/products?is_active=true&q={}&per_page=20",
                urlencoding_minimal(&q)
            )
        };
        // MAPPS-503: keep the failure, so a failed search is its own state.
        crate::hooks::fetch::api::get_authed::<PickerPage>(&path)
            .await
            .map(|p| p.data)
            .inspect_err(|e| tracing::warn!("product search failed: {e}"))
    });

    let clear_on_select = props.clear_on_select;

    if let Some(_id) = &props.selected_id {
        if !editing() {
            let name = props.value.clone();
            let onclear = props.onclear;
            let show_label = !props.label.trim().is_empty();
            return rsx! {
                div { class: "space-y-1 w-full",
                    if show_label {
                        label { class: "block text-sm font-medium text-content",
                            "{props.label}"
                            if props.required {
                                span { class: "text-red-500 dark:text-red-400 ml-0.5", "*" }
                            }
                        }
                    }
                    div {
                        class: "flex items-center justify-between border border-line rounded-md px-3 py-2 bg-app w-full min-w-0",
                        div { class: "min-w-0 flex-1 text-left",
                            p { class: "text-sm font-medium text-content truncate", "{name}" }
                        }
                        div { class: "flex items-center gap-1 shrink-0 ml-2",
                            Button {
                                variant: ButtonVariant::Link,
                                size: ButtonSize::Small,
                                onclick: move |_| {
                                    query.set(String::new());
                                    editing.set(true);
                                    nav.open();
                                },
                                "Change"
                            }
                            button {
                                r#type: "button",
                                class: "text-xs text-muted hover:text-red-600 dark:hover:text-red-400 px-2 py-1",
                                onclick: move |_| {
                                    onclear.call(());
                                    query.set(String::new());
                                },
                                "Clear"
                            }
                        }
                    }
                }
            };
        }
    }

    let snap = results.read_unchecked();
    let onselect = props.onselect;
    let rows: Vec<PickerProduct> = match &*snap {
        Some(Ok(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    let nav_len = rows.len();
    let rows_for_keys = rows.clone();
    let mut commit = move |row: &PickerProduct| {
        let picked = PickedProduct::from_row(row);
        let name = picked.name.clone();
        onselect.call(picked);
        editing.set(false);
        query.set(if clear_on_select { String::new() } else { name });
    };

    rsx! {
        div { class: "relative space-y-1",
            div {
                role: "combobox",
                aria_expanded: nav.expanded(),
                aria_controls: nav.panel_id(),
                aria_activedescendant: nav.active_descendant(),
                onfocusin: move |_| nav.open(),
                onclick: move |_| nav.open(),
                onkeydown: move |e: KeyboardEvent| {
                    let rows = rows_for_keys.clone();
                    nav.keydown(&e, nav_len, move |index| {
                        if let Some(row) = rows.get(index) {
                            commit(row);
                        }
                    });
                },
                Input {
                    name: "product_search",
                    label: props.label,
                    placeholder: props.placeholder,
                    required: props.required,
                    value: query.read().clone(),
                    oninput: move |e: FormEvent| {
                        query.set(e.value());
                        nav.open_fresh();
                    },
                }
            }
            if nav.is_open() {
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| {
                        nav.close();
                        editing.set(false);
                    },
                }
                div {
                    id: nav.panel_id(),
                    role: "listbox",
                    class: "dropdown-panel absolute z-20 left-0 right-0 mt-1 max-h-72 overflow-y-auto",
                    match &*snap {
                        None => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted", "Searching…" }
                        },
                        Some(Err(_)) => rsx! {
                            ErrorBanner { class: "m-1", "Could not search. Try again." }
                        },
                        Some(Ok(_)) if rows.is_empty() => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted",
                                if query_text.is_empty() {
                                    "No active products in the price list yet."
                                } else {
                                    "No matches."
                                }
                            }
                        },
                        Some(Ok(_)) => rsx! {
                            ul { class: "py-1", role: "none",
                                for (index , row) in rows.iter().enumerate() {
                                    {
                                        let key = row.id.to_string();
                                        let name = row.name.clone();
                                        let detail = product_row_detail(row.sku.as_deref(), &row.unit_price, &row.unit);
                                        let row_for_click = row.clone();
                                        rsx! {
                                            li {
                                                key: "{key}",
                                                id: nav.row_id(index),
                                                role: "option",
                                                aria_selected: nav.row_selected(index),
                                                button {
                                                    r#type: "button",
                                                    tabindex: "-1",
                                                    class: nav.row_class(index, "w-full text-left px-3 py-2 text-sm hover:bg-surface-2 flex items-baseline justify-between gap-3"),
                                                    onclick: move |_| {
                                                        commit(&row_for_click);
                                                        nav.close();
                                                    },
                                                    span { class: "font-medium truncate", "{name}" }
                                                    span { class: "text-xs text-muted shrink-0", "{detail}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::product_row_detail;

    #[test]
    fn a_row_shows_sku_then_price_per_unit() {
        assert_eq!(
            product_row_detail(Some("WS-01"), "49.00", "month"),
            "WS-01 · $49.00 / month"
        );
        assert_eq!(product_row_detail(None, "1250", "each"), "$1,250.00 / each");
        assert_eq!(product_row_detail(Some("  "), "5", ""), "$5.00");
    }
}
