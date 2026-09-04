//! Mokosh Platform Library
//!
//! This library provides the core functionality for the Mokosh platform.

use dioxus::prelude::*;

pub mod branding;
pub mod components;
pub mod hooks;
pub mod modules;
pub mod pages;
// MAPPS-504: everything the app needs from its host, declared once and
// implemented per target. Browser bindings are reachable only from here.
pub mod platform;
pub mod utils;

pub use modules::auth::CurrentUser;
pub use utils::error::{AppError, AppResult};

// MAPPS-366: the persistent app shell, referenced by the `#[layout(AppShell)]`
// attribute inside the `Route` enum below, so it must be in scope here (the
// `Routable` derive expands the attribute at the enum site).
use components::AppShell;

/// MAPPS-623: return true when the given URL pathname is a surface a
/// portal contact should NEVER reach. The sidebar already hides
/// these NavItems (each is gated on `use_capability(STAFF_ONLY)` in
/// `components/layout.rs`), but the routes themselves stayed
/// unrestricted at the router level, so a contact who typed the URL
/// or followed a stale bookmark could still hit e.g. `/companies`
/// and see the SPA render a broken page whose fetches then 401. The
/// AuthGuard's contact-plane branch consults this helper on every
/// render and redirects any hit to `/dashboard`.
///
/// Denies by prefix on the pathname (the router's compiled `Route`
/// enum is not something the guard can easily match without an
/// exhaustive arm per variant). Ordering:
/// - Individual prefixes for the top-level staff-only surfaces.
/// - A special-case KB edit path since the read side (`/kb`,
///   `/kb/articles/:id`) is contact-permitted.
/// - Every `/settings/*` except a small contact-permitted allowlist.
///
/// Anything the pathname does not match falls through to the shared
/// Outlet so the contact-permitted routes stay reachable.
pub fn pathname_is_contact_forbidden(path: &str) -> bool {
    // Strip query + trailing slash so the compare is stable
    // regardless of how the browser hands us the URL.
    let p = path.split('?').next().unwrap_or(path).trim_end_matches('/');
    // Top-level staff-only prefixes. `starts_with("{prefix}/")` covers
    // every nested route under the prefix; a bare equality catches
    // the index route (`/companies` itself).
    const DENIED_PREFIXES: &[&str] = &[
        "/companies",
        "/contacts",
        "/calendar",
        "/dispatch",
        "/scheduling-templates",
        "/rate-cards",
        "/payments",
        "/tax-rates",
        "/payment-gateways",
        "/reports",
        "/time",
        "/timesheets",
        "/dashboards",
        "/dashboard/tv",
        "/big",
        "/admin",
        "/pick-tenant",
        "/create-org",
        "/invite",
        "/dev",
    ];
    for prefix in DENIED_PREFIXES {
        if p == *prefix || p.starts_with(&format!("{prefix}/")) {
            return true;
        }
    }
    // KB reads are contact-permitted (kb:read cap); creating +
    // editing an article is staff-only. Match the two mutation URLs
    // specifically so `/kb/articles/:id` (read-only detail) still
    // resolves.
    if p == "/kb/articles/new" {
        return true;
    }
    if p.starts_with("/kb/articles/") && p.ends_with("/edit") {
        return true;
    }
    // Every /settings/* except a small contact-permitted allowlist.
    // `/settings` (hub, tile filter already scopes visibility),
    // `/settings/portal-branding` (contact editor, has its own cap
    // gate), and `/settings/appearance` (personal theme picker) are
    // the only three surfaces a contact reaches under this tree.
    if p == "/settings" || p == "/settings/portal-branding" || p == "/settings/appearance" {
        return false;
    }
    if p.starts_with("/settings/") || p == "/settings" {
        return true;
    }
    false
}

/// Layout component that gates all authenticated routes (declared
/// here, before the `Route` enum, because the `Routable` derive
/// expands the `#[layout(AuthGuard)]` reference at the enum site
/// and needs the component already in scope).
///
/// Renders nothing when the user is not signed in and asks the
/// navigator to replace the current entry with `/login` during
/// render. The render-time guard is what defeats the back-button
/// bypass: a popstate-driven re-render of a protected route never
/// commits any of its content to the DOM, so there is no flash of
/// authenticated UI before redirect.
#[component]
pub fn AuthGuard() -> Element {
    let auth = hooks::use_auth();
    let nav = use_navigator();
    // mokosh-contact-login prompt 005: cold-load bootstrap for the
    // contact plane. Fires once on mount; if there is no in-memory
    // contact access token but a refresh token is mirrored in
    // localStorage, kick off `/contact/auth/refresh` in the
    // background. This races the first render (intentionally) and
    // mirrors the staff-bearer bootstrap in hooks/auth.rs: a hard
    // refresh on `/dashboard` under a contact session may transiently
    // fall through the guard's next branch, then re-render once the
    // refresh lands.
    #[cfg(feature = "web")]
    use_effect(|| {
        // MAPPS-630: skip the contact rehydrate when a staff bearer
        // is present. The two planes are mutually exclusive within
        // one browser origin, and blindly rehydrating a stale
        // contact refresh token here (localStorage IS cross-tab)
        // would resurrect a portal session inside a freshly-opened
        // staff tab, shadowing the staff sidebar via the MAPPS-625
        // "contact wins" precedence.
        if crate::hooks::fetch::api::current_access_token().is_some() {
            return;
        }
        if !crate::hooks::fetch::api::has_contact_session()
            && crate::hooks::fetch::api::current_contact_refresh_token().is_some()
        {
            spawn(async move {
                let _ = crate::hooks::contact_auth::refresh_contact_session().await;
            });
        }
    });
    let auth_state = auth.read();
    if auth_state.is_loading {
        // Still hydrating tokens from sessionStorage. Render a
        // placeholder so we do not kick off the OIDC dance just to
        // find we already have a session.
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                "Loading…"
            }
        };
    }
    if !auth_state.is_authenticated() {
        // MAPPS-520: platform-plane admins pass through the tenant
        // AuthGuard when they hold a valid platform bearer in
        // sessionStorage. The MAPPS-518 platform-admin surface
        // (currently only `/admin/tenants`, `TenantManagementPage`)
        // gates its own render on the same signal and issues its own
        // fetches with the platform bearer, so a platform-only
        // caller can reach it without the tenant `AuthContext`
        // being populated. AppShell / Sidebar / TopBar all read the
        // tenant user via `.as_ref().map(...).unwrap_or(false)` so
        // they render sensibly with no tenant session; the platform
        // admin sees a nav where every tenant-role-gated item is
        // hidden EXCEPT the Tenants item (which gates on
        // `platform_bearer_present()`).
        //
        // Every OTHER `AuthGuard` fall-through remains: no platform
        // bearer AND no tenant auth still bounces to `/login` (or
        // kicks the OIDC flow off), so nothing above this line
        // silently unlocks a route that used to require tenant auth.
        #[cfg(feature = "web")]
        if crate::hooks::fetch::api::current_platform_access_token().is_some() {
            return rsx! {
                ErrorBoundary {
                    handle_error: |errors: ErrorContext| rsx! {
                        RouteErrorFallback { errors }
                    },
                    Outlet::<Route> {}
                }
            };
        }

        // mokosh-contact-login prompt 005: contact-plane session
        // recognition. A contact JWT (`typ: "contact"`) is a distinct
        // identity from the tenant staff bearer; the shared mokosh
        // workspace routes render for either. The tenant `AuthContext`
        // is empty in this branch (the contact identity is not a
        // `users` row), so downstream pages that read `use_auth()` see
        // `is_authenticated() == false` - capability gating lands in
        // prompt 006 to hide the staff-only surfaces.
        #[cfg(feature = "web")]
        if crate::hooks::fetch::api::has_contact_session() {
            // MAPPS-623: hard block on staff-only surfaces. The
            // sidebar already hides these NavItems for contacts
            // (each is behind `use_capability(STAFF_ONLY)`), but a
            // contact could still reach them by typing the URL,
            // following a stale bookmark, or getting bounced from a
            // deep-link that was authored for a staff user. Every
            // staff-only prefix redirects to /dashboard rather than
            // rendering a page whose data fetches would 401/403 and
            // read as broken. Contact-permitted routes fall through
            // to the shared Outlet unchanged.
            #[cfg(target_arch = "wasm32")]
            let pathname = web_sys::window()
                .and_then(|w| w.location().pathname().ok())
                .unwrap_or_default();
            #[cfg(not(target_arch = "wasm32"))]
            let pathname = String::new();
            if pathname_is_contact_forbidden(&pathname) {
                nav.replace(Route::Dashboard {});
                return rsx! {
                    div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                        "Redirecting to your dashboard…"
                    }
                };
            }
            return rsx! {
                ErrorBoundary {
                    handle_error: |errors: ErrorContext| rsx! {
                        RouteErrorFallback { errors }
                    },
                    Outlet::<Route> {}
                }
            };
        }

        // MAPPS-615 (prompt 014): origin cue - a stranded visitor on
        // a `/portal/*` URL with no session gets bounced to the
        // contact login instead of the staff `/login`.
        //
        // MAPPS-634: also bounce a stranded visitor with NO active
        // session but a `contact_last_portal_id` or
        // `contact_last_slug` hint in localStorage (they were signed
        // in as a contact previously; their session expired while
        // they were on `/dashboard` or another shared route). Preserve
        // "the last plane I signed into" without needing them to be
        // physically on a `/portal/*` URL when the session died. Prefer
        // the Portal-ID URL shape (prompt 011 primary); fall back to
        // the legacy slug URL; last resort is the generic step-1 page
        // so the visitor can retype their Portal ID.
        //
        // Both branches run BEFORE the standalone-mode staff bounce
        // below so a contact never falls through to the staff login.
        #[cfg(feature = "web")]
        {
            #[cfg(target_arch = "wasm32")]
            let on_portal_url = web_sys::window()
                .and_then(|w| w.location().pathname().ok())
                .is_some_and(|p: String| p.starts_with("/portal/") || p == "/portal");
            #[cfg(not(target_arch = "wasm32"))]
            let on_portal_url = false;
            let last_portal_id = crate::hooks::fetch::api::current_contact_last_portal_id();
            let last_slug = crate::hooks::fetch::api::current_contact_last_slug();
            let contact_hint_present = last_portal_id.is_some() || last_slug.is_some();
            if on_portal_url || contact_hint_present {
                if let Some(pid) = last_portal_id {
                    let dest = format!("/portal/{pid}/login");
                    #[cfg(target_arch = "wasm32")]
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().replace(&dest);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = dest;
                } else if let Some(slug) = last_slug {
                    let dest = format!("/portal/{slug}/login");
                    #[cfg(target_arch = "wasm32")]
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().replace(&dest);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = dest;
                } else {
                    nav.replace(Route::ContactGenericLogin {});
                }
                return rsx! {
                    div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                        "Redirecting to sign in…"
                    }
                };
            }
        }

        // MAPPS-368: a deployment with no OIDC issuer has no bunyip OP to
        // redirect to, so send the user to the standalone username/password
        // login form instead of a dead `/oauth2/authorize`.
        //
        // mokosh-contact-login: the pre-pivot portal-host redirect to
        // `Route::PortalLogin` retires with the customer-portal route
        // family (prompt 001). Contact plane replacement in prompt 005.
        if crate::modules::oidc::is_standalone() {
            nav.replace(Route::Login {});
            return rsx! {
                div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                    "Redirecting to sign in…"
                }
            };
        }
        // No local session. Kick off the OIDC code+PKCE flow ON THIS
        // ORIGIN so the PendingFlow (code_verifier + state + nonce)
        // lands in *this* SPA's sessionStorage and /auth/callback can
        // complete the code exchange. start_login replaces the page
        // with /oauth2/authorize. From there:
        //   - if the user has an OP session cookie (e.g. they signed
        //     in on the Bunyip hub and clicked a launcher tile),
        //     authorize 302s straight back to /auth/callback?code=...
        //   - otherwise authorize 302s to bunyip's /login?return_to=
        //     and the SSO bridge closes the loop after they sign in.
        //
        // The `/login` route is the explicit user-facing entry point and
        // runs the same start_login kickoff (see the Login component below).
        // Hitting `/login` directly or hitting a protected route both lead
        // through bunyip's /oauth2/authorize on this origin.
        let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
        // Carry the route the user actually asked for through the OIDC
        // round-trip so a cold-loaded / bookmarked deep link lands back on
        // it instead of `/dashboard` (MAPPS-323). The interactive `Login`
        // component passes its own `/dashboard` default; this guard is the
        // path that fires for protected deep links.
        let return_to = crate::modules::oidc::current_return_to();
        // MAPPS-432: a kickoff that fails leaves the user on the placeholder
        // below with no other trace, so log the cause rather than dropping it.
        if let Err(e) = crate::modules::oidc::start_login(&cfg, return_to) {
            crate::modules::oidc::log_auth_error(&format!("auth guard: login kickoff failed: {e}"));
        }
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                "Signing you in…"
            }
        };
    }
    // Forced onboarding for new Bunyip-JIT users: redirect to
    // /onboarding/profile whenever the server reports
    // profile_completed = false (first + last name still synthetic
    // placeholders). Bypass when the user is already on the
    // onboarding route itself, otherwise the AuthGuard would re-fire
    // its own redirect every render and loop. Reads the pathname out
    // of the current location rather than the router's current Route
    // because we need to make the comparison synchronously inside
    // render; reading the location avoids a re-entrant signal read.
    //
    // MAPPS-317: gate the redirect on `server_loaded` so the optimistic
    // rehydrate window (which sets profile_completed=true before /me
    // confirms) cannot transiently report `needs_onboarding = false`
    // and then flip later. Without the gate, the chain
    //   AuthGuard (flip to false) -> /onboarding/profile mount
    //   -> Onboarding's defense-in-depth effect sees the next /me
    //   -> flip back to true -> nav.replace(Dashboard)
    // bounces a user clicking Calendar into Dashboard on the first
    // click. With the gate, AuthGuard only redirects after the first
    // /me reconcile, by which point profile_completed is stable.
    let needs_onboarding = auth_state.server_loaded
        && auth_state
            .user
            .as_ref()
            .is_some_and(|u| !u.profile_completed);
    if needs_onboarding {
        let on_onboarding_route =
            crate::platform::location::pathname().is_some_and(|p| p == "/onboarding/profile");
        if !on_onboarding_route {
            tracing::info!(
                target: "auth_guard",
                "redirecting to /onboarding/profile (profile_completed=false, server_loaded=true)"
            );
            nav.replace(Route::Onboarding {});
            return rsx! {
                div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                    "Setting up your profile…"
                }
            };
        }
    }
    // MAPPS-318: catch errors propagated up from any route subtree so a
    // single broken page does not abandon the user on a frozen URL with
    // no way out. The boundary catches `?` / `bail!` errors returned
    // from a component returning `Element` (which is
    // `Result<VNode, RenderError>`). It does NOT catch wasm panics:
    // dioxus-core 0.7 documents that `CapturedPanic` is unreachable on
    // wasm because the runtime does not support unwinding, so a `panic!`
    // still aborts the runtime. Follow-up for genuine panic recovery is
    // parked until upstream wasm supports catching unwinds; for now the
    // hook-of-hook and similar invariants must be caught in tests + the
    // console-error-panic-hook trace.
    rsx! {
        ErrorBoundary {
            handle_error: |errors: ErrorContext| rsx! {
                RouteErrorFallback { errors }
            },
            Outlet::<Route> {}
        }
    }
}

