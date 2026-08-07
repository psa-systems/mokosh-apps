# KB UI/UX Overhaul Implementation Plan

> **Historical record.** The article-header layout this plan builds (rating, badges
> and Edit inline on the title row behind `OverflowActions`) was superseded by
> #MAPPS-423; see the note at the top of `kb-ui-overhaul-design.md` for the current
> arrangement of the header, the right rail, the rating and the nav tree.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the KB reading and authoring experience: rendered (sanitized) markdown directly on the page, a three-pane reading view with a collapsible category/article tree, a consolidated article header (breadcrumb, title, inline rating, badges, Edit, overflow dropdown), per-user density + read-mode + rail-collapse preferences, and an authoring form with live slug generation and Write/Preview tabs.

**Architecture:** Pure logic (markdown render+sanitize, breadcrumb path resolution, category/article tree building, slug-touched behavior) lands in unit-tested helpers under `src/utils/` and `src/pages/knowledge_base.rs` test modules. Reusable UI primitives (`CollapsibleRail`, `OverflowActions`) go in `src/components/`. KB-specific components (`KbTreeNav`, `KbBreadcrumb`, `RatingBar`, `DensityToggle`, `ReadModeButton`) live in `knowledge_base.rs`. Per-user preferences persist in `localStorage` via a small typed helper.

**Tech Stack:** Rust, Dioxus 0.7 (web), `pulldown-cmark` (already a dep), `ammonia` (new), Tailwind classes, `web-sys` localStorage.

