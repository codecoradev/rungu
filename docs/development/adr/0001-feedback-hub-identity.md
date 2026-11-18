# ADR 0001: Feedback hub identity and integration

- **Status:** Accepted, 2026-11-18 (owner decision). Tracked in epic #270, issue #271.
- **Scope:** how several products send feedback into one Rungu instance and how Rungu decides who a request acts as. It does not cover the widget, the TypeScript client or the UI redesign; those follow in a later epic.

## Context

Rungu is meant to be the central place where more than one product collects feedback. Each product is its own Rungu project, and a person who uses two products is one person in Rungu.

Today:

- The only integration credential is `RUNGU_API_KEY`. It works on every project and always acts as the built-in `agent@rungu.local` admin, so rows an integration creates carry no real author.
- `find_or_create_user` finds or creates a user by email and promotes any address in `ADMIN_EMAILS` to admin. That is safe while the email comes from an OAuth provider that verified it. It is not safe if a product can simply claim an email.
- `/mcp` already accepts access tokens from an OIDC issuer (resource-server mode, `RUNGU_MCP_OAUTH_ISSUER`), and web login supports a generic OIDC provider (`OIDC_ISSUER`). The REST API accepts neither an issuer token nor a per-project credential.
- Session tokens are signed with `APP_SECRET`, last 7 days and cannot be revoked.

Other feedback tools solve the same problem with signed tokens. Canny's SSO token is a JWT (HS256) carrying `email`, `id`, `name` and an optional `exp`, and its REST API creates a post for an `authorID` while an admin is recorded as `byID`. Featurebase moved from an HMAC `userHash` to JWTs so that every attribute is signed and tokens expire. The shape below follows the same idea, with two differences: Rungu prefers a token from the user's own issuer, and it limits what an asserted email can do.

## Decision

1. **One instance, one project per product.** The hub is a single self-hosted Rungu instance. Rungu stays open source and single-tenant: no per-customer isolation, no billing.
2. **Identity is the email.** An integration must tell Rungu who the user is by email. Rungu resolves it with find-or-create. Usernames and numeric ids from other systems are not identities; an `external_id` may be stored as a hint only.
3. **Sign-in goes through an external OIDC issuer.** Rungu never issues access tokens for other services. CodeCora uses Gatehouse as the issuer. The Rungu code uses only OIDC and RFC behaviour and has no issuer-specific branches.
4. **Four ways to act as a user** (the identity paths below). Paths A to C carry an email the issuer verified. Path D carries an email the product claims.
5. **Asserted identity is limited.** An identity that only an integration vouches for is scoped to that integration's project and never gives admin rights or a reserved identity.
6. **Email normalisation is trim and lowercase.** Nothing more.

## Identity paths

