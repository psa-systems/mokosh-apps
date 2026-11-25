# Installation

Mokosh apps ships two ways: the `mokosh-www` container image for web deployments, and native desktop installers for Windows, macOS, and Linux. The UI, the data, and the sign-in flow are the same across all three.

This page is for operators choosing which target to deploy and installing it. End-user tour of the running app is in [The shell](shell.md).

## Web deployment

The container serves a WebAssembly bundle through Caddy. One image for staging, production, and every self-hosted deployment; the differences are environment variables on the container.

- Image: `dev.a8n.run/psa-systems-public/mokosh-www`
- Tags: `vX.Y.Z` for a pinned release, `latest` for the main branch
- Architectures: `linux/amd64` and `linux/arm64`; compose pulls the right one for the host kernel

The reference compose file lives in the repository at `oci-build/compose.example.yml`. Copy it, set the `MOKOSH_*` variables for the target deployment, and run `docker compose up --detach`. Full step-by-step is in [Self-hosting](self-hosting.md), including the TLS reverse proxy and the cookie-domain pairing with mokosh-server.

Pin a `vX.Y.Z` tag in production so rollouts are predictable. If the deployment runs more than one replica behind a load balancer, the rollout has an ordering constraint; see [Release and rollout](operations/release-and-rollout.md).

## Desktop installation

The desktop binary runs on Windows, macOS, and Linux. The UI is the same components rendered through the host OS's own webview; no Electron, no bundled browser.

### Windows and macOS

Download the installer for the platform from the release page, run it, and launch the app. The OS webview ships with the OS; nothing else is required on the system.

### Linux

Linux desktop builds use the system WebKitGTK and GTK libraries. Install these first:

```nu
# openSUSE
sudo zypper install webkit2gtk3-soup2-devel gtk3-devel libsoup-devel

# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev
```

The build looks for `webkit2gtk-4.1`, `gtk+-3.0`, `libsoup-3.0`, and `javascriptcoregtk-4.1` through `pkg-config`.

### Pointing the desktop app at a server

A browser tab inherits its API base from its own origin. A desktop window has no origin to inherit from, so the operator has to supply the configuration.

Resolution order, highest first:

1. A `MOKOSH_<FIELD>` environment variable in the shell that launched the app, for example `MOKOSH_API_BASE`.
2. The matching key in `config.json` in the per-user config directory (`~/.config/mokosh-apps/config.json` on Linux; the platform equivalent on macOS and Windows).
3. The compile-time default baked into the binary.

A minimal `config.json`:

```json
{
  "api_base": "https://api.msp.example.com/api/v1",
  "hub_base_url": "https://example.com",
  "oidc_issuer": "https://api.example.com",
  "brand_name": "PSA Systems",
  "brand_logo_url": "https://example.com/branding/logo.svg"
}
```

Every key the container injects into the web client at runtime is settable here under the same name. See [Configuration](configuration.md) for the list.

### CORS for the desktop origin

A desktop webview has its own origin that is not a browser origin. mokosh-server's CORS allowlist is exact-match with credentials, so the operator adds the desktop origin for the OS the install targets:

| OS | `CORS_ORIGIN` value |
| --- | --- |
| Windows | `http://dioxus.index.html` |
| Android | `https://dioxus.index.html` |
| macOS, Linux, iOS | `dioxus://index.html` |

A desktop install pointed at a CORS allowlist that only names the browser origin will load the sign-in page and then stop. [Troubleshooting](troubleshooting.md) covers the symptom.

### What the installer leaves on disk

- The binary, in the OS's standard install location.
- A per-user config directory with `config.json` and the preferences file.
- A per-user downloads path the app writes to when the user exports a file. The app prints the path when the export finishes.

The installer does not touch system services, system fonts, or the browser.

## Choosing between web and desktop

| Consideration | Web | Desktop |
| --- | --- | --- |
| Install burden | Operator installs once | Every user installs the app |
| Updates | Container restart updates every browser tab | User reinstalls the binary, or uses the installer's update path |
| OIDC redirect | Browser redirect | Loopback redirect on `127.0.0.1:<ephemeral>` |
| OS integration | None beyond the browser | Window, tray, native downloads, native theme |
| Works offline | No | No; the API is always remote |

Most deployments start web-only, and add desktop for staff who want a persistent window alongside their chat client.
