//! Knowledge base client-side DTOs.
//!
//! These mirror the response/request shapes in mokosh-server's
//! `src/modules/knowledge_base/models.rs` but only carry the fields the
//! SPA reads or sends. Responses derive `Deserialize`; request bodies
//! derive `Serialize`. Every struct derives `PartialEq` because Dioxus
//! `Props` (and any value passed as a prop) requires it, and `#[serde(default)]`
//! guards every optional/collection field so the server adding columns
//! never breaks decoding.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `KbCategoryResponse` subset. The category grid keys badges/links on
/// `id`, `name`, `slug`, `description`, and `visibility`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct KbCategory {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub sort_order: i32,
}

/// `KbArticleResponse` subset. Shared by the agent article views and the
/// portal feed (the server returns the same DTO from
/// `GET /api/v1/portal/kb`).
///
/// MAPPS-739 (reversing the MAPPS-138 no-fix): the header prints who wrote
/// and who last edited the article, so the author and the editor ride
/// along as the display NAMES the server resolves (PMS-1126), never as
/// ids. The contact projection carries none of these, so every one
/// defaults, and the page prints attribution only on a staff session.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct KbArticle {
    pub id: Uuid,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author_id: Option<Uuid>,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub updated_by_id: Option<Uuid>,
    #[serde(default)]
    pub updated_by_name: Option<String>,
    /// The highest version number, so the history card can mark the row
    /// that is the live article. `0` for a contact read, which never shows
    /// the card.
    #[serde(default)]
    pub current_version: i32,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub category_id: Option<Uuid>,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub view_count: i32,
    #[serde(default)]
    pub helpful_count: i32,
    #[serde(default)]
    pub not_helpful_count: i32,
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// MAPPS-515: companies a `client_specific` article is scoped to; empty
    /// for `public` / `internal`. The article form round-trips this set, and
    /// the portal filter matches on it, so an empty set on a client-specific
    /// article means no client can see it.
    #[serde(default)]
    pub company_ids: Vec<Uuid>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// `KbArticleVersionResponse` subset for the version-history list.
///
/// MAPPS-739: a version says who wrote it, what kind of change it was
/// (`create`, `edit`, `restore`), why, and for a restore which version it
/// brought back (PMS-1126).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct KbArticleVersion {
    pub id: Uuid,
    pub article_id: Uuid,
    pub version_number: i32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub edited_by_name: Option<String>,
    #[serde(default)]
    pub change_note: Option<String>,
    #[serde(default)]
    pub change_kind: String,
    #[serde(default)]
    pub restored_from_version: Option<i32>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// MAPPS-739: the body of `POST /kb/articles/{id}/versions/{n}/restore`.
#[derive(Clone, Debug, PartialEq, Serialize, Default)]
pub struct RestoreKbArticleVersionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_note: Option<String>,
}

/// `KbArticleFeedbackResponse`: returned by the helpful / not_helpful
/// toggle endpoints and by `GET /kb/articles/{id}/vote`, so the detail
/// page can re-render tallies and the caller's current vote without a
/// full article reload.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct KbArticleFeedback {
    pub id: Uuid,
    #[serde(default)]
    pub helpful_count: i32,
    #[serde(default)]
    pub not_helpful_count: i32,
    /// `"helpful"` | `"not_helpful"` | `null` (null = no vote / cleared).
    #[serde(default)]
    pub my_vote: Option<String>,
}

/// `CreateKbCategoryRequest`. Mirrors the server's create body. `slug` is
/// required server-side; the category form derives one from the name when the
/// author leaves it blank.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CreateKbCategoryRequest {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub visibility: String,
    pub sort_order: i32,
}

/// `UpdateKbCategoryRequest`. Every field is optional; the edit form sends the
/// full set so omitted-vs-cleared is unambiguous (mirrors
/// [`UpdateKbArticleRequest`]).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpdateKbCategoryRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub visibility: Option<String>,
    pub sort_order: Option<i32>,
}

/// `CreateKbArticleRequest`. `slug` is required server-side; the new-article
/// form derives one from the title when the author leaves it blank.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CreateKbArticleRequest {
    pub title: String,
    pub slug: String,
    pub content: String,
    pub summary: Option<String>,
    pub category_id: Option<Uuid>,
    pub visibility: String,
    pub status: String,
    pub tags: Vec<String>,
    /// MAPPS-515: required (non-empty) when `visibility = client_specific`;
    /// `None` for any other visibility, which the server stores as an empty
    /// scope.
    pub company_ids: Option<Vec<Uuid>>,
}

/// One uploaded image, as `POST /kb/articles/{id}/attachments` returns it
/// (MAPPS-587, server side PMS-923).
///
/// Only `url` is read today, and it is the reason the rest is here: the field
/// is relative on purpose, because nothing server-side can know the API base
/// the SPA is talking to. It goes into the Markdown as-is and the browser
/// resolves it against the page.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct KbAttachmentResponse {
    pub id: Uuid,
    pub article_id: Uuid,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: i64,
    pub url: String,
}

/// `UpdateKbArticleRequest`. Every field is optional; the edit form sends
/// the full set so omitted-vs-cleared is unambiguous.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpdateKbArticleRequest {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub content: Option<String>,
    pub summary: Option<String>,
    pub category_id: Option<Uuid>,
    pub visibility: Option<String>,
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
    /// MAPPS-515: the company scope. `Some(ids)` when the submitted
    /// visibility is `client_specific`; `None` otherwise, since the server
    /// clears the stored scope for any other visibility anyway.
    pub company_ids: Option<Vec<Uuid>>,
    /// MAPPS-739: why this edit was made; the server stores it on the
    /// version the save creates and drops it when the save creates none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_note: Option<String>,
}

/// PMS-485: one row of the `/kb/top-ticket-driving-articles` feed used
/// by the KB landing-page widget.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TopTicketDrivingArticle {
    pub id: Uuid,
    pub title: String,
    pub ticket_count: i64,
}

/// PMS-732: what the tracked time says a request documented by this article
/// actually takes (`GET /kb/articles/{id}/measured-duration`).
///
/// Every measurement field is `Option` and they move together: an article no
/// request type has tracked time against reports `null`, not zero. Zero
/// minutes would be a measurement ("these take no time"), and rendering that
/// as a confident estimate is worse than the hand-written guess this replaces,
/// so the card treats null as "no data yet" and says so.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ArticleMeasuredDuration {
    /// Start of the window the figure covers. Always present, so the number
    /// is never ambiguous about what it measured.
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub ticket_count: Option<i64>,
    #[serde(default)]
    pub total_minutes: Option<i64>,
    #[serde(default)]
    pub average_minutes: Option<f64>,
}
