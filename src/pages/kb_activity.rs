//! The activity stream at the foot of a knowledge base article (MAPPS-742).
//!
//! One stream of two things the page already fetches: the comments
//! (PMS-1128) and the versions (PMS-1126), merged by time, oldest first, so
//! "what happened to this article" reads top to bottom. Tabs narrow it to
//! comments or history; a version event is one line that opens to the
//! MAPPS-739 diff; a comment is a thread one reply deep, resolvable at the
//! root, editable by its author or an admin, and its text goes through the
//! same `components::Markdown` as the article, so there is no second, laxer
//! render path for a body someone else typed. The composer is the same
//! editor as everywhere else, with the same `@handle` autocomplete; the
//! server records and notifies the mention (PMS-1129).
//!
//! No server activity endpoint: the merge is the client's, because both
//! halves are already on it and the order is a sort.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

use crate::components::{
    Avatar, Button, ButtonSize, ButtonVariant, Card, ConfirmDialog, ErrorBanner,
};
use crate::modules::kb::{
    CreateKbCommentRequest, KbArticle, KbArticleVersion, KbComment, UpdateKbCommentRequest,
};
use crate::pages::knowledge_base::{change_kind_label, person_label, when_label, LineDiffView};
use crate::utils::datetime::fmt_relative;

/// Which slice of the stream is showing. Remembered in `prefs` under
/// `kb_activity_tab`, which is what "Activity settings" is here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    All,
    Comments,
    History,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::All, Tab::Comments, Tab::History];

    pub fn key(self) -> &'static str {
        match self {
            Tab::All => "all",
            Tab::Comments => "comments",
            Tab::History => "history",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::All => "All activity",
            Tab::Comments => "Comments",
            Tab::History => "History",
        }
    }

    pub fn from_key(key: &str) -> Tab {
        match key {
            "comments" => Tab::Comments,
            "history" => Tab::History,
            _ => Tab::All,
        }
    }

    /// The tab an arrow key lands on: left goes back, right goes on, both
    /// wrapping, the WAI-ARIA tabs pattern.
    pub fn step(self, forward: bool) -> Tab {
        let idx = Tab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        let n = Tab::ALL.len();
        let next = if forward {
            (idx + 1) % n
        } else {
            (idx + n - 1) % n
        };
        Tab::ALL[next]
    }
}

/// One thing in the stream.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// A root comment with its replies.
    Thread(KbComment),
    /// A version, with the version before it for the diff.
    Version {
        version: KbArticleVersion,
        previous: Option<KbArticleVersion>,
    },
}

impl Entry {
    fn at(&self) -> DateTime<Utc> {
        match self {
            Entry::Thread(c) => c.created_at,
            Entry::Version { version, .. } => version.created_at.unwrap_or_default(),
        }
    }
}

/// Merge the two halves by time, oldest first, and apply the tab and the
/// resolved filter. Versions come newest first from the card's fetch, so the
/// previous version of each is the one after it in that order.
pub fn build_stream(
    comments: &[KbComment],
    versions_newest_first: &[KbArticleVersion],
    tab: Tab,
    show_resolved: bool,
) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    if tab != Tab::History {
        for c in comments {
            if !show_resolved && c.resolved_at.is_some() {
                continue;
            }
            out.push(Entry::Thread(c.clone()));
        }
    }
    if tab != Tab::Comments {
        for (idx, v) in versions_newest_first.iter().enumerate() {
            out.push(Entry::Version {
                version: v.clone(),
                previous: versions_newest_first.get(idx + 1).cloned(),
            });
        }
    }
    out.sort_by_key(Entry::at);
    out
}

const TAB_PREF: &str = "kb_activity_tab";
const RESOLVED_PREF: &str = "kb_activity_show_resolved";

