//! SVG icon components
//!
//! Using Heroicons (MIT licensed) for consistent iconography

use dioxus::prelude::*;

/// Icon size
#[derive(Clone, Copy, PartialEq, Default)]
pub enum IconSize {
    Small, // 16px
    #[default]
    Medium, // 20px
    Large, // 24px
}

impl IconSize {
    fn class(&self) -> &'static str {
        match self {
            IconSize::Small => "w-4 h-4",
            IconSize::Medium => "w-5 h-5",
            IconSize::Large => "w-6 h-6",
        }
    }
}

// Navigation icons

#[component]
pub fn SwatchIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M4.098 19.902a3.75 3.75 0 0 0 5.304 0l6.401-6.402M6.75 21A3.75 3.75 0 0 1 3 17.25V4.125C3 3.504 3.504 3 4.125 3h5.25c.621 0 1.125.504 1.125 1.125v4.072M6.75 21a3.75 3.75 0 0 0 3.75-3.75V8.197M6.75 21h13.125c.621 0 1.125-.504 1.125-1.125v-5.25c0-.621-.504-1.125-1.125-1.125h-4.072M10.5 8.197l2.88-2.88c.438-.439 1.15-.439 1.59 0l3.712 3.713c.44.44.44 1.152 0 1.59l-2.879 2.88M6.75 17.25h.008v.008H6.75v-.008Z",
            }
        }
    }
}

#[component]
pub fn HomeIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "m2.25 12 8.954-8.955c.44-.439 1.152-.439 1.591 0L21.75 12M4.5 9.75v10.125c0 .621.504 1.125 1.125 1.125H9.75v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21h4.125c.621 0 1.125-.504 1.125-1.125V9.75M8.25 21h8.25",
            }
        }
    }
}

#[component]
pub fn TicketIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M16.5 6v.75m0 3v.75m0 3v.75m0 3V18m-9-5.25h5.25M7.5 15h3M3.375 5.25c-.621 0-1.125.504-1.125 1.125v3.026a2.999 2.999 0 0 1 0 5.198v3.026c0 .621.504 1.125 1.125 1.125h17.25c.621 0 1.125-.504 1.125-1.125v-3.026a2.999 2.999 0 0 1 0-5.198V6.375c0-.621-.504-1.125-1.125-1.125H3.375Z",
            }
        }
    }
}

#[component]
pub fn ClockIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M12 6v6h4.5m4.5 0a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z",
            }
        }
    }
}

#[component]
pub fn FolderIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z",
            }
        }
    }
}

/// Path of [`UsersIcon`] (Heroicons `users`). Exported for the MAPPS-359
/// `distinct_icons` guard (Contacts vs Team).
pub const USERS_PATH: &str = "M15 19.128a9.38 9.38 0 0 0 2.625.372 9.337 9.337 0 0 0 4.121-.952 4.125 4.125 0 0 0-7.533-2.493M15 19.128v-.003c0-1.113-.285-2.16-.786-3.07M15 19.128v.106A12.318 12.318 0 0 1 8.624 21c-2.331 0-4.512-.645-6.374-1.766l-.001-.109a6.375 6.375 0 0 1 11.964-3.07M12 6.375a3.375 3.375 0 1 1-6.75 0 3.375 3.375 0 0 1 6.75 0Zm8.25 2.25a2.625 2.625 0 1 1-5.25 0 2.625 2.625 0 0 1 5.25 0Z";

#[component]
pub fn UsersIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: USERS_PATH,
            }
        }
    }
}

#[component]
pub fn BuildingIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M3.75 21h16.5M4.5 3h15M5.25 3v18m13.5-18v18M9 6.75h1.5m-1.5 3h1.5m-1.5 3h1.5m3-6H15m-1.5 3H15m-1.5 3H15M9 21v-3.375c0-.621.504-1.125 1.125-1.125h3.75c.621 0 1.125.504 1.125 1.125V21",
            }
        }
    }
}

