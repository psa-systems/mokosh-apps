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
//! 2. **Personal info.** Title, mobile,
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
    use_page_title, BannerTone, Button, ButtonVariant, Card, ErrorBanner, Input, Modal, ModalSize,
    PageHeader, Select, SelectOption, StatusBanner,
};
use crate::utils::datetime::{format_user_datetime, preset_label, token_warnings, PRESET_FORMATS};
use crate::utils::prefs;
use crate::Route;

/// Subset of mokosh-server's `UserResponse` the profile screen reads.
/// Mirrors the shape returned by `GET /api/v1/auth/me`; fields not
/// rendered here are dropped at deserialise time.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct MeResponse {
    #[serde(default)]
    mobile: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    timezone: String,
    /// PMS-253: per-user date/time format string. None means "use the
    /// browser locale" (the legacy rendering behaviour).
    #[serde(default)]
    date_format_string: Option<String>,
}

/// MAPPS-604: subset of mokosh-server's `ContactMe` the contact-facing
/// profile screen reads. Mirrors the shape returned by
/// `GET /api/v1/contact/auth/me`. Fields not rendered here are dropped
/// at deserialise time.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ContactMeResponse {
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    /// Display-only for contacts; staff CRM owns the identity, so this
    /// is rendered but the form has no submit path for it.
    #[serde(default)]
    email: String,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    mobile: Option<String>,
    #[serde(default)]
    timezone: String,
    #[serde(default)]
    company_name: Option<String>,
}

/// Body for `PUT /api/v1/contact/auth/me`. Omits `email` (staff owns
/// contact identity per prompt 013a). Every field is optional so a
/// partial edit sends only the touched fields.
#[derive(Clone, Debug, Default, Serialize)]
struct UpdateContactMeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mobile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timezone: Option<String>,
}

/// Body sent on `PUT /api/v1/auth/me`. Matches mokosh-server's
/// `UpdateUserRequest`; fields not editable from this screen are
/// omitted so the server's existing validation rejects any attempt to
/// change them. Empty optionals are sent as `None` (no-op on the
/// server) rather than empty strings.
///
/// PMS-512 / MAPPS-431: `first_name`, `last_name` and `phone` are
/// deliberately absent from the wire even though the form still binds them
/// to input signals for display. Bunyip is the identity source of truth for
/// the names + phone; mokosh keeps them as a read-only cache refreshed on
/// every login via `upsert_user_from_oidc`. Sending them here was worse than
/// not asking: the user typed a new name, hit Save, saw a "Saved" toast, and
/// the request reached no column. The input fields stay visible so the user
/// can see the current bunyip-sourced values; the wire strips them so the
/// request stops pretending to persist edits it never persisted. Nothing
/// here may send a field the server does not accept.
#[derive(Clone, Debug, Serialize)]
struct UpdateMeRequest {
    mobile: Option<String>,
    title: Option<String>,
    timezone: Option<String>,
    /// PMS-253: send the picked preset back as a non-empty string, or
    /// `None` to clear the pref and fall back to the browser locale.
    date_format_string: Option<String>,
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
/// `Intl.DateTimeFormat().resolvedOptions().timeZone` in the browser,
/// the OS zone on the desktop (MAPPS-504, `crate::platform::tz`).
/// Returns `None` when the host will not say, and the fallback is the
/// form's own initial value, which is whatever mokosh-server gave us.
fn browser_timezone() -> Option<String> {
    crate::platform::tz::local_iana()
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
    // MAPPS-604: a contact-plane session sees a different profile body
    // (fetches `/contact/auth/me`, PUTs the same route, email is
    // read-only). Staff sessions fall through to the workspace body
    // below, unchanged. The gate re-checks `settings:manage_own` so a
    // contact role without the capability sees the same
    // `PermissionRequired` splash the sidebar entry already hides
    // behind.
    #[cfg(feature = "web")]
    if crate::hooks::fetch::api::has_contact_session()
        && crate::hooks::fetch::api::current_access_token().is_none()
    {
        return rsx! { ContactProfilePage {} };
    }

    use_page_title("Profile");
    // MAPPS-331: keep the actual `/auth/me` failure mode on the resource
    // (status + server message) instead of collapsing every fault into a
    // bare `None`. The banner below surfaces the real reason so the user
    // can hand a concrete fault back without DevTools digging, and so a
    // future regression is observable from the page itself.
    let me_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the profile auto-refetches
        // the instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::api::get_authed_typed::<MeResponse>("/auth/me").await
        }
        #[cfg(not(feature = "app"))]
        {
            Err::<MeResponse, crate::hooks::fetch::api::ApiError>(
                crate::hooks::fetch::api::ApiError::Network("non-app build".into()),
            )
        }
    });

    let snap = me_resource.read_unchecked();

    // MAPPS-357: /auth/me is this page's PRIMARY resource (the fetched entity
    // the personal-info form edits). A failed load while the server is flagged
    // down is an outage, not a real profile error - render the honest
    // unavailable state (which keeps the nav + banner) instead of the inline
    // "Could not load your profile" error card below. A fetch that fails while
    // the server is still reachable (a 4xx) keeps the MAPPS-331 error banner
    // with its concrete detail. This early return sits AFTER the only hook in
    // this body (`me_resource`) so hook order is preserved.
    let fetch_failed = matches!(&*snap, Some(Err(_)));
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Profile".to_string() }
        };
    }

    rsx! {
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
                crate::components::DetailSkeleton {} // PMS-353
            },
            Some(Err(err)) => {
                let detail = err.to_string();
                let toast = err.user_message();
                rsx! {
                    Card {
                        div { class: "py-12 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300",
                                "Could not load your profile: {toast}"
                            }
                            p { class: "mt-2 text-xs text-muted",
                                "Detail: {detail}"
                            }
                        }
                    }
                }
            }
            Some(Ok(me)) => rsx! {
                PersonalInfoForm { initial: me.clone() }
            },
        }

        PreferencesCard {}
    }
}