// mokosh-contact-login: PortalGuard retired with the /portal/* route
// family (prompt 001). Contact plane replacement in prompt 005 will
// gate the contact-scoped surface via a different layout.

/// MAPPS-318: full-screen fallback rendered when the route-level
/// `ErrorBoundary` catches a propagated error. Sidebar / topbar live
/// inside each route's `AppLayout`, so they are not present here; the
/// "Go to dashboard" button clears the boundary AND navigates so the
/// user lands on a fresh route subtree with chrome restored.
#[component]
fn RouteErrorFallback(errors: ErrorContext) -> Element {
    let nav = use_navigator();
    let errors_for_log = errors.clone();
    use_effect(move || {
        tracing::error!(
            target: "route_error_boundary",
            "caught route render error: {:?}",
            errors_for_log
        );
    });
    let goto_dashboard = {
        let errors = errors.clone();
        move |_| {
            errors.clear_errors();
            nav.replace(Route::Dashboard {});
        }
    };
    let reload = {
        let errors = errors.clone();
        move |_| {
            // MAPPS-504: a desktop window cannot reload a document, so it
            // takes the same recovery the sibling "back to the dashboard"
            // control does rather than doing nothing when clicked.
            if !crate::platform::location::reload() {
                errors.clear_errors();
                nav.replace(Route::Dashboard {});
            }
        }
    };
    let detail = format!("{errors:?}");
    rsx! {
        div { class: "min-h-screen flex items-center justify-center px-4 bg-app",
            div { class: "max-w-md w-full bg-surface rounded-lg shadow-lg p-8 text-center",
                h1 { class: "text-xl font-semibold text-content mb-2",
                    "Something went wrong on this page"
                }
                p { class: "text-sm text-muted mb-6",
                    "An unexpected error stopped this view from rendering. Your sign-in and other tabs are unaffected."
                }
                if cfg!(debug_assertions) {
                    pre {
                        class: "text-left text-xs bg-surface-2 rounded-md p-3 mb-4 overflow-x-auto whitespace-pre-wrap break-words",
                        "{detail}"
                    }
                }
                div { class: "flex justify-center gap-3",
                    button {
                        r#type: "button",
                        class: "inline-flex items-center justify-center font-medium rounded-md px-4 py-2 text-sm bg-accent text-on-accent hover:opacity-90",
                        onclick: goto_dashboard,
                        "Go to dashboard"
                    }
                    button {
                        r#type: "button",
                        class: "inline-flex items-center justify-center font-medium rounded-md px-4 py-2 text-sm bg-surface-2 text-content border border-line",
                        onclick: reload,
                        "Reload page"
                    }
                }
            }
        }
    }
}