#[component]
pub fn CalendarIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 0 1 2.25-2.25h13.5A2.25 2.25 0 0 1 21 7.5v11.25m-18 0A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75m-18 0v-7.5A2.25 2.25 0 0 1 5.25 9h13.5A2.25 2.25 0 0 1 21 11.25v7.5",
            }
        }
    }
}

/// Truck icon (Heroicons, MIT). Used for the Dispatch nav item so the
/// dispatch board is visually distinct from the Calendar (MAPPS-253), which
/// shares `CalendarIcon`.
#[component]
pub fn TruckIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M8.25 18.75a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m3 0h6m-9 0H3.375a1.125 1.125 0 0 1-1.125-1.125V14.25m17.25 4.5a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m3 0h1.125c.621 0 1.129-.504 1.09-1.124a17.902 17.902 0 0 0-3.213-9.193 2.056 2.056 0 0 0-1.58-.86H14.25M16.5 18.75h-2.25m0-11.177v-.958c0-.568-.422-1.048-.987-1.106a48.554 48.554 0 0 0-10.026 0 1.106 1.106 0 0 0-.987 1.106v7.635m12-6.677v6.677m0 4.5v-4.5m0 0h-12",
            }
        }
    }
}

/// Path of [`DocumentIcon`] (Heroicons `document-text`). Exported so the
/// sidebar's MAPPS-359 `distinct_icons` guard can prove the rows that used
/// to all share this glyph now render distinct icons.
pub const DOCUMENT_PATH: &str = "M19.5 14.25v-2.625a3.375 3.375 0 0 0-3.375-3.375h-1.5A1.125 1.125 0 0 1 13.5 7.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 0 0-9-9Z";

#[component]
pub fn DocumentIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: DOCUMENT_PATH,
            }
        }
    }
}

/// Path of [`CurrencyIcon`] (Heroicons `currency-dollar`). Exported for the
/// MAPPS-359 `distinct_icons` guard (Invoices vs Payments).
pub const CURRENCY_PATH: &str = "M12 6v12m-3-2.818.879.659c1.171.879 3.07.879 4.242 0 1.172-.879 1.172-2.303 0-3.182C13.536 12.219 12.768 12 12 12c-.725 0-1.45-.22-2.003-.659-1.106-.879-1.106-2.303 0-3.182s2.9-.879 4.006 0l.415.33M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z";

#[component]
pub fn CurrencyIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: CURRENCY_PATH,
            }
        }
    }
}

#[component]
pub fn ServerIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M21.75 17.25v-.228a4.5 4.5 0 0 0-.12-1.03l-2.268-9.64a3.375 3.375 0 0 0-3.285-2.602H7.923a3.375 3.375 0 0 0-3.285 2.602l-2.268 9.64a4.5 4.5 0 0 0-.12 1.03v.228m19.5 0a3 3 0 0 1-3 3H5.25a3 3 0 0 1-3-3m19.5 0a3 3 0 0 0-3-3H5.25a3 3 0 0 0-3 3m16.5 0h.008v.008h-.008v-.008Zm-3 0h.008v.008h-.008v-.008Z",
            }
        }
    }
}

#[component]
pub fn BookIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M12 6.042A8.967 8.967 0 0 0 6 3.75c-1.052 0-2.062.18-3 .512v14.25A8.987 8.987 0 0 1 6 18c2.305 0 4.408.867 6 2.292m0-14.25a8.966 8.966 0 0 1 6-2.292c1.052 0 2.062.18 3 .512v14.25A8.987 8.987 0 0 0 18 18a8.967 8.967 0 0 0-6 2.292m0-14.25v14.25",
            }
        }
    }
}

#[component]
pub fn ChartIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 0 1 3 19.875v-6.75ZM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V8.625ZM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 0 1-1.125-1.125V4.125Z",
            }
        }
    }
}

// MAPPS-359: distinct per-item nav icons. Before this, six sidebar rows
// reused `DocumentIcon` (Timesheets, Timesheet Approvals, Contracts, Rate
// Cards, Audit Log, SLA Management), two reused `UsersIcon` (Contacts,
// Team), and two reused `CurrencyIcon` (Invoices, Payments), so the rail
// read as three blocks of identical glyphs. Each of the icons below (all
// Heroicons v2 outline, MIT) disambiguates one of those rows. Every icon's
// primary path is exported as a `*_PATH` const so the sidebar's
// `distinct_icons` guard can assert every nav item renders a unique glyph
// from one source of truth.

