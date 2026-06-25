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
/// The server's `KbArticleResponse` also carries `author_id`. This client
/// omits it on purpose: serde drops the unknown key, nothing here displays
/// the author, so there is no fix to make. Add
/// `#[serde(default)] pub author_id: Option<Uuid>` only if author display
/// is wanted later (MAPPS-138, recorded no-fix).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct KbArticle {
    pub id: Uuid,
    #[serde(default)]
    pub title: String,
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
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// `KbArticleVersionResponse` subset for the version-history list.
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
    pub created_at: Option<DateTime<Utc>>,
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
}

/// PMS-485: one row of the `/kb/top-ticket-driving-articles` feed used
/// by the KB landing-page widget.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TopTicketDrivingArticle {
    pub id: Uuid,
    pub title: String,
    pub ticket_count: i64,
}