#[component]
pub fn ArticleActivity(
    article: KbArticle,
    versions_resource: Resource<Option<Vec<KbArticleVersion>>>,
    /// MAPPS-744: read by the page and shared with the inline layer.
    comments_resource: Resource<Option<Vec<KbComment>>>,
    /// MAPPS-744: the anchored roots whose passage is no longer in the
    /// article, as the inline layer found on this render.
    orphans: Signal<Vec<uuid::Uuid>>,
    /// MAPPS-744: the thread a mark asked to jump to.
    focus_thread: Signal<Option<uuid::Uuid>>,
) -> Element {
    let article_id = article.id.to_string();
    let mut comments_resource = comments_resource;
    let mut tab = use_signal(|| Tab::from_key(&crate::utils::prefs::get_str(TAB_PREF, "all")));
    let mut show_resolved = use_signal(|| crate::utils::prefs::get_bool(RESOLVED_PREF, false));
    let mut settings_open = use_signal(|| false);
    let mut error = use_signal(String::new);
    let auth = crate::hooks::auth::use_auth();
    let (me, my_name, my_avatar, is_admin) = {
        let a = auth.read();
        (
            a.user.as_ref().map(|u| u.id),
            a.user
                .as_ref()
                .map(|u| {
                    format!("{} {}", u.first_name, u.last_name)
                        .trim()
                        .to_string()
                })
                .unwrap_or_default(),
            a.user.as_ref().and_then(|u| u.avatar_url.clone()),
            a.is_admin(),
        )
    };
    let can_mutate = crate::hooks::use_can_mutate();
    let mention_directory = crate::hooks::use_mention_directory(true);

    let comments_snap = comments_resource.read_unchecked().clone();
    let versions_snap = versions_resource.read_unchecked().clone();
    let comments: Vec<KbComment> = comments_snap.clone().flatten().unwrap_or_default();
    let versions: Vec<KbArticleVersion> = versions_snap.clone().flatten().unwrap_or_default();
    let stream = build_stream(&comments, &versions, tab(), show_resolved());
    let comments_failed = matches!(comments_snap, Some(None));
    let loading = comments_snap.is_none();
    let on_changed = EventHandler::new(move |_: ()| comments_resource.restart());
    let people = crate::hooks::mention_people(&mention_directory);

    rsx! {
        div { class: "mt-8",
            Card { title: "Activity",
                div { class: "flex items-center justify-between gap-3 border-b border-line pb-2 mb-4",
                    // MAPPS-742: the three slices, a real tablist so the arrow
                    // keys walk it.
                    div {
                        role: "tablist",
                        "aria-label": "Activity",
                        class: "inline-flex gap-1 rounded-lg bg-surface-2 p-1",
                        onkeydown: move |e: KeyboardEvent| {
                            let next = match e.key() {
                                Key::ArrowRight => Some(tab().step(true)),
                                Key::ArrowLeft => Some(tab().step(false)),
                                _ => None,
                            };
                            if let Some(next) = next {
                                e.prevent_default();
                                tab.set(next);
                                crate::utils::prefs::set_str(TAB_PREF, next.key());
                                crate::platform::dom::focus_by_id(&format!("kb-activity-tab-{}", next.key()));
                            }
                        },
                        for t in Tab::ALL {
                            {
                                let selected = tab() == t;
                                let cls = if selected {
                                    "bg-surface text-content shadow-sm"
                                } else {
                                    "text-muted hover:text-content"
                                };
                                rsx! {
                                    button {
                                        key: "{t.key()}",
                                        id: "kb-activity-tab-{t.key()}",
                                        r#type: "button",
                                        role: "tab",
                                        "aria-selected": if selected { "true" } else { "false" },
                                        tabindex: if selected { "0" } else { "-1" },
                                        class: "rounded-md px-3 py-1 text-xs font-medium focus:outline-none focus:ring-2 focus:ring-accent {cls}",
                                        onclick: move |_| {
                                            tab.set(t);
                                            crate::utils::prefs::set_str(TAB_PREF, t.key());
                                        },
                                        "{t.label()}"
                                    }
                                }
                            }
                        }
                    }
                    // "Activity settings": what stays folded.
                    div { class: "relative",
                        button {
                            r#type: "button",
                            class: "text-xs text-muted hover:text-content",
                            "aria-expanded": if settings_open() { "true" } else { "false" },
                            onclick: move |_| settings_open.toggle(),
                            "Activity settings \u{25be}"
                        }
                        if settings_open() {
                            div { class: "fixed inset-0 z-10", onclick: move |_| settings_open.set(false) }
                            div { class: "dropdown-panel absolute right-0 z-20 mt-1 w-56 p-3",
                                label { class: "flex items-center gap-2 text-sm text-content",
                                    input {
                                        r#type: "checkbox",
                                        checked: show_resolved(),
                                        onchange: move |e: FormEvent| {
                                            let on = e.checked();
                                            show_resolved.set(on);
                                            crate::utils::prefs::set_bool(RESOLVED_PREF, on);
                                        },
                                    }
                                    "Show resolved threads"
                                }
                            }
                        }
                    }
                }

                if !error.read().is_empty() {
                    ErrorBanner { "{error.read()}" }
                }
                if comments_failed {
                    p { class: "text-sm text-red-600 dark:text-red-300", "Could not load the comments." }
                }

                if loading && stream.is_empty() {
                    p { class: "text-sm text-subtle", "Loading…" }
                } else if stream.is_empty() {
                    p { class: "text-sm text-subtle italic",
                        match tab() {
                            Tab::Comments => "No comments yet. Start the discussion below.",
                            Tab::History => "No history to show.",
                            Tab::All => "Nothing has happened here yet.",
                        }
                    }
                } else {
                    ul { class: "space-y-4",
                        for entry in stream.iter() {
                            match entry {
                                Entry::Thread(root) => rsx! {
                                    li { key: "c-{root.id}", id: "kb-comment-{root.id}",
                                        CommentThread {
                                            root: root.clone(),
                                            article_id: article_id.clone(),
                                            me,
                                            is_admin,
                                            can_mutate,
                                            people: people.clone(),
                                            orphaned: orphans.read().contains(&root.id),
                                            focused: focus_thread() == Some(root.id),
                                            on_changed,
                                            on_error: move |m: String| error.set(m),
                                        }
                                    }
                                },
                                Entry::Version { version, previous } => rsx! {
                                    li { key: "v-{version.id}",
                                        VersionEvent { version: version.clone(), previous: previous.clone() }
                                    }
                                },
                            }
                        }
                    }
                }

                // The composer: the same editor as everywhere, the reader's own
                // disc beside it, Ctrl/Cmd+Enter to send.
                if tab() != Tab::History {
                    div { class: "mt-6 pt-4 border-t border-line",
                        CommentComposer {
                            article_id: article_id.clone(),
                            parent_id: None,
                            anchor: None,
                            author_name: my_name.clone(),
                            author_avatar: my_avatar.clone(),
                            can_mutate,
                            people: people.clone(),
                            placeholder: "Write a comment, @mention people".to_string(),
                            submit_label: "Comment".to_string(),
                            on_posted: move |_| {
                                error.set(String::new());
                                on_changed.call(());
                            },
                            on_cancel: None,
                        }
                    }
                }
            }
        }
    }
}

