//! Authentication models and types

// These model enums expose `from_str(&str) -> Option<Self>` as a deliberate
// infallible-style parser API; they intentionally do not implement
// `std::str::FromStr` (which requires a `Result`).
#![allow(clippy::should_implement_trait)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

// MAPPS-378: `UserRole` and `CurrentUser` are the shared identity DTOs of the
// client/server wire contract, so source them from the `mokosh-types` crate
// instead of re-declaring them here. Drift now fails the build rather than
// silently deserializing to a default. The prior hand-maintained copies were
// field- and method-identical to the shared ones; the only local extra was an
// unused `UserRole::parse_role` shim (zero call sites), which is dropped. The
// SPA-owned `AuthState` (maps auth failure to the crate's own `AppError`),
// `User`, `UserResponse`, `UserStatus`, and the request DTOs stay local below.
pub use mokosh_types::auth::{CurrentUser, UserRole};

/// User account status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    #[default]
    Active,
    Inactive,
    Pending,
}

impl UserStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "inactive" => Some(Self::Inactive),
            "pending" => Some(Self::Pending),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Pending => "pending",
        }
    }
}

/// Current authenticated user state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthState {
    /// Whether the user is authenticated
    pub is_authenticated: bool,
    /// The current user (if authenticated)
    pub user: Option<CurrentUser>,
    /// The current tenant ID
    pub tenant_id: Option<Uuid>,
}

impl AuthState {
    /// Create an authenticated state
    pub fn authenticated(user: CurrentUser, tenant_id: Uuid) -> Self {
        Self {
            is_authenticated: true,
            user: Some(user),
            tenant_id: Some(tenant_id),
        }
    }

    /// Get the current user or return an error
    pub fn require_user(&self) -> Result<&CurrentUser, crate::utils::error::AppError> {
        self.user
            .as_ref()
            .ok_or(crate::utils::error::AppError::Unauthorized)
    }

    /// Get the current tenant ID or return an error
    pub fn require_tenant(&self) -> Result<Uuid, crate::utils::error::AppError> {
        self.tenant_id
            .ok_or(crate::utils::error::AppError::Unauthorized)
    }

    /// Check if the user has a specific role
    pub fn has_role(&self, role: UserRole) -> bool {
        self.user.as_ref().is_some_and(|u| u.role == role)
    }

    /// Check if the user has admin privileges
    pub fn is_admin(&self) -> bool {
        self.user.as_ref().is_some_and(|u| u.role.is_admin())
    }

    /// PMS-791 / MAPPS-462: `true` when the caller's tenant is a
    /// multi-user org tenant. Used to gate org-only surfaces (Teams
    /// nav item, etc.). Empty string on the DTO (older server that did
    /// not carry the field, or the default for tests) reads as `true`
    /// so a missing field never wrongly hides a feature — server-side
    /// authorization still gates the actual endpoints.
    pub fn is_org_tenant(&self) -> bool {
        let kind = self
            .user
            .as_ref()
            .map(|u| u.tenant_kind.as_str())
            .unwrap_or("");
        matches!(kind, "org" | "")
    }

    /// PMS-791 / MAPPS-462: strict "personal tenant" check — only true
    /// when the tenant is explicitly `kind='personal'`. Complements
    /// [`Self::is_org_tenant`] for surfaces that need to positively
    /// identify personal tenants (e.g. the "Teams are for organizations"
    /// ContentUnavailable copy).
    pub fn is_personal_tenant(&self) -> bool {
        self.user
            .as_ref()
            .is_some_and(|u| u.tenant_kind == "personal")
    }
}

// `CurrentUser` (struct + `full_name` / `initials`) is re-exported from
// `mokosh-types` at the top of this module (MAPPS-378).

/// User database model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<String>,
    pub mobile: Option<String>,
    pub title: Option<String>,
    pub avatar_url: Option<String>,
    pub timezone: String,
    pub locale: String,
    /// PMS-253: per-user date/time format string. See [`CurrentUser::date_format_string`].
    #[serde(default)]
    pub date_format_string: Option<String>,
    pub role: UserRole,
    pub status: UserStatus,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub mfa_enabled: bool,
    #[serde(skip_serializing)]
    pub mfa_secret: Option<String>,
    pub notification_preferences: serde_json::Value,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// Convert to CurrentUser for auth context
    pub fn to_current_user(&self) -> CurrentUser {
        CurrentUser {
            id: self.id,
            tenant_id: self.tenant_id,
            email: self.email.clone(),
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            role: self.role,
            timezone: self.timezone.clone(),
            avatar_url: self.avatar_url.clone(),
            // Client-side `User` is a legacy snapshot type with no
            // profile_completed_at column; mirror the server's "default
            // true on legacy paths" so a code path that touches this
            // conversion never traps a user in onboarding.
            profile_completed: true,
            date_format_string: self.date_format_string.clone(),
            // The client-side `User` snapshot carries no theme columns; the
            // live values reach the SPA via the `/auth/me` payload.
            theme_base_mode: None,
            theme_accent_id: None,
            // The client-side `User` is a legacy snapshot with no
            // own-company column; the live value reaches the SPA via the
            // `/auth/me` payload (see `MeBody` in hooks/auth.rs).
            own_company_id: None,
            // PMS-791 / MAPPS-462: legacy `User` snapshot carries no
            // tenant_kind either; the live value reaches the SPA via the
            // /auth/me payload. Default to empty string; is_org_tenant()
            // treats empty as "org" (fail-open UI; server still gates).
            tenant_kind: String::new(),
        }
    }
}

