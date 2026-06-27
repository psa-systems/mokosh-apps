# SPA rollout runbook: one build behind the LB at a time

How to roll the `mokosh-www` SPA so the load balancer never serves two
different builds at once. Written for MAPPS-315 ("two mokosh-www builds
served from the same LB - sidebar / Settings inconsistency on rollout").

## TL;DR

- Pin a versioned image tag (`:vX.Y.Z`) per deploy in the deployment
  repo, never `:latest`, so a rollout flips every replica to the same
  digest in lockstep.
- After the deploy, confirm `GET /_mokosh_config.js` returns the same
  `build_sha` from every replica (curl the public URL N times).
- The `/settings` misroute and the missing/extra `ADMIN` sidebar
  section are symptoms of the skew, not separate bugs: once a single
  build serves every request they disappear with no further code
  change.

## Why two builds can serve at once

The SPA image bakes its build provenance into `_mokosh_config.js` at
container start. [`oci-build/entrypoint.sh`](../oci-build/entrypoint.sh)
emits a `build_sha` field from the image's `GIT_SHA` build arg, and
[`oci-build/Caddyfile`](../oci-build/Caddyfile) serves that file
`Cache-Control: no-cache, no-store, must-revalidate` so browsers always
read the live value.

An open tab polls that file in the background
([`src/hooks/update_check.rs`](../src/hooks/update_check.rs), started
from [`src/main.rs`](../src/main.rs)) and reloads at the next safe
boundary when `build_sha` changes. That auto-reload is designed for a
clean cutover, where every replica reports the same `build_sha`.

The skew appears when the LB fronts more than one replica and those
replicas pull a floating tag (`:latest`):

1. A new release pushes a new digest to `:latest`.
2. A rolling restart re-pulls and restarts one replica; it now serves
   the new digest. The other replica still runs the old digest until
   its own restart.
3. For the window between the two restarts, the LB hands some requests
   to the new build and some to the old build. The poller sees
   `build_sha` flip back and forth on every request and the SPA can
   reload into a loop.

Because the two builds differ, navigation differs per request: the
older build in the captured session showed no `ADMIN` section and
misrouted `/settings`, while the newer build showed the full `ADMIN`
group (Team, Audit Log, SLA Management, Settings) and routed
`/settings` correctly. The `/settings` route itself is declared in the
router, so the misroute is purely "an old bundle is still being
served," not a missing route.

## The fix: pin a versioned tag per deploy

The SPA is stateless, so the only thing a rollout has to guarantee is
that every replica lands on the same digest before traffic is split
across them. Pin a concrete `:vX.Y.Z` tag (a release tag resolves to a
single immutable digest) instead of `:latest`.

Where the pin lives: the per-host compose for the LB-fronted hosts is
in the deployment repo `dev.a8n.run/NiceGuyIT/docker`, not in this
`mokosh-apps` source repo. See `dev-docs/milestone-1-handoff.md` for
the host map. The relevant files are:

- `docker/server/c-01/mokosh-apps/compose-variables.yml` (the host
  flagged in MAPPS-315 as still on `:latest`).
- `docker/server/nc-01/mokosh-apps/compose-variables.yml` (already
  pins a versioned tag; mirror its shape).

In that file set the image to the release you intend to ship, e.g.:

```yaml
mokosh_apps_image: dev.a8n.run/psa-systems/mokosh-www:v0.2.0
```

The image is built and tagged by
[`.forgejo/workflows/build-oci-image.yml`](../.forgejo/workflows/build-oci-image.yml)
(private registry, mirrored to the public owner) and the release tag
`vX.Y.Z` is created by
[`.forgejo/workflows/create-release.yml`](../.forgejo/workflows/create-release.yml)
after the `just create-release` PR merges. See
[`versioning.md`](versioning.md) for how the displayed version and the
git tag relate.

`:latest` self-hosting guidance for the reference compose lives in
[`oci-build/compose.example.yml`](../oci-build/compose.example.yml),
which already documents pinning `:vX.Y.Z` "for predictable rollouts."
The single-replica self-host case never sees the skew; this runbook is
about the multi-replica, LB-fronted case.

## Rollout sequence (lockstep)

Run these against the deployment repo / host. They pull a single pinned
digest, so even a non-atomic restart lands both replicas on the same
build.

1. Bump the pin in the deployment repo's
   `docker/server/<host>/mokosh-apps/compose-variables.yml` to the
   target `:vX.Y.Z` and merge it.
2. On the host, pull the pinned digest for every replica before
   restarting any of them:

   ```bash
   cd /opt/docker/<host>
   git pull
   docker compose pull mokosh-apps
   ```

   `docker compose pull` fetches the one digest the pin resolves to, so
   both replicas now have the identical image cached locally.
3. Bring the replicas up on the new image:

   ```bash
   docker compose up --detach mokosh-apps
   ```

   Because both replicas resolve the same pinned digest, the order they
   restart in no longer matters: there is only one digest to serve.
4. If the deployer restarts replicas one at a time behind the LB,
   that is fine here precisely because step 2 already pulled the same
   digest to both. The failure mode this runbook prevents is two
   replicas resolving two different digests, which only happens with a
   floating tag.

## Verify a single build is serving

After the deploy, confirm every replica reports the same `build_sha`.
Hit the public URL repeatedly so the LB spreads the requests across
replicas:

```bash
for i in $(seq 1 20); do
  curl --silent https://msp.a8n.systems/_mokosh_config.js \
    | grep --only-matching 'build_sha[^,}]*'
done | sort | uniq -c
```

Expected: a single distinct `build_sha` line. More than one means a
replica is still on the old digest (its `docker compose up` did not
land, or a stray replica is still on `:latest`); re-run the pull + up
on that replica.

Cross-check the value against the release you pinned: it must be the
12-char git SHA of the `:vX.Y.Z` build, not the previous one.

## Acceptance-criteria mapping (MAPPS-315)

- AC1 (pin a specific tag / digest, not `:latest`): executed in the
  `NiceGuyIT/docker` deployment repo per "The fix" above. Tracked under
  this issue; the edit is in that repo because the per-host compose
  lives there, not in `mokosh-apps`.
- AC2 (every replica returns the same `build_sha` after a deploy):
  verified by the curl loop above.
- AC3 (runbook documenting the SPA-rollout sequence): this document.
- AC4 (`/settings` renders in every served build): no code change. The
  route is already declared; the misroute was the stale build being
  served, so it evaporates once a single build serves every request.
