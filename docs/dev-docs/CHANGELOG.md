# Changelog

Internal, name-free history for mokosh-apps. Retired point-in-time docs (milestone handoffs, audit reports) are distilled here so the tree keeps only forward-useful reference material. `codebase-state.md` was kept rather than folded in, and is itself a retired point-in-time audit as of MAPPS-540: it records a 2026-05-06 walk and is not maintained. Entries are newest-first and vary in depth.

## 2026-07-01 - Docs reorganization and history sanitization

- Markdown consolidated under `docs/` (public / how-to) and `docs/dev-docs/` (internal working notes); `README.md` stays at the repo root, README files stay colocated with their code. The two source comments that pointed at moved docs were repointed.
- Retired audits and the 2026-05 milestone-1 handoff were removed and distilled into the entries below. `codebase-state.md` was kept (it carries the F1..F19 ids that source comments cite) and moved to `docs/dev-docs/`. MAPPS-540 later reframed it as the historical record it always was.
- The local-only `For AI/` scratch directory stays gitignored. Contributor names were stripped from commit-message text in a companion history rewrite; git author/committer fields were left unchanged.

## 2026-06 - Client form-validation sweep (distilled)

An audit swept the client's form fields and catalogued which inputs lacked inline validation (empty-required, format, range) versus which already rejected bad input, recording the fix for each so the New Company / ticket / billing forms give consistent inline feedback instead of silent acceptance or full-page errors.

## 2026-06-06 - Multi-repo platform audit (distilled)

A file-granularity audit read every source file across bunyip, mokosh-apps, and mokosh-server (~580 files) with git-forensic reconstruction and a cross-repo contract check on the apps-to-server boundary. High and critical findings were adversarially verified against live source before scoring. Nothing was modified by the audit; findings were filed as remediation issues.

Baseline: 409 total findings (3 critical, 37 high, 197 medium, 172 low; 11 rejected on verification), spanning 146 correctness, 19 cross-repo contract-drift, 82 "too many cooks", 131 dead/unused, and 31 infra/CI.

Headline findings that concerned mokosh-apps:

- Filtered time-entry list 500 (critical): the count-query shared a WHERE clause whose placeholders started past what the count statement bound, so any filtered time-entry list errored. Fix: build a separate count-query placeholder index.
- Timer / entry IDOR (high): stop-timer and entry update/delete scoped only by tenant and id, with no owner check, so any tenant user could stop or tamper a colleague's timer. Fix: thread the user id and scope by it, exempting admins.
- Pagination double-direction (high): the order-by helper appended a direction to a default field that already embedded one, so the no-sort path emitted `... DESC DESC`. Fix: strip the direction from the default-field arguments.
- Dead code from the bunyip-OIDC pivot: an orphaned federated-login hook module and several unused fetch/pagination helpers and login-flow helpers were left behind and flagged for removal.

Cross-cutting themes (duplicated helpers, parallel paginated-envelope types, the count-query family recurring without filtered-list tests) and the full per-finding detail lived in the audit tree that this entry replaces; the remediation issues carry the actionable items forward.

## 2026-05 - Milestone 1: foundation (distilled)

Milestone 1 stood up mokosh-apps as the cross-platform Dioxus client for the Mokosh platform, served as a static WASM bundle behind Caddy, talking to mokosh-server over `/api/v1`. Several pages began as stub/demo lists and were progressively wired to the live backend; per-page status and the running fix list (`F1..F19`) are tracked in `docs/dev-docs/codebase-state.md`.
