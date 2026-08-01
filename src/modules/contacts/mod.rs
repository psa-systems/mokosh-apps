//! Contact Management Module
//!
//! Handles companies (clients), contacts, and sites.
//!
//! MAPPS-383: the DTOs are re-exported from the shared `mokosh-types` crate
//! rather than hand-copied, so the compiler enforces the wire contract with
//! mokosh-server. The previous copy had drifted (no `Contact::company_name`,
//! no `CompanyType::Internal`).

pub use mokosh_types::contacts::*;

#[cfg(test)]
mod tests {
    /// MAPPS-383 guard: these names must resolve to the shared crate's types.
    /// Re-introducing a hand copy under this module breaks the identity
    /// conversions below at compile time.
    #[test]
    fn dtos_are_the_shared_types() {
        let _: fn(mokosh_types::contacts::Company) -> super::Company = |v| v;
        let _: fn(mokosh_types::contacts::Contact) -> super::Contact = |v| v;
        let _: fn(mokosh_types::contacts::Site) -> super::Site = |v| v;
        let _: fn(mokosh_types::contacts::Address) -> super::Address = |v| v;
        let _: fn(mokosh_types::contacts::CompanyType) -> super::CompanyType = |v| v;
    }

    /// The drift MAPPS-383 removes: the old hand copy had no `Internal`
    /// variant and no `Contact::company_name`.
    #[test]
    fn shared_shape_carries_the_previously_missing_members() {
        assert_eq!(super::CompanyType::Internal.as_str(), "internal");
        assert_eq!(
            super::CompanyType::from_str("internal"),
            Some(super::CompanyType::Internal)
        );
        // Compile-time: the field must exist on the shared struct.
        fn _company_name(c: &super::Contact) -> &Option<String> {
            &c.company_name
        }
    }
}