/// Top-of-page strip. Pulls the user's identity (name, email, role)
/// from `AuthContext` rather than from the mokosh `/auth/me` payload:
/// `AuthContext` is hydrated from the OIDC id_token at sign-in and
/// then refreshed against bunyip's `/v1/auth/me`, so Bunyip stays
/// authoritative for who the user is. Links over to the Bunyip
/// Account Settings page where these fields are actually editable.
///
/// Confirmed MAPPS-138: mokosh-server's full settings surface
/// (`src/modules/settings/routes.rs`) is intentionally hub-only and is
/// not consumed by this SPA; profile editing redirects to the Bunyip hub
/// rather than rendering an in-app settings UI.
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
    let brand = crate::branding::product_name();

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
                    p { class: "text-lg font-semibold text-content truncate",
                        "{full_name}"
                    }
                    if !email.is_empty() {
                        p { class: "text-sm text-muted truncate",
                            "{email}"
                        }
                    }
                    if !role.is_empty() {
                        // MAPPS-329: explicit "<brand> Role" so a user with
                        // admin-on-mokosh does not assume the same level on
                        // the Bunyip hub. The Bunyip role is a separate
                        // claim issued by the OP and managed in Bunyip's
                        // own admin surface.
                        p { class: "mt-1 text-xs uppercase tracking-wide text-muted",
                            "{brand} Role: {role}"
                        }
                        p { class: "text-xs text-muted",
                            "Bunyip hub role is separate."
                        }
                    }
                }
                div { class: "flex flex-col items-end gap-1",
                    a {
                        href: "{account_settings_url}",
                        class: "text-sm font-medium text-accent hover:underline",
                        "Account Settings (Bunyip)"
                    }
                    p { class: "text-xs text-muted text-right max-w-xs",
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
    // PMS-253: per-user date/time format. Empty string = "Browser
    // default" sentinel, sent to the server as None on save.
    let date_format = use_signal(|| props.initial.date_format_string.clone().unwrap_or_default());

    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saved = use_signal(|| false);

    // MAPPS-357: block the profile PUT while the server is unreachable so a
    // Save cannot silently fail (edits are discarded, not queued). Reactive:
    // the button re-enables itself on reconnect.
    let can_mutate = crate::hooks::use_can_mutate();

    let handle_save = move |_| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        saved.set(false);
        // PMS-512: first_name / last_name / phone stripped from the
        // wire; bunyip owns those fields (see UpdateMeRequest doc).
        let body = UpdateMeRequest {
            mobile: optional_field(&mobile()),
            title: optional_field(&title()),
            timezone: optional_field(&timezone()),
            date_format_string: optional_field(&date_format()),
        };
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::put_authed_typed::<MeResponse, _>("/auth/me", &body)
                    .await
                {
                    Ok(_) => saved.set(true),
                    Err(e) => error.set(format!("Could not save profile: {}", e.user_message())),
                }
            }
            #[cfg(not(feature = "app"))]
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
                    h2 { class: "text-base font-semibold text-content",
                        "Personal info"
                    }
                    p { class: "text-sm text-muted",
                        "How you show up in this organization. Saved on mokosh."
                    }
                }

                if !error().is_empty() {
                    ErrorBanner { "{error}" }
                }
                if saved() {
                    StatusBanner { tone: BannerTone::Success, "Profile saved." }
                }

                div { class: "grid gap-4 sm:grid-cols-2",
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
                    DateFormatField { value: date_format }
                    Input {
                        name: "mobile",
                        label: "Mobile",
                        r#type: "tel".to_string(),
                        value: mobile(),
                        help: "Your own number, stored here. Your name and work phone belong to your account; change those in Account Settings above.".to_string(),
                        oninput: move |e: FormEvent| mobile.set(e.value()),
                    }
                }

                div { class: "flex justify-end",
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: handle_save,
                        disabled: saving() || !can_mutate,
                        loading: saving(),
                        title: (!can_mutate).then(|| "Can't save changes while the server is unreachable".to_string()),
                        "Save changes"
                    }
                }
            }
        }
    }
}

