# OIDC integration

This page is for operators wiring Bunyip as the OIDC issuer for a mokosh-apps deployment. Mokosh does not issue sessions on its own; every user signs in through Bunyip, which hands back a bearer the client presents to mokosh-server.

## What the client does

1. The user clicks sign in.
2. The client generates PKCE and a `state` value, stores them, and redirects the browser (or opens a system browser, for desktop) to `<issuer>/oauth2/authorize`.
3. Bunyip authenticates the user and redirects back to the client with `code` and `state` in the query.
4. The client verifies `state`, exchanges `code` for an access token and a refresh token at `<issuer>/oauth2/token`, and stores them.
5. On every API call the client attaches `Authorization: Bearer <access token>`. On expiry the client uses the refresh token to get a new pair silently.
6. On sign out the client revokes the refresh token at `<issuer>/oauth2/revoke` and clears its local copy.

The flow is standard OAuth 2.1 Authorization Code with PKCE. The client is a public client; no client secret is used or stored.

## Register the client in Bunyip

In Bunyip, create an OIDC client with:

- **Client type**: public
- **Grant types**: `authorization_code`, `refresh_token`
- **Redirect URI**: the browser redirect for the web deployment, and the loopback form for every desktop OS that will install the app
- **Scopes**: at least `openid email offline_access`

Set the client id on the mokosh-apps container through `MOKOSH_OIDC_CLIENT_ID`. Or in the desktop `config.json` under `oidc_client_id`.

### Redirect URIs

The browser deployment redirects to `<client origin>/auth/callback`. Example: `https://msp.example.com/auth/callback`.

The desktop app has no browser origin. It binds an ephemeral port on `127.0.0.1` and uses `http://127.0.0.1:<port>/auth/callback` as the redirect. The OP must accept any port on `127.0.0.1` for the registered redirect URI. This is the RFC 8252 native-app rule; Bunyip supports it when the redirect URI is registered as `http://127.0.0.1/auth/callback`.

### Scopes

The default scope string is `openid email offline_access`. Override with `MOKOSH_OIDC_SCOPES` to request Bunyip's `profile` or `phone` claims without rebuilding the image.

## Point the client at the issuer

| Variable | What to set it to |
| --- | --- |
| `MOKOSH_OIDC_ISSUER` | The Bunyip issuer URL (for example `https://api.msp.example.com`). Unset derives it from the web host on a `msp.<tld>` deployment. |
| `MOKOSH_OIDC_CLIENT_ID` | The client id from the Bunyip registration above. |
| `MOKOSH_OIDC_REDIRECT_URI` | Explicit redirect URI for a path-prefixed or non-default deployment. Unset keeps the compile-time default. |
| `MOKOSH_OIDC_SCOPES` | Space-separated override for the default scope string. |

See [Configuration](configuration.md) for the full variable list.

## Token storage

Token storage differs between the web and desktop targets.

### Web

- The access token lives in `sessionStorage`. It clears on tab close.
- The refresh token lives in `localStorage` under an HttpOnly-equivalent key. It survives tab close within the same browser profile.
- The PKCE verifier and the `state` value live in `sessionStorage` for the duration of the pending sign-in, and are cleared after the exchange.

Nothing is written to IndexedDB. Nothing is written to a cookie.

### Desktop

- Both the access token and the refresh token live in memory for the lifetime of the process.
- The tokens are not persisted to disk. Closing the window ends the session; the next launch signs in again.

Persisting the desktop refresh token to disk is deliberately not done, because a plaintext refresh token under the user's home directory is a credential anything on the system can read. The RFC 8252 design is that each session starts with a fresh sign-in through the system browser, which reuses the browser's existing OP session if any.

## Verifying the setup

1. Open the client, click sign in.
2. The browser lands on `<issuer>/oauth2/authorize?response_type=code&client_id=...&redirect_uri=...`.
3. Sign in at Bunyip.
4. The browser redirects back to the client with `code` and `state`.
5. The home shell opens on the Dashboard.

If any step above fails, see [Troubleshooting](troubleshooting.md) for the common failures: wrong client id, redirect URI mismatch, scope not consented, clock skew between client and issuer.

## What mokosh-server sees

On every API call the server receives `Authorization: Bearer <access token>`. The server validates the signature against Bunyip's JWKS, confirms the issuer and audience, confirms the user still exists on the server side, and answers.

The server never gets the refresh token, because the client renews on its own and only sends the current access token. The server does not maintain its own session state for an OIDC user; every request stands on its own bearer.
