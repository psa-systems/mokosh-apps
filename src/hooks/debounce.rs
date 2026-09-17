//! Debounce a fast-changing text signal (MAPPS-855).
//!
//! Thirteen keystroke-driven fetch sites (six pickers, six list pages, the KB
//! tag view) read a text-input `Signal<String>` directly inside a
//! `use_resource` closure, so Dioxus subscribes the resource to every
//! keystroke and fires one HTTP request per character typed rather than one
//! per pause in typing. [`use_debounced_signal`] sits between the input and
//! the resource: the input's `oninput` still writes the raw signal
//! immediately, so the field itself stays responsive, but a `use_resource`
//! reads the DEBOUNCED signal this returns instead, so it only re-fetches
//! once the user stops typing for `ms` milliseconds.

use dioxus::prelude::*;

use crate::platform::timer::sleep_ms;

/// Debounce `source` by `ms` milliseconds.
///
/// Every change to `source` starts a fresh `ms`-long timer. A generation
/// counter, bumped each time `source` changes and re-checked after the
/// sleep, lets a timer detect that a newer keystroke arrived while it slept
/// and skip its write, so only the last keystroke in a burst ever lands on
/// the returned signal.
pub fn use_debounced_signal(source: Signal<String>, ms: u32) -> Signal<String> {
    let mut debounced = use_signal(move || source.peek().clone());
    let mut generation = use_signal(|| 0u64);

    use_effect(move || {
        let value = source.read().clone();
        let this_gen = {
            let mut g = generation.write();
            *g += 1;
            *g
        };
        spawn(async move {
            sleep_ms(ms).await;
            if *generation.peek() == this_gen {
                debounced.set(value);
            }
        });
    });

    debounced
}

/// MAPPS-855: exercises [`use_debounced_signal`] against real `Signal`s
/// inside a bare `VirtualDom`, mirroring `mapps837_search_resets_page_tests`
/// in `pages/tickets.rs`.
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod mapps855_debounce_tests {
    use super::use_debounced_signal;
    use dioxus::dioxus_core::NoOpMutations;
    use dioxus::prelude::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    #[tokio::test]
    async fn a_burst_of_keystrokes_settles_on_the_last_value_once() {
        let history: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        let mut dom = VirtualDom::new_with_props(
            |history: Rc<RefCell<Vec<String>>>| {
                let mut source = use_signal(String::new);
                let debounced = use_debounced_signal(source, 30);

                let recorded = history.clone();
                use_effect(move || {
                    recorded.borrow_mut().push(debounced.read().clone());
                });

                use_future(move || async move {
                    for ch in ["w", "wi", "wid", "widg", "widge", "widget"] {
                        source.set(ch.to_string());
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                });

                rsx! {}
            },
            history.clone(),
        );

        dom.rebuild_in_place();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        while tokio::time::Instant::now() < deadline {
            tokio::select! {
                _ = dom.wait_for_work() => {}
                _ = tokio::time::sleep_until(deadline) => break,
            }
            dom.render_immediate(&mut NoOpMutations);
        }

        let recorded = history.borrow().clone();
        assert_eq!(
            recorded.last(),
            Some(&"widget".to_string()),
            "debounced signal must settle on the last keystroke's value, got {recorded:?}"
        );
        assert!(
            recorded.len() < 6,
            "debounced signal must not write once per keystroke (6 keystrokes fired), got {recorded:?}"
        );
    }
}
