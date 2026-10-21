# Self-hosting the SPA

Pull the pre-built multi-arch image from the public registry and run it with the
reference compose file. No build, no registry login.

```nu
cp oci-build/compose.example.yml compose.yml
# Edit the MOKOSH_* env vars in compose.yml to point at your own
# mokosh-server API and OIDC issuer.
docker compose up --detach
```

[`oci-build/compose.example.yml`](../oci-build/compose.example.yml) is the
authoritative reference: it carries the same variables described here, with the
production notes, alongside each one.

This stack is the SPA only. The API and the OIDC issuer it calls are
mokosh-server, which has its own repository and its own compose file.

## The image

`dev.a8n.run/psa-systems-public/mokosh-www`, built from
[`oci-build/Dockerfile`](../oci-build/Dockerfile) and serving the WASM bundle
with Caddy.

- `:vX.Y.Z` pins a specific release.
- `:latest` rolls forward on every push to `main`.

It supports `linux/amd64` and `linux/arm64`, and compose pulls the variant
matching the host kernel automatically.

Pin a `:vX.Y.Z` tag for predictable rollouts. Rolling the SPA across more than
one replica has an ordering constraint of its own, so that a load balancer never
serves two builds at once: [`spa-rollout-runbook.md`](spa-rollout-runbook.md).

## Runtime configuration

Configuration is supplied as environment variables on the container, not baked
into the image, so one image serves staging, production and every self-host
deployment. [`oci-build/entrypoint.sh`](../oci-build/entrypoint.sh) writes a
small `/_mokosh_config.js` from them on each start, and the SPA reads it before
falling through to its compile-time defaults. Only variables that are set and
non-empty are emitted. Restart the container to pick up a change.

Every operator-facing variable, with its Required / Added-in facts and the
Breaking changes in the last five releases, is in
[`configuration.md`](configuration.md). The table is generated from
`config/variables.toml` by `just config-docs` and CI fails on drift (MAPPS-1027).
`just config-since <running version>` lists the changes to review before an
upgrade.

The visitor addresses their Company from the URL path
(`/portal/{portal_id}/...`), so `MOKOSH_PORTAL_HOST` needs one A or AAAA record
and a certificate covering that single host. It replaced the per-MSP subdomain
variable `MOKOSH_PORTAL_HOST_SUFFIX`, which required a DNS wildcard and a
matching TLS SAN (MAPPS-649).

The `MOKOSH_BRAND_*` variables, and the static files an operator mounts
alongside them, are [`deployment-branding.md`](deployment-branding.md).

## Ingress and cookies

The reference compose file binds `127.0.0.1:8080` and expects a TLS reverse
proxy in front, so that cookies set by mokosh-server can use `Secure` and
browsers stop rejecting mixed-scheme redirects.

The SPA and the API have to share a parent domain, for example
`msp.example.com` and `api.example.com` under `.example.com`, or the OIDC
session cookie is not sent on cross-origin API calls. mokosh-server's
`COOKIE_DOMAIN` is the matching setting.

## Applying an update

Bump the tag in `compose.yml` and run
`docker compose pull && docker compose up --detach`. The SPA is stateless, so
there is no migration step and no downtime window beyond the container restart.

Open tabs do not need a hard refresh. The config shim carries the build
revision, the running bundle polls it, and a redeploy raises a reload prompt and
reloads at the next safe boundary. What each banner means, and how the version
comparison against the server works, is [`versioning.md`](versioning.md).
