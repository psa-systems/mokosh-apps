//! Knowledge base pages.
//!
//! Wired to the mokosh-server KB module (PMS-79..84):
//!   - `GET/POST  /api/v1/kb/categories`
//!   - `PUT/DELETE /api/v1/kb/categories/{id}`
//!   - `GET/POST  /api/v1/kb/articles` (filters: `category_id`, `q`,
//!     `status`, `visibility`; paginated `{ data, meta: { total } }`)
//!   - `GET/PUT/DELETE /api/v1/kb/articles/{id}`
//!   - `GET  /api/v1/kb/articles/{id}/versions`
//!   - `POST /api/v1/kb/articles/{id}/versions/{n}/restore`
//!   - `POST /api/v1/kb/articles/{id}/helpful` and `/not_helpful`
//!
//! Structure and conventions mirror `crate::pages::contacts`: every
//! list/detail view reads `active_tenant_generation()` inside its
//! `use_resource` closure so an org switch / token swap re-fetches, the
//! string-returning `*_authed` API helpers carry the bearer token, and
//! loading / empty / error states match the contacts pages.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, ChevronRightIcon, CollapsibleRail,
    DataTable, IconSize, OverflowActions, PageHeader, PlusIcon, RailSide, SearchInput, Select,
    SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading,
    TableRow,
};
use crate::modules::kb::{
    CreateKbArticleRequest, KbArticle, KbArticleFeedback, KbArticleVersion, KbCategory,
    UpdateKbArticleRequest,
};
use crate::Route;

/// Rows per page for the article list (mirrors contacts `PER_PAGE`).
const PER_PAGE: usize = 25;

/// How many recent articles the home page surfaces.
const RECENT_LIMIT: usize = 5;