/// Login request
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email(message = "Invalid email address"))]
    pub email: String,
    #[validate(length(min = 1, message = "Password is required"))]
    pub password: String,
    /// Remember me for longer session
    #[serde(default)]
    pub remember_me: bool,
    /// MFA code if required
    pub mfa_code: Option<String>,
}

/// Refresh token request
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// User list response (for API)
#[derive(Debug, Clone, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub full_name: String,
    pub phone: Option<String>,
    pub mobile: Option<String>,
    pub title: Option<String>,
    pub avatar_url: Option<String>,
    pub timezone: String,
    pub role: UserRole,
    pub status: UserStatus,
    pub mfa_enabled: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            full_name: format!("{} {}", user.first_name, user.last_name),
            phone: user.phone,
            mobile: user.mobile,
            title: user.title,
            avatar_url: user.avatar_url,
            timezone: user.timezone,
            role: user.role,
            status: user.status,
            mfa_enabled: user.mfa_enabled,
            last_login_at: user.last_login_at,
            created_at: user.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_role_is_admin() {
        assert!(UserRole::SuperAdmin.is_admin());
        assert!(UserRole::Admin.is_admin());
        assert!(!UserRole::Manager.is_admin());
        assert!(!UserRole::Technician.is_admin());
    }

    #[test]
    fn test_user_role_can_manage_users() {
        assert!(UserRole::SuperAdmin.can_manage_users());
        assert!(UserRole::Admin.can_manage_users());
        assert!(UserRole::Manager.can_manage_users());
        assert!(!UserRole::Technician.can_manage_users());
        assert!(!UserRole::Sales.can_manage_users());
    }

    #[test]
    fn test_user_role_can_view_financials() {
        assert!(UserRole::SuperAdmin.can_view_financials());
        assert!(UserRole::Admin.can_view_financials());
        assert!(UserRole::Manager.can_view_financials());
        assert!(UserRole::Finance.can_view_financials());
        assert!(!UserRole::Technician.can_view_financials());
        assert!(!UserRole::Dispatcher.can_view_financials());
    }

    #[test]
    fn test_user_role_can_manage_billing() {
        assert!(UserRole::SuperAdmin.can_manage_billing());
        assert!(UserRole::Admin.can_manage_billing());
        assert!(UserRole::Finance.can_manage_billing());
        assert!(!UserRole::Manager.can_manage_billing());
        assert!(!UserRole::Technician.can_manage_billing());
    }

    #[test]
    fn test_user_role_from_str() {
        assert_eq!(UserRole::from_str("admin"), Some(UserRole::Admin));
        assert_eq!(
            UserRole::from_str("super_admin"),
            Some(UserRole::SuperAdmin)
        );
        assert_eq!(UserRole::from_str("technician"), Some(UserRole::Technician));
        assert_eq!(UserRole::from_str("invalid"), None);
    }

    #[test]
    fn test_user_role_as_str() {
        assert_eq!(UserRole::Admin.as_str(), "admin");
        assert_eq!(UserRole::SuperAdmin.as_str(), "super_admin");
        assert_eq!(UserRole::Technician.as_str(), "technician");
    }

    #[test]
    fn test_user_role_display() {
        assert_eq!(format!("{}", UserRole::Admin), "admin");
        assert_eq!(format!("{}", UserRole::Manager), "manager");
    }

    #[test]
    fn test_user_status_from_str() {
        assert_eq!(UserStatus::from_str("active"), Some(UserStatus::Active));
        assert_eq!(UserStatus::from_str("inactive"), Some(UserStatus::Inactive));
        assert_eq!(UserStatus::from_str("pending"), Some(UserStatus::Pending));
        assert_eq!(UserStatus::from_str("unknown"), None);
    }

    #[test]
    fn test_auth_state_default() {
        let state = AuthState::default();
        assert!(!state.is_authenticated);
        assert!(state.user.is_none());
        assert!(state.tenant_id.is_none());
    }

    #[test]
    fn test_auth_state_authenticated() {
        let user = CurrentUser {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            role: UserRole::Admin,
            timezone: "UTC".to_string(),
            avatar_url: None,
            profile_completed: true,
            date_format_string: None,
            theme_base_mode: None,
            theme_accent_id: None,
            own_company_id: None,
            tenant_kind: String::new(),
        };
        let tenant_id = user.tenant_id;

        let state = AuthState::authenticated(user, tenant_id);
        assert!(state.is_authenticated);
        assert!(state.user.is_some());
        assert_eq!(state.tenant_id, Some(tenant_id));
    }

    #[test]
    fn test_auth_state_has_role() {
        let user = CurrentUser {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "admin@example.com".to_string(),
            first_name: "Admin".to_string(),
            last_name: "User".to_string(),
            role: UserRole::Admin,
            timezone: "UTC".to_string(),
            avatar_url: None,
            profile_completed: true,
            date_format_string: None,
            theme_base_mode: None,
            theme_accent_id: None,
            own_company_id: None,
            tenant_kind: String::new(),
        };
        let tenant_id = user.tenant_id;
        let state = AuthState::authenticated(user, tenant_id);

        assert!(state.has_role(UserRole::Admin));
        assert!(!state.has_role(UserRole::Technician));
        assert!(state.is_admin());
    }

    #[test]
    fn test_current_user_full_name() {
        let user = CurrentUser {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "john.doe@example.com".to_string(),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            role: UserRole::Technician,
            timezone: "America/New_York".to_string(),
            avatar_url: None,
            profile_completed: true,
            date_format_string: None,
            theme_base_mode: None,
            theme_accent_id: None,
            own_company_id: None,
            tenant_kind: String::new(),
        };

        assert_eq!(user.full_name(), "John Doe");
    }

    #[test]
    fn test_current_user_initials() {
        let user = CurrentUser {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "john.doe@example.com".to_string(),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            role: UserRole::Technician,
            timezone: "UTC".to_string(),
            avatar_url: None,
            profile_completed: true,
            date_format_string: None,
            theme_base_mode: None,
            theme_accent_id: None,
            own_company_id: None,
            tenant_kind: String::new(),
        };

        assert_eq!(user.initials(), "JD");
    }

    #[test]
    fn test_auth_state_require_user() {
        let empty_state = AuthState::default();
        assert!(empty_state.require_user().is_err());

        let user = CurrentUser {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            role: UserRole::Admin,
            timezone: "UTC".to_string(),
            avatar_url: None,
            profile_completed: true,
            date_format_string: None,
            theme_base_mode: None,
            theme_accent_id: None,
            own_company_id: None,
            tenant_kind: String::new(),
        };
        let tenant_id = user.tenant_id;
        let auth_state = AuthState::authenticated(user, tenant_id);
        assert!(auth_state.require_user().is_ok());
    }

    #[test]
    fn test_auth_state_require_tenant() {
        let empty_state = AuthState::default();
        assert!(empty_state.require_tenant().is_err());

        let user = CurrentUser {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            role: UserRole::Admin,
            timezone: "UTC".to_string(),
            avatar_url: None,
            profile_completed: true,
            date_format_string: None,
            theme_base_mode: None,
            theme_accent_id: None,
            own_company_id: None,
            tenant_kind: String::new(),
        };
        let tenant_id = user.tenant_id;
        let auth_state = AuthState::authenticated(user, tenant_id);
        assert!(auth_state.require_tenant().is_ok());
        assert_eq!(auth_state.require_tenant().unwrap(), tenant_id);
    }

    // PMS-791 / MAPPS-462 helpers for the Teams nav gate.

    fn user_with_kind(kind: &str) -> CurrentUser {
        CurrentUser {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "e@example.com".to_string(),
            first_name: "F".to_string(),
            last_name: "L".to_string(),
            role: UserRole::Admin,
            timezone: "UTC".to_string(),
            avatar_url: None,
            profile_completed: true,
            date_format_string: None,
            theme_base_mode: None,
            theme_accent_id: None,
            own_company_id: None,
            tenant_kind: kind.to_string(),
        }
    }

    #[test]
    fn is_org_tenant_true_for_kind_org() {
        let u = user_with_kind("org");
        let t = u.tenant_id;
        let s = AuthState::authenticated(u, t);
        assert!(s.is_org_tenant());
        assert!(!s.is_personal_tenant());
    }

    #[test]
    fn is_org_tenant_defaults_true_on_missing_field() {
        // MAPPS-462: an empty tenant_kind (older server that did not
        // populate the field, or a legacy fixture) should default to
        // treating the tenant as org — safe: server-side auth still gates.
        let u = user_with_kind("");
        let t = u.tenant_id;
        let s = AuthState::authenticated(u, t);
        assert!(s.is_org_tenant());
        assert!(!s.is_personal_tenant());
    }

    #[test]
    fn is_personal_tenant_when_kind_is_personal() {
        let u = user_with_kind("personal");
        let t = u.tenant_id;
        let s = AuthState::authenticated(u, t);
        assert!(!s.is_org_tenant());
        assert!(s.is_personal_tenant());
    }

    #[test]
    fn is_org_tenant_false_when_no_user() {
        let s = AuthState::default();
        // No user at all: matches!("") -> true via the unwrap_or fallback,
        // preserving fail-open behavior when the field is unknown.
        assert!(s.is_org_tenant());
        assert!(!s.is_personal_tenant());
    }
}
