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
        // MAPPS-368: a deployment with no OIDC issuer has no bunyip OP to
        // redirect to, so send the user to the standalone username/password
        // login form instead of a dead `/oauth2/authorize`.
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

/// MAPPS-395: layout component gating the client-portal routes.
///
/// The portal runs on its own identity (a `contacts` row) and its own token
/// class: mokosh-server's `portal_auth_middleware` rejects any bearer whose
/// `typ` is not `portal_access`, so an agent session is worth exactly as much
/// here as no session at all. Both cases redirect to `/portal/login` rather
/// than rendering a page whose every fetch would 401.
///
/// Declared before the `Route` enum for the same reason [`AuthGuard`] is: the
/// `Routable` derive expands `#[layout(PortalGuard)]` at the enum site.
#[component]
pub fn PortalGuard() -> Element {
    let nav = use_navigator();
    #[cfg(feature = "app")]
    let signed_in = hooks::fetch::api::has_portal_session();
    // The portal fetch helpers only exist in the `app` build, so a non-`app`
    // build has no portal session to hold.
    #[cfg(not(feature = "app"))]
    let signed_in = false;
    if !signed_in {
        nav.replace(Route::PortalLogin {});
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                "Redirecting to the portal sign-in…"
            }
        };
    }
    rsx! {
        Outlet::<Route> {}
    }
}

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

    #[route("/login")]
    Login {},

    #[route("/auth/callback")]
    AuthCallback {},

    #[route("/forgot-password")]
    ForgotPassword {},

    #[route("/reset-password/:token")]
    ResetPassword { token: String },

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
    #[route("/admin/team")]
    Team {},

    // Admin (multi-tenant only)
    #[cfg(feature = "multi-tenant")]
    #[route("/admin/tenants")]
    TenantManagement {},

    // MAPPS-366: close the AppShell layout. Every route above (from Dashboard
    // down) renders inside the persistent shell; the chromeless routes at the
    // top of the AuthGuard block and the portal routes below do not.
    #[end_layout]

    // End of AuthGuard scope. Portal routes have their own layout and
    // auth model (client portal vs internal tools); the catch-all 404
    // is intentionally public so logged-out users see a real 404 page.
    #[end_layout]

    // Client Portal Routes (separate layout)
    //
    // The two public entry points come first: both are reachable without a
    // portal session by construction, so they sit outside `PortalGuard`.

    // MAPPS-395: portal sign-in. Issues the `typ: "portal_access"` token the
    // guarded routes below need.
    #[route("/portal/login")]
    PortalLogin {},

    // MAPPS-396: the destination of the portal setup email mokosh-server
    // sends on a portal-access grant. Public by construction: the emailed
    // single-use token in `?token=` is the only credential the visitor has.
    #[route("/portal/set-password")]
    PortalSetPassword {},

    // PMS-832: the destination of the portal password-reset email PMS-820
    // added. Public for the same reason as its sibling above: the emailed
    // single-use token in `?token=` is the only credential the visitor has, and
    // a visitor who has forgotten their password has no portal session to
    // satisfy `PortalGuard` with.
    #[route("/portal/reset-password")]
    PortalResetPassword {},

    // PMS-832: where a customer asks for that email. Public by construction:
    // someone who has forgotten their password has no portal session, so a
    // `PortalGuard` above this route would bounce the only people who need it.
    #[route("/portal/forgot-password")]
    PortalForgotPassword {},

    // MAPPS-395: everything below needs a portal session. Without the guard a
    // signed-out visitor (or an agent, whose bearer is the wrong token class)
    // renders the page and collects a 401 from every fetch.
    #[layout(PortalGuard)]
    #[route("/portal")]
    PortalHome {},

    #[route("/portal/tickets")]
    PortalTicketList {},

    #[route("/portal/tickets/new")]
    PortalTicketNew {},

    #[route("/portal/tickets/:id")]
    PortalTicketDetail { id: String },

    #[route("/portal/quotes")]
    PortalQuoteList {},

    #[route("/portal/quotes/:id")]
    PortalQuoteDetail { id: String },

    #[route("/portal/invoices")]
    PortalInvoiceList {},

    #[route("/portal/invoices/:id")]
    PortalInvoiceDetail { id: String },

    #[route("/portal/kb")]
    PortalKB {},

    // End of PortalGuard scope.
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
#[component]
fn Login() -> Element {
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

#[component]
fn ForgotPassword() -> Element {
    rsx! { HubRedirect { target: "/forgot-password".to_string(), label: "password reset" } }
}

#[component]
fn ResetPassword(token: String) -> Element {
    rsx! { HubRedirect { target: format!("/reset-password/{token}"), label: "password reset" } }
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

// MAPPS-652: no `max-w-7xl mx-auto` wrapper, deliberately, and the same
// reasoning as `KBArticleEdit` below. See the comment there.
#[component]
fn KBArticleNew() -> Element {
    rsx! { knowledge_base::KBArticleNewPage {} }
}

// MAPPS-624: no `max-w-7xl mx-auto` wrapper, deliberately. The reading view
// has a left tree rail, the article body and a right rail, and at 1280px the
// body column was the one that lost the space. It fills whatever width `main`
// gives it.
#[component]
fn KBArticleDetail(id: String) -> Element {
    rsx! { knowledge_base::KBArticleDetailPage { id } }
}

// MAPPS-652: no `max-w-7xl mx-auto` wrapper, deliberately. `mx-auto` splits
// whatever `main` has over 1280px into two equal margins, so the editor's
// edges were a function of the SHELL's width: collapsing the sidebar handed
// back 12rem and the centred column just slid 6rem right instead of growing.
// Without the cap the panel's edges are `main`'s own content box, which starts
// where the sidebar ends, so collapsing the rail widens the writing area by
// exactly what it gave back. `main` keeps `px-4 sm:px-6 lg:px-8`, which is the
// only inset left.
#[component]
fn KBArticleEdit(id: String) -> Element {
    rsx! { knowledge_base::KBArticleEditPage { id } }
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
fn Team() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            team::TeamPage {}
        }
    }
}

