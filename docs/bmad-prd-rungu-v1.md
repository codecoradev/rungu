# Rungu PRD — Product Requirements Document
> BMAD Brownfield Analysis | Date: 2026-08-12
> Status: ✅ FINAL — Multi-Agent Discussion Synthesized
> Analyst: CTO | Discussion: 5-C-Level via Uteke Coordination (7-Round)

---

## 1. Executive Summary

**Product:** Rungu — Self-hosted, MCP-first feedback board for developer teams.
**Current:** v0.2.1 → **Target:** v0.3.0 (4-week rescoped sprint)
**Stack:** Rust (Axum 0.8) + SvelteKit 5 + SQLite/PostgreSQL
**License:** Apache-2.0
**Positioning:** "Self-hosted feedback board that talks to your AI agents"

**Strategic Role:** Ecosystem infrastructure — completes the CodeCoraDev DevOps loop
(TrapFall errors + Rungu feedback + Cora Code review). NOT a standalone revenue product.

### Multi-Agent Verdict Matrix
| Role | Score | Verdict | Key Condition |
|------|-------|---------|---------------|
| CTO | 7/10 | CONDITIONAL GO | Launch after P0 fixes (1 week) |
| CFO | 3/10 | NO MONETIZATION | Valid — ecosystem value, not revenue |
| CMO | 5.5/10 | WAIT 2-3 weeks | MCP wedge + Astuto orphans |
| CLO | 4/10 risk | LOW-MODERATE | Nothing blocks OSS. Trademark before cloud |
| COO | 7/10 | SHIP rescoped | 4 weeks, not 12. Engineering already solid |
| **FUSED** | **5.7/10** | **CONDITIONAL GO** | **4-week sprint → maintenance mode** |

---

## 2. Competitive Position

### Rungu's Unique Advantages (Verified)
1. **Only Rust-based** OSS feedback tool in existence
2. **Only MCP-first** (14 AI agent tools, production-ready — NOT stub despite README)
3. **Only Apache-2.0** (vs AGPL-3.0 competitors — commercial-friendly)
4. **Only dual database** (SQLite + PostgreSQL out of the box)
5. **CodeCoraDev ecosystem** (TrapFall + Cora Code integration)

### Market Gap: Astuto Orphaned
- Astuto (Ruby, 2,350 stars) **ARCHIVED Feb 2026**
- 2,350 users need a new home
- Fider (Go, 4,463 stars) is the only active OSS alternative
- **Rungu can capture Astuto migrants with a migration guide + script**

---

## 3. Scope: v0.3.0 (4-Week Sprint)

### IN SCOPE
- [ ] Fix README MCP stub disclaimer (WRONG — MCP is production-ready)
- [ ] Deploy live demo instance
- [ ] OpenAPI coverage 63% → 100%
- [ ] Webhook system (subscribe, deliver, retry, SSRF protection)
- [ ] E2E Playwright tests (~15 tests, 3 core flows)
- [ ] Astuto migration guide + script
- [ ] SECURITY.md + vulnerability disclosure
- [ ] MCP parity fixes (add missing tools, fix response shapes)
- [ ] MCP registry listing (Anthropic + Cursor)
- [ ] Production deployment guide
- [ ] JS embeddable widget (`<script>` embed)
- [ ] Response shape consistency audit

### OUT OF SCOPE (Deferred)
- ❌ Cloud/hosted tier (CFO: hard no, unanimous)
- ❌ Email notifications (2-week feature, too heavy)
- ❌ Custom fields (low ROI, high complexity)
- ❌ SSO SAML/OIDC (no demand signal)
- ❌ i18n (wait for community)
- ❌ GitHub Issues sync (webhooks make this unnecessary)

---

## 4. Success Metrics

| Metric | Current | v0.3.0 Target | Kill Threshold |
|--------|---------|---------------|----------------|
| GitHub stars | 7 | 50 | <50 at 3 months = stop |
| External deployments | 0 | 3 | <3 = maintenance only |
| Community PRs/issues | 0 | 2+ | 0 = consider sunset |
| MCP registry installs | 0 | 50 | — |
| Astuto migrants | 0 | 10 | — |

### Review Checkpoint: November 12, 2026 (3 months post-v0.3.0)

---

## 5. Technical Architecture (Preserved)

No architectural changes needed. 6-crate DAG is clean:

```
rungu-proto  →  Wire types (DTOs, enums)
rungu-core   →  Storage + Store (concrete struct, dual SQLite/Postgres)
rungu-api    →  27 Axum routes (projects, posts, comments, attachments, votes)
rungu-auth   →  OAuth (Google/GitHub/Keycloak) + session middleware
rungu-mcp    →  14 MCP tools (stdio JSON-RPC 2.0, PRODUCTION-READY)
rungud       →  Binary: config, rate limiter, server, SPA serving
```

### New Addition: rungu-webhook (Week 2)
- Crate for webhook subscription + delivery
- SSRF protection (block RFC1918, link-local, metadata endpoints)
- Per-tenant rate limiting
- Retry with exponential backoff

---

## 6. Kill Criteria

Stop active development if ANY at 3-month checkpoint:
1. <50 GitHub stars (currently 7)
2. <3 external deployments (currently 0)
3. Zero organic community contributions
4. No inbound interest
5. Cora Code or Titen showing revenue traction

**Current: 4 of 5 already met. Rungu gets 4 weeks to prove adoption signal.**

---

*Full synthesis: docs/bmad-synthesis-blueprint.md*
*Discussion records: uteke room `disc:rungu-optimization`*