/// Application routes
#[derive(Clone, Routable, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    // Public routes
    #[route("/")]
    Home {},

    // MAPPS-520: unified `/login` for both personas.
    //
    // Post-MAPPS-518 the SPA had TWO visible login URLs (`/login` for
    // the mokosh platform super-admin, `/client/login` for the MSP
    // tenant admin/user). MAPPS-520 collapses them into ONE URL and
    // one page: the `Login` component (see `pages::login`) tries the
    // platform-admin credential first and falls back to the tenant
    // credential on 401. The MAPPS-518 credential isolation stays
    // intact underneath (`platform_admins` and `users` are still
    // separate tables; the MAPPS-498 mirror still cannot touch
    // `platform_admins`) — the unification is UI only.
    //
    // `/client/login` and `/platform/login` are retained as `#[route]`
    // entries but render "moved" redirect stubs (see
    // `ClientLoginLegacy` and `PlatformLoginLegacy` components
    // below) so bookmarks from either previous URL land on the
    // unified page without a 404.
    #[route("/login")]
    Login {},

    #[route("/client/login")]
    ClientLoginLegacy {},

    #[route("/platform/login")]
    PlatformLoginLegacy {},

    // MAPPS-497 item 6: dedicated intermediate routes for the
    // identity-first login flow. Both read cross-page state from the
    // `PENDING_LOGIN` global signal; empty state redirects them back
    // to /login. Same auth surface as /login (public routes; no
    // AuthGuard gating).
    #[route("/pick-tenant")]
    PickTenant {},

    #[route("/create-org")]
    CreateOrg {},

    #[route("/auth/callback")]
    AuthCallback {},

    #[route("/forgot-password")]
    ForgotPassword {},

    #[route("/reset-password/:token")]
    ResetPassword { token: String },

    // MAPPS-552: first-time password setup for a fresh client-admin.
    // The welcome email from `TenantService::mint_and_send_welcome`
    // points here (not `/reset-password/:token`) so the URL bar and
    // the page heading match what the recipient is actually doing -
    // setting a password for a specific client portal, not resetting
    // an existing one.
    #[route("/set-password/:token")]
    SetPassword { token: String },

    #[route("/invite/:token")]
    InviteAccept { token: String },

    // PMS-730: the destination of the client request-form email
    // mokosh-server sends when an agent issues a request link. Public by
    // construction: the emailed single-use token in the path is the only
    // credential the visitor has, and they are a client with no session.
    #[route("/request-forms/:token")]
    RequestForm { token: String },

    #[route("/signup")]
    Signup {},

    #[route("/signup/:token")]
    SignupComplete { token: String },

    // mokosh-contact-login prompt 005: contact-plane portal routes.
    // Public (no AuthGuard, no PortalGuard). The token on the two
    // password-write routes arrives via the MAPPS-560 query-segment
    // shape so the component receives it as a prop rather than
    // scraping `window.location.search`.
    //
    // MAPPS-589 (prompt 011): the primary three-field login page
    // (Portal ID + email + password) mounts at `/portal/login`; the
    // Portal-ID-scoped page shares the `/portal/:handle/login` path
    // shape with the legacy slug login and is dispatched at render
    // time by the `ContactHandleLogin` wrapper (shape check on the
    // handle: 9 ASCII digits -> Portal ID, otherwise legacy slug).
    // Collapsing the two into ONE route avoids the Dioxus route
    // collision that having two `/portal/:X/login` shapes would
    // create.
    #[route("/portal/login")]
    ContactGenericLogin {},

    #[route("/portal/:handle/login")]
    ContactHandleLogin { handle: String },

    #[route("/portal/:slug/set-password?:token")]
    ContactSetPassword { slug: String, token: String },

    #[route("/portal/:slug/forgot-password")]
    ContactForgotPassword { slug: String },

    #[route("/portal/:slug/reset-password?:token")]
    ContactResetPassword { slug: String, token: String },

    // MAPPS-572 (prompt 010): magic-link finder + Company picker.
    // Both public (no AuthGuard). Finder accepts an optional
    // `?email=` query segment so the picker's "Request a new sign-in
    // link" button, and the password-login page's "sign in without a
    // password" affordance, can pre-fill the field on hop.
    //
    // MAPPS-589 (prompt 011): moved from `/portal/login` to
    // `/portal/find?:email` so the shorter path can host the primary
    // three-field password page. The finder path change is contained
    // to the router: every in-code Link/nav.replace still uses the
    // `Route::ContactMagicLinkLogin` variant.
    #[route("/portal/find?:email")]
    ContactMagicLinkLogin { email: String },

    #[route("/portal/pick?:token")]
    ContactPicker { token: String },

    // ======================================================================
    // Authenticated routes. The `AuthGuard` layout below renders nothing
    // (and synchronously navigates to /login) whenever the in-memory
    // auth signal reports the user as unauthenticated. This is what
    // stops the back-button from flashing /dashboard after logout: a
    // popstate-driven re-render of a protected route is gated *during*
    // render, not after, so the dashboard component never gets a frame
    // to display its content. `use_require_auth` inside individual
    // pages still runs but is now a redundant safety net.
    // ======================================================================
    #[layout(AuthGuard)]

      // ==================================================================
      // MAPPS-366: chromeless authenticated routes. These render full-screen
      // WITHOUT the AppShell chrome (no TopBar / Sidebar / banners), so they
      // sit directly under AuthGuard, BEFORE the AppShell layout opens below.
      // ==================================================================

      // Forced-onboarding for new Bunyip-JIT users. Sits inside
      // AuthGuard (the user MUST be authenticated to reach it) but
      // the guard exempts it from its own redirect; see the
      // pathname check in AuthGuard above. Full-screen, no chrome.
      #[route("/onboarding/profile")]
      Onboarding {},

      // MAPPS-256: full-screen, team-scoped wall-monitor "TV view" of the
      // dashboard. Renders WITHOUT chrome so no TopBar / Sidebar / banners /
      // ToastRoot appear over the bare table.
      #[route("/dashboard/tv")]
      DashboardTv {},

      // MAPPS-302: NOC "Big View" kiosk routes - no sidebar or top bar, larger
      // typography, auto-refresh tick. Moved up here (out of the chrome group)
      // for MAPPS-366 so they stay outside the AppShell layout.
      #[route("/big/tickets")]
      BigTickets {},

      #[route("/big/dispatch")]
      BigDispatch {},

      #[route("/big/calendar")]
      BigCalendar {},

    // ====================================================================
    // MAPPS-366: everything below runs inside the persistent AppShell layout
    // (TopBar + Sidebar + banners + ToastRoot). The shell stays mounted across
    // navigation; only the routed subtree swaps through its Outlet, so the
    // user<->admin dashboard switch no longer blanks the screen.
    // ====================================================================
    #[layout(AppShell)]

      // Dashboard
      #[route("/dashboard")]
      Dashboard {},

      // PMS-453: per-user saved dashboards (Phase 1: management surface).
      #[route("/dashboards")]
      SavedDashboards {},

      // PMS-472: view surface for one saved dashboard.
      #[route("/dashboards/:id/view")]
      SavedDashboardView { id: String },

    // Tickets
    #[route("/tickets")]
    TicketList {},

    #[route("/tickets/new")]
    TicketNew {},

    #[route("/tickets/:id")]
    TicketDetail { id: String },

    // Time Tracking
    #[route("/time")]
    TimeEntryList {},

    #[route("/time/new")]
    TimeEntryNew {},

    #[route("/timesheets")]
    Timesheets {},

    // MAPPS-194: manager/admin queue to approve/reject submitted timesheets.
    #[route("/timesheets/approvals")]
    TimesheetApprovals {},

    // PMS-481: "My approvals" queue across every approval target
    // (ticket / time_entry / change_request / quote). The signed-in
    // user sees pending rows where they are the named approver or
    // hold the assigned role; approve / reject inline.
    #[route("/approvals")]
    Approvals {},

    // Projects
    #[route("/projects")]
    ProjectList {},

    #[route("/projects/new")]
    ProjectNew {},

    #[route("/projects/:id")]
    ProjectDetail { id: String },

    #[route("/projects/:id/tasks")]
    ProjectTasks { id: String },

    // Contacts
    #[route("/companies")]
    CompanyList {},

    #[route("/companies/new")]
    CompanyNew {},

    #[route("/companies/:id")]
    CompanyDetail { id: String },

    #[route("/companies/:id/edit")]
    CompanyEdit { id: String },

    // MAPPS-590 (mokosh-contact-login prompt 012): Company-scoped
    // portal role editor. `:id == "new"` creates a Company-scoped
    // role; any UUID edits an existing scoped role. The list of
    // scoped roles for a Company lives on the CompanyRolesCard
    // rendered inside `CompanyDetailPage`.
    #[route("/companies/:company_id/roles/:id")]
    CompanyRoleEdit { company_id: String, id: String },

    #[route("/contacts")]
    ContactList {},

    #[route("/contacts/new")]
    ContactNew {},

    #[route("/contacts/:id")]
    ContactDetail { id: String },

    #[route("/contacts/:id/edit")]
    ContactEdit { id: String },

    // Calendar
    #[route("/calendar")]
    Calendar {},

    #[route("/dispatch")]
    DispatchBoard {},

    #[route("/scheduling-templates")]
    SchedulingTemplates {},

    // MAPPS-302 "Big View" kiosk routes moved up under AuthGuard (chromeless)
    // for MAPPS-366; see the top of the authenticated block. Operators still
    // bookmark them on a wall screen with the `?refresh=` / `?status=` /
    // `?priority=` query params.

    // Contracts
    // Quotes (PMS-675): the sales document that precedes a Project.
    #[route("/quotes")]
    QuoteList {},

    #[route("/quotes/new")]
    QuoteNew {},

    #[route("/quotes/:id")]
    QuoteDetail { id: String },

    #[route("/quotes/:id/edit")]
    QuoteEdit { id: String },

    #[route("/contracts")]
    ContractList {},

    #[route("/contracts/new")]
    ContractNew {},

    #[route("/contracts/:id")]
    ContractDetail { id: String },

    #[route("/contracts/:id/edit")]
    ContractEdit { id: String },

    // Rate cards
    #[route("/rate-cards")]
    RateCardList {},

    // Static `/new` must precede the `:id` route so it is not parsed as a
    // rate-card id (MAPPS-217).
    #[route("/rate-cards/new")]
    RateCardNew {},

    #[route("/rate-cards/:id")]
    RateCardDetail { id: String },

    // Billing
    #[route("/invoices")]
    InvoiceList {},

    #[route("/invoices/new")]
    InvoiceNew {},

    #[route("/invoices/:id")]
    InvoiceDetail { id: String },

    #[route("/payments")]
    PaymentList {},

    #[route("/tax-rates")]
    TaxRateList {},

    #[route("/payment-gateways")]
    PaymentGatewayConfig {},

    // MAPPS-638: credit notes, the correction path for a sent invoice.
    #[route("/credit-notes")]
    CreditNoteList {},

    #[route("/credit-notes/:id")]
    CreditNoteDetail { id: String },

    // MAPPS-639: a company's account over a period, computed and not stored.
    #[route("/statements")]
    Statement {},

    // Assets
    #[route("/assets")]
    AssetList {},

    #[route("/assets/new")]
    AssetNew {},

    #[route("/assets/:id")]
    AssetDetail { id: String },

    // Knowledge Base
    #[route("/kb")]
    KBHome {},

    #[route("/kb/articles?:q&:tag&:category")]
    KBArticleList { q: String, tag: String, category: String },

    #[route("/kb/articles/new")]
    KBArticleNew {},

    #[route("/kb/articles/:id")]
    KBArticleDetail { id: String },

    #[route("/kb/articles/:id/edit")]
    KBArticleEdit { id: String },

    // Reports
    #[route("/reports")]
    Reports {},

    #[route("/reports/:report_type")]
    ReportDetail { report_type: String },

    // Settings
    //
    // Account-management surfaces (profile, security, sessions, audit
    // logs, user management, invites) moved to the Bunyip hub in
    // docs/bunyip/08-mokosh-clients-cleanup.md. The per-PSA-feature
    // settings (teams, notifications, integrations, billing-config)
    // belong here but their pages are not implemented yet; bring them
    // back as `/operations/*` routes when the work is scheduled.
    #[route("/settings/active-tenant")]
    ActiveTenant {},

    // MAPPS-169: centralized Settings hub. One left-nav entry lands on
    // `/settings` (grouped cards); each configuration surface gets a
    // sub-route. The "re-homed" surfaces (SLA, rate cards, tax rates,
    // payment gateways) render the SAME page components as their original
    // `/admin/*` and `/`-prefixed routes, which stay in place - the old
    // nav items and buttons keep working (chosen "keep old + add Settings").
    #[route("/settings")]
    SettingsHome {},
    /// MAPPS-622 (mokosh-branding prompt 003 sibling): staff-side
    /// tenant branding editor at `/settings/branding`. Sets the MSP
    /// defaults every Company inherits from. Per-Company overrides
    /// still edit on the Company detail page (MAPPS-619). Page gates
    /// on `role.is_admin()`; non-admins get `ContentUnavailable`.
    #[route("/settings/branding")]
    SettingsBranding {},
    /// MAPPS-620 (mokosh-branding prompt 004): contact-plane portal
    /// branding editor. The page's first-render gate checks
    /// `use_capability("settings:manage_company_branding")`; a
    /// contact without the capability sees a `ContentUnavailable`
    /// splash. Staff hitting this URL land on the same page but the
    /// contact-only endpoint (`/contact/companies/self/branding`)
    /// returns 401 for them, which the page surfaces as an error;
    /// staff editing a per-Company brand goes through the Company
    /// detail page instead (MAPPS-619).
    #[route("/settings/portal-branding")]
    ContactPortalBranding {},
    // MAPPS-258: per-group landing routes. The index lists these four
    // groups; each landing lists only its own leaf surfaces. The leaf
    // routes below stay flat so existing deep links keep resolving.
    #[route("/settings/group/service-types")]
    SettingsGroupServiceTypes {},
    #[route("/settings/group/billing")]
    SettingsGroupBilling {},
    #[route("/settings/group/tickets")]
    SettingsGroupTickets {},
    #[route("/settings/group/integrations")]
    SettingsGroupIntegrations {},
    // MAPPS-364: Data (tenant import/export) group landing.
    #[route("/settings/group/data")]
    SettingsGroupData {},
    #[route("/settings/work-types")]
    SettingsWorkTypes {},
    #[route("/settings/task-statuses")]
    SettingsTaskStatuses {},
    #[route("/settings/asset-types")]
    SettingsAssetTypes {},
    // PMS-601: company industry lookup editor.
    #[route("/settings/company-industries")]
    SettingsCompanyIndustries {},
    // MAPPS-173: project type editor (server CRUD from PMS-322).
    #[route("/settings/project-types")]
    SettingsProjectTypes {},
    #[route("/settings/sla")]
    SettingsSla {},
    // MAPPS-345: tenant-wide standard due date (PMS-345 server setting).
    #[route("/settings/scheduling")]
    SettingsScheduling {},
    // MAPPS-259: per-user theme + accent picker.
    #[route("/settings/appearance")]
    SettingsAppearance {},
    // MAPPS-256: per-user wall-monitor TV-view toggle + team scope.
    #[route("/settings/tv-view")]
    SettingsTvView {},
    // MAPPS-244: tenant-wide max hours per day (PMS-396 server setting).
    #[route("/settings/time-tracking")]
    SettingsTimeTracking {},
    #[route("/settings/rate-cards")]
    SettingsRateCards {},
    #[route("/settings/tax-rates")]
    SettingsTaxRates {},
    #[route("/settings/gateways")]
    SettingsGateways {},
    // MAPPS-170: invoice payment-terms lookup editor (server CRUD from PMS-333).
    #[route("/settings/payment-terms")]
    SettingsPaymentTerms {},
    // MAPPS-640: the product catalog (server CRUD from PMS-955).
    #[route("/settings/products")]
    SettingsProducts {},
    // MAPPS-172: ticket lookup editors (server CRUD from PMS-321).
    #[route("/settings/ticket-statuses")]
    SettingsTicketStatuses {},
    #[route("/settings/ticket-priorities")]
    SettingsTicketPriorities {},
    #[route("/settings/ticket-types")]
    SettingsTicketTypes {},
    #[route("/settings/ticket-queues")]
    SettingsTicketQueues {},
    #[route("/settings/ticket-categories")]
    SettingsTicketCategories {},
    // MAPPS-199: RMM integration admin UI (server CRUD from PMS-102/103/104/105).
    #[route("/settings/rmm/connections")]
    SettingsRmmConnections {},
    #[route("/settings/rmm/device-mappings")]
    SettingsRmmDeviceMappings {},
    #[route("/settings/rmm/alert-rules")]
    SettingsRmmAlertRules {},
    // MAPPS-364: admin-only tenant data import/export (server PMS-646).
    #[route("/settings/import-export")]
    SettingsImportExport {},
    // mokosh-contact-login prompt 007: MSP portal-role management. List
    // + edit live under `/settings/contact-roles*`. The card on a
    // contact detail (`ContactPortalCard`, prompt 003) points admins
    // here to create / rename / retire portal roles.
    #[route("/settings/contact-roles")]
    ContactRolesList {},
    #[route("/settings/contact-roles/:id")]
    ContactRoleEdit { id: String },
    // MAPPS-426: admin-only rename of this tenant. The name is not an
    // internal label - it goes out in the request-form email subject and the
    // invitation email, and a fresh tenant is seeded "My workspace".
    #[route("/settings/organization")]
    SettingsOrganization {},

    // Mokosh-side profile. Edits the tenant-scoped fields on the
    // user row (name, title, phone, mobile, timezone). Cross-app
    // identity (email, password, MFA, sessions, billing) lives on
    // bunyip-web's `/settings`; the UserMenu's "Account Settings"
    // link sends users there.
    #[route("/profile")]
    Profile {},

    // Build versions + live API/dependency health. Reachable by any
    // authenticated user from the user menu (PMS-237).
    #[route("/system-status")]
    SystemStatus {},

    // Internal reference: canonical Button variants/sizes/states (PMS-357 AC4).
    #[route("/dev/buttons")]
    ButtonShowcase {},

    // Admin surfaces under /admin/*, gated at runtime by the user's role
    // inside each page (matching the server's RequireAdmin), available in
    // every build since the server endpoints exist regardless of tenancy.
    // MAPPS-526: that gate is enforced by the `admin_route_role_gates` tests
    // at the bottom of this file, not by this comment - /admin/forms went a
    // release without one while the comment claimed otherwise.
    #[route("/admin/audit")]
    AuditLog {},
    // PMS-731: request-form builder. Admin config, so it lives with the
    // other /admin/* surfaces.
    #[route("/admin/forms")]
    FormsBuilder {},
    #[route("/admin/sla")]
    SlaManagement {},
    // PMS-791 phase 2: retired the old "team" nav name; the page was
    // always invitations. `/admin/team` still routes here for one release
    // via TeamLegacyRedirect below so bookmarks do not break.
    #[route("/admin/invitations")]
    Invitations {},
    #[route("/admin/team")]
    TeamLegacyRedirect {},
    // PMS-791 phase 2: the actual teams management page (list + create +
    // edit + membership).
    #[cfg(feature = "multi-tenant")]
    #[route("/admin/teams")]
    Teams {},

    // mokosh-contact-login: /admin/tenants (Clients tab / TenantManagement)
    // retired on this branch (prompt 001).

    // MAPPS-366: close the AppShell layout. Every route above (from Dashboard
    // down) renders inside the persistent shell; the chromeless routes at the
    // top of the AuthGuard block and the portal routes below do not.
    #[end_layout]

    // End of AuthGuard scope.
    //
    // mokosh-contact-login: the whole /portal/* customer-portal route
    // tree retired on this branch (prompt 001). The contact plane
    // lands under /portal/{slug}/* in prompt 005 with a different
    // shape - random slug per Company, no PortalGuard layout, same
    // mokosh workspace routes with capability gating.
    #[end_layout]

    // Catch-all 404
    #[route("/:..route")]
    NotFound { route: Vec<String> },
}

