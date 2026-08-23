//! Authentication models.
//!
//! MAPPS-536: these are the shared wire DTOs, so they come from `mokosh-types`
//! rather than being declared here. MAPPS-378 adopted `CurrentUser` and
//! `UserRole`; this finishes the module.
//!
//! What was here was 200 lines of hand copy that nothing imported. `AuthState`,
//! `UserStatus`, `User`, `UserResponse` and `RefreshTokenRequest` had no call
//! sites anywhere in the SPA, so they were not a wire contract the compiler was
//! checking - they were a snapshot of one, quietly ageing. Two had already
//! drifted: the local `AuthState` was missing the crate's `deleted` flag
//! (MAPPS-348, the tombstoned-account 410 path), and `LoginRequest` carried no
//! validation attributes.
//!
//! Two signatures differ from the copy they replace, which matters to whoever
//! writes the first caller. The crate's `AuthState::require_user` and
//! `require_tenant` yield the `AuthRequired` marker rather than this crate's
//! `AppError::Unauthorized`, because the shared crate cannot reach our error
//! type; map it at the call site. And the crate's `AuthState` has the `deleted`
//! field, so an exhaustive struct literal needs it.
//!
//! Where the SPA actually reads server payloads is `src/pages/`, in structs
//! declared per page. Those are the hand copies that still matter, and this
//! change does not touch them; see `docs/client-server-integration.md`.

pub use mokosh_types::auth::*;

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// MAPPS-536 guard, in the shape of `src/modules/contacts/mod.rs`: these
    /// names must resolve to the shared crate's types. Re-declaring any of them
    /// under this module breaks the identity conversions below at compile time,
    /// which is the whole point of the re-export.
    #[test]
    fn dtos_are_the_shared_types() {
        let _: fn(mokosh_types::auth::AuthState) -> super::AuthState = |v| v;
        let _: fn(mokosh_types::auth::CurrentUser) -> super::CurrentUser = |v| v;
        let _: fn(mokosh_types::auth::User) -> super::User = |v| v;
        let _: fn(mokosh_types::auth::UserResponse) -> super::UserResponse = |v| v;
        let _: fn(mokosh_types::auth::UserRole) -> super::UserRole = |v| v;
        let _: fn(mokosh_types::auth::UserStatus) -> super::UserStatus = |v| v;
        let _: fn(mokosh_types::auth::LoginRequest) -> super::LoginRequest = |v| v;
        let _: fn(mokosh_types::auth::RefreshTokenRequest) -> super::RefreshTokenRequest = |v| v;
    }

    /// The drift this adoption closes: the hand-copied `AuthState` had no
    /// `deleted` flag, so the SPA could not tell a tombstoned account
    /// (MAPPS-348, answered 410) from an expired session (401).
    #[test]
    fn shared_auth_state_carries_the_deleted_flag() {
        let deleted = super::AuthState::deleted();
        assert!(deleted.deleted, "a tombstoned account is marked deleted");
        assert!(!deleted.is_authenticated);
        assert!(!super::AuthState::default().deleted);
    }

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
        };
        let tenant_id = user.tenant_id;
        let auth_state = AuthState::authenticated(user, tenant_id);
        assert!(auth_state.require_tenant().is_ok());
        assert_eq!(auth_state.require_tenant().unwrap(), tenant_id);
    }
}
