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

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

use crate::components::{
    kb_article_status_badge, AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card,
    ChevronRightIcon, CollapsibleRail, ConfirmDialog, DataTable, IconSize, Modal, ModalSize,
    OverflowActions, PageHeader, PencilIcon, PlusIcon, RailSide, SearchInput, Select, SelectOption,
    Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
    TrashIcon,
};
use crate::modules::kb::{
    CreateKbArticleRequest, CreateKbCategoryRequest, KbArticle, KbArticleFeedback,
    KbArticleVersion, KbCategory, TopTicketDrivingArticle, UpdateKbArticleRequest,
    UpdateKbCategoryRequest,
};
use crate::utils::url::urlencoding_minimal;
use crate::utils::{FormGuard, Paginated, Rule};
use crate::Route;

/// Rows per page for the article list (mirrors contacts `PER_PAGE`).
const PER_PAGE: usize = 25;

/// How many articles to pull when a tag filter is active. The server
/// cannot narrow on tags (its `q` filter ignores them), so the list fetches
/// a broad page and filters by tag client-side. Matches the tree rail's
/// broad fetch cap.
const TAG_FETCH_LIMIT: usize = 200;

/// How many recent articles the home page surfaces.
const RECENT_LIMIT: usize = 5;

/// Client-side `maxlength` caps for the article form's text fields, mirroring
/// the server's KB article column limits so over-long input is blocked at the
/// field rather than failing as an opaque 422 (MAPPS-218). The slug column is
/// `length(min = 1, max = 255)` (see [`slugify`]); title and summary share the
/// 255-char varchar cap; the body is a large TEXT column, capped here only to
/// stop an absurd paste. The server stays the source of truth.
const TITLE_MAX: usize = 255;
const SLUG_MAX: usize = 255;
const SUMMARY_MAX: usize = 255;
const BODY_MAX: usize = 100_000;

/// Per-tag character cap. Tags render as inline chips and feed the tag
/// taxonomy (search/filter), so a single tag longer than this is almost
/// certainly garbage (MAPPS-218).
const TAG_MAX: usize = 50;

/// Split a comma-separated tag string into clean tags: strip markup (`<`/`>`)
/// and control characters, trim, cap each tag at [`TAG_MAX`] characters, and
/// drop empties. Mirrors the issue's "tags are length-capped and stripped of
/// markup/control characters" requirement so junk like `<script>` or an
/// unbounded blob never reaches the tag taxonomy (MAPPS-218).
fn sanitize_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .filter_map(|t| {
            let cleaned: String = t
                .chars()
                .filter(|c| !c.is_control() && !matches!(c, '<' | '>'))
                .collect();
            let capped: String = cleaned.trim().chars().take(TAG_MAX).collect();
            let tag = capped.trim().to_string();
            if tag.is_empty() {
                None
            } else {
                Some(tag)
            }
        })
        .collect()
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

/// Decide the slug to apply as the title changes: follow the title
/// (via `slugify`) until the user has manually edited the slug, after
/// which leave it alone. Returns None to leave the slug untouched.
fn next_slug(new_title: &str, touched: bool) -> Option<String> {
    if touched {
        None
    } else {
        Some(slugify(new_title))
    }
}

/// Tab state for the article body editor.
#[derive(Clone, Copy, PartialEq)]
enum BodyTab {
    Write,
    Preview,
}

