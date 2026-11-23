# Upgrades

How a running mokosh-apps deployment picks up a new version, and what the operator controls. The web client and the desktop binary take different paths.

## The web client

A browser tab is pinned to the bundle it loaded. The running tab learns about a new deploy by watching the build revision carried by `_mokosh_config.json`, which the container rewrites on every start. When the running tab's revision differs from the deployed revision, the tab shows an update banner.

The banner has three states:

- **Available**: a new version is deployed. The banner shows "Update available" with a Reload now action.
- **Dismissed**: the user clicked away. The banner is hidden until the next natural navigation, where it reappears.
- **Applying**: the user clicked Reload now. The tab reloads at the next safe boundary.

Safe boundary means the tab is not in the middle of an operation that would lose state on reload: an open editor with unsaved changes, a timer running, a dialog mid-save. The reload waits for the operation to finish, then reloads.

Open tabs do not need a hard refresh to pick up the new bundle. Reload now is equivalent to the browser's refresh; the banner just prompts the user to run it.

### Checking on the server

The client checks its deployed revision against the server's revision on each sign-in and periodically while signed in. The server exposes its own version through the API so the client can warn when the two have drifted too far apart. A drift wider than one minor version shows a Server version mismatch banner with a note to upgrade mokosh-server or mokosh-apps to close the gap.

### Operator knobs

Nothing to set. The update banner is on by default and is not configurable. Rollouts are driven by the deployed image tag; see [Release and rollout](release-and-rollout.md).

A deployment that wants to delay the client picking up a new revision pins an older image tag in the compose file; the running tabs keep showing the version they loaded until the operator bumps the tag.

## The desktop binary

A desktop binary is not served a bundle, and it has no in-app update check: the running app never looks for a newer version. Updates arrive only through a new installer, which the operator redistributes through whatever channel they already use (an internal download page, email, a software deployment tool). The user downloads the new installer, runs it, and it replaces the old binary. Until they do, the old binary keeps running with no notice or warning that a newer version exists.

## Rolling the server

Mokosh-apps and mokosh-server release independently. The client supports the current server minor and the two previous server minors. Upgrading the server past that range fires the Server version mismatch banner mentioned above.

Upgrade order when both are being bumped on the same day:

1. Deploy the new mokosh-server.
2. Wait until mokosh-server is healthy (its `/api/health` returns ok).
3. Deploy the new mokosh-apps.

Running the client ahead of the server is the compatibility window the client is designed for and is usually fine; running the server ahead of the client is also fine for the supported range. Running outside the supported range breaks sign-in or specific features and is flagged.
