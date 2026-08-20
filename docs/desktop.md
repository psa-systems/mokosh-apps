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

The recipes pass `--no-default-features --features web,multi-tenant,desktop`.
The `web` feature is the app-runtime gate, not a browser gate, so it stays on;
dropping the default features is what turns the *web renderer* off so a native
binary does not link `dioxus-web` for nothing. Renaming `web` to `app` and
removing that wart is tracked in MAPPS-507.

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
  "oidc_issuer": "https://api.example.com"
}
```

The same file backs `crate::modules::runtime_config`, so every key the
production container injects into `window.__MOKOSH_CONFIG__` is settable here
by the same name.

mokosh-server has to accept the desktop client's requests through CORS; the
webview does not send a same-origin `Origin` header.

## Signing in

The desktop build signs in with the standalone username/password path
(MAPPS-368, `POST /api/v1/auth/login`), which is what a deployment with no OIDC
issuer configured already uses.

Browser-redirect OIDC does not work here yet: `start_login` has no origin to
build a `redirect_uri` from and no document to be redirected. It reports that
rather than failing quietly. The RFC 8252 loopback flow that fixes it is
MAPPS-505.

## The host boundary

Everything the app needs from its host lives in `src/platform/`, split on
`#[cfg(target_arch = "wasm32")]` rather than on a cargo feature:

| Module | Browser | Desktop |
| --- | --- | --- |
| `http` | `gloo-net` (`fetch`) | `reqwest` (rustls) |
| `store` | `sessionStorage` | in-process map, session-lifetime |
| `prefs` | `localStorage` | JSON file in the config directory |
| `config` | `window.__MOKOSH_CONFIG__` | `config.json` + `MOKOSH_*` env |
| `location` | `window.location` | no URL; readers answer `None` |
| `dom` | `web-sys` on the document | JavaScript evaluated in the webview |
| `download` | Blob + synthesized anchor | writes to the downloads directory |
| `timer` | `gloo-timers` | `tokio::time` |
| `tz` | `Intl.DateTimeFormat` | `iana-time-zone` |
| `clock` | `chrono` (`wasmbind`) | `chrono` |

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

## Known gaps

- Closing the window with unsaved changes does not prompt: MAPPS-506.
- Sidebar scroll memory, modal focus return, markdown task-list toggling, and
  live OS theme switching are inert: MAPPS-511.
- OIDC sign-in: MAPPS-505.
