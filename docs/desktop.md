# The desktop build

`mokosh-apps` builds as a native desktop application from the same source tree
as the web SPA. The UI is identical because it is the same components; what
changes is the host underneath them.

Added in MAPPS-504.

## Prerequisites

Beyond the [README](../README.md) prerequisites, the desktop build needs
`dioxus-cli` on the host (`cargo install dioxus-cli`) and a system webview.

On Linux:

```nu
# openSUSE
sudo zypper install webkit2gtk3-soup2-devel gtk3-devel libsoup-devel

# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev
```

The `pkg-config` names the build looks for are `webkit2gtk-4.1`, `gtk+-3.0`,
`libsoup-3.0` and `javascriptcoregtk-4.1`. macOS and Windows use the webview
that ships with the OS and need nothing installed.

## Building and running

```nu
just desktop-run       # build and run, with hot reload
just desktop-build     # build the binary only
just desktop-bundle    # build an installable bundle for this platform
just check-desktop     # type-check the desktop build (part of `just check`)
```

`just desktop-bundle` reads the `[bundle]` block in `Dioxus.toml` (identifier,
publisher, and the icons under `assets/icons/`).

The recipes pass `--no-default-features --features desktop,multi-tenant`.
`desktop` pulls in the `app` feature, which is the app-runtime gate rather than
a platform gate; dropping the default features is what turns the *web renderer*
off so a native binary does not link `dioxus-web` for nothing.

## Pointing it at a server

A browser tab gets its API base from its own origin. A desktop window has no
origin, so it has to be told. Resolution order, highest first:

1. A `MOKOSH_<FIELD>` environment variable, e.g. `MOKOSH_API_BASE`.
2. The matching key in `config.json` in the per-user config directory
   (`~/.config/mokosh-apps/config.json` on Linux, the platform equivalent
   elsewhere).
3. `option_env!("MOKOSH_API_BASE")` baked in at compile time.
4. `http://localhost:8080/api/v1`, which is right for development against a
   local mokosh-server and wrong everywhere else.

```json
{
  "api_base": "https://api.msp.example.com/api/v1",
  "hub_base_url": "https://example.com",
  "oidc_issuer": "https://api.example.com",
  "brand_name": "PSA Systems",
  "brand_logo_url": "https://example.com/branding/logo.svg"
}
```

The same file backs `crate::modules::runtime_config`, so every key the
production container injects into `window.__MOKOSH_CONFIG__` is settable here
by the same name. That includes the MAPPS-509 branding keys (`brand_name`,
`brand_logo_url`, `brand_hero_url`), so a desktop install brands itself the
same way a hosted deployment does, down to the OS window title. See
[deployment-branding.md](deployment-branding.md) for what each key changes;
the container-side half of that document (Caddy, `entrypoint.sh`,
`index.html`) does not apply here.

mokosh-server has to accept the desktop client's requests through CORS; the
webview does not send a same-origin `Origin` header.

## Signing in

With no OIDC issuer configured, the desktop build signs in with the standalone
username/password path (MAPPS-368, `POST /api/v1/auth/login`), the same as any
other deployment that has no OP.

With an issuer configured (`oidc_issuer`, see above), it signs in against that
OP using the RFC 8252 native-app flow (MAPPS-505). A desktop window has no
origin, so there is no `redirect_uri` to hand the OP and no document to
redirect. Instead:

1. The app binds an ephemeral port on `127.0.0.1` and uses
   `http://127.0.0.1:<port>/auth/callback` as the `redirect_uri`
   (`src/platform/loopback.rs`).
2. `<issuer>/oauth2/authorize` opens in the user's own browser, not the app's
   webview. RFC 8252 section 8.12 rules out an embedded user-agent: the app
   could read the credentials typed into it, and the user cannot check the URL
   bar to see who is asking. The real browser also means an existing OP session
   applies.
3. The listener serves exactly one request, the OP's redirect, answers it with
   a "you can close this window" page, and releases the port. An abandoned
   sign-in times it out after five minutes rather than holding the socket.
4. `code` and `state` go to the same `/auth/callback` route the browser build
   uses, which runs the same exchange. PKCE, the `state` check, the pending-flow
   expiry, the `nonce` binding and the error classification are shared code, not
   a second implementation.

The OP has to allow a loopback redirect URI for this client. RFC 8252 section
7.3 requires it to accept any port on `127.0.0.1` for the registered URI,
because the port is chosen per flow at run time.

One behaviour differs from the browser. A callback failure that only means "no
live authorization flow here" is recovered in a browser by re-navigating to
`/login`; there is no URL to re-navigate here, so those cases render the error
screen instead of retrying silently.

## Closing the window

Closing the window with a dirty form asks first (MAPPS-506), the way a browser
tab does. The prompt is the app's own `ConfirmDialog`, not an OS message box,
so the wording matches every other discard-your-work confirmation.

The dirty flag is `hooks::unsaved_guard::UNSAVED_CHANGES`, published by
`use_unsaved_guard` and read by both hosts: `beforeunload` in the browser, and
`platform::window_close` here. There is one flag, so the two prompt on the same
condition.

