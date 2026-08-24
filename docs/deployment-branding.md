# Branding a hosted deployment

How an operator makes the `mokosh-www` image present as their own brand:
the logo, the wordmark, the tab title, the favicons, the PWA manifest and
the marketing hero. Everything here is applied at container start, from
the compose file plus a few mounted files. No rebuild, no fork, no source
edit (MAPPS-509).

This covers the **platform's own** identity, the one that renders before
anyone signs in. It is a different layer from the per-tenant branding
stored on the tenant record (logo, colours, favicon), which is resolved
from the API after sign-in. Both can be set and they do not interact.

Nothing here renames anything internal. The API paths, the `MOKOSH_*`
env-var prefix, the image and service names, the cookie names, the log
targets and the `mokosh_accent` storage key are unaffected.

## TL;DR

```yaml
services:
  mokosh-www:
    image: dev.a8n.run/psa-systems-public/mokosh-www:latest
    environment:
      MOKOSH_BRAND_NAME: PSA Systems
      MOKOSH_BRAND_LOGO_URL: /branding/logo.svg
      MOKOSH_BRAND_HERO_URL: /branding/hero.png
    volumes:
      - ./branding:/usr/share/caddy/branding:ro
      - ./branding/favicon.svg:/usr/share/caddy/favicon.svg:ro
      - ./branding/favicon.ico:/usr/share/caddy/favicon.ico:ro
      - ./branding/apple-touch-icon.png:/usr/share/caddy/apple-touch-icon.png:ro
      - ./branding/manifest.webmanifest:/usr/share/caddy/manifest.webmanifest:ro
      - ./branding/icon-192.png:/usr/share/caddy/icon-192.png:ro
      - ./branding/icon-512.png:/usr/share/caddy/icon-512.png:ro
      - ./branding/icon-maskable-512.png:/usr/share/caddy/icon-maskable-512.png:ro
```

Then `docker compose up --detach`. Set none of it and the deployment
renders exactly as it does today.

## Two mechanisms, and why there are two

| Mechanism | What it covers | Why |
| --- | --- | --- |
| Mounting a file over the image web root (`/usr/share/caddy`) | Favicons, apple-touch icon, PWA manifest and its icons, `index.html` | These are referenced by fixed, non-hashed root paths, so a file mounted at that path wins. |
| A `MOKOSH_BRAND_*` env var | Product name, in-app logo, marketing hero | These are compiled into the WASM bundle: strings as literals, images as `asset!()` references. |

The second row is the whole reason the env vars exist. `asset!()` is a
build-time macro: `assets/icon-192.png` ships as
`/assets/icon-192-<contenthash>.png`, and nothing in the bundle points at
`/icon-192.png` any more. **Mounting a file over a content-hashed asset
cannot work** - you would have to know the hash, and it changes on every
release. Same for the product name, which is a string literal inside the
compiled bundle. So the SPA reads these three values at runtime instead,
from the same `_mokosh_config.js` shim that already carries `api_base`,
`oidc_issuer` and the rest.

## Mounted static files

`oci-build/Dockerfile` copies these into the Caddy web root, so each one
is served at its root path and can be replaced by a bind mount:

| Path in the container | Referenced by | Notes |
| --- | --- | --- |
| `/usr/share/caddy/favicon.svg` | `index.html` `<link rel="icon">` | Preferred by modern browsers. |
| `/usr/share/caddy/favicon.ico` | `index.html` `<link rel="icon">` | Legacy fallback. |
| `/usr/share/caddy/apple-touch-icon.png` | `index.html` | 180x180. |
| `/usr/share/caddy/manifest.webmanifest` | `index.html` `<link rel="manifest">` | Carries `name`, `short_name`, `description`, `theme_color` and the icon list. Edit all of them; the SPA does not read this file. |
| `/usr/share/caddy/icon-192.png` | the manifest | 192x192. |
| `/usr/share/caddy/icon-512.png` | the manifest | 512x512. |
| `/usr/share/caddy/icon-maskable-512.png` | the manifest | 512x512, `purpose: maskable`. |
| `/usr/share/caddy/index.html` | the browser | `<title>`, `<meta name="description">`, `<meta name="theme-color">`. |

