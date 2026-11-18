# ADR 0001: Feedback hub identity and integration

- **Status:** Accepted with open items (listed at the end), 2026-11-18. Epic #270, issue #271.
- **Scope:** how several products send feedback into one Rungu instance and how Rungu decides who a request acts as. The widget, the TypeScript client and the UI redesign are out of scope and follow in a later epic.

## Context

Rungu is meant to be the central place where more than one product collects feedback. Each product is its own Rungu project, and a person who uses two products is one person in Rungu.

Today:

- The only integration credential is `RUNGU_API_KEY`. It works on every project and always acts as the built-in `agent@rungu.local` admin, so rows an integration creates carry no real author.
- `find_or_create_user` finds or creates a user by email and gives the admin role to any address in `ADMIN_EMAILS`, **including when it creates the row**. That is safe while the email comes from a provider that verified it. It is not safe if a product can simply claim an email.
- `Operations` takes global ids (`post_id`, `comment_id`, `attachment_id`, `webhook_id`) and an `Actor { id, role }`. Nothing in it knows about a project scope.
- `/mcp` accepts access tokens from an OIDC issuer (resource-server mode, `RUNGU_MCP_OAUTH_ISSUER`). Web login supports a generic OIDC provider (`OIDC_ISSUER`, with PKCE); Google, GitHub and Keycloak login do not use PKCE. The REST API accepts neither an issuer token nor a per-project credential.
- Any `Authorization: Bearer` value that is not `RUNGU_API_KEY` is an immediate 401 and never falls through to cookies.
- Session tokens are signed with `APP_SECRET`, last 7 days and cannot be revoked.
- Rungu sends email notifications, so an email address stored for a user can receive mail.

Other feedback tools solve the same problem with signed tokens. Canny's SSO token is a JWT (HS256) carrying `email`, `id`, `name` and an optional `exp`, and its REST API creates a post for an `authorID` while an admin is recorded as `byID`. Featurebase moved from an HMAC `userHash` to JWTs so that every attribute is signed and tokens expire. The shape below follows the same idea, with differences: Rungu prefers a token from the user's own issuer, and it limits what an asserted email can do.

## Decision

1. **One instance, one project per product.** The hub is a single self-hosted Rungu instance. Rungu stays open source and single-tenant: no per-customer isolation, no billing.
2. **Identity is the email.** An integration must tell Rungu who the user is by email. Rungu resolves it with find-or-create. Numeric ids from other systems are not identities; an `external_id` is a hint stored for matching and idempotency only.
3. **Sign-in goes through an external OIDC issuer.** CodeCora uses Gatehouse. The Rungu code uses only OIDC and RFC behaviour and has no issuer-specific branches. Rungu does not become an OAuth authorization server for other services. Its own session and widget credentials are Rungu-internal, like the existing session cookie.
4. **Four ways to act as a user** (the identity paths below). Paths A to C carry an email the issuer verified. Path D carries an email the product claims.
5. **A claimed identity is limited.** An identity that only an integration vouches for is scoped to that integration's project and never gives admin rights or a reserved identity.
6. **Email normalisation is trim and lowercase,** applied inside the store function that resolves users, so every entry point inherits it. Nothing more.
7. **No mail to unverified addresses.** Rungu sends no notification email to an address that no issuer or OAuth provider has verified.
8. **An issuer token on REST is a member, never an admin.** It is forwarded through other products' backends and is not bound to a project.

## Identity paths

| Path | Caller | Credential | Email proof |
|---|---|---|---|
| A. Web login | Browser | Generic OIDC with PKCE, or Google, GitHub or Keycloak; then the Rungu session cookie | `email_verified` from the provider |
| B. Product backend | Server of a product | The user's access token from the issuer, audience = the Rungu API | Verified by the issuer |
| C. Widget | Browser inside a product | Not decided here, see Path C | Verified by the issuer |
| D. Server job without a user token | Batch import, ticket sync, events | Per-project integration key plus an acting email | Claimed by the product |

Path B is preferred whenever the product has the user's token. Path D exists because the issuer used by CodeCora supports only the authorization-code and refresh-token grants: there is no client-credentials grant, so a job with no user present has no token to forward.

## Credential dispatch

Four kinds of Bearer value can arrive on the same header, so the order is fixed and there is no fall-through once a kind is chosen:

1. A value equal to `RUNGU_API_KEY` (constant-time comparison) is the local agent, as today.
2. A value starting with `rgk_` is an integration key (path D). It is never tried as a token.
3. A value with three dot-separated segments is a JWT. It is an issuer token (path B) when issuer auth for REST is enabled. Otherwise it is refused.
4. Anything else is refused.

A wrong value of the chosen kind is a 401 and is not retried as another kind. Cookies are consulted only when there is no `Authorization` header.