**Conventions / environment:**
- All `cargo`/`just` commands run **inside the dev container** (no host toolchain). Use `just check` (= `check-web` + `check-clippy` + `check-fmt`), `just test`, `just fmt`.
- Unit tests follow the existing `#[cfg(test)] mod tests { ... }` pattern (see `src/utils/pagination.rs`).
- Commit after each task. No YouTrack reference in commits (matches this branch's earlier commits and the calendar follow-up fixes).
- Branch: `feat/kb-ui-overhaul` (already created; the spec lives at `dev-docs/kb-ui-overhaul-design.md`).

**Spec:** `dev-docs/kb-ui-overhaul-design.md`

---

## File structure

- `Cargo.toml` - add `ammonia`.
- `src/utils/markdown.rs` (new) - `render_markdown(src) -> String` (pulldown-cmark -> ammonia-sanitized HTML). Unit-tested.
- `src/utils/prefs.rs` (new) - typed localStorage get/set for the per-user UI prefs (`kb_density`, `kb_left_rail`, `kb_right_rail`, `kb_read_mode`).
- `src/utils/mod.rs` - export the two new modules.
- `src/components/collapsible_rail.rs` (new) - `CollapsibleRail` (side, persisted key, overlay-on-narrow). Generic.
- `src/components/overflow_actions.rs` (new) - `OverflowActions` (inline children that collapse into a `⋯` dropdown when too narrow).
- `src/components/mod.rs` - declare + re-export the two new components.
- `src/pages/knowledge_base.rs` - the bulk: `render_markdown` usage, `KbBreadcrumb`, `KbTreeNav`, tree builder + breadcrumb resolver (logic + tests), `RatingBar`, `DensityToggle`, `ReadModeButton`, rewritten `KBArticleDetailPage`, `KBArticleListPage` (tree rail), `ArticleForm` (live slug + Write/Preview tabs), slimmed `VersionHistoryCard`.

---

## Task 1: Markdown render + sanitize helper

**Files:**
- Modify: `Cargo.toml`
- Create: `src/utils/markdown.rs`
- Modify: `src/utils/mod.rs`

- [ ] **Step 1: Add the ammonia dependency**

In `Cargo.toml`, under `[dependencies]` near `pulldown-cmark = "0.12"`, add:

```toml
ammonia = "4"
```

- [ ] **Step 2: Write the failing tests**

Create `src/utils/markdown.rs`:

```rust
//! Render KB article Markdown to sanitized HTML for direct injection
//! via Dioxus `dangerous_inner_html`. Authors are internal staff, but
//! the same content feeds the public portal feed, so the HTML is always
//! scrubbed with ammonia before it reaches a browser.

use pulldown_cmark::{html, Options, Parser};

/// Render Markdown source to sanitized HTML.
pub fn render_markdown(src: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(src, options);
    let mut unsafe_html = String::new();
    html::push_html(&mut unsafe_html, parser);
    ammonia::clean(&unsafe_html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_headings_and_lists() {
        let out = render_markdown("# Title\n\n- a\n- b");
        assert!(out.contains("<h1>Title</h1>"));
        assert!(out.contains("<li>a</li>"));
    }

    #[test]
    fn renders_code_block() {
        let out = render_markdown("```\nlet x = 1;\n```");
        assert!(out.contains("<pre><code"));
        assert!(out.contains("let x = 1;"));
    }

    #[test]
    fn strips_script_tags() {
        let out = render_markdown("hello <script>alert(1)</script> world");
        assert!(!out.contains("<script"));
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
    }

    #[test]
    fn strips_event_handler_attributes() {
        let out = render_markdown("<img src=x onerror=\"alert(1)\">");
        assert!(!out.contains("onerror"));
    }

    #[test]
    fn keeps_links_but_drops_javascript_scheme() {
        let out = render_markdown("[x](javascript:alert(1))");
        assert!(!out.contains("javascript:"));
    }
}
```

- [ ] **Step 3: Export the module**

In `src/utils/mod.rs`, add alongside the other `mod`/`pub use` lines:

```rust
pub mod markdown;
```

- [ ] **Step 4: Run tests, expect pass**

Run (in dev container): `just test` (or `cargo test --lib utils::markdown`).
Expected: the 5 `utils::markdown::tests::*` pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/utils/markdown.rs src/utils/mod.rs
git commit -m "feat(kb): add sanitized markdown render helper"
```

---

## Task 2: Per-user preference helper (localStorage)

**Files:**
- Create: `src/utils/prefs.rs`
- Modify: `src/utils/mod.rs`

- [ ] **Step 1: Implement the helper**

Create `src/utils/prefs.rs`:

```rust
//! Tiny typed wrapper over `localStorage` for per-user KB UI prefs.
//! Non-web builds and storage errors degrade to the provided default
//! so callers never have to special-case the environment.

/// Read a boolean pref, falling back to `default` when unset/unavailable.
pub fn get_bool(key: &str, default: bool) -> bool {
    #[cfg(feature = "web")]
    {
        if let Some(Ok(Some(v))) = web_sys::window()
            .map(|w| w.local_storage())
            .and_then(|s| s.ok())
            .map(|s| s.map(|s| s.get_item(key)))
            .map(|r| r.transpose().ok().flatten())
            .map(Ok)
        {
            return v == "1";
        }
        default
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = key;
        default
    }
}

/// Persist a boolean pref. No-op off web or on storage error.
pub fn set_bool(key: &str, value: bool) {
    #[cfg(feature = "web")]
    {
        if let Some(Ok(Some(s))) = web_sys::window().map(|w| w.local_storage()).map(|r| r.map(Some))
        {
            let _ = s.set_item(key, if value { "1" } else { "0" });
        }
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = (key, value);
    }
}
```

> Note: if the nested `Option`/`Result` juggling trips clippy, simplify to a straightforward `match web_sys::window()` block - the contract (return `default` unless a stored `"1"`/`"0"` is found) is what matters. Verify shape with `just check-clippy`.

- [ ] **Step 2: Export the module**

In `src/utils/mod.rs` add:

```rust
pub mod prefs;
```

- [ ] **Step 3: Verify it compiles**

Run: `just check-web` then `just check-clippy`.
Expected: clean (no warnings; clippy is `-D warnings`).

- [ ] **Step 4: Commit**

```bash
git add src/utils/prefs.rs src/utils/mod.rs
git commit -m "feat(kb): add localStorage pref helper for KB UI state"
```

---

## Task 3: Breadcrumb path resolution (logic + tests)

**Files:**
- Modify: `src/pages/knowledge_base.rs` (add `resolve_category_path` + tests)

- [ ] **Step 1: Write the failing test**

Add near the top of `knowledge_base.rs` (after imports) and a test module at the bottom:

```rust
/// Resolve the chain of category names from root down to the article's
/// own category, by walking `parent_id`. Returns `[]` when the article
/// has no category or the id is dangling. Guards against cycles with a
/// visited set.
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
```

Test module (extend or create `#[cfg(test)] mod tests` at end of file):

```rust
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
}
```

> If `KbCategory` has fields beyond those shown, copy the real struct shape from `src/modules/kb/models.rs` into the `cat()` helper. Confirm before running.

- [ ] **Step 2: Run tests, expect pass**

Run: `just test` (or `cargo test --lib pages::knowledge_base::tests`).
Expected: the three `resolve_*` tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/pages/knowledge_base.rs
git commit -m "feat(kb): add category breadcrumb path resolution"
```

---

## Task 4: Category/article tree builder (logic + tests)

**Files:**
- Modify: `src/pages/knowledge_base.rs` (add `TreeNode` + `build_kb_tree` + tests)

- [ ] **Step 1: Write the failing test**

Add to `knowledge_base.rs`:

```rust
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
```

Add tests to the `tests` module:

```rust
    fn art(id: Uuid, cat: Option<Uuid>, title: &str) -> KbArticle {
        KbArticle {
            id,
            title: title.to_string(),
            slug: title.to_lowercase(),
            content: String::new(),
            summary: None,
            category_id: cat,
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
```

> Match the real `KbArticle` field list from `src/modules/kb/models.rs` in `art()`.

- [ ] **Step 2: Run tests, expect pass**

Run: `just test`.
Expected: tree tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/pages/knowledge_base.rs
git commit -m "feat(kb): add category/article tree builder"
```

---

## Task 5: `KbBreadcrumb` and `KbTreeNav` components

**Files:**
- Modify: `src/pages/knowledge_base.rs`

- [ ] **Step 1: Add `KbBreadcrumb`**

```rust
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
```

- [ ] **Step 2: Add `KbTreeNav`**

Renders the forest from `build_kb_tree`; current article highlighted; categories expand/collapse with a local `use_signal` set of open ids; each article is a `Link` to `Route::KBArticleDetail { id }`.

```rust
#[component]
fn KbTreeNav(
    categories: Vec<KbCategory>,
    articles: Vec<KbArticle>,
    current_id: String,
) -> Element {
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
                span { class: if open() { "rotate-90 transition-transform" } else { "transition-transform" },
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
```

> If `ChevronRightIcon` is not already imported in this file, add it to the `crate::components` use list (it is exported from `components`). Confirm the icon name with `grep -n "ChevronRightIcon\|ChevronDownIcon" src/components/icons.rs`.

- [ ] **Step 3: Verify compiles**

Run: `just check-web && just check-clippy`. Expected: clean. (Components are not yet mounted; this just confirms they compile.)

- [ ] **Step 4: Commit**

```bash
git add src/pages/knowledge_base.rs
git commit -m "feat(kb): add breadcrumb and category/article tree-nav components"
```

---

## Task 6: `CollapsibleRail` component

**Files:**
- Create: `src/components/collapsible_rail.rs`
- Modify: `src/components/mod.rs`

- [ ] **Step 1: Implement the component**

A rail that: on wide screens shows its children inline with a chevron to collapse (persisted via `prefs`); on narrow screens (`lg:` breakpoint) renders as an edge toggle that opens an overlay. The parent coordinates "only one overlay open" by owning an `open_side: Signal<Option<Side>>` and passing it in.

Create `src/components/collapsible_rail.rs`:

```rust
//! A collapsible side rail used by the KB reading view. On wide screens
//! it shows inline with a chevron toggle (state persisted per user); on
//! narrow screens it becomes an edge handle that opens an overlay. The
//! caller owns `open_overlay` so only one rail overlay is open at once.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum RailSide {
    Left,
    Right,
}

#[component]
pub fn CollapsibleRail(
    side: RailSide,
    /// Wide-screen collapsed state (persisted by the parent).
    collapsed: Signal<bool>,
    /// Which rail's overlay is open on narrow screens (shared across rails).
    open_overlay: Signal<Option<RailSide>>,
    children: Element,
) -> Element {
    let is_overlay_open = open_overlay() == Some(side);
    let chevron_collapse = match side {
        RailSide::Left => "‹",
        RailSide::Right => "›",
    };
    let chevron_expand = match side {
        RailSide::Left => "›",
        RailSide::Right => "‹",
    };
    rsx! {
        // Wide screens: inline rail, collapses to a thin handle.
        div { class: "hidden lg:block relative",
            if collapsed() {
                button {
                    class: "h-full px-1 text-gray-400 hover:text-gray-600",
                    title: "Expand",
                    onclick: move |_| collapsed.set(false),
                    "{chevron_expand}"
                }
            } else {
                div { class: "w-64 shrink-0",
                    div { class: "flex justify-end",
                        button {
                            class: "px-1 text-gray-400 hover:text-gray-600",
                            title: "Collapse",
                            onclick: move |_| collapsed.set(true),
                            "{chevron_collapse}"
                        }
                    }
                    {children.clone()}
                }
            }
        }
        // Narrow screens: edge handle + overlay.
        div { class: "lg:hidden",
            button {
                class: "px-2 py-1 text-gray-500",
                onclick: move |_| {
                    if is_overlay_open { open_overlay.set(None); } else { open_overlay.set(Some(side)); }
                },
                "{chevron_expand}"
            }
            if is_overlay_open {
                // Backdrop: click-out closes.
                div {
                    class: "fixed inset-0 z-40 bg-black/30",
                    onclick: move |_| open_overlay.set(None),
                }
                div { class: "fixed z-50 top-0 bottom-0 w-72 bg-white dark:bg-gray-900 shadow-xl p-4 overflow-y-auto",
                    class: if side == RailSide::Left { "left-0" } else { "right-0" },
                    div { class: "flex justify-end",
                        button { class: "text-gray-400", onclick: move |_| open_overlay.set(None), "✕" }
                    }
                    {children}
                }
            }
        }
    }
}
```

> Dioxus 0.7: passing `children: Element` and rendering `{children}` is valid; clone it if used in two branches (as above). Verify with `just check-web`.

- [ ] **Step 2: Declare + export**

In `src/components/mod.rs` add `mod collapsible_rail;` and `pub use collapsible_rail::*;` in the existing alphabetical spots.

- [ ] **Step 3: Verify compiles**

Run: `just check-web && just check-clippy`. Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/collapsible_rail.rs src/components/mod.rs
git commit -m "feat(components): add CollapsibleRail with persist + overlay"
```

---

## Task 7: `OverflowActions` component

**Files:**
- Create: `src/components/overflow_actions.rs`
- Modify: `src/components/mod.rs`

The header's secondary items must collapse into a `⋯` dropdown when too narrow. A pixel-perfect measure-and-reflow is overkill; use a CSS-driven approach: show the inline row at `sm:` and up; below `sm:` show a `⋯` button that toggles a dropdown containing the same items. The parent passes the items twice (inline + menu) via two render closures, or—simpler—passes one `children` shown inline on wide and inside the dropdown on narrow.

- [ ] **Step 1: Implement**

Create `src/components/overflow_actions.rs`:

```rust
//! Header action cluster that shows its children inline on wider rows
//! and collapses them into a `⋯` dropdown when the row is too narrow to
//! fit them, so they never overflow the container.

use dioxus::prelude::*;

#[component]
pub fn OverflowActions(children: Element) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        // Inline on >= sm.
        div { class: "hidden sm:flex items-center gap-2", {children.clone()} }
        // Collapsed menu on < sm.
        div { class: "sm:hidden relative",
            button {
                class: "px-2 py-1 text-gray-500 hover:text-gray-700",
                onclick: move |_| open.toggle(),
                "⋯"
            }
            if open() {
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |_| open.set(false),
                }
                div { class: "absolute right-0 z-50 mt-1 w-48 rounded-md bg-white dark:bg-gray-800 shadow-lg ring-1 ring-black/5 p-2 flex flex-col gap-2",
                    {children}
                }
            }
        }
    }
}
```

> This uses the `sm` breakpoint as the collapse threshold rather than runtime measurement; that satisfies "collapse before it bleeds out" without a JS ResizeObserver. If a different breakpoint reads better during manual testing, change `sm:` to `md:`.

- [ ] **Step 2: Declare + export** in `src/components/mod.rs` (`mod overflow_actions;` / `pub use overflow_actions::*;`).

- [ ] **Step 3: Verify** `just check-web && just check-clippy`. Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/overflow_actions.rs src/components/mod.rs
git commit -m "feat(components): add OverflowActions collapsing header cluster"
```

