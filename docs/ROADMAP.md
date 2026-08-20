# Roadmap

Where the plan lives. Durable narrative only: goals, sequencing, and the
reasoning behind the order. Every item links to its YouTrack issue, and the
status is read from YouTrack, never restated here.

## Desktop as a second target

`mokosh-apps` was a browser SPA that happened to be written in Rust. The UI is
Tailwind-styled HTML rendered by Dioxus, which a webview renders unchanged, so
the only thing standing between it and a desktop application was that the code
talked to the browser directly: 161 call sites across 34 files reached for
`web_sys`, `js_sys`, `gloo-net` and `gloo-timers` with nothing between them and
the host.

The sequence is deliberately "make the boundary exist, then move things across
it", not "port everything at once":

1. [MAPPS-504](https://niceguyit.myjetbrains.com/youtrack/issue/MAPPS-504) - the boundary and the
   target. Every host call moves behind `src/platform/`, split on
   `target_arch`; the wasm-only crates become wasm-only dependencies so an
   escaped browser call fails to compile rather than panicking at run time; and
   the app builds, runs and bundles as a native window. Sign-in uses the
   standalone username/password path, because it needs no redirect.
2. [MAPPS-505](https://niceguyit.myjetbrains.com/youtrack/issue/MAPPS-505) - OIDC sign-in on the
   desktop, via an RFC 8252 loopback redirect. Separate because it needs a
   redirect-URI registration on the Bunyip OP, which is somebody else's change
   in somebody else's repo, and the desktop target should not wait on it.
3. [MAPPS-511](https://niceguyit.myjetbrains.com/youtrack/issue/MAPPS-511) - the DOM-dependent UI
   behaviours that are inert on the desktop because reading a value back out of
   a webview is asynchronous. Sidebar scroll memory, modal focus return,
   markdown task-list toggling, live OS theme switching. Separate because it
   needs a second mechanism (a bidirectional `eval` channel) that nothing else
   in the desktop target depends on.
4. [MAPPS-506](https://niceguyit.myjetbrains.com/youtrack/issue/MAPPS-506) - confirm unsaved
   changes when the window is closed, the desktop counterpart to the browser's
   `beforeunload` prompt.
5. [MAPPS-507](https://niceguyit.myjetbrains.com/youtrack/issue/MAPPS-507) - rename the `web`
   cargo feature to `app`. Naming debt MAPPS-504 created and could not fix in
   the same change without touching 295 unrelated call sites.

Packaging beyond `just desktop-bundle` (CI artifacts, code signing,
auto-update) is not planned yet and has no issue. It should get one before any
of it is built.

See [desktop.md](desktop.md) for how the build works today.