/// Primary path of [`DocumentCheckIcon`] (Heroicons `document-check`).
pub const DOCUMENT_CHECK_PATH: &str = "M10.125 2.25h-4.5c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125v-9M10.125 2.25h.375a9 9 0 0 1 9 9v.375M10.125 2.25A3.375 3.375 0 0 1 13.5 5.625v1.5c0 .621.504 1.125 1.125 1.125h1.5a3.375 3.375 0 0 1 3.375 3.375M9 15l2.25 2.25L15 12";

/// Document with a check mark. Used for the Timesheet Approvals nav item so
/// the approvals queue is distinct from Timesheets (`DocumentIcon`).
#[component]
pub fn DocumentCheckIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: DOCUMENT_CHECK_PATH,
            }
        }
    }
}

/// Primary path of [`ScaleIcon`] (Heroicons `scale`).
pub const SCALE_PATH: &str = "M12 3v17.25m0 0c-1.472 0-2.882.265-4.185.75M12 20.25c1.472 0 2.882.265 4.185.75M18.75 4.97A48.416 48.416 0 0 0 12 4.5c-2.291 0-4.545.16-6.75.47m13.5 0c1.01.143 2.01.317 3 .52m-3-.52 2.62 10.726c.122.499-.106 1.028-.589 1.202a5.988 5.988 0 0 1-2.031.352 5.988 5.988 0 0 1-2.031-.352c-.483-.174-.711-.703-.59-1.202L18.75 4.971Zm-16.5.52c.99-.203 1.99-.377 3-.52m0 0 2.62 10.726c.122.499-.106 1.028-.589 1.202a5.989 5.989 0 0 1-2.031.352 5.989 5.989 0 0 1-2.031-.352c-.483-.174-.711-.703-.59-1.202L5.25 4.971Z";

/// Balance scale. Used for the Contracts nav item (legal agreements) so it
/// is distinct from Timesheets/other document rows.
#[component]
pub fn ScaleIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: SCALE_PATH,
            }
        }
    }
}

/// Primary path of [`TagIcon`] (Heroicons `tag`).
pub const TAG_PATH: &str = "M9.568 3H5.25A2.25 2.25 0 0 0 3 5.25v4.318c0 .597.237 1.17.659 1.591l9.581 9.581c.699.699 1.78.872 2.607.33a18.095 18.095 0 0 0 5.223-5.223c.542-.827.369-1.908-.33-2.607L11.16 3.66A2.25 2.25 0 0 0 9.568 3Z";

/// Price tag. Used for the Rate Cards nav item (pricing) so it is distinct
/// from Contracts and the other billing document rows.
#[component]
pub fn TagIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: TAG_PATH,
            }
            // Punch-hole of the tag.
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M6 6h.008v.008H6V6Z",
            }
        }
    }
}

/// Primary path of [`CreditCardIcon`] (Heroicons `credit-card`).
pub const CREDIT_CARD_PATH: &str = "M2.25 8.25h19.5M2.25 9h19.5m-16.5 5.25h6m-6 2.25h3m-3.75 3h15a2.25 2.25 0 0 0 2.25-2.25V6.75A2.25 2.25 0 0 0 19.5 4.5h-15a2.25 2.25 0 0 0-2.25 2.25v10.5A2.25 2.25 0 0 0 4.5 19.5Z";

/// Credit card. Used for the Payments nav item so it is distinct from
/// Invoices (`CurrencyIcon`).
#[component]
pub fn CreditCardIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: CREDIT_CARD_PATH,
            }
        }
    }
}

