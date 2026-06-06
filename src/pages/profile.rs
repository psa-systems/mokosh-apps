//! Profile page (mokosh-side).
//!
//! Three sections, layered from "identity owned by Bunyip" through
//! "tenant-scoped fields on the mokosh user row" to "local-only UI
//! preferences":
//!
//! 1. **Bunyip identity strip.** Full name, email, and role, sourced
//!    from `AuthContext` (which the SPA hydrates from the OIDC
//!    id_token + a periodic `/v1/auth/me` refresh against bunyip).
//!    Read-only here; editing requires the "Account Settings" link
//!    that bounces over to bunyip-web's `/settings`.
//!
//! 2. **Personal info.** First / last name, title, phone, mobile,
//!    timezone. Lives on mokosh-server's `users` row, edited via
//!    `GET` + `PUT /api/v1/auth/me`. mokosh-server's
//!    `update_current_user` handler already strips role / status from
//!    the inbound request, so the form does not need to defend
//!    against escalation.
//!
//! 3. **Preferences.** Theme, time format, and first day of week.
//!    Persisted to `localStorage` via `utils::prefs`; no server
//!    round-trip. Applies immediately; theme toggling re-applies the
//!    `<html class="dark">` Tailwind variant via `hooks::theme`.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    AppLayout, Button, ButtonVariant, Card, Input, PageHeader, Select, SelectOption,
};
use crate::hooks::theme::{self, Theme};
use crate::utils::prefs;

/// Subset of mokosh-server's `UserResponse` the profile screen reads.
/// Mirrors the shape returned by `GET /api/v1/auth/me`; fields not
/// rendered here are dropped at deserialise time.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct MeResponse {
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    mobile: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    timezone: String,
}

/// Body sent on `PUT /api/v1/auth/me`. Matches mokosh-server's
/// `UpdateUserRequest`; fields not editable from this screen are
/// omitted so the server's existing validation rejects any attempt to
/// change them. Empty optionals are sent as `None` (no-op on the
/// server) rather than empty strings.
#[derive(Clone, Debug, Serialize)]
struct UpdateMeRequest {
    first_name: Option<String>,
    last_name: Option<String>,
    phone: Option<String>,
    mobile: Option<String>,
    title: Option<String>,
    timezone: Option<String>,
}