// Route component wrappers - these import the actual page components
//
// MAPPS-624: every wrapper under `#[layout(AppShell)]` carries its own
// `div { class: "max-w-7xl mx-auto" }`. That cap used to sit in `AppShell`
// around the `Outlet`, which fixed every page at 1280px; it is inlined per
// route now so widening one page is a one-line change here and touches no
// other page. `KBArticleDetail` is the one route that omits it and fills the
// window (`scripts/check-page-width.sh` holds the rest of them to it).
use pages::*;

/// Top-level navigate to the Bunyip hub. Used by the legacy
/// account-surface routes (`/login`, `/forgot-password`, etc.) so that
/// existing bookmarks keep working instead of 404ing. `replace()`
/// rather than `assign()` so the hub's URL takes over the history
/// entry; the back button skips the dead mokosh-clients URL.
fn redirect_to_hub(path: &str) {
    let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
    crate::platform::location::replace(&cfg.hub_url(path));
}

#[component]
fn HubRedirect(target: String, label: &'static str) -> Element {
    use_effect({
        let target = target.clone();
        move || {
            redirect_to_hub(&target);
        }
    });
    rsx! {
        div { class: "min-h-screen flex items-center justify-center p-8 text-sm text-muted",
            "Redirecting to {label}…"
        }
    }
}