The window is launched as `WindowCloseBehaviour::WindowHides`, because that is
the only answer `dioxus-desktop` has to a close request that does not destroy
the webview, and a destroyed webview has already lost the edits. The close
guard runs ahead of `dioxus-desktop` on each close request and picks: nothing
unsaved switches the window to `WindowCloses` and lets the request through (one
click, no prompt); unsaved changes leave it hidden-not-destroyed, raise the
modal, and re-show the window. Whether that re-show is visible as a blink has
not been checked on a real display: MAPPS-631.

## The host boundary

Everything the app needs from its host lives in `src/platform/`, split on
`#[cfg(target_arch = "wasm32")]` rather than on a cargo feature:

| Module | Browser | Desktop |
| --- | --- | --- |
| `http` | `gloo-net` (`fetch`) | `reqwest` (rustls) |
| `store` | `sessionStorage` | in-process map, session-lifetime |
| `prefs` | `localStorage` | JSON file in the config directory |
| `config` | `window.__MOKOSH_CONFIG__` | `config.json` + `MOKOSH_*` env |
| `location` | `window.location` | no URL bar; readers answer `None`, except `current_query`, which answers from the router |
| `loopback` | not used (the document redirects) | RFC 8252 listener for OIDC sign-in |
| `dom` | `web-sys` on the document | JavaScript evaluated in the webview |
| `log` | `console.error` | `tracing` |
| `download` | Blob + synthesized anchor | writes to the downloads directory |
| `timer` | `gloo-timers` | `tokio::time` |
| `tz` | `Intl.DateTimeFormat` | `iana-time-zone` |
| `clock` | `chrono` (`wasmbind`) | `chrono` |

Internal navigation goes through a router `Link` or the `Navigator`, never a raw
`a { href: "/..." }`. The webview's navigation handler refuses every
`dioxus://` target after the first load, and a relative `href` resolves to one,
so a raw anchor is a dead control here while it works in a browser (MAPPS-632,
MAPPS-683). A `Link` whose target carries a query keeps it verbatim, and
`location::current_query` reads it back off the router.

`web-sys`, `js-sys`, `gloo-net`, `gloo-timers` and `wasm-bindgen` are declared
only under `[target.'cfg(target_arch = "wasm32")'.dependencies]`. A browser call
that escapes `src/platform/` therefore fails to resolve on the desktop build,
instead of compiling into a binding that panics when it runs.

## Deliberate differences

These behave differently on the desktop on purpose, not by omission:

- **The update banner is inert.** It exists because an open browser tab is
  pinned to the bundle it loaded. A desktop binary is not served a bundle and
  updates through its installer.
- **The back-forward-cache guard is inert.** There is no bfcache without
  navigation away from a document.
- **The window is always "visible"** to the auth heartbeat, so it keeps polling
  while minimised. Erring the other way would mean a minimised window stops
  noticing that its account was deleted.
- **"Reload the app"** (after a data import) re-drives every subscribed
  resource instead of reloading a document.
- **Exports report their path.** The browser has a download shelf; here the app
  picks the destination, so it says where the file went.

## Reading from the webview

Writes into the document are one-way `eval` calls and need nothing back. Four
behaviours needed something back, and each of them was inert here until
MAPPS-511: sidebar scroll memory, modal focus return, markdown task-list
toggling, and live OS theme switching.

`dioxus::document::eval` is bidirectional, so all four go through the same
channel rather than one mechanism each (`src/platform/dom.rs`):

- **A value out of the webview** is an `async` read. `scroll_top_async` returns
  what the script returns, and the caller awaits it from the handler it already
  runs inside. The browser answers from the document it is already in.
- **An event into Rust** is the injected script attaching the listener and
  `dioxus.send`ing each occurrence, with a spawned task looping on `recv()`.
  That carries the markdown checkbox clicks (they live inside
  `dangerous_inner_html`, so they cannot carry a Dioxus handler) and the OS
  light/dark switch.

Focus is the exception: nothing is read. An async read of
`document.activeElement` would resolve after the dialog had already taken
focus, so `capture_focus` has the script park the element in the webview under
a token and `restore` focuses whatever is parked there.

The OS theme comes from the webview's own `prefers-color-scheme`, not from
tao's `WindowEvent::ThemeChanged`. tao 0.34 emits that event on macOS and
Windows only (`platform_impl/{macos,windows}`, verified in the vendored
source), so a Linux window would go on ignoring theme changes; the media query
is also exactly what the browser build listens to. The window's tao theme is
still what resolves `Theme::System` at boot, until the listener reports.

## Known gaps

Both need the same channel described above, and both are MAPPS-699. Their
comments in `src/` still name MAPPS-511 as the reason they are inert, which
that issue corrects.

- Pasting an image into the KB body does nothing
  (`src/platform/clipboard.rs`); pasting text works.
- The markdown editor's two panes scroll independently
  (`src/platform/scroll_sync.rs`).
