//! Where to send a contact after they sign in (MAPPS-761).
//!
//! A portal link arrives by email pointing at a login page, because the page
//! it really wants is behind a session the customer does not have yet. PMS-1168
//! is the shape: the invoice mail links the contact login, and the invoice
//! itself is at `/invoices/{id}`, so without this the customer signs in and
//! lands on the dashboard holding an email about one specific invoice.
//!
//! So a link may carry `?next=<path>`, and this module is the whole rule for
//! what happens to it.
//!
//! ## Why it is remembered rather than threaded through the routes
//!
//! Signing in is not one hop. A customer can land on the generic login, be
//! moved to the Portal-ID login, ask for a magic link, pick a company, or come
//! back through set-password or reset-password - and each of those replaces
//! the URL, which would drop a query parameter every time. Adding a `next`
//! field to six route variants would mean every one of the 36 construction
//! sites naming it, and every internal hop remembering to pass it on; the one
//! that forgot would fail silently, which is this issue over again.
//!
//! `sessionStorage` instead: written once when the customer arrives, read once
//! when they are signed in, and gone when the tab closes. It is consumed on
//! read so a target cannot outlive the sign-in it was for and redirect some
//! later, unrelated login in the same tab.
//!
//! ## What is accepted
//!
//! The value has to name a route THIS BUILD SERVES. That is a stronger gate
//! than any string check and it closes two holes at once: an absolute URL can
//! never parse as a route, so the link cannot be turned into an open redirect
//! wearing the MSP's domain; and a path that no longer exists is refused here
//! rather than becoming the 404 the customer was sent to (PMS-1168 again).
//!
//! The value is deliberately NOT percent-decoded. A path this links to is
//! plain ASCII (`/invoices/{uuid}`), and leaving escapes alone means an
//! attacker's `%2F%2Fevil.example` fails the leading-slash check instead of
//! becoming `//evil.example` after a decode nobody re-checked.

use std::str::FromStr;

use crate::Route;

/// Where the pending target lives between the arrival and the sign-in.
const STORAGE_KEY: &str = "mokosh.portal.next";

/// A path longer than this is not a route this app serves, and refusing it
/// early keeps anything odd out of storage.
const MAX_LEN: usize = 512;

/// The route `raw` names, or `None` if it names anything else.
///
/// Pure, so the rules are testable without a browser.
pub(crate) fn sanitize(raw: &str) -> Option<Route> {
    let value = raw.trim();
    if value.is_empty() || value.len() > MAX_LEN {
        return None;
    }
    // Same-origin paths only. `//host` is protocol-relative and would leave
    // the origin; a backslash is normalised to a slash by browsers, so
    // `/\evil.example` is the same trick spelled differently.
    if !value.starts_with('/') || value.starts_with("//") || value.contains('\\') {
        return None;
    }
    if value.chars().any(|c| c.is_control()) {
        return None;
    }
    // An absolute URL cannot reach here (it fails the leading slash), but a
    // scheme-bearing value is worth refusing by name rather than by accident.
    if crate::utils::url::scheme_of(value).is_some() {
        return None;
    }
    match Route::from_str(value) {
        // The catch-all swallows every unmatched path, so it has to be
        // refused explicitly or "does this route exist" always answers yes.
        Ok(Route::NotFound { .. }) => None,
        Ok(route) => Some(route),
        Err(_) => None,
    }
}

/// Remember the `?next=` on the current URL, if it carries a usable one.
///
/// Called when a contact lands on a portal page. A value that does not
/// sanitize is dropped silently: the customer still signs in and still gets
/// the dashboard, which is what they got before this existed.
pub(crate) fn remember_from_query() {
    let Some(raw) = crate::utils::url::current_query_param("next") else {
        return;
    };
    if sanitize(&raw).is_none() {
        tracing::warn!("portal next target refused; signing in will land on the dashboard");
        return;
    }
    if let Ok(store) = crate::platform::store::session() {
        // The RAW value is stored and re-sanitized on the way out, so storage
        // is never trusted as a source of routes.
        let _ = store.set_item(STORAGE_KEY, &raw);
    }
}

/// The remembered route, consumed.
///
/// `None` when nothing was remembered, when storage is unavailable, or when
/// what was stored no longer names a route - a build can ship between the
/// click and the sign-in.
pub(crate) fn take() -> Option<Route> {
    let store = crate::platform::store::session().ok()?;
    let raw = store.get_item(STORAGE_KEY).ok().flatten();
    // Removed whether or not it is usable, so a bad value cannot sit in the
    // tab affecting later sign-ins.
    let _ = store.remove_item(STORAGE_KEY);
    sanitize(&raw?)
}