fn optional_field(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ── Timezone dropdown ───────────────────────────────────────────────────────
//
// A curated, alphabetised list of IANA tz names covering the regions
// mokosh customers actually serve. NOT exhaustive (the IANA database
// has ~600 zones); the goal is a usable picker, not a full atlas.
// `Etc/UTC` stays at the top because it is the schema default and
// what JIT-provisioned rows arrive with. If the user's stored
// timezone is not in this list, the form prepends it as a "(current)"
// entry so saving does not silently overwrite an unknown choice.
const COMMON_TIMEZONES: &[(&str, &str)] = &[
    ("Etc/UTC", "UTC (Coordinated Universal Time)"),
    // Americas
    ("America/Anchorage", "America/Anchorage (Alaska)"),
    ("America/Chicago", "America/Chicago (Central US)"),
    ("America/Denver", "America/Denver (Mountain US)"),
    ("America/Halifax", "America/Halifax (Atlantic Canada)"),
    ("America/Los_Angeles", "America/Los Angeles (Pacific US)"),
    ("America/Mexico_City", "America/Mexico City"),
    ("America/New_York", "America/New York (Eastern US)"),
    ("America/Phoenix", "America/Phoenix (Arizona, no DST)"),
    ("America/Sao_Paulo", "America/Sao Paulo"),
    ("America/Toronto", "America/Toronto"),
    ("America/Vancouver", "America/Vancouver"),
    ("Pacific/Honolulu", "Pacific/Honolulu (Hawaii)"),
    // Europe
    ("Europe/Amsterdam", "Europe/Amsterdam"),
    ("Europe/Berlin", "Europe/Berlin"),
    ("Europe/Dublin", "Europe/Dublin"),
    ("Europe/Helsinki", "Europe/Helsinki"),
    ("Europe/Lisbon", "Europe/Lisbon"),
    ("Europe/London", "Europe/London"),
    ("Europe/Madrid", "Europe/Madrid"),
    ("Europe/Paris", "Europe/Paris"),
    ("Europe/Rome", "Europe/Rome"),
    ("Europe/Stockholm", "Europe/Stockholm"),
    ("Europe/Warsaw", "Europe/Warsaw"),
    ("Europe/Zurich", "Europe/Zurich"),
    // Africa / Middle East
    ("Africa/Cairo", "Africa/Cairo"),
    ("Africa/Johannesburg", "Africa/Johannesburg"),
    ("Asia/Dubai", "Asia/Dubai"),
    ("Asia/Jerusalem", "Asia/Jerusalem"),
    ("Asia/Riyadh", "Asia/Riyadh"),
    // Asia
    ("Asia/Hong_Kong", "Asia/Hong Kong"),
    ("Asia/Karachi", "Asia/Karachi"),
    ("Asia/Kolkata", "Asia/Kolkata (India)"),
    ("Asia/Seoul", "Asia/Seoul"),
    ("Asia/Shanghai", "Asia/Shanghai"),
    ("Asia/Singapore", "Asia/Singapore"),
    ("Asia/Tokyo", "Asia/Tokyo"),
    // Australia / Pacific
    ("Australia/Brisbane", "Australia/Brisbane (no DST)"),
    ("Australia/Melbourne", "Australia/Melbourne"),
    ("Australia/Perth", "Australia/Perth"),
    ("Australia/Sydney", "Australia/Sydney"),
    ("Pacific/Auckland", "Pacific/Auckland"),
];

/// Browser-detected IANA timezone via
/// `Intl.DateTimeFormat().resolvedOptions().timeZone`. Returns `None`
/// when the API is unavailable (older browser, non-web build, or some
/// hardened environment without Intl). The fallback is the form's
/// own initial value, which is whatever mokosh-server gave us.
#[cfg(feature = "web")]
fn browser_timezone() -> Option<String> {
    use wasm_bindgen::{JsCast, JsValue};
    let intl = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("Intl")).ok()?;
    let dtf_ctor = js_sys::Reflect::get(&intl, &JsValue::from_str("DateTimeFormat")).ok()?;
    let dtf_fn = dtf_ctor.dyn_into::<js_sys::Function>().ok()?;
    let dtf = js_sys::Reflect::construct(&dtf_fn, &js_sys::Array::new()).ok()?;
    let resolved_options_fn = js_sys::Reflect::get(&dtf, &JsValue::from_str("resolvedOptions"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    let resolved = resolved_options_fn.call0(&dtf).ok()?;
    js_sys::Reflect::get(&resolved, &JsValue::from_str("timeZone"))
        .ok()?
        .as_string()
}

#[cfg(not(feature = "web"))]
fn browser_timezone() -> Option<String> {
    None
}

/// Build the option list, prepending the currently-stored value if it
/// isn't already in `COMMON_TIMEZONES` (so a user with an exotic zone
/// can keep it without us silently changing it on save).
fn timezone_options(current: &str) -> Vec<SelectOption> {
    let mut opts: Vec<SelectOption> = COMMON_TIMEZONES
        .iter()
        .map(|(v, l)| SelectOption::new(*v, *l))
        .collect();
    let known = COMMON_TIMEZONES.iter().any(|(v, _)| *v == current);
    if !known && !current.is_empty() {
        opts.insert(
            0,
            SelectOption::new(current, format!("{current} (current)")),
        );
    }
    opts
}

// ── Preference keys ─────────────────────────────────────────────────────────
//
// String prefs use `utils::prefs::get_str/set_str`. Keep the keys
// stable across releases; renaming would silently reset every user.

const PREF_TIME_FORMAT: &str = "mokosh_time_format";
const PREF_FIRST_DAY: &str = "mokosh_first_day_of_week";

#[component]
pub fn ProfilePage() -> Element {
    let me_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_authed::<MeResponse>("/auth/me")
                .await
                .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            None::<MeResponse>
        }
    });

    let snap = me_resource.read_unchecked();
    rsx! {
        AppLayout { title: "Profile",
            PageHeader {
                title: "Profile",
                subtitle: "Identity, personal info, and your local preferences.",
            }

            // Identity strip is rendered unconditionally: it reads
            // from AuthContext (already loaded by the time this page
            // mounts) so it does not block on the mokosh `/auth/me`
            // round-trip.
            IdentityStrip {}

            match &*snap {
                None => rsx! {
                    Card {
                        div { class: "py-12 text-center text-sm text-gray-500 dark:text-gray-400",
                            "Loading profile..."
                        }
                    }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-12 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300",
                                "Could not load your profile. Refresh the page to retry."
                            }
                        }
                    }
                },
                Some(Some(me)) => rsx! {
                    PersonalInfoForm { initial: me.clone() }
                },
            }

            PreferencesCard {}
        }
    }
}

