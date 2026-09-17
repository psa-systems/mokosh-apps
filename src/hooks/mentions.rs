//! The directory `@handle` resolves and completes against (MAPPS-592).
//!
//! Three places need the same list and each had fetched it for itself: the
//! renderer, to turn a handle into a chip; the KB editor, to complete one while
//! it is typed; and now the ticket description editor. The endpoint choice is
//! the part worth sharing rather than the request.
//!
//! MAPPS-860: sharing the endpoint choice was not sharing the fetch. Every
//! `Markdown` instance without an explicit `people` prop still ran its own
//! `use_resource`, so a page rendering many instances (a ticket's journal, a
//! KB article's comments) fired one request per instance despite a comment
//! here claiming otherwise. [`use_mention_directory`] is now backed by a
//! single App-root resource (see [`use_mention_directory_provider`]),
//! mirroring [`crate::hooks::user_roster`]: the first caller to pass
//! `enabled = true` triggers the one fetch, and every sibling instance,
//! including ones that mount later, reads the same cached result.

use dioxus::prelude::*;

use crate::utils::mentions::Mention;

/// Shared "has any consumer asked for the directory yet" flag, mirroring
/// [`crate::hooks::user_roster`]'s `RosterWanted`.
type DirectoryWanted = Signal<bool>;

/// Provide the shared mention-directory resource and its `enabled` flag at
/// the App root. Call once, alongside
/// [`crate::hooks::user_roster::use_user_roster_provider`].
pub fn use_mention_directory_provider() {
    let wanted = use_signal(|| false);
    use_context_provider::<DirectoryWanted>(|| wanted);

    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return None;
        }
        fetch_directory().await
    });
    use_context_provider::<Resource<Option<Vec<Mention>>>>(|| resource);
}

async fn fetch_directory() -> Option<Vec<Mention>> {
    #[cfg(feature = "app")]
    {
        #[derive(serde::Deserialize)]
        struct DirectoryEntry {
            id: uuid::Uuid,
            #[serde(default)]
            name: String,
            #[serde(default)]
            handle: String,
        }
        let rows = crate::hooks::fetch::api::get_all_authed::<DirectoryEntry>("/auth/directory")
            .await
            // Best-effort: the autocomplete offers no names and the
            // typed handle is submitted as written.
            .inspect_err(|e| tracing::warn!("mention directory load failed: {e}"))
            .ok()?;
        Some(
            rows.into_iter()
                .map(|u| Mention {
                    id: u.id.to_string(),
                    // The KB editor's copy fell back to the handle for a row
                    // with no name, and the renderer's did not. Keeping the
                    // fallback: a chip reading "@" and nothing else names
                    // nobody.
                    display: if u.name.trim().is_empty() {
                        u.handle.clone()
                    } else {
                        u.name
                    },
                    handle: u.handle,
                })
                .collect(),
        )
    }
    #[cfg(not(feature = "app"))]
    None
}

/// Everyone who can be mentioned, or `None` when the list could not be read.
///
/// `GET /auth/directory` (PMS-921), never `/auth/users`. The latter is
/// `RequireManager`, so a Technician got a 403 and saw every mention as plain
/// text, and got no completion at all: a KB article is written for technicians
/// and its mentions assign ownership, so the reader who most needed to know who
/// was named was the one who could not see it. The directory is `RequireAuth`
/// and returns id, name and handle only. It is also the only source carrying
/// `handle`, which is what resolution matches on.
///
/// A failure is not an error state anywhere it is used. It yields an empty
/// directory, which renders every `@` as the plain text it already was and
/// disables completion; a handle typed by hand still resolves at render time
/// for any reader whose own fetch succeeded. Nothing about a mention is worth
/// blocking a page over.
///
/// `enabled` is each caller's own gate (a surface that renders text of
/// unknown provenance and wants mentions off), unchanged from before
/// MAPPS-860. Passing `true` from any single mount is enough to trigger (and
/// thereafter share, via [`use_mention_directory_provider`]) the one
/// underlying fetch; passing `false` never blocks a directory another
/// consumer already cached.
pub fn use_mention_directory(enabled: bool) -> Resource<Option<Vec<Mention>>> {
    let mut wanted = use_context::<DirectoryWanted>();
    if enabled && !*wanted.read() {
        wanted.set(true);
    }
    use_context::<Resource<Option<Vec<Mention>>>>()
}

/// The list itself, flattened: a failed or still-running fetch is an empty
/// directory, which is the degrade every caller wants.
pub fn mention_people(directory: &Resource<Option<Vec<Mention>>>) -> Vec<Mention> {
    directory
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("mentions.rs");

    /// PMS-921: the unprivileged directory, never the manager-gated user list.
    /// Pinned here now that this is the only definition, so the rule cannot be
    /// lost by editing whichever copy a reader happened to open.
    #[test]
    fn the_directory_is_the_source_not_user_management() {
        let code = &SRC[..SRC.find("mod tests").expect("tests are in this file")];
        assert!(code.contains("\"/auth/directory\""));
        assert!(!code.contains("\"/auth/users\""));
    }

    /// Every caller degrades to an empty list rather than an error, so the
    /// flattening has to swallow both "failed" and "still loading".
    #[test]
    fn a_missing_directory_is_empty_not_an_error() {
        let code = &SRC[..SRC.find("mod tests").expect("tests are in this file")];
        assert!(code.contains(".flatten()"), "loading and failed collapse");
        assert!(code.contains(".unwrap_or_default()"), "into an empty list");
    }
}
