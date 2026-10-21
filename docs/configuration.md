# Configuration

Operator-facing runtime variables the mokosh-www image reads. The variable
column is sourced from `config/variables.toml`; the Required and Added in
columns are generated from the same file by `just config-docs`, and
`just check-config-docs` fails CI when any row disagrees with it.

Across the SPA's two deployment shapes:

- **Container (mokosh-www)**: an operator sets the variables below on the
  container (compose variables / Kubernetes env). The entrypoint renders
  them into `_mokosh_config.js` / `_mokosh_config.json` and patches
  `index.html` with the `<script>` include at container start. The SPA
  reads them before falling back to its compile-time defaults and the
  built-in `msp.<tld>` host-prefix derivation.
- **Desktop (mokosh-apps-desktop)**: there is no entrypoint. A desktop
  install reads the same fields from `config.json` in the per-user
  config directory, with a `MOKOSH_<FIELD>` environment variable as an
  override. Variables documented below are therefore operator-facing on
  the container and user-facing on the desktop.

## Recently added

<!-- BEGIN GENERATED: recently-added (just config-docs) -->

Configuration changes in the last 5 releases, newest first, rendered by `just config-docs` from `config/variables.toml` (edit that, not this section). **Breaking: won't start** marks a change after which the container refuses to start until the operator acts; apply it before restarting. **Breaking: feature off** marks a change after which the image runs but the named feature stops working or changes until the operator acts; review it before restarting. Both name the operator action and the issue that introduced them. An unmarked entry needs no action unless you relied on the old behavior. Upgrading across more releases than this? Run `just config-since <running version>`.

<a id="release-v0-17-0"></a>

### v0.17.0

No configuration changes.

<a id="release-v0-16-0"></a>

### v0.16.0

#### Changed