/// PMS-253: date/time format picker that sits next to the timezone
/// dropdown. Ships the preset list + a "Custom…" button that opens
/// the [`CustomFormatBuilder`] modal (PMS-254). The matching token
/// grammar + renderer live in [`crate::utils::datetime`].
#[component]
fn DateFormatField(value: Signal<String>) -> Element {
    let mut value = value;
    let mut show_builder = use_signal(|| false);
    let preview_now = chrono::Utc::now();
    let current = value();
    let preview = if current.trim().is_empty() {
        "Browser default".to_string()
    } else {
        format_user_datetime(preview_now, Some(&current))
    };

    let mut opts: Vec<SelectOption> = vec![SelectOption::new("", "Browser default (locale)")];
    // MAPPS-144: prefill each preset with a live example rendered
    // against the current instant so the option reads as
    // `Jun-11-2026 08:40 (MMM-DD-YYYY HH:mm)` instead of the bare
    // token string.
    for (_label, fmt) in PRESET_FORMATS {
        opts.push(SelectOption::new(*fmt, preset_label(preview_now, fmt)));
    }
    // If the user already has a custom format that isn't in the preset
    // list (saved via the Custom builder), surface it as the active
    // option so saving doesn't silently overwrite it.
    if !current.is_empty() && !PRESET_FORMATS.iter().any(|(_, f)| *f == current) {
        opts.push(SelectOption::new(
            current.clone(),
            format!("{} (custom)", preset_label(preview_now, &current)),
        ));
    }

    rsx! {
        div { class: "space-y-2",
            Select {
                name: "date_format_string",
                label: "Date & time format",
                options: opts,
                value: current.clone(),
                help: "Applied everywhere a timestamp is shown. \"Browser default\" follows your system locale.",
                onchange: move |e: FormEvent| value.set(e.value()),
            }
            div { class: "flex items-center gap-3 text-xs text-muted",
                span { class: "font-medium text-content",
                    "Preview:"
                }
                span { "{preview}" }
            }
            div {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| show_builder.set(true),
                    "Custom\u{2026}"
                }
            }
            CustomFormatBuilder { value: value, open: show_builder }
        }
    }
}

/// PMS-254: token pill grouped by the component it sets.
#[derive(PartialEq)]
struct TokenGroup {
    label: &'static str,
    items: &'static [(&'static str, &'static str)], // (token, descriptor)
}

