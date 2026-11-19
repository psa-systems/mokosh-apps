# Troubleshooting

Five failures operators hit most often, each with the symptom, the cause, and the fix.

## Sign-in loop on the OIDC callback

**Symptom**: the user clicks sign in, Bunyip authenticates them, the browser redirects back, and the client immediately redirects to sign in again. The loop repeats until the user closes the tab.

**Cause**: the redirect URI the client sent to Bunyip does not match the redirect URI Bunyip has registered for this client, or the client id in `MOKOSH_OIDC_CLIENT_ID` does not match the client Bunyip registered.

**Fix**:

1. In Bunyip, open the OIDC client and check both the Client id and the Redirect URIs.
2. In the operator's compose file or the desktop `config.json`, check `MOKOSH_OIDC_CLIENT_ID` and `MOKOSH_OIDC_REDIRECT_URI` match what Bunyip has.
3. For a desktop install, confirm Bunyip accepts `http://127.0.0.1/auth/callback` as a loopback redirect (any port on the registered URI is allowed per RFC 8252).
4. Restart the container after an environment change so `_mokosh_config.js` is regenerated.

The browser console shows the exact redirect URI the client sent, which is the fastest way to compare against Bunyip's registered value.

## Desktop app cannot find config on first run

**Symptom**: the desktop app launches, shows a blank home shell with a "Server not reachable" notice, and signs the user out immediately. The logs name a missing `MOKOSH_API_BASE` or a connection error to `http://localhost:8080`.

**Cause**: the desktop app has no browser origin to derive from, so the first launch falls through to the compile-time default (`http://localhost:8080/api/v1`), which is right for local development and wrong everywhere else. No `config.json` was created or the file is in the wrong directory.

**Fix**:

1. Create `config.json` in the per-user config directory. On Linux this is `~/.config/mokosh-apps/config.json`; on macOS, `~/Library/Application Support/mokosh-apps/config.json`; on Windows, `%APPDATA%\mokosh-apps\config.json`.
2. Fill in at least `api_base`, `oidc_issuer`, and `oidc_client_id`. See [Installation](installation.md) for the minimal shape.
3. Restart the app.

An operator rolling the desktop binary to many users ships a per-OS installer that drops `config.json` in the right path with the deployment's values pre-filled, so the user does not edit the file.

## SPA shows stale data after a server-side change

**Symptom**: an admin changes a user's role in Bunyip or mokosh-server, the user refreshes, and the UI still reflects the old role until the user signs out and back in.

**Cause**: the client caches the bearer's claims and the user profile for the lifetime of the session. A role change on the server does not revoke the client's cached copy; the cache clears on the next sign-in.

**Fix**:

1. Ask the user to sign out from the account menu and sign back in.
2. On the server side, revoking the user's refresh token through Bunyip forces the client to re-authenticate on its next refresh, which picks up the new role without the user signing out manually.

A deployment that needs role changes to take effect without a sign-out configures mokosh-server to issue short-lived access tokens (for example, 10 minutes) so the role is refreshed at least once every ten minutes. The tradeoff is more token exchanges.

## `MOKOSH_API_BASE` misconfigured

**Symptom**: the sign-in screen loads but every network request fails with a CORS error, a "Server not reachable" notice, or a 404 on `/api/v1/...`.

**Cause**: one of three variants.

- **Unreachable**: the configured URL does not resolve or the server is not running.
- **Wrong scheme**: HTTP configured where HTTPS is required (or vice versa). Cookies with the `Secure` flag are refused on HTTP.
- **Wrong path**: the configured URL includes or omits the `/api/v1` suffix in a way the server does not match.

**Fix**:

1. From the operator's shell, fetch the health endpoint: `curl -I https://api.msp.example.com/api/v1/health` should answer 200. If it does not, the issue is on mokosh-server or in DNS, not in the client.
2. If the health check works, compare the URL to `MOKOSH_API_BASE`. The client suffixes `/api/v1` to the base; set the base to the host without the suffix.
3. For a browser deployment, check the browser console for the exact URL and the CORS error message. mokosh-server's `CORS_ORIGIN` must list the client origin verbatim, no wildcard.
4. For a desktop deployment, check that `CORS_ORIGIN` on the server lists the desktop webview's origin per OS; see [Installation](installation.md).

## Update check fails

**Symptom**: the desktop app logs "Update check failed" repeatedly. The web client's update banner never fires even after a new image is deployed.

**Cause**:

- **Desktop**: the update URL is unreachable, returns a non-JSON body, or the installer-download URL is not writable by the running process.
- **Web**: the container was not restarted after the image bump, so `_mokosh_config.js` still carries the old build revision; the running tab compares its revision against the stale shim and sees no change.

**Fix (desktop)**:

1. Fetch the update URL from the operator's shell: `curl https://downloads.msp.example.com/mokosh-apps/updates.json` should return the JSON shape documented in [Upgrades](operations/upgrades.md).
2. If the fetch works but the app still logs a failure, check the app's log for the next line: a permission error on the install directory, a signature check failure, or a disk-full condition.
3. On Linux, confirm the AppImage path is writable by the user running the app.

**Fix (web)**:

1. On the host serving `mokosh-www`, run `docker compose up --detach` to restart the container; the entrypoint rewrites `_mokosh_config.js` with the new build revision.
2. Reload the tab; the update banner fires.

A deployment that pins a release tag in the compose file does not see the banner even after a `main` push, because the container is still serving the pinned tag. Bump the tag in the compose file and restart to roll forward.