fn body_tab_class(active: bool) -> &'static str {
    if active {
        "px-3 py-1 text-sm border-b-2 border-accent text-accent font-medium"
    } else {
        "px-3 py-1 text-sm border-b-2 border-transparent text-muted hover:text-content"
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

/// Truncate an ISO timestamp to its date portion for compact display.
/// The server returns RFC 3339 (`2026-06-05T12:34:56Z`); we show the
/// leading `YYYY-MM-DD`. Falls back to the raw string if it is shorter.
fn date_only(ts: &Option<DateTime<Utc>>) -> String {
    match ts {
        Some(dt) => dt.format("%Y-%m-%d").to_string(),
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

    let mut categories_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbCategory>>(
            "/kb/categories?page=1&per_page=100",
            &token,
        )
        .await
        .ok()
    });

    // Category CRUD UI state (MAPPS-230). `category_form` drives the
    // create/edit modal (`None` = closed); `deleting_category` drives the
    // delete confirmation. Both restart `categories_resource` on success so
    // the grid reflects the change without a full reload.
    let mut category_form = use_signal(|| None::<CategoryEdit>);
    let mut deleting_category = use_signal(|| None::<KbCategory>);
    let mut delete_busy = use_signal(|| false);
    let mut delete_error = use_signal(String::new);

    let recent_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        // No `sort`/`sort_dir`: the server `KbService::list_articles`
        // hardcodes its ORDER BY and silently ignores those params, so
        // sending them was a no-op (MAPPS-138).
        let path = format!("/kb/articles?page=1&per_page={RECENT_LIMIT}");
        crate::hooks::fetch::api::get_with_auth::<Paginated<KbArticle>>(&path, &token)
            .await
            .ok()
    });

    // PMS-485: top ticket-driving articles widget. Server endpoint
    // defaults to a 90-day window; we ask for the top 10 to fit the
    // landing-page card.
    let top_driving_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<Vec<TopTicketDrivingArticle>>(
            "/kb/top-ticket-driving-articles?limit=10",
            &token,
        )
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

    let top_driving_snapshot = top_driving_resource.read_unchecked();
    let top_driving_loading = top_driving_snapshot.is_none();
    let top_driving: Vec<TopTicketDrivingArticle> = match &*top_driving_snapshot {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    // Submitting the home search jumps to the full article list, which
    // owns the live `?q=` filter against the server. The typed term is
    // forwarded via the route's `?q=` query so the list opens pre-filtered.
    let go_search = move |e: FormEvent| {
        e.prevent_default();
        let q = search.read().trim().to_string();
        navigator.push(Route::KBArticleList {
            q,
            tag: String::new(),
            category: String::new(),
        });
    };

    rsx! {
        AppLayout { title: "Knowledge Base",
            PageHeader {
                title: "Knowledge Base",
                subtitle: "Documentation and troubleshooting guides",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| category_form.set(Some(CategoryEdit::New)),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Category"
                    }
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
                            div { class: "h-16 bg-surface-2 rounded animate-pulse" }
                        }
                    }
                }
            } else if categories.is_empty() {
                Card { class: "mb-8",
                    div { class: "py-8 text-center text-sm text-muted",
                        "No categories yet."
                    }
                }
            } else {
                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 mb-8",
                    for category in categories.iter().cloned() {
                        CategoryCard {
                            key: "{category.id}",
                            category: category.clone(),
                            on_edit: move |c: KbCategory| category_form.set(Some(CategoryEdit::Existing(c))),
                            on_delete: move |c: KbCategory| {
                                delete_error.set(String::new());
                                deleting_category.set(Some(c));
                            },
                        }
                    }
                }
            }

            // PMS-485: Top ticket-driving articles widget. Reads
            // tickets joined to kb_articles on `source_kb_article_id`
            // (stamped on ticket create by PMS-482), grouped + ordered
            // by count over the trailing 90 days.
            Card { title: "Top ticket-driving articles",
                if top_driving_loading {
                    div { class: "space-y-2",
                        for _ in 0..3 {
                            div { class: "h-8 bg-surface-2 rounded animate-pulse" }
                        }
                    }
                } else if top_driving.is_empty() {
                    div { class: "py-6 text-center text-sm text-muted",
                        "No ticket-driving articles yet. Use 'Open ticket about this article' on a KB page to start tracking."
                    }
                } else {
                    ul { class: "divide-y divide-border",
                        for row in top_driving.iter().cloned() {
                            li { class: "py-2 flex items-center justify-between gap-3",
                                Link {
                                    to: Route::KBArticleDetail { id: row.id.to_string() },
                                    class: "text-accent hover:underline truncate",
                                    "{row.title}"
                                }
                                Badge { variant: BadgeVariant::Gray, "{row.ticket_count}" }
                            }
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
                            div { class: "h-10 bg-surface-2 rounded animate-pulse" }
                        }
                    }
                } else if recent.is_empty() {
                    div { class: "py-8 text-center text-sm text-muted",
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

            // Create / edit category modal (MAPPS-230).
            if let Some(edit) = category_form() {
                CategoryFormModal {
                    edit,
                    categories: categories.clone(),
                    on_close: move |_| category_form.set(None),
                    on_saved: move |_| {
                        category_form.set(None);
                        categories_resource.restart();
                    },
                }
            }

            // Delete category confirmation (MAPPS-230).
            ConfirmDialog {
                open: deleting_category.read().is_some(),
                title: "Delete category".to_string(),
                message: {
                    let name = deleting_category
                        .read()
                        .as_ref()
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    let mut msg = format!(
                        "Delete the \"{name}\" category? Articles filed under it are not deleted; \
                         they become uncategorized."
                    );
                    if !delete_error.read().is_empty() {
                        msg.push_str(&format!("\n\n{}", delete_error.read()));
                    }
                    msg
                },
                confirm_text: "Delete".to_string(),
                destructive: true,
                loading: *delete_busy.read(),
                oncancel: move |_| {
                    deleting_category.set(None);
                    delete_error.set(String::new());
                },
                onconfirm: move |_| {
                    let Some(target) = deleting_category.read().clone() else {
                        return;
                    };
                    if *delete_busy.read() {
                        return;
                    }
                    delete_busy.set(true);
                    delete_error.set(String::new());
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/kb/categories/{}", target.id);
                            match crate::hooks::fetch::api::delete_authed(&path).await {
                                Ok(()) => {
                                    deleting_category.set(None);
                                    categories_resource.restart();
                                }
                                Err(err) => {
                                    delete_error.set(format!("Could not delete category: {err}"));
                                }
                            }
                        }
                        #[cfg(not(feature = "web"))]
                        let _ = &target;
                        delete_busy.set(false);
                    });
                },
            }
        }
    }
}

/// Whether the category modal is creating a new category or editing an
/// existing one (MAPPS-230).
#[derive(Clone, PartialEq)]
enum CategoryEdit {
    New,
    Existing(KbCategory),
}

#[derive(Props, Clone, PartialEq)]
struct CategoryCardProps {
    category: KbCategory,
    on_edit: EventHandler<KbCategory>,
    on_delete: EventHandler<KbCategory>,
}

