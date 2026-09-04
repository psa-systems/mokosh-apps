//! 404 Not Found page

use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant};
use crate::Route;

/// 404 Not Found page
#[derive(Props, Clone, PartialEq)]
pub struct NotFoundPageProps {
    pub route: Vec<String>,
}

#[component]
pub fn NotFoundPage(props: NotFoundPageProps) -> Element {
    let path = props.route.join("/");

    rsx! {
        div { class: "min-h-screen flex items-center justify-center bg-app px-4",
            div { class: "max-w-lg w-full text-center",
                // 404 graphic
                div { class: "mb-8",
                    h1 { class: "text-9xl font-bold text-accent", "404" }
                }

                // Message
                h2 { class: "text-2xl font-bold text-content mb-4",
                    "Page not found"
                }
                p { class: "text-muted mb-8",
                    "Sorry, we couldn't find the page you're looking for."
                }

                // Debug info (only shown in development)
                if !path.is_empty() {
                    p { class: "text-sm text-subtle mb-8 font-mono",
                        "/{path}"
                    }
                }

                // Actions
                div { class: "flex flex-col sm:flex-row items-center justify-center gap-4",
                    Link {
                        to: Route::Dashboard {},
                        Button { variant: ButtonVariant::Primary,
                            "Go to Dashboard"
                        }
                    }
                    Link {
                        to: Route::Home {},
                        Button { variant: ButtonVariant::Secondary,
                            "Go to Home"
                        }
                    }
                    // mokosh-contact-login: "Go to Client Portal" link retired
                    // with the /portal/* route family (prompt 001).
                }

                // Help text
                p { class: "mt-8 text-sm text-muted",
                    "Need help? "
                    Link {
                        to: Route::KBHome {},
                        class: "text-accent hover:opacity-90",
                        "Browse the Knowledge Base"
                    }
                }
            }
        }
    }
}
