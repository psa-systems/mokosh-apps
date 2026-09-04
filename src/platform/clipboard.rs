//! Reading a pasted image out of the clipboard (MAPPS-588).
//!
//! This exists because dioxus cannot do it. `DragData` implements
//! `HasFileData`, so a drop hands over files through an ordinary handler with
//! no `web_sys` anywhere; `ClipboardData` carries only the event, so a paste
//! has to be read from a real `paste` listener over `clipboardData.items`.
//! Under MAPPS-504 that listener belongs here rather than in a page.
//!
//! Two rules govern everything below, and both were learned the hard way.
//!
//! The callback is an [`EventHandler`], never a bare closure, and the signature
//! makes that the only option a caller has. A raw DOM listener runs with no
//! dioxus scope on the stack; anything that touches runtime state from one
//! panics inside `Runtime::current_scope_id`, and because release builds are
//! `panic = "abort"` that panic leaks a borrow and kills every later render in
//! the page. That is MAPPS-586, and it was a bare closure in exactly this
//! position. `EventHandler::call` pushes its origin scope first.
//!
//! A paste that is not an image is left completely alone. `preventDefault` is
//! called only once an image has been found, so pasting text into the body
//! still does what the browser does, which is the behaviour every author uses
//! a hundred times more often than this one.

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::EventHandler;

/// Marks an element whose listener is already installed, so a second call is a
/// no-op rather than a second listener. The closure below is `forget`-ed and
/// cannot be removed, so installing twice would upload a pasted image twice.
#[cfg(target_arch = "wasm32")]
const INSTALLED_ATTR: &str = "data-paste-listener";

/// Call `on_file` with `(file_name, mime, bytes)` when an image is pasted into
/// the element with this `id`. Installing twice is a no-op.
///
/// Silently does nothing when the element is not in the document yet; the
/// caller installs from an effect, which runs after the first render.
#[cfg(target_arch = "wasm32")]
pub fn on_paste_image(id: &str, on_file: EventHandler<(String, String, Vec<u8>)>) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
    else {
        return;
    };
    if el.has_attribute(INSTALLED_ATTR) {
        return;
    }
    let _ = el.set_attribute(INSTALLED_ATTR, "1");

    let cb = Closure::wrap(Box::new(move |evt: web_sys::Event| {
        let Ok(evt) = evt.dyn_into::<web_sys::ClipboardEvent>() else {
            return;
        };
        let Some(file) = first_image(&evt) else {
            // Not an image. Leave the paste entirely alone.
            return;
        };
        // Only now: the browser must not also insert whatever it thinks the
        // clipboard's text form is next to the image we are about to add.
        evt.prevent_default();

        let name = file.name();
        let mime = file.type_();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(bytes) = read_bytes(&file).await {
                on_file.call((name, mime, bytes));
            }
        });
    }) as Box<dyn FnMut(web_sys::Event)>);

    if el
        .add_event_listener_with_callback("paste", cb.as_ref().unchecked_ref())
        .is_err()
    {
        // Pasting would silently do nothing, which reads as the feature being
        // broken rather than absent.
        tracing::error!("could not attach the paste listener");
        let _ = el.remove_attribute(INSTALLED_ATTR);
        return;
    }
    // Lives as long as the element; the page unmount takes the DOM with it.
    cb.forget();
}

/// The first image on the clipboard, if there is one.
///
/// A screenshot arrives as one `file` item whose type is `image/png`. A copy
/// out of a document often carries several representations of the same thing
/// (an image AND its HTML AND its plain text), so the list is scanned rather
/// than only its first entry read.
#[cfg(target_arch = "wasm32")]
fn first_image(evt: &web_sys::ClipboardEvent) -> Option<web_sys::File> {
    let items = evt.clipboard_data()?.items();
    for i in 0..items.length() {
        let Some(item) = items.get(i) else { continue };
        if item.kind() != "file" || !item.type_().starts_with("image/") {
            continue;
        }
        if let Ok(Some(file)) = item.get_as_file() {
            return Some(file);
        }
    }
    None
}