`index.html` is a special case. `oci-build/entrypoint.sh` rewrites it in
place at container start (it injects the `_mokosh_config.js` script tag),
and the OpenGraph / Twitter link-preview tags (see [Link previews](#link-previews-opengraph--twitter)),
so it must stay writable: mount it read-write, or leave it alone and rely
on `MOKOSH_BRAND_NAME` for the title. The `<title>` in `index.html` is
only what the tab shows **before** the SPA boots; once mounted, the SPA
sets `document.title` from `MOKOSH_BRAND_NAME`. Set both so the tab does
not flicker from one name to the other on a cold load.

## Link previews (OpenGraph / Twitter)

When someone pastes a Mokosh link into a chat client or a social platform,
the preview is built by a crawler that fetches the page but does **not** run
the WASM app. The OpenGraph and Twitter card tags therefore cannot be set by
the SPA; `oci-build/entrypoint.sh` stamps them into `index.html`'s `<head>` at
container start, from the branding env, the same way it writes
`_mokosh_config.js`. Set none of it and the tags carry the built-in Mokosh
defaults.

| Env var | Tag | Default |
| --- | --- | --- |
| `MOKOSH_BRAND_NAME` | `og:title`, `og:site_name`, `twitter:title` | `Mokosh Platform` |
| `MOKOSH_BRAND_DESCRIPTION` | `og:description`, `twitter:description` | `Mokosh Platform - Professional Services Automation for MSPs` |
| `MOKOSH_BRAND_LOGO_URL` | `og:image`, `twitter:image` | none: the image tags are omitted and `twitter:card` becomes `summary` instead of `summary_large_image` |
| `MOKOSH_PUBLIC_URL` | resolves a root-relative `MOKOSH_BRAND_LOGO_URL` to the absolute form `og:image` requires | none: a root-relative logo yields no image tags |

`MOKOSH_BRAND_DESCRIPTION` is used **only** for the link preview; unlike the
other `MOKOSH_BRAND_*` vars it is not read by the SPA. Because the tags are
written into `index.html`, the same writable-root caveat applies as for the
`_mokosh_config.js` injection above: mount `index.html` read-write (or leave
it in place) so the entrypoint can patch it. A read-only root logs a warning
and serves the un-injected page.

### The preview image URL must be absolute

`og:image` is fetched by the crawler's own servers, which have no page to
resolve a relative path against, so a root-relative value is dropped and the
card renders without artwork. Root-relative is exactly what
[the section below](#where-the-logo-and-hero-may-be-served-from) recommends for
the in-app logo, because the browser CSP is `img-src 'self'` - so set
`MOKOSH_PUBLIC_URL` to the site's public base URL and the entrypoint joins the
two:

```yaml
environment:
  MOKOSH_PUBLIC_URL: https://msp.example.com
  MOKOSH_BRAND_LOGO_URL: /branding/logo.svg   # og:image https://msp.example.com/branding/logo.svg
```

An already-absolute `MOKOSH_BRAND_LOGO_URL` is used unchanged and needs no
`MOKOSH_PUBLIC_URL`. A relative one with no `MOKOSH_PUBLIC_URL` set drops the
image tags (falling back to `twitter:card: summary`) and logs the reason at
container start, rather than emitting a URL no crawler can fetch. CSP does not
apply here: the crawler is not a browser.

### Verifying a preview

`just check-link-preview` runs the real entrypoint and Caddyfile in a container
and fetches the result with `curl`, which is what a crawler is: an HTTP GET with
no JavaScript. It checks the branded, default and relative-logo cases, on the
root URL and on a deep client-side route. Against a live deployment, the same
check by hand is:

```nushell
http get --raw https://msp.example.com/tickets/12345 | lines | find 'og:'
```

A card validator (or pasting the link into the chat client itself) confirms the
last mile, since each platform decides its own layout from these tags.

## Runtime config env vars

Set on the `mokosh-www` container. All three are optional; an unset or
empty variable is omitted from `_mokosh_config.js` entirely and the SPA
falls back to its built-in value.

| Env var | Config field | Default | Where it renders |
| --- | --- | --- | --- |
| `MOKOSH_BRAND_NAME` | `brand_name` | `Mokosh Platform` | Tab title (bare and as the `"{page} \| {brand}"` suffix), the app top-bar wordmark, the sign-in wordmark, the client-portal footer ("Powered by ..."), the marketing wordmark and copyright line, "Sign in to ...", "Welcome to ...", the profile role line, the System Status subtitle, the demo-data copy, the theme-picker note and the update banner. |
| `MOKOSH_BRAND_LOGO_URL` | `brand_logo_url` | built-in `icon-192.png` | The logo in the app top bar and on the sign-in screen. Its `alt` text follows the brand name. |
| `MOKOSH_BRAND_HERO_URL` | `brand_hero_url` | built-in `mokosh-hero.png` | The illustration on the marketing landing page. Setting it also replaces the alt text, which otherwise describes artwork that is no longer on the page. |
| `MOKOSH_DOCS_URL` | `docs_base_url` | none (hidden) | The documentation subdomain base URL (e.g. `https://docs.n.niceguyit.biz`). Set it to show a top-level **Documentation** entry in the sidebar and to activate the contextual help links (each deep-links to an article under this base). Unset, both stay hidden so no link points at a missing site (MAPPS-453). |

`src/branding.rs` is the single reader; every render site calls it.

### Where the logo and hero may be served from

The Caddyfile sets

```
img-src 'self' data: {$MOKOSH_API_ORIGIN:}
```

so a branded image must be served from **the SPA's own origin or the API
origin**. A third-party CDN URL is blocked by the browser, and the only
evidence is a CSP violation in the console. The simple option is to mount
the file into the web root and use a root-relative URL:

```yaml
volumes:
  - ./branding:/usr/share/caddy/branding:ro
environment:
  MOKOSH_BRAND_LOGO_URL: /branding/logo.svg
```

Avoid `/assets/...` and `/wasm/...` for these files: see caching, below.

### Cache headers

Caddy serves everything `no-cache, no-store, must-revalidate` **except**
`/assets/*` and `/wasm/*`, which are content-hashed by the build and are
sent `immutable` with a one-year max-age. So:

- A branding file mounted anywhere outside `/assets` and `/wasm`
  (`/branding/logo.svg`, `/favicon.svg`, `/manifest.webmanifest`,
  `/index.html`) revalidates on every load, and replacing it propagates
  on the next page load.
- A file mounted **into** `/assets` would be cached by browsers for a
  year at a stable URL, so a later change would not reach anyone who had
  already loaded it. Do not put branding there.

`_mokosh_config.js` itself is `no-cache`, so an env-var change takes
effect on the next load after `docker compose up --detach`.

## Verifying

```nushell
docker compose up --detach
http get http://localhost:8080/_mokosh_config.js
http get http://localhost:8080/branding/logo.svg | describe
```

`_mokosh_config.js` should contain the `brand_name`, `brand_logo_url` and
`brand_hero_url` fields you set, and nothing for the ones you left unset.
Then load the app and check the tab title, the sign-in wordmark and the
top-bar logo, with the browser console open: a blocked image reports as a
`Content-Security-Policy` violation naming `img-src`.

## What is not covered

- Per-tenant branding (the tenant record's logo, colours and favicon) is
  a separate, API-resolved layer applied after sign-in.
- Emails, PDFs and anything else the API renders are branded on the
  server, not here. See the `mokosh-server` repo.
- Colour theming is the accent/base theme system, not this document.
