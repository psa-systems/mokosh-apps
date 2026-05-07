//! Home/landing page

use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant};
use crate::Route;

/// Landing page component
#[component]
pub fn HomePage() -> Element {
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
                            class: "bg-white text-blue-600 px-4 py-2 rounded-md font-medium hover:bg-blue-50 transition-colors",
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
                            class: "bg-white text-blue-600 px-6 py-3 rounded-md font-semibold hover:bg-blue-50 transition-colors",
                            "Get Started"
                        }
                        a {
                            href: "#features",
                            class: "border-2 border-white text-white px-6 py-3 rounded-md font-semibold hover:bg-white hover:text-blue-600 transition-colors",
                            "Learn More"
                        }
                    }
                }
            }

            // Features section
            section { id: "features", class: "bg-white py-24",
                div { class: "container mx-auto px-6",
                    h2 { class: "text-3xl font-bold text-gray-900 text-center mb-12",
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
            section { class: "bg-gray-900 py-16",
                div { class: "container mx-auto px-6 text-center",
                    h2 { class: "text-3xl font-bold text-white mb-4",
                        "Ready to Transform Your MSP?"
                    }
                    p { class: "text-gray-400 mb-8 max-w-2xl mx-auto",
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
            footer { class: "bg-gray-900 border-t border-gray-800 py-8",
                div { class: "container mx-auto px-6",
                    div { class: "flex flex-col md:flex-row items-center justify-between",
                        span { class: "text-gray-400 text-sm",
                            "© 2025 Mokosh Platform. All rights reserved."
                        }
                        div { class: "flex space-x-6 mt-4 md:mt-0",
                            a { class: "text-gray-400 hover:text-white text-sm", "Privacy Policy" }
                            a { class: "text-gray-400 hover:text-white text-sm", "Terms of Service" }
                            a { class: "text-gray-400 hover:text-white text-sm", "Contact" }
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
        div { class: "bg-gray-50 rounded-lg p-6 hover:shadow-lg transition-shadow",
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
            h3 { class: "text-xl font-semibold text-gray-900 mb-2",
                "{props.title}"
            }
            p { class: "text-gray-600",
                "{props.description}"
            }
        }
    }
}
