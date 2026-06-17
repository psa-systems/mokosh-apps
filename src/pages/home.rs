//! Home/landing page

use dioxus::prelude::*;

use chrono::Datelike;

use crate::Route;

/// Landing page component
#[component]
pub fn HomePage() -> Element {
    // Derive the copyright year at render so it never goes stale (MAPPS-229).
    let current_year = chrono::Utc::now().year();
    rsx! {
        div { class: "min-h-screen bg-gradient-to-br from-blue-600 to-indigo-900",
            // Navigation
            nav { class: "container mx-auto px-6 py-4",
                div { class: "flex items-center justify-between",
                    span { class: "text-2xl font-bold text-white", "Mokosh Platform" }
                    div { class: "flex items-center space-x-4",
                        Link {
                            to: Route::Login {},
                            class: "text-white hover:text-blue-200 transition-colors",
                            "Login"
                        }
                        Link {
                            to: Route::PortalHome {},
                            class: "bg-white text-blue-600 px-4 py-2 rounded-md font-medium hover:bg-blue-50 transition-colors", // theme-guard-allow: marketing hero CTA on brand gradient
                            "Client Portal"
                        }
                    }
                }
            }

            // Hero section
            div { class: "container mx-auto px-6 py-24",
                div { class: "max-w-3xl",
                    h1 { class: "text-5xl font-bold text-white mb-6",
                        "Professional Services Automation for Modern MSPs"
                    }
                    p { class: "text-xl text-blue-100 mb-8",
                        "Streamline your IT service business with our all-in-one platform. "
                        "Manage tickets, track time, bill clients, and deliver exceptional service."
                    }
                    div { class: "flex space-x-4",
                        Link {
                            to: Route::Login {},
                            class: "bg-white text-blue-600 px-6 py-3 rounded-md font-semibold hover:bg-blue-50 transition-colors", // theme-guard-allow: marketing hero CTA on brand gradient
                            "Get Started"
                        }
                        a {
                            href: "#features",
                            class: "border-2 border-white text-white px-6 py-3 rounded-md font-semibold hover:bg-white hover:text-blue-600 transition-colors", // theme-guard-allow: marketing hero CTA on brand gradient
                            "Learn More"
                        }
                    }
                }
            }

            // Features section
            section { id: "features", class: "bg-surface py-24",
                div { class: "container mx-auto px-6",
                    h2 { class: "text-3xl font-bold text-content text-center mb-12",
                        "Everything You Need to Run Your MSP"
                    }
                    div { class: "grid md:grid-cols-3 gap-8",
                        FeatureCard {
                            title: "Ticketing System",
                            description: "Full-featured service desk with email integration, automation rules, and SLA management.",
                            icon: "ticket",
                        }
                        FeatureCard {
                            title: "Time Tracking",
                            description: "Track billable and non-billable time with stopwatch timers, approvals, and timesheet management.",
                            icon: "clock",
                        }
                        FeatureCard {
                            title: "Project Management",
                            description: "Manage projects with tasks, milestones, Gantt charts, and team collaboration tools.",
                            icon: "folder",
                        }
                        FeatureCard {
                            title: "Contract Management",
                            description: "Handle T&M, fixed-price, and recurring contracts with automated billing.",
                            icon: "document",
                        }
                        FeatureCard {
                            title: "Asset Management",
                            description: "Track customer assets, configurations, and credentials with full audit trails.",
                            icon: "server",
                        }
                        FeatureCard {
                            title: "RMM Integration",
                            description: "Connect with Tactical RMM for endpoint monitoring and remote management.",
                            icon: "link",
                        }
                    }
                }
            }

            // CTA section
            section { class: "bg-surface-2 py-16",
                div { class: "container mx-auto px-6 text-center",
                    h2 { class: "text-3xl font-bold text-content mb-4",
                        "Ready to Transform Your MSP?"
                    }
                    p { class: "text-subtle mb-8 max-w-2xl mx-auto",
                        "Join hundreds of MSPs who have streamlined their operations with our platform."
                    }
                    Link {
                        to: Route::Login {},
                        class: "inline-block bg-blue-600 text-white px-8 py-4 rounded-md font-semibold hover:bg-blue-700 transition-colors",
                        "Start Free Trial"
                    }
                }
            }

            // Footer
            footer { class: "bg-surface-2 border-t border-line py-8",
                div { class: "container mx-auto px-6",
                    div { class: "flex flex-col md:flex-row items-center justify-between",
                        span { class: "text-subtle text-sm",
                            "© {current_year} Mokosh Platform. All rights reserved."
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct FeatureCardProps {
    title: String,
    description: String,
    icon: String,
}

#[component]
fn FeatureCard(props: FeatureCardProps) -> Element {
    rsx! {
        div { class: "bg-surface-2 rounded-lg p-6 hover:shadow-lg transition-shadow",
            div { class: "w-12 h-12 bg-blue-100 rounded-lg flex items-center justify-center mb-4",
                span { class: "text-2xl text-blue-600",
                    match props.icon.as_str() {
                        "ticket" => "🎫",
                        "clock" => "⏱️",
                        "folder" => "📁",
                        "document" => "📄",
                        "server" => "🖥️",
                        "link" => "🔗",
                        _ => "📋",
                    }
                }
            }
            h3 { class: "text-xl font-semibold text-content mb-2",
                "{props.title}"
            }
            p { class: "text-muted",
                "{props.description}"
            }
        }
    }
}