/// Top-of-page strip. Pulls the user's identity (name, email, role)
/// from `AuthContext` rather than from the mokosh `/auth/me` payload:
/// `AuthContext` is hydrated from the OIDC id_token at sign-in and
/// then refreshed against bunyip's `/v1/auth/me`, so Bunyip stays
/// authoritative for who the user is. Links over to the Bunyip
/// Account Settings page where these fields are actually editable.
#[component]
fn IdentityStrip() -> Element {
    let auth = crate::hooks::use_auth();
    let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
    let account_settings_url = cfg.hub_url("/settings");

    let auth_read = auth.read();
    let (full_name, initials, email, role) = match auth_read.user.as_ref() {
        Some(u) => (
            u.full_name(),
            u.initials(),
            u.email.clone(),
            format!("{:?}", u.role),
        ),
        None => (
            "Unknown".to_string(),
            "?".to_string(),
            String::new(),
            String::new(),
        ),
    };

    rsx! {
        Card {
            div { class: "flex flex-wrap items-center gap-4 p-6",
                // Initials disc. The avatar URL pipeline lands as a
                // follow-up; until then we render initials in a
                // gradient pill so the strip is not visually flat.
                div { class: "flex h-14 w-14 items-center justify-center rounded-full bg-gradient-to-br from-blue-500 to-indigo-600 text-white text-lg font-semibold",
                    "{initials}"
                }
                div { class: "flex-1 min-w-0",
                    p { class: "text-lg font-semibold text-gray-900 dark:text-white truncate",
                        "{full_name}"
                    }
                    if !email.is_empty() {
                        p { class: "text-sm text-gray-500 dark:text-gray-400 truncate",
                            "{email}"
                        }
                    }
                    if !role.is_empty() {
                        p { class: "mt-1 text-xs uppercase tracking-wide text-gray-500 dark:text-gray-400",
                            "Role: {role}"
                        }
                    }
                }
                div { class: "flex flex-col items-end gap-1",
                    a {
                        href: "{account_settings_url}",
                        class: "text-sm font-medium text-blue-600 dark:text-blue-400 hover:underline",
                        "Account Settings (Bunyip)"
                    }
                    p { class: "text-xs text-gray-500 dark:text-gray-400 text-right max-w-xs",
                        "Name, email, password, 2FA, sessions, and billing are owned by Bunyip. Change them there."
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PersonalInfoFormProps {
    initial: MeResponse,
}

/// Editable mokosh-side fields. Same shape PR #82 shipped; first /
/// last name remain editable because mokosh's `users` row carries
/// them per tenant (a user may go by "Sam" in one PSA tenant and
/// "Samantha" in another), but the identity strip above shows
/// Bunyip's canonical name so the difference is visible at a glance.
#[component]
fn PersonalInfoForm(props: PersonalInfoFormProps) -> Element {
    let mut first_name = use_signal(|| props.initial.first_name.clone());
    let mut last_name = use_signal(|| props.initial.last_name.clone());
    let mut phone = use_signal(|| props.initial.phone.clone().unwrap_or_default());
    let mut mobile = use_signal(|| props.initial.mobile.clone().unwrap_or_default());
    let mut title = use_signal(|| props.initial.title.clone().unwrap_or_default());
    // Default the timezone signal to the saved value; when the saved
    // value is the JIT placeholder ("UTC" or "Etc/UTC"), seed with the
    // browser's detected zone so the form arrives pre-populated with
    // a sensible local choice. The user can change it before saving.
    let initial_tz = props.initial.timezone.clone();
    let mut timezone = use_signal(|| {
        let saved = initial_tz.clone();
        let looks_placeholder = saved.is_empty() || saved == "UTC" || saved == "Etc/UTC";
        if looks_placeholder {
            browser_timezone().unwrap_or(saved)
        } else {
            saved
        }
    });

    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saved = use_signal(|| false);

    let handle_save = move |_| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        saved.set(false);
        let body = UpdateMeRequest {
            first_name: optional_field(&first_name()),
            last_name: optional_field(&last_name()),
            phone: optional_field(&phone()),
            mobile: optional_field(&mobile()),
            title: optional_field(&title()),
            timezone: optional_field(&timezone()),
        };
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::put_authed_typed::<MeResponse, _>("/auth/me", &body)
                    .await
                {
                    Ok(_) => saved.set(true),
                    Err(e) => error.set(format!("Could not save profile: {}", e.user_message())),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = body;
            }
            saving.set(false);
        });
    };

    rsx! {
        Card {
            div { class: "space-y-6 p-6",
                div {
                    h2 { class: "text-base font-semibold text-gray-900 dark:text-white",
                        "Personal info"
                    }
                    p { class: "text-sm text-gray-500 dark:text-gray-400",
                        "How you show up in this organization. Saved on mokosh."
                    }
                }

                if !error().is_empty() {
                    div { class: "rounded-md bg-red-50 dark:bg-red-900/40 p-3 text-sm text-red-700 dark:text-red-300",
                        "{error}"
                    }
                }
                if saved() {
                    div { class: "rounded-md bg-green-50 dark:bg-green-900/40 p-3 text-sm text-green-700 dark:text-green-300",
                        "Profile saved."
                    }
                }

                div { class: "grid gap-4 sm:grid-cols-2",
                    Input {
                        name: "first_name",
                        label: "First name",
                        value: first_name(),
                        oninput: move |e: FormEvent| first_name.set(e.value()),
                    }
                    Input {
                        name: "last_name",
                        label: "Last name",
                        value: last_name(),
                        oninput: move |e: FormEvent| last_name.set(e.value()),
                    }
                    Input {
                        name: "title",
                        label: "Title",
                        placeholder: "e.g. Senior Technician",
                        value: title(),
                        oninput: move |e: FormEvent| title.set(e.value()),
                    }
                    Select {
                        name: "timezone",
                        label: "Timezone",
                        options: timezone_options(&timezone()),
                        value: timezone(),
                        help: "Affects appointment + dispatch grid times. Auto-detected from your browser when unset.",
                        onchange: move |e: FormEvent| timezone.set(e.value()),
                    }
                    Input {
                        name: "phone",
                        label: "Work phone",
                        r#type: "tel".to_string(),
                        value: phone(),
                        oninput: move |e: FormEvent| phone.set(e.value()),
                    }
                    Input {
                        name: "mobile",
                        label: "Mobile",
                        r#type: "tel".to_string(),
                        value: mobile(),
                        oninput: move |e: FormEvent| mobile.set(e.value()),
                    }
                }

                div { class: "flex justify-end",
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: handle_save,
                        disabled: saving(),
                        if saving() { "Saving..." } else { "Save changes" }
                    }
                }
            }
        }
    }
}

/// Local-only preferences. No server writes; everything persists to
/// `localStorage` via `utils::prefs`. Applies immediately on
/// selection.
#[component]
fn PreferencesCard() -> Element {
    let mut theme_value = use_signal(theme::current);
    let mut time_format = use_signal(|| prefs::get_str(PREF_TIME_FORMAT, "12h"));
    let mut first_day = use_signal(|| prefs::get_str(PREF_FIRST_DAY, "sunday"));

    rsx! {
        Card {
            div { class: "space-y-6 p-6",
                div {
                    h2 { class: "text-base font-semibold text-gray-900 dark:text-white",
                        "Preferences"
                    }
                    p { class: "text-sm text-gray-500 dark:text-gray-400",
                        "Saved on this device. Applies immediately."
                    }
                }

                div { class: "grid gap-6 sm:grid-cols-3",
                    // Theme
                    fieldset { class: "space-y-2",
                        legend { class: "text-sm font-medium text-gray-700 dark:text-gray-300",
                            "Theme"
                        }
                        for opt in [Theme::System, Theme::Light, Theme::Dark] {
                            label {
                                class: "flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300",
                                input {
                                    r#type: "radio",
                                    name: "theme",
                                    value: "{opt.as_str()}",
                                    checked: theme_value() == opt,
                                    onchange: move |_| {
                                        theme_value.set(opt);
                                        theme::set(opt);
                                    },
                                }
                                match opt {
                                    Theme::System => "Match system",
                                    Theme::Light => "Light",
                                    Theme::Dark => "Dark",
                                }
                            }
                        }
                    }

                    // Time format
                    fieldset { class: "space-y-2",
                        legend { class: "text-sm font-medium text-gray-700 dark:text-gray-300",
                            "Time format"
                        }
                        for (val, label) in [("12h", "12-hour (1:30 PM)"), ("24h", "24-hour (13:30)")] {
                            label {
                                class: "flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300",
                                input {
                                    r#type: "radio",
                                    name: "time_format",
                                    value: "{val}",
                                    checked: time_format() == val,
                                    onchange: move |_| {
                                        time_format.set(val.to_string());
                                        prefs::set_str(PREF_TIME_FORMAT, val);
                                    },
                                }
                                "{label}"
                            }
                        }
                    }

                    // First day of week
                    fieldset { class: "space-y-2",
                        legend { class: "text-sm font-medium text-gray-700 dark:text-gray-300",
                            "First day of week"
                        }
                        for (val, label) in [("sunday", "Sunday"), ("monday", "Monday")] {
                            label {
                                class: "flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300",
                                input {
                                    r#type: "radio",
                                    name: "first_day",
                                    value: "{val}",
                                    checked: first_day() == val,
                                    onchange: move |_| {
                                        first_day.set(val.to_string());
                                        prefs::set_str(PREF_FIRST_DAY, val);
                                    },
                                }
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