#[cfg(feature = "multi-tenant")]
#[component]
fn TenantManagement() -> Element {
    rsx! {
        div { class: "max-w-7xl mx-auto",
            admin::TenantManagementPage {}
        }
    }
}

#[component]
fn PortalHome() -> Element {
    rsx! { portal::PortalHomePage {} }
}

#[component]
fn PortalLogin() -> Element {
    rsx! { portal_login::PortalLoginPage {} }
}

#[component]
fn PortalSetPassword() -> Element {
    rsx! { portal_set_password::PortalSetPasswordPage {} }
}

#[component]
fn PortalForgotPassword() -> Element {
    rsx! { portal_forgot_password::PortalForgotPasswordPage {} }
}

#[component]
fn PortalResetPassword() -> Element {
    rsx! { portal_reset_password::PortalResetPasswordPage {} }
}

#[component]
fn RequestForm(token: String) -> Element {
    rsx! { request_form::RequestFormPage { token } }
}

#[component]
fn PortalTicketList() -> Element {
    rsx! { portal::PortalTicketListPage {} }
}

#[component]
fn PortalTicketNew() -> Element {
    rsx! { portal::PortalTicketNewPage {} }
}

#[component]
fn PortalTicketDetail(id: String) -> Element {
    rsx! { portal::PortalTicketDetailPage { id } }
}

#[component]
fn PortalQuoteList() -> Element {
    rsx! { portal::PortalQuoteListPage {} }
}

#[component]
fn PortalQuoteDetail(id: String) -> Element {
    rsx! { portal::PortalQuoteDetailPage { id } }
}

#[component]
fn PortalInvoiceList() -> Element {
    rsx! { portal::PortalInvoiceListPage {} }
}

#[component]
fn PortalInvoiceDetail(id: String) -> Element {
    rsx! { portal::PortalInvoiceDetailPage { id } }
}

