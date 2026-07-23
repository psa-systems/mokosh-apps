//! MAPPS-271: shared "permission required" empty states.
//!
//! Lifts the friendly locked-state UI that `pages::billing` already
//! used for Invoices and Payments into a shared component, so every
//! permission-gated list view (Contracts, Rate Cards, future
//! billing-adjacent surfaces) surfaces the same posture instead of the
//! misleading "Could not load. Refresh the page to retry." sentence the
//! generic fetch-failed branch shows on a 403.
//!
//! The component reads the signed-in user's role from the auth context
//! so the "your current role: X" line shows without per-call-site
//! plumbing. Callers pass:
//!   - `title`: the surface being locked ("Contracts", "Rate Cards", ...)
//!     used both as the page title and the heading.
//!   - `body`: a one-line explanation of who actually has access.
//!     Each surface picks its own wording so a future tenant-role
//!     change doesn't have to edit every page.

use dioxus::prelude::*;

use crate::components::{use_page_title, Card, IconSize, InformationIcon, PageHeader};

#[derive(Props, Clone, PartialEq)]
pub struct PermissionRequiredProps {
    pub title: String,
    pub body: String,
}

#[component]
pub fn PermissionRequired(props: PermissionRequiredProps) -> Element {
    let auth = crate::hooks::use_auth();
    let role = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.as_str().to_string())
        .unwrap_or_default();
    let heading = format!("{} access required", props.title);
    // MAPPS-366: rendered as a page body inside the persistent AppShell, so it
    // sets its own tab title rather than passing it to an AppLayout wrapper.
    use_page_title(props.title.clone());
    rsx! {
        PageHeader { title: "{props.title}" }
        Card {
            div { class: "py-12 px-6 mx-auto flex max-w-md flex-col items-center text-center",
                div { class: "mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-surface-2",
                    InformationIcon { size: IconSize::Large, class: "text-subtle".to_string() }
                }
                h3 { class: "text-base font-medium text-content",
                    "{heading}"
                }
                p { class: "mt-2 text-sm text-muted",
                    "{props.body}"
                }
                if !role.is_empty() {
                    p { class: "mt-4 text-xs text-subtle",
                        "Your current role: {role}"
                    }
                }
            }
        }
    }
}