## Path B

- The token is a JWT verified against the issuer's JWKS: signature, `iss`, `aud`, `exp` and `nbf`; RS and ES algorithms only; DPoP-bound and opaque tokens refused. This is the check `/mcp` performs.
- The audience is a separate setting for the REST API, `RUNGU_API_RESOURCE_URL`. A token whose audience is only `/mcp` is not valid for REST and the other way round. A token that names both resources is valid on both, so the issuer must mint one audience per token. Issuer auth for REST is **opt-in**: it is enabled only when the issuer and this REST audience are both set, and the value must be an https URL without query, fragment or credentials.
- Email comes from the token or, if the token has none, from `userinfo`. `email_verified: true` is required. REST does **not** inherit the `/mcp` opt-out for issuers that omit the claim.
- The user is resolved as for web login, so `ADMIN_EMAILS` still decides the stored role. The request itself runs as `member`.
- A token is valid for every project, because the audience names Rungu, not a product. The product that holds it can act as that user anywhere in Rungu as a member. This is per-user trust and is accepted; an optional allowed-client list is an open item.
- Revocation: a JWT stays valid until `exp`. When the token has no email, Rungu asks `userinfo` at most every 60 seconds, so a banned user is refused within about a minute. The remembered identity is shared with `/mcp`. When the token carries an email claim, it is valid until `exp`. Operators should keep access tokens short (one hour or less). Introspection is an optional hardening and an open item.

## Path C

Direction only. The widget is expected to use a popup sign-in on Rungu's own origin so that no third-party cookies are needed. It **must not be built** until its own ADR (0002) decides: how the allowed embedding origins are checked per project, the `postMessage` target and source checks, the token format, signing key and audience, where the token is kept (memory only), whether `jti` is checked against a replay store, how renewal works without third-party cookies, and framing protection on the sign-in page.

## Path D

