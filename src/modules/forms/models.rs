//! PMS-731 form-definition client-side DTOs.
//!
//! These mirror mokosh-server's `src/modules/forms/models.rs` but carry only
//! what the SPA reads or sends. Responses derive `Deserialize`, request bodies
//! derive `Serialize`, everything derives `PartialEq` because Dioxus props
//! require it, and `#[serde(default)]` guards every optional field so the
//! server adding columns never breaks decoding.

// Mirrors `modules::tickets::models`: these enums expose
// `from_str(&str) -> Option<Self>` as a deliberate infallible-style parser API
// and intentionally do not implement `std::str::FromStr` (which requires a
// `Result`).
#![allow(clippy::should_implement_trait)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The field types the server accepts. Kept as a typed enum rather than a
/// bare string so the builder cannot author a type the server would reject
/// with a CHECK-constraint violation.
///
/// Adding a variant here without the server growing it in a migration
/// produces a 422 on save, which is the safe direction: the server is the
/// authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    Text,
    Textarea,
    Email,
    Date,
    Select,
    Boolean,
}

impl FieldType {
    pub const ALL: [FieldType; 6] = [
        FieldType::Text,
        FieldType::Textarea,
        FieldType::Email,
        FieldType::Date,
        FieldType::Select,
        FieldType::Boolean,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            FieldType::Text => "text",
            FieldType::Textarea => "textarea",
            FieldType::Email => "email",
            FieldType::Date => "date",
            FieldType::Select => "select",
            FieldType::Boolean => "boolean",
        }
    }

    /// Operator-facing name for the type picker.
    pub fn label(&self) -> &'static str {
        match self {
            FieldType::Text => "Short text",
            FieldType::Textarea => "Long text",
            FieldType::Email => "Email address",
            FieldType::Date => "Date",
            FieldType::Select => "Choice list",
            FieldType::Boolean => "Yes / no",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Whether an option set is required for this type. The server rejects a
    /// `select` with no options at write time, so the builder blocks it first.
    pub fn needs_options(&self) -> bool {
        matches!(self, FieldType::Select)
    }

    /// Whether a character-length bound means anything. The server ignores
    /// bounds on other types rather than erroring, but showing the input
    /// would imply it does something.
    pub fn honours_length(&self) -> bool {
        matches!(
            self,
            FieldType::Text | FieldType::Textarea | FieldType::Email
        )
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FormField {
    pub id: Uuid,
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub help_text: Option<String>,
    pub field_type: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub min_length: Option<i32>,
    #[serde(default)]
    pub max_length: Option<i32>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub date_not_in_past: bool,
    #[serde(default)]
    pub sort_order: i32,
}

/// A cross-field rule. The server supports exactly one kind (`required_if`);
/// an unknown kind decodes to `Other` so a definition authored by a newer
/// server still lists and edits rather than failing to load. Saving a
/// definition whose rules contain `Other` would drop it, so the editor blocks
/// that case explicitly rather than silently discarding a rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FormRule {
    RequiredIf {
        field: String,
        when_field: String,
        equals: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FormDefinition {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: Option<String>,
    /// PMS-748: how a client reaches the MSP about this request. Shown on the
    /// client's form page and in the email that links to it.
    #[serde(default)]
    pub contact_info: Option<String>,
    #[serde(default)]
    pub kb_article_id: Option<Uuid>,
    #[serde(default)]
    pub kb_article_title: Option<String>,
    #[serde(default)]
    pub rules: Vec<FormRule>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub fields: Vec<FormField>,
}

/// Field as sent on create/update. No `id`: the server replaces the whole
/// field set on a PATCH that carries `fields`, because field identity is the
/// payload key and a merge cannot express a rename unambiguously.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpsertFormField {
    pub name: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
    pub field_type: String,
    pub is_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    pub date_not_in_past: bool,
    pub sort_order: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CreateFormDefinitionRequest {
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_info: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kb_article_id: Option<Uuid>,
    pub rules: Vec<FormRule>,
    pub is_active: bool,
    pub fields: Vec<UpsertFormField>,
}

/// Update body. Every key is optional server-side, but the editor always
/// submits the whole definition (it is a whole-form editor), so this mirrors
/// create except that `slug` is absent: the slug is the link-stable
/// identifier and the server does not accept a change to it, so the editor
/// shows it read-only on an existing definition.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpdateFormDefinitionRequest {
    pub name: String,
    pub description: Option<String>,
    /// Serialised even when `None`, like `description`: the server treats an
    /// explicit null as "clear this", which is how the editor empties it.
    pub contact_info: Option<String>,
    pub kb_article_id: Option<Uuid>,
    pub rules: Vec<FormRule>,
    pub is_active: bool,
    pub fields: Vec<UpsertFormField>,
}

// ============================================================================
// PMS-730: REQUEST LINKS
// ============================================================================

/// A link issued to a client, as the agent surface sees it.
///
/// Deliberately has no token field, and the server never sends one: the token
/// is a credential for the recipient, and echoing it into an agent response
/// would put it in logs and browser history. A link that needs resending is
/// reissued, not recovered.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RequestLink {
    pub id: Uuid,
    pub form_definition_id: Uuid,
    #[serde(default)]
    pub form_name: String,
    pub company_id: Uuid,
    #[serde(default)]
    pub company_name: String,
    #[serde(default)]
    pub contact_id: Option<Uuid>,
    #[serde(default)]
    pub recipient_email: String,
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub used_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub submission_id: Option<Uuid>,
}

impl RequestLink {
    /// What has become of this link. Submitted wins over expired: a link that
    /// was used and has since passed its expiry is still a request that came
    /// in, and reporting it as expired would read as though the client never
    /// replied.
    pub fn status(&self, now: DateTime<Utc>) -> RequestLinkStatus {
        if self.used_at.is_some() {
            RequestLinkStatus::Submitted
        } else if self.expires_at <= now {
            RequestLinkStatus::Expired
        } else {
            RequestLinkStatus::Awaiting
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestLinkStatus {
    Awaiting,
    Submitted,
    Expired,
}

impl RequestLinkStatus {
    pub fn label(&self) -> &'static str {
        match self {
            RequestLinkStatus::Awaiting => "Awaiting reply",
            RequestLinkStatus::Submitted => "Submitted",
            RequestLinkStatus::Expired => "Expired",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IssueRequestLinkRequest {
    pub form_definition_id: Uuid,
    pub company_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_email: Option<String>,
}
