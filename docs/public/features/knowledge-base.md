# Knowledge base

The knowledge base is the tenant's library of articles. Staff write them, read them on the ticket side panel, and comment on them for internal review. Clients see the published-to-portal articles through their portal view.

## Browsing

Open Knowledge base from the sidebar. The index shows categories and recent articles. Search the KB from the top-bar search, scoped to the KB section.

Each article has a path, a title, a body, and a visibility: Staff-only, Portal, or Public. The index filter bar toggles between these; a client's portal view sees only Portal (and Public) articles.

## Reading an article

An article opens in read view with the body rendered, the version history on the side, and the comments below. The side panel carries the article's category, its tags, and a Related articles list.

A client reading an article from the portal sees the body and the public metadata; staff-only comments and the edit action are not shown.

## Creating and editing

Click New article from the index, or Edit from an article the signed-in user has write access to.

The editor is split: markdown source on the left, rendered preview on the right. Formatting is markdown; code blocks support syntax highlighting; images are attached by paste or by the Attach image button. A deployment that caps attachment size (see the knowledge-base attachment variable in `Configuration`) rejects an oversize file in the client, before the upload.

The markdown source field supports `Cmd+B`/`Ctrl+B` for Bold, `Cmd+I`/`Ctrl+I` for Italic, and `Cmd+K`/`Ctrl+K` for Link, matching the formatting toolbar above it. The same shortcuts work in every markdown field that uses this editor, including the ticket description and contact notes fields.

Save as Draft or Publish. A draft is visible only to the editing user and anyone the tenant configures as a reviewer. A published article is visible per its visibility setting.

## Versions

Every save creates a version. The version history on the side shows who, when, and what kind:

- **Content**: the body changed.
- **Metadata**: the title, category, or tags changed.
- **Visibility**: the visibility level changed.
- **Published**: the draft was published, or a published article was retracted to Draft.

Click a version to see the body at that point. Restore from a version returns the article to that state and opens a new draft; restore is recorded as a new save, not a rewind.

## Comments

Comments on an article are staff-only. Clients reading the article through the portal do not see them.

Three kinds:

- **Discussion**: a comment thread on the article. Anyone with read access can post.
- **Suggestion**: a proposed change to a specific span of the body. The reviewer accepts or rejects; accepted suggestions apply to the current draft.
- **Approval**: a review note. The tenant decides whether approval is required before publishing.

A comment on an article never reaches a client. The audit log records every comment.

## Templates

Some articles are templates rather than finished prose. A template is rendered when opened: a `{{customer.name}}` in the template becomes the current customer's name, a `{{ticket.title}}` becomes the title of the ticket that opened the article.

Template rendering happens server-side and is validated before display. A template that references a field the current context does not have renders that field as empty and records a warning on the article's audit trail.

Mark an article as a template through the article's side panel, Template toggle. A template cannot be made Public.

## From a ticket

Open the ticket's side panel and the KB suggestions list the articles most relevant to the ticket. Suggestions come from the ticket's title and the recent timeline; a staff member picks an article to attach it to the ticket, which inserts a link in the next composer message and records the attachment on the timeline.
