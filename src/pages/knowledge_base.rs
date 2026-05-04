//! Knowledge base pages

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, BookIcon, Button, ButtonVariant, Card, DataTable, IconSize,
    PageHeader, PlusIcon, SearchInput, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::Route;

/// Knowledge base home page
#[component]
pub fn KBHomePage() -> Element {
    let mut search = use_signal(String::new);

    rsx! {
        AppLayout { title: "Knowledge Base",
            PageHeader {
                title: "Knowledge Base",
                subtitle: "Documentation and troubleshooting guides",
                actions: rsx! {
                    Link {
                        to: Route::KBArticleNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Article"
                        }
                    }
                },
            }

            // Search
            Card { class: "mb-6",
                SearchInput {
                    value: search.read().clone(),
                    placeholder: "Search articles...",
                    oninput: move |e: FormEvent| search.set(e.value()),
                }
            }

            // Categories
            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 mb-8",
                CategoryCard {
                    title: "Getting Started",
                    description: "New user guides and onboarding documentation",
                    article_count: 12,
                    icon: "book",
                }
                CategoryCard {
                    title: "Troubleshooting",
                    description: "Common issues and solutions",
                    article_count: 45,
                    icon: "wrench",
                }
                CategoryCard {
                    title: "How-To Guides",
                    description: "Step-by-step instructions",
                    article_count: 28,
                    icon: "list",
                }
                CategoryCard {
                    title: "Network",
                    description: "Networking documentation and guides",
                    article_count: 18,
                    icon: "network",
                }
                CategoryCard {
                    title: "Security",
                    description: "Security best practices and procedures",
                    article_count: 15,
                    icon: "shield",
                }
                CategoryCard {
                    title: "Microsoft 365",
                    description: "Office 365 and Azure documentation",
                    article_count: 32,
                    icon: "cloud",
                }
            }

            // Recent articles
            Card { title: "Recent Articles",
                div { class: "space-y-4",
                    ArticleItem {
                        id: "1",
                        title: "How to Reset a User's Password in Active Directory",
                        category: "How-To Guides",
                        updated: "2 hours ago",
                    }
                    ArticleItem {
                        id: "2",
                        title: "Troubleshooting VPN Connection Issues",
                        category: "Troubleshooting",
                        updated: "1 day ago",
                    }
                    ArticleItem {
                        id: "3",
                        title: "Setting Up Multi-Factor Authentication",
                        category: "Security",
                        updated: "2 days ago",
                    }
                    ArticleItem {
                        id: "4",
                        title: "Exchange Online Migration Checklist",
                        category: "Microsoft 365",
                        updated: "3 days ago",
                    }
                    ArticleItem {
                        id: "5",
                        title: "Configuring Firewall Rules for Remote Access",
                        category: "Network",
                        updated: "1 week ago",
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CategoryCardProps {
    title: String,
    description: String,
    article_count: u32,
    icon: String,
}

#[component]
fn CategoryCard(props: CategoryCardProps) -> Element {
    rsx! {
        Card { class: "hover:shadow-lg transition-shadow cursor-pointer",
            div { class: "flex items-start",
                div { class: "flex-shrink-0 w-10 h-10 bg-blue-100 dark:bg-blue-900 rounded-lg flex items-center justify-center",
                    BookIcon { class: "h-5 w-5 text-blue-600 dark:text-blue-400".to_string() }
                }
                div { class: "ml-4",
                    h3 { class: "text-lg font-medium text-gray-900 dark:text-white",
                        "{props.title}"
                    }
                    p { class: "text-sm text-gray-500 dark:text-gray-400 mt-1",
                        "{props.description}"
                    }
                    p { class: "text-sm text-blue-600 dark:text-blue-400 mt-2",
                        "{props.article_count} articles"
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ArticleItemProps {
    id: String,
    title: String,
    category: String,
    updated: String,
}

#[component]
fn ArticleItem(props: ArticleItemProps) -> Element {
    rsx! {
        Link {
            to: Route::KBArticleDetail { id: props.id.clone() },
            class: "block p-4 -mx-4 hover:bg-gray-50 dark:hover:bg-gray-800 rounded-lg transition-colors",
            div { class: "flex items-center justify-between",
                div {
                    h4 { class: "font-medium text-gray-900 dark:text-white",
                        "{props.title}"
                    }
                    p { class: "text-sm text-gray-500 dark:text-gray-400 mt-1",
                        "{props.category}"
                    }
                }
                span { class: "text-sm text-gray-400", "{props.updated}" }
            }
        }
    }
}

/// Article list page
#[component]
pub fn KBArticleListPage() -> Element {
    rsx! {
        AppLayout { title: "Articles",
            PageHeader {
                title: "All Articles",
                actions: rsx! {
                    Link {
                        to: Route::KBArticleNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Article"
                        }
                    }
                },
            }

            DataTable {
                total_items: 150,
                current_page: 1,
                per_page: 25,
                columns: 4,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Title" }
                            TableHeader { "Category" }
                            TableHeader { "Visibility" }
                            TableHeader { sortable: true, "Updated" }
                        }
                    }
                    TableBody {
                        TableRow { clickable: true,
                            TableCell {
                                Link {
                                    to: Route::KBArticleDetail { id: "1".to_string() },
                                    class: "font-medium text-blue-600 hover:text-blue-500",
                                    "How to Reset a User's Password in Active Directory"
                                }
                            }
                            TableCell { "How-To Guides" }
                            TableCell { Badge { variant: BadgeVariant::Blue, "Internal" } }
                            TableCell { class: "text-gray-500", "2 hours ago" }
                        }
                        TableRow { clickable: true,
                            TableCell {
                                Link {
                                    to: Route::KBArticleDetail { id: "2".to_string() },
                                    class: "font-medium text-blue-600 hover:text-blue-500",
                                    "Troubleshooting VPN Connection Issues"
                                }
                            }
                            TableCell { "Troubleshooting" }
                            TableCell { Badge { variant: BadgeVariant::Green, "Public" } }
                            TableCell { class: "text-gray-500", "1 day ago" }
                        }
                    }
                }
            }
        }
    }
}

/// New article page
#[component]
pub fn KBArticleNewPage() -> Element {
    rsx! {
        AppLayout { title: "New Article",
            PageHeader {
                title: "New Article",
                subtitle: "Create a new knowledge base article",
            }

            Card {
                form { class: "space-y-6",
                    p { class: "text-gray-500", "Article editor would go here (Markdown/WYSIWYG)." }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::KBHome {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button { variant: ButtonVariant::Secondary, "Save Draft" }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            "Publish"
                        }
                    }
                }
            }
        }
    }
}

/// Article detail page
#[derive(Props, Clone, PartialEq)]
pub struct KBArticleDetailPageProps {
    pub id: String,
}

#[component]
pub fn KBArticleDetailPage(props: KBArticleDetailPageProps) -> Element {
    rsx! {
        AppLayout { title: "Article",
            PageHeader {
                title: "How to Reset a User's Password in Active Directory",
                actions: rsx! {
                    Button { variant: ButtonVariant::Secondary, "Edit" }
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-4 gap-6",
                // Article content
                div { class: "lg:col-span-3",
                    Card {
                        article { class: "prose dark:prose-invert max-w-none",
                            p { class: "lead",
                                "This guide walks you through resetting a user's password in Active Directory using Active Directory Users and Computers (ADUC)."
                            }

                            h2 { "Prerequisites" }
                            ul {
                                li { "Domain Admin or delegated password reset permissions" }
                                li { "Access to Active Directory Users and Computers" }
                                li { "Network connectivity to domain controller" }
                            }

                            h2 { "Steps" }
                            ol {
                                li {
                                    strong { "Open Active Directory Users and Computers" }
                                    p { "Press Win+R, type 'dsa.msc', and press Enter." }
                                }
                                li {
                                    strong { "Locate the user account" }
                                    p { "Navigate to the appropriate OU or use Find (Ctrl+F) to search for the user." }
                                }
                                li {
                                    strong { "Reset the password" }
                                    p { "Right-click the user account and select 'Reset Password'. Enter the new password twice and click OK." }
                                }
                                li {
                                    strong { "Verify the reset" }
                                    p { "Have the user test logging in with the new password." }
                                }
                            }

                            h2 { "Troubleshooting" }
                            p {
                                "If the password reset fails, check that:"
                            }
                            ul {
                                li { "The new password meets complexity requirements" }
                                li { "The account is not locked out" }
                                li { "You have the necessary permissions" }
                            }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    Card { title: "Article Info",
                        dl { class: "space-y-4",
                            div {
                                dt { class: "text-sm text-gray-500", "Category" }
                                dd { class: "mt-1", "How-To Guides" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Visibility" }
                                dd { class: "mt-1", Badge { variant: BadgeVariant::Blue, "Internal" } }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Author" }
                                dd { class: "mt-1", "John Smith" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Created" }
                                dd { class: "mt-1", "Dec 1, 2024" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Updated" }
                                dd { class: "mt-1", "2 hours ago" }
                            }
                        }
                    }

                    Card { title: "Related Articles",
                        div { class: "space-y-2 text-sm",
                            a { href: "#", class: "block text-blue-600 hover:text-blue-500",
                                "Active Directory Best Practices"
                            }
                            a { href: "#", class: "block text-blue-600 hover:text-blue-500",
                                "Setting Up AD Password Policies"
                            }
                            a { href: "#", class: "block text-blue-600 hover:text-blue-500",
                                "Unlocking User Accounts"
                            }
                        }
                    }
                }
            }
        }
    }
}
