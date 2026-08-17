//! MAPPS-395: `/portal/login`, the client portal's sign-in page.
//!
//! Portal identity is a `contacts` row, not a `users` row, so a portal
//! visitor has no agent bearer and the agent bearer is useless here: every
//! `/api/v1/portal/*` route decodes the token and rejects anything whose
//! `typ` is not `portal_access` (mokosh-server `PortalAuthService::decode_token`).
//! The only issuer of such a token is `POST /api/v1/portal/auth/login`, which
//! this page posts to; the result lands in the portal-only holder in
//! `src/hooks/fetch.rs` that the `_portal_authed` fetch helpers read.
//!
//! Every deploy hosts a client's portal at a per-tenant subdomain
//! (`{slug}.client.<apex>`), so the server resolves the tenant from
//! the Host header on every request. The customer sees only Email +
//! Password fields; the legacy `?tenant=` slug fallback that used to
//! render an "Account name" input is gone (a customer who arrived at
//! a bare host has the wrong URL, not a missing account name).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Button, ButtonVariant, Input};
use crate::Route;

/// Request body for `POST /api/v1/portal/auth/login`, matching
/// mokosh-server's `PortalLoginRequest`. `tenant_slug` is always
/// empty on the wire: every portal deploy is per-tenant subdomain,
/// so the server resolves the tenant from Host and
/// `#[serde(skip_serializing_if = "String::is_empty")]` drops the
/// field entirely. Kept as a serialised field only because the
/// server-side DTO still declares it (a wire-shape rename would
/// need coordinated releases).
#[derive(Serialize)]
struct PortalLoginBody {
    #[serde(skip_serializing_if = "String::is_empty")]
    tenant_slug: String,
    email: String,
    password: String,
    /// PMS-729 phase 2 H4: TOTP code entered after the server returned
    /// `mfa_required: true`. Dropped from the wire when empty so the
    /// server sees a clean shape on the first-attempt (no-code) path.
    #[serde(skip_serializing_if = "String::is_empty")]
    mfa_code: String,
    /// PMS-729 phase 2 H4: single-use recovery code. Supplied instead
    /// of `mfa_code` when the customer lost their authenticator device.
    /// Dropped from the wire when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    recovery_code: String,
    /// PMS-729 phase 2 H8: Cloudflare Turnstile response token. Only
    /// needed after the source IP has crossed the failure threshold;
    /// the server returns a 403 CAPTCHA_REQUIRED envelope carrying
    /// the site_key, the SPA renders the widget, and the returned
    /// token comes back here on the retry.
    #[serde(skip_serializing_if = "String::is_empty")]
    captcha_token: String,
}

/// The fields of mokosh-server's `PortalLoginResponse` this page consumes.
/// `expires_at`, `refresh_expires_at`, and `contact` are ignored here:
/// tokens are memory-only and the portal pages read their own data from
/// the API. `refresh_token` (PMS-729 phase 2 H2) is stashed alongside
/// the access token so the auto-refresh hook can rotate before expiry.
/// `mfa_required` (PMS-729 phase 2 H4) is `true` when the server needs a
/// second-factor code; the page then shows the MFA input and re-POSTs.
#[derive(Deserialize)]
struct PortalLoginResp {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    mfa_required: bool,
}

