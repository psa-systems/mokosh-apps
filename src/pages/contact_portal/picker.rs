//! MAPPS-572 (mokosh-contact-login prompt 010): magic-link redemption
//! + multi-Company picker.
//!
//! Mounted at `/portal/pick?:token` (MAPPS-560 query-segment shape).
//! Public (no `AuthGuard`). On mount POSTs `/contact/auth/login-link/
//! redeem { token }`; the response is a
//! [`LoginLinkRedeemOutcomeWire`] that steers the render branch:
//!
//! - `auto: Some(resp)` with `mfa_required == false`: install the
//!   session (mirrors prompt 005 ContactLoginPage) and navigate to
//!   `/dashboard`.
//! - `auto: Some(resp)` with `mfa_required == true`: render an inline
//!   TOTP input. Submit reposts `/contact/auth/login-link/redeem` with
//!   `{ token, mfa_code }` (see [`REDEEM_MFA_ENDPOINT`] const below;
//!   PMS-918's spec suggests the MFA path reuses the redeem endpoint
//!   with a `mfa_code` body field. If the server ships a dedicated
//!   `/verify-mfa` endpoint instead, flip the const at the top of the
//!   file; nothing else changes).
//! - `candidates: Some(cands)`: one tile per Company. Click posts
//!   `/contact/auth/login-link/select { selection_token, contact_id }`
//!   and, on success, installs the session + navigates. An
//!   `mfa_required` response on select prompts a TOTP inline before
//!   completing.
//! - Anything else (both `auto` and `candidates` are `None`, or the
//!   candidates list is empty): renders "This link is invalid or has
//!   expired" with a "Request a new sign-in link" button that hops
//!   back to `/portal/login`.
//!
//! Both endpoints on 400 (invalid / expired / replayed / revoked)
//! render the invalid-link branch above. Per PMS-918 the server
//! returns a single opaque copy for any of those reasons so the SPA
//! does not need to distinguish.

use dioxus::prelude::*;
use dioxus::router::Navigator;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

/// PMS-918 spec: the magic-link redeem path reuses the same
/// `/contact/auth/login-link/redeem` endpoint for the MFA step, with
/// the second post carrying `{ token, mfa_code }`. If the server-side
/// implementation ships a dedicated verify-mfa endpoint instead, flip
/// this to `"/contact/auth/login-link/verify-mfa"` and nothing else in
/// this file changes.
const REDEEM_MFA_ENDPOINT: &str = "/contact/auth/login-link/redeem";

/// Same rule applies to the select MFA follow-up: PMS-918 documents it
/// as a re-post to `/select` with `{ selection_token, contact_id,
/// mfa_code }`. If a distinct `select/verify-mfa` shape lands, flip
/// this const.
const SELECT_MFA_ENDPOINT: &str = "/contact/auth/login-link/select";