#[component]
fn Home() -> Element {
    // mokosh-contact-login: the on_portal_host branch retired with the
    // customer-portal route family (prompt 001). Home renders the
    // agent-side marketing page unconditionally now. Contact plane
    // lands under /portal/{slug}/login in prompt 005 (a separate,
    // routed page, not a host-branch on Home).
    rsx! { home::HomePage {} }
}

/// `/login` is the explicit sign-in entry point. Bookmarks, the homepage CTA,
/// and the navbar "Sign in" link all land here. Kick off the OIDC code+PKCE
/// flow against the configured issuer (bunyip-api post-cutover) and show a
/// "Signing you in…" placeholder while the browser navigates to
/// `/oauth2/authorize`.
///
/// This is the same kickoff `AuthGuard` does when an unauthenticated user
/// hits a protected route. The pre-cutover behaviour (HubRedirect to bunyip's
/// `/login` directly) skipped the OIDC handshake entirely, which is why the
/// user landed on bunyip's dashboard with no way back to msp.
// MAPPS-520: legacy-URL redirect stubs. Post-unification the only
// public login URL is `/login`; the two previous URLs (`/client/login`
// and `/platform/login`) render a tiny redirect component so bookmarks
// from before the unification land on the new URL without a 404. Kept
// separate from the unified `Login` route wrapper below so a
// bookmarked visitor sees a moved-page hint rather than the
// unification landing silently. Public (no AuthGuard).
#[component]
fn ClientLoginLegacy() -> Element {
    let nav = use_navigator();
    use_hook(move || {
        nav.replace(Route::Login {});
    });
    rsx! {
        div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
            "Sign-in has moved to /login. Redirecting…"
        }
    }
}

#[component]
fn PlatformLoginLegacy() -> Element {
    let nav = use_navigator();
    use_hook(move || {
        nav.replace(Route::Login {});
    });
    rsx! {
        div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
            "Sign-in has moved to /login. Redirecting…"
        }
    }
}

#[component]
fn Login() -> Element {
    let _nav = use_navigator();
    // MAPPS-554: on a tenant subdomain the mokosh-workspace login is
    // NOT reachable. Portal admins have no users row (post-554
    // provisioning creates a contacts row only), so `StandaloneLogin`
    // mokosh-contact-login: PortalLogin redirect retired with the
    // /portal/* route family (prompt 001). Contact login lands under
    // /portal/{slug}/login in prompt 005.

    // MAPPS-368: no OIDC issuer configured -> present the standalone
    // username/password form instead of the bunyip redirect. `is_standalone`
    // is stable for the session (config is memoized), so the effect below is
    // still called on every render and the hook order never changes.
    let standalone = crate::modules::oidc::is_standalone();
    use_effect(move || {
        if !standalone {
            let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
            // MAPPS-432: this is where a recoverable callback error restarts to,
            // so a swallowed failure here would strand the user on the
            // placeholder below with nothing in the console to explain it.
            if let Err(e) = crate::modules::oidc::start_login(&cfg, "/dashboard") {
                crate::modules::oidc::log_auth_error(&format!("login page: kickoff failed: {e}"));
            }
        }
    });
    if standalone {
        return rsx! { crate::pages::login::StandaloneLogin {} };
    }
    rsx! {
        div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
            "Signing you in…"
        }
    }
}

#[component]
fn AuthCallback() -> Element {
    rsx! { auth_callback::AuthCallbackPage {} }
}

// MAPPS-497 item 6: route wrappers so the standalone login's
// picker + create-org steps have their own URLs (better back-button
// + deep-link behavior than the previous inline shape).
#[component]
fn PickTenant() -> Element {
    rsx! { crate::pages::pick_tenant::PickTenantPage {} }
}

#[component]
fn CreateOrg() -> Element {
    rsx! { crate::pages::create_org::CreateOrgPage {} }
}

#[component]
fn ForgotPassword() -> Element {
    // MAPPS-510: standalone deploys have no bunyip hub to redirect to;
    // render the local form and POST to mokosh-server directly.
    if crate::modules::oidc::is_standalone() {
        return rsx! { crate::pages::forgot_password::StandaloneForgotPassword {} };
    }
    rsx! { HubRedirect { target: "/forgot-password".to_string(), label: "password reset" } }
}

#[component]
fn ResetPassword(token: String) -> Element {
    // MAPPS-510: standalone deploys own the reset flow locally (the
    // token was minted by mokosh-server's `AuthService::reset_password`
    // path, and no bunyip is running to redeem it).
    if crate::modules::oidc::is_standalone() {
        return rsx! { crate::pages::reset_password::StandaloneResetPassword { token } };
    }
    rsx! { HubRedirect { target: format!("/reset-password/{token}"), label: "password reset" } }
}

// MAPPS-552: first-time password setup for a fresh client-admin.
// Landing page for the welcome-email link. Post MAPPS-551 the setup
// token redeems through the same `POST /auth/reset-password` handler
// as forgot-password, so the URL split is UI-only: a distinct page
// with copy that names the specific client portal ("Set your password
// for [Client Name]"), instead of the confusing "Reset password" the
// welcome recipient used to see.
#[component]
fn SetPassword(token: String) -> Element {
    // Standalone deploys own the setup flow locally, same posture as
    // ResetPassword above. HubRedirect for bunyip-configured deploys
    // keeps the token in the URL so bunyip's own set-password surface
    // (when one exists) can pick it up; today it falls back to the
    // same /reset-password shape until a hub-side companion lands.
    if crate::modules::oidc::is_standalone() {
        return rsx! { crate::pages::set_password::SetPasswordPage { token } };
    }
    rsx! { HubRedirect { target: format!("/set-password/{token}"), label: "password setup" } }
}

#[component]
fn InviteAccept(token: String) -> Element {
    rsx! { HubRedirect { target: format!("/invitations/accept?token={token}"), label: "invite accept" } }
}

#[component]
fn Signup() -> Element {
    rsx! { HubRedirect { target: "/signup".to_string(), label: "sign up" } }
}

#[component]
fn SignupComplete(token: String) -> Element {
    rsx! { HubRedirect { target: format!("/signup/{token}"), label: "sign up" } }
}

#[component]
fn Dashboard() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            dashboards_view::DefaultDashboardPage {}
        }
    }
}

#[component]
fn SavedDashboards() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            dashboards::SavedDashboardsPage {}
        }
    }
}

#[component]
fn SavedDashboardView(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            dashboards_view::SavedDashboardViewPage { id }
        }
    }
}

#[component]
fn DashboardTv() -> Element {
    rsx! { dashboard::DashboardTvPage {} }
}

#[component]
fn Onboarding() -> Element {
    rsx! { onboarding::Onboarding {} }
}

#[component]
fn TicketList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            tickets::TicketListPage {}
        }
    }
}

#[component]
fn TicketNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            tickets::TicketNewPage {}
        }
    }
}

#[component]
fn TicketDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            tickets::TicketDetailPage { id }
        }
    }
}

#[component]
fn TimeEntryList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            time::TimeEntryListPage {}
        }
    }
}

#[component]
fn TimeEntryNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            time::TimeEntryNewPage {}
        }
    }
}

#[component]
fn Timesheets() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            time::TimesheetsPage {}
        }
    }
}

#[component]
fn TimesheetApprovals() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            time::TimesheetApprovalsPage {}
        }
    }
}

#[component]
fn Approvals() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            approvals::ApprovalsPage {}
        }
    }
}

#[component]
fn ProjectList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            projects::ProjectListPage {}
        }
    }
}

#[component]
fn ProjectNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            projects::ProjectNewPage {}
        }
    }
}

#[component]
fn ProjectDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            projects::ProjectDetailPage { id }
        }
    }
}

#[component]
fn ProjectTasks(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            projects::ProjectTasksPage { id }
        }
    }
}

#[component]
fn CompanyList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contacts::CompanyListPage {}
        }
    }
}

#[component]
fn CompanyNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contacts::CompanyNewPage {}
        }
    }
}

#[component]
fn CompanyDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contacts::CompanyDetailPage { id }
        }
    }
}

#[component]
fn CompanyEdit(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contacts::CompanyEditPage { id }
        }
    }
}

// MAPPS-590 (mokosh-contact-login prompt 012): thin wrapper so the
// `Routable` derive resolves `Route::CompanyRoleEdit { company_id, id }`
// to the CompanyRoleEditPage component. Two positional path params
// (company_id + id) are forwarded verbatim.
#[component]
fn CompanyRoleEdit(company_id: String, id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            company_role_edit::CompanyRoleEditPage { company_id, id }
        }
    }
}

#[component]
fn ContactList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contacts::ContactListPage {}
        }
    }
}

