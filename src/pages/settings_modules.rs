//! Settings > Organization & Data > Modules (MAPPS-747).
//!
//! The ten route groups the server gates per tenant, each with a switch.
//! Until this the flags were reachable only through
//! `PUT /api/v1/settings/modules/{module}` with an admin bearer, which is
//! how MAPPS-746 happened: the tenant's `timesheets` module was off (a
//! `personal` tenant starts with it off, PMS-943), the clock-in strip hid
//! itself, and an admin who worked that out had nowhere in this app to act.
//!
//! Admin only, like the write it drives. A row says what turning the module
//! off does before the switch is flipped, and a module with no row reads
//! as off, the server's own rule. A successful toggle re-reads the list
//! and refreshes the shared flags the nav follows (MAPPS-748), so the
//! Timesheets entries and the clock-in strip react without a reload.

use dioxus::prelude::*;

use crate::components::{use_page_title, Card, Checkbox, ErrorBanner, PageHeader};
use crate::hooks::modules::{enabled_in, refresh_module_flags, ModuleConfig, GATED_MODULES};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

#[component]
pub fn ModulesSettingsPage() -> Element {
    use_page_title("Modules");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Modules" } };
    }
    rsx! { ModulesSettingsBody {} }
}

#[component]
fn ModulesSettingsBody() -> Element {
    let mut rows = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_all_authed::<ModuleConfig>("/settings/modules")
            .await
            .inspect_err(|e| tracing::error!("module config load failed: {e}"))
            .ok()
    });
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| None::<&'static str>);
    let can_mutate = crate::hooks::use_can_mutate();
    let snap = rows.read_unchecked().clone();

    rsx! {
        PageHeader {
            title: "Modules",
            subtitle: "Which parts of the platform this organization uses. A module that is off answers not found everywhere it is asked for.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsModules {} }
            },
        }
        if !error().is_empty() {
            ErrorBanner { "{error}" }
        }
        Card {
            match snap {
                None => rsx! { p { class: "p-6 text-sm text-subtle", "Loading…" } },
                Some(None) => rsx! {
                    p { class: "p-6 text-sm text-red-600 dark:text-red-300", "Could not load the module settings." }
                },
                Some(Some(configs)) => rsx! {
                    ul { class: "divide-y divide-line",
                        for module in GATED_MODULES.iter() {
                            {
                                let name = module.name;
                                let on = enabled_in(&configs, name);
                                let saving = busy() == Some(name);
                                rsx! {
                                    li { key: "{name}", class: "flex items-start justify-between gap-6 px-6 py-4",
                                        div { class: "min-w-0",
                                            p { class: "text-sm font-medium text-content", "{module.label}" }
                                            p { class: "mt-1 text-sm text-muted", "Off: {module.off_means}" }
                                        }
                                        Checkbox {
                                            name: "module_{name}",
                                            label: if on { "On".to_string() } else { "Off".to_string() },
                                            checked: on,
                                            disabled: saving || !can_mutate,
                                            onchange: move |e: FormEvent| {
                                                let next = e.checked();
                                                busy.set(Some(name));
                                                error.set(String::new());
                                                spawn(async move {
                                                    #[cfg(feature = "app")]
                                                    {
                                                        let path = format!("/settings/modules/{name}");
                                                        let body = serde_json::json!({ "is_enabled": next, "config": {} });
                                                        match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body).await {
                                                            Ok(_) => {
                                                                rows.restart();
                                                                refresh_module_flags();
                                                            }
                                                            Err(e) => error.set(format!("Could not change {}: {e}", module.label)),
                                                        }
                                                    }
                                                    #[cfg(not(feature = "app"))]
                                                    let _ = next;
                                                    busy.set(None);
                                                });
                                            },
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

#[cfg(test)]
mod tests {
    /// Admin only, every gated module listed, the write goes to the
    /// server's path, and a toggle refreshes both the list and the shared
    /// flags the nav follows.
    #[test]
    fn the_page_is_admin_only_and_a_toggle_refreshes_the_flags() {
        let src = include_str!("settings_modules.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("if !crate::pages::settings::use_is_admin() {"));
        assert!(head.contains("for module in GATED_MODULES.iter() {"));
        assert!(head.contains("let path = format!(\"/settings/modules/{name}\");"));
        assert!(head.contains("rows.restart();\n                                                                refresh_module_flags();"));
        assert!(
            head.contains("\"is_enabled\": next, \"config\": {}"),
            "the server's body shape"
        );
    }
}
