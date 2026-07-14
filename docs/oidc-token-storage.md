# OIDC token storage: accepted-risk decision (MAPPS-362)

Status: **Accepted risk** (SPA public-client model kept). Severity: LOW. Owner: mokosh-apps. Last reviewed: MAPPS-362.

This records a deliberate security tradeoff and the decision taken, so it is explicit and owned rather than implicit.

## What we store, and where

mokosh-apps is a Dioxus WASM **public** OIDC client (Authorization Code + PKCE against the bunyip OP). After a successful login the token bundle is written to `sessionStorage`:

- `StoredTokens { access_token, id_token, refresh_token, expires_at, scope }` under key `mokosh_auth_bundle_v1` (`src/modules/oidc/storage.rs`). `sessionStorage`, not `localStorage`: the bundle is tab-scoped and cleared when the tab closes, which matches the OP session-cookie lifetime and avoids the cross-tab leak `localStorage` would add.
- The short-lived code-flow state (`PendingFlow`: PKCE verifier + state + nonce + `return_to`) is written under `mokosh_oidc_flow_v1` only between `start_login` and `complete_login`.
- The ID token is decoded **without signature verification** in the browser (`IdTokenClaims::parse_unverified`, `src/modules/oidc/tokens.rs:59`) purely to read display claims. This is an accepted SPA pattern: the token arrives over TLS directly from the issuer, and the backend independently re-validates the access token on every API call, so the browser never trusts the ID token for authorization.

## The risk

Web storage (and WASM memory) is readable by any script running in the origin. So:

- An XSS foothold in the SPA can exfiltrate all three tokens (access, ID, refresh).
- The OAuth 2.0 Security BCP specifically discourages keeping **refresh** tokens in browser storage.

XSS is the prerequisite for this risk; the SPA's XSS defenses (CSP, no untrusted HTML injection, framework-escaped rendering) are the primary control. Given an XSS foothold, an attacker can already read the access/ID tokens out of WASM memory, so writing them to `sessionStorage` adds little marginal exposure for those two; the refresh token is the one item whose browser custody is genuinely the OAuth-BCP concern.

## Mitigations in place

- **Short access-token TTL** so an exfiltrated access token has a small window.
- **Server-side refresh-token rotation with family-reuse detection** on the OP (bunyip): a stolen refresh token, once the legitimate client rotates, invalidates the whole token family, so silent reuse is detected and the session is killed.
- **TLS-only token delivery** direct from the issuer; **PKCE** on the code exchange.
- **`nonce` validation** on the ID token (`complete_login` compares the echoed `nonce` to the stored `PendingFlow.nonce`; mismatch is treated as replay, `src/modules/oidc/flow.rs:25`) and **`return_to` sanitization** before redirect. (mokosh-apps does both, unlike the sibling Drillmark SPA, tracked separately.)
- **`sessionStorage`, not `localStorage`** (tab-scoped, no cross-tab persistence).
- **Backend re-validates the access token** on every request; the browser-decoded ID token is display-only.

## Decision

**Keep the SPA public-client model and accept this risk.** Rationale: it is the standard architecture for a public browser SPA with no backend session layer of its own; the residual exposure is gated behind an XSS foothold (defended separately) and bounded by the short access-token TTL plus OP-side rotation/reuse-detection; and the data reached with these tokens is the same tenant-scoped PSA data the user is already authorized for. The httpOnly-cookie BFF (below) is recorded as the migration option, **not mandated**.

Revisit this decision if the risk profile changes: markedly higher-value data, a compliance requirement (e.g. mandating no refresh token in the browser), or a change in XSS exposure.

## The deferred alternative: an httpOnly-cookie BFF

Move token custody to a Backend-For-Frontend: the backend completes the code exchange, holds the access + refresh tokens server-side, and hands the browser only an httpOnly + Secure + SameSite session cookie; the SPA calls the API through the BFF, which attaches the access token and performs refresh.

- Removes web-storage/JS exposure of the refresh **and** access tokens (the XSS-exfil surface for tokens goes away; a session cookie is httpOnly-unreadable by script).
- Cost: a stateful backend session layer + a per-request proxy/attach hop, and CSRF defense on the cookie session. It is a materially larger change than this doc.

This is the decision point held open by MAPPS-362; it is deferred, not chosen.

## Does the browser actually need the refresh token? (MAPPS-362 AC)

Yes, in the current architecture. The SPA performs its **own** silent renewal in the browser: `use_token_refresh` (`src/hooks/auth.rs:288`) reads the stored refresh token and calls `refresh_tokens` with `grant_type=refresh_token` (`src/modules/oidc/flow.rs:305`) to mint a fresh access token before expiry, and `offline_access` is requested (`src/modules/oidc/config.rs:36`) precisely to obtain that refresh token. There is no backend session to hold it on the SPA's behalf. Therefore the refresh token **cannot** be moved server-side without adopting the BFF above; browser custody is a property of the public-client model, and removing it is exactly the BFF migration. Confirmed: the browser-held refresh token is required by the current design, and the only way to relocate it is the deferred BFF option.