// ============================================================
// Wire types (must match PMS-918 server DTOs).
// ============================================================

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LoginLinkRedeemOutcomeWire {
    #[serde(default)]
    pub auto: Option<ContactLoginResponseWire>,
    #[serde(default)]
    pub candidates: Option<LoginLinkCandidatesWire>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ContactLoginResponseWire {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub mfa_required: bool,
    #[serde(default)]
    pub contact: Option<ContactSnippetWire>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ContactSnippetWire {
    #[serde(default)]
    pub caps: Vec<String>,
    /// portal_slug of the Company this session is for. Persisted via
    /// `set_contact_last_slug` on install so an expired-session bounce
    /// remembers where to send the visitor.
    #[serde(default)]
    pub portal_slug: String,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LoginLinkCandidatesWire {
    #[serde(default)]
    pub selection_token: String,
    #[serde(default)]
    pub companies: Vec<LoginLinkCandidateWire>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct LoginLinkCandidateWire {
    pub contact_id: Uuid,
    #[serde(default)]
    pub company_name: String,
    #[serde(default)]
    pub portal_slug: String,
}

#[derive(Serialize)]
struct RedeemBody {
    token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_code: Option<String>,
}

#[derive(Serialize)]
struct SelectBody {
    selection_token: String,
    contact_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_code: Option<String>,
}

// ============================================================
// Pure branch classifier + tests.
// ============================================================

/// Render branches the picker page can land on after a redeem call.
/// Kept pure so a unit test can exercise the four cases without a
/// browser (per prompt 005's rule: no `web_sys` in tests, they panic
/// on native).
#[derive(Debug, PartialEq, Eq)]
pub enum RedeemBranch {
    /// Single-match, no MFA: install the session and navigate.
    InstallSession,
    /// Single-match, MFA required: render TOTP input.
    PromptMfa,
    /// Multi-match with a non-empty candidate list: render the picker.
    ShowPicker,
    /// Everything else (both `None`, or an empty candidate list, or
    /// nonsense combinations): invalid-link fallback.
    InvalidLink,
}

/// Classify a redeem outcome into a render branch. Pure function; the
/// bulky wire types are collapsed to the four cases the page cares
/// about, so `#[cfg(test)]` can exercise every branch without a
/// running Dioxus tree.
pub fn classify_redeem(outcome: &LoginLinkRedeemOutcomeWire) -> RedeemBranch {
    if let Some(resp) = &outcome.auto {
        if resp.mfa_required {
            return RedeemBranch::PromptMfa;
        }
        return RedeemBranch::InstallSession;
    }
    if let Some(cands) = &outcome.candidates {
        if !cands.companies.is_empty() && !cands.selection_token.is_empty() {
            return RedeemBranch::ShowPicker;
        }
    }
    RedeemBranch::InvalidLink
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(mfa: bool) -> ContactLoginResponseWire {
        ContactLoginResponseWire {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            mfa_required: mfa,
            contact: None,
        }
    }

    fn cand() -> LoginLinkCandidateWire {
        LoginLinkCandidateWire {
            contact_id: Uuid::new_v4(),
            company_name: "Acme".into(),
            portal_slug: "acme".into(),
        }
    }

    #[test]
    fn install_session_branch() {
        let outcome = LoginLinkRedeemOutcomeWire {
            auto: Some(resp(false)),
            candidates: None,
        };
        assert_eq!(classify_redeem(&outcome), RedeemBranch::InstallSession);
    }

    #[test]
    fn prompt_mfa_branch() {
        let outcome = LoginLinkRedeemOutcomeWire {
            auto: Some(resp(true)),
            candidates: None,
        };
        assert_eq!(classify_redeem(&outcome), RedeemBranch::PromptMfa);
    }

    #[test]
    fn show_picker_branch() {
        let outcome = LoginLinkRedeemOutcomeWire {
            auto: None,
            candidates: Some(LoginLinkCandidatesWire {
                selection_token: "tok".into(),
                companies: vec![cand(), cand()],
            }),
        };
        assert_eq!(classify_redeem(&outcome), RedeemBranch::ShowPicker);
    }

    #[test]
    fn invalid_link_when_both_none() {
        let outcome = LoginLinkRedeemOutcomeWire::default();
        assert_eq!(classify_redeem(&outcome), RedeemBranch::InvalidLink);
    }

    #[test]
    fn invalid_link_when_candidates_empty() {
        let outcome = LoginLinkRedeemOutcomeWire {
            auto: None,
            candidates: Some(LoginLinkCandidatesWire {
                selection_token: "tok".into(),
                companies: vec![],
            }),
        };
        assert_eq!(classify_redeem(&outcome), RedeemBranch::InvalidLink);
    }
}

// ============================================================
// Component.
// ============================================================

/// Local UI state for the picker page. Kept as an enum so a mid-flow
/// branch (e.g. auto -> MFA prompt) does not have to null out signals
/// from an unrelated branch.
#[derive(Clone, Debug, PartialEq)]
enum PickerState {
    /// The initial `/redeem` request is in flight.
    Loading,
    /// Single-match, MFA needed. The token is carried through so the
    /// TOTP submit can re-post the same redeem call.
    MfaAuto,
    /// Multi-match: render the picker.
    Picker(LoginLinkCandidatesWire),
    /// Selected a candidate whose contact has MFA. The selection_token
    /// + contact_id are carried through for the TOTP submit.
    MfaSelect {
        selection_token: String,
        contact_id: Uuid,
    },
    /// Invalid / expired / revoked / any 4xx from either endpoint.
    Invalid,
}

#[component]
pub fn ContactPickerPage(token: String) -> Element {
    let nav = use_navigator();
    let mut state = use_signal(|| PickerState::Loading);
    let mut mfa_code = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut submitting = use_signal(|| false);

    // Fire the initial redeem on mount.
    let token_for_effect = token.clone();
    use_effect(move || {
        let tok = token_for_effect.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match post_redeem(&tok, None).await {
                    Ok(outcome) => match classify_redeem(&outcome) {
                        RedeemBranch::InstallSession => {
                            if let Some(resp) = outcome.auto.as_ref() {
                                install_session_and_go(&nav, resp);
                            } else {
                                state.set(PickerState::Invalid);
                            }
                        }
                        RedeemBranch::PromptMfa => {
                            state.set(PickerState::MfaAuto);
                        }
                        RedeemBranch::ShowPicker => {
                            if let Some(cands) = outcome.candidates {
                                state.set(PickerState::Picker(cands));
                            } else {
                                state.set(PickerState::Invalid);
                            }
                        }
                        RedeemBranch::InvalidLink => {
                            state.set(PickerState::Invalid);
                        }
                    },
                    Err(()) => {
                        state.set(PickerState::Invalid);
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = tok;
                state.set(PickerState::Invalid);
            }
        });
    });

    // Submit the MFA code for the single-match auto path.
    let token_for_mfa = token.clone();
    let mut submit_mfa_auto = move |_| {
        if submitting() {
            return;
        }
        let code = mfa_code.read().trim().to_string();
        if code.is_empty() {
            error.set("Enter the 6-digit code from your authenticator app.".to_string());
            return;
        }
        let tok = token_for_mfa.clone();
        submitting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match post_redeem(&tok, Some(code)).await {
                    Ok(outcome) => {
                        if let Some(resp) = outcome.auto.as_ref() {
                            if resp.mfa_required {
                                error.set("Incorrect code. Try again.".to_string());
                            } else {
                                install_session_and_go(&nav, resp);
                            }
                        } else {
                            state.set(PickerState::Invalid);
                        }
                    }
                    Err(()) => {
                        state.set(PickerState::Invalid);
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (tok, code);
            }
            submitting.set(false);
        });
    };

    // Submit an MFA code from the picker's post-select prompt.
    let mut submit_mfa_select = move |sel_tok: String, cid: Uuid| {
        if submitting() {
            return;
        }
        let code = mfa_code.read().trim().to_string();
        if code.is_empty() {
            error.set("Enter the 6-digit code from your authenticator app.".to_string());
            return;
        }
        submitting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match post_select(&sel_tok, cid, Some(code)).await {
                    Ok(resp) => {
                        if resp.mfa_required {
                            error.set("Incorrect code. Try again.".to_string());
                        } else {
                            install_session_and_go(&nav, &resp);
                        }
                    }
                    Err(()) => {
                        state.set(PickerState::Invalid);
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (sel_tok, cid, code);
            }
            submitting.set(false);
        });
    };

    // Click handler for one candidate tile.
    let mut pick_candidate = move |sel_tok: String, cid: Uuid| {
        if submitting() {
            return;
        }
        submitting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match post_select(&sel_tok, cid, None).await {
                    Ok(resp) => {
                        if resp.mfa_required {
                            state.set(PickerState::MfaSelect {
                                selection_token: sel_tok,
                                contact_id: cid,
                            });
                        } else {
                            install_session_and_go(&nav, &resp);
                        }
                    }
                    Err(()) => {
                        state.set(PickerState::Invalid);
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (sel_tok, cid);
            }
            submitting.set(false);
        });
    };

    let snap = state.read().clone();
    rsx! {
        AuthLayout {
            match snap {
                PickerState::Loading => rsx! {
                    div { class: "text-center py-8",
                        p { class: "text-sm text-content", "Signing you in…" }
                    }
                },
                PickerState::MfaAuto => rsx! {
                    div { class: "text-center mb-6",
                        h1 { class: "text-2xl font-semibold text-content", "Two-factor code" }
                        p { class: "mt-2 text-sm text-content",
                            "Enter the 6-digit code from your authenticator app."
                        }
                    }
                    form {
                        class: "space-y-4",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            submit_mfa_auto(());
                        },
                        Input {
                            name: "mfa_code",
                            label: "Authentication code",
                            r#type: "text".to_string(),
                            value: mfa_code(),
                            required: true,
                            disabled: submitting(),
                            oninput: move |e: FormEvent| {
                                error.set(String::new());
                                mfa_code.set(e.value());
                            },
                        }
                        if !error().is_empty() {
                            p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                        }
                        div { class: "pt-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: submitting(),
                                loading: submitting(),
                                r#type: "submit".to_string(),
                                class: "w-full".to_string(),
                                "Verify"
                            }
                        }
                    }
                },
                PickerState::Picker(cands) => {
                    let sel_tok = cands.selection_token.clone();
                    rsx! {
                        div { class: "text-center mb-6",
                            h1 { class: "text-2xl font-semibold text-content", "Choose a Company" }
                            p { class: "mt-2 text-sm text-content",
                                "This email has portal access to more than one Company."
                            }
                        }
                        div { class: "space-y-2",
                            for cand in cands.companies.iter().cloned() {
                                {
                                    let sel_tok = sel_tok.clone();
                                    let cid = cand.contact_id;
                                    let name = if cand.company_name.trim().is_empty() {
                                        cand.portal_slug.clone()
                                    } else {
                                        cand.company_name.clone()
                                    };
                                    let slug = cand.portal_slug.clone();
                                    rsx! {
                                        button {
                                            key: "{cid}",
                                            r#type: "button",
                                            class: "w-full text-left rounded-md border border-line bg-surface hover:bg-surface-2 px-4 py-3 transition-colors disabled:opacity-60",
                                            disabled: submitting(),
                                            onclick: move |_| {
                                                pick_candidate(sel_tok.clone(), cid);
                                            },
                                            div { class: "text-sm font-medium text-content",
                                                "{name}"
                                            }
                                            if !slug.is_empty() {
                                                div { class: "text-xs text-muted mt-0.5",
                                                    "/portal/{slug}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !error().is_empty() {
                            p { role: "alert", class: "mt-4 text-sm text-red-600 dark:text-red-400", "{error}" }
                        }
                    }
                },
                PickerState::MfaSelect { selection_token, contact_id } => {
                    let sel_tok = selection_token.clone();
                    rsx! {
                        div { class: "text-center mb-6",
                            h1 { class: "text-2xl font-semibold text-content", "Two-factor code" }
                            p { class: "mt-2 text-sm text-content",
                                "Enter the 6-digit code from your authenticator app."
                            }
                        }
                        form {
                            class: "space-y-4",
                            onsubmit: move |evt: Event<FormData>| {
                                evt.prevent_default();
                                submit_mfa_select(sel_tok.clone(), contact_id);
                            },
                            Input {
                                name: "mfa_code",
                                label: "Authentication code",
                                r#type: "text".to_string(),
                                value: mfa_code(),
                                required: true,
                                disabled: submitting(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    mfa_code.set(e.value());
                                },
                            }
                            if !error().is_empty() {
                                p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                            }
                            div { class: "pt-2",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    disabled: submitting(),
                                    loading: submitting(),
                                    r#type: "submit".to_string(),
                                    class: "w-full".to_string(),
                                    "Verify"
                                }
                            }
                        }
                    }
                },
                PickerState::Invalid => rsx! {
                    div { class: "text-center mb-6",
                        h1 { class: "text-2xl font-semibold text-content", "Link expired" }
                        p { class: "mt-2 text-sm text-content",
                            "This link is invalid or has expired."
                        }
                    }
                    div { class: "pt-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            r#type: "button".to_string(),
                            class: "w-full".to_string(),
                            onclick: move |_| {
                                // We do not know the visitor's email at
                                // this point (the redeem token carries
                                // it opaquely server-side), so nothing
                                // to pre-fill via `?email=`. Send them
                                // to the empty finder.
                                nav.replace(Route::ContactMagicLinkLogin { email: String::new() });
                            },
                            "Request a new sign-in link"
                        }
                    }
                },
            }
        }
    }
}

// ============================================================
// Web-only helpers (kept out of the pure test surface).
// ============================================================

/// Web-only wrapper around the initial or MFA-follow-up redeem POST.
/// `Err(())` means "treat as invalid link" (any 4xx / network error).
/// Kept opaque so the picker page does not have to know about
/// [`ApiError`] shape.
#[cfg(feature = "web")]
async fn post_redeem(
    token: &str,
    mfa_code: Option<String>,
) -> Result<LoginLinkRedeemOutcomeWire, ()> {
    let body = RedeemBody {
        token: token.to_string(),
        mfa_code: mfa_code.clone(),
    };
    let path = if mfa_code.is_some() {
        REDEEM_MFA_ENDPOINT
    } else {
        "/contact/auth/login-link/redeem"
    };
    crate::hooks::fetch::api::post_typed::<LoginLinkRedeemOutcomeWire, _>(path, &body)
        .await
        .map_err(|_| ())
}

/// Web-only wrapper around the select POST. Same `Err(())` posture as
/// [`post_redeem`].
#[cfg(feature = "web")]
async fn post_select(
    selection_token: &str,
    contact_id: Uuid,
    mfa_code: Option<String>,
) -> Result<ContactLoginResponseWire, ()> {
    let body = SelectBody {
        selection_token: selection_token.to_string(),
        contact_id,
        mfa_code: mfa_code.clone(),
    };
    let path = if mfa_code.is_some() {
        SELECT_MFA_ENDPOINT
    } else {
        "/contact/auth/login-link/select"
    };
    crate::hooks::fetch::api::post_typed::<ContactLoginResponseWire, _>(path, &body)
        .await
        .map_err(|_| ())
}

/// Install the session tokens + capabilities from a
/// [`ContactLoginResponseWire`] and hop to `/dashboard`. Mirrors the
/// exact install pattern in `src/pages/contact_portal/login.rs` from
/// prompt 005 so any future change to session install lands in one
/// place. `nav` is passed in because `use_navigator()` is a hook and
/// cannot be re-called from a spawned async block; the caller
/// captures the navigator once at render time and hands it here.
#[cfg(feature = "web")]
fn install_session_and_go(nav: &Navigator, resp: &ContactLoginResponseWire) {
    let caps = resp
        .contact
        .as_ref()
        .map(|c| c.caps.clone())
        .unwrap_or_default();
    let slug = resp
        .contact
        .as_ref()
        .map(|c| c.portal_slug.clone())
        .unwrap_or_default();
    crate::hooks::fetch::api::set_contact_access_token(Some(resp.access_token.clone()));
    crate::hooks::fetch::api::set_contact_refresh_token(Some(resp.refresh_token.clone()));
    if !slug.is_empty() {
        crate::hooks::fetch::api::set_contact_last_slug(&slug);
    }
    crate::hooks::capabilities::set_contact_capabilities(Some(caps));
    nav.replace(Route::Dashboard {});
}
