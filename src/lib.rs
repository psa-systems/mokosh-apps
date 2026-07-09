//! Mokosh Platform Library
//!
//! This library provides the core functionality for the Mokosh platform.

use dioxus::prelude::*;

pub mod components;
pub mod hooks;
pub mod modules;
pub mod pages;
pub mod utils;

pub use modules::auth::CurrentUser;
pub use utils::error::{AppError, AppResult};

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
                "Loading..."
            }
        };
    }
    if !auth_state.is_authenticated() {
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
        let _ = crate::modules::oidc::start_login(&cfg, return_to);
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                "Signing you in..."
            }
        };
    }
    // Forced onboarding for new Bunyip-JIT users: redirect to
    // /onboarding/profile whenever the server reports
    // profile_completed = false (first + last name still synthetic
    // placeholders). Bypass when the user is already on the
    // onboarding route itself, otherwise the AuthGuard would re-fire
    // its own redirect every render and loop. Reads the pathname out
    // of `window.location` rather than the router's current Route
    // because we need to make the comparison synchronously inside
    // render; pulling from web_sys avoids a re-entrant signal read.
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
        #[cfg(target_arch = "wasm32")]
        let on_onboarding_route = web_sys::window()
            .and_then(|w| w.location().pathname().ok())
            .is_some_and(|p| p == "/onboarding/profile");
        #[cfg(not(target_arch = "wasm32"))]
        let on_onboarding_route = false;
        if !on_onboarding_route {
            tracing::info!(
                target: "auth_guard",
                "redirecting to /onboarding/profile (profile_completed=false, server_loaded=true)"
            );
            nav.replace(Route::Onboarding {});
            return rsx! {
                div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
                    "Setting up your profile..."
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
    let reload = move |_| {
        #[cfg(target_arch = "wasm32")]
        if let Some(win) = web_sys::window() {
            let _ = win.location().reload();
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

      // Forced-onboarding for new Bunyip-JIT users. Sits inside
      // AuthGuard (the user MUST be authenticated to reach it) but
      // the guard exempts it from its own redirect; see the
      // pathname check in AuthGuard above.
      #[route("/onboarding/profile")]
      Onboarding {},

      // Dashboard
      #[route("/dashboard")]
      Dashboard {},

      // MAPPS-256: full-screen, team-scoped wall-monitor "TV view" of the
      // dashboard. Inside AuthGuard like every authenticated route, but the
      // page renders WITHOUT `AppLayout` so no TopBar / Sidebar / banners /
      // ToastRoot chrome appears over the bare table.
      #[route("/dashboard/tv")]
      DashboardTv {},

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

    // MAPPS-302: NOC "Big View" routes. Kiosk-friendly variants of the
    // tickets queue, the dispatch board, and the calendar - no sidebar
    // or top bar, larger typography, auto-refresh tick. Operators
    // bookmark these on a wall-mounted screen via the `?refresh=` and
    // `?status=` / `?priority=` query params.
    #[route("/big/tickets")]
    BigTickets {},

    #[route("/big/dispatch")]
    BigDispatch {},

    #[route("/big/calendar")]
    BigCalendar {},

    // Contracts
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
    #[route("/admin/audit")]
    AuditLog {},
    #[route("/admin/sla")]
    SlaManagement {},
    #[route("/admin/team")]
    Team {},

    // Admin (multi-tenant only)
    #[cfg(feature = "multi-tenant")]
    #[route("/admin/tenants")]
    TenantManagement {},

    // End of AuthGuard scope. Portal routes have their own layout and
    // auth model (client portal vs internal tools); the catch-all 404
    // is intentionally public so logged-out users see a real 404 page.
    #[end_layout]

    // Client Portal Routes (separate layout)
    #[route("/portal")]
    PortalHome {},

    #[route("/portal/tickets")]
    PortalTicketList {},

    #[route("/portal/tickets/new")]
    PortalTicketNew {},

    #[route("/portal/tickets/:id")]
    PortalTicketDetail { id: String },

    #[route("/portal/invoices")]
    PortalInvoiceList {},

    #[route("/portal/invoices/:id")]
    PortalInvoiceDetail { id: String },

    #[route("/portal/kb")]
    PortalKB {},

    // Catch-all 404
    #[route("/:..route")]
    NotFound { route: Vec<String> },
}

// Route component wrappers - these import the actual page components
use pages::*;

/// Top-level navigate to the Bunyip hub. Used by the legacy
/// account-surface routes (`/login`, `/forgot-password`, etc.) so that
/// existing bookmarks keep working instead of 404ing. `replace()`
/// rather than `assign()` so the hub's URL takes over the history
/// entry; the back button skips the dead mokosh-clients URL.
fn redirect_to_hub(path: &str) {
    if let Some(win) = web_sys::window() {
        let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
        let url = cfg.hub_url(path);
        let _ = win.location().replace(&url);
    }
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
            "Redirecting to {label}..."
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
/// "Signing in..." placeholder while the browser navigates to
/// `/oauth2/authorize`.
///
/// This is the same kickoff `AuthGuard` does when an unauthenticated user
/// hits a protected route. The pre-cutover behaviour (HubRedirect to bunyip's
/// `/login` directly) skipped the OIDC handshake entirely, which is why the
/// user landed on bunyip's dashboard with no way back to msp.
#[component]
fn Login() -> Element {
    use_effect(move || {
        let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
        let _ = crate::modules::oidc::start_login(&cfg, "/dashboard");
    });
    rsx! {
        div { class: "min-h-screen flex items-center justify-center text-sm text-muted",
            "Signing you in..."
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
    rsx! { dashboards_view::DefaultDashboardPage {} }
}

#[component]
fn SavedDashboards() -> Element {
    rsx! { dashboards::SavedDashboardsPage {} }
}

#[component]
fn SavedDashboardView(id: String) -> Element {
    rsx! { dashboards_view::SavedDashboardViewPage { id } }
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
    rsx! { tickets::TicketListPage {} }
}

#[component]
fn TicketNew() -> Element {
    rsx! { tickets::TicketNewPage {} }
}

#[component]
fn TicketDetail(id: String) -> Element {
    rsx! { tickets::TicketDetailPage { id } }
}

#[component]
fn TimeEntryList() -> Element {
    rsx! { time::TimeEntryListPage {} }
}

#[component]
fn TimeEntryNew() -> Element {
    rsx! { time::TimeEntryNewPage {} }
}

#[component]
fn Timesheets() -> Element {
    rsx! { time::TimesheetsPage {} }
}

#[component]
fn TimesheetApprovals() -> Element {
    rsx! { time::TimesheetApprovalsPage {} }
}

#[component]
fn Approvals() -> Element {
    rsx! { approvals::ApprovalsPage {} }
}

#[component]
fn ProjectList() -> Element {
    rsx! { projects::ProjectListPage {} }
}

#[component]
fn ProjectNew() -> Element {
    rsx! { projects::ProjectNewPage {} }
}

#[component]
fn ProjectDetail(id: String) -> Element {
    rsx! { projects::ProjectDetailPage { id } }
}

#[component]
fn ProjectTasks(id: String) -> Element {
    rsx! { projects::ProjectTasksPage { id } }
}

#[component]
fn CompanyList() -> Element {
    rsx! { contacts::CompanyListPage {} }
}

#[component]
fn CompanyNew() -> Element {
    rsx! { contacts::CompanyNewPage {} }
}

#[component]
fn CompanyDetail(id: String) -> Element {
    rsx! { contacts::CompanyDetailPage { id } }
}

#[component]
fn CompanyEdit(id: String) -> Element {
    rsx! { contacts::CompanyEditPage { id } }
}

#[component]
fn ContactList() -> Element {
    rsx! { contacts::ContactListPage {} }
}

#[component]
fn ContactNew() -> Element {
    rsx! { contacts::ContactNewPage {} }
}

#[component]
fn ContactDetail(id: String) -> Element {
    rsx! { contacts::ContactDetailPage { id } }
}

#[component]
fn ContactEdit(id: String) -> Element {
    rsx! { contacts::ContactEditPage { id } }
}

#[component]
fn Calendar() -> Element {
    rsx! { calendar::CalendarPage {} }
}

#[component]
fn DispatchBoard() -> Element {
    rsx! { calendar::DispatchBoardPage {} }
}

#[component]
fn SchedulingTemplates() -> Element {
    rsx! { calendar::SchedulingTemplatesPage {} }
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
fn ContractList() -> Element {
    rsx! { contracts::ContractListPage {} }
}

#[component]
fn ContractNew() -> Element {
    rsx! { contracts::ContractNewPage {} }
}

#[component]
fn ContractDetail(id: String) -> Element {
    rsx! { contracts::ContractDetailPage { id } }
}

#[component]
fn ContractEdit(id: String) -> Element {
    rsx! { contracts::ContractEditPage { id } }
}

#[component]
fn RateCardList() -> Element {
    rsx! { contracts::RateCardListPage {} }
}

#[component]
fn RateCardNew() -> Element {
    rsx! { contracts::RateCardListPage { open_create: true } }
}

#[component]
fn RateCardDetail(id: String) -> Element {
    rsx! { contracts::RateCardDetailPage { id } }
}

#[component]
fn InvoiceList() -> Element {
    rsx! { billing::InvoiceListPage {} }
}

#[component]
fn InvoiceNew() -> Element {
    rsx! { billing::InvoiceNewPage {} }
}

#[component]
fn InvoiceDetail(id: String) -> Element {
    rsx! { billing::InvoiceDetailPage { id } }
}

#[component]
fn PaymentList() -> Element {
    rsx! { billing::PaymentListPage {} }
}

#[component]
fn TaxRateList() -> Element {
    rsx! { billing::TaxRateListPage {} }
}

#[component]
fn PaymentGatewayConfig() -> Element {
    rsx! { billing::PaymentGatewayConfigPage {} }
}

#[component]
fn AssetList() -> Element {
    rsx! { assets::AssetListPage {} }
}

#[component]
fn AssetNew() -> Element {
    rsx! { assets::AssetNewPage {} }
}

#[component]
fn AssetDetail(id: String) -> Element {
    rsx! { assets::AssetDetailPage { id } }
}

#[component]
fn KBHome() -> Element {
    rsx! { knowledge_base::KBHomePage {} }
}

#[component]
fn KBArticleList(q: String, tag: String, category: String) -> Element {
    rsx! { knowledge_base::KBArticleListPage { initial_q: q, initial_tag: tag, initial_category: category } }
}

#[component]
fn KBArticleNew() -> Element {
    rsx! { knowledge_base::KBArticleNewPage {} }
}

#[component]
fn KBArticleDetail(id: String) -> Element {
    rsx! { knowledge_base::KBArticleDetailPage { id } }
}

#[component]
fn KBArticleEdit(id: String) -> Element {
    rsx! { knowledge_base::KBArticleEditPage { id } }
}

#[component]
fn Reports() -> Element {
    rsx! { reports::ReportsPage {} }
}

#[component]
fn ReportDetail(report_type: String) -> Element {
    rsx! { reports::ReportDetailPage { report_type } }
}

#[component]
fn ActiveTenant() -> Element {
    // Tenant switching moved to the bunyip hub per
    // docs/migration/settings-split.md. Bookmarks at the legacy URL
    // bounce there instead of 404ing.
    rsx! { HubRedirect { target: "/settings/active-tenant".to_string(), label: "tenant switcher" } }
}

#[component]
fn Profile() -> Element {
    rsx! { profile::ProfilePage {} }
}

// MAPPS-169 Settings hub + sub-routes. The type editors are net-new
// (pages::settings); the re-homed surfaces reuse the existing page
// components so there is one source of truth per surface.
#[component]
fn SettingsHome() -> Element {
    rsx! { settings::SettingsHomePage {} }
}

// MAPPS-258 per-group landing pages.
#[component]
fn SettingsGroupServiceTypes() -> Element {
    rsx! { settings::ServiceTypesGroupPage {} }
}

#[component]
fn SettingsGroupBilling() -> Element {
    rsx! { settings::BillingGroupPage {} }
}

#[component]
fn SettingsGroupTickets() -> Element {
    rsx! { settings::TicketsGroupPage {} }
}

#[component]
fn SettingsGroupIntegrations() -> Element {
    rsx! { settings::IntegrationsGroupPage {} }
}

#[component]
fn SettingsWorkTypes() -> Element {
    rsx! { settings::WorkTypesSettingsPage {} }
}

#[component]
fn SettingsTaskStatuses() -> Element {
    rsx! { settings::TaskStatusesSettingsPage {} }
}

#[component]
fn SettingsAssetTypes() -> Element {
    rsx! { settings::AssetTypesSettingsPage {} }
}

#[component]
fn SettingsCompanyIndustries() -> Element {
    rsx! { settings::CompanyIndustriesSettingsPage {} }
}

#[component]
fn SettingsProjectTypes() -> Element {
    rsx! { settings::ProjectTypesSettingsPage {} }
}

#[component]
fn SettingsPaymentTerms() -> Element {
    rsx! { settings::PaymentTermsSettingsPage {} }
}

#[component]
fn SettingsSla() -> Element {
    rsx! { sla::SlaManagementPage {} }
}

#[component]
fn SettingsScheduling() -> Element {
    rsx! { settings::SchedulingSettingsPage {} }
}

#[component]
fn SettingsAppearance() -> Element {
    rsx! { settings::AppearanceSettingsPage {} }
}

#[component]
fn SettingsTvView() -> Element {
    rsx! { settings::TvViewSettingsPage {} }
}

#[component]
fn SettingsTimeTracking() -> Element {
    rsx! { settings::MaxHoursPerDaySettingsPage {} }
}

#[component]
fn SettingsRateCards() -> Element {
    rsx! { contracts::RateCardListPage {} }
}

#[component]
fn SettingsTaxRates() -> Element {
    rsx! { billing::TaxRateListPage {} }
}

#[component]
fn SettingsGateways() -> Element {
    rsx! { billing::PaymentGatewayConfigPage {} }
}

#[component]
fn SettingsImportExport() -> Element {
    rsx! { settings::ImportExportSettingsPage {} }
}

// MAPPS-172 ticket lookup editors.
#[component]
fn SettingsTicketStatuses() -> Element {
    rsx! { settings::TicketStatusesSettingsPage {} }
}

#[component]
fn SettingsTicketPriorities() -> Element {
    rsx! { settings::TicketPrioritiesSettingsPage {} }
}

#[component]
fn SettingsTicketTypes() -> Element {
    rsx! { settings::TicketTypesSettingsPage {} }
}

#[component]
fn SettingsTicketQueues() -> Element {
    rsx! { settings::TicketQueuesSettingsPage {} }
}

#[component]
fn SettingsTicketCategories() -> Element {
    rsx! { settings::TicketCategoriesSettingsPage {} }
}

// MAPPS-199 RMM integration admin UI.
#[component]
fn SettingsRmmConnections() -> Element {
    rsx! { settings::RmmConnectionsSettingsPage {} }
}

#[component]
fn SettingsRmmDeviceMappings() -> Element {
    rsx! { settings::RmmDeviceMappingsSettingsPage {} }
}

#[component]
fn SettingsRmmAlertRules() -> Element {
    rsx! { settings::RmmAlertRulesSettingsPage {} }
}

#[component]
fn SystemStatus() -> Element {
    rsx! { system_status::SystemStatusPage {} }
}

#[component]
fn ButtonShowcase() -> Element {
    rsx! { button_showcase::ButtonShowcasePage {} }
}

#[component]
fn AuditLog() -> Element {
    rsx! { audit_log::AuditLogPage {} }
}

#[component]
fn SlaManagement() -> Element {
    rsx! { sla::SlaManagementPage {} }
}

#[component]
fn Team() -> Element {
    rsx! { team::TeamPage {} }
}

#[cfg(feature = "multi-tenant")]
#[component]
fn TenantManagement() -> Element {
    rsx! { admin::TenantManagementPage {} }
}

#[component]
fn PortalHome() -> Element {
    rsx! { portal::PortalHomePage {} }
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