/// Where a signed-in contact should land: the remembered target, else the
/// dashboard, which is where every one of these landed before MAPPS-761.
pub(crate) fn landing() -> Route {
    take().unwrap_or(Route::Dashboard {})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case the issue exists for.
    #[test]
    fn an_app_path_is_accepted() {
        let target = sanitize("/invoices/2f1c2f1e-0000-4000-8000-00000000abcd");
        assert!(
            matches!(target, Some(Route::InvoiceDetail { .. })),
            "{target:?}"
        );
    }

    /// The value arrives in a URL sent by email and is followed AFTER the
    /// customer authenticates, so anything that could leave the origin turns
    /// the MSP's own domain into a phishing hop.
    #[test]
    fn nothing_that_could_leave_the_origin_is_accepted() {
        for hostile in [
            "//evil.example",
            "///evil.example",
            "https://evil.example",
            "http://evil.example/invoices/1",
            "javascript:alert(1)",
            "java\tscript:alert(1)",
            "/\\evil.example",
            "\\\\evil.example",
            "evil.example",
            "invoices/1",
        ] {
            assert!(sanitize(hostile).is_none(), "accepted {hostile:?}");
        }
    }

    /// An escaped separator stays escaped. Decoding and then re-checking is
    /// the step everyone forgets, so this never decodes at all.
    #[test]
    fn percent_escapes_are_not_decoded_into_an_escape_hatch() {
        for encoded in [
            "%2F%2Fevil.example",
            "/%2e%2e/%2e%2e/admin",
            "%2Finvoices%2F1",
        ] {
            assert!(sanitize(encoded).is_none(), "accepted {encoded:?}");
        }
    }

    /// A path this build does not serve is refused here rather than becoming
    /// the 404 the customer was sent to. `/portal/invoices/{id}` is the exact
    /// path PMS-1168 was emailing.
    #[test]
    fn a_path_that_names_no_route_is_refused() {
        assert!(sanitize("/portal/invoices/2f1c2f1e-0000-4000-8000-00000000abcd").is_none());
        assert!(sanitize("/this-page-does-not-exist").is_none());
    }

    /// Every contact-plane sign-in goes through [`landing`].
    ///
    /// A fourth landing that navigated straight to the dashboard would drop
    /// the target silently for whichever path it served, and silence is the
    /// failure mode this whole module exists to remove. Source scan, because
    /// what has to hold is "nobody does it the other way", which no
    /// behavioural test over three call sites can say about a fourth.
    #[test]
    fn no_contact_landing_navigates_to_the_dashboard_directly() {
        const PAGES: &[(&str, &str)] = &[
            ("login.rs", include_str!("login.rs")),
            ("picker.rs", include_str!("picker.rs")),
            ("portal_id_login.rs", include_str!("portal_id_login.rs")),
            ("generic_login.rs", include_str!("generic_login.rs")),
            ("magic_link_login.rs", include_str!("magic_link_login.rs")),
            ("set_password.rs", include_str!("set_password.rs")),
            ("reset_password.rs", include_str!("reset_password.rs")),
        ];
        for (name, source) in PAGES {
            assert!(
                !source.contains("Route::Dashboard"),
                "{name} navigates to the dashboard itself; it should call next_target::landing()"
            );
        }
    }

    /// Empty, blank and absurd values are not routes.
    #[test]
    fn empty_and_oversized_values_are_refused() {
        assert!(sanitize("").is_none());
        assert!(sanitize("   ").is_none());
        assert!(sanitize(&format!("/{}", "a".repeat(MAX_LEN))).is_none());
    }

    /// A control character INSIDE the value is a value trying to be read two
    /// ways: browsers strip them before a URL resolves, so `/inv\0oices/1`
    /// and `/invoices/1` are the same request to a browser and different
    /// strings to a check. Surrounding whitespace is a different thing and is
    /// trimmed, the way `query_param_in` already trims it, so a trailing
    /// newline yields the route rather than a refusal.
    #[test]
    fn embedded_control_characters_are_refused_and_surrounding_space_is_trimmed() {
        assert!(sanitize("/inv\u{0}oices/1").is_none());
        assert!(sanitize("/invoices/1\u{7}").is_none());
        let trimmed = sanitize("  /invoices/2f1c2f1e-0000-4000-8000-00000000abcd\n");
        assert!(
            matches!(trimmed, Some(Route::InvoiceDetail { .. })),
            "{trimmed:?}"
        );
    }
}
