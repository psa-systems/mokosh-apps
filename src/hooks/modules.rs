//! Which gated modules are on for the active tenant (MAPPS-748).
//!
//! The server gates ten route groups per tenant through `module_config`
//! (`RequireModuleEnabled`, PMS-116 / PMS-943): a module that is off
//! answers 404 exactly like a route that does not exist. The nav and the
//! pages need the same answer so a switched-off feature is not listed, and
//! they need it once per tenant rather than once per nav item, so the flags
//! live in one `GlobalSignal` read from `GET /settings/modules` (any staff
//! role may read it; only an admin may write) and refreshed on an org
//! switch and after a toggle on the Modules page (MAPPS-747).
//!
//! A module with no row reads as off, the server's own rule
//! (`SettingsService::is_module_enabled` COALESCEs a missing row to false).
//! Until the first read lands a module reads as off too, so a dead link is
//! never shown for the time it takes the answer to arrive. A contact
//! session has no modules and reads every one as off.

use dioxus::prelude::*;
use serde::Deserialize;

/// The server's `gated_module!` table, copied: name, label, and what
/// turning it off does, in the words the Modules page shows. Kept in step
/// by hand; `GATED_MODULES_ARE_THE_SERVERS` pins the names.
pub const GATED_MODULES: &[GatedModule] = &[
    GatedModule { name: "billing", label: "Billing", off_means: "Invoices, payments, credit notes, statements and products answer not found." },
    GatedModule { name: "projects", label: "Projects", off_means: "Projects and tasks answer not found." },
    GatedModule { name: "calendar", label: "Calendar", off_means: "Appointments, scheduling and dispatch answer not found." },
    GatedModule { name: "contracts", label: "Contracts", off_means: "Contracts and block hours answer not found; time is billed hourly." },
    GatedModule { name: "assets", label: "Assets", off_means: "Assets and their RMM mappings answer not found." },
    GatedModule { name: "knowledge_base", label: "Knowledge base", off_means: "Articles, categories and their comments answer not found." },
    GatedModule { name: "rmm_integration", label: "RMM integration", off_means: "RMM connections, device mappings and alert rules answer not found." },
    GatedModule { name: "reports", label: "Reports", off_means: "Reports and saved reports answer not found." },
    GatedModule { name: "time_tracking", label: "Time tracking", off_means: "Time entries, timers and mileage answer not found; timesheets and the work-day clock go with them." },
    GatedModule { name: "timesheets", label: "Timesheets", off_means: "Submitting a week for approval, the work-day clock and breaks answer not found; the Timesheets pages and the clock-in strip are hidden." },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatedModule {
    pub name: &'static str,
    pub label: &'static str,
    pub off_means: &'static str,
}

/// One row of `GET /settings/modules` (`ModuleConfigResponse`), the fields
/// the client reads.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ModuleConfig {
    #[serde(default)]
    pub module_name: String,
    #[serde(default)]
    pub is_enabled: bool,
}

/// What the tenant has turned on, and which tenant generation it is for.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModuleFlags {
    pub generation: u64,
    pub enabled: Vec<String>,
}

pub static MODULE_FLAGS: GlobalSignal<Option<ModuleFlags>> = Signal::global(|| None);
static LOADING_FOR: GlobalSignal<Option<u64>> = Signal::global(|| None);

/// Is `name` on in `rows`? A missing row is off.
pub fn enabled_in(rows: &[ModuleConfig], name: &str) -> bool {
    rows.iter().any(|r| r.module_name == name && r.is_enabled)
}

/// Read the flags for the active tenant into [`MODULE_FLAGS`]. Called by
/// the hook when the cache is missing or from another tenant, and by the
/// Modules page after a toggle.
pub fn refresh_module_flags() {
    #[cfg(feature = "app")]
    {
        let generation = crate::hooks::fetch::active_tenant_generation();
        if *LOADING_FOR.read() == Some(generation) {
            return;
        }
        *LOADING_FOR.write() = Some(generation);
        spawn(async move {
            let rows =
                crate::hooks::fetch::api::get_all_authed::<ModuleConfig>("/settings/modules")
                    .await
                    .inspect_err(|e| {
                        tracing::warn!("module flags load failed, every module reads as off: {e}")
                    })
                    .unwrap_or_default();
            let enabled = rows
                .iter()
                .filter(|r| r.is_enabled)
                .map(|r| r.module_name.clone())
                .collect();
            *MODULE_FLAGS.write() = Some(ModuleFlags {
                generation,
                enabled,
            });
            *LOADING_FOR.write() = None;
        });
    }
}

/// The flags, loading them when they are missing or stale.
pub fn use_module_flags() -> Option<ModuleFlags> {
    #[cfg(feature = "app")]
    {
        let generation = crate::hooks::fetch::active_tenant_generation();
        let staff = crate::hooks::fetch::api::current_access_token().is_some()
            && !crate::hooks::fetch::api::has_contact_session();
        use_effect(use_reactive!(|generation, staff| {
            if !staff {
                return;
            }
            let stale = MODULE_FLAGS
                .read()
                .as_ref()
                .is_none_or(|f| f.generation != generation);
            if stale {
                refresh_module_flags();
            }
        }));
        let flags = MODULE_FLAGS.read().clone();
        flags.filter(|f| f.generation == generation)
    }
    #[cfg(not(feature = "app"))]
    {
        None
    }
}

/// Is the gated module `name` on for the active tenant? Off until known,
/// off for a contact session, off for a module with no row.
pub fn use_module_enabled(name: &str) -> bool {
    use_module_flags().is_some_and(|f| f.enabled.iter().any(|m| m == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_row_is_off_and_a_row_says_what_it_says() {
        let rows = vec![
            ModuleConfig {
                module_name: "timesheets".into(),
                is_enabled: false,
            },
            ModuleConfig {
                module_name: "billing".into(),
                is_enabled: true,
            },
        ];
        assert!(!enabled_in(&rows, "timesheets"));
        assert!(enabled_in(&rows, "billing"));
        assert!(!enabled_in(&rows, "projects"), "no row is off");
    }

    /// The client's table names the server's ten gated modules and no other.
    #[test]
    fn gated_modules_are_the_servers() {
        let names: Vec<&str> = GATED_MODULES.iter().map(|m| m.name).collect();
        assert_eq!(
            names,
            [
                "billing",
                "projects",
                "calendar",
                "contracts",
                "assets",
                "knowledge_base",
                "rmm_integration",
                "reports",
                "time_tracking",
                "timesheets",
            ]
        );
        for m in GATED_MODULES {
            assert!(
                !m.label.is_empty() && m.off_means.ends_with('.'),
                "{}",
                m.name
            );
        }
    }
}
