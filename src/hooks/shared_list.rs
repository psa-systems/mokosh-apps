//! One App-root cache per reference list (work types, task statuses, the
//! user roster, ...), fetched once a page asks for it and shared by every
//! page after that.
//!
//! MAPPS-1001: each list used to put its "a page asked" flag in context as a
//! bare `Signal<bool>`. Dioxus keys context by type, so every provider replaced
//! the one before it and every page set the last list's flag: only tax rates
//! ever loaded. [`Wanted`] is keyed by the row type, so two lists cannot share
//! a flag.

use std::future::Future;
use std::marker::PhantomData;

use dioxus::prelude::*;

/// "A page asked for the `T` list." One context entry per row type.
pub struct Wanted<T: 'static> {
    flag: Signal<bool>,
    _rows: PhantomData<fn() -> T>,
}

impl<T> Clone for Wanted<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Wanted<T> {}

/// Provide the shared `T` list at the App root. `what` names the list in the
/// log and the error toast, plural and lowercase: `"work types"`.
pub fn use_shared_list_provider<T>(what: &'static str, endpoint: &'static str)
where
    T: serde::de::DeserializeOwned + Clone + 'static,
{
    use_list_provider_with::<T, _, _>(move || fetch_list::<T>(what, endpoint));
}

/// [`use_shared_list_provider`] with the fetch passed in, so a test can count
/// requests without a server.
pub(crate) fn use_list_provider_with<T, F, Fut>(fetch: F)
where
    T: Clone + 'static,
    F: Fn() -> Fut + Copy + 'static,
    Fut: Future<Output = Vec<T>> + 'static,
{
    let flag = use_signal(|| false);
    use_context_provider(|| Wanted::<T> {
        flag,
        _rows: PhantomData,
    });

    let resource = use_resource(move || async move {
        // Read before any early return so Dioxus subscribes the resource to
        // it: signing in, a token refresh and an org switch all bump it.
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*flag.read() {
            return Vec::new();
        }
        if !signed_in() {
            // Not a failure: `set_access_token` bumps the generation read
            // above, which re-runs this once the session exists.
            tracing::debug!("reference list waits for sign-in before fetching");
            return Vec::new();
        }
        fetch().await
    });
    use_context_provider::<Resource<Vec<T>>>(|| resource);
}

/// Ask for the shared `T` list. `enabled` is the caller's own gate; `true`
/// from any one mount starts the fetch, and `false` never hides a list
/// another page already loaded.
pub fn use_shared_list<T: Clone + 'static>(enabled: bool) -> Resource<Vec<T>> {
    let Wanted { mut flag, .. } = use_context::<Wanted<T>>();
    if enabled && !*flag.read() {
        flag.set(true);
    }
    use_context::<Resource<Vec<T>>>()
}

#[cfg(feature = "app")]
fn signed_in() -> bool {
    crate::hooks::fetch::api::current_access_token().is_some()
}

#[cfg(not(feature = "app"))]
fn signed_in() -> bool {
    false
}

#[cfg(feature = "app")]
pub(crate) async fn fetch_list<T: serde::de::DeserializeOwned>(
    what: &'static str,
    endpoint: &str,
) -> Vec<T> {
    match crate::hooks::fetch::api::get_all_authed_typed::<T>(endpoint).await {
        Ok(rows) => {
            if rows.is_empty() {
                tracing::info!("{what} load succeeded and this tenant has none");
            }
            rows
        }
        // The endpoint refusing this role (a technician on `/auth/users`) is
        // an answer, not a fault: its pickers stay empty without a toast on
        // every page that role opens.
        Err(e) if e.status_code() == Some(403) => {
            tracing::warn!("{what} load refused for this role, its pickers stay empty: {e}");
            Vec::new()
        }
        Err(e) => {
            tracing::error!("{what} load failed, its pickers are empty: {e}");
            crate::hooks::push_toast(
                crate::components::AlertType::Error,
                format!("Could not load {what}: {}", e.user_message()),
            );
            Vec::new()
        }
    }
}

#[cfg(not(feature = "app"))]
pub(crate) async fn fetch_list<T>(_what: &'static str, _endpoint: &str) -> Vec<T> {
    Vec::new()
}

