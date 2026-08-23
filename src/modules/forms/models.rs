//! Form DTOs.
//!
//! MAPPS-535: the wire types come from `mokosh_types::forms` (PMS-898) rather
//! than being declared here. This module carried a hand copy under its own
//! names, and `src/pages/request_form.rs` carried a second one for the public
//! subset; forms was the last module in either repo doing that.
//!
//! What stays below is the part that is not a wire type. `RequestLinkStatus`
//! is a client-side reading of `used_at` and `expires_at`, not a field the
//! server sends, and the `FieldTypeExt` helpers are operator-facing
//! presentation the server has no use for. `FieldType` is a foreign type now,
//! so those cannot be inherent impls.

pub use mokosh_types::forms::*;

// The names the pages already use. Aliases, not copies: `pub use ... as ...`
// cannot drift from what it points at, which is the whole reason this module
// stopped declaring these types. The server's names are the longer
// `*Response` / `*Request` forms because it has both sides of each exchange;
// the SPA only ever sees one, so the short name reads better at the call site.
pub use mokosh_types::forms::CreateFormFieldRequest as UpsertFormField;
pub use mokosh_types::forms::FormDefinitionResponse as FormDefinition;
pub use mokosh_types::forms::FormFieldResponse as FormField;
pub use mokosh_types::forms::RequestLinkResponse as RequestLink;

use chrono::{DateTime, Utc};

/// SPA-only presentation for the shared [`FieldType`].
///
/// An extension trait rather than inherent methods, because the type is
/// defined in another crate. `as_str` and `from_str` are NOT here: the shared
/// type already carries them, and a second spelling of the wire value is
/// exactly the drift this adoption removes.
pub trait FieldTypeExt {
    /// Every type, in the order the builder's picker offers them.
    ///
    /// `FieldType::Unknown` is absent on purpose: it is the catch-all a newer
    /// server's type deserialises into, not a type an operator can pick. The
    /// server refuses it on write (PMS-898), so offering it would build a
    /// definition that cannot be saved.
    const ALL: [FieldType; 6] = [
        FieldType::Text,
        FieldType::Textarea,
        FieldType::Email,
        FieldType::Date,
        FieldType::Select,
        FieldType::Boolean,
    ];

    /// Operator-facing name for the type picker.
    fn label(&self) -> &'static str;

    /// Whether an option set is required. The server rejects a `select` with
    /// no options at write time, so the builder blocks it first.
    fn needs_options(&self) -> bool;

    /// Whether a character-length bound means anything. The server ignores
    /// bounds on other types rather than erroring, but showing the input would
    /// imply it does something.
    fn honours_length(&self) -> bool;
}

impl FieldTypeExt for FieldType {
    fn label(&self) -> &'static str {
        match self {
            FieldType::Text => "Short text",
            FieldType::Textarea => "Long text",
            FieldType::Email => "Email address",
            FieldType::Date => "Date",
            FieldType::Select => "Choice list",
            FieldType::Boolean => "Yes / no",
            // A type this build does not know, sent by a newer server. The
            // builder renders it read-only rather than mislabelling it.
            FieldType::Unknown => "Unsupported type",
        }
    }

    fn needs_options(&self) -> bool {
        matches!(self, FieldType::Select)
    }

    fn honours_length(&self) -> bool {
        matches!(
            self,
            FieldType::Text | FieldType::Textarea | FieldType::Email
        )
    }
}

/// What has become of a request link.
///
/// Client-side, and an extension trait for the same reason as [`FieldTypeExt`]:
/// the server sends `used_at` and `expires_at` and no status, so this is a
/// reading of those two rather than a field on the wire.
pub trait RequestLinkExt {
    /// Submitted wins over expired, deliberately: a link used before it lapsed
    /// is still a request that came in, and calling it expired would read as
    /// though the client never replied.
    fn status(&self, now: DateTime<Utc>) -> RequestLinkStatus;
}

impl RequestLinkExt for RequestLink {
    fn status(&self, now: DateTime<Utc>) -> RequestLinkStatus {
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