/// A `File` is a `Blob`, and `array_buffer()` is the way to its bytes without
/// a second forgotten closure for a `FileReader`'s `load` event.
#[cfg(target_arch = "wasm32")]
async fn read_bytes(file: &web_sys::File) -> Option<Vec<u8>> {
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        // A `None` here silently drops the pasted image: nothing is uploaded
        // and nothing is said, so the reason has to reach the console.
        .inspect_err(|e| tracing::warn!("pasted image could not be read: {e:?}"))
        .ok()?;
    // `Uint8Array::new` takes the buffer as a `JsValue`, which is what the
    // promise already resolved to; there is nothing to downcast.
    Some(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// One pasted image, as the injected script posts it back.
///
/// Base64 rather than an array of bytes: a screenshot is megabytes, and a JSON
/// number per byte would cost about four times the transfer for the same image.
/// `FileReader` produces the standard alphabet, which is what decodes it below.
#[cfg(not(target_arch = "wasm32"))]
#[derive(serde::Deserialize)]
struct PastedImage {
    name: String,
    mime: String,
    data: String,
}

/// The desktop renderer runs the UI in a webview, so there is no clipboard to
/// read from in this address space. MAPPS-699: the script attaches the listener
/// from inside the webview instead and `dioxus.send`s each image back over the
/// `eval` channel, exactly as MAPPS-511 does for the markdown checkboxes; the
/// task below decodes it and calls the same handler the browser calls.
///
/// Both rules in the module header hold here too, in JavaScript. The callback
/// is still an [`EventHandler`], which is what pushes a dioxus scope before the
/// app's code runs (MAPPS-586), and `preventDefault` is called only once an
/// image has been found, so pasting text is left to the webview.
///
/// The install-once marker is the same `data-paste-listener` attribute, read
/// through `dataset` because the listener is created in the webview and cannot
/// be removed from here either.
#[cfg(not(target_arch = "wasm32"))]
pub fn on_paste_image(id: &str, on_file: dioxus::prelude::EventHandler<(String, String, Vec<u8>)>) {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    if !crate::platform::dom::in_runtime() {
        return;
    }
    let mut eval = dioxus::document::eval(&format!(
        "const el = document.getElementById({}); \
         if (!el) return 'missing'; \
         if (el.dataset.pasteListener) return 'installed'; \
         el.dataset.pasteListener = '1'; \
         el.addEventListener('paste', (e) => {{ \
            const items = (e.clipboardData && e.clipboardData.items) || []; \
            let file = null; \
            for (let i = 0; i < items.length; i++) {{ \
               const item = items[i]; \
               if (item.kind !== 'file' || !item.type.startsWith('image/')) continue; \
               const candidate = item.getAsFile(); \
               if (candidate) {{ file = candidate; break; }} \
            }} \
            if (!file) return; \
            e.preventDefault(); \
            const reader = new FileReader(); \
            reader.onload = () => dioxus.send({{ name: file.name || 'pasted-image', mime: file.type, data: String(reader.result).split(',')[1] || '' }}); \
            reader.onerror = () => console.error('the pasted image could not be read', reader.error); \
            reader.readAsDataURL(file); \
         }}); \
         return 'installed';",
        crate::platform::dom::js_string(id)
    ));
    dioxus::prelude::spawn(async move {
        loop {
            match eval.recv::<PastedImage>().await {
                Ok(image) => match STANDARD.decode(&image.data) {
                    Ok(bytes) => on_file.call((image.name, image.mime, bytes)),
                    // Nothing else would say why: the paste was intercepted, so
                    // the author sees neither their image nor their text.
                    Err(e) => tracing::error!("a pasted image arrived undecodable: {e}"),
                },
                Err(e) => {
                    // Pasting an image would silently do nothing again, which
                    // reads as the feature being broken rather than absent.
                    tracing::error!("stopped listening for pasted images: {e}");
                    return;
                }
            }
        }
    });
}

/// MAPPS-588: the three decisions in this module that no host test can drive.
///
/// The listener only exists under `wasm32` with a real DOM, so what is pinned
/// is the shape of the code rather than its behaviour. Each assertion quotes
/// the pattern it looks for, so the scan stops at this module or it would match
/// itself and pass with the feature removed.
#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("clipboard.rs");

    fn code_only() -> String {
        let end = SRC.find("mod tests").expect("this module is in this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The MAPPS-586 rule, in the one place it is easiest to get wrong again.
    ///
    /// A raw DOM listener has no dioxus scope. A bare closure that touches
    /// runtime state from one panics inside `Runtime::current_scope_id`, and
    /// release builds are `panic = "abort"`, so that panic leaks a borrow and
    /// every later render in the page dies. Taking an `EventHandler` in the
    /// signature is what makes the safe thing the only thing a caller can do.
    #[test]
    fn the_callback_carries_its_own_scope() {
        let code = code_only();
        assert!(
            code.contains("on_file: EventHandler<(String, String, Vec<u8>)>"),
            "the callback must be an EventHandler, which pushes its origin scope"
        );
        // The one boxed closure here is the wasm-bindgen `Closure` wrapper,
        // which is how a listener is registered at all. What must not appear is
        // a SECOND one carrying the callback out to the app: that is the shape
        // that crashed in MAPPS-586.
        assert_eq!(
            code.matches("Box<dyn").count(),
            1,
            "the only boxed closure is the listener wrapper; a boxed callback \
             out to the app is the MAPPS-586 crash"
        );
        assert!(
            code.contains("Box<dyn FnMut(web_sys::Event)>"),
            "and that one is the listener wrapper"
        );
        assert!(
            code.contains("on_file.call("),
            "and it must be invoked through that handler"
        );
    }

    /// A paste that is not an image must reach the browser untouched.
    ///
    /// `preventDefault` is called only after an image has been found. Calling
    /// it first, or unconditionally, would break pasting text into the body,
    /// which authors do a hundred times more often than pasting a screenshot.
    #[test]
    fn only_an_image_paste_is_intercepted() {
        let code = code_only();
        let at_prevent = code
            .find("evt.prevent_default();")
            .expect("the handler prevents the default for an image");
        let at_guard = code
            .find("let Some(file) = first_image(&evt) else")
            .expect("the handler looks for an image first");
        assert!(
            at_guard < at_prevent,
            "prevent_default must come AFTER the image is found, or a text \
             paste stops working"
        );
        assert_eq!(
            code.matches("prevent_default()").count(),
            1,
            "exactly one place decides to intercept"
        );
    }

    /// Installing twice would upload a pasted image twice.
    ///
    /// The closure is `forget`-ed and cannot be removed, so the guard has to be
    /// on the element rather than a handle this module keeps.
    #[test]
    fn installing_twice_is_a_no_op() {
        let code = code_only();
        assert!(
            code.contains("if el.has_attribute(INSTALLED_ATTR) { return; }"),
            "a second install must return before adding another listener"
        );
        assert!(
            code.contains("el.set_attribute(INSTALLED_ATTR"),
            "and must mark the element so the check above can see it"
        );
    }

    /// MAPPS-699: the desktop half, pinned in source the way
    /// `platform::dom::desktop_wiring_tests` pins the rest of the channel.
    ///
    /// It needs a webview to evaluate JavaScript in, so no host test can drive
    /// it. What can regress is the wiring, and this went missing once already
    /// as an empty function that said nothing about it.
    #[test]
    fn the_desktop_reads_the_paste_inside_the_webview() {
        let code = code_only();
        assert_eq!(
            code.matches("pub fn on_paste_image(").count(),
            2,
            "one implementation per target, and neither is an empty body"
        );
        assert!(
            code.contains("el.addEventListener('paste', (e) =>"),
            "the desktop cannot attach a listener from Rust, so the injected \
             script attaches it"
        );
        assert!(
            code.contains("reader.readAsDataURL(file);"),
            "and reads the image as base64, because a JSON number per byte \
             costs about four times the transfer"
        );
        assert!(
            code.contains("eval.recv::<PastedImage>().await"),
            "a spawned task loops on the channel, the MAPPS-511 shape"
        );
        assert!(
            code.contains("STANDARD.decode(&image.data)"),
            "and decodes it back to the bytes the shared upload path takes"
        );
    }

    /// The text-paste rule, in the desktop script as well as the browser one.
    ///
    /// `preventDefault` runs only after an image has been found here too.
    /// Intercepting every paste in the webview would break pasting text into
    /// the body, which is the far more common action.
    #[test]
    fn the_desktop_script_intercepts_only_an_image() {
        let code = code_only();
        let at_guard = code
            .find("if (!file) return;")
            .expect("the script looks for an image first");
        let at_prevent = code
            .find("e.preventDefault();")
            .expect("and prevents the default for one");
        assert!(
            at_guard < at_prevent,
            "a paste with no image must return before the default is prevented"
        );
        assert_eq!(
            code.matches("e.preventDefault();").count(),
            1,
            "exactly one place in the script decides to intercept"
        );
    }
}
