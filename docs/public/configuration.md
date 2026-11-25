# Configuration

This page is for operators configuring a mokosh-apps deployment. Every value here is a `MOKOSH_*` environment variable on the `mokosh-www` container or a matching key in the desktop `config.json`.

The web client picks values up from `/_mokosh_config.js`, which the container rewrites on each start. Only variables that are set and non-empty reach the client; an unset variable falls through to the compile-time default. Restart the container after an environment change so the file is regenerated.

The desktop client reads the same names from `config.json` in the per-user config directory, or from a `MOKOSH_<FIELD>` environment variable in the shell that launched it. See [Installation](installation.md) for the per-OS config path.

## Server endpoints

| Variable | Required | Summary |
| --- | --- | --- |
| `MOKOSH_API_BASE` | No | mokosh-server API base URL (for example `https://api.msp.example.com`). Unset derives it from the request host on a `msp.<tld>` web deployment and falls back to same-origin `/api/v1` in dev. |
| `MOKOSH_OIDC_ISSUER` | No | Bunyip OIDC issuer URL. Unset derives it from the request host on a `msp.<tld>` web deployment. |
| `MOKOSH_HUB_BASE_URL` | No | Base URL of the Bunyip hub (the account management surfaces). Unset keeps the compile-time default. |

A desktop install has no request host to derive from, so both are supplied explicitly through `config.json`.

## OIDC client

| Variable | Required | Summary |
| --- | --- | --- |
| `MOKOSH_OIDC_CLIENT_ID` | No | The SPA's registered Bunyip client id. Unset keeps the SPA's compile-time default. Register the client in Bunyip first. |
| `MOKOSH_OIDC_REDIRECT_URI` | No | Fully qualified redirect URI the OIDC authorize call sends. Needed for a path-prefixed or otherwise non-default deployment. |
| `MOKOSH_OIDC_SCOPES` | No | Space-separated scope string. Default is `openid email offline_access`. Set to opt in to Bunyip's `profile` or `phone` claims. |

Full OIDC setup is in [OIDC integration](oidc-integration.md).

## Portal and documentation

| Variable | Required | Summary |
| --- | --- | --- |
| `MOKOSH_PORTAL_HOST` | No | The single host the client portal is served from (for example `portal.psa.systems`). Needs one DNS record and a matching TLS SAN. Empty disables the portal-host derivation. |
| `MOKOSH_PUBLIC_URL` | No | The site's public base URL. Used to absolutise a root-relative `MOKOSH_BRAND_LOGO_URL` so link-preview tags resolve for crawlers. |
| `MOKOSH_DOCS_BASE_URL` | No | Documentation subdomain base URL (for example `https://docs.a8n.systems`). Unset hides the Documentation menu unless derived from a `msp.<tld>` host. Set to the empty string to hide it explicitly. |

## Branding

| Variable | Required | Summary |
| --- | --- | --- |
| `MOKOSH_BRAND_NAME` | No | Operator product name. Shown in the tab title, the top bar, the sign-in screen, and every link preview. Default is `Mokosh Platform`. |
| `MOKOSH_BRAND_DESCRIPTION` | No | Operator product description. Used only for link previews. |
| `MOKOSH_BRAND_LOGO_URL` | No | Operator logo URL. Serve from the client origin or the API origin; third-party CDNs are blocked by the browser CSP. |
| `MOKOSH_BRAND_HERO_URL` | No | Operator hero illustration URL for the marketing landing. Same origin rules as the logo. |

Mount the actual image files, favicons, and PWA manifest alongside these. Full branding walkthrough is in [Branding](branding.md).

## Features

| Variable | Required | Summary |
| --- | --- | --- |
| `MOKOSH_ORGANIZATIONS_ENABLED` | No | Set to `1`, `true`, or `yes` to show the organization switcher and the create-team action. Off by default. |
| `MOKOSH_KB_ATTACHMENT_MAX_BYTES` | No | The client's pre-upload cap for knowledge base attachments. Set to the same value as mokosh-server's `KB_ATTACHMENT_MAX_BYTES` so limits do not drift. Unset keeps the compile-time 5 MiB default. |

## Runtime

| Variable | Required | Summary |
| --- | --- | --- |
| `MOKOSH_RUNTIME_DIR` | No | Writable directory the image uses for runtime scratch. Defaults to `/tmp/mokosh-runtime`. Override when the container runs with that path read-only or when a different writable mount is preferred. |

## Deprecated

| Variable | Removed | Replacement |
| --- | --- | --- |
| `MOKOSH_PORTAL_HOST_SUFFIX` | v0.14.0 | `MOKOSH_PORTAL_HOST` (single host, no wildcard) |
| `MOKOSH_TEAM_ENABLED` | v0.14.0 | None; Teams is core and has no flag |
| `MOKOSH_DOCS_URL` | still read, warns | `MOKOSH_DOCS_BASE_URL` |

Delete a removed variable from the operator's compose file; the image ignores it.

## Dev-only variables

These are build-time inputs, not runtime variables. They have no effect on a production image.

| Variable | Summary |
| --- | --- |
| `ADMIN_EMAIL` | Dev-only login bypass. Baked into the bundle at compile time with the `dev_admin_bypass` cargo feature. Never use in production. |
| `ADMIN_PASSWORD` | Paired with `ADMIN_EMAIL`. Same compile-time-only, dev-only rules apply. |