/// A version as a stream line: what kind of change, by whom, when, the note,
/// and a disclosure to the diff against the version before.
#[component]
fn VersionEvent(version: KbArticleVersion, previous: Option<KbArticleVersion>) -> Element {
    let mut open = use_signal(|| false);
    let kind = change_kind_label(&version.change_kind, version.restored_from_version);
    let who = person_label(version.edited_by_name.as_deref());
    let when = version
        .created_at
        .map(fmt_relative)
        .unwrap_or_else(|| "-".to_string());
    let absolute = when_label(&version.created_at);
    let note = version
        .change_note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let n = version.version_number;
    rsx! {
        div { class: "flex gap-3 text-sm",
            Avatar { name: who.clone() }
            div { class: "min-w-0 flex-1",
                p { class: "text-content",
                    span { class: "font-medium", "{who}" }
                    " {kind.to_lowercase()} the article (v{n}) "
                    span { class: "text-subtle", title: "{absolute}", "{when}" }
                }
                if let Some(note) = note {
                    p { class: "italic text-muted", "\u{201c}{note}\u{201d}" }
                }
                if let Some(previous) = previous.as_ref() {
                    button {
                        r#type: "button",
                        class: "mt-1 text-xs text-accent hover:opacity-90",
                        "aria-expanded": if open() { "true" } else { "false" },
                        onclick: move |_| open.toggle(),
                        if open() { "Hide changes" } else { "Show changes" }
                    }
                    if open() {
                        div { class: "mt-2",
                            LineDiffView { old: previous.content.clone(), new: version.content.clone() }
                        }
                    }
                }
            }
        }
    }
}