#[component]
fn ContactNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contacts::ContactNewPage {}
        }
    }
}

#[component]
fn ContactDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contacts::ContactDetailPage { id }
        }
    }
}

#[component]
fn ContactEdit(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contacts::ContactEditPage { id }
        }
    }
}

#[component]
fn Calendar() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            calendar::CalendarPage {}
        }
    }
}

#[component]
fn DispatchBoard() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            calendar::DispatchBoardPage {}
        }
    }
}

#[component]
fn SchedulingTemplates() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            calendar::SchedulingTemplatesPage {}
        }
    }
}

// MAPPS-302: NOC "Big View" route handlers.
#[component]
fn BigTickets() -> Element {
    rsx! { big_view::BigTicketsPage {} }
}

#[component]
fn BigDispatch() -> Element {
    rsx! { big_view::BigDispatchPage {} }
}

#[component]
fn BigCalendar() -> Element {
    rsx! { big_view::BigCalendarPage {} }
}

#[component]
fn QuoteList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            quotes::QuoteListPage {}
        }
    }
}

#[component]
fn QuoteNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            quotes::QuoteNewPage {}
        }
    }
}

#[component]
fn QuoteDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            quotes::QuoteDetailPage { id }
        }
    }
}

#[component]
fn QuoteEdit(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            quotes::QuoteEditPage { id }
        }
    }
}

#[component]
fn ContractList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contracts::ContractListPage {}
        }
    }
}

#[component]
fn ContractNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contracts::ContractNewPage {}
        }
    }
}

#[component]
fn ContractDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contracts::ContractDetailPage { id }
        }
    }
}

#[component]
fn ContractEdit(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contracts::ContractEditPage { id }
        }
    }
}

#[component]
fn RateCardList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contracts::RateCardListPage {}
        }
    }
}

#[component]
fn RateCardNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contracts::RateCardListPage { open_create: true }
        }
    }
}

#[component]
fn RateCardDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contracts::RateCardDetailPage { id }
        }
    }
}

#[component]
fn InvoiceList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            billing::InvoiceListPage {}
        }
    }
}

#[component]
fn InvoiceNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            billing::InvoiceNewPage {}
        }
    }
}

#[component]
fn InvoiceDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            billing::InvoiceDetailPage { id }
        }
    }
}

#[component]
fn PaymentList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            billing::PaymentListPage {}
        }
    }
}

#[component]
fn TaxRateList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            billing::TaxRateListPage {}
        }
    }
}

#[component]
fn PaymentGatewayConfig() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            billing::PaymentGatewayConfigPage {}
        }
    }
}

#[component]
fn CreditNoteList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            credit_notes::CreditNoteListPage {}
        }
    }
}

#[component]
fn CreditNoteDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            credit_notes::CreditNoteDetailPage { id }
        }
    }
}

#[component]
fn Statement() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            statements::StatementPage {}
        }
    }
}

#[component]
fn AssetList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            assets::AssetListPage {}
        }
    }
}

#[component]
fn AssetNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            assets::AssetNewPage {}
        }
    }
}

#[component]
fn AssetDetail(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            assets::AssetDetailPage { id }
        }
    }
}

#[component]
fn KBHome() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            knowledge_base::KBHomePage {}
        }
    }
}

#[component]
fn KBArticleList(q: String, tag: String, category: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            knowledge_base::KBArticleListPage { initial_q: q, initial_tag: tag, initial_category: category }
        }
    }
}

#[component]
fn KBArticleNew() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            knowledge_base::KBArticleNewPage {}
        }
    }
}

// MAPPS-624: no `max-w-7xl mx-auto` wrapper, deliberately. The reading view
// has a left tree rail, the article body and a right rail, and at 1280px the
// body column was the one that lost the space. It fills whatever width `main`
// gives it.
#[component]
fn KBArticleDetail(id: String) -> Element {
    rsx! { knowledge_base::KBArticleDetailPage { id } }
}

#[component]
fn KBArticleEdit(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            knowledge_base::KBArticleEditPage { id }
        }
    }
}

#[component]
fn Reports() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            reports::ReportsPage {}
        }
    }
}

#[component]
fn ReportDetail(report_type: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            reports::ReportDetailPage { report_type }
        }
    }
}

#[component]
fn ActiveTenant() -> Element {
    // Tenant switching moved to the bunyip hub per
    // docs/migration/settings-split.md. Bookmarks at the legacy URL
    // bounce there instead of 404ing.
    rsx! {
        div { class: "max-w-7xl mx-auto",
            HubRedirect { target: "/settings/active-tenant".to_string(), label: "tenant switcher" }
        }
    }
}

#[component]
fn Profile() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            profile::ProfilePage {}
        }
    }
}

// MAPPS-169 Settings hub + sub-routes. The type editors are net-new
// (pages::settings); the re-homed surfaces reuse the existing page
// components so there is one source of truth per surface.
#[component]
fn SettingsHome() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::SettingsHomePage {}
        }
    }
}

/// MAPPS-620 (mokosh-branding prompt 004): wire the contact-plane
/// portal branding editor into the AppShell-scoped router. Page owns
/// its own capability gate + load state.
#[component]
fn ContactPortalBranding() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            pages::contact_portal::portal_branding::ContactPortalBrandingPage {}
        }
    }
}

/// MAPPS-622: staff-side tenant branding editor.
#[component]
fn SettingsBranding() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            pages::settings_branding::SettingsBrandingPage {}
        }
    }
}

// MAPPS-258 per-group landing pages.
#[component]
fn SettingsGroupServiceTypes() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::ServiceTypesGroupPage {}
        }
    }
}

#[component]
fn SettingsGroupBilling() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::BillingGroupPage {}
        }
    }
}

#[component]
fn SettingsGroupTickets() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::TicketsGroupPage {}
        }
    }
}

#[component]
fn SettingsGroupIntegrations() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::IntegrationsGroupPage {}
        }
    }
}

#[component]
fn SettingsGroupData() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::DataGroupPage {}
        }
    }
}

#[component]
fn SettingsWorkTypes() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::WorkTypesSettingsPage {}
        }
    }
}

#[component]
fn SettingsTaskStatuses() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::TaskStatusesSettingsPage {}
        }
    }
}

#[component]
fn SettingsAssetTypes() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::AssetTypesSettingsPage {}
        }
    }
}

#[component]
fn SettingsCompanyIndustries() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::CompanyIndustriesSettingsPage {}
        }
    }
}

#[component]
fn SettingsProjectTypes() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::ProjectTypesSettingsPage {}
        }
    }
}

#[component]
fn SettingsPaymentTerms() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::PaymentTermsSettingsPage {}
        }
    }
}

#[component]
fn SettingsProducts() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            products::ProductsSettingsPage {}
        }
    }
}

#[component]
fn SettingsSla() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            sla::SlaManagementPage {}
        }
    }
}

#[component]
fn SettingsScheduling() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::SchedulingSettingsPage {}
        }
    }
}

#[component]
fn SettingsAppearance() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::AppearanceSettingsPage {}
        }
    }
}

#[component]
fn SettingsTvView() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::TvViewSettingsPage {}
        }
    }
}

#[component]
fn SettingsTimeTracking() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::MaxHoursPerDaySettingsPage {}
        }
    }
}

#[component]
fn SettingsRateCards() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            contracts::RateCardListPage {}
        }
    }
}

#[component]
fn SettingsTaxRates() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            billing::TaxRateListPage {}
        }
    }
}

#[component]
fn SettingsGateways() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            billing::PaymentGatewayConfigPage {}
        }
    }
}

#[component]
fn SettingsImportExport() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::ImportExportSettingsPage {}
        }
    }
}

#[component]
fn SettingsOrganization() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::OrganizationSettingsPage {}
        }
    }
}

// mokosh-contact-login prompt 007: Settings > Contact Roles list +
// edit. Two wrappers so the routing derive sees the correct component
// names for `/settings/contact-roles` and `/settings/contact-roles/:id`.
#[component]
fn ContactRolesList() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings_contact_roles::ContactRolesListPage {}
        }
    }
}

#[component]
fn ContactRoleEdit(id: String) -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings_contact_roles::ContactRoleEditPage { id }
        }
    }
}

// MAPPS-172 ticket lookup editors.
#[component]
fn SettingsTicketStatuses() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::TicketStatusesSettingsPage {}
        }
    }
}

#[component]
fn SettingsTicketPriorities() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::TicketPrioritiesSettingsPage {}
        }
    }
}

#[component]
fn SettingsTicketTypes() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::TicketTypesSettingsPage {}
        }
    }
}

#[component]
fn SettingsTicketQueues() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::TicketQueuesSettingsPage {}
        }
    }
}

#[component]
fn SettingsTicketCategories() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::TicketCategoriesSettingsPage {}
        }
    }
}

// MAPPS-199 RMM integration admin UI.
#[component]
fn SettingsRmmConnections() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::RmmConnectionsSettingsPage {}
        }
    }
}

#[component]
fn SettingsRmmDeviceMappings() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::RmmDeviceMappingsSettingsPage {}
        }
    }
}

#[component]
fn SettingsRmmAlertRules() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            settings::RmmAlertRulesSettingsPage {}
        }
    }
}

#[component]
fn SystemStatus() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            system_status::SystemStatusPage {}
        }
    }
}

#[component]
fn ButtonShowcase() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            button_showcase::ButtonShowcasePage {}
        }
    }
}

#[component]
fn AuditLog() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            audit_log::AuditLogPage {}
        }
    }
}

#[component]
fn FormsBuilder() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            forms::FormsBuilderPage {}
        }
    }
}

#[component]
fn SlaManagement() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            sla::SlaManagementPage {}
        }
    }
}

#[component]
fn Invitations() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            invitations::InvitationsPage {}
        }
    }
}