/// Primary path of [`UserGroupIcon`] (Heroicons `user-group`).
pub const USER_GROUP_PATH: &str = "M18 18.72a9.094 9.094 0 0 0 3.741-.479 3 3 0 0 0-4.682-2.72m.94 3.198.001.031c0 .225-.012.447-.037.666A11.944 11.944 0 0 1 12 21c-2.17 0-4.207-.576-5.963-1.584A6.062 6.062 0 0 1 6 18.719m12 0a5.971 5.971 0 0 0-.941-3.197m0 0A5.995 5.995 0 0 0 12 12.75a5.995 5.995 0 0 0-5.058 2.772m0 0a3 3 0 0 0-4.681 2.72 8.986 8.986 0 0 0 3.74.477m.94-3.197a5.971 5.971 0 0 0-.94 3.197M15 6.75a3 3 0 1 1-6 0 3 3 0 0 1 6 0Zm6 3a2.25 2.25 0 1 1-4.5 0 2.25 2.25 0 0 1 4.5 0Zm-13.5 0a2.25 2.25 0 1 1-4.5 0 2.25 2.25 0 0 1 4.5 0Z";

/// Group of people. Used for the Team nav item so it is distinct from
/// Contacts (`UsersIcon`).
#[component]
pub fn UserGroupIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: USER_GROUP_PATH,
            }
        }
    }
}

/// Primary path of [`ClipboardDocumentListIcon`] (Heroicons
/// `clipboard-document-list`).
pub const CLIPBOARD_DOCUMENT_LIST_PATH: &str = "M9 12h3.75M9 15h3.75M9 18h3.75m3 .75H18a2.25 2.25 0 0 0 2.25-2.25V6.108c0-1.135-.845-2.098-1.976-2.192a48.424 48.424 0 0 0-1.123-.08m-5.801 0c-.065.21-.1.433-.1.664 0 .414.336.75.75.75h4.5a.75.75 0 0 0 .75-.75 2.25 2.25 0 0 0-.1-.664m-5.8 0A2.251 2.251 0 0 1 13.5 2.25H15c1.012 0 1.867.668 2.15 1.586m-5.8 0c-.376.023-.75.05-1.124.08C9.095 4.01 8.25 4.973 8.25 6.108V8.25m0 0H4.875c-.621 0-1.125.504-1.125 1.125v11.25c0 .621.504 1.125 1.125 1.125h9.75c.621 0 1.125-.504 1.125-1.125V9.375c0-.621-.504-1.125-1.125-1.125H8.25ZM6.75 12h.008v.008H6.75V12Zm0 3h.008v.008H6.75V15Zm0 3h.008v.008H6.75V18Z";

/// Clipboard with a document list. Used for the Audit Log nav item so it is
/// distinct from the plain document rows.
#[component]
pub fn ClipboardDocumentListIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: CLIPBOARD_DOCUMENT_LIST_PATH,
            }
        }
    }
}

/// Primary path of [`TableCellsIcon`]: a bordered grid. PMS-752: the Timesheets
/// nav row, which shared `DocumentIcon` with Quotes. A timesheet IS a grid of
/// days against hours, and a quote is genuinely a document, so the grid moved
/// and the page stayed.
pub const TABLE_CELLS_PATH: &str = "M4.5 5.25h15a.75.75 0 0 1 .75.75v12a.75.75 0 0 1-.75.75h-15a.75.75 0 0 1-.75-.75V6a.75.75 0 0 1 .75-.75Zm-.75 4.5h16.5m-16.5 4.5h16.5M10.5 5.25v13.5";

/// Bordered grid. Used for the Timesheets nav item.
#[component]
pub fn TableCellsIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: TABLE_CELLS_PATH,
            }
        }
    }
}

/// Primary path of [`InboxArrowDownIcon`]: an open tray with an arrow coming
/// into it. PMS-752: the Request Forms nav row, which used to share the
/// clipboard glyph with Audit Log. A request form is something you send out and
/// get answers back from, so "arriving in a tray" reads closer than a clipboard
/// and, more to the point, is not the icon the row above it uses.
pub const INBOX_ARROW_DOWN_PATH: &str =
    "M3.75 9.75v7.5A2.25 2.25 0 0 0 6 19.5h12a2.25 2.25 0 0 0 2.25-2.25v-7.5M12 3v10.5m0 0 3.75-3.75M12 13.5 8.25 9.75";