/// A root comment with its replies, the reply composer under it, and the
/// resolve control on the root. A resolved thread folds to one line.
#[component]
fn CommentThread(
    root: KbComment,
    article_id: String,
    me: Option<uuid::Uuid>,
    is_admin: bool,
    can_mutate: bool,
    people: Vec<crate::utils::mentions::Mention>,
    /// MAPPS-744: the passage this thread was anchored to is gone.
    orphaned: bool,
    /// MAPPS-744: a mark asked to jump here.
    focused: bool,
    on_changed: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut replying = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let resolved = root.resolved_at.is_some();
    let root_id = root.id;
    let reply_count = root.replies.len();
    let resolver = root
        .resolved_by_name
        .clone()
        .unwrap_or_else(|| "someone".to_string());
    let set_resolved = use_callback(move |resolve: bool| {
        if busy() || !can_mutate {
            return;
        }
        busy.set(true);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let verb = if resolve { "resolve" } else { "unresolve" };
                let path = format!("/kb/comments/{root_id}/{verb}");
                match crate::hooks::fetch::api::post_authed_typed::<KbComment, _>(
                    &path,
                    &serde_json::json!({}),
                )
                .await
                {
                    Ok(_) => on_changed.call(()),
                    Err(e) => {
                        on_error.call(format!("Could not {verb} the thread: {}", e.user_message()))
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = resolve;
            busy.set(false);
        });
    });

    // MAPPS-744: an anchored root quotes its passage; an orphan says the
    // passage is gone and keeps the quote, which is what it has left.
    let quote = root
        .anchor
        .as_ref()
        .and_then(crate::utils::anchor::Anchor::from_json)
        .map(|a| a.exact);
    let root_for_jump = root.id.to_string();
    let frame = if focused {
        "rounded-md border border-accent p-3 ring-2 ring-accent"
    } else {
        "rounded-md border border-line p-3"
    };
    rsx! {
        div { class: "{frame}",
            if let Some(quote) = quote.as_ref() {
                div { class: "mb-2 flex items-start justify-between gap-2",
                    blockquote { class: "min-w-0 border-l-2 border-amber-400 pl-2 text-sm italic text-muted truncate", "\u{201c}{quote}\u{201d}" }
                    if orphaned {
                        span { class: "shrink-0 rounded-full bg-surface-2 px-2 py-0.5 text-xs text-muted", title: "The text this comment was attached to is no longer in the article",
                            "Refers to text that is no longer in the article"
                        }
                    } else if !resolved {
                        Button {
                            variant: ButtonVariant::Link,
                            size: ButtonSize::Small,
                            onclick: move |_| crate::platform::anchor_dom::focus_mark(crate::pages::knowledge_base::KB_ARTICLE_BODY_ID, &root_for_jump),
                            "Show in article"
                        }
                    }
                }
            }
            if resolved {
                // Folded: one line, and the way back.
                div { class: "flex items-center justify-between gap-3 text-sm",
                    p { class: "text-muted truncate",
                        span { class: "font-medium text-content", "{root.author_name}" }
                        ": "
                        span { class: "line-through", "{root.body}" }
                        span { class: "ml-2 text-xs text-subtle", "resolved by {resolver}" }
                        if reply_count > 0 {
                            span { class: "ml-2 text-xs text-subtle", "({reply_count} replies)" }
                        }
                    }
                    Button {
                        variant: ButtonVariant::Link,
                        size: ButtonSize::Small,
                        disabled: busy() || !can_mutate,
                        onclick: move |_| set_resolved.call(false),
                        "Unresolve"
                    }
                }
            } else {
                CommentBody {
                    comment: root.clone(),
                    article_id: article_id.clone(),
                    me,
                    is_admin,
                    can_mutate,
                    people: people.clone(),
                    on_changed,
                    on_error,
                    extra: rsx! {
                        Button {
                            variant: ButtonVariant::Link,
                            size: ButtonSize::Small,
                            disabled: !can_mutate,
                            onclick: move |_| replying.toggle(),
                            "Reply"
                        }
                        Button {
                            variant: ButtonVariant::Link,
                            size: ButtonSize::Small,
                            disabled: busy() || !can_mutate,
                            onclick: move |_| set_resolved.call(true),
                            "Resolve"
                        }
                    },
                }
                if !root.replies.is_empty() {
                    ul { class: "mt-3 ml-8 space-y-3 border-l border-line pl-4",
                        for reply in root.replies.iter() {
                            li { key: "{reply.id}",
                                CommentBody {
                                    comment: reply.clone(),
                                    article_id: article_id.clone(),
                                    me,
                                    is_admin,
                                    can_mutate,
                                    people: people.clone(),
                                    on_changed,
                                    on_error,
                                    extra: rsx! {},
                                }
                            }
                        }
                    }
                }
                if replying() {
                    div { class: "mt-3 ml-8",
                        CommentComposer {
                            article_id: article_id.clone(),
                            parent_id: Some(root_id),
                            anchor: None,
                            author_name: String::new(),
                            author_avatar: None,
                            can_mutate,
                            people: people.clone(),
                            placeholder: "Write a reply, @mention people".to_string(),
                            submit_label: "Reply".to_string(),
                            on_posted: move |_| {
                                replying.set(false);
                                on_changed.call(());
                            },
                            on_cancel: Some(EventHandler::new(move |_: ()| replying.set(false))),
                        }
                    }
                }
            }
        }
    }
}

