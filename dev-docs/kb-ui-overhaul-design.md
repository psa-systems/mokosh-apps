# KB UI/UX Overhaul - Design

Date: 2026-06-05
Status: Proposed (awaiting review)
Area: `src/pages/knowledge_base.rs`, `src/modules/kb/`, `Cargo.toml`

## Background

The Knowledge Base client pages were wired to the API under PMS-79. They work, but the reading and authoring experience is rough:

- The article body is rendered inside a `<pre class="whitespace-pre-wrap">` block ([knowledge_base.rs:655](../src/pages/knowledge_base.rs#L655)). Markdown source is shown verbatim, not rendered, and the `<pre>` gives it a boxed, monospace look. A comment at line 651 explicitly defers real rendering.
- The article detail view is a 4-column grid: the article occupies `lg:col-span-3` on the left with the rating thumbs embedded in that same column, and the right `1/4` holds metadata plus version history.
- There is no way to browse between articles without going back to the list; the category hierarchy (`KbCategory.parent_id`) is not surfaced as navigation.
- On the create/edit form, the slug only derives from the title at submit time when left blank ([knowledge_base.rs:1035](../src/pages/knowledge_base.rs#L1035)); it does not update live as the author types the title.
- There is no preview of the markdown while authoring.

The data model already supports the target design: `KbCategory` carries `parent_id`, `slug`, and `sort_order` (a real hierarchy); `KbArticle` carries `category_id`, `content` (markdown), `helpful_count`/`not_helpful_count`, and versions exist via `KbArticleVersion`. `pulldown-cmark = "0.12"` is already a dependency.

## Goal

Make the KB read and author like a modern docs tool (YouTrack KB as the reference): a clean three-pane reading layout with rendered markdown, a browsable category/article tree, breadcrumbs, a reader density preference, and a smoother authoring form with live slug generation and a markdown preview.

## Design

### 1. Markdown rendering (shared)

Add a single helper, `render_markdown(src: &str) -> String`, in a new `src/utils/markdown.rs`:

1. Parse `src` with `pulldown-cmark` (already a dependency) into HTML.
2. Sanitize the HTML with `ammonia` (new dependency) before returning it.

Both the reading view and the authoring preview consume this helper. The output is injected with Dioxus `dangerous_inner_html`. Sanitizing matters because KB content also feeds the public portal feed, so we do not trust raw HTML to reach a browser unscrubbed even though authors are internal staff.

### 2. Reading view (`/kb/articles/:id`) - three panes

Replace the current 4-column grid with a three-pane layout:

```
+-------------------+--------------------------------+---------------+
| category/article  | breadcrumb: KB / Cat / Title   | metadata      |
| tree (collapsible)| # Title                        | rating 👍/👎  |
|  current highlighted| rendered markdown (no <pre>) | versions(tiny)|
+-------------------+--------------------------------+---------------+
```

- **Left pane - tree nav.** A collapsible tree built from `KbCategory` (grouped by `parent_id`, ordered by `sort_order`) with each category's articles (`category_id`) nested beneath it. The current article is highlighted. Categories collapse/expand; the whole rail collapses to reclaim width. Data comes from the existing `/kb/categories` and `/kb/articles` list endpoints.
- **Center pane - article only.** Breadcrumb at the top (`KB / <category path> / <title>`), resolved by looking up `category_id` and walking `parent_id` to the root. Then the title, then the rendered markdown. No rating or version controls here.
- **Right pane - metadata rail.** Status, visibility, and updated date (as today), then the rating widget (`helpful_count` / `not_helpful_count` with 👍 / 👎 actions, moved out of the center column), then a compact version-history list (smaller type, restore action preserved).

### 3. Reader density toggle

A compact/comfortable toggle on the reading view, persisted per user in `localStorage` (key `kb_density`). It switches the prose container's spacing classes (line-height and vertical margins). Default: comfortable. It only affects the center article pane.

### 4. Breadcrumb

A small reusable piece that, given an article's `category_id`, resolves the category name and walks `parent_id` up to the root to render `KB / Parent / Child / <title>`. Each segment except the title links to its scope (KB home, category-filtered list).

### 5. Authoring form (`/kb/articles/new`, `/edit`)

- **Live slug generation.** As the author types the title, the slug field updates live via the existing `slugify()` helper. Track a `slug_touched` signal: once the author edits the slug by hand, stop auto-overriding it. On an existing article (edit mode) the slug is treated as already touched so we never clobber a published slug.
- **Write / Preview tabs.** The body editor gets two tabs: "Write" (the current textarea) and "Preview" (the body run through `render_markdown`). Tabs, not a split pane, so it works on narrow screens.

## Components affected

- `src/utils/markdown.rs` (new) - `render_markdown`.
- `src/pages/knowledge_base.rs` - `KBArticleDetailPage` (three-pane rewrite, density toggle, breadcrumb, rating moved, versions restyled), `KBArticleListPage` (add tree rail), `ArticleForm` (live slug, write/preview tabs). New child components: `KbTreeNav`, `KbBreadcrumb`, `RatingWidget`, `DensityToggle`.
- `Cargo.toml` - add `ammonia`.

`knowledge_base.rs` is already ~1200 lines; the new child components keep each unit focused rather than growing the page functions further.

## Alternatives considered

- **Tree nav on every KB page (incl. home/edit):** rejected for now to bound scope; home stays the card landing, edit stays focused on the form. Detail + list cover the browse use case.
- **Split-pane live preview:** rejected in favor of Write/Preview tabs so the editor is usable on narrow screens.
- **Skip sanitization (trust authors):** rejected because the same content reaches the public portal feed.
- **Server-side markdown rendering:** rejected; rendering client-side keeps the server returning source and lets the preview reuse the exact same path.

## Testing

- Unit-test `render_markdown`: headings/lists/code render; a script tag / `onerror` attribute is stripped by ammonia.
- Unit-test breadcrumb path resolution for a nested category and for an article with no category.
- Unit-test `slugify` live behavior is already covered by the helper; add a test that `slug_touched` stops auto-override.
- Manual: reading view renders real markdown with no box, tree highlights current article, density toggle persists across reload, rating posts and updates counts, Write/Preview tabs, live slug on a new article.

## Acceptance criteria

- [ ] Article body renders as real markdown (headings, lists, code, links), not preformatted text, with no surrounding box.
- [ ] Rendered HTML is sanitized (ammonia); a malicious tag in content does not execute.
- [ ] Reading view is three panes: tree nav left, article only center (with breadcrumb), metadata+rating+tiny versions right.
- [ ] Left tree nav lists categories (hierarchical, sorted) with their articles, highlights the current one, and is collapsible. Present on detail and list pages.
- [ ] Breadcrumb shows `KB / <category path> / <title>` and resolves nested categories.
- [ ] Reader density toggle (compact/comfortable) changes article spacing and persists per user across reloads.
- [ ] Rating 👍/👎 lives in the right rail and reflects/updates `helpful_count` / `not_helpful_count`.
- [ ] Version history is in the right rail, compact, with restore intact.
- [ ] On the create form, the slug auto-fills from the title as you type and stops once the slug is edited by hand; edit mode never auto-clobbers the existing slug.
- [ ] The body editor has Write / Preview tabs; Preview renders via the same `render_markdown` path.
- [ ] `cargo check`, `cargo clippy -- -D warnings`, `cargo fmt --check` pass (in the dev container).