---

## Task 8: `RatingBar`, `DensityToggle`, `ReadModeButton`

**Files:**
- Modify: `src/pages/knowledge_base.rs`

- [ ] **Step 1: `RatingBar`** - thumbs with counts, reusing the existing `/helpful` + `/not_helpful` POST flow. It owns the `feedback`/`feedback_busy` signals (lifted from the old "Was this helpful?" card).

```rust
#[component]
fn RatingBar(article_id: String, helpful: i32, not_helpful: i32) -> Element {
    let mut counts = use_signal(|| (helpful, not_helpful));
    let mut busy = use_signal(|| false);
    let (h, n) = counts();
    let id_h = article_id.clone();
    let id_n = article_id.clone();
    let vote = move |id: String, endpoint: &'static str, mut counts: Signal<(i32, i32)>, mut busy: Signal<bool>| {
        if busy() { return; }
        busy.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/kb/articles/{id}/{endpoint}");
                if let Ok(fb) = crate::hooks::fetch::api::post_authed::<KbArticleFeedback, _>(&path, &serde_json::json!({})).await {
                    counts.set((fb.helpful_count, fb.not_helpful_count));
                }
            }
            busy.set(false);
        });
    };
    rsx! {
        div { class: "flex items-center gap-3 text-sm",
            button {
                class: "flex items-center gap-1 text-gray-600 dark:text-gray-300 hover:text-green-600 disabled:opacity-50",
                disabled: busy(),
                onclick: move |_| vote(id_h.clone(), "helpful", counts, busy),
                span { "👍" } span { class: "tabular-nums", "{h}" }
            }
            button {
                class: "flex items-center gap-1 text-gray-600 dark:text-gray-300 hover:text-red-600 disabled:opacity-50",
                disabled: busy(),
                onclick: move |_| vote(id_n.clone(), "not_helpful", counts, busy),
                span { "👎" } span { class: "tabular-nums", "{n}" }
            }
        }
    }
}
```

