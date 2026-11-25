# Getting started

Three steps to a working sign-in. The flow is the same whether the client is served from the web image or launched as a desktop binary.

## 1. Point the client at a mokosh-server

Every deployment starts with a mokosh-server reachable over HTTPS, and a Bunyip OIDC issuer the server is configured to trust. The client sends every API call to the server's `/api/v1` and signs users in against the issuer.

A `msp.<tld>` web deployment derives both from the request host automatically: the API base becomes `https://api.<tld>` and the issuer becomes the same host. Deployments that do not follow that shape set `MOKOSH_API_BASE` and `MOKOSH_OIDC_ISSUER` on the container. A desktop install carries the same values in its per-user `config.json`.

See [Configuration](configuration.md) for every variable the client reads.

## 2. Run the image, open the URL

```yaml
services:
  mokosh-www:
    image: dev.a8n.run/psa-systems-public/mokosh-www:latest
    environment:
      MOKOSH_API_BASE: https://api.msp.example.com/api/v1
      MOKOSH_OIDC_ISSUER: https://api.msp.example.com
      MOKOSH_OIDC_CLIENT_ID: <client id from bunyip-api>
      MOKOSH_BRAND_NAME: PSA Systems
```

```nu
docker compose up --detach
```

A full operator install sits behind a TLS reverse proxy, mounts branding files, and pins a `vX.Y.Z` tag. The step-by-step is in [Self-hosting](self-hosting.md).

The desktop binary runs with no compose file. Download the installer for the operator's platform, launch it, and the window opens. [Installation](installation.md) covers the per-platform details.

## 3. Sign in

Opening the client lands on the sign-in screen. The sign-in button hands off to Bunyip, the user authenticates there, and Bunyip redirects back with an authorization code. The exchange happens in the client, which stores the resulting bearer and refreshes it before expiry.

The first user to sign in needs an identity Bunyip already knows about. Create that identity in Bunyip first; mokosh-apps does not sign users up.

After sign-in the home shell opens on the Dashboard. [The shell](shell.md) is a tour of the layout, the site switcher, the theme toggle, and the account menu.

## Next steps

- [OIDC integration](oidc-integration.md) if the sign-in fails or redirects loop.
- [Branding](branding.md) to replace the Mokosh wordmark, logo, and hero with the operator's own.
- [Self-hosting](self-hosting.md) for the full operator install.
- [Troubleshooting](troubleshooting.md) if sign-in works in staging but not production.