/// Server-side paginated envelope (`PaginatedResponse<T>`): `{ data, meta }`.
#[derive(Clone, Debug, Deserialize)]
struct Paginated<T> {
    data: Vec<T>,
    #[serde(default)]
    meta: PaginationMeta,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PaginationMeta {
    #[serde(default)]
    total: u64,
}

/// Tiny percent-encoder for query-string values, copied from
/// `contacts.rs` so the two pages stay consistent without pulling in the
/// full `urlencoding` crate. The server ILIKE / similarity-matches the
/// result so non-ASCII passes straight through.
fn urlencoding_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '+' => out.push_str("%2B"),
            '=' => out.push_str("%3D"),
            c if (c as u32) < 0x20 => out.push_str(&format!("%{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Derive a URL slug from a title: lowercase, non-alphanumerics collapse
/// to single hyphens, trimmed. Mirrors the obvious server expectation
/// (`slug` is required, `length(min = 1, max = 255)`). Empty input yields
/// `"article"` so we never POST an empty slug.
fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "article".to_string()
    } else {
        trimmed
    }
}

/// Resolve the chain of categories from root down to the article's own
/// category by walking `parent_id`. Returns `[]` when the article has no
/// category or the id is dangling. Guards against cycles with a visited
/// set so a malformed parent chain cannot loop forever.
fn resolve_category_path(category_id: Option<uuid::Uuid>, all: &[KbCategory]) -> Vec<KbCategory> {
    use std::collections::HashSet;
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = category_id;
    while let Some(id) = current {
        if !seen.insert(id) {
            break; // cycle guard
        }
        let Some(cat) = all.iter().find(|c| c.id == id) else {
            break;
        };
        chain.push(cat.clone());
        current = cat.parent_id;
    }
    chain.reverse();
    chain
}

/// A category node in the nav tree: the category, its child categories
/// (recursively, ordered by `sort_order` then name), and the articles
/// filed directly under it (ordered by title).
#[derive(Clone, PartialEq)]
struct TreeNode {
    category: KbCategory,
    children: Vec<TreeNode>,
    articles: Vec<KbArticle>,
}

/// Build the category forest (root categories with nested children and
/// their articles). Articles with no/unknown category go in a separate
/// "uncategorized" bucket returned as the second element.
fn build_kb_tree(
    categories: &[KbCategory],
    articles: &[KbArticle],
) -> (Vec<TreeNode>, Vec<KbArticle>) {
    fn node_for(cat: &KbCategory, cats: &[KbCategory], arts: &[KbArticle]) -> TreeNode {
        let mut children: Vec<TreeNode> = cats
            .iter()
            .filter(|c| c.parent_id == Some(cat.id))
            .map(|c| node_for(c, cats, arts))
            .collect();
        children.sort_by(|a, b| {
            a.category
                .sort_order
                .cmp(&b.category.sort_order)
                .then(a.category.name.cmp(&b.category.name))
        });
        let mut articles: Vec<KbArticle> = arts
            .iter()
            .filter(|a| a.category_id == Some(cat.id))
            .cloned()
            .collect();
        articles.sort_by(|a, b| a.title.cmp(&b.title));
        TreeNode {
            category: cat.clone(),
            children,
            articles,
        }
    }

    let known: std::collections::HashSet<_> = categories.iter().map(|c| c.id).collect();
    let mut roots: Vec<TreeNode> = categories
        .iter()
        .filter(|c| c.parent_id.is_none())
        .map(|c| node_for(c, categories, articles))
        .collect();
    roots.sort_by(|a, b| {
        a.category
            .sort_order
            .cmp(&b.category.sort_order)
            .then(a.category.name.cmp(&b.category.name))
    });
    let uncategorized: Vec<KbArticle> = articles
        .iter()
        .filter(|a| a.category_id.is_none() || a.category_id.is_some_and(|id| !known.contains(&id)))
        .cloned()
        .collect();
    (roots, uncategorized)
}

/// Title-case the server's lowercase visibility tag for display, and pick
/// a badge color. Unknown values fall through unchanged / gray.
fn visibility_label(raw: &str) -> (String, BadgeVariant) {
    match raw {
        "public" => ("Public".to_string(), BadgeVariant::Green),
        "internal" => ("Internal".to_string(), BadgeVariant::Blue),
        "client_specific" => ("Client-specific".to_string(), BadgeVariant::Purple),
        "" => ("Internal".to_string(), BadgeVariant::Blue),
        other => (other.to_string(), BadgeVariant::Gray),
    }
}

/// Badge color for an article status (`draft` / `published` / `archived`).
fn status_variant(raw: &str) -> BadgeVariant {
    match raw {
        "published" => BadgeVariant::Green,
        "draft" => BadgeVariant::Yellow,
        "archived" => BadgeVariant::Gray,
        _ => BadgeVariant::Gray,
    }
}

/// Truncate an ISO timestamp to its date portion for compact display.
/// The server returns RFC 3339 (`2026-06-05T12:34:56Z`); we show the
/// leading `YYYY-MM-DD`. Falls back to the raw string if it is shorter.
fn date_only(ts: &Option<String>) -> String {
    match ts {
        Some(s) if s.len() >= 10 => s[..10].to_string(),
        Some(s) => s.clone(),
        None => "-".to_string(),
    }
}

// ============================================================================
// Home page
// ============================================================================

/// Knowledge base home page: category grid + recent articles.
#[component]
pub fn KBHomePage() -> Element {
    let mut search = use_signal(String::new);
    let navigator = use_navigator();

    let categories_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbCategory>>(
            "/kb/categories?page=1&per_page=100",
            &token,
        )
        .await
        .ok()
    });

    let recent_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        let path =
            format!("/kb/articles?page=1&per_page={RECENT_LIMIT}&sort=updated_at&sort_dir=desc");
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbArticle>>(&path, &token)
            .await
            .ok()
    });

    let categories_snapshot = categories_resource.read_unchecked();
    let categories_loading = categories_snapshot.is_none();
    let categories: Vec<KbCategory> = match &*categories_snapshot {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };

    let recent_snapshot = recent_resource.read_unchecked();
    let recent_loading = recent_snapshot.is_none();
    let recent_failed = matches!(*recent_snapshot, Some(None));
    let recent: Vec<KbArticle> = match &*recent_snapshot {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };

    // Submitting the home search jumps to the full article list, which
    // owns the live `?q=` filter against the server. Routes carry no
    // query string, so the term is not forwarded; the list's own search
    // box is the canonical filter entry point.
    let go_search = move |e: FormEvent| {
        e.prevent_default();
        navigator.push(Route::KBArticleList {});
    };

    rsx! {
        AppLayout { title: "Knowledge Base",
            PageHeader {
                title: "Knowledge Base",
                subtitle: "Documentation and troubleshooting guides",
                actions: rsx! {
                    Link {
                        to: Route::KBArticleNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Article"
                        }
                    }
                },
            }

            // Search: a submit jumps to the full article list (which owns
            // the live `?q=` filter against the server).
            Card { class: "mb-6",
                form {
                    onsubmit: go_search,
                    SearchInput {
                        value: search.read().clone(),
                        placeholder: "Search articles...",
                        oninput: move |e: FormEvent| search.set(e.value()),
                    }
                }
            }

            // Categories
            if categories_loading {
                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 mb-8",
                    for _ in 0..3 {
                        Card {
                            div { class: "h-16 bg-gray-100 dark:bg-gray-800 rounded animate-pulse" }
                        }
                    }
                }
            } else if categories.is_empty() {
                Card { class: "mb-8",
                    div { class: "py-8 text-center text-sm text-gray-500",
                        "No categories yet."
                    }
                }
            } else {
                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 mb-8",
                    for category in categories.iter().cloned() {
                        CategoryCard {
                            key: "{category.id}",
                            title: category.name,
                            description: category.description.unwrap_or_default(),
                        }
                    }
                }
            }

            // Recent articles
            Card { title: "Recent Articles",
                if recent_failed {
                    div { class: "py-8 text-center text-sm text-red-600 dark:text-red-300",
                        "Could not load recent articles."
                    }
                } else if recent_loading {
                    div { class: "space-y-4",
                        for _ in 0..3 {
                            div { class: "h-10 bg-gray-100 dark:bg-gray-800 rounded animate-pulse" }
                        }
                    }
                } else if recent.is_empty() {
                    div { class: "py-8 text-center text-sm text-gray-500",
                        "No articles yet. Click New Article to create one."
                    }
                } else {
                    div { class: "space-y-4",
                        for article in recent.iter().cloned() {
                            ArticleItem {
                                key: "{article.id}",
                                id: article.id.to_string(),
                                title: article.title,
                                updated: date_only(&article.updated_at),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CategoryCardProps {
    title: String,
    description: String,
}

#[component]
fn CategoryCard(props: CategoryCardProps) -> Element {
    let navigator = use_navigator();
    // The list route carries no query string, so the category filter is
    // not pre-applied; clicking lands on the full article list where the
    // category dropdown is the canonical filter.
    rsx! {
        button {
            r#type: "button",
            class: "block w-full text-left",
            onclick: move |_| {
                navigator.push(Route::KBArticleList {});
            },
            Card { class: "hover:shadow-lg transition-shadow cursor-pointer",
                div { class: "flex items-start",
                    div { class: "flex-shrink-0 w-10 h-10 bg-blue-100 dark:bg-blue-900 rounded-lg flex items-center justify-center",
                        crate::components::BookIcon { class: "h-5 w-5 text-blue-600 dark:text-blue-400".to_string() }
                    }
                    div { class: "ml-4",
                        h3 { class: "text-lg font-medium text-gray-900 dark:text-white",
                            "{props.title}"
                        }
                        if !props.description.is_empty() {
                            p { class: "text-sm text-gray-500 dark:text-gray-400 mt-1",
                                "{props.description}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ArticleItemProps {
    id: String,
    title: String,
    updated: String,
}

#[component]
fn ArticleItem(props: ArticleItemProps) -> Element {
    rsx! {
        Link {
            to: Route::KBArticleDetail { id: props.id.clone() },
            class: "block p-4 -mx-4 hover:bg-gray-50 dark:hover:bg-gray-800 rounded-lg transition-colors",
            div { class: "flex items-center justify-between",
                div {
                    h4 { class: "font-medium text-gray-900 dark:text-white", "{props.title}" }
                }
                span { class: "text-sm text-gray-400", "{props.updated}" }
            }
        }
    }
}

// ============================================================================
// Article list page
// ============================================================================

/// Article list page: search box + category filter, server-paginated.
#[component]
pub fn KBArticleListPage() -> Element {
    let mut search = use_signal(String::new);
    let mut category_filter = use_signal(String::new);
    let mut page = use_signal(|| 1usize);

    // Rail collapse state, persisted under the same key as the detail view.
    let left_collapsed = use_signal(|| crate::utils::prefs::get_bool("kb_left_rail", false));
    let open_overlay = use_signal(|| None::<crate::components::RailSide>);
    use_effect(move || crate::utils::prefs::set_bool("kb_left_rail", left_collapsed()));

    // Category options for the filter dropdown and the tree rail.
    let categories_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbCategory>>(
            "/kb/categories?page=1&per_page=100",
            &token,
        )
        .await
        .ok()
    });
    let categories: Vec<KbCategory> = match &*categories_resource.read_unchecked() {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };
    let mut category_options = vec![SelectOption::new("", "All Categories")];
    for c in categories.iter() {
        category_options.push(SelectOption::new(c.id.to_string(), c.name.clone()));
    }

    // Article list feeding the tree rail (broader fetch than the paginated table).
    let tree_articles_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbArticle>>(
            "/kb/articles?page=1&per_page=200",
            &token,
        )
        .await
        .ok()
    });
    let tree_articles: Vec<KbArticle> = match &*tree_articles_resource.read_unchecked() {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };

    let search_text = search.read().trim().to_string();
    let category_text = category_filter.read().clone();
    let current_page = (*page.read()).max(1);

    let q_for_resource = search_text.clone();
    let cat_for_resource = category_text.clone();
    let articles_resource = use_resource(move || {
        let q = q_for_resource.clone();
        let category_id = cat_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let mut path = format!("/kb/articles?page={current_page}&per_page={PER_PAGE}");
            if !q.is_empty() {
                path.push_str(&format!("&q={}", urlencoding_minimal(&q)));
            }
            if !category_id.is_empty() {
                path.push_str(&format!(
                    "&category_id={}",
                    urlencoding_minimal(&category_id)
                ));
            }
            crate::hooks::fetch::api::get_with_auth::<Paginated<KbArticle>>(&path, &token)
                .await
                .ok()
        }
    });

    let resource_snapshot = articles_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let (page_rows, total): (Vec<KbArticle>, u64) = match &*resource_snapshot {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let has_filters = !search_text.is_empty() || !category_text.is_empty();

    rsx! {
        AppLayout { title: "Articles",
            PageHeader {
                title: "All Articles",
                actions: rsx! {
                    Link {
                        to: Route::KBArticleNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Article"
                        }
                    }
                },
            }

            div { class: "flex gap-6 items-start",
                CollapsibleRail { side: RailSide::Left, collapsed: left_collapsed, open_overlay,
                    KbTreeNav {
                        categories: categories.clone(),
                        articles: tree_articles.clone(),
                        current_id: String::new(),
                    }
                }
                div { class: "flex-1 min-w-0",
                    // Filters
                    Card { class: "mb-6",
                        div { class: "flex flex-col sm:flex-row gap-4",
                            div { class: "flex-1",
                                SearchInput {
                                    value: search.read().clone(),
                                    placeholder: "Search articles...",
                                    oninput: move |e: FormEvent| {
                                        search.set(e.value());
                                        page.set(1);
                                    },
                                }
                            }
                            Select {
                                name: "category",
                                options: category_options,
                                value: category_filter.read().clone(),
                                onchange: move |e: FormEvent| {
                                    category_filter.set(e.value());
                                    page.set(1);
                                },
                            }
                        }
                    }

                    if fetch_failed {
                        div {
                            class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                            "Could not load articles. Refresh the page to retry."
                        }
                    }

                    DataTable {
                        loading: is_loading,
                        total_items: total as usize,
                        current_page,
                        per_page: PER_PAGE,
                        columns: 4,
                        onpagechange: move |p| page.set(p),
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Title" }
                                    TableHeader { "Status" }
                                    TableHeader { "Visibility" }
                                    TableHeader { "Updated" }
                                }
                            }
                            if is_loading {
                                TableLoading { columns: 4, rows: 5 }
                            } else if page_rows.is_empty() {
                                TableEmpty {
                                    columns: 4,
                                    message: if has_filters {
                                        "No articles match your filters.".to_string()
                                    } else {
                                        "No articles yet. Click New Article to create one.".to_string()
                                    },
                                }
                            } else {
                                TableBody {
                                    for article in page_rows.iter().cloned() {
                                        ArticleRow {
                                            key: "{article.id}",
                                            id: article.id.to_string(),
                                            title: article.title,
                                            status: article.status,
                                            visibility: article.visibility,
                                            updated: date_only(&article.updated_at),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ArticleRowProps {
    id: String,
    title: String,
    status: String,
    visibility: String,
    updated: String,
}

#[component]
fn ArticleRow(props: ArticleRowProps) -> Element {
    let navigator = use_navigator();
    let id = props.id.clone();
    let (vis_label, vis_variant) = visibility_label(&props.visibility);
    let status_var = status_variant(&props.status);
    let status_label = if props.status.is_empty() {
        "Draft".to_string()
    } else {
        let mut chars = props.status.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => props.status.clone(),
        }
    };
    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::KBArticleDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::KBArticleDetail { id: props.id.clone() },
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.title}"
                }
            }
            TableCell { Badge { variant: status_var, "{status_label}" } }
            TableCell { Badge { variant: vis_variant, "{vis_label}" } }
            TableCell { class: "text-gray-500", "{props.updated}" }
        }
    }
}

// ============================================================================
// Article detail page
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct KBArticleDetailPageProps {
    pub id: String,
}

#[component]
pub fn KBArticleDetailPage(props: KBArticleDetailPageProps) -> Element {
    let id_for_article = props.id.clone();
    let id_for_versions = props.id.clone();

    let mut article_resource = use_resource(move || {
        let id = id_for_article.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<KbArticle>(&format!("/kb/articles/{id}"))
                .await
                .ok()
        }
    });

    let mut versions_resource = use_resource(move || {
        let id = id_for_versions.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<KbArticleVersion>>(&format!(
                "/kb/articles/{id}/versions?page=1&per_page=50"
            ))
            .await
            .ok()
        }
    });

    // Category list for the breadcrumb path and the left tree rail.
    let categories_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbCategory>>(
            "/kb/categories?page=1&per_page=100",
            &token,
        )
        .await
        .ok()
    });

    // Article list feeding the left tree rail.
    let tree_articles_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbArticle>>(
            "/kb/articles?page=1&per_page=200",
            &token,
        )
        .await
        .ok()
    });

    let categories: Vec<KbCategory> = match &*categories_resource.read_unchecked() {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };
    let tree_articles: Vec<KbArticle> = match &*tree_articles_resource.read_unchecked() {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };

    // Reading-view UI state, persisted per user via the prefs store.
    let left_collapsed = use_signal(|| crate::utils::prefs::get_bool("kb_left_rail", false));
    let right_collapsed = use_signal(|| crate::utils::prefs::get_bool("kb_right_rail", false));
    let read_mode = use_signal(|| crate::utils::prefs::get_bool("kb_read_mode", false));
    let comfortable = use_signal(|| crate::utils::prefs::get_bool("kb_density", true));
    let prior = use_signal(|| (false, false));
    let open_overlay = use_signal(|| None::<crate::components::RailSide>);
    use_effect(move || crate::utils::prefs::set_bool("kb_left_rail", left_collapsed()));
    use_effect(move || crate::utils::prefs::set_bool("kb_right_rail", right_collapsed()));

    let article_snapshot = article_resource.read_unchecked();
    let header_title = match &*article_snapshot {
        Some(Some(a)) => a.title.clone(),
        _ => "Article".to_string(),
    };

    rsx! {
        AppLayout { title: "{header_title}",
            match &*article_snapshot {
                None => rsx! {
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading article..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load article." }
                            Link {
                                to: Route::KBHome {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Back to knowledge base"
                            }
                        }
                    }
                },
                Some(Some(article)) => {
                    let (vis_label, vis_variant) = visibility_label(&article.visibility);
                    let status_label = if article.status.is_empty() {
                        "Draft".to_string()
                    } else {
                        article.status.clone()
                    };
                    let updated = date_only(&article.updated_at);
                    let content = article.content.clone();
                    let path = resolve_category_path(article.category_id, &categories);
                    let prose_density = if comfortable() {
                        "prose-base leading-relaxed"
                    } else {
                        "prose-sm leading-snug"
                    };
                    rsx! {
                        div { class: "flex gap-6 items-start",
                            CollapsibleRail { side: RailSide::Left, collapsed: left_collapsed, open_overlay,
                                KbTreeNav {
                                    categories: categories.clone(),
                                    articles: tree_articles.clone(),
                                    current_id: props.id.clone(),
                                }
                            }
                            div { class: "flex-1 min-w-0",
                                div { class: "flex items-center justify-between gap-2",
                                    KbBreadcrumb { path: path.clone(), title: article.title.clone() }
                                    ReadModeButton { read_mode, left_collapsed, right_collapsed, prior }
                                }
                                div { class: "mt-2 flex items-center justify-between gap-3 flex-wrap",
                                    h1 { class: "text-2xl font-semibold text-gray-900 dark:text-white truncate", "{article.title}" }
                                    OverflowActions {
                                        RatingBar {
                                            article_id: props.id.clone(),
                                            helpful: article.helpful_count,
                                            not_helpful: article.not_helpful_count,
                                        }
                                        Badge { variant: status_variant(&status_label), "{status_label}" }
                                        Badge { variant: vis_variant, "{vis_label}" }
                                        Link { to: Route::KBArticleEdit { id: props.id.clone() },
                                            Button { variant: ButtonVariant::Secondary, "Edit" }
                                        }
                                        DensityToggle { comfortable }
                                    }
                                }
                                p { class: "mt-1 text-xs text-gray-400", "Updated {updated}" }
                                article {
                                    class: "mt-4 prose dark:prose-invert max-w-none {prose_density}",
                                    dangerous_inner_html: crate::utils::markdown::render_markdown(&content),
                                }
                            }
                            CollapsibleRail { side: RailSide::Right, collapsed: right_collapsed, open_overlay,
                                VersionHistoryCard {
                                    article_id: props.id.clone(),
                                    versions_resource,
                                    on_restored: move |_| {
                                        article_resource.restart();
                                        versions_resource.restart();
                                    },
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn VersionHistoryCard(
    article_id: String,
    versions_resource: Resource<Option<Paginated<KbArticleVersion>>>,
    on_restored: EventHandler<()>,
) -> Element {
    let snap = versions_resource.read_unchecked();
    let mut restoring = use_signal(|| None::<i32>);

    rsx! {
        div { class: "rounded border border-gray-200 dark:border-gray-700",
            p { class: "px-3 py-2 text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700",
                "Versions"
            }
            match &*snap {
                None => rsx! {
                    p { class: "px-3 py-3 text-xs text-gray-400", "Loading..." }
                },
                Some(None) => rsx! {
                    p { class: "px-3 py-3 text-xs text-red-600 dark:text-red-300", "Could not load version history." }
                },
                Some(Some(page)) if page.data.is_empty() => rsx! {
                    p { class: "px-3 py-3 text-xs text-gray-400", "No prior versions." }
                },
                Some(Some(page)) => {
                    let rows = page.data.clone();
                    let article_id = article_id.clone();
                    rsx! {
                        ul { class: "divide-y divide-gray-100 dark:divide-gray-800",
                            for version in rows.into_iter() {
                                {
                                    let key = version.id.to_string();
                                    let n = version.version_number;
                                    let title = version.title.clone();
                                    let saved = date_only(&version.created_at);
                                    let id_for_restore = article_id.clone();
                                    rsx! {
                                        li { key: "{key}", class: "px-3 py-2 text-xs",
                                            div { class: "flex items-center justify-between gap-2",
                                                span { class: "font-medium text-gray-700 dark:text-gray-200", "v{n}" }
                                                button {
                                                    class: "text-blue-600 hover:text-blue-500 disabled:opacity-50",
                                                    disabled: *restoring.read() == Some(n),
                                                    onclick: move |_| {
                                                        if restoring.read().is_some() { return; }
                                                        restoring.set(Some(n));
                                                        let id = id_for_restore.clone();
                                                        spawn(async move {
                                                            #[cfg(feature = "web")]
                                                            {
                                                                let path = format!("/kb/articles/{id}/versions/{n}/restore");
                                                                if crate::hooks::fetch::api::post_authed::<KbArticle, _>(&path, &serde_json::json!({})).await.is_ok() {
                                                                    on_restored.call(());
                                                                }
                                                            }
                                                            #[cfg(not(feature = "web"))]
                                                            let _ = &id;
                                                            restoring.set(None);
                                                        });
                                                    },
                                                    "Restore"
                                                }
                                            }
                                            p { class: "mt-0.5 text-gray-500 dark:text-gray-400 truncate", "{title}" }
                                            p { class: "text-gray-400", "{saved}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

// ============================================================================
// New / edit article pages
// ============================================================================

/// Editable form values shared by the new and edit flows.
#[derive(Clone, Debug, Default, PartialEq)]
struct ArticleFormValues {
    title: String,
    slug: String,
    summary: String,
    category_id: String,
    visibility: String,
    status: String,
    content: String,
    tags: String,
}

#[derive(Clone, Debug, PartialEq)]
enum ArticleFormMode {
    Create,
    Edit { id: String },
}

/// New article page.
#[component]
pub fn KBArticleNewPage() -> Element {
    rsx! {
        AppLayout { title: "New Article",
            PageHeader { title: "New Article", subtitle: "Create a new knowledge base article" }
            ArticleForm {
                initial: ArticleFormValues {
                    visibility: "internal".to_string(),
                    status: "draft".to_string(),
                    ..Default::default()
                },
                mode: ArticleFormMode::Create,
            }
        }
    }
}

/// Edit article page.
#[derive(Props, Clone, PartialEq)]
pub struct KBArticleEditPageProps {
    pub id: String,
}

#[component]
pub fn KBArticleEditPage(props: KBArticleEditPageProps) -> Element {
    let id_for_resource = props.id.clone();
    let id_for_form = props.id.clone();
    let article_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<KbArticle>(&format!("/kb/articles/{id}"))
                .await
                .ok()
        }
    });
    let snap = article_resource.read_unchecked();
    rsx! {
        AppLayout { title: "Edit Article",
            PageHeader { title: "Edit Article" }
            match &*snap {
                None => rsx! {
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading article..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load article." }
                            Link {
                                to: Route::KBHome {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Back to knowledge base"
                            }
                        }
                    }
                },
                Some(Some(article)) => {
                    let initial = ArticleFormValues {
                        title: article.title.clone(),
                        slug: article.slug.clone(),
                        summary: article.summary.clone().unwrap_or_default(),
                        category_id: article.category_id.map(|c| c.to_string()).unwrap_or_default(),
                        visibility: if article.visibility.is_empty() {
                            "internal".to_string()
                        } else {
                            article.visibility.clone()
                        },
                        status: if article.status.is_empty() {
                            "draft".to_string()
                        } else {
                            article.status.clone()
                        },
                        content: article.content.clone(),
                        tags: article.tags.join(", "),
                    };
                    let id = id_for_form.clone();
                    rsx! {
                        ArticleForm {
                            initial,
                            mode: ArticleFormMode::Edit { id },
                        }
                    }
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ArticleFormProps {
    initial: ArticleFormValues,
    mode: ArticleFormMode,
}

#[component]
fn ArticleForm(props: ArticleFormProps) -> Element {
    let initial = props.initial.clone();
    let mode = props.mode.clone();

    let mut title = use_signal(|| initial.title.clone());
    let mut slug = use_signal(|| initial.slug.clone());
    let mut summary = use_signal(|| initial.summary.clone());
    let mut category_id = use_signal(|| initial.category_id.clone());
    let mut visibility = use_signal(|| {
        if initial.visibility.is_empty() {
            "internal".to_string()
        } else {
            initial.visibility.clone()
        }
    });
    let mut status = use_signal(|| {
        if initial.status.is_empty() {
            "draft".to_string()
        } else {
            initial.status.clone()
        }
    });
    let mut content = use_signal(|| initial.content.clone());
    let mut tags = use_signal(|| initial.tags.clone());
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Category dropdown options, fetched live.
    let categories_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbCategory>>(
            "/kb/categories?page=1&per_page=100",
            &token,
        )
        .await
        .ok()
    });
    let categories: Vec<KbCategory> = match &*categories_resource.read_unchecked() {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };
    let mut category_options = vec![SelectOption::new("", "Uncategorized")];
    for c in categories.iter() {
        category_options.push(SelectOption::new(c.id.to_string(), c.name.clone()));
    }

    let visibility_options = vec![
        SelectOption::new("internal", "Internal"),
        SelectOption::new("public", "Public"),
        SelectOption::new("client_specific", "Client-specific"),
    ];
    let status_options = vec![
        SelectOption::new("draft", "Draft"),
        SelectOption::new("published", "Published"),
        SelectOption::new("archived", "Archived"),
    ];

    let navigator = use_navigator();
    let is_edit = matches!(mode, ArticleFormMode::Edit { .. });
    let submit_label = if is_edit { "Save Changes" } else { "Publish" };

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        let title_val = title.read().trim().to_string();
        if title_val.is_empty() {
            error.set("Title is required.".to_string());
            return;
        }
        let content_val = content.read().to_string();
        if content_val.trim().is_empty() {
            error.set("Body is required.".to_string());
            return;
        }
        is_submitting.set(true);
        error.set(String::new());

        // Slug: use the author's value, else derive from the title.
        let slug_raw = slug.read().trim().to_string();
        let slug_val = if slug_raw.is_empty() {
            slugify(&title_val)
        } else {
            slug_raw
        };
        let summary_val = summary.read().trim().to_string();
        let summary_opt = if summary_val.is_empty() {
            None
        } else {
            Some(summary_val)
        };
        let category_opt = {
            let raw = category_id.read().clone();
            if raw.is_empty() {
                None
            } else {
                raw.parse::<uuid::Uuid>().ok()
            }
        };
        let visibility_val = visibility.read().clone();
        let status_val = status.read().clone();
        let tags_vec: Vec<String> = tags
            .read()
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        let mode = mode.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match &mode {
                    ArticleFormMode::Create => {
                        let body = CreateKbArticleRequest {
                            title: title_val.clone(),
                            slug: slug_val.clone(),
                            content: content_val.clone(),
                            summary: summary_opt.clone(),
                            category_id: category_opt,
                            visibility: visibility_val.clone(),
                            status: status_val.clone(),
                            tags: tags_vec.clone(),
                        };
                        crate::hooks::fetch::api::post_authed::<KbArticle, _>("/kb/articles", &body)
                            .await
                            .map(|a| a.id.to_string())
                    }
                    ArticleFormMode::Edit { id } => {
                        let body = UpdateKbArticleRequest {
                            title: Some(title_val.clone()),
                            slug: Some(slug_val.clone()),
                            content: Some(content_val.clone()),
                            summary: summary_opt.clone(),
                            category_id: category_opt,
                            visibility: Some(visibility_val.clone()),
                            status: Some(status_val.clone()),
                            tags: Some(tags_vec.clone()),
                        };
                        let path = format!("/kb/articles/{id}");
                        crate::hooks::fetch::api::put_authed::<KbArticle, _>(&path, &body)
                            .await
                            .map(|_| id.clone())
                    }
                };
                match result {
                    Ok(id) => {
                        navigator.push(Route::KBArticleDetail { id });
                    }
                    Err(err) => {
                        error.set(format!("Could not save article: {err}"));
                    }
                }
            }
            is_submitting.set(false);
        });
    };

    rsx! {
        Card {
            form {
                class: "space-y-6",
                onsubmit: handle_submit,

                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }

                crate::components::Input {
                    name: "title",
                    label: "Title",
                    placeholder: "How to ...",
                    required: true,
                    value: title.read().clone(),
                    oninput: move |e: FormEvent| title.set(e.value()),
                }

                crate::components::Input {
                    name: "slug",
                    label: "Slug",
                    placeholder: "Leave blank to derive from the title",
                    help: "URL-safe identifier; auto-generated from the title when blank.",
                    value: slug.read().clone(),
                    oninput: move |e: FormEvent| slug.set(e.value()),
                }

                crate::components::Input {
                    name: "summary",
                    label: "Summary",
                    placeholder: "Short one-line description (optional)",
                    value: summary.read().clone(),
                    oninput: move |e: FormEvent| summary.set(e.value()),
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-3",
                    Select {
                        name: "category",
                        label: "Category",
                        options: category_options,
                        value: category_id.read().clone(),
                        onchange: move |e: FormEvent| category_id.set(e.value()),
                    }
                    Select {
                        name: "visibility",
                        label: "Visibility",
                        options: visibility_options,
                        value: visibility.read().clone(),
                        onchange: move |e: FormEvent| visibility.set(e.value()),
                    }
                    Select {
                        name: "status",
                        label: "Status",
                        options: status_options,
                        value: status.read().clone(),
                        onchange: move |e: FormEvent| status.set(e.value()),
                    }
                }

                crate::components::Input {
                    name: "tags",
                    label: "Tags",
                    placeholder: "Comma-separated, e.g. vpn, network",
                    value: tags.read().clone(),
                    oninput: move |e: FormEvent| tags.set(e.value()),
                }

                crate::components::Textarea {
                    name: "content",
                    label: "Body (Markdown)",
                    placeholder: "## Overview\n\nWrite the article in Markdown...",
                    rows: 16,
                    required: true,
                    value: content.read().clone(),
                    oninput: move |e: FormEvent| content.set(e.value()),
                }
                p { class: "text-xs text-gray-500",
                    "Markdown source is stored verbatim; a rendered preview lands with the WYSIWYG editor."
                }

                div { class: "flex justify-end space-x-3",
                    Link {
                        to: Route::KBHome {},
                        Button { variant: ButtonVariant::Secondary, "Cancel" }
                    }
                    Button {
                        r#type: "submit",
                        variant: ButtonVariant::Primary,
                        loading: *is_submitting.read(),
                        "{submit_label}"
                    }
                }
            }
        }
    }
}

// ============================================================================
// Breadcrumb and tree-nav components
// ============================================================================

#[component]
fn KbBreadcrumb(path: Vec<KbCategory>, title: String) -> Element {
    rsx! {
        nav { class: "flex items-center flex-wrap gap-1 text-sm text-gray-500 dark:text-gray-400",
            Link { to: Route::KBHome {}, class: "hover:text-gray-700 dark:hover:text-gray-200", "KB" }
            for cat in path.iter() {
                ChevronRightIcon { size: IconSize::Small }
                Link {
                    to: Route::KBArticleList {},
                    class: "hover:text-gray-700 dark:hover:text-gray-200",
                    "{cat.name}"
                }
            }
            ChevronRightIcon { size: IconSize::Small }
            span { class: "text-gray-700 dark:text-gray-200 font-medium", "{title}" }
        }
    }
}

#[component]
fn KbTreeNav(categories: Vec<KbCategory>, articles: Vec<KbArticle>, current_id: String) -> Element {
    let (roots, uncategorized) = build_kb_tree(&categories, &articles);
    rsx! {
        nav { class: "text-sm space-y-1",
            for node in roots.iter() {
                KbTreeCategory { node: node.clone(), current_id: current_id.clone() }
            }
            if !uncategorized.is_empty() {
                div { class: "pt-2 mt-2 border-t border-gray-200 dark:border-gray-700",
                    p { class: "px-2 py-1 text-xs uppercase tracking-wide text-gray-400", "Uncategorized" }
                    for a in uncategorized.iter() {
                        KbTreeArticle { article: a.clone(), current_id: current_id.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn KbTreeCategory(node: TreeNode, current_id: String) -> Element {
    let mut open = use_signal(|| true);
    rsx! {
        div {
            button {
                class: "w-full flex items-center gap-1 px-2 py-1 rounded text-left text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800",
                onclick: move |_| open.toggle(),
                span {
                    class: if open() { "rotate-90 transition-transform" } else { "transition-transform" },
                    ChevronRightIcon { size: IconSize::Small }
                }
                span { class: "font-medium truncate", "{node.category.name}" }
            }
            if open() {
                div { class: "ml-4 border-l border-gray-200 dark:border-gray-700 pl-2 space-y-1",
                    for child in node.children.iter() {
                        KbTreeCategory { node: child.clone(), current_id: current_id.clone() }
                    }
                    for a in node.articles.iter() {
                        KbTreeArticle { article: a.clone(), current_id: current_id.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn KbTreeArticle(article: KbArticle, current_id: String) -> Element {
    let is_current = article.id.to_string() == current_id;
    let base = "block px-2 py-1 rounded truncate hover:bg-gray-100 dark:hover:bg-gray-800";
    let cls = if is_current {
        format!("{base} bg-blue-50 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300 font-medium")
    } else {
        format!("{base} text-gray-600 dark:text-gray-300")
    };
    rsx! {
        Link { to: Route::KBArticleDetail { id: article.id.to_string() }, class: "{cls}", "{article.title}" }
    }
}

// ============================================================================
// Rating bar, density toggle, read-mode button
// ============================================================================

#[component]
fn RatingBar(article_id: String, helpful: i32, not_helpful: i32) -> Element {
    let mut counts = use_signal(|| (helpful, not_helpful));
    let mut busy = use_signal(|| false);
    let (h, n) = counts();
    let id_h = article_id.clone();
    let id_n = article_id.clone();
    rsx! {
        div { class: "flex items-center gap-3 text-sm",
            button {
                class: "flex items-center gap-1 text-gray-600 dark:text-gray-300 hover:text-green-600 disabled:opacity-50",
                disabled: busy(),
                onclick: move |_| {
                    if busy() { return; }
                    busy.set(true);
                    let id = id_h.clone();
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/kb/articles/{id}/helpful");
                            if let Ok(fb) = crate::hooks::fetch::api::post_authed::<KbArticleFeedback, _>(&path, &serde_json::json!({})).await {
                                counts.set((fb.helpful_count, fb.not_helpful_count));
                            }
                        }
                        #[cfg(not(feature = "web"))]
                        let _ = &id;
                        busy.set(false);
                    });
                },
                span { "\u{1F44D}" }
                span { class: "tabular-nums", "{h}" }
            }
            button {
                class: "flex items-center gap-1 text-gray-600 dark:text-gray-300 hover:text-red-600 disabled:opacity-50",
                disabled: busy(),
                onclick: move |_| {
                    if busy() { return; }
                    busy.set(true);
                    let id = id_n.clone();
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/kb/articles/{id}/not_helpful");
                            if let Ok(fb) = crate::hooks::fetch::api::post_authed::<KbArticleFeedback, _>(&path, &serde_json::json!({})).await {
                                counts.set((fb.helpful_count, fb.not_helpful_count));
                            }
                        }
                        #[cfg(not(feature = "web"))]
                        let _ = &id;
                        busy.set(false);
                    });
                },
                span { "\u{1F44E}" }
                span { class: "tabular-nums", "{n}" }
            }
        }
    }
}

#[component]
fn DensityToggle(comfortable: Signal<bool>) -> Element {
    let mut comfortable = comfortable;
    rsx! {
        button {
            class: "text-xs px-2 py-1 rounded border border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300",
            title: "Toggle reading density",
            onclick: move |_| {
                let next = !comfortable();
                comfortable.set(next);
                crate::utils::prefs::set_bool("kb_density", next);
            },
            if comfortable() { "Comfortable" } else { "Compact" }
        }
    }
}

#[component]
fn ReadModeButton(
    read_mode: Signal<bool>,
    left_collapsed: Signal<bool>,
    right_collapsed: Signal<bool>,
    prior: Signal<(bool, bool)>,
) -> Element {
    let mut read_mode = read_mode;
    let mut left_collapsed = left_collapsed;
    let mut right_collapsed = right_collapsed;
    let mut prior = prior;
    rsx! {
        button {
            class: "p-1 text-gray-500 hover:text-gray-700 dark:hover:text-gray-200",
            title: if read_mode() { "Exit read mode" } else { "Read mode (hide panels)" },
            onclick: move |_| {
                if read_mode() {
                    let (l, r) = prior();
                    left_collapsed.set(l);
                    right_collapsed.set(r);
                    read_mode.set(false);
                    crate::utils::prefs::set_bool("kb_read_mode", false);
                } else {
                    prior.set((left_collapsed(), right_collapsed()));
                    left_collapsed.set(true);
                    right_collapsed.set(true);
                    read_mode.set(true);
                    crate::utils::prefs::set_bool("kb_read_mode", true);
                }
            },
            if read_mode() { "\u{2921}" } else { "\u{2922}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn cat(id: Uuid, parent: Option<Uuid>, name: &str) -> KbCategory {
        KbCategory {
            id,
            name: name.to_string(),
            description: None,
            parent_id: parent,
            slug: name.to_lowercase(),
            visibility: "internal".into(),
            sort_order: 0,
        }
    }

    #[test]
    fn resolves_nested_path_root_first() {
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let cats = vec![cat(root, None, "Networking"), cat(child, Some(root), "VPN")];
        let path = resolve_category_path(Some(child), &cats);
        let names: Vec<_> = path.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Networking", "VPN"]);
    }

    #[test]
    fn empty_when_no_category() {
        let cats = vec![cat(Uuid::new_v4(), None, "X")];
        assert!(resolve_category_path(None, &cats).is_empty());
    }

    #[test]
    fn empty_when_dangling() {
        let cats = vec![cat(Uuid::new_v4(), None, "X")];
        assert!(resolve_category_path(Some(Uuid::new_v4()), &cats).is_empty());
    }

    fn art(id: Uuid, category: Option<Uuid>, title: &str) -> KbArticle {
        KbArticle {
            id,
            title: title.to_string(),
            slug: title.to_lowercase(),
            content: String::new(),
            summary: None,
            category_id: category,
            visibility: "internal".into(),
            status: "published".into(),
            view_count: 0,
            helpful_count: 0,
            not_helpful_count: 0,
            published_at: None,
            tags: vec![],
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn nests_children_and_articles() {
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let cats = vec![cat(root, None, "Net"), cat(child, Some(root), "VPN")];
        let arts = vec![art(Uuid::new_v4(), Some(child), "Setup")];
        let (roots, uncat) = build_kb_tree(&cats, &arts);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].children.len(), 1);
        assert_eq!(roots[0].children[0].articles[0].title, "Setup");
        assert!(uncat.is_empty());
    }

    #[test]
    fn uncategorized_bucket_collects_orphans() {
        let arts = vec![art(Uuid::new_v4(), None, "Loose")];
        let (roots, uncat) = build_kb_tree(&[], &arts);
        assert!(roots.is_empty());
        assert_eq!(uncat.len(), 1);
    }
}