> Confirm `post_authed::<KbArticleFeedback, _>` signature matches the existing call in the current detail page (it does as of this plan). If `vote` as a closure capturing generics is awkward, inline the two `spawn` blocks into each button's `onclick` (matches the current code).

- [ ] **Step 2: `DensityToggle`** - compact/comfortable, persisted under `kb_density`. Exposes the current density to the article container via a returned class. Implement as a component that takes `density: Signal<bool>` (true = comfortable) and renders a small toggle; the parent maps it to prose classes.

```rust
#[component]
fn DensityToggle(comfortable: Signal<bool>) -> Element {
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
```

- [ ] **Step 3: `ReadModeButton`** - master collapse of both rails, persisted `kb_read_mode`. Takes the two rail `collapsed` signals plus a `read_mode` signal; entering sets both collapsed and remembers prior state, exiting restores.

```rust
#[component]
fn ReadModeButton(
    read_mode: Signal<bool>,
    left_collapsed: Signal<bool>,
    right_collapsed: Signal<bool>,
    prior: Signal<(bool, bool)>,
) -> Element {
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
            if read_mode() { "⤡" } else { "⤢" }
        }
    }
}
```

- [ ] **Step 4: Verify compiles** `just check-web && just check-clippy`. Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/pages/knowledge_base.rs
git commit -m "feat(kb): add rating bar, density toggle, read-mode button"
```

---

## Task 9: Rewrite `KBArticleDetailPage` to the three-pane layout

**Files:**
- Modify: `src/pages/knowledge_base.rs` (`KBArticleDetailPage`, and slim `VersionHistoryCard`)

- [ ] **Step 1: Add the categories resource** to the detail page (needed for breadcrumb + tree). Mirror the existing `categories_resource` in `KBHomePage` (GET `/kb/categories?page=1&per_page=100`) and an articles list resource (GET `/kb/articles?page=1&per_page=200`) for the tree.

- [ ] **Step 2: Replace the render body.** Inside the `Some(Ok(article))` arm, replace the `grid grid-cols-1 lg:grid-cols-4` block with the three-pane layout. Signals at the top of the component:

```rust
let mut left_collapsed = use_signal(|| crate::utils::prefs::get_bool("kb_left_rail", false));
let mut right_collapsed = use_signal(|| crate::utils::prefs::get_bool("kb_right_rail", false));
let mut read_mode = use_signal(|| crate::utils::prefs::get_bool("kb_read_mode", false));
let mut comfortable = use_signal(|| crate::utils::prefs::get_bool("kb_density", true));
let prior = use_signal(|| (false, false));
let open_overlay = use_signal(|| None::<crate::components::RailSide>);
```

Persist left/right collapse whenever toggled by wrapping `set` calls (the rail toggles call `collapsed.set`; add `use_effect` to persist, or persist in the `ReadModeButton`/manual toggles). Simplest: add two `use_effect`s:

```rust
use_effect(move || crate::utils::prefs::set_bool("kb_left_rail", left_collapsed()));
use_effect(move || crate::utils::prefs::set_bool("kb_right_rail", right_collapsed()));
```

Layout RSX (inside the loaded arm):

```rust
let path = resolve_category_path(article.category_id, &categories);
let prose_density = if comfortable() { "prose-base leading-relaxed" } else { "prose-sm leading-snug" };
rsx! {
    div { class: "flex gap-6 items-start",
        // LEFT rail: tree
        CollapsibleRail { side: crate::components::RailSide::Left, collapsed: left_collapsed, open_overlay,
            KbTreeNav { categories: categories.clone(), articles: tree_articles.clone(), current_id: props.id.clone() }
        }
        // CENTER: article
        div { class: "flex-1 min-w-0",
            div { class: "flex items-center justify-between gap-2",
                KbBreadcrumb { path: path.clone(), title: article.title.clone() }
                ReadModeButton { read_mode, left_collapsed, right_collapsed, prior }
            }
            div { class: "mt-2 flex items-center justify-between gap-3 flex-wrap",
                h1 { class: "text-2xl font-semibold text-gray-900 dark:text-white truncate", "{article.title}" }
                OverflowActions {
                    RatingBar { article_id: props.id.clone(), helpful: article.helpful_count, not_helpful: article.not_helpful_count }
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
        // RIGHT rail: versions only
        CollapsibleRail { side: crate::components::RailSide::Right, collapsed: right_collapsed, open_overlay,
            VersionHistoryCard { article_id: props.id.clone(), versions_resource, on_restored: move |_| {
                article_resource.restart();
                versions_resource.restart();
            } }
        }
    }
}
```

> `tree_articles` is the data from the new articles list resource (`Vec<KbArticle>`). `content`, `updated`, `status_label`, `vis_label`, `status_variant`, `vis_variant` already exist in this arm - reuse them. Remove the old "Was this helpful?" card and the old right sidebar "Article Info" card entirely (their content now lives in the header / rating bar).

- [ ] **Step 3: Slim `VersionHistoryCard`** - keep the data/restore logic; render compactly (smaller type, no outer `Card title` chrome necessary; a simple bordered list is fine). Keep the `on_restored` and per-version restore button working.

- [ ] **Step 4: Verify compiles + manual check**

Run: `just check` (web + clippy + fmt). Expected: clean.
Then `just dev` and open an article: confirm markdown renders (headings/lists/code), no card/box around the body, tree on the left with current article highlighted, breadcrumb correct, rating on the title row updates on click, versions on the right, density toggle changes spacing and persists across reload, both chevrons collapse their rails and persist, read-mode button hides both rails and restores them, narrow window collapses rails to overlays one-at-a-time, and the header items collapse into `⋯` when narrow.

- [ ] **Step 5: Commit**

```bash
git add src/pages/knowledge_base.rs
git commit -m "feat(kb): three-pane reading view with rendered markdown and header"
```

---

## Task 10: Add tree rail to `KBArticleListPage`

**Files:**
- Modify: `src/pages/knowledge_base.rs` (`KBArticleListPage`)

- [ ] **Step 1:** Add a categories resource (if not present) and reuse the existing articles list resource for the tree. Wrap the existing list content in the same `flex gap-6` two-pane shell with a left `CollapsibleRail { side: Left, ... }` holding `KbTreeNav` (pass an empty/none `current_id`, e.g. `String::new()`), and the existing table as the main pane. No right rail here.

```rust
// current_id "" means nothing highlighted on the list page
KbTreeNav { categories: categories.clone(), articles: articles.clone(), current_id: String::new() }
```

- [ ] **Step 2: Verify** `just check` + `just dev`: list page shows the collapsible tree rail; collapse state persists (`kb_left_rail`, shared with detail).

- [ ] **Step 3: Commit**

```bash
git add src/pages/knowledge_base.rs
git commit -m "feat(kb): add collapsible tree rail to the article list page"
```

---

## Task 11: Authoring form - live slug + Write/Preview tabs

**Files:**
- Modify: `src/pages/knowledge_base.rs` (`ArticleForm`)

- [ ] **Step 1: Add the slug-touched test**

The form already has a `slug` signal and `slugify()` helper. Add a tiny pure helper and test for the touch behavior:

```rust
/// Given the previous title-derived slug, the current slug field value,
/// and whether the user has manually edited the slug, decide whether the
/// slug should follow the title. Returns the slug to set, or None to
/// leave it. Once `touched`, the slug never auto-updates again.
fn next_slug(new_title: &str, touched: bool) -> Option<String> {
    if touched { None } else { Some(slugify(new_title)) }
}
```

Test:

```rust
    #[test]
    fn slug_follows_title_until_touched() {
        assert_eq!(next_slug("My Article", false), Some("my-article".to_string()));
        assert_eq!(next_slug("My Article", true), None);
    }
```

(Confirm `slugify("My Article") == "my-article"` against the real helper; adjust the expected string to match its actual normalization.)

- [ ] **Step 2: Wire live slug.** Add `let mut slug_touched = use_signal(|| is_edit);` (edit mode starts touched so we never clobber an existing slug). In the title input `oninput`, after `title.set(...)`, also:

```rust
if let Some(s) = next_slug(&e.value(), slug_touched()) {
    slug.set(s);
}
```

In the slug input `oninput`, set `slug_touched.set(true);` before `slug.set(...)`.

- [ ] **Step 3: Write/Preview tabs.** Replace the single body `Textarea` with a two-tab control. Local `let mut tab = use_signal(|| BodyTab::Write);` where `enum BodyTab { Write, Preview }`. Tab buttons toggle it; Write shows the existing textarea bound to `content`; Preview shows `article { class: "prose dark:prose-invert max-w-none", dangerous_inner_html: crate::utils::markdown::render_markdown(&content.read()) }`.

```rust
#[derive(Clone, Copy, PartialEq)]
enum BodyTab { Write, Preview }
```

```rust
div { class: "flex gap-2 border-b border-gray-200 dark:border-gray-700 mb-2",
    button { class: tab_class(tab() == BodyTab::Write), onclick: move |_| tab.set(BodyTab::Write), "Write" }
    button { class: tab_class(tab() == BodyTab::Preview), onclick: move |_| tab.set(BodyTab::Preview), "Preview" }
}
match tab() {
    BodyTab::Write => rsx! { Textarea { /* existing props bound to content */ } },
    BodyTab::Preview => rsx! {
        article { class: "prose dark:prose-invert max-w-none p-2 min-h-40",
            dangerous_inner_html: crate::utils::markdown::render_markdown(&content.read()) }
    },
}
```

Add a small `fn tab_class(active: bool) -> &'static str` returning the active/inactive Tailwind classes.

- [ ] **Step 4: Run tests + verify**

Run: `just test` (slug test passes), then `just check`, then `just dev`: on a new article, slug fills as you type the title and stops once you edit the slug; on edit, slug is not auto-overwritten; Write/Preview tabs render the same markdown as the reading view.

- [ ] **Step 5: Commit**

```bash
git add src/pages/knowledge_base.rs
git commit -m "feat(kb): live slug generation and Write/Preview tabs on the form"
```

---

## Task 12: Final verification + PR

**Files:** none (verification)

- [ ] **Step 1: Full gate**

Run: `just check` (web + clippy `-D warnings` + fmt) and `just test`. Expected: all clean/green.

- [ ] **Step 2: Manual AC walkthrough** against `dev-docs/kb-ui-overhaul-design.md` acceptance criteria (every checkbox). Note any miss and fix before PR.

- [ ] **Step 3: Open PR via tea**

```bash
git push -u origin feat/kb-ui-overhaul
tea pr create --login a8n --base main --head feat/kb-ui-overhaul \
  --title "feat(kb): KB UI/UX overhaul - rendered markdown, three-pane reading view" \
  --description "<summary referencing dev-docs/kb-ui-overhaul-design.md and its acceptance criteria>"
```

- [ ] **Step 4:** `git checkout main` (user merges the PR).

---

## Self-review notes

- **Spec coverage:** markdown render+sanitize (T1), prefs persistence (T2), breadcrumb (T3/T5), tree build+nav (T4/T5), collapsible rails + overlay one-at-a-time (T6), overflow dropdown (T7), rating-on-title + density + read-mode (T8), three-pane detail with card-free article (T9), tree on list page (T10), live slug + Write/Preview tabs (T11), final gate (T12). All spec ACs map to a task.
- **TDD applied** to the pure logic (markdown, breadcrumb path, tree builder, slug-touched). Dioxus component behavior is verified manually under `just dev` because the project has no component-render test harness; the logic those components depend on is unit-tested.
- **Type consistency:** `render_markdown`, `resolve_category_path`, `build_kb_tree`/`TreeNode`, `next_slug`, `CollapsibleRail`/`RailSide`, `OverflowActions`, `RatingBar`, `DensityToggle`, `ReadModeButton`, `BodyTab` names are used consistently across tasks.
- **Open confirmations for the implementer (verify against live code before writing):** exact `KbCategory`/`KbArticle` field lists in `src/modules/kb/models.rs`; the `post_authed` generic signature; whether `ChevronRightIcon`/a down-chevron exist in `src/components/icons.rs`; `slugify`'s exact normalization.