/// PMS-791 phase 2: legacy bookmarks for `/admin/team` land here and
/// bounce to the new `/admin/invitations` route. Delete after the
/// release after this one; the redirect is only there so a customer with
/// a saved link does not hit a 404 the day this ships.
#[component]
fn TeamLegacyRedirect() -> Element {
    #[cfg(feature = "web")]
    {
        let nav = use_navigator();
        nav.replace(Route::Invitations {});
    }
    rsx! {
        div { class: "max-w-7xl mx-auto min-h-screen flex items-center justify-center text-sm text-muted",
            "Redirecting to Invitations…"
        }
    }
}

#[cfg(feature = "multi-tenant")]
#[component]
fn Teams() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            teams::TeamsPage {}
        }
    }
}

// mokosh-contact-login: TenantManagement wrapper retired with the
// Clients tab (prompt 001). admin::TenantManagementPage stays in the
// admin.rs file as dead code for a follow-up cleanup.

// mokosh-contact-login: all pre-pivot Portal* route wrapper components
// retired with the customer-portal /portal/* routes (prompt 001). The
// contact-plane replacements below land per prompt 005 under a new
// route family (`ContactLogin` etc.) with no PortalGuard layout.

// MAPPS-589 (prompt 011): the two new public login routes.
//
// - `ContactGenericLogin` at `/portal/login` renders the three-field
//   Portal ID + email + password form.
// - `ContactHandleLogin` at `/portal/:handle/login` collapses the
//   legacy `/portal/:slug/login` and the new
//   `/portal/:portal_id/login` into ONE route so we do not ship two
//   `/portal/:X/login` shapes (Dioxus would not be able to
//   deterministically pick between them). The wrapper inspects the
//   `handle` at render time: a 9-digit numeric handle mounts the
//   `ContactLoginByPortalIdPage`; anything else falls through to the
//   legacy `ContactLoginPage`, which itself fires the slug ->
//   portal_id resolve-and-redirect on mount (see
//   `src/pages/contact_portal/login.rs`). Live invitation emails
//   built against a slug URL continue to work through this path.
#[component]
fn ContactGenericLogin() -> Element {
    rsx! { contact_portal::generic_login::ContactGenericLoginPage {} }
}

#[component]
fn ContactHandleLogin(handle: String) -> Element {
    if contact_portal::generic_login::handle_is_portal_id_shape(&handle) {
        rsx! { contact_portal::portal_id_login::ContactLoginByPortalIdPage { portal_id: handle } }
    } else {
        rsx! { contact_portal::login::ContactLoginPage { slug: handle } }
    }
}

#[component]
fn ContactSetPassword(slug: String, token: String) -> Element {
    rsx! { contact_portal::set_password::ContactSetPasswordPage { slug, token } }
}

#[component]
fn ContactForgotPassword(slug: String) -> Element {
    rsx! { contact_portal::forgot_password::ContactForgotPasswordPage { slug } }
}

#[component]
fn ContactResetPassword(slug: String, token: String) -> Element {
    rsx! { contact_portal::reset_password::ContactResetPasswordPage { slug, token } }
}

// MAPPS-572 (prompt 010): the slug-less magic-link finder + the
// picker/redemption landing page. Both public (no AuthGuard). The
// finder's `?:email` query segment is optional; when absent the router
// hands the wrapper an empty string, which the page treats as "no
// pre-fill".
#[component]
fn ContactMagicLinkLogin(email: String) -> Element {
    rsx! { contact_portal::magic_link_login::ContactMagicLinkLoginPage { email } }
}

#[component]
fn ContactPicker(token: String) -> Element {
    rsx! { contact_portal::picker::ContactPickerPage { token } }
}

// mokosh-contact-login: PortalForgotPassword / PortalResetPassword
// wrappers retired with the customer-portal route family (prompt 001).
// Contact-plane replacements live under ContactForgotPassword /
// ContactResetPassword above.

#[component]
fn RequestForm(token: String) -> Element {
    rsx! { request_form::RequestFormPage { token } }
}

#[component]
fn NotFound(route: Vec<String>) -> Element {
    rsx! { not_found::NotFoundPage { route } }
}

/// MAPPS-396 recurrence gate: every link mokosh-server builds on the SPA
/// origin (`CLIENT_ORIGIN`, wired into the services in `src/api/router.rs`)
/// and emails to a user must resolve to a real route here, not the catch-all
/// `NotFound`. The `/portal/set-password` link had been in customers' inboxes
/// since PMS-136 with no page behind it.
///
/// The list is the full set of emitters on mokosh-server `main`, verified at
/// ff429b3c. Adding an emailed link server-side without adding its route here
/// fails this test.
#[cfg(test)]
mod emailed_link_routes {
    use super::Route;
    use std::str::FromStr;

    /// (server emitter, path as the customer receives it). Path parameters
    /// carry a representative value; the query string is dropped because the
    /// router matches on the path.
    // mokosh-contact-login: /portal/set-password + /portal/quotes/{id}
    // links were emitted by retired portal + quote-sign-off flows
    // (prompt 001). Contact-plane replacement lands in prompt 005.
    const EMAILED_LINKS: &[(&str, &str)] = &[
        // src/modules/forms/request_links.rs: client request-form link
        // (PMS-730). The token is `{token_id}.{secret}`.
        (
            "forms::issue_request_link",
            "/request-forms/2f1c2f1e-0000-4000-8000-00000000abcd.Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9",
        ),
        // src/modules/auth/service.rs: password reset + staff welcome links.
        (
            "auth::request_password_reset",
            "/reset-password/Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9",
        ),
        // src/modules/auth/service.rs (security notice) and
        // src/modules/invitations/service.rs (invite): the bare SPA origin.
        ("auth::security_notice / invitations::create", "/"),
    ];

    #[test]
    fn every_emailed_link_resolves_to_a_route() {
        for (emitter, path) in EMAILED_LINKS {
            let route = Route::from_str(path)
                .unwrap_or_else(|e| panic!("{emitter} emails {path}, which does not parse: {e}"));
            assert!(
                !matches!(route, Route::NotFound { .. }),
                "{emitter} emails {path}, which falls through to the 404 catch-all",
            );
        }
    }
}

// mokosh-contact-login: portal_login_route test retired with the
// /portal/* route family (prompt 001). Contact-plane replacements
// below (prompt 005) pin the four new route shapes end-to-end so a
// magic link built server-side round-trips through the router without
// stripping the slug or the token.
#[cfg(test)]
mod contact_route_gate {
    use super::pathname_is_contact_forbidden as f;

    #[test]
    fn crm_paths_are_blocked() {
        for p in [
            "/companies",
            "/companies/",
            "/companies/new",
            "/companies/abcd-1234",
            "/companies/abcd-1234/edit",
            "/contacts",
            "/contacts/new",
            "/contacts/abcd/edit",
        ] {
            assert!(f(p), "{p} should be forbidden");
        }
    }

    #[test]
    fn scheduling_and_ops_are_blocked() {
        for p in [
            "/calendar",
            "/dispatch",
            "/scheduling-templates",
            "/rate-cards",
            "/rate-cards/new",
            "/payments",
            "/reports",
            "/reports/timesheet",
            "/time",
            "/time/new",
            "/timesheets",
            "/timesheets/approvals",
        ] {
            assert!(f(p), "{p} should be forbidden");
        }
    }

    #[test]
    fn admin_and_platform_are_blocked() {
        for p in [
            "/admin",
            "/admin/audit",
            "/admin/team",
            "/dashboards",
            "/dashboards/x/view",
            "/dashboard/tv",
            "/big/tickets",
        ] {
            assert!(f(p), "{p} should be forbidden");
        }
    }

    #[test]
    fn settings_hub_and_contact_surfaces_pass() {
        for p in [
            "/dashboard",
            "/tickets",
            "/tickets/new",
            "/tickets/xyz",
            "/invoices",
            "/invoices/xyz",
            "/quotes",
            "/contracts",
            "/assets",
            "/assets/xyz",
            "/projects",
            "/kb",
            "/kb/articles",
            "/kb/articles/xyz",
            "/settings",
            "/settings/portal-branding",
            "/settings/appearance",
            "/profile",
            "/system-status",
            "/notifications",
        ] {
            assert!(!f(p), "{p} should be allowed for contacts");
        }
    }

    #[test]
    fn settings_staff_subroutes_are_blocked() {
        for p in [
            "/settings/branding",
            "/settings/work-types",
            "/settings/sla",
            "/settings/scheduling",
            "/settings/rate-cards",
            "/settings/tax-rates",
            "/settings/import-export",
            "/settings/contact-roles",
            "/settings/contact-roles/abc",
            "/settings/group/service-types",
        ] {
            assert!(f(p), "{p} should be forbidden");
        }
    }

    #[test]
    fn kb_edit_is_blocked_but_read_is_open() {
        assert!(f("/kb/articles/new"));
        assert!(f("/kb/articles/abc/edit"));
        assert!(!f("/kb/articles/abc"));
        assert!(!f("/kb"));
    }

    #[test]
    fn timesheets_does_not_leak_via_time_prefix() {
        // Naive prefix matching would fold "/timesheets" into the
        // "/time" entry; the entries are distinct on purpose.
        assert!(f("/timesheets"));
        assert!(f("/time"));
        assert!(f("/time/new"));
    }

    #[test]
    fn query_strings_and_trailing_slashes_are_normalised() {
        assert!(f("/companies?filter=abc"));
        assert!(f("/companies/"));
        assert!(!f("/settings?tab=personalization"));
    }
}

#[cfg(test)]
mod contact_portal_routes {
    use super::Route;
    use std::str::FromStr;