#[component]
fn CategoryCard(props: CategoryCardProps) -> Element {
    let navigator = use_navigator();
    let category_id = props.category.id.to_string();
    let title = props.category.name.clone();
    let description = props.category.description.clone().unwrap_or_default();
    let edit_target = props.category.clone();
    let delete_target = props.category.clone();
    // Clicking the card body lands on the article list pre-filtered to this
    // category: the id rides the route's `?category=` query so the list
    // pre-selects the dropdown and fetches `GET /kb/articles?category_id=...`.
    // The edit/delete buttons (MAPPS-230) sit in the top-right corner and
    // stop propagation so they never trigger that navigation.
    rsx! {
        Card { class: "relative hover:shadow-lg transition-shadow",
            div {
                class: "absolute top-2 right-2 flex gap-1",
                onclick: move |e| e.stop_propagation(),
                button {
                    r#type: "button",
                    class: "p-1 rounded text-subtle hover:text-accent hover:bg-surface-2",
                    title: "Edit category",
                    "aria-label": "Edit category",
                    onclick: move |_| props.on_edit.call(edit_target.clone()),
                    PencilIcon { size: IconSize::Small }
                }
                button {
                    r#type: "button",
                    class: "p-1 rounded text-subtle hover:text-red-600 hover:bg-surface-2",
                    title: "Delete category",
                    "aria-label": "Delete category",
                    onclick: move |_| props.on_delete.call(delete_target.clone()),
                    TrashIcon { size: IconSize::Small }
                }
            }
            button {
                r#type: "button",
                class: "block w-full text-left cursor-pointer pr-12",
                onclick: move |_| {
                    navigator.push(Route::KBArticleList {
                        q: String::new(),
                        tag: String::new(),
                        category: category_id.clone(),
                    });
                },
                div { class: "flex items-start",
                    div { class: "flex-shrink-0 w-10 h-10 bg-accent-100 dark:bg-accent-900 rounded-lg flex items-center justify-center",
                        crate::components::BookIcon { class: "h-5 w-5 text-accent".to_string() }
                    }
                    div { class: "ml-4",
                        h3 { class: "text-lg font-medium text-content",
                            "{title}"
                        }
                        if !description.is_empty() {
                            p { class: "text-sm text-muted mt-1",
                                "{description}"
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
            class: "block p-4 -mx-4 hover:bg-surface-2 rounded-lg transition-colors",
            div { class: "flex items-center justify-between",
                div {
                    h4 { class: "font-medium text-content", "{props.title}" }
                }
                span { class: "text-sm text-subtle", "{props.updated}" }
            }
        }
    }
}

// ============================================================================
// Article list page
// ============================================================================

/// Article list page: search box + category filter, server-paginated.
///
/// `initial_q` seeds the search box from the `?q=` route query so the KB
/// home search carries the typed term through to this list. `initial_tag`
/// seeds the tag filter from the `?tag=` route query so clicking a tag chip
/// (on a row or the detail view) lands on the list filtered to that tag.
/// `initial_category` seeds the category dropdown from the `?category=` route
/// query so clicking a category card (or breadcrumb) lands on the list
/// pre-filtered to that category.
#[component]
pub fn KBArticleListPage(
    #[props(default)] initial_q: String,
    #[props(default)] initial_tag: String,
    #[props(default)] initial_category: String,
) -> Element {
    let mut search = use_signal(|| initial_q.clone());
    let mut category_filter = use_signal(|| initial_category.clone());
    let mut tag_filter = use_signal(|| initial_tag.clone());
    let mut page = use_signal(|| 1usize);
    let navigator = use_navigator();

    // Keep the tag filter in sync with the `?tag=` route query: navigating
    // from one tag chip to another re-renders this component with a new
    // `initial_tag` prop without remounting, so seeding the signal once is
    // not enough. `use_reactive` re-runs the effect whenever the prop
    // changes; `peek` reads the signal without subscribing the effect to it.
    use_effect(use_reactive!(|initial_tag| {
        if *tag_filter.peek() != initial_tag {
            tag_filter.set(initial_tag.clone());
            page.set(1);
        }
    }));

    // Keep the category filter in sync with the `?category=` route query for
    // the same reason: navigating from one category card to another re-renders
    // this component with a new `initial_category` prop without remounting.
    use_effect(use_reactive!(|initial_category| {
        if *category_filter.peek() != initial_category {
            category_filter.set(initial_category.clone());
            page.set(1);
        }
    }));

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

    // Read every reactive input (search, category, tag, page) INSIDE the
    // resource closure so a change to any of them re-runs the fetch. Dioxus
    // `use_resource` only subscribes to signals read within the closure;
    // values captured by value never subscribe (mirrors the contacts list,
    // MAPPS-148). When a tag filter is active the server cannot narrow on
    // tags, so we pull a broad page and filter client-side below.
    let articles_resource = use_resource(move || {
        let q = search.read().trim().to_string();
        let category_id = category_filter.read().clone();
        let tag = tag_filter.read().trim().to_string();
        let current_page = (*page.read()).max(1);
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let (page_param, per_page) = if tag.is_empty() {
                (current_page, PER_PAGE)
            } else {
                (1, TAG_FETCH_LIMIT)
            };
            let mut path = format!("/kb/articles?page={page_param}&per_page={per_page}");
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

    let search_text = search.read().trim().to_string();
    let category_text = category_filter.read().clone();
    let tag_text = tag_filter.read().trim().to_string();
    let tag_active = !tag_text.is_empty();
    let req_page = (*page.read()).max(1);

    let resource_snapshot = articles_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let (fetched_rows, fetched_total): (Vec<KbArticle>, u64) = match &*resource_snapshot {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // With a tag filter active the fetch is broad and unpaginated by the
    // server, so filter by tag and paginate client-side here.
    let (page_rows, total, current_page): (Vec<KbArticle>, u64, usize) = if tag_active {
        let matching: Vec<KbArticle> = fetched_rows
            .into_iter()
            .filter(|a| a.tags.iter().any(|t| t.eq_ignore_ascii_case(&tag_text)))
            .collect();
        let matched = matching.len();
        let page_count = matched.div_ceil(PER_PAGE).max(1);
        let clamped = req_page.min(page_count);
        let start = (clamped - 1) * PER_PAGE;
        let rows: Vec<KbArticle> = matching.into_iter().skip(start).take(PER_PAGE).collect();
        (rows, matched as u64, clamped)
    } else {
        (fetched_rows, fetched_total, req_page)
    };
    let has_filters = !search_text.is_empty() || !category_text.is_empty() || tag_active;

    // Heading reflects the active category filter: show the category name when
    // one is selected, otherwise the generic "All Articles".
    let heading = if category_text.is_empty() {
        "All Articles".to_string()
    } else {
        categories
            .iter()
            .find(|c| c.id.to_string() == category_text)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "All Articles".to_string())
    };

    rsx! {
        AppLayout { title: "Articles",
            PageHeader {
                title: "{heading}",
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
                                    let value = e.value();
                                    category_filter.set(value.clone());
                                    page.set(1);
                                    // Reflect the chosen category in the URL so the filtered
                                    // view is shareable and survives the back button. Clearing
                                    // to "All Categories" pushes an empty `?category=`.
                                    navigator.push(Route::KBArticleList {
                                        q: search.read().trim().to_string(),
                                        tag: tag_filter.read().trim().to_string(),
                                        category: value,
                                    });
                                },
                            }
                        }
                    }

                    if tag_active {
                        div { class: "mb-4 flex items-center flex-wrap gap-2 text-sm",
                            span { class: "text-muted", "Filtered by tag:" }
                            Badge { variant: BadgeVariant::Blue, "#{tag_text}" }
                            Link {
                                to: Route::KBArticleList { q: search_text.clone(), tag: String::new(), category: category_text.clone() },
                                class: "text-accent hover:opacity-90",
                                "Clear filter"
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
                            striped: true,
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
                                if has_filters {
                                    // Filtered to nothing: no CTA (creating won't help);
                                    // guide the user back to the filters.
                                    TableEmpty {
                                        columns: 4,
                                        title: "No articles match your filters".to_string(),
                                        description: "Try clearing or adjusting the filters above.".to_string(),
                                    }
                                } else {
                                    TableEmpty {
                                        columns: 4,
                                        title: "No articles yet".to_string(),
                                        description: "Write your first knowledge base article.".to_string(),
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
                                            tags: article.tags,
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
    #[props(default)]
    tags: Vec<String>,
}

#[component]
fn ArticleRow(props: ArticleRowProps) -> Element {
    let navigator = use_navigator();
    let id = props.id.clone();
    let (vis_label, vis_variant) = visibility_label(&props.visibility);
    let raw_status = if props.status.is_empty() {
        "draft"
    } else {
        &props.status
    };
    let (status_var, status_label) = kb_article_status_badge(raw_status);
    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::KBArticleDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::KBArticleDetail { id: props.id.clone() },
                    class: "font-medium text-accent hover:opacity-90",
                    "{props.title}"
                }
                if !props.tags.is_empty() {
                    TagChips { tags: props.tags.clone(), class: "mt-1".to_string() }
                }
            }
            TableCell { Badge { variant: status_var, "{status_label}" } }
            TableCell { Badge { variant: vis_variant, "{vis_label}" } }
            TableCell { class: "text-muted", "{props.updated}" }
        }
    }
}

/// Clickable tag chips. Each chip links to the article list filtered to that
/// tag (`?tag=`), so tags are visible and clickable wherever an article is
/// shown (rows and the detail view). The container stops click propagation so
/// a chip click inside a clickable table row filters by the tag rather than
/// opening the row's article.
#[component]
fn TagChips(tags: Vec<String>, #[props(default)] class: String) -> Element {
    rsx! {
        div {
            class: "flex flex-wrap gap-1.5 {class}",
            onclick: move |e| e.stop_propagation(),
            for tag in tags.iter().cloned() {
                Link {
                    key: "{tag}",
                    to: Route::KBArticleList { q: String::new(), tag: tag.clone(), category: String::new() },
                    class: "inline-flex items-center rounded-full bg-accent-50 dark:bg-accent-900/40 px-2 py-0.5 text-xs font-medium text-accent-700 dark:text-accent-300 hover:bg-accent-100 dark:hover:bg-accent-900/70",
                    "#{tag}"
                }
            }
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

    // Bind the route's `id` param into each fetch with `use_reactive!` so an
    // in-app article-to-article navigation (same component, new `id` prop)
    // restarts the resource. A plain captured `String` is not reactive, so
    // without this the detail pane keeps showing the previous article even
    // though the URL changed (MAPPS-155).
    let mut article_resource = use_resource(use_reactive!(|id_for_article| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<KbArticle>(&format!("/kb/articles/{id_for_article}"))
            .await
            .ok()
    }));

    let mut versions_resource = use_resource(use_reactive!(|id_for_versions| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<KbArticleVersion>>(&format!(
            "/kb/articles/{id_for_versions}/versions?page=1&per_page=50"
        ))
        .await
        .ok()
    }));

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

    // MAPPS-309: delete-article state. `confirming_delete` drives the
    // ConfirmDialog; `delete_busy` mirrors the in-flight DELETE so the
    // dialog locks until the request returns. On success we navigate
    // back to the KB landing page (the article id is about to 404).
    let mut confirming_delete = use_signal(|| false);
    let mut delete_busy = use_signal(|| false);
    let mut delete_error = use_signal(String::new);
    let nav_after_delete = use_navigator();

    // Reading-view UI state, persisted per user via the prefs store.
    let left_collapsed = use_signal(|| crate::utils::prefs::get_bool("kb_left_rail", false));
    let right_collapsed = use_signal(|| crate::utils::prefs::get_bool("kb_right_rail", false));
    let comfortable = use_signal(|| crate::utils::prefs::get_bool("kb_density", true));
    let open_overlay = use_signal(|| None::<crate::components::RailSide>);
    use_effect(move || crate::utils::prefs::set_bool("kb_left_rail", left_collapsed()));
    use_effect(move || crate::utils::prefs::set_bool("kb_right_rail", right_collapsed()));

    // Shared rating counts + my_vote: declared unconditionally (rules-of-hooks),
    // seeded from the article payload first, then refined by GET /vote which
    // also returns the caller's current vote. Both OverflowActions copies of
    // RatingBar share the same two signals.
    let mut rating = use_signal(|| (0i32, 0i32));
    let mut my_vote = use_signal(|| None::<String>);
    use_effect(move || {
        if let Some(Some(a)) = &*article_resource.read_unchecked() {
            rating.set((a.helpful_count, a.not_helpful_count));
        }
    });

    // Seed my_vote (and refine counts) from GET /kb/articles/{id}/vote once
    // the article id is available. Declared unconditionally for rules-of-hooks.
    let id_for_vote = props.id.clone();
    use_resource(use_reactive!(|id_for_vote| async move {
        #[cfg(feature = "web")]
        {
            let path = format!("/kb/articles/{id_for_vote}/vote");
            if let Ok(fb) = crate::hooks::fetch::api::get_authed::<KbArticleFeedback>(&path).await {
                rating.set((fb.helpful_count, fb.not_helpful_count));
                my_vote.set(fb.my_vote);
            }
        }
        #[cfg(not(feature = "web"))]
        let _ = &id_for_vote;
    }));

    let article_snapshot = article_resource.read_unchecked();
    let header_title = match &*article_snapshot {
        Some(Some(a)) => a.title.clone(),
        _ => "Article".to_string(),
    };

    let title_for_dialog = match &*article_snapshot {
        Some(Some(a)) => a.title.clone(),
        _ => String::new(),
    };
    let id_for_delete = props.id.clone();

    rsx! {
        AppLayout { title: "{header_title}",
            // MAPPS-309: confirm-before-delete dialog for the article.
            // Rendered at the AppLayout root so it overlays every
            // match arm of the article snapshot.
            crate::components::ConfirmDialog {
                open: confirming_delete(),
                title: "Delete article".to_string(),
                message: {
                    let mut msg = if title_for_dialog.is_empty() {
                        "Delete this article? This cannot be undone.".to_string()
                    } else {
                        format!("Delete \"{title_for_dialog}\"? This cannot be undone.")
                    };
                    if !delete_error.read().is_empty() {
                        msg.push_str(&format!("\n\n{}", delete_error.read()));
                    }
                    msg
                },
                confirm_text: "Delete".to_string(),
                cancel_text: "Cancel".to_string(),
                destructive: true,
                loading: delete_busy(),
                oncancel: move |_| {
                    if !delete_busy() {
                        confirming_delete.set(false);
                        delete_error.set(String::new());
                    }
                },
                onconfirm: move |_| {
                    if delete_busy() { return; }
                    delete_busy.set(true);
                    delete_error.set(String::new());
                    let id = id_for_delete.clone();
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/kb/articles/{id}");
                            match crate::hooks::fetch::api::delete_authed(&path).await {
                                Ok(()) => {
                                    crate::hooks::toast::push_toast(
                                        crate::components::AlertType::Success,
                                        "Article deleted.",
                                    );
                                    confirming_delete.set(false);
                                    nav_after_delete.push(Route::KBHome {});
                                }
                                Err(err) => {
                                    delete_error.set(format!("Could not delete article: {err}"));
                                }
                            }
                        }
                        #[cfg(not(feature = "web"))]
                        let _ = &id;
                        delete_busy.set(false);
                    });
                },
            }
            match &*article_snapshot {
                None => rsx! {
                    // PMS-353
                    crate::components::DetailSkeleton {}
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load article." }
                            Link {
                                to: Route::KBHome {},
                                class: "text-sm text-accent hover:opacity-90",
                                "Back to knowledge base"
                            }
                        }
                    }
                },
                Some(Some(article)) => {
                    let (vis_label, vis_variant) = visibility_label(&article.visibility);
                    let status_raw = if article.status.is_empty() {
                        "draft"
                    } else {
                        &article.status
                    };
                    let (status_variant, status_label) = kb_article_status_badge(status_raw);
                    let updated = date_only(&article.updated_at);
                    let content = article.content.clone();
                    let path = resolve_category_path(article.category_id, &categories);
                    // Density drives BOTH the line spacing (leading-*) and the
                    // gap between paragraphs ([&_p]:my-*): comfortable reads
                    // airy, compact reads dense. The arbitrary `[&_p]:` variant
                    // overrides the prose plugin's default paragraph margins.
                    let prose_density = if comfortable() {
                        "prose-base leading-relaxed [&_p]:my-5"
                    } else {
                        "prose-sm leading-snug [&_p]:my-2"
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
                                    ReadModeButton { left_collapsed, right_collapsed }
                                }
                                div { class: "mt-2 flex items-center justify-between gap-3 flex-wrap",
                                    h1 { class: "text-2xl font-semibold text-content truncate", "{article.title}" }
                                    OverflowActions {
                                        RatingBar {
                                            article_id: props.id.clone(),
                                            counts: rating,
                                            my_vote,
                                        }
                                        Badge { variant: status_variant, "{status_label}" }
                                        Badge { variant: vis_variant, "{vis_label}" }
                                        // PMS-482: "Open ticket about this
                                        // article". Navigates to the ticket-
                                        // new form with the article id +
                                        // title + URL on the query string;
                                        // the form pre-fills its title /
                                        // description and stamps
                                        // `source_kb_article_id` on the
                                        // create body. Plain `<a>` so the
                                        // query string survives intact and
                                        // the user can right-click to open
                                        // in a new tab.
                                        {
                                            let qs = format!(
                                                "from_kb_article={}&from_kb_title={}&from_kb_url={}",
                                                props.id,
                                                crate::utils::url::urlencoding_minimal(&article.title),
                                                crate::utils::url::urlencoding_minimal(&format!("/kb/articles/{}", props.id)),
                                            );
                                            rsx! {
                                                a {
                                                    href: "/tickets/new?{qs}",
                                                    class: "inline-flex items-center px-3 py-2 text-sm font-medium rounded-md border border-line hover:bg-surface-2 text-content",
                                                    "Open ticket about this article"
                                                }
                                            }
                                        }
                                        Link { to: Route::KBArticleEdit { id: props.id.clone() },
                                            Button { variant: ButtonVariant::Secondary, "Edit" }
                                        }
                                        // MAPPS-309: delete affordance. Gated by
                                        // `ConfirmDialog` rendered at the page
                                        // root; success navigates back to the KB
                                        // landing.
                                        Button {
                                            variant: ButtonVariant::Danger,
                                            disabled: delete_busy(),
                                            onclick: move |_| {
                                                delete_error.set(String::new());
                                                confirming_delete.set(true);
                                            },
                                            "Delete"
                                        }
                                        DensityToggle { comfortable }
                                    }
                                }
                                p { class: "mt-1 text-xs text-subtle", "Updated {updated}" }
                                if !article.tags.is_empty() {
                                    TagChips { tags: article.tags.clone(), class: "mt-3".to_string() }
                                }
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
        div { class: "rounded border border-line",
            p { class: "px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted border-b border-line",
                "Versions"
            }
            match &*snap {
                None => rsx! {
                    p { class: "px-3 py-3 text-xs text-subtle", "Loading..." }
                },
                Some(None) => rsx! {
                    p { class: "px-3 py-3 text-xs text-red-600 dark:text-red-300", "Could not load version history." }
                },
                Some(Some(page)) if page.data.is_empty() => rsx! {
                    p { class: "px-3 py-3 text-xs text-subtle", "No prior versions." }
                },
                Some(Some(page)) => {
                    let rows = page.data.clone();
                    let article_id = article_id.clone();
                    rsx! {
                        ul { class: "divide-y divide-line",
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
                                                span { class: "font-medium text-content", "v{n}" }
                                                button {
                                                    class: "text-accent hover:opacity-90 disabled:opacity-50",
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
                                            p { class: "mt-0.5 text-muted truncate", "{title}" }
                                            p { class: "text-subtle", "{saved}" }
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
                    // PMS-353
                    crate::components::DetailSkeleton {}
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load article." }
                            Link {
                                to: Route::KBHome {},
                                class: "text-sm text-accent hover:opacity-90",
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
    // PMS-518: per-field inline error slots, fed by the FormGuard on submit.
    let mut title_error = use_signal(String::new);
    let mut content_error = use_signal(String::new);

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

    let mut tab = use_signal(|| BodyTab::Write);

    let navigator = use_navigator();
    let is_edit = matches!(mode, ArticleFormMode::Edit { .. });
    // In edit mode the slug is already published, so start it touched so
    // typing a new title never clobbers an existing URL slug.
    let mut slug_touched = use_signal(|| is_edit);
    let submit_label = if is_edit { "Save Changes" } else { "Publish" };

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        error.set(String::new());

        // PMS-518: validate both required fields through the shared FormGuard so
        // a missing Title no longer masks a missing Body; each surfaces in its own
        // inline slot and the first invalid field is focused.
        let mut guard = FormGuard::new();
        let title_val = title.read().trim().to_string();
        title_error.set(guard.field("title", &title_val, "Title", &[Rule::Required]));
        let content_val = content.read().to_string();
        content_error.set(guard.field("content", content_val.trim(), "Body", &[Rule::Required]));
        if guard.blocked() {
            return;
        }

        is_submitting.set(true);

        // Slug: normalize through `slugify` so the stored value is always
        // URL-safe (lowercase, hyphen-separated, no spaces/`<>`/punctuation),
        // whether it was author-supplied or derived from the title when blank.
        // `slugify` is idempotent on an already-valid slug (MAPPS-218).
        let slug_raw = slug.read().trim().to_string();
        let slug_source = if slug_raw.is_empty() {
            &title_val
        } else {
            &slug_raw
        };
        let slug_val = slugify(slug_source);
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
        let tags_vec: Vec<String> = sanitize_tags(&tags.read());

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
                    maxlength: TITLE_MAX as i64,
                    rules: vec![Rule::Required],
                    error: title_error.read().clone(),
                    value: title.read().clone(),
                    oninput: move |e: FormEvent| {
                        title_error.set(String::new());
                        title.set(e.value());
                        if let Some(s) = next_slug(&e.value(), slug_touched()) {
                            slug.set(s);
                        }
                    },
                }

                crate::components::Input {
                    name: "slug",
                    label: "Slug",
                    placeholder: "Leave blank to derive from the title",
                    help: "URL-safe identifier; normalized to lowercase hyphenated form on save, and auto-generated from the title when blank.",
                    maxlength: SLUG_MAX as i64,
                    value: slug.read().clone(),
                    oninput: move |e: FormEvent| {
                        slug_touched.set(true);
                        slug.set(e.value());
                    },
                }

                crate::components::Input {
                    name: "summary",
                    label: "Summary",
                    placeholder: "Short one-line description (optional)",
                    maxlength: SUMMARY_MAX as i64,
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

                div {
                    div { class: "flex gap-2 border-b border-line mb-2",
                        button { r#type: "button", class: body_tab_class(tab() == BodyTab::Write), onclick: move |_| tab.set(BodyTab::Write), "Write" }
                        button { r#type: "button", class: body_tab_class(tab() == BodyTab::Preview), onclick: move |_| tab.set(BodyTab::Preview), "Preview" }
                    }
                    match tab() {
                        BodyTab::Write => rsx! {
                            crate::components::Textarea {
                                name: "content",
                                label: "Body (Markdown)",
                                placeholder: "## Overview\n\nWrite the article in Markdown...",
                                rows: 16,
                                required: true,
                                maxlength: BODY_MAX as i64,
                                rules: vec![Rule::Required],
                                error: content_error.read().clone(),
                                value: content.read().clone(),
                                oninput: move |e: FormEvent| {
                                    content_error.set(String::new());
                                    content.set(e.value());
                                },
                            }
                        },
                        BodyTab::Preview => rsx! {
                            article {
                                class: "prose dark:prose-invert max-w-none p-2 min-h-40 border border-line rounded",
                                dangerous_inner_html: crate::utils::markdown::render_markdown(&content.read()),
                            }
                        },
                    }
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
// Category create / edit modal
// ============================================================================

/// Create or edit a KB category (MAPPS-230). Posts to `/kb/categories` (create)
/// or puts to `/kb/categories/{id}` (edit), then calls `on_saved` so the home
/// grid refetches. Mirrors the article form's slug handling: the slug follows
/// the name until the author edits it, and is normalized through [`slugify`]
/// on save.
#[derive(Props, Clone, PartialEq)]
struct CategoryFormModalProps {
    edit: CategoryEdit,
    categories: Vec<KbCategory>,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
}

#[component]
fn CategoryFormModal(props: CategoryFormModalProps) -> Element {
    let existing = match &props.edit {
        CategoryEdit::New => None,
        CategoryEdit::Existing(c) => Some(c.clone()),
    };
    let is_edit = existing.is_some();
    let edit_id = existing.as_ref().map(|c| c.id);

    let mut name = use_signal(|| {
        existing
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default()
    });
    let mut slug = use_signal(|| {
        existing
            .as_ref()
            .map(|c| c.slug.clone())
            .unwrap_or_default()
    });
    let mut description = use_signal(|| {
        existing
            .as_ref()
            .and_then(|c| c.description.clone())
            .unwrap_or_default()
    });
    let mut parent_id = use_signal(|| {
        existing
            .as_ref()
            .and_then(|c| c.parent_id)
            .map(|id| id.to_string())
            .unwrap_or_default()
    });
    let mut visibility = use_signal(|| {
        existing
            .as_ref()
            .map(|c| {
                if c.visibility.is_empty() {
                    "internal".to_string()
                } else {
                    c.visibility.clone()
                }
            })
            .unwrap_or_else(|| "internal".to_string())
    });
    let mut sort_order = use_signal(|| {
        existing
            .as_ref()
            .map(|c| c.sort_order.to_string())
            .unwrap_or_else(|| "0".to_string())
    });
    // In edit mode the slug is already published, so start it touched so
    // editing the name never clobbers an existing URL slug.
    let mut slug_touched = use_signal(|| is_edit);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: inline error slot for the required Name, fed by the FormGuard.
    let mut name_error = use_signal(String::new);

    // Parent dropdown: "None" plus every other category. The category being
    // edited is excluded so it cannot be made its own parent.
    let mut parent_options = vec![SelectOption::new("", "None (top-level)")];
    for c in props.categories.iter() {
        if Some(c.id) == edit_id {
            continue;
        }
        parent_options.push(SelectOption::new(c.id.to_string(), c.name.clone()));
    }

    let visibility_options = vec![
        SelectOption::new("internal", "Internal"),
        SelectOption::new("public", "Public"),
        SelectOption::new("client_specific", "Client-specific"),
    ];

    let title = if is_edit {
        "Edit Category"
    } else {
        "New Category"
    };
    let submit_label = if is_edit { "Save Changes" } else { "Create" };

    let on_saved = props.on_saved;
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        error.set(String::new());

        // PMS-518: required Name validated through the shared FormGuard (inline
        // slot + focus), for consistency with the other migrated forms.
        let mut guard = FormGuard::new();
        let name_val = name.read().trim().to_string();
        name_error.set(guard.field("name", &name_val, "Name", &[Rule::Required]));
        if guard.blocked() {
            return;
        }

        is_submitting.set(true);

        // Slug normalized through `slugify`, derived from the name when blank.
        let slug_raw = slug.read().trim().to_string();
        let slug_source = if slug_raw.is_empty() {
            &name_val
        } else {
            &slug_raw
        };
        let slug_val = slugify(slug_source);
        let description_val = description.read().trim().to_string();
        let description_opt = if description_val.is_empty() {
            None
        } else {
            Some(description_val)
        };
        let parent_opt = {
            let raw = parent_id.read().clone();
            if raw.is_empty() {
                None
            } else {
                raw.parse::<uuid::Uuid>().ok()
            }
        };
        let visibility_val = visibility.read().clone();
        let sort_val = sort_order.read().trim().parse::<i32>().unwrap_or(0);

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match edit_id {
                    None => {
                        let body = CreateKbCategoryRequest {
                            name: name_val.clone(),
                            slug: slug_val.clone(),
                            description: description_opt.clone(),
                            parent_id: parent_opt,
                            visibility: visibility_val.clone(),
                            sort_order: sort_val,
                        };
                        crate::hooks::fetch::api::post_authed::<KbCategory, _>(
                            "/kb/categories",
                            &body,
                        )
                        .await
                        .map(|_| ())
                    }
                    Some(id) => {
                        let body = UpdateKbCategoryRequest {
                            name: Some(name_val.clone()),
                            slug: Some(slug_val.clone()),
                            description: description_opt.clone(),
                            parent_id: parent_opt,
                            visibility: Some(visibility_val.clone()),
                            sort_order: Some(sort_val),
                        };
                        let path = format!("/kb/categories/{id}");
                        crate::hooks::fetch::api::put_authed::<KbCategory, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => {
                        on_saved.call(());
                    }
                    Err(err) => {
                        error.set(format!("Could not save category: {err}"));
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (
                    &name_val,
                    &slug_val,
                    &description_opt,
                    &parent_opt,
                    &visibility_val,
                    sort_val,
                    edit_id,
                );
            }
            is_submitting.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: title.to_string(),
            size: ModalSize::Large,
            onclose: move |_| props.on_close.call(()),
            form {
                class: "space-y-4",
                onsubmit: handle_submit,

                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }

                crate::components::Input {
                    name: "name",
                    label: "Name",
                    placeholder: "e.g. Networking",
                    required: true,
                    rules: vec![Rule::Required],
                    error: name_error.read().clone(),
                    value: name.read().clone(),
                    oninput: move |e: FormEvent| {
                        name_error.set(String::new());
                        name.set(e.value());
                        if let Some(s) = next_slug(&e.value(), slug_touched()) {
                            slug.set(s);
                        }
                    },
                }

                crate::components::Input {
                    name: "slug",
                    label: "Slug",
                    placeholder: "Leave blank to derive from the name",
                    help: "URL-safe identifier; normalized to lowercase hyphenated form on save, and auto-generated from the name when blank.",
                    value: slug.read().clone(),
                    oninput: move |e: FormEvent| {
                        slug_touched.set(true);
                        slug.set(e.value());
                    },
                }

                crate::components::Input {
                    name: "description",
                    label: "Description",
                    placeholder: "Short one-line description (optional)",
                    value: description.read().clone(),
                    oninput: move |e: FormEvent| description.set(e.value()),
                }

                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-3",
                    Select {
                        name: "parent",
                        label: "Parent category",
                        options: parent_options,
                        value: parent_id.read().clone(),
                        onchange: move |e: FormEvent| parent_id.set(e.value()),
                    }
                    Select {
                        name: "visibility",
                        label: "Visibility",
                        options: visibility_options,
                        value: visibility.read().clone(),
                        onchange: move |e: FormEvent| visibility.set(e.value()),
                    }
                    crate::components::Input {
                        name: "sort_order",
                        label: "Sort order",
                        placeholder: "0",
                        help: "Lower numbers sort first within their level.",
                        value: sort_order.read().clone(),
                        oninput: move |e: FormEvent| sort_order.set(e.value()),
                    }
                }

                div { class: "flex justify-end space-x-3 pt-2",
                    Button {
                        r#type: "button",
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| props.on_close.call(()),
                        "Cancel"
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
        nav { class: "flex items-center flex-wrap gap-1 text-sm text-muted",
            Link { to: Route::KBHome {}, class: "hover:text-content", "KB" }
            for cat in path.iter() {
                ChevronRightIcon { size: IconSize::Small }
                Link {
                    to: Route::KBArticleList { q: String::new(), tag: String::new(), category: cat.id.to_string() },
                    class: "hover:text-content",
                    "{cat.name}"
                }
            }
            ChevronRightIcon { size: IconSize::Small }
            span { class: "text-content font-medium", "{title}" }
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
                div { class: "pt-2 mt-2 border-t border-line",
                    p { class: "px-2 py-1 text-xs uppercase tracking-wide text-subtle", "Uncategorized" }
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
                class: "w-full flex items-center gap-1 px-2 py-1 rounded text-left text-content hover:bg-surface-2",
                onclick: move |_| open.toggle(),
                span {
                    class: if open() { "rotate-90 transition-transform" } else { "transition-transform" },
                    ChevronRightIcon { size: IconSize::Small }
                }
                span { class: "font-medium truncate", "{node.category.name}" }
            }
            if open() {
                div { class: "ml-4 border-l border-line pl-2 space-y-1",
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
    let base = "block px-2 py-1 rounded truncate hover:bg-surface-2";
    let cls = if is_current {
        format!(
            "{base} bg-accent-50 dark:bg-accent-900/40 text-accent-700 dark:text-accent-300 font-medium"
        )
    } else {
        format!("{base} text-content")
    };
    rsx! {
        Link { to: Route::KBArticleDetail { id: article.id.to_string() }, class: "{cls}", "{article.title}" }
    }
}

// ============================================================================
// Rating bar, density toggle, read-mode button
// ============================================================================

#[component]
fn RatingBar(
    article_id: String,
    counts: Signal<(i32, i32)>,
    my_vote: Signal<Option<String>>,
) -> Element {
    let mut counts = counts;
    let mut my_vote = my_vote;
    let mut busy = use_signal(|| false);
    let (h, n) = counts();
    let active_vote = my_vote();
    let id_h = article_id.clone();
    let id_n = article_id.clone();

    // Active state classes: green for helpful, red for not_helpful, neutral otherwise.
    let helpful_class = if active_vote.as_deref() == Some("helpful") {
        "flex items-center gap-1 text-green-600 font-semibold disabled:opacity-50"
    } else {
        "flex items-center gap-1 text-content hover:text-green-600 \
         disabled:opacity-50"
    };
    let not_helpful_class = if active_vote.as_deref() == Some("not_helpful") {
        "flex items-center gap-1 text-red-600 font-semibold disabled:opacity-50"
    } else {
        "flex items-center gap-1 text-content hover:text-red-600 \
         disabled:opacity-50"
    };

    rsx! {
        div { class: "flex items-center gap-3 text-sm",
            button {
                class: "{helpful_class}",
                disabled: busy(),
                onclick: move |_| {
                    if busy() { return; }
                    busy.set(true);
                    let id = id_h.clone();
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/kb/articles/{id}/helpful");
                            if let Ok(fb) =
                                crate::hooks::fetch::api::post_authed::<KbArticleFeedback, _>(
                                    &path,
                                    &serde_json::json!({}),
                                )
                                .await
                            {
                                counts.set((fb.helpful_count, fb.not_helpful_count));
                                my_vote.set(fb.my_vote);
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
                class: "{not_helpful_class}",
                disabled: busy(),
                onclick: move |_| {
                    if busy() { return; }
                    busy.set(true);
                    let id = id_n.clone();
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/kb/articles/{id}/not_helpful");
                            if let Ok(fb) =
                                crate::hooks::fetch::api::post_authed::<KbArticleFeedback, _>(
                                    &path,
                                    &serde_json::json!({}),
                                )
                                .await
                            {
                                counts.set((fb.helpful_count, fb.not_helpful_count));
                                my_vote.set(fb.my_vote);
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
            class: "text-xs px-2 py-1 rounded border border-line text-content",
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
fn ReadModeButton(left_collapsed: Signal<bool>, right_collapsed: Signal<bool>) -> Element {
    let mut left_collapsed = left_collapsed;
    let mut right_collapsed = right_collapsed;
    let both = left_collapsed() && right_collapsed();
    rsx! {
        button {
            class: "p-1 text-muted hover:text-content",
            title: if both { "Exit read mode (show panels)" } else { "Read mode (hide panels)" },
            onclick: move |_| {
                let collapse = !(left_collapsed() && right_collapsed());
                left_collapsed.set(collapse);
                right_collapsed.set(collapse);
            },
            if both {
                // arrows pointing inward (minimize / compress)
                svg { class: "h-5 w-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M9 9L4 4M9 9V5M9 9H5" }
                    path { d: "M15 9l5-5M15 9V5M15 9h4" }
                    path { d: "M9 15l-5 5M9 15v4M9 15H5" }
                    path { d: "M15 15l5 5M15 15v4M15 15h4" }
                }
            } else {
                // arrows pointing outward (expand / fullscreen)
                svg { class: "h-5 w-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M4 9V4h5" }
                    path { d: "M20 9V4h-5" }
                    path { d: "M4 15v5h5" }
                    path { d: "M20 15v5h-5" }
                }
            }
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

    #[test]
    fn slug_follows_title_until_touched() {
        assert_eq!(next_slug("My Article", false), Some(slugify("My Article")));
        assert_eq!(next_slug("My Article", true), None);
    }

    #[test]
    fn slugify_normalizes_unsafe_author_input() {
        // The repro value from MAPPS-218 becomes URL-safe.
        assert_eq!(
            slugify("Bad Slug With Spaces!!! <>"),
            "bad-slug-with-spaces"
        );
        // Already-valid slugs survive unchanged (idempotent).
        assert_eq!(slugify("my-valid-slug"), "my-valid-slug");
    }

    #[test]
    fn sanitize_tags_strips_markup_and_caps_length() {
        let tags = sanitize_tags(&format!("tag1, <script>, {}", "x".repeat(100)));
        // Markup chars are stripped, the over-long tag is capped to TAG_MAX.
        assert_eq!(
            tags,
            vec![
                "tag1".to_string(),
                "script".to_string(),
                "x".repeat(TAG_MAX)
            ]
        );
    }

    #[test]
    fn sanitize_tags_drops_empty_and_control_only() {
        // Whitespace-only, comma-only, and control-character-only entries drop out.
        assert_eq!(sanitize_tags("  , \t , a , "), vec!["a".to_string()]);
        assert!(sanitize_tags("").is_empty());
    }
}