// Native only: `tokio` drives the VirtualDom here and has no wasm32 build.
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::dioxus_core::NoOpMutations;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Duration;

    async fn settle(dom: &mut VirtualDom) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
        while tokio::time::Instant::now() < deadline {
            tokio::select! {
                _ = dom.wait_for_work() => {}
                _ = tokio::time::sleep_until(deadline) => break,
            }
            dom.render_immediate(&mut NoOpMutations);
        }
    }

    /// The reference lists the App root provides, in `main.rs` order.
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum List {
        Roster,
        Mentions,
        WorkTypes,
        AssetTypes,
        TaskStatuses,
        KbCategories,
        PaymentTerms,
        TaxRates,
    }

    const LISTS: [List; 8] = [
        List::Roster,
        List::Mentions,
        List::WorkTypes,
        List::AssetTypes,
        List::TaskStatuses,
        List::KbCategories,
        List::PaymentTerms,
        List::TaxRates,
    ];

    fn ask(list: List) {
        match list {
            List::Roster => {
                crate::hooks::use_user_roster(true);
            }
            List::Mentions => {
                crate::hooks::use_mention_directory(true);
            }
            List::WorkTypes => {
                crate::hooks::use_work_types(true);
            }
            List::AssetTypes => {
                crate::hooks::use_asset_types(true);
            }
            List::TaskStatuses => {
                crate::hooks::use_task_statuses(true);
            }
            List::KbCategories => {
                crate::hooks::use_kb_categories(true);
            }
            List::PaymentTerms => {
                crate::hooks::use_payment_terms(true);
            }
            List::TaxRates => {
                crate::hooks::use_tax_rates(true);
            }
        }
    }

    type Seen = Rc<RefCell<Vec<(List, bool)>>>;

    #[derive(Props, Clone)]
    struct PageProps {
        asked: List,
        seen: Seen,
    }

    impl PartialEq for PageProps {
        fn eq(&self, other: &Self) -> bool {
            self.asked == other.asked && Rc::ptr_eq(&self.seen, &other.seen)
        }
    }

    #[allow(non_snake_case)]
    fn Page(props: PageProps) -> Element {
        ask(props.asked);
        *props.seen.borrow_mut() = flags();
        rsx! {}
    }

    fn flags() -> Vec<(List, bool)> {
        use crate::hooks::*;
        let read = |w: Signal<bool>| *w.peek();
        vec![
            (List::Roster, read(use_context::<Wanted<UserRow>>().flag)),
            (List::Mentions, read(mentions::directory_wanted_flag())),
            (
                List::WorkTypes,
                read(use_context::<Wanted<WorkTypeRow>>().flag),
            ),
            (
                List::AssetTypes,
                read(use_context::<Wanted<AssetTypeRow>>().flag),
            ),
            (
                List::TaskStatuses,
                read(use_context::<Wanted<TaskStatusRow>>().flag),
            ),
            (
                List::KbCategories,
                read(use_context::<Wanted<crate::modules::kb::KbCategory>>().flag),
            ),
            (
                List::PaymentTerms,
                read(use_context::<Wanted<PaymentTermRow>>().flag),
            ),
            (
                List::TaxRates,
                read(use_context::<Wanted<TaxRateRow>>().flag),
            ),
        ]
    }

    /// MAPPS-1001: with every provider mounted in one scope, as `main.rs`
    /// does, asking for one list sets that list's flag and no other.
    #[tokio::test]
    async fn each_list_has_its_own_wanted_flag_in_one_scope() {
        for asked in LISTS {
            let seen: Seen = Rc::default();
            let mut dom = VirtualDom::new_with_props(
                |(asked, seen): (List, Seen)| {
                    // MAPPS-1010: the roster provider now reads the auth
                    // context to choose its source endpoint, so this scope
                    // needs one, same as the real App root provides via
                    // `use_auth_provider`.
                    let auth_signal = use_signal(crate::hooks::auth::AuthContext::default);
                    use_context_provider(|| auth_signal);
                    crate::hooks::use_user_roster_provider();
                    crate::hooks::use_mention_directory_provider();
                    crate::hooks::use_work_types_provider();
                    crate::hooks::use_asset_types_provider();
                    crate::hooks::use_task_statuses_provider();
                    crate::hooks::use_kb_categories_provider();
                    crate::hooks::use_payment_terms_provider();
                    crate::hooks::use_tax_rates_provider();
                    rsx! { Page { asked, seen } }
                },
                (asked, seen.clone()),
            );
            dom.rebuild_in_place();
            settle(&mut dom).await;

            let set: Vec<List> = seen
                .borrow()
                .iter()
                .filter(|(_, on)| *on)
                .map(|(l, _)| *l)
                .collect();
            assert_eq!(set, vec![asked], "asking for {asked:?} set {set:?}");
        }
    }

    /// A list asked for before sign-in does not fetch, then fetches once the
    /// access token is set (MAPPS-1001: retried once authenticated).
    #[tokio::test]
    async fn a_list_asked_for_before_sign_in_fetches_once_signed_in() {
        thread_local! {
            static FETCHES: Cell<u32> = const { Cell::new(0) };
        }
        let rows_seen: Rc<Cell<usize>> = Rc::default();
        let fetches_before_sign_in: Rc<Cell<Option<u32>>> = Rc::default();

        crate::hooks::fetch::api::set_access_token_for_test(None);
        let mut dom = VirtualDom::new_with_props(
            |(rows_seen, before): (Rc<Cell<usize>>, Rc<Cell<Option<u32>>>)| {
                use_list_provider_with::<u8, _, _>(|| async {
                    FETCHES.with(|f| f.set(f.get() + 1));
                    vec![1, 2, 3]
                });
                let list = use_shared_list::<u8>(true);
                rows_seen.set(list.read().as_ref().map_or(0, Vec::len));
                use_future(move || {
                    let before = before.clone();
                    async move {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        before.set(Some(FETCHES.with(Cell::get)));
                        crate::hooks::fetch::api::set_access_token(Some("signed-in".into()));
                    }
                });
                rsx! {}
            },
            (rows_seen.clone(), fetches_before_sign_in.clone()),
        );
        dom.rebuild_in_place();
        settle(&mut dom).await;

        assert_eq!(
            fetches_before_sign_in.get(),
            Some(0),
            "no fetch before sign-in"
        );
        assert_eq!(FETCHES.with(Cell::get), 1, "one fetch once signed in");
        assert_eq!(rows_seen.get(), 3);
        crate::hooks::fetch::api::set_access_token_for_test(None);
    }
}
