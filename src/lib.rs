//! Mokosh Platform Library
//!
//! This library provides the core functionality for the Mokosh platform.

use dioxus::prelude::*;

pub mod components;
pub mod hooks;
pub mod modules;
pub mod pages;
pub mod utils;

pub use modules::auth::{AuthState, CurrentUser};
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
    let auth_state = auth.read();
    if auth_state.is_loading {
        // Still hydrating tokens from sessionStorage. Render a
        // placeholder so we do not kick off the OIDC dance just to
        // find we already have a session.
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center text-sm text-gray-500",
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
        let _ = crate::modules::oidc::start_login(&cfg, "");
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center text-sm text-gray-500",
                "Signing you in..."
            }
        };
    }
    rsx! { Outlet::<Route> {} }
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

      // Dashboard
      #[route("/dashboard")]
      Dashboard {},

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

    #[route("/kb/articles")]
    KBArticleList {},

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

    // Mokosh-side profile. Edits the tenant-scoped fields on the
    // user row (name, title, phone, mobile, timezone). Cross-app
    // identity (email, password, MFA, sessions, billing) lives on
    // bunyip-web's `/settings`; the UserMenu's "Account Settings"
    // link sends users there.
    #[route("/profile")]
    Profile {},

    // Admin surfaces under /admin/*, gated at runtime by the user's role
    // inside each page (matching the server's RequireAdmin), available in
    // every build since the server endpoints exist regardless of tenancy.
    #[route("/admin/audit")]
    AuditLog {},
    #[route("/admin/sla")]
    SlaManagement {},

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
        div { class: "min-h-screen flex items-center justify-center p-8 text-sm text-gray-500 dark:text-gray-400",
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
        div { class: "min-h-screen flex items-center justify-center text-sm text-gray-500",
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
    rsx! { dashboard::DashboardPage {} }
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
fn KBArticleList() -> Element {
    rsx! { knowledge_base::KBArticleListPage {} }
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

#[component]
fn AuditLog() -> Element {
    rsx! { audit_log::AuditLogPage {} }
}

#[component]
fn SlaManagement() -> Element {
    rsx! { sla::SlaManagementPage {} }
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
    pub use crate::modules::auth::{AuthState, CurrentUser};
    pub use crate::utils::error::{AppError, AppResult};
    pub use crate::Route;
    pub use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
    pub use serde::{Deserialize, Serialize};
    pub use uuid::Uuid;
    pub use validator::Validate;
}
