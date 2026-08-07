# KB UI/UX Overhaul - Design

Date: 2026-06-05
Status: Proposed (awaiting review)
Area: `src/pages/knowledge_base.rs`, `src/modules/kb/`, `Cargo.toml`

> **Historical record.** Parts of the article-detail layout below were superseded
> by #MAPPS-423, which found the resulting title row too busy. Where this document
> puts the rating, Edit / Delete, "Open ticket about this article" and the density
> toggle on the title row inside `OverflowActions`, the current screen puts those
> four actions in an "Actions" card at the top of the right rail (with a header
> `...` menu as the fallback for read mode and sub-`sm` widths), keeps only the
> breadcrumb, title, status / visibility badges, `Updated` and the read-mode button
> in the header, and moves the rating to a "Was this helpful?" row at the end of the
> article body, rendered only when the tenant has two or more users. The nav tree
> also hides categories that hold no articles anywhere in their subtree.

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
+--[‹]--------------+-------------------------------[⛶]+--------[›]-----+
| category/article  | KB / Cat / Title              fs |  Versions     |
| tree (chevron ‹)  | # Title  👍12 👎1  [Pub][Int] Edit|  v3 today     |
|  current highlighted| Updated 6/5                     |  v2 · 6/4 ··· |
|                   | rendered markdown (no <pre>)     |  v1 · ...      |
+-------------------+----------------------------------+---------------+
   left rail                 article only                right rail (chevron ›)
   [⛶] = fullscreen read-mode toggle, top-right of the center pane
   title row carries rating + status/visibility badges + Edit; right rail is just versions
```

Both side rails are collapsible (see "Rail collapse and responsive behavior" below); the center article pane always stays.

- **Left pane - tree nav.** A tree built from `KbCategory` (grouped by `parent_id`, ordered by `sort_order`) with each category's articles (`category_id`) nested beneath it. The current article is highlighted. Categories collapse/expand internally; the whole rail is collapsible via a chevron. Data comes from the existing `/kb/categories` and `/kb/articles` list endpoints.
- **Center pane - article header + article.** A header block then the body:
  - Breadcrumb row: `KB / <category path> / <title>` (resolved via `category_id` + walking `parent_id`) on the left; the fullscreen read-mode toggle on the right.
  - Title row: the title, with the **rating inline** - a thumbs-up and thumbs-down each showing its count (`👍 12  👎 1`, from `helpful_count` / `not_helpful_count`) - plus the **status** and **visibility** badges and the **Edit** action. Everything that can live with the title moves here so it stays visible regardless of rail collapse or read mode.
  - **Responsive overflow:** the title always stays inline. When the row is too narrow to fit the secondary header items (rating, status/visibility badges, Edit) without overflowing, they collapse into a single overflow dropdown (a `⋯` / kebab trigger on the title row) instead of bleeding out of the container; widen the row and they return inline. One reusable `OverflowActions` piece owns this measure-and-collapse behavior.
  - Sub-line: a small muted "Updated <date>" (and created, if useful).
  - Then the rendered markdown, **directly on the page** - not wrapped in a `Card`, not in a `<pre>` block - so it reads as a document, not a boxed card.
- **Right pane - version-history rail.** Slimmed to just the compact version-history list (small type, restore action preserved). Status, visibility, updated date, rating, and edit have all moved to the article header, so the right rail is now only "tiny version history on the right," leaving the center as just the article.

### 2b. Rail collapse and responsive behavior

Each side rail (left tree, right metadata) carries a directional chevron toggle that collapses it to a thin edge handle and expands it again. The chevron points outward toward the screen edge to collapse and inward to expand (left rail: `<` collapses / `>` expands; right rail: `>` collapses / `<` expands), so the affordance reads naturally on each side.

- **Wide screens:** both rails are expanded by default and sit inline beside the article. The user may collapse either (or both) via its chevron to focus on the article; the collapsed/expanded state for each rail is persisted per user in `localStorage` (keys `kb_left_rail`, `kb_right_rail`) so the choice sticks across reloads, same mechanism as the density toggle.
- **Narrow screens:** rails auto-collapse to edge handles so the article gets the full width. A collapsed rail opens as an overlay above the article when its chevron is clicked. Only one rail overlay may be open at a time: while one is open the other cannot be opened until the first is dismissed (click outside it, or its close chevron). This avoids two overlays fighting for a narrow viewport.

The collapse mechanism is one reusable `CollapsibleRail` component (side = left|right, persisted key, overlay-on-narrow), used for both rails so the behavior is identical.

**Read mode (wide screens).** A fullscreen icon button collapses both rails in a single click for a distraction-free, article-only read; clicking it again restores whatever each rail's prior state was. It is effectively a master toggle over the two per-rail chevrons. The read-mode state persists per user (`kb_read_mode`); while active it overrides the individual rail states, and exiting it restores them.

Placement: top-right of the center article pane, right-aligned on the breadcrumb row (breadcrumb on the left, the fullscreen icon on the right). This is the conventional corner for an expand/fullscreen control, it sits next to the right rail it hides, and it stays clear of the title and body so it never interrupts the reading flow. The icon swaps between an expand glyph (enter read mode) and a contract glyph (exit).

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

- [ ] Article body renders as real markdown (headings, lists, code, links), not preformatted text, directly on the page with no surrounding `Card` or box.
- [ ] Rendered HTML is sanitized (ammonia); a malicious tag in content does not execute.
- [ ] Reading view is three panes: tree nav left; article center with a header (breadcrumb, title, inline rating, status/visibility badges, Edit, updated date); right rail is only the tiny version history.
- [ ] Rating is `👍 <n>  👎 <n>` on the title row (not on the right rail) and stays visible under rail collapse and read mode; status, visibility, updated date, and Edit are also in the header, not the rail.
- [ ] When the header row is too narrow to fit inline, the secondary items (rating, badges, Edit) collapse into an overflow `⋯` dropdown rather than overflowing the container; the title stays inline; widening restores them.
- [ ] Left tree nav lists categories (hierarchical, sorted) with their articles, highlights the current one, and is collapsible. Present on detail and list pages.
- [ ] Each side rail has a directional chevron toggle (`<`/`>`) that collapses/expands it; the wide-screen collapsed state persists per user across reloads.
- [ ] On narrow screens both rails auto-collapse and open as overlays; only one rail overlay can be open at a time (the other is blocked until the open one is dismissed).
- [ ] A fullscreen read-mode button at the top-right of the center pane collapses both rails in one click and restores their prior state on exit; its state persists per user and the icon reflects enter/exit.
- [ ] Breadcrumb shows `KB / <category path> / <title>` and resolves nested categories.
- [ ] Reader density toggle (compact/comfortable) changes article spacing and persists per user across reloads.
- [ ] Rating 👍/👎 lives in the right rail and reflects/updates `helpful_count` / `not_helpful_count`.
- [ ] Version history is in the right rail, compact, with restore intact.
- [ ] On the create form, the slug auto-fills from the title as you type and stops once the slug is edited by hand; edit mode never auto-clobbers the existing slug.
- [ ] The body editor has Write / Preview tabs; Preview renders via the same `render_markdown` path.
- [ ] `cargo check`, `cargo clippy -- -D warnings`, `cargo fmt --check` pass (in the dev container).
