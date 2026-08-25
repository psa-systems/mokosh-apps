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
                        // MAPPS-520: unified sign-in. The mokosh
                        // operator and the MSP tenant admin/user
                        // both sign in at `/login` (`Route::Login`);
                        // the page tries the platform credential
                        // first and falls back to the tenant
                        // credential on 401.
                        //
                        // MAPPS-572 (prompt 010): "Client portal"
                        // rejoins the nav as a secondary affordance
                        // beside the staff sign-in. It hops to the
                        // slug-less magic-link finder, which reveals
                        // nothing pre-auth about any specific Company
                        // or portal, so putting it back on the
                        // marketing landing does not leak the
                        // enumeration signal the MAPPS-520 collapse
                        // was worried about.
                        Link {
                            to: Route::ContactMagicLinkLogin { email: String::new() },
                            class: "text-white border border-white/60 px-4 py-2 rounded-md font-medium hover:bg-white/10 transition-colors", // theme-guard-allow: marketing hero CTA on brand gradient
                            "Client portal"
                        }
                        Link {
                            to: Route::Login {},
                            class: "bg-white text-blue-600 px-4 py-2 rounded-md font-medium hover:bg-blue-50 transition-colors", // theme-guard-allow: marketing hero CTA on brand gradient
                            "Sign in"
                        }
                    }
                }
            }

            // Hero section - two-column: copy + mascot (MAPPS-327)
            div { class: "container mx-auto px-6 py-16 lg:py-24",
                div { class: "grid items-center gap-12 lg:grid-cols-2",
                    // Copy + CTAs
                    div { class: "max-w-xl",
                        span { class: "mb-5 inline-flex items-center rounded-full bg-white/10 px-3 py-1 text-xs font-medium uppercase tracking-wide text-blue-100", // theme-guard-allow: marketing hero eyebrow on brand gradient
                            "Professional Services Automation"
                        }
                        h1 { class: "text-4xl sm:text-5xl lg:text-6xl font-bold text-white leading-tight",
                            "Run your whole MSP from one platform."
                        }
                        p { class: "mt-6 text-lg text-blue-100 leading-relaxed",
                            "Tickets, time, projects, contracts, billing, and assets - woven into a single service platform so nothing slips through the cracks."
                        }
                        div { class: "mt-8 flex flex-col sm:flex-row gap-4",
                            Link {
                                to: Route::Login {},
                                class: "bg-white text-blue-600 px-6 py-3 rounded-md font-semibold hover:bg-blue-50 transition-colors text-center", // theme-guard-allow: marketing hero CTA on brand gradient
                                "Get Started"
                            }
                            a {
                                href: "#features",
                                class: "border-2 border-white text-white px-6 py-3 rounded-md font-semibold hover:bg-white hover:text-blue-600 transition-colors text-center", // theme-guard-allow: marketing hero CTA on brand gradient
                                "Learn More"
                            }
                        }
                    }
                    // Mascot + tagline caption
                    div { class: "flex flex-col items-center",
                        img {
                            src: asset!("/assets/mokosh-hero.png"),
                            alt: "Mokosh, the weaver goddess, at her loom",
                            class: "w-full max-w-md h-auto drop-shadow-2xl",
                        }
                        p { class: "relative z-10 -mt-24 rounded-full border border-blue-100 bg-white px-5 py-2 text-sm italic text-blue-700 shadow-lg whitespace-nowrap", // theme-guard-allow: marketing hero tagline pill on brand gradient
                            "\"Weaves it all together.\""
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