const TOKEN_GROUPS: &[TokenGroup] = &[
    TokenGroup {
        label: "Year",
        items: &[("YYYY", "4-digit"), ("YY", "2-digit")],
    },
    TokenGroup {
        label: "Month",
        items: &[
            ("MM", "padded"),
            ("M", "short"),
            ("MMM", "abbr"),
            ("MMMM", "full"),
        ],
    },
    TokenGroup {
        label: "Day",
        items: &[("DD", "padded"), ("D", "short"), ("Do", "ordinal")],
    },
    TokenGroup {
        label: "Weekday",
        items: &[("ddd", "abbr"), ("dddd", "full")],
    },
    TokenGroup {
        label: "Hour",
        items: &[
            ("HH", "24h pad"),
            ("H", "24h"),
            ("hh", "12h pad"),
            ("h", "12h"),
        ],
    },
    TokenGroup {
        label: "Minute",
        items: &[("mm", "padded"), ("m", "short")],
    },
    TokenGroup {
        label: "Second",
        items: &[("ss", "padded"), ("s", "short")],
    },
    TokenGroup {
        label: "AM/PM",
        items: &[("A", "upper"), ("a", "lower"), ("a.m.", "dots")],
    },
    TokenGroup {
        label: "Separators",
        items: &[
            ("-", "dash"),
            ("/", "slash"),
            (".", "dot"),
            (",", "comma"),
            (" ", "space"),
            (":", "colon"),
        ],
    },
];

/// PMS-254: free-form custom date/time format builder.
///
/// Opens in a modal triggered by the "Custom…" button under the
/// preset dropdown. The user picks tokens via the pill grid or types
/// directly into the format string input; either path keeps the live
/// preview at the top in sync. Unrecognized alphabetic runs (e.g. a
/// typo'd `yyyy`) light up as a yellow warning so the user knows the
/// renderer will pass that text through verbatim.
///
/// Saving from the modal just writes the format back into the parent
/// signal; the actual persistence happens when the user clicks Save
/// changes on the Profile page, sharing the same /me PUT round-trip
/// the preset slice already uses.
#[component]
fn CustomFormatBuilder(value: Signal<String>, open: Signal<bool>) -> Element {
    let mut value = value;
    let mut open = open;
    // Local draft so cancelling the modal does not stomp the parent's
    // saved value (e.g. the user opens the builder, makes a mess, hits
    // Cancel: the previously-saved preset survives).
    let initial = value();
    let mut draft = use_signal(|| initial.clone());
    // Reseed the draft each time the modal opens so a reopened builder
    // starts from the currently-saved value, not the last cancelled
    // edit. `open` flipping to true is the trigger.
    use_effect(move || {
        if open() {
            draft.set(value());
        }
    });

    let draft_str = draft();
    let preview_now = chrono::Utc::now();
    let preview = if draft_str.trim().is_empty() {
        "Browser default".to_string()
    } else {
        format_user_datetime(preview_now, Some(&draft_str))
    };
    let warnings = token_warnings(&draft_str);
    let preset_value = if PRESET_FORMATS.iter().any(|(_, f)| *f == draft_str) {
        draft_str.clone()
    } else {
        String::new()
    };

    let mut preset_opts: Vec<SelectOption> = vec![SelectOption::new("", "(none -- custom format)")];
    // MAPPS-144: same prefilled example labels as the main picker.
    for (_label, fmt) in PRESET_FORMATS {
        preset_opts.push(SelectOption::new(*fmt, preset_label(preview_now, fmt)));
    }

    rsx! {
        Modal {
            open: open(),
            title: "Custom date & time format".to_string(),
            size: ModalSize::XLarge,
            onclose: move |_| open.set(false),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| open.set(false),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: !warnings.is_empty(),
                    title: (!warnings.is_empty())
                        .then(|| "Resolve the format warnings before applying.".to_string()),
                    onclick: move |_| {
                        value.set(draft());
                        open.set(false);
                    },
                    "Apply"
                }
            },
            div { class: "space-y-4 p-4",
                Select {
                    name: "preset_seed",
                    label: "Start from a preset",
                    options: preset_opts,
                    value: preset_value,
                    help: "Pick a preset to populate the format string below, then tweak.",
                    onchange: move |e: FormEvent| {
                        let v = e.value();
                        if !v.is_empty() {
                            draft.set(v);
                        }
                    },
                }
                Input {
                    name: "format_string",
                    label: "Format string",
                    value: draft_str.clone(),
                    oninput: move |e: FormEvent| draft.set(e.value()),
                }
                div { class: "rounded-md bg-app px-3 py-2 text-sm",
                    span { class: "font-medium text-content", "Preview: " }
                    span { class: "text-content", "{preview}" }
                }
                if !warnings.is_empty() {
                    div { class: "rounded-md bg-yellow-50 dark:bg-yellow-900/40 px-3 py-2 text-sm text-yellow-700 dark:text-yellow-200",
                        "\u{26A0} Unrecognized tokens: "
                        span { class: "font-mono", "{warnings.join(\", \")}" }
                        " -- these will appear as literal text. Fix or remove them to apply."
                    }
                }
                div { class: "rounded-md border border-line p-3 space-y-2",
                    div { class: "text-sm font-medium text-content",
                        "Date Builder"
                    }
                    for group in TOKEN_GROUPS.iter() {
                        TokenGroupRow { group: group, draft: draft }
                    }
                }
            }
        }
    }
}

