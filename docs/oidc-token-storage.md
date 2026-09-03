# OIDC token storage: accepted-risk decision (MAPPS-362)

Status: **Accepted risk** (SPA public-client model kept). Severity: LOW. Owner: mokosh-apps. Last reviewed: MAPPS-362.

This records a deliberate security tradeoff and the decision taken, so it is explicit and owned rather than implicit.

## What we store, and where

mokosh-apps is a Dioxus WASM **public** OIDC client (Authorization Code + PKCE against the bunyip OP). After a successful login the token bundle is written to the session store:

- `StoredTokens { access_token, id_token, refresh_token, expires_at, scope }` under key `mokosh_auth_bundle_v1` (`src/modules/oidc/storage.rs`). `sessionStorage`, not `localStorage`: the bundle is tab-scoped and cleared when the tab closes, which matches the OP session-cookie lifetime and avoids the cross-tab leak `localStorage` would add.

  MAPPS-504 put that behind `crate::platform::store`, which is `sessionStorage` in the browser and an in-process map on the desktop build. The property this decision rests on is unchanged on either host: the bundle does not outlive the session, and it is never written to disk. Everything below is about the browser, which is where the risk lives; a desktop window has no other origin script to defend against.
- The short-lived code-flow state (`PendingFlow`: PKCE verifier + state + nonce + `return_to`) is written under `mokosh_oidc_flow_v1` only between `start_login` and `complete_login`.

  MAPPS-505 runs the same flow on the desktop, as an RFC 8252 native app: same public-client model, same PKCE, same `PendingFlow`, and the authorization response arrives on a listener bound to `127.0.0.1` for that one flow instead of in a URL. The listener is loopback-only and serves exactly one request, so it is not reachable from the network and is not a second place a token can rest.
- The ID token is decoded **without signature verification** in the browser (`IdTokenClaims::parse_unverified`, `src/modules/oidc/tokens.rs:56`) purely to read display claims. This is an accepted SPA pattern: the token arrives over TLS directly from the issuer, and the backend independently re-validates the access token on every API call, so the browser never trusts the ID token for authorization.

## What the stored bundle is, and what it is not (MAPPS-661)

The bundle is a **cache**. It is not, by itself, evidence that a session exists.

`sessionStorage` outlives a tab the browser unloads to reclaim memory and hands back on restore, and nothing in the bundle records whether the SSO session it was minted under has since ended. So a restored tab can hold a perfectly well-formed bundle for a session that is over, and rebuilding `AuthContext` from it (`rehydrate_from_storage` / `rehydrate_standalone`, `src/hooks/auth.rs`) says only "we have tokens", never "we have a session".

The two facts are therefore held apart. `AuthContext.confirmation` (`SessionConfirmation`) carries whether the identity provider has answered for this session during **this page lifetime**:

- both rehydrate paths produce `Unconfirmed`;
- a successful refresh-token rotation, or a 200 from `GET /api/v1/auth/me` (which mokosh-server answers only for a verified `at+jwt`), moves it to `Confirmed`;
- a rotation the OP refuses as `invalid_grant`, or a 401 the fetch layer cannot renew past, moves it to `Ended` and clears the store.

`confirm_restored_session` puts the question at mount rather than on the next poll tick, through the single-flight renewal in `src/hooks/fetch.rs` so it never opens a second flight against the same refresh token. While the answer is outstanding for a bundle whose access token is spent or inside the renewal window, the app renders its loading state and not the signed-in shell; a bundle still comfortably inside the short access-token lifetime renders immediately, because the OP issued that token moments ago and re-confirming it would cost a spinner on every ordinary navigation.

This is what the storage model rests on: an exfiltrated or merely outdated bundle is worth a request that the backend still independently validates, never a claim of identity the SPA makes on its own authority.

## The risk

Web storage (and WASM memory) is readable by any script running in the origin. So:

- An XSS foothold in the SPA can exfiltrate all three tokens (access, ID, refresh).
- The OAuth 2.0 Security BCP specifically discourages keeping **refresh** tokens in browser storage.

XSS is the prerequisite for this risk; the SPA's XSS defenses (CSP, no untrusted HTML injection, framework-escaped rendering) are the primary control. Given an XSS foothold, an attacker can already read the access/ID tokens out of WASM memory, so writing them to `sessionStorage` adds little marginal exposure for those two; the refresh token is the one item whose browser custody is genuinely the OAuth-BCP concern.

## Mitigations in place

- **Short access-token TTL** so an exfiltrated access token has a small window.
- **Server-side refresh-token rotation with family-reuse detection** on the OP (bunyip): a stolen refresh token, once the legitimate client rotates, invalidates the whole token family, so silent reuse is detected and the session is killed.
- **TLS-only token delivery** direct from the issuer; **PKCE** on the code exchange.
- **`nonce` validation** on the ID token (`complete_login` compares the echoed `nonce` to the stored `PendingFlow.nonce`; mismatch is treated as replay, `FlowError::NonceMismatch` in `src/modules/oidc/flow.rs`) and **`return_to` sanitization** before redirect. (mokosh-apps does both, unlike the sibling Drillmark SPA, tracked separately.)
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

Yes, in the current architecture. The SPA performs its **own** silent renewal in the browser: `use_token_refresh` (`src/hooks/auth.rs:596`) reads the stored refresh token and calls `refresh_tokens` with `grant_type=refresh_token` (`src/modules/oidc/flow.rs:397`) to mint a fresh access token before expiry, and `offline_access` is requested (`src/modules/oidc/config.rs:46`) precisely to obtain that refresh token. There is no backend session to hold it on the SPA's behalf. Therefore the refresh token **cannot** be moved server-side without adopting the BFF above; browser custody is a property of the public-client model, and removing it is exactly the BFF migration. Confirmed: the browser-held refresh token is required by the current design, and the only way to relocate it is the deferred BFF option.
