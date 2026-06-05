//! Knowledge base client-side DTOs.
//!
//! These mirror the response/request shapes in mokosh-server's
//! `src/modules/knowledge_base/models.rs` but only carry the fields the
//! SPA reads or sends. Responses derive `Deserialize`; request bodies
//! derive `Serialize`. Every struct derives `PartialEq` because Dioxus
//! `Props` (and any value passed as a prop) requires it, and `#[serde(default)]`
//! guards every optional/collection field so the server adding columns
//! never breaks decoding.

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
    pub published_at: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
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
    pub created_at: Option<String>,
}

/// `KbArticleFeedbackResponse`: returned by the helpful / not_helpful
/// endpoints so the detail page can re-render tallies without a GET.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct KbArticleFeedback {
    pub id: Uuid,
    #[serde(default)]
    pub helpful_count: i32,
    #[serde(default)]
    pub not_helpful_count: i32,
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