    // MAPPS-589 (prompt 011): the legacy slug route
    // `/portal/:slug/login` and the new portal_id route
    // `/portal/:portal_id/login` share the same `/portal/:X/login`
    // shape and are collapsed into ONE `ContactHandleLogin` route.
    // Both legacy Crockford-slug URLs and 9-digit-numeric Portal ID
    // URLs resolve here; the wrapper (`src/lib.rs
    // ContactHandleLogin`) dispatches at render time based on the
    // handle shape.
    #[test]
    fn login_with_legacy_slug_resolves_to_handle_route() {
        let route =
            Route::from_str("/portal/K3F9M7N2Q8XR5J4W/login").expect("legacy slug login parses");
        match route {
            Route::ContactHandleLogin { handle } => {
                assert_eq!(handle, "K3F9M7N2Q8XR5J4W");
            }
            other => panic!("expected ContactHandleLogin, got {other:?}"),
        }
    }

    #[test]
    fn login_with_portal_id_resolves_to_handle_route() {
        let route = Route::from_str("/portal/555556666/login").expect("portal-id login parses");
        match route {
            Route::ContactHandleLogin { handle } => {
                assert_eq!(handle, "555556666");
            }
            other => panic!("expected ContactHandleLogin, got {other:?}"),
        }
    }

    #[test]
    fn generic_login_resolves() {
        let route = Route::from_str("/portal/login").expect("generic login parses");
        match route {
            Route::ContactGenericLogin {} => {}
            other => panic!("expected ContactGenericLogin, got {other:?}"),
        }
    }

    #[test]
    fn set_password_carries_slug_and_token() {
        let route =
            Route::from_str("/portal/abc/set-password?token=xyz").expect("set-password parses");
        match route {
            Route::ContactSetPassword { slug, token } => {
                assert_eq!(slug, "abc");
                assert_eq!(token, "xyz");
            }
            other => panic!("expected ContactSetPassword, got {other:?}"),
        }
    }

    #[test]
    fn forgot_password_resolves_with_slug() {
        let route = Route::from_str("/portal/abc/forgot-password").expect("forgot-password parses");
        match route {
            Route::ContactForgotPassword { slug } => assert_eq!(slug, "abc"),
            other => panic!("expected ContactForgotPassword, got {other:?}"),
        }
    }

    #[test]
    fn reset_password_carries_slug_and_token() {
        let route =
            Route::from_str("/portal/abc/reset-password?token=xyz").expect("reset-password parses");
        match route {
            Route::ContactResetPassword { slug, token } => {
                assert_eq!(slug, "abc");
                assert_eq!(token, "xyz");
            }
            other => panic!("expected ContactResetPassword, got {other:?}"),
        }
    }

    // MAPPS-572 (prompt 010): pin the two slug-less magic-link routes.
    // Same shape as the four routes above (query-segment `?:token` /
    // `?:email` per MAPPS-560) so the emailed magic link a server
    // builds under a bare `/portal/pick?token=...` URL round-trips
    // through the router without stripping the token.
    //
    // MAPPS-589 (prompt 011): the finder path moved from
    // `/portal/login` to `/portal/find?:email` so the shorter path
    // can host the primary three-field password page. The picker's
    // token-carrying URL is unchanged.
    #[test]
    fn magic_link_login_resolves() {
        let route = Route::from_str("/portal/find").expect("magic-link login parses");
        match route {
            Route::ContactMagicLinkLogin { email } => assert_eq!(email, ""),
            other => panic!("expected ContactMagicLinkLogin, got {other:?}"),
        }
    }

    #[test]
    fn magic_link_login_carries_email() {
        let route = Route::from_str("/portal/find?email=alice%40example.com")
            .expect("magic-link login with email parses");
        match route {
            Route::ContactMagicLinkLogin { email } => {
                assert_eq!(email, "alice@example.com");
            }
            other => panic!("expected ContactMagicLinkLogin, got {other:?}"),
        }
    }

    #[test]
    fn picker_carries_token() {
        let route = Route::from_str("/portal/pick?token=xyz").expect("picker parses");
        match route {
            Route::ContactPicker { token } => assert_eq!(token, "xyz"),
            other => panic!("expected ContactPicker, got {other:?}"),
        }
    }

    /// PMS-832 / MAPPS-538: the password-reset pair resolves, and resolves to
    /// the PORTAL pages.
    ///
    /// The emailed link landing on the 404 catch-all is the defect this work
    /// fixes, and `emailed_link_routes` above now covers that. What it cannot
    /// see is the other half: `/reset-password/{token}` is the PLATFORM page,
    /// which posts to `/api/v1/auth/reset-password` and resolves the token
    /// against `users`. A portal customer reaching that page resets a staff
    /// login, which is the PMS-820 defect exactly. These paths differ by one
    /// prefix, so the two are asserted apart rather than assumed.
    #[test]
    fn the_portal_reset_pages_resolve_and_are_not_the_platform_one() {
        let reset =
            Route::from_str("/portal/reset-password").expect("/portal/reset-password parses");
        assert!(
            matches!(reset, Route::ContactResetPassword { .. }),
            "the emailed portal link must land on the portal page, got {reset:?}"
        );

        let forgot =
            Route::from_str("/portal/forgot-password").expect("/portal/forgot-password parses");
        assert!(
            matches!(forgot, Route::ContactForgotPassword { .. }),
            "/portal/forgot-password must resolve to PortalForgotPassword, got {forgot:?}"
        );

        // The platform page is still its own route, one prefix away.
        let platform = Route::from_str("/reset-password/Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9")
            .expect("the platform reset route parses");
        assert!(
            !matches!(platform, Route::ContactResetPassword { .. }),
            "the platform reset link must not resolve to the portal page: it posts to \
             /api/v1/auth/reset-password, which resolves the token against `users`"
        );
    }
}

/// MAPPS-526: every `/admin/*` route's page carries the role gate the route
/// table above claims it does. `/admin/forms` was added by PMS-731 with the
/// comment and no gate, so a technician reached a full form editor whose every
/// save 403s server-side. The claim is a test now: a new `/admin/*` route
/// added without a gate fails here rather than shipping.
///
/// The check is a source scan rather than a render, because the pages are
/// Dioxus components that need a running virtual DOM and an auth context to
/// render at all, and what is being asserted is structural: the page consults
/// the role before deciding what to show.
#[cfg(test)]
mod admin_route_role_gates {
    use std::collections::BTreeSet;

    /// (route path, page source path, page source). One entry per `/admin/*`
    /// route in the `Route` enum, `#[cfg]`-gated ones included.
    const ADMIN_ROUTE_PAGES: &[(&str, &str, &str)] = &[
        (
            "/admin/audit",
            "src/pages/audit_log.rs",
            include_str!("pages/audit_log.rs"),
        ),
        (
            "/admin/forms",
            "src/pages/forms.rs",
            include_str!("pages/forms.rs"),
        ),
        (
            "/admin/sla",
            "src/pages/sla.rs",
            include_str!("pages/sla.rs"),
        ),
        (
            "/admin/tenants",
            "src/pages/admin.rs",
            include_str!("pages/admin.rs"),
        ),
    ];

    /// `/admin/*` paths declared in this file's `Route` enum. Read from the
    /// source rather than from `Route`, so a `#[cfg]`-gated route counts in
    /// every build instead of vanishing from the check with its feature.
    fn declared_admin_routes() -> BTreeSet<String> {
        include_str!("lib.rs")
            .lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("#[route(\"/admin/")?;
                let path = rest.split('"').next()?;
                Some(format!("/admin/{path}"))
            })
            .collect()
    }

    #[test]
    fn every_admin_route_has_a_table_entry() {
        let declared = declared_admin_routes();
        let tabled: BTreeSet<String> = ADMIN_ROUTE_PAGES
            .iter()
            .map(|(route, _, _)| (*route).to_string())
            .collect();

        let missing: Vec<&String> = declared.difference(&tabled).collect();
        assert!(
            missing.is_empty(),
            "these /admin/* routes have no entry in ADMIN_ROUTE_PAGES, so nothing checks their role gate: {missing:?}"
        );

        let stale: Vec<&String> = tabled.difference(&declared).collect();
        assert!(
            stale.is_empty(),
            "ADMIN_ROUTE_PAGES lists routes the Route enum no longer declares: {stale:?}"
        );
    }

    /// How a page reads the caller's role. `/admin/tenants` reads super-admin
    /// because its server endpoint takes `RequireSuperAdmin`, not `RequireAdmin`.
    const ROLE_READS: &[&str] = &["is_admin", "is_super_admin"];

    /// How a page refuses on that role. The gate has to be a refusal: reading
    /// the role and rendering the page anyway is what `/admin/forms` did.
    const ROLE_REFUSALS: &[&str] = &["if !is_admin", "if !use_is_admin()", "if !is_super_admin"];

    #[test]
    fn every_admin_page_has_a_role_gate() {
        for (route, path, source) in ADMIN_ROUTE_PAGES {
            assert!(
                ROLE_READS.iter().any(|pat| source.contains(pat)),
                "{route} renders {path}, which never reads the user's role. \
                 Gate it like src/pages/audit_log.rs does (`u.role.is_admin()`)."
            );
            assert!(
                ROLE_REFUSALS.iter().any(|pat| source.contains(pat)),
                "{route} renders {path}, which reads the user's role but never refuses on it. \
                 Return the access-denied view for a non-admin, as src/pages/audit_log.rs does."
            );
        }
    }
}

/// Prelude module for common imports
pub mod prelude {
    pub use crate::modules::auth::CurrentUser;
    pub use crate::utils::error::{AppError, AppResult};
    pub use crate::Route;
    pub use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
    pub use serde::{Deserialize, Serialize};
    pub use uuid::Uuid;
    pub use validator::Validate;
}
