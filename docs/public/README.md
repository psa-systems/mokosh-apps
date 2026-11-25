# Mokosh apps

Mokosh apps is the client for the Mokosh PSA platform. One source tree builds as a WebAssembly single-page app in the browser and as a native desktop binary. The UI is identical across targets.

This documentation is for the people who run Mokosh and the people who use it:

- **Operators** installing the client, pointing it at a running mokosh-server, configuring Bunyip OIDC, branding the deployment, and rolling out releases.
- **Technicians** working tickets, time, billing, and the knowledge base from the signed-in shell.
- **Clients** reaching the portal their MSP exposes.

It is not a developer guide. The engineering documentation lives alongside the code and is not published here.

## When not to use mokosh-apps

Mokosh apps is not standalone. It needs a running mokosh-server to talk to, and a Bunyip OIDC issuer to sign users in through. A deployment without those two is a browser app that cannot reach data.

For the server side and its own operator documentation, see the mokosh-server repository.

## Where to start

- **First time installing?** Read [Getting started](getting-started.md), then [Installation](installation.md) for the platform you are deploying to.
- **Setting up OIDC?** [OIDC integration](oidc-integration.md) covers the Bunyip wiring.
- **Branding a deployment?** [Branding](branding.md).
- **Using the app?** [The shell](shell.md), then the [Features](features/overview.md) section.
- **Something broken?** [Troubleshooting](troubleshooting.md) has the five failures operators hit most often.

## Features at a glance

- [Dashboard](features/dashboard.md): the home screen, ranges, and widgets.
- [Tickets](features/tickets.md): intake, assignment, approval, resolution.
- [Customers](features/customers.md): companies, overview, filters.
- [Contacts](features/contacts.md): the people inside a company.
- [Billing](features/billing.md): invoices, quotes, payments.
- [Time tracking](features/time-tracking.md): segments, timesheets, approvals.
- [Knowledge base](features/knowledge-base.md): articles, versions, comments.
