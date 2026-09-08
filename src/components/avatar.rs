//! A person's disc (MAPPS-742).
//!
//! The ticket journal drew a generic icon in a tinted ring for every entry,
//! the profile page drew initials, and nothing rendered `avatar_url`. One
//! component now: the picture when there is one, the initials when there is
//! not, and the name in `title` either way so the picture is never the only
//! way to know who this is.

use dioxus::prelude::*;

/// The first letter of the first two words of `name`, upper-cased; "?" for
/// a name with no letters, which is what "Unknown" would otherwise abbreviate
/// to a misleading "U".
pub fn initials_of(name: &str) -> String {
    if name.trim().eq_ignore_ascii_case("unknown") {
        return "?".to_string();
    }
    let letters: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .flat_map(|c| c.to_uppercase())
        .collect();
    if letters.is_empty() {
        "?".to_string()
    } else {
        letters
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AvatarSize {
    #[default]
    Small,
    Medium,
}

impl AvatarSize {
    fn classes(self) -> &'static str {
        match self {
            AvatarSize::Small => "h-8 w-8 text-xs",
            AvatarSize::Medium => "h-10 w-10 text-sm",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct AvatarProps {
    /// The person's display name; the initials and the `title` come from it.
    pub name: String,
    /// Their picture, when they have one.
    #[props(default)]
    pub url: Option<String>,
    #[props(default)]
    pub size: AvatarSize,
    /// Extra classes on the disc (a ring, a margin).
    #[props(default)]
    pub class: String,
}

#[component]
pub fn Avatar(props: AvatarProps) -> Element {
    let size = props.size.classes();
    let initials = initials_of(&props.name);
    match props.url.as_deref().filter(|u| !u.trim().is_empty()) {
        Some(url) => rsx! {
            img {
                class: "{size} rounded-full object-cover shrink-0 {props.class}",
                src: "{url}",
                alt: "{props.name}",
                title: "{props.name}",
            }
        },
        None => rsx! {
            span {
                class: "{size} rounded-full bg-accent-100 dark:bg-accent-900 text-accent font-medium flex items-center justify-center shrink-0 select-none {props.class}",
                title: "{props.name}",
                "aria-label": "{props.name}",
                role: "img",
                "{initials}"
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::initials_of;

    #[test]
    fn two_initials_from_the_first_two_words_and_a_mark_for_nobody() {
        assert_eq!(initials_of("Ada Lovelace"), "AL");
        assert_eq!(initials_of("ada"), "A");
        assert_eq!(initials_of("  Grace Brewster Murray Hopper "), "GB");
        assert_eq!(initials_of("Unknown"), "?");
        assert_eq!(initials_of("  "), "?");
        assert_eq!(initials_of("\u{e9}lise dupont"), "\u{c9}D");
    }
}