#[component]
pub fn PortalLoginPage() -> Element {
    let nav = use_navigator();

    // Every portal deploy is per-tenant subdomain, so the server
    // resolves the tenant from Host. The legacy `?tenant=` slug fallback
    // is gone; a bare-host visit renders the shell without a slug
    // input and the server returns a wrong-password envelope on any
    // login attempt (no leak of "which tenant this was").
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-729 phase 2 H4: MFA branch. `mfa_required` flips to true
    // when the server said "need a second factor"; the UI then swaps
    // the credentials block for a code input and re-POSTs with
    // `mfa_code` populated.
    let mut mfa_required = use_signal(|| false);
    let mut mfa_code = use_signal(String::new);
    // PMS-729 phase 2 H4 follow-up: recovery code alternative to TOTP.
    // Rendered next to the MFA input when `use_recovery = true`. Sent
    // as `recovery_code` (dropping `mfa_code`) so a customer with a
    // lost authenticator device can still sign in.
    let mut use_recovery = use_signal(|| false);
    let mut recovery_code = use_signal(String::new);
    // PMS-729 phase 2 H8: CAPTCHA challenge state. `captcha_site_key`
    // flips to Some(...) when the server returns a
    // 403 CAPTCHA_REQUIRED / CAPTCHA_INVALID envelope; the SPA lazy-
    // loads the Turnstile script and renders a widget. The widget's
    // JS callback pokes the token into `captcha_token` via a global
    // window function set up by `install_turnstile_callback`.
    let mut captcha_site_key = use_signal(|| None::<String>);
    let mut captcha_token = use_signal(String::new);
    // Install the Turnstile JS -> Rust bridge once per page mount. The
    // callback is registered on `window` so Cloudflare's `data-callback`
    // attribute can call into it; it writes back into our own signal so
    // the submit path picks up the token on the next click.
    #[cfg(feature = "web")]
    use_hook(move || install_turnstile_callback(captcha_token));
    // PMS-729: kick the shared `/portal/host` branding fetch. The result
    // lives in `PORTAL_HOST_HINT` so `PortalLayout` can paint the same
    // MSP name + logo after login without re-fetching. Idempotent - the
    // helper latches to a one-shot flag so both this page and the layout
    // can call it during a session without duplicating the request.
    #[cfg(feature = "web")]
    use_hook(crate::hooks::portal_branding::ensure_portal_branding_fetch);

    // PMS-729 phase 2 §6 slice 3: apply the portal-scoped theme on the
    // pre-auth page too so a customer who set Dark from inside the
    // portal sees the same base mode on their next return trip. Cheap
    // and idempotent; the media-listener registration is latched.
    #[cfg(feature = "web")]
    crate::hooks::portal_theme::use_apply_portal_theme();

    // PMS-729: read the shared branding hint. The reactive read here
    // triggers a re-render when the fetch flips it from `None` to
    // `Some(_)`, so the slug input hides and the branding block appears
    // without the page having to poll.
    let hint_snapshot = crate::hooks::portal_branding::use_portal_host_hint();

    // Session-expired banner (set by `refresh_portal_session` when the
    // token dies). Reactive read so the banner appears the moment the
    // background loop flips the flag, without a route change.
    let session_expired = crate::hooks::portal_auth::portal_session_expired_flag();

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let em = email.read().trim().to_string();
        let pw = password.read().clone();
        let code = mfa_code.read().trim().to_string();
        let rec = recovery_code.read().trim().to_string();
        let cap = captcha_token.read().trim().to_string();
        let is_mfa_stage = mfa_required();
        let recovery_mode = use_recovery();
        if em.is_empty() || pw.is_empty() {
            error.set("Enter your email and password.".to_string());
            return;
        }
        if is_mfa_stage && recovery_mode && rec.is_empty() {
            error.set("Paste a recovery code from the list your MSP gave you.".to_string());
            return;
        }
        if is_mfa_stage && !recovery_mode && code.is_empty() {
            error.set("Enter the 6-digit code from your authenticator app.".to_string());
            return;
        }
        // When the widget is up, refuse to submit without a token so the
        // customer sees "solve the CAPTCHA" instead of the server 403.
        let captcha_up = captcha_site_key.read().is_some();
        if captcha_up && cap.is_empty() {
            error.set("Please complete the CAPTCHA challenge above and try again.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = PortalLoginBody {
                    // Empty: server resolves the tenant from Host.
                    // `#[serde(skip_serializing_if = "String::is_empty")]`
                    // drops the field from the wire entirely.
                    tenant_slug: String::new(),
                    email: em.clone(),
                    password: pw.clone(),
                    // Only one of mfa_code / recovery_code goes on the
                    // wire per attempt, so the server side treats the
                    // request unambiguously.
                    mfa_code: if recovery_mode {
                        String::new()
                    } else {
                        code.clone()
                    },
                    recovery_code: if recovery_mode {
                        rec.clone()
                    } else {
                        String::new()
                    },
                    captcha_token: cap.clone(),
                };
                match crate::hooks::fetch::api::post_typed::<PortalLoginResp, _>(
                    "/portal/auth/login",
                    &body,
                )
                .await
                {
                    Ok(PortalLoginResp {
                        mfa_required: true, ..
                    }) => {
                        mfa_required.set(true);
                        mfa_code.set(String::new());
                        recovery_code.set(String::new());
                        // A successful password + captcha exchange consumed
                        // the token; clear + hide the widget so the MFA
                        // retry does not carry a stale token.
                        captcha_token.set(String::new());
                        captcha_site_key.set(None);
                    }
                    Ok(resp) => {
                        crate::hooks::fetch::api::set_portal_access_token(Some(resp.access_token));
                        crate::hooks::fetch::api::set_portal_refresh_token(Some(
                            resp.refresh_token,
                        ));
                        // Fresh session; drop the sticky "your last
                        // session expired" flag so returning to
                        // /portal/login later starts clean.
                        crate::hooks::portal_auth::clear_portal_session_expired();
                        nav.replace(Route::PortalHome {});
                    }
                    // PMS-729 phase 2 H8: 403 CAPTCHA_REQUIRED /
                    // CAPTCHA_INVALID. Envelope carries the Turnstile
                    // site_key; render the widget and prompt the
                    // customer to solve it. Second submit re-sends the
                    // form with captcha_token populated.
                    Err(ApiError::Status {
                        code: 403,
                        envelope_code,
                        envelope_body,
                        message,
                        ..
                    }) if envelope_code == "CAPTCHA_REQUIRED"
                        || envelope_code == "CAPTCHA_INVALID" =>
                    {
                        let site_key = envelope_body
                            .as_ref()
                            .and_then(|v| v.get("error"))
                            .and_then(|e| e.get("captcha"))
                            .and_then(|c| c.get("site_key"))
                            .and_then(|s| s.as_str())
                            .map(str::to_string)
                            .unwrap_or_default();
                        captcha_token.set(String::new());
                        if site_key.is_empty() {
                            // Server should always send a key with a
                            // CAPTCHA_REQUIRED envelope; if not, we
                            // cannot render the widget and the account
                            // is effectively locked out. Fall back to
                            // the server's own message so the customer
                            // sees an actionable error.
                            error.set(if message.is_empty() {
                                "The server asked for a CAPTCHA but did not provide one. Contact your account team.".to_string()
                            } else { message });
                        } else {
                            captcha_site_key.set(Some(site_key));
                            error.set(
                                "Too many sign-in attempts from this network. Solve the CAPTCHA below and try again.".to_string(),
                            );
                        }
                    }
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set("Invalid email or password.".to_string());
                    }
                    Err(ApiError::Status { code: 429, .. }) => {
                        error.set(
                            "Too many sign-in attempts. Please wait a moment and try again."
                                .to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (em, pw, code, rec, cap);
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen bg-app flex items-center justify-center px-4",
            div { class: "max-w-md w-full",
                div { class: "bg-surface rounded-lg shadow-lg p-8",
                    div { class: "text-center mb-6",
                        // PMS-729: MSP branding block above the credential
                        // fields. Rendered only when the /portal/host
                        // endpoint returned an active tenant; on legacy
                        // hosts, falls back to the generic title below.
                        if let Some(hint) = &hint_snapshot {
                            // PMS-729 phase 2 §6 slice 2: pick the theme-
                            // appropriate logo. `logo_for` falls back to
                            // the light logo when only one is supplied.
                            // Slice 3: read the portal-scoped theme so
                            // the login page tracks the customer's
                            // portal preference (independent of any
                            // agent-side theme on the same machine).
                            {
                                let is_dark = crate::hooks::portal_theme::current_is_dark();
                                rsx! {
                                    if let Some(url) = hint.branding.logo_for(is_dark) {
                                        img {
                                            src: "{url}",
                                            alt: "{hint.name}",
                                            class: "h-14 w-auto mx-auto mb-3",
                                        }
                                    }
                                }
                            }
                            h1 { class: "text-2xl font-semibold text-content",
                                "Sign in to {hint.name}"
                            }
                        } else {
                            h1 { class: "text-2xl font-semibold text-content",
                                "Sign in to the Client Portal"
                            }
                        }
                        // Prefer the tenant's welcome copy when set;
                        // otherwise fall back to the generic hint.
                        if let Some(msg) = hint_snapshot
                            .as_ref()
                            .and_then(|h| h.branding.welcome_message.as_deref())
                        {
                            p { class: "mt-2 text-sm text-content", "{msg}" }
                        } else {
                            p { class: "mt-2 text-sm text-content",
                                "Use the email your account team set up for you."
                            }
                        }
                    }

                    // Session-expired notice. Rendered above the form
                    // whenever the background refresh loop just landed
                    // a 401 (revoked / expired / replay-detected) so a
                    // customer bounced mid-work sees an explicit reason
                    // rather than an unexplained login prompt.
                    if session_expired {
                        div { class: "mb-4 rounded-md border border-amber-300 dark:border-amber-500/40 bg-amber-50 dark:bg-amber-500/10 p-3",
                            p { class: "text-sm text-amber-800 dark:text-amber-200",
                                role: "status",
                                "You were signed out because your session expired. Please sign in again to continue."
                            }
                        }
                    }

                    form {
                        class: "space-y-4",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_submit(());
                        },

                        if mfa_required() {
                            // PMS-729 phase 2 H4: MFA input stage. The
                            // credentials are already verified server-
                            // side (this is the second POST); the user
                            // types the 6-digit code from their
                            // authenticator app and re-submits. When
                            // `use_recovery` is on, the recovery-code
                            // input replaces the TOTP input; the
                            // submit picks the right field based on
                            // the same signal.
                            div {
                                if use_recovery() {
                                    p { class: "text-sm text-content mb-2",
                                        "Paste one of the recovery codes your MSP gave you when you enabled two-factor auth. Each code works only once."
                                    }
                                    Input {
                                        name: "recovery_code",
                                        label: "Recovery code",
                                        r#type: "text".to_string(),
                                        value: recovery_code(),
                                        required: true,
                                        disabled: saving(),
                                        oninput: move |e: FormEvent| {
                                            error.set(String::new());
                                            recovery_code.set(e.value());
                                        },
                                    }
                                } else {
                                    p { class: "text-sm text-content mb-2",
                                        "Enter the 6-digit code from your authenticator app to finish signing in."
                                    }
                                    Input {
                                        name: "mfa_code",
                                        label: "Authenticator code",
                                        r#type: "text".to_string(),
                                        value: mfa_code(),
                                        required: true,
                                        disabled: saving(),
                                        oninput: move |e: FormEvent| {
                                            error.set(String::new());
                                            mfa_code.set(e.value());
                                        },
                                    }
                                }
                                button {
                                    r#type: "button",
                                    class: "mt-2 text-xs text-accent hover:opacity-80",
                                    onclick: move |_| {
                                        let flip = !use_recovery();
                                        use_recovery.set(flip);
                                        error.set(String::new());
                                        if flip {
                                            mfa_code.set(String::new());
                                        } else {
                                            recovery_code.set(String::new());
                                        }
                                    },
                                    if use_recovery() {
                                        "Use my authenticator code instead"
                                    } else {
                                        "I lost my authenticator - use a recovery code"
                                    }
                                }
                            }
                        } else {
                            // Every deploy hosts a client's portal at a
                            // per-tenant subdomain (`{slug}.client.<apex>`
                            // in prod, `{slug}.client.localhost:PORT` in
                            // dev), so the server resolves the tenant
                            // from the Host header. The slug is never
                            // typed by a customer; a bare-host visit
                            // means the URL is wrong, not that they
                            // need to type an account name.
                            Input {
                                name: "email",
                                label: "Email",
                                r#type: "email".to_string(),
                                value: email(),
                                required: true,
                                disabled: saving(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    email.set(e.value());
                                },
                            }

                            Input {
                                name: "password",
                                label: "Password",
                                r#type: "password".to_string(),
                                value: password(),
                                required: true,
                                disabled: saving(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    password.set(e.value());
                                },
                            }
                        }

                        // PMS-729 phase 2 H8: Cloudflare Turnstile
                        // challenge. Rendered only when the server
                        // returned a CAPTCHA_REQUIRED / CAPTCHA_INVALID
                        // envelope; the Turnstile JS script auto-mounts
                        // into any element with class=cf-turnstile and
                        // calls `window.__portal_captcha_cb` (installed
                        // in `install_turnstile_callback`) with the
                        // token. Loading the script lazily keeps a
                        // customer whose IP never crossed the threshold
                        // from paying the extra request.
                        if let Some(site_key) = captcha_site_key.read().as_ref() {
                            div { class: "mt-4",
                                p { class: "text-xs text-muted mb-2",
                                    "For your security, please verify you are not a bot."
                                }
                                div {
                                    class: "cf-turnstile",
                                    "data-sitekey": "{site_key}",
                                    "data-callback": "__portal_captcha_cb",
                                }
                                {
                                    #[cfg(feature = "web")]
                                    ensure_turnstile_script_loaded();
                                    rsx! {}
                                }
                            }
                        }

                        if !error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", role: "alert", "{error}" }
                        }

                        div { class: "pt-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: saving(),
                                loading: saving(),
                                r#type: "submit".to_string(),
                                class: "w-full".to_string(),
                                if mfa_required() { "Verify code" } else { "Sign in" }
                            }
                        }

                        // PMS-729 phase 2 H3: link to the forgot-password page.
                        div { class: "text-center pt-2",
                            Link {
                                to: Route::PortalForgotPassword {},
                                class: "text-sm text-accent hover:opacity-80",
                                "Forgot your password?"
                            }
                        }

                        // PMS-729 phase 2 §6 slice 2: support contact
                        // block. Only rendered when the tenant filled in
                        // at least one of email / phone / hours.
                        if let Some(hint) = &hint_snapshot {
                            if hint.branding.support_email.is_some()
                                || hint.branding.support_phone.is_some()
                                || hint.branding.support_hours.is_some()
                            {
                                div { class: "mt-4 border-t border-line pt-3 text-center text-xs text-subtle space-y-0.5",
                                    p { "Need help signing in?" }
                                    if let Some(mail) = &hint.branding.support_email {
                                        p {
                                            a {
                                                class: "text-accent hover:opacity-80",
                                                href: "mailto:{mail}",
                                                "{mail}"
                                            }
                                        }
                                    }
                                    if let Some(phone) = &hint.branding.support_phone {
                                        p {
                                            a {
                                                class: "text-accent hover:opacity-80",
                                                href: "tel:{phone}",
                                                "{phone}"
                                            }
                                        }
                                    }
                                    if let Some(hours) = &hint.branding.support_hours {
                                        p { "{hours}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// PMS-729 phase 2 H8: bind the token signal to the JS callback
/// Cloudflare Turnstile invokes when the widget resolves. Turnstile
/// posts the token as the first argument; a global function is the
/// only integration seam its `data-callback` attribute exposes. The
/// callback stashes the token into a WASM-side JS bag that this hook
/// polls at 400ms on each render. Fires once per page mount via
/// `use_hook` so a re-render does not re-register.
#[cfg(feature = "web")]
fn install_turnstile_callback(mut captcha_token: Signal<String>) {
    use wasm_bindgen::prelude::Closure;
    use wasm_bindgen::JsCast;

    let Some(win) = web_sys::window() else {
        return;
    };
    // Idempotent: only install once per page load. Dioxus signals are
    // `Copy`; a re-invocation would leak a second closure and orphan
    // the first, wasting memory but not breaking correctness. Skip
    // the second install.
    if js_sys::Reflect::get(&win, &"__portal_captcha_cb".into())
        .map(|v| !v.is_undefined() && !v.is_null())
        .unwrap_or(false)
    {
        return;
    }
    // Turnstile's data-callback expects a single-arg fn(token). We use
    // `FnMut` because Dioxus's `Signal::set` takes `&mut self`; wrap in
    // `Closure::wrap` (which accepts FnMut) instead of `Closure::new`
    // (which requires Fn).
    let cb = Closure::wrap(Box::new(move |token: wasm_bindgen::JsValue| {
        if let Some(s) = token.as_string() {
            captcha_token.set(s);
        }
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    let _ = js_sys::Reflect::set(
        &win,
        &"__portal_captcha_cb".into(),
        cb.as_ref().unchecked_ref(),
    );
    // Leak the closure so it stays alive for the lifetime of the page.
    // The alternative (storing it in a signal) would drop it on the
    // component's own re-render / unmount and break the callback the
    // second the customer navigates.
    cb.forget();
}

/// Lazily append the Turnstile script into `<head>` the first time a
/// challenge is displayed. Cloudflare's own script scans the DOM for
/// `class=cf-turnstile` and mounts a widget in each, so nothing else
/// is required from the SPA. Idempotent: guards on a data-attribute
/// tag so a subsequent render does not add duplicate `<script>` tags.
#[cfg(feature = "web")]
fn ensure_turnstile_script_loaded() {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    if doc
        .query_selector("script[data-portal-turnstile='1']")
        .ok()
        .flatten()
        .is_some()
    {
        return;
    }
    let Some(head) = doc.head() else {
        return;
    };
    let Ok(el) = doc.create_element("script") else {
        return;
    };
    let _ = el.set_attribute(
        "src",
        "https://challenges.cloudflare.com/turnstile/v0/api.js",
    );
    let _ = el.set_attribute("async", "true");
    let _ = el.set_attribute("defer", "true");
    let _ = el.set_attribute("data-portal-turnstile", "1");
    let _ = head.append_child(&el);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(mfa_code: &str, recovery_code: &str, captcha_token: &str) -> PortalLoginBody {
        PortalLoginBody {
            tenant_slug: "acme".to_string(),
            email: "contact@example.com".to_string(),
            password: "pw".to_string(),
            mfa_code: mfa_code.to_string(),
            recovery_code: recovery_code.to_string(),
            captcha_token: captcha_token.to_string(),
        }
    }

    #[test]
    fn body_matches_the_server_request_shape() {
        let json = serde_json::to_string(&body("", "", "")).expect("serializes");
        assert!(json.contains(r#""tenant_slug":"acme""#), "{json}");
        assert!(json.contains(r#""email":"contact@example.com""#), "{json}");
        assert!(json.contains(r#""password":"pw""#), "{json}");
        // Empty optional fields drop from the wire so the first-attempt
        // POST looks identical to the pre-H4 shape.
        assert!(!json.contains("mfa_code"), "empty mfa_code dropped: {json}");
        assert!(
            !json.contains("recovery_code"),
            "empty recovery_code dropped: {json}"
        );
        assert!(
            !json.contains("captcha_token"),
            "empty captcha_token dropped: {json}"
        );
    }

    #[test]
    fn body_includes_mfa_code_when_supplied() {
        let json = serde_json::to_string(&body("123456", "", "")).expect("serializes");
        assert!(json.contains(r#""mfa_code":"123456""#), "{json}");
    }

    #[test]
    fn body_carries_recovery_and_captcha_when_supplied() {
        let json = serde_json::to_string(&body("", "ABCDE-12345", "tk_test")).expect("serializes");
        assert!(json.contains(r#""recovery_code":"ABCDE-12345""#), "{json}");
        assert!(json.contains(r#""captcha_token":"tk_test""#), "{json}");
    }

    #[test]
    fn decodes_the_login_response() {
        // PMS-729 phase 2 H2: response now carries refresh_token +
        // refresh_expires_at alongside the access_token. `contact` +
        // `expires_at` are still there but this page ignores them.
        let resp: PortalLoginResp = serde_json::from_str(
            r#"{
                "access_token":"portal.jwt",
                "expires_at":"2026-08-01T00:00:00Z",
                "refresh_token":"00000000-0000-0000-0000-000000000042.some-secret",
                "refresh_expires_at":"2026-08-31T00:00:00Z",
                "contact":{"id":"00000000-0000-0000-0000-000000000000"}
            }"#,
        )
        .expect("portal login response decodes");
        assert_eq!(resp.access_token, "portal.jwt");
        assert!(!resp.refresh_token.is_empty());
    }
}
