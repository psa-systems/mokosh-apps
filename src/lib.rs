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

    #[route("/contacts")]
    ContactList {},

    #[route("/contacts/new")]
    ContactNew {},

    #[route("/contacts/:id")]
    ContactDetail { id: String },

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

    // Billing
    #[route("/invoices")]
    InvoiceList {},

    #[route("/invoices/new")]
    InvoiceNew {},

    #[route("/invoices/:id")]
    InvoiceDetail { id: String },

    #[route("/payments")]
    PaymentList {},

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

    // Reports
    #[route("/reports")]
    Reports {},

    #[route("/reports/:report_type")]
    ReportDetail { report_type: String },

    // Settings
    #[route("/settings")]
    Settings {},

    #[route("/settings/users")]
    UserManagement {},

    #[route("/settings/users/invite")]
    InviteCreate {},

    #[route("/settings/users/invites")]
    InviteList {},

    #[route("/settings/teams")]
    TeamManagement {},

    #[route("/settings/notifications")]
    NotificationSettings {},

    #[route("/settings/integrations")]
    IntegrationSettings {},

    #[route("/settings/billing")]
    BillingSettings {},

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

/// Layout component that gates all authenticated routes. Renders
/// nothing when the user is not signed in and asks the navigator to
/// replace the current entry with `/login` synchronously during
/// render. The render-time guard is what defeats the back-button
/// bypass: a popstate-driven re-render of a protected route never
/// commits any of its content to the DOM, so there is no flash of
/// authenticated UI before redirect.
#[component]
fn AuthGuard() -> Element {
    let auth = hooks::use_auth();
    let nav = use_navigator();
    if !auth.read().is_authenticated() {
        // `replace`, not `push`: the back stack should not record a
        // login redirect on top of the protected URL the user just
        // tried to revisit.
        nav.replace(Route::Login {});
        return rsx! {};
    }
    rsx! { Outlet::<Route> {} }
}

#[component]
fn Home() -> Element {
    rsx! { home::HomePage {} }
}

#[component]
fn Login() -> Element {
    rsx! { auth::LoginPage {} }
}

#[component]
fn AuthCallback() -> Element {
    rsx! { auth_callback::AuthCallbackPage {} }
}

#[component]
fn ForgotPassword() -> Element {
    rsx! { auth::ForgotPasswordPage {} }
}

#[component]
fn ResetPassword(token: String) -> Element {
    rsx! { auth::ResetPasswordPage { token } }
}

#[component]
fn InviteAccept(token: String) -> Element {
    rsx! { invite_accept::InviteAcceptPage { token } }
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
fn Reports() -> Element {
    rsx! { reports::ReportsPage {} }
}

#[component]
fn ReportDetail(report_type: String) -> Element {
    rsx! { reports::ReportDetailPage { report_type } }
}

#[component]
fn Settings() -> Element {
    rsx! { settings::SettingsPage {} }
}

#[component]
fn UserManagement() -> Element {
    rsx! { settings::UserManagementPage {} }
}

#[component]
fn InviteCreate() -> Element {
    rsx! { invites::InviteCreatePage {} }
}

#[component]
fn InviteList() -> Element {
    rsx! { invites::InviteListPage {} }
}

#[component]
fn TeamManagement() -> Element {
    rsx! { settings::TeamManagementPage {} }
}

#[component]
fn NotificationSettings() -> Element {
    rsx! { settings::NotificationSettingsPage {} }
}

#[component]
fn IntegrationSettings() -> Element {
    rsx! { settings::IntegrationSettingsPage {} }
}

#[component]
fn BillingSettings() -> Element {
    rsx! { settings::BillingSettingsPage {} }
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
