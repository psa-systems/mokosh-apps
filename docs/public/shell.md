# The shell

This page is for anyone using mokosh-apps: operators, technicians, and clients. It is a short tour of the layout after sign-in and the controls that are the same on every screen.

## The layout

After sign-in the home shell opens on the Dashboard. Three regions:

- **Top bar**: product wordmark on the left, the global search field in the middle, the site switcher and account menu on the right. The wordmark is a link to the home screen.
- **Sidebar**: the main navigation. Each entry opens a section: Dashboard, Tickets, Customers, Billing, Time, Knowledge base, and whatever feature flags are on for the deployment.
- **Main area**: the content for the selected section.

The sidebar collapses to icons below a narrow width. The icons stay in place; hovering shows the label. The top bar does not collapse.

## The site switcher

The site switcher lives on the top bar. Click it to switch between the sites, organizations, or tenants the signed-in user belongs to. A user in one tenant sees only that tenant and has no switcher. A user in two or more sees one entry per tenant and, when organizations are enabled, a nested list of organizations under each.

Switching sites reloads the main area for the new context. The URL updates to reflect the active tenant.

Organizations are off by default. The operator turns them on through `MOKOSH_ORGANIZATIONS_ENABLED`; see [Configuration](configuration.md).

## The account menu

The account menu on the top bar opens a popover with:

- **Signed in as**: the user's name and email.
- **Theme**: Light, Dark, System. The choice persists per browser (or per OS user for the desktop binary).
- **Account**: opens Bunyip's account management surfaces in a new tab. Change password, enroll two-factor, review sessions, revoke devices.
- **Sign out**: ends the session and returns to the sign-in screen. The desktop app closes the window.

The hub URL comes from `MOKOSH_HUB_BASE_URL`; see [Configuration](configuration.md).

## Keyboard shortcuts

A handful of shortcuts work from any screen. They are not case sensitive.

| Shortcut | Action |
| --- | --- |
| `/` | Focus the global search |
| `g` then `d` | Go to Dashboard |
| `g` then `t` | Go to Tickets |
| `g` then `c` | Go to Customers |
| `g` then `b` | Go to Billing |
| `?` | Show the shortcut cheat sheet |
| `Esc` | Close the active dialog or popover |
| `Cmd+K` / `Ctrl+K` | Open the global command palette, when enabled |

Dialog-local shortcuts (save, cancel, submit) are shown on the dialog itself.

## The theme toggle

The theme choice is a per-user preference. Light and Dark are explicit; System follows the OS theme and switches when the OS switches. The desktop app listens for OS theme changes through the webview and swaps without a reload.

The accent colour is a deployment setting and is not user-adjustable in the shell. See [Branding](branding.md).

## Navigation

- Click a sidebar entry to switch sections.
- Click a link in the main area to go deeper. The browser Back and Forward buttons work; on the desktop the window has its own back arrow in the top bar.
- Use the Breadcrumbs under the main area title to climb back up from a deep page.
- Opening a link in a new tab (middle-click or Cmd+click) works in the browser and opens a new OS browser tab from the desktop app.

A link to an external site (OIDC hub, documentation) opens in a new window or tab, so the current context is not lost.

## The staff bar and preview

On a client-portal page, a signed-in staff member sees a staff bar at the top. The bar names the client being viewed and offers a View as visitor action that drops the staff session and reloads the page as the client would see it. Exit the visitor view by signing in again.

The staff bar is never shown to clients; it needs a staff session with permission to view the current portal.

## Updates

A new version of the deployed client is picked up on the next load. The running tab has a built-in update banner that fires when the deployed bundle revision differs from the one the tab is running, and reloads at the next safe boundary. Clicking Reload now reloads immediately; dismissing waits until the next natural navigation.

The desktop binary does not auto-update; updates go through the installer. See [Upgrades](operations/upgrades.md).
