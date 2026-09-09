# Mokosh Apps documentation

The public documentation set. The repository [`README.md`](../README.md) is
deliberately short and points here; anything longer than a paragraph lives on
one of these pages.

## Getting it running

| Page | Purpose |
| --- | --- |
| [`quickstart.md`](quickstart.md) | Get a fresh clone serving in the browser: prerequisites, the `common` submodule, `just dev`, the ports, the dev-only login bypass, and the failures that bite first. |
| [`recipes.md`](recipes.md) | The task runner, recipe by recipe, grouped the way `just --list` groups them. |
| [`desktop.md`](desktop.md) | The native desktop build: its extra prerequisites, its recipes, and how it is pointed at a server. |
| [`self-hosting.md`](self-hosting.md) | Run the published `mokosh-www` image: the tags, every runtime environment variable, ingress and cookie constraints, and applying an update. |
| [`deployment-branding.md`](deployment-branding.md) | Make the image present as an operator's own brand: the logo, wordmark, tab title, favicons, PWA manifest and marketing hero, all applied at container start. |

## How it is built

| Page | Purpose |
| --- | --- |
| [`architecture.md`](architecture.md) | The stack, the repository layout, the cargo features, and the two artifacts a build produces. |
| [`client-server-integration.md`](client-server-integration.md) | The path a request takes out of this client, and what the two repositories share on the wire: the `mokosh-types` crate, the pin guard, and the copies the compiler cannot see. |
| [`versioning.md`](versioning.md) | Where the displayed version comes from, how a redeploy reaches an open tab, and how a release is cut. |
| [`spa-rollout-runbook.md`](spa-rollout-runbook.md) | Rolling the SPA so a load balancer never serves two builds at once. |
| [`oidc-token-storage.md`](oidc-token-storage.md) | Where the tokens are kept, and the accepted-risk decision behind that choice. |

## Conventions a change has to keep

Each of these is backed by a guard in `scripts/` that fails `just check` when
the convention is broken.

| Page | Purpose |
| --- | --- |
| [`form-conventions.md`](form-conventions.md) | Create and edit forms: modal against full page, and which reference picker a field takes. |
| [`button-variants.md`](button-variants.md) | One correct button variant per action. `/dev/buttons` renders every one of them. |
| [`destructive-actions.md`](destructive-actions.md) | Every destructive action confirms before it mutates, and reports the server's refusal. |
| [`email-actions.md`](email-actions.md) | Every click that makes the server email someone is marked as such and offers a preview first. |

## Plan and internal notes

[`ROADMAP.md`](ROADMAP.md) holds the durable plan: goals, sequencing, and the
reasoning behind the order. Each item links its YouTrack issue, and status is
read from the tracker rather than restated there.

[`dev-docs/`](dev-docs/README.md) holds the internal working notes, including a
frozen 2026-05-06 audit snapshot and the QA plans. Its index says which of those
is maintained and which is a point-in-time record.