/// One comment: disc, name, when, edited mark, the body through the shared
/// renderer, and Edit / Delete for its author or an admin. `extra` is what
/// the caller adds beside those (Reply and Resolve on a root).
#[component]
fn CommentBody(
    comment: KbComment,
    article_id: String,
    me: Option<uuid::Uuid>,
    is_admin: bool,
    can_mutate: bool,
    people: Vec<crate::utils::mentions::Mention>,
    on_changed: EventHandler<()>,
    on_error: EventHandler<String>,
    extra: Element,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut confirming_delete = use_signal(|| false);
    let mut delete_error = use_signal(String::new);
    let id = comment.id;
    let mine = me == Some(comment.author_id);
    let may_change = !comment.deleted && (mine || is_admin);
    let when = fmt_relative(comment.created_at);
    let absolute = crate::utils::datetime::fmt_datetime_pref(comment.created_at);
    let body_for_edit = comment.body.clone();

    rsx! {
        ConfirmDialog {
            open: confirming_delete(),
            title: "Delete this comment?".to_string(),
            message: "The text goes; the thread keeps its place so replies still read in order.".to_string(),
            confirm_text: "Delete".to_string(),
            destructive: true,
            loading: busy(),
            error: delete_error(),
            onconfirm: move |_| {
                if busy() {
                    return;
                }
                busy.set(true);
                spawn(async move {
                    #[cfg(feature = "app")]
                    {
                        match crate::hooks::fetch::api::delete_authed(&format!("/kb/comments/{id}")).await {
                            Ok(()) => {
                                confirming_delete.set(false);
                                on_changed.call(());
                            }
                            Err(e) => delete_error.set(format!("Could not delete the comment: {e}")),
                        }
                    }
                    busy.set(false);
                });
            },
            oncancel: move |_| confirming_delete.set(false),
        }
        div { class: "flex gap-3",
            Avatar { name: comment.author_name.clone(), url: comment.author_avatar_url.clone() }
            div { class: "min-w-0 flex-1",
                div { class: "flex items-center gap-2 text-sm",
                    span { class: "font-medium text-content", "{comment.author_name}" }
                    span { class: "text-subtle", title: "{absolute}", "{when}" }
                    if comment.edited_at.is_some() && !comment.deleted {
                        span { class: "text-xs text-subtle", "(edited)" }
                    }
                }
                if comment.deleted {
                    p { class: "mt-1 text-sm italic text-subtle", "Comment deleted" }
                } else if editing() {
                    div { class: "mt-2 space-y-2",
                        crate::components::MarkdownEditor {
                            name: "kb_comment_edit".to_string(),
                            label: "Edit comment".to_string(),
                            label_hidden: true,
                            rows: 4,
                            value: draft(),
                            people: people.clone(),
                            oninput: move |next: String| draft.set(next),
                        }
                        div { class: "flex gap-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Small,
                                loading: busy(),
                                disabled: busy() || draft().trim().is_empty() || !can_mutate,
                                onclick: move |_| {
                                    busy.set(true);
                                    let body = draft().trim().to_string();
                                    spawn(async move {
                                        #[cfg(feature = "app")]
                                        {
                                            let path = format!("/kb/comments/{id}");
                                            match crate::hooks::fetch::api::put_authed::<KbComment, _>(&path, &UpdateKbCommentRequest { body }).await {
                                                Ok(_) => {
                                                    editing.set(false);
                                                    on_changed.call(());
                                                }
                                                Err(e) => on_error.call(format!("Could not save the comment: {e}")),
                                            }
                                        }
                                        #[cfg(not(feature = "app"))]
                                        let _ = body;
                                        busy.set(false);
                                    });
                                },
                                "Save"
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Small,
                                onclick: move |_| editing.set(false),
                                "Cancel"
                            }
                        }
                    }
                } else {
                    // The same renderer and sanitizer as the article.
                    div { class: "mt-1 text-sm",
                        crate::components::Markdown { content: comment.body.clone() }
                    }
                }
                if !comment.deleted && !editing() {
                    div { class: "mt-1 flex items-center gap-1",
                        {extra}
                        if may_change {
                            Button {
                                variant: ButtonVariant::Link,
                                size: ButtonSize::Small,
                                disabled: !can_mutate,
                                onclick: move |_| {
                                    draft.set(body_for_edit.clone());
                                    editing.set(true);
                                },
                                "Edit"
                            }
                            // MAPPS-436: the button only opens the dialog; the
                            // DELETE fires from its onconfirm.
                            Button {
                                variant: ButtonVariant::Link,
                                size: ButtonSize::Small,
                                class: "text-red-600 dark:text-red-400".to_string(),
                                disabled: !can_mutate,
                                onclick: move |_| {
                                    delete_error.set(String::new());
                                    confirming_delete.set(true);
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The editor plus a send button; a root composer shows the author's disc,
/// a reply composer sits inside the thread and can be cancelled.
#[component]
pub(crate) fn CommentComposer(
    article_id: String,
    parent_id: Option<uuid::Uuid>,
    /// MAPPS-744: the passage a root is about (PMS-1130).
    anchor: Option<serde_json::Value>,
    author_name: String,
    author_avatar: Option<String>,
    can_mutate: bool,
    people: Vec<crate::utils::mentions::Mention>,
    placeholder: String,
    submit_label: String,
    on_posted: EventHandler<()>,
    on_cancel: Option<EventHandler<()>>,
) -> Element {
    let mut text = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let submit = use_callback(move |_: ()| {
        let body = text().trim().to_string();
        if body.is_empty() || busy() || !can_mutate {
            return;
        }
        busy.set(true);
        error.set(String::new());
        let id = article_id.clone();
        let anchor = anchor.clone();
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("/kb/articles/{id}/comments");
                let req = CreateKbCommentRequest {
                    body,
                    parent_id,
                    anchor: anchor.clone(),
                };
                match crate::hooks::fetch::api::post_authed_typed::<KbComment, _>(&path, &req).await
                {
                    Ok(_) => {
                        text.set(String::new());
                        on_posted.call(());
                    }
                    Err(e) => error.set(format!("Could not post: {}", e.user_message())),
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = (&id, &body, parent_id, &anchor);
            busy.set(false);
        });
    });
    let show_avatar = !author_name.is_empty();
    rsx! {
        div { class: "flex gap-3",
            if show_avatar {
                Avatar { name: author_name.clone(), url: author_avatar.clone() }
            }
            div {
                class: "min-w-0 flex-1 space-y-2",
                // Ctrl/Cmd+Enter sends; the keydown bubbles up from the
                // editor's textarea.
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter && (e.modifiers().ctrl() || e.modifiers().meta()) {
                        e.prevent_default();
                        submit.call(());
                    }
                },
                if !error().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-300", "{error}" }
                }
                crate::components::MarkdownEditor {
                    name: "kb_comment".to_string(),
                    label: "Comment".to_string(),
                    label_hidden: true,
                    placeholder: placeholder.clone(),
                    rows: 3,
                    disabled: !can_mutate,
                    value: text(),
                    people: people.clone(),
                    oninput: move |next: String| text.set(next),
                }
                div { class: "flex items-center gap-2",
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Small,
                        loading: busy(),
                        disabled: busy() || text().trim().is_empty() || !can_mutate,
                        title: (!can_mutate).then(|| "Can't comment while the server is unreachable".to_string()),
                        onclick: move |_| submit.call(()),
                        "{submit_label}"
                    }
                    if let Some(cancel) = on_cancel {
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: move |_| cancel.call(()),
                            "Cancel"
                        }
                    }
                    span { class: "text-xs text-subtle", "Ctrl+Enter to send" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn at(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 8, h, 0, 0).unwrap()
    }

    fn comment(h: u32, resolved: bool) -> KbComment {
        KbComment {
            id: uuid::Uuid::new_v4(),
            article_id: uuid::Uuid::new_v4(),
            parent_id: None,
            author_id: uuid::Uuid::new_v4(),
            author_name: "Ada".into(),
            author_avatar_url: None,
            body: "hi".into(),
            anchor: None,
            anchor_version: None,
            created_at: at(h),
            edited_at: None,
            resolved_at: resolved.then(|| at(h + 1)),
            resolved_by_name: resolved.then(|| "Grace".to_string()),
            deleted: false,
            replies: Vec::new(),
        }
    }

    fn version(n: i32, h: u32) -> KbArticleVersion {
        KbArticleVersion {
            id: uuid::Uuid::new_v4(),
            article_id: uuid::Uuid::new_v4(),
            version_number: n,
            title: "t".into(),
            content: format!("body {n}"),
            edited_by_name: Some("Ada".into()),
            change_note: None,
            change_kind: if n == 1 {
                "create".into()
            } else {
                "edit".into()
            },
            restored_from_version: None,
            created_at: Some(at(h)),
        }
    }

    /// Oldest first across both kinds, each version paired with the one
    /// before it, resolved threads folded away unless asked for.
    #[test]
    fn the_stream_merges_by_time_and_pairs_each_version_with_its_predecessor() {
        let comments = vec![comment(10, false), comment(13, true)];
        let versions = vec![version(2, 12), version(1, 9)];
        let all = build_stream(&comments, &versions, Tab::All, true);
        let order: Vec<u32> = all.iter().map(|e| e.at().hour()).collect();
        assert_eq!(order, [9, 10, 12, 13]);
        match &all[2] {
            Entry::Version { version, previous } => {
                assert_eq!(version.version_number, 2);
                assert_eq!(previous.as_ref().map(|p| p.version_number), Some(1));
            }
            other => panic!("expected v2: {other:?}"),
        }
        match &all[0] {
            Entry::Version { previous, .. } => {
                assert!(previous.is_none(), "v1 has nothing before it")
            }
            other => panic!("expected v1: {other:?}"),
        }
        let hidden = build_stream(&comments, &versions, Tab::All, false);
        assert_eq!(hidden.len(), 3, "the resolved thread is folded away");
        assert!(build_stream(&comments, &versions, Tab::Comments, true)
            .iter()
            .all(|e| matches!(e, Entry::Thread(_))));
        assert!(build_stream(&comments, &versions, Tab::History, true)
            .iter()
            .all(|e| matches!(e, Entry::Version { .. })));
    }

    #[test]
    fn the_tabs_wrap_and_remember_by_key() {
        assert_eq!(Tab::All.step(true), Tab::Comments);
        assert_eq!(Tab::History.step(true), Tab::All);
        assert_eq!(Tab::All.step(false), Tab::History);
        for t in Tab::ALL {
            assert_eq!(Tab::from_key(t.key()), t);
        }
        assert_eq!(Tab::from_key("garbage"), Tab::All);
    }

    /// The composer says what it is for, a body renders through the shared
    /// renderer and never a second path, the tabs are a real tablist, and
    /// the stream mounts on a staff session only.
    #[test]
    fn the_stream_uses_the_shared_renderer_and_mounts_for_staff_only() {
        let src = include_str!("kb_activity.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("placeholder: \"Write a comment, @mention people\".to_string(),"));
        assert!(head.contains("crate::components::Markdown { content: comment.body.clone() }"));
        assert!(
            !head.contains("dangerous_inner_html"),
            "no second render path"
        );
        assert!(head.contains("role: \"tablist\","));
        assert!(head.contains("Key::ArrowRight => Some(tab().step(true)),"));
        assert!(head
            .contains("e.key() == Key::Enter && (e.modifiers().ctrl() || e.modifiers().meta())"));
        let page = include_str!("knowledge_base.rs");
        assert!(
            page.contains("if !is_contact {\n                                crate::pages::kb_activity::ArticleActivity {"),
            "staff only"
        );
    }
}