- [`MOKOSH_DOCS_BASE_URL`](#var-mokosh-docs-base-url) (Required: no) - Unset on a `msp.<tld>` host now builds `https://docs.<tld>` instead of hiding the Documentation menu. If you want the menu hidden, set this to the empty string explicitly; if you want to override the derivation, set it to your docs URL as before. (MAPPS-999)

<a id="release-v0-15-3"></a>

### v0.15.3

No configuration changes.

<a id="release-v0-15-2"></a>

### v0.15.2

No configuration changes.

<a id="release-v0-15-1"></a>

### v0.15.1

No configuration changes.

<!-- END GENERATED: recently-added -->

## All variables

| Variable | Required | Added in | Where set | Purpose |
| --- | --- | --- | --- | --- |
| <a id="var-mokosh-api-base"></a>`MOKOSH_API_BASE` | no | v0.1.0 | compose variable | mokosh-server API base URL. Unset derives it from the request host on a `msp.<tld>` deployment, falling back to same-origin `/api/v1` in dev. |
| <a id="var-mokosh-oidc-issuer"></a>`MOKOSH_OIDC_ISSUER` | no | v0.1.0 | compose variable | Bunyip OIDC issuer URL. Unset derives it from the request host on a `msp.<tld>` deployment. |
| <a id="var-mokosh-oidc-client-id"></a>`MOKOSH_OIDC_CLIENT_ID` | no | v0.1.0 | compose variable | The SPA's registered Bunyip client id. Unset keeps the SPA's compile-time default. |
| <a id="var-mokosh-oidc-redirect-uri"></a>`MOKOSH_OIDC_REDIRECT_URI` | no | v0.14.0 | compose variable | Fully-qualified redirect URI the OIDC authorize call sends. Needed for a path-prefixed or otherwise non-default deployment; unset keeps the SPA's compile-time default. |
| <a id="var-mokosh-oidc-scopes"></a>`MOKOSH_OIDC_SCOPES` | no | v0.14.0 | compose variable | Space-separated scope string for `/oauth2/authorize`. Default is `openid email offline_access`; set this to opt in to Bunyip's `profile` or `phone` claim emission without rebuilding the image. |
| <a id="var-mokosh-hub-base-url"></a>`MOKOSH_HUB_BASE_URL` | no | v0.1.0 | compose variable | Base URL of the Bunyip hub (account management surfaces). Unset keeps the compile-time default. |
| <a id="var-mokosh-portal-host"></a>`MOKOSH_PORTAL_HOST` | no | v0.14.0 | compose variable | The single host the client portal is served from. Empty turns the portal-host derivation off; set it alongside a cross-origin API host. |
| <a id="var-mokosh-public-url"></a>`MOKOSH_PUBLIC_URL` | no | v0.14.0 | compose variable | The site's public base URL, used to absolutise a root-relative `MOKOSH_BRAND_LOGO_URL` for og:image / twitter:image. Unset means a relative logo URL is dropped from the link-preview tags. |
| <a id="var-mokosh-docs-base-url"></a>`MOKOSH_DOCS_BASE_URL` | no | v0.14.0 | compose variable | Documentation subdomain base URL. Unset hides the Documentation menu and every contextual help link, unless the SPA can derive it from a `msp.<tld>` host (MAPPS-999). |
| <a id="var-mokosh-docs-url"></a>`MOKOSH_DOCS_URL` | no | v0.1.0; deprecated in v0.14.0, use `MOKOSH_DOCS_BASE_URL` | compose variable | Deprecated predecessor of `MOKOSH_DOCS_BASE_URL`. Still honoured as a fallback with a warning in the entrypoint log. |
| <a id="var-mokosh-organizations-enabled"></a>`MOKOSH_ORGANIZATIONS_ENABLED` | no | v0.14.0 | compose variable | Set to `1`, `true` or `yes` to show the organization switcher and the create-team action. Off by default: the feature is unbuilt and its charging model is unresolved. |
| <a id="var-mokosh-kb-attachment-max-bytes"></a>`MOKOSH_KB_ATTACHMENT_MAX_BYTES` | no | v0.14.0 | compose variable | The SPA's pre-upload cap for KB attachments. Set to the SAME value as mokosh-server's `KB_ATTACHMENT_MAX_BYTES` so the client and server limits do not drift. Unset keeps the compile-time 5 MiB default. |
| <a id="var-mokosh-brand-name"></a>`MOKOSH_BRAND_NAME` | no | v0.12.0 | compose variable | Operator product name shown in the title, OG/Twitter tags and the SPA shell when no tenant branding overrides it (MAPPS-509). |
| <a id="var-mokosh-brand-description"></a>`MOKOSH_BRAND_DESCRIPTION` | no | v0.14.0 | compose variable | Operator product description used as the og:description / twitter:description when a link is pasted into a chat client or social platform (MAPPS-477). |
| <a id="var-mokosh-brand-logo-url"></a>`MOKOSH_BRAND_LOGO_URL` | no | v0.12.0 | compose variable | Operator logo URL. Serve from the SPA origin or from the API origin; the Caddyfile CSP is `img-src 'self' data: {API origin}`. A root-relative value needs `MOKOSH_PUBLIC_URL` set for og:image to resolve. |
| <a id="var-mokosh-brand-hero-url"></a>`MOKOSH_BRAND_HERO_URL` | no | v0.14.0 | compose variable | Operator hero image URL for the unauthenticated landing page. Same origin / CSP rules as `MOKOSH_BRAND_LOGO_URL`. |
| <a id="var-mokosh-portal-host-suffix"></a>`MOKOSH_PORTAL_HOST_SUFFIX` | no | v0.1.0; removed in v0.14.0 | compose variable | Retired per-MSP portal-host suffix (replaced by `MOKOSH_PORTAL_HOST`). |
| <a id="var-mokosh-team-enabled"></a>`MOKOSH_TEAM_ENABLED` | no | v0.12.0; removed in v0.14.0 | compose variable | Retired Teams feature flag; Teams is core and no longer needs a flag. |
| <a id="var-mokosh-runtime-dir"></a>`MOKOSH_RUNTIME_DIR` | no | v0.1.0 | compose variable | Not described yet: say what it does. |