| Path | Caller | Credential | Email proof | Notes |
|---|---|---|---|---|
| A. Web login | Browser | OIDC authorization code with PKCE, then the Rungu session cookie | `email_verified` from the issuer | Exists (#268). Gains back-channel logout (#275). |
| B. Product backend | Server of a product | The user's access token from the issuer, audience = the Rungu API | Verified by the issuer, email read from `userinfo` | Reuses the `/mcp` verification for REST (#272). |
| C. Widget | Browser inside a product | Popup sign-in on Rungu's own origin, then a short-lived Rungu token as Bearer | Verified by the issuer | Designed here, built in the next epic. No third-party cookies are needed. |
| D. Server job without a user token | Batch import, ticket sync, events | Per-project integration key plus an acting email | Claimed by the product | Marked `asserted` (#273, #274). |

Path B is preferred whenever the product has the user's token. Path D exists because the issuer used by CodeCora supports only the authorization-code and refresh-token grants: there is no client-credentials grant and no token exchange, so a job with no user present has no token to forward.

### Path B details

- The issuer token is a JWT verified against the issuer's JWKS: signature, `iss`, `aud`, `exp` and `nbf`; RS and ES algorithms only; DPoP-bound and opaque tokens refused. This is the check `/mcp` already performs.
- The audience is a separate setting for the REST API (name decided in #272). A token issued for `/mcp` is not valid for REST and the other way round.
- The access token carries no email. Rungu reads `email` and `email_verified` from `userinfo` and requires `email_verified: true`. Results are cached briefly per `sub`.
- The role comes only from `ADMIN_EMAILS`, as for web login.

### Path C details

- The widget opens a popup to Rungu's own sign-in, so the session cookie is first-party. The popup returns a Rungu token to the widget with `postMessage`, restricted to the embedding origin allowed for that project.
- The token lives at most 10 minutes, is bound to one project and one origin, and has a `jti`. The widget renews it by opening the popup silently while the Rungu session is valid.

### Path D details

- **Integration key.** One key belongs to one project and has scopes: `submit`, `vote`, `comment`, `read`. The secret is random with at least 256 bits of entropy, shown once, and stored as a SHA-256 digest compared in constant time (the same approach as `RUNGU_API_KEY`). Keys can be revoked, record their last use, and appear in logs only by prefix.
- **Acting user.** The request names the user it acts for: `on_behalf_of: { email, name?, external_id? }`. The exact carrier for requests without a JSON body (a vote, for example) is decided in #273.
- **Idempotency.** Create requests accept an `Idempotency-Key` (and `external_id`) so a retrying product does not create duplicates (#277).

## Trust rules

These rules are the reason for the design and each has a test.

1. **Never admin.** A request acting through an integration key runs as `member`, even when the email is in `ADMIN_EMAILS` and even when the stored user is already an admin. The stored role is not changed by such a request.
2. **Never reserved.** `agent@rungu.local` and `mcp@rungu.local` are refused as an acting email on every path. The check stays in one function and is extended if more built-in identities are added.
3. **Project scope.** A key acts only inside its own project. Any resource that belongs to another project is refused, including posts addressed by id.
4. **Scope check.** An action outside the key's scopes is a 403.
5. **Proof level is recorded.** Every write notes whether the actor was proven by the issuer (`oidc`) or asserted by a key (`asserted`), and which key. This goes into the audit log (#277).
6. **No silent merges.** If a known issuer subject arrives with a different email, Rungu updates that user's email, keeps the old one as an alias and logs the change. If the new email already belongs to someone else, that is a conflict, not a merge (#276).

## Threats and how they are handled

| Threat | Handling |
|---|---|
| A leaked integration key | Project and scope limits, never admin, revocation, last-use tracking, audit log. The damage is limited to spam and impersonation as a member of one project. |
| A product claims the owner's email to become admin | Trust rule 1. |
| A product claims a victim's email and posts in their name | The acting identity is `asserted` and recorded with its key. The first verified sign-in of that email takes over the account, so the person can see and delete what was posted. Showing the source on such posts is considered for the admin UI in the next epic. |
| Two people merged by aggressive normalisation | Only trim and lowercase. Gmail dot and plus rules are not applied, because they are provider rules and would join different people at other providers. |
| Stolen session after a ban or logout at the issuer | Back-channel logout and a per-user session version (#275). |
| Token for one resource used on another | Separate audiences for `/mcp` and the REST API. |
| Replay of a widget token | Short lifetime, `jti`, origin and project binding. |

## Consequences

- Path D can attribute rows to a person but cannot prove that person exists. Operators who need proof should use paths B or C.
- Rungu holds personal data (email) from several products in one database, so deletion and export per email are needed. They are not part of this epic and are tracked as a follow-up.
- `RUNGU_API_KEY` stays for local agents and the stdio MCP transport. It is no longer the integration mechanism for products.
- The REST and MCP parity tests must keep passing. A capability that gains an acting-user rule gets the rule once, in `Operations`, and both transports inherit it.
- Documentation for operators of the issuer is needed: register Rungu as a client, add the Rungu API as an allowed resource, link each product's client to that resource, and register Rungu's back-channel logout URL.

## Alternatives considered

- **Keep one global API key.** Rejected: no attribution, no scoping, one leak affects every project.
- **A per-user API key.** Rejected: products would need to create and store a Rungu secret per user, and it adds a second login system next to the issuer.
- **HMAC of the email (`userHash`).** Rejected: it proves only the email, has no expiry and cannot protect other attributes. Featurebase moved away from it for the same reasons.
- **Token exchange or client credentials at the issuer.** Rejected for now: the issuer does not support them. Path D covers the gap, and this ADR is revisited if the issuer adds either grant.
- **One Rungu instance per product.** Rejected: it defeats the point of a central hub and a per-email view across products.
- **A realm or tenant per product.** Rejected: it duplicates users per product and brings back multi-tenancy, which Rungu has ruled out.
- **Strip dots and plus tags from emails.** Rejected, see the threat table.

## Implementation

| Issue | Change |
|---|---|
| #272 | Issuer Bearer tokens on the REST API (path B) |
| #273 | Per-project integration keys with `on_behalf_of` (path D) |
| #274 | Asserted emails never grant admin or reserved identities |
| #275 | Back-channel logout and session revocation, which also closes #164 |
| #276 | Email change detection with `(issuer, sub)` |
| #277 | Audit log and `Idempotency-Key` |

## References

- OpenID Connect Back-Channel Logout 1.0
- RFC 8707 (resource indicators), RFC 9728 (protected resource metadata), RFC 7662 (token introspection)
- Canny developer docs: single sign-on tokens and the posts API (`authorID`, `byID`)
- Featurebase help center: migration from identity verification to JWT authentication