- **Integration key.** Format `rgk_<random>` with at least 256 bits of randomness, shown once, stored as a SHA-256 digest and compared in constant time (the approach used for `RUNGU_API_KEY`). A key belongs to one project and has scopes: `submit`, `vote`, `comment`, `read`. A project can have more than one active key so keys can be rotated with an overlap. Keys record their last use, can be revoked, and appear in logs only by a short prefix.
- **Acting user.** Every request names the user in headers: `Rungu-On-Behalf-Of: <email>` and optionally `Rungu-External-Id: <id>`. A header works for every endpoint, including bodyless ones such as a vote, and for MCP, and it is read before any handler runs.
- **Provisional users.** A user created from a claimed email is created by a store method that never assigns admin (role is always `member`) and is marked `provisional`. The flag is cleared by the first sign-in verified by an issuer or provider. Provisional users receive no notification email.
- **Rate limit.** Requests made with a key are limited per key.
- **Idempotency.** Create requests accept an `Idempotency-Key` (#277). `external_id` is unique per project.

## Trust rules

Each rule has a test.

1. **Never admin.** A request acting through an integration key runs as `member`, even when the email is in `ADMIN_EMAILS` and even when the stored user is an admin. Users are created through the provisional-user method described under Path D, so an `ADMIN_EMAILS` address that has never been seen is created as a `member`, not an admin. The stored role is never changed by such a request.
2. **Never reserved.** `agent@rungu.local` and `mcp@rungu.local` are refused as an acting email on every path. A person whose issuer-verified email is one of them is refused sign-in, as today. The check stays in one function.
3. **Project scope lives in `Operations`.** The acting identity carries an optional constraint (project and scopes). Every `Operations` method that takes an id loads the resource first and refuses it when it belongs to another project. This covers `update_post`, `delete_post`, `toggle_vote`, `create_comment`, `delete_comment`, `set_official_response`, `check_can_upload`, `upload_attachment`, `delete_attachment`, and the id-addressed reads in `BoardQuery` (a post, its comments, attachments and similar posts). Admin routes, project management and webhooks need admin and are therefore never reachable with a key. REST and MCP both inherit the check, and the parity tests exercise it.
4. **Scope check.** An action outside the key's scopes is a 403.
5. **Proof level is recorded.** Every write notes whether the actor was proven (`oidc`) or claimed (`asserted`) and which key was used. Posts and comments created through a key expose that `source` in the API so a client can show `via <product>`. Showing it in the UI is part of the next epic.
6. **Email change.** An OIDC identity is stored in `user_identities` as `provider = "oidc:<issuer>"` and `provider_id = <sub>`; no schema change is needed, but the identity write must stop being `ON CONFLICT DO NOTHING` and be looked up first. A known `(issuer, sub)` with a new verified email updates that user's email. The old email is **not** an alias and does not authenticate; the change goes to the audit log. A new email that already belongs to another user is a conflict (409 for tokens, an error page for web login), never a merge.

## Threats and how they are handled

| Threat | Handling |
|---|---|
| A leaked integration key | Project and scope limits, never admin, rate limit per key, revocation, last-use tracking, audit log. The damage is spam and impersonation as a member of one project. |
| A product claims an `ADMIN_EMAILS` address | Trust rule 1, including the creation case. |
| A product claims a victim's email and posts in their name | Rows carry `source` and a proof level. The user is `provisional`, so no mail goes to that address. The first verified sign-in of that email takes over the account, so the person can then see and delete those rows. Listing such rows to the person at first sign-in is an open item. |
| Claimed emails used to enumerate or spam addresses | No mail to unverified addresses (Decision 7), rate limit per key, audit log. |
| Two people merged by aggressive normalisation | Only trim and lowercase. Gmail dot and plus rules are not applied. |
| Stolen web session after a ban or logout at the issuer | Back-channel logout and a per-user session version (#275). |
| A banned user's issuer token on REST | Within about 60 seconds when `userinfo` is used; until `exp` when the token carries an email. Short token lifetimes are required; introspection is optional. |
| A user's token used by another product's backend | Accepted per-user trust, capped at `member`, optional allowed-client list as an open item. |
| Token for one resource used on another | Separate audiences for `/mcp` and REST, tested both ways. A token that names both is valid on both, so the issuer must mint one audience per token. |
| A role change hidden by the 60-second actor cache | Documented. Role changes through `ADMIN_EMAILS` apply on the next resolution. |
| A freed email address reissued to someone else | Aliases do not authenticate (rule 6), and an email change is applied only for a verified email. |

## Consequences

- Path D can attribute rows to a person but cannot prove that person exists. Operators who need proof should use path B.
- Rungu holds personal data (email) from several products in one database, so deletion and export per email are needed. They are tracked as a follow-up to this epic.
- `RUNGU_API_KEY` stays for local agents and the stdio MCP transport and still acts as an admin. It should be used only over TLS and kept out of product backends; products get integration keys.
- The REST and MCP parity tests must keep passing. A rule that applies to an acting identity is written once, in `Operations`, and both transports inherit it.
- Operators of the issuer need to: register Rungu as a client, add the Rungu API as an allowed resource, link each product's client to that resource, and register Rungu's back-channel logout URL.

## Alternatives considered

- **Keep one global API key.** Rejected: no attribution, no scoping, one leak affects every project.
- **A per-user API key.** Rejected: products would store a Rungu secret per user, which adds a second login system next to the issuer.
- **HMAC of the email (`userHash`).** Rejected: it proves only the email, has no expiry and cannot protect other attributes. Featurebase moved away from it.
- **Token exchange or client credentials at the issuer.** Rejected for now: the issuer does not support them. Path D covers the gap, and this ADR is revisited if the issuer adds either grant.
- **One Rungu instance per product.** Rejected: it defeats a central hub and a per-email view across products.
- **A realm or tenant per product.** Rejected: it duplicates users and brings back multi-tenancy, which Rungu has ruled out.
- **Strip dots and plus tags from emails.** Rejected, see the threat table.
- **Acting email in the request body.** Rejected: it would need every handler to read it and cannot cover bodyless requests or MCP.

## Open items

Each item names the constraint its decision must meet.

| Item | Decided in | Constraint |
|---|---|---|
| Optional allowed-client list per project for path B (`azp`) | follow-up | Off by default; enforced in the same verifier |
| Introspection for path B | follow-up | Optional; short cache; must not be required for startup |
| Meaning of the `read` scope | #273 | A key never reaches admin routes; public reads need no key |
| Listing provisional rows to a person at first sign-in | next epic | Must allow deleting them |
| Showing `source` in the UI | next epic | The API field exists first |
| Path C widget | ADR 0002 | See Path C |

## Implementation

| Issue | Change |
|---|---|
| #272 | Issuer Bearer tokens on the REST API (path B) |
| #273 | Per-project integration keys with `Rungu-On-Behalf-Of` (path D) |
| #274 | Claimed emails never grant admin or reserved identities; project scope in `Operations` |
| #275 | Back-channel logout and session revocation, which also closes #164 |
| #276 | Email change detection with `(issuer, sub)` |
| #277 | Audit log and `Idempotency-Key` |

## References

- OpenID Connect Back-Channel Logout 1.0
- RFC 8707 (resource indicators), RFC 9728 (protected resource metadata), RFC 7662 (token introspection)
- Canny developer docs: single sign-on tokens and the posts API (`authorID`, `byID`)
- Featurebase help center: migration from identity verification to JWT authentication
