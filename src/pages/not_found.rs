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
        div { class: "min-h-screen flex items-center justify-center bg-gray-50 dark:bg-gray-900 px-4",
            div { class: "max-w-lg w-full text-center",
                // 404 graphic
                div { class: "mb-8",
                    h1 { class: "text-9xl font-bold text-blue-600 dark:text-blue-400", "404" }
                }

                // Message
                h2 { class: "text-2xl font-bold text-gray-900 dark:text-white mb-4",
                    "Page not found"
                }
                p { class: "text-gray-600 dark:text-gray-400 mb-8",
                    "Sorry, we couldn't find the page you're looking for."
                }

                // Debug info (only shown in development)
                if !path.is_empty() {
                    p { class: "text-sm text-gray-400 mb-8 font-mono",
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
                }

                // Help text
                p { class: "mt-8 text-sm text-gray-500 dark:text-gray-400",
                    "Need help? "
                    a {
                        href: "mailto:support@example.com",
                        class: "text-blue-600 hover:text-blue-500 dark:text-blue-400",
                        "Contact support"
                    }
                }
            }
        }
    }
}