/// One row of the Date Builder grid: a label on the left, pill buttons
/// on the right. Clicking a pill appends its token to the draft format
/// string (caret-aware insertion is a future polish; v1 appends at the
/// end which preserves typed prefix and is predictable).
#[component]
fn TokenGroupRow(group: &'static TokenGroup, draft: Signal<String>) -> Element {
    let mut draft = draft;
    let preview_now = chrono::Utc::now();
    rsx! {
        div { class: "flex items-start gap-3 py-1",
            div { class: "w-20 shrink-0 text-xs text-muted pt-1.5",
                "{group.label}"
            }
            div { class: "flex flex-wrap gap-1.5",
                for (token, descriptor) in group.items.iter() {
                    {
                        let token = *token;
                        let descriptor = *descriptor;
                        let rendered = if token.chars().all(|c| !c.is_ascii_alphabetic()) {
                            // Separator tokens render as themselves; the
                            // tokenizer would just pass them through.
                            if token == " " { "\u{2423}".to_string() } else { token.to_string() }
                        } else {
                            format_user_datetime(preview_now, Some(token))
                        };
                        rsx! {
                            button {
                                key: "{group.label}-{token}-{descriptor}",
                                class: "inline-flex items-center gap-1 rounded border border-accent-200 dark:border-accent-700 bg-accent-50 dark:bg-accent-900/40 px-2 py-1 text-xs text-accent-700 dark:text-accent-300 hover:bg-accent-100 dark:hover:bg-accent-900/60",
                                title: "Token: {token}",
                                onclick: move |_| {
                                    let mut cur = draft();
                                    cur.push_str(token);
                                    draft.set(cur);
                                },
                                span { class: "font-mono font-medium", "{rendered}" }
                                span { class: "text-accent-500 dark:text-accent-400", "{descriptor}" }
                            }
                        }
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
    let mut time_format = use_signal(|| prefs::get_str(PREF_TIME_FORMAT, "12h"));
    let mut first_day = use_signal(|| prefs::get_str(PREF_FIRST_DAY, "sunday"));
    let mut duration_format = use_signal(|| {
        prefs::get_str(
            crate::utils::duration::PREF_DURATION_FORMAT,
            crate::utils::duration::DEFAULT_DURATION_FORMAT,
        )
    });

    rsx! {
        Card {
            div { class: "space-y-6 p-6",
                div {
                    h2 { class: "text-base font-semibold text-content",
                        "Preferences"
                    }
                    p { class: "text-sm text-muted",
                        "Saved on this device. Applies immediately."
                    }
                }

                div { class: "grid gap-6 sm:grid-cols-3",
                    // Theme + accent moved to Settings > Appearance
                    // (MAPPS-259): one account-synced picker, also reachable
                    // from the swatch in the top bar.
                    fieldset { class: "space-y-2",
                        legend { class: "text-sm font-medium text-content",
                            "Theme"
                        }
                        p { class: "text-sm text-muted",
                            "Theme and accent color are set in "
                            Link {
                                to: Route::SettingsAppearance {},
                                class: "font-medium text-accent hover:opacity-90",
                                "Settings > Appearance"
                            }
                            ", or from the swatch in the top bar."
                        }
                    }

                    // Time format
                    fieldset { class: "space-y-2",
                        legend { class: "text-sm font-medium text-content",
                            "Time format"
                        }
                        for (val, label) in [("12h", "12-hour (1:30 PM)"), ("24h", "24-hour (13:30)")] {
                            label {
                                class: "flex items-center gap-2 text-sm text-content",
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
                        legend { class: "text-sm font-medium text-content",
                            "First day of week"
                        }
                        for (val, label) in [("sunday", "Sunday"), ("monday", "Monday")] {
                            label {
                                class: "flex items-center gap-2 text-sm text-content",
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

                    // Duration format: how logged time is displayed
                    // across timesheets, the time list, and the
                    // dashboard (PMS-265).
                    fieldset { class: "space-y-2",
                        legend { class: "text-sm font-medium text-content",
                            "Duration format"
                        }
                        for (val , label) in [("decimal", "Decimal (1.5h)"), ("hm", "Hours:minutes (1:30)")] {
                            label {
                                class: "flex items-center gap-2 text-sm text-content",
                                input {
                                    r#type: "radio",
                                    name: "duration_format",
                                    value: "{val}",
                                    checked: duration_format() == val,
                                    onchange: move |_| {
                                        duration_format.set(val.to_string());
                                        prefs::set_str(crate::utils::duration::PREF_DURATION_FORMAT, val);
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

// ============================================================================
// MAPPS-604: contact-plane profile.
// ============================================================================

/// Contact-plane profile page. Rendered when [`has_contact_session`]
/// returns true (see `ProfilePage`'s early branch). Fetches
/// `/contact/auth/me`, populates the personal-info form, PUTs back to
/// `/contact/auth/me`. Email is display-only ("Managed by your MSP")
/// per prompt 013a; contacts cannot change identity through the portal.
///
/// Gated on `settings:manage_own` so a contact role without the
/// capability sees the same `PermissionRequired` splash the sidebar
/// entry already hides behind.
#[component]
fn ContactProfilePage() -> Element {
    use_page_title("Profile");

    let can_manage_own = crate::hooks::capabilities::use_capability("settings:manage_own");
    if !can_manage_own {
        return rsx! {
            crate::components::PermissionRequired {
                title: "Profile".to_string(),
                body: "Ask your MSP to grant you the settings:manage_own capability.".to_string(),
            }
        };
    }

    let me_resource = use_resource(|| async {
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_contact_authed::<ContactMeResponse>("/contact/auth/me")
                .await
        }
        #[cfg(not(feature = "web"))]
        {
            Err::<ContactMeResponse, crate::hooks::fetch::api::ApiError>(
                crate::hooks::fetch::api::ApiError::Network("non-web build".into()),
            )
        }
    });

    let snap = me_resource.read_unchecked();

    rsx! {
        PageHeader {
            title: "Profile",
            subtitle: "Your contact information for this portal.",
        }

        match &*snap {
            None => rsx! { crate::components::DetailSkeleton {} },
            Some(Err(err)) => {
                let detail = err.to_string();
                let toast = err.user_message();
                rsx! {
                    Card {
                        div { class: "py-12 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300",
                                "Could not load your profile: {toast}"
                            }
                            p { class: "mt-2 text-xs text-muted", "Detail: {detail}" }
                        }
                    }
                }
            }
            Some(Ok(me)) => rsx! {
                ContactPersonalInfoForm { initial: me.clone() }
            },
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ContactPersonalInfoFormProps {
    initial: ContactMeResponse,
}

/// Editable contact-owned fields. Email is rendered but not editable;
/// staff owns contact identity (see MSP CRM). The submit path PUTs the
/// same `/contact/auth/me` route.
#[component]
fn ContactPersonalInfoForm(props: ContactPersonalInfoFormProps) -> Element {
    let mut first_name = use_signal(|| props.initial.first_name.clone());
    let mut last_name = use_signal(|| props.initial.last_name.clone());
    let mut phone = use_signal(|| props.initial.phone.clone().unwrap_or_default());
    let mut mobile = use_signal(|| props.initial.mobile.clone().unwrap_or_default());
    let mut timezone = use_signal(|| {
        let saved = props.initial.timezone.clone();
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

    let can_mutate = crate::hooks::use_can_mutate();
    let email = props.initial.email.clone();
    let company_name = props.initial.company_name.clone();

    let handle_save = move |_| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        saved.set(false);
        let body = UpdateContactMeRequest {
            first_name: optional_field(&first_name()),
            last_name: optional_field(&last_name()),
            phone: optional_field(&phone()),
            mobile: optional_field(&mobile()),
            timezone: optional_field(&timezone()),
        };
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::put_contact_authed_typed::<ContactMeResponse, _>(
                    "/contact/auth/me",
                    &body,
                )
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
                    h2 { class: "text-base font-semibold text-content", "Personal info" }
                    p { class: "text-sm text-muted",
                        "Your name, contact numbers, and timezone. Email is managed by your MSP."
                    }
                }

                if !error().is_empty() {
                    ErrorBanner { "{error}" }
                }
                if saved() {
                    StatusBanner { tone: BannerTone::Success, "Profile saved." }
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
                    div { class: "space-y-1",
                        label { class: "block text-sm font-medium text-content", "Email" }
                        div {
                            class: "block w-full rounded-md border border-line bg-surface-2 px-3 py-2 text-sm text-content",
                            "{email}"
                        }
                        p { class: "text-xs text-muted", "Managed by your MSP." }
                    }
                    if let Some(name) = company_name.clone() {
                        if !name.trim().is_empty() {
                            div { class: "space-y-1",
                                label { class: "block text-sm font-medium text-content", "Company" }
                                div {
                                    class: "block w-full rounded-md border border-line bg-surface-2 px-3 py-2 text-sm text-content",
                                    "{name}"
                                }
                                p { class: "text-xs text-muted", "Company scope for this portal." }
                            }
                        }
                    }
                    Select {
                        name: "timezone",
                        label: "Timezone",
                        options: timezone_options(&timezone()),
                        value: timezone(),
                        help: "Affects timestamps rendered for you.",
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
                        disabled: saving() || !can_mutate,
                        loading: saving(),
                        title: (!can_mutate).then(|| "Can't save changes while the server is unreachable".to_string()),
                        "Save changes"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// This module's own source, minus this test module: the assertion below
    /// names the very strings it forbids.
    fn production_src() -> &'static str {
        const PROFILE_SRC: &str = include_str!("profile.rs");
        PROFILE_SRC
            .split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .expect("this file has a test module")
    }

    /// MAPPS-431 recurrence guard.
    ///
    /// The page used to send `first_name`, `last_name` and `phone` to
    /// `PUT /auth/me`, which discards all three: PMS-512 removed them from
    /// `UpdateUserRequest` because bunyip owns identity and mokosh keeps a
    /// read-only cache. The PUT succeeded, the keys reached no column, and the
    /// screen said "Saved".
    ///
    /// A source scan rather than a behavioural test, because what is being
    /// pinned is which keys the body carries, and that is visible in the
    /// source. Anything added back here has to exist in `UpdateUserRequest`
    /// first.
    #[test]
    fn the_profile_never_sends_a_field_the_server_discards() {
        let body_start = production_src()
            .find("struct UpdateMeRequest")
            .expect("the request body is defined here");
        let body = &production_src()[body_start..];
        let body = &body[..body.find('}').expect("struct ends")];

        for ignored in ["first_name", "last_name", "phone:"] {
            assert!(
                !body.contains(ignored),
                "`{ignored}` is absent from mokosh-server's UpdateUserRequest, so sending it \
                 saves nothing and tells the user it did"
            );
        }
        // `mobile` IS mokosh's own column, and the distinction is the whole
        // point: it stays.
        assert!(body.contains("mobile"));
    }
}