#[component]
fn PortalKB() -> Element {
    rsx! { portal::PortalKBPage {} }
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
    const EMAILED_LINKS: &[(&str, &str)] = &[
        // src/modules/contacts/service.rs: portal-access grant setup link.
        ("contacts::send_setup_email", "/portal/set-password"),
        // src/modules/portal/service.rs: portal password-reset link (PMS-820).
        // The page behind it is PMS-832 / MAPPS-538; before that this emitter
        // was the one deliberate omission from this list, because listing a
        // link with no route behind it fails the very test the list feeds.
        ("portal::send_reset_email", "/portal/reset-password"),
        // src/modules/forms/request_links.rs: client request-form link
        // (PMS-730). The token is `{token_id}.{secret}`.
        (
            "forms::issue_request_link",
            "/request-forms/2f1c2f1e-0000-4000-8000-00000000abcd.Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9",
        ),
        // src/modules/quotes/service.rs: client quote sign-off link.
        (
            "quotes::send_quote_ready",
            "/portal/quotes/2f1c2f1e-0000-4000-8000-00000000abcd",
        ),
        // src/modules/billing/service.rs: invoice "Pay Now" link, sent to the
        // billing contact on the send transition (PMS-711). The page it lands
        // on got its Pay control in MAPPS-523.
        (
            "billing::notify_invoice_pay_now",
            "/portal/invoices/2f1c2f1e-0000-4000-8000-00000000abcd",
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

/// MAPPS-395: the portal sign-in route exists and is public. `/portal/login`
/// is where `PortalGuard` sends a visitor with no portal session, so a typo in
/// the route (or a stray `#[layout(PortalGuard)]` above it) would bounce the
/// redirect back into itself.
#[cfg(test)]
mod portal_login_route {
    use super::Route;
    use std::str::FromStr;

    #[test]
    fn portal_login_resolves_to_its_own_route() {
        let route = Route::from_str("/portal/login").expect("/portal/login parses");
        assert!(
            matches!(route, Route::PortalLogin {}),
            "/portal/login must resolve to PortalLogin, got {route:?}"
        );
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
            matches!(reset, Route::PortalResetPassword {}),
            "the emailed portal link must land on the portal page, got {reset:?}"
        );

        let forgot =
            Route::from_str("/portal/forgot-password").expect("/portal/forgot-password parses");
        assert!(
            matches!(forgot, Route::PortalForgotPassword {}),
            "/portal/forgot-password must resolve to PortalForgotPassword, got {forgot:?}"
        );

        // The platform page is still its own route, one prefix away.
        let platform = Route::from_str("/reset-password/Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9Zt4kQ1p9")
            .expect("the platform reset route parses");
        assert!(
            !matches!(platform, Route::PortalResetPassword {}),
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
            "/admin/team",
            "src/pages/team.rs",
            include_str!("pages/team.rs"),
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

/// MAPPS-652: the editing surface at a wide, a medium and a narrow viewport.
///
/// There is no browser in this suite, so each tier is asserted as the class
/// ladder that decides the layout at that width, in the same source-scan shape
/// `admin_route_role_gates` above uses. The layout spans four files - the shell
/// supplies the padding, `src/lib.rs` decides the width, the page states the
/// panel height and `MarkdownEditor` lays the panes out - and a change to any
/// one of them can undo the other three, which is why they are checked
/// together rather than one test per file.
///
/// `scripts/check-page-width.sh` is the other half: it fails if either editing
/// route takes the `max-w-7xl mx-auto` cap back, which is the regression these
/// tests would otherwise only catch through the `no cap` assertion below.
#[cfg(test)]
mod editing_surface_width {
    const LIB: &str = include_str!("lib.rs");
    const LAYOUT: &str = include_str!("components/layout.rs");
    const KB: &str = include_str!("pages/knowledge_base.rs");
    const EDITOR: &str = include_str!("components/markdown_editor.rs");
    const TOOLBAR: &str = include_str!("components/markdown_toolbar.rs");

    /// The routes whose whole job is editing a document.
    const EDITING_ROUTES: &[&str] = &["KBArticleNew", "KBArticleEdit"];

    /// A route component's body, from its signature to the closing brace. The
    /// comments ABOVE it are excluded on purpose: they name the classes the
    /// route does not carry, and reading those as the code would pass a route
    /// that never dropped the cap.
    fn route_body(name: &str) -> &'static str {
        let head = format!("\nfn {name}(");
        let start = LIB
            .find(&head)
            .unwrap_or_else(|| panic!("{name} is a route component in src/lib.rs"))
            + 1;
        let rest = &LIB[start..];
        let end = rest
            .find("\n}\n")
            .unwrap_or_else(|| panic!("{name}'s component never closes"));
        &rest[..end]
    }

    /// Wide: the editor gets the window, not a 1280px column centred in it.
    ///
    /// `max-w-7xl mx-auto` is what put dead margins either side of the writing
    /// area on a monitor with room for twice that.
    #[test]
    fn wide_gives_the_editor_the_window_rather_than_a_centred_column() {
        for name in EDITING_ROUTES {
            let body = route_body(name);
            assert!(
                !body.contains("max-w-"),
                "{name} is an editing surface and must not cap its width: {body}"
            );
            assert!(
                !body.contains("mx-auto"),
                "{name} must not centre itself either: auto margins are what turn \
                 spare width into dead space instead of writing area: {body}"
            );
        }
        assert!(
            EDITOR.contains("grid gap-4 grid-cols-1 grid-rows-2 lg:grid-cols-2 lg:grid-rows-1"),
            "and at `lg` the spare width buys a second pane side by side, which is \
             the whole reason Split is worth having on a wide screen"
        );
    }

    /// Medium: the ladder steps down rather than switching off. The shell's
    /// padding tightens and the metadata row is still multi-column.
    #[test]
    fn medium_steps_the_padding_down_and_keeps_the_metadata_row_in_columns() {
        assert!(
            LAYOUT.contains("px-4 sm:px-6 lg:px-8"),
            "`main` owns the only inset the editor has left, and it is a ladder: \
             a single fixed value here is the dead margin coming back"
        );
        assert!(
            KB.contains(r#"div { class: "grid grid-cols-1 gap-6 sm:grid-cols-3","#),
            "Category, Visibility and Status stay three-up from `sm`, so the \
             details block does not push the body down a screen at tablet width"
        );
        assert!(
            KB.contains(r#"panel_class: "h-[calc(100vh-16rem)] min-h-[26rem]".to_string()"#),
            "and the panel's height is still the window's minus the chrome beside \
             it, with a floor for a short one"
        );
    }

    /// Narrow: everything collapses to one column and nothing is unreachable.
    #[test]
    fn narrow_stacks_into_one_column_and_the_toolbar_wraps() {
        assert!(
            EDITOR.contains("grid-cols-1 grid-rows-2 lg:"),
            "below `lg` Split stacks the panes instead of halving a phone's width"
        );
        assert!(
            KB.contains("grid grid-cols-1 gap-6 sm:grid-cols-3"),
            "and the metadata row is one column before `sm`"
        );
        assert!(
            TOOLBAR.contains("flex flex-wrap items-center"),
            "the toolbar wraps rather than scrolling its later groups off the edge"
        );
    }

    /// AC3/AC4. The rail and the editor are flex siblings, so the 12rem a
    /// collapse gives back lands in `main`. With `mx-auto` still on the page it
    /// landed in the two margins instead, and collapsing the sidebar moved the
    /// editor sideways without widening it by a pixel.
    #[test]
    fn collapsing_the_sidebar_widens_the_editor_instead_of_its_margins() {
        assert!(
            LAYOUT
                .contains(r#"let desktop_width = if collapsed { "lg:w-16" } else { "lg:w-64" };"#),
            "the rail's width is the only thing the collapse changes"
        );
        assert!(
            LAYOUT.contains(r#"main { class: "flex-1 overflow-y-auto overscroll-contain py-6"#),
            "and `main` takes whatever it releases"
        );
        for name in EDITING_ROUTES {
            assert!(
                !route_body(name).contains("mx-auto"),
                "{name} would hand it straight back as margin"
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