/// Tray with an inbound arrow. Used for the Request Forms nav item.
#[component]
pub fn InboxArrowDownIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: INBOX_ARROW_DOWN_PATH,
            }
        }
    }
}

/// Primary path of [`ShieldCheckIcon`] (Heroicons `shield-check`).
pub const SHIELD_CHECK_PATH: &str = "M9 12.75 11.25 15 15 9.75m-3-7.036A11.959 11.959 0 0 1 3.598 6 11.99 11.99 0 0 0 3 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285Z";

/// Shield with a check mark. Used for the SLA Management nav item (service
/// guarantees) so it is distinct from the plain document rows.
#[component]
pub fn ShieldCheckIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: SHIELD_CHECK_PATH,
            }
        }
    }
}

// Action icons

#[component]
pub fn PlusIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M12 4.5v15m7.5-7.5h-15",
            }
        }
    }
}

/// PMS-760: reordering an item in a list, as icon-only controls.
///
/// Added for the request-form builder, where "Move up" / "Move down" as text
/// links were most of the noise on a field row. Paired with [`ArrowDownIcon`];
/// use them through `IconButton`, which requires the accessible name that the
/// text used to carry.
#[component]
pub fn ArrowUpIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M4.5 10.5 12 3m0 0 7.5 7.5M12 3v18",
            }
        }
    }
}

/// See [`ArrowUpIcon`].
#[component]
pub fn ArrowDownIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M19.5 13.5 12 21m0 0-7.5-7.5M12 21V3",
            }
        }
    }
}

#[component]
pub fn PencilIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L6.832 19.82a4.5 4.5 0 0 1-1.897 1.13l-2.685.8.8-2.685a4.5 4.5 0 0 1 1.13-1.897L16.863 4.487Zm0 0L19.5 7.125",
            }
        }
    }
}

#[component]
pub fn TrashIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0",
            }
        }
    }
}

#[component]
pub fn XMarkIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M6 18 18 6M6 6l12 12",
            }
        }
    }
}

#[component]
pub fn CheckIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "m4.5 12.75 6 6 9-13.5",
            }
        }
    }
}

#[component]
pub fn ChevronDownIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "m19.5 8.25-7.5 7.5-7.5-7.5",
            }
        }
    }
}

#[component]
pub fn ChevronRightIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "m8.25 4.5 7.5 7.5-7.5 7.5",
            }
        }
    }
}

#[component]
pub fn BellIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M14.857 17.082a23.848 23.848 0 0 0 5.454-1.31A8.967 8.967 0 0 1 18 9.75V9A6 6 0 0 0 6 9v.75a8.967 8.967 0 0 1-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 0 1-5.714 0m5.714 0a3 3 0 1 1-5.714 0",
            }
        }
    }
}

#[component]
pub fn MenuIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5",
            }
        }
    }
}

#[component]
pub fn UserCircleIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M17.982 18.725A7.488 7.488 0 0 0 12 15.75a7.488 7.488 0 0 0-5.982 2.975m11.963 0a9 9 0 1 0-11.963 0m11.963 0A8.966 8.966 0 0 1 12 21a8.966 8.966 0 0 1-5.982-2.275M15 9.75a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
            }
        }
    }
}

// Status icons

#[component]
pub fn ExclamationIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z",
            }
        }
    }
}

#[component]
pub fn InformationIcon(
    #[props(default)] size: IconSize,
    #[props(default)] class: String,
) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "m11.25 11.25.041-.02a.75.75 0 0 1 1.063.852l-.708 2.836a.75.75 0 0 0 1.063.853l.041-.021M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9-3.75h.008v.008H12V8.25Z",
            }
        }
    }
}

/// Gear / cog. Heroicons `cog-6-tooth` (outline). Used for the Settings
/// nav entry (MAPPS-169).
#[component]
pub fn CogIcon(#[props(default)] size: IconSize, #[props(default)] class: String) -> Element {
    let size_class = size.class();
    let class = format!("{} {}", size_class, class);

    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.24-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281Z",
            }
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
            }
        }
    }
}
