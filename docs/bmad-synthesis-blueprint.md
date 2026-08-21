# BMAD Synthesis Blueprint — Rungu Optimization
> Multi-Agent Discussion Result | 2026-08-12
> Participants: CTO (7/10), CFO (3/10), CMO (5.5/10), CLO (4/10 risk), COO (7/10)
> Method: 7-Round Uteke Coordination (CTO Leader + 4 parallel subagents)

---

## Executive Verdict

### FUSED SCORE: 5.7/10 — CONDITIONAL GO (Strategic Portfolio Play)

**Not a revenue product. Not a throwaway. A strategic ecosystem component.**

Rungu's value is NOT standalone revenue (CFO is correct: ~$150-300/yr expected). Rungu's value is **DevOps loop completeness**: TrapFall (errors) + Rungu (feedback) + Cora Code (review) = a self-hosted developer tools ecosystem that no competitor offers.

| Perspective | Score | Verdict | Key Insight |
|-------------|-------|---------|-------------|
| CTO | 7/10 | Conditional GO | Architecture solid, ecosystem moat |
| CFO | 3/10 | No monetization | Pure revenue: negative ROI. Valid. |
| CMO | 5.5/10 | WAIT 2-3 weeks | MCP is wedge. Astuto orphans = opportunity |
| CLO | 4/10 risk | Low-moderate | Nothing blocks OSS. Trademark before cloud. |
| COO | 7/10 | SHIP rescoped Phase 1 | 4 weeks, not 12. Engineering already solid |

### Resolution: CFO vs CTO/COO
- **CFO is RIGHT about standalone revenue.** Don't monetize. Don't build cloud tier.
- **CTO/COO are RIGHT about ecosystem value.** Rungu completes the DevOps loop.
- **Synthesis:** Treat Rungu as ecosystem infrastructure, not a revenue product.
  Budget = max 4 weeks focused dev, then maintenance mode.
  Success metric = GitHub stars + external deployments + ecosystem integration depth, NOT revenue.

---

## Ground Truth (Verified Aug 12, 2026)

### Codebase Stats (Corrected)
| Metric | Value | Source |
|--------|-------|--------|
| Total commits | 69 | git log |
| Last commit | Jul 9, 2026 | git log |
| Rust LOC | ~7,000 | wc -l (all crates) |
| TypeScript LOC | ~3,850 | COO estimate |
| Crates | 6 | proto, core, api, auth, mcp, rungud |
| Tests | 97 (42 integration + 16 store + ~39 unit) | COO deep count |
| API routes | 27 | COO count (17 was undercount) |
| MCP tools | 14 (13 in code + list_attachments) | grep verified |
| Web pages | 7+ | find routes |
| SQL migrations | 3 | find *.sql |
| Stars | 7 | GitHub |
| Open issues | 1 (#28 CLOSED — MCP implemented) | gh issue |

### ⚠️ MCP Status: PRODUCTION-READY (NOT stub)
**README is WRONG.** Line 88 says "⚠️ Experimental — tool handlers return stub data."
**Actual code:** All 14 MCP tools call `store.*` methods directly. Issue #28 is CLOSED.
**Action:** Remove experimental disclaimer from README immediately.

---

## Consolidated Gap Analysis

### Priority Matrix (Fused from all 5 perspectives)

| # | Gap | Impact | Effort | Priority | Source |
|---|-----|--------|--------|----------|--------|
| 1 | README MCP stub disclaimer (WRONG) | HIGH | 5 min | **P0 NOW** | CTO correction |
| 2 | OpenAPI coverage 63% → 100% | HIGH | 2 days | **P0** | COO |
| 3 | Live demo deployment | HIGH | 1 day | **P0** | CMO |
| 4 | Screenshots in README | MEDIUM | 2 hrs | **P0** | CMO |
| 5 | Webhook system | HIGH | 3 days | **P1** | CTO + COO |
| 6 | E2E tests (Playwright, ~15 tests) | HIGH | 2 days | **P1** | COO |
| 7 | Astuto migration guide | HIGH | 4 hrs | **P1** | CMO |
| 8 | SECURITY.md + disclosure policy | MEDIUM | 2 hrs | **P1** | CLO |
| 9 | Response shape consistency (MCP vs REST) | MEDIUM | 1 day | **P1** | COO |
| 10 | Production deployment guide (Traefik, backup, update) | MEDIUM | 1 day | **P2** | COO |
| 11 | JS embeddable widget | HIGH | 5 days | **P2** | CFO + CMO |
| 12 | Privacy policy + ToS templates | LOW (OSS) | 1 day | **P2** | CLO |
| 13 | MCP registry listing (Anthropic/Cursor) | HIGH | 2 hrs | **P2** | CMO |
| 14 | Trademark registration (DJKI) | LOW (now) | 1 day process | **P3** | CLO |
| 15 | Email notifications | HIGH | 2 weeks | **DEFER** | COO (too heavy for solo) |
| 16 | Custom fields | LOW | HIGH | **DEFER** | COO |
| 17 | SSO (SAML/OIDC) | LOW | HIGH | **DEFER** | CLO (no IP risk, but low demand) |
| 18 | Cloud/hosted tier | — | — | **KILL** | CFO (unanimous) |

---

## Roadmap: Rescoped (4 Weeks, Not 12)

### Week 1: Credibility Sprint (P0 items)
**Goal:** Make Rungu look alive and trustworthy.

| Day | Task | Owner | Output |
|-----|------|-------|--------|
| 1 | Fix README: remove MCP stub disclaimer, add screenshots, update feature list | Sibung | README.md |
| 1 | Deploy live demo (demo.rungu.dev or rungu.codecora.dev) | Sibung | URL |
| 2-3 | OpenAPI coverage 63% → 100% (document remaining 10 endpoints) | Sibung | Swagger UI complete |
| 4 | Write Astuto migration guide + script | Sibung | docs/migrate-from-astuto.md |
| 5 | Add SECURITY.md + vulnerability disclosure | Sibung | SECURITY.md |

### Week 2: Integration Sprint (P1 items)
**Goal:** Make Rungu the integration hub of the DevOps loop.

| Day | Task | Owner | Output |
|-----|------|-------|--------|
| 1-3 | Webhook system (subscribe, deliver, retry, SSRF protection) | Sibung | rungu-webhook crate |
| 4-5 | E2E tests: Playwright (login→post→vote→comment, 15 tests) | Sibung | web/tests/e2e/ |

### Week 3: Distribution Sprint (P2 items)
**Goal:** Expand surface area for adoption.

| Day | Task | Owner | Output |
|-----|------|-------|--------|
| 1-3 | MCP parity fixes: add delete_post, missing tools, fix response shapes | Sibung | MCP complete |
| 4 | List Rungu MCP on Anthropic + Cursor registries | Sibung | Registry listings |
| 5 | Production deployment guide (Traefik + TLS + backup + update) | Sibung | docs/deploy.md |

### Week 4: Widget + Polish
**Goal:** Embeddable surface for viral distribution.

| Day | Task | Owner | Output |
|-----|------|-------|--------|
| 1-4 | JS embeddable widget (`<script src="rungu-widget.js">`) | Sibung | web/widget/ |
| 5 | Response shape consistency audit (MCP vs REST) | Sibung | API contract doc |

### Post-v0.3.0: Maintenance Mode
- Add features ONLY when requested by real users (GitHub issues)
- Monitor stars, deployments, MCP registry downloads
- Kill criteria review at 3-month checkpoint:
  - <50 stars → stop active development
  - <3 external deployments → archive to maintenance
  - Zero community contributions → consider sunsetting

---

## Ecosystem Integration Architecture

```
┌─────────────────────────────────────────────────┐
│                CodeCoraDev Ecosystem             │
│                                                  │
│  ┌──────────┐   MCP   ┌──────────┐   MCP   ┌──────────┐
│  │  TrapFall │◄───────►│  Rungu   │◄───────►│ Cora Code│
│  │ (errors)  │         │(feedback)│         │ (review) │
│  └────┬─────┘         └────┬─────┘         └────┬─────┘
│       │                    │                    │
│       │   SENTRY_DSN       │ Webhooks           │ MCP tools
│       │   (done ✅)         │ (Week 2)           │ (Week 3)
│       │                    │                    │
│  ┌────▼──────────────────▼─────────────────────▼────┐
│  │              Unified Dashboard (Future)            │
│  │  Errors + Feedback + Code Quality in one view      │
│  └────────────────────────────────────────────────────┘
```

### Integration Status
| Integration | Status | Effort |
|-------------|--------|--------|
| TrapFall → Rungu (SENTRY_DSN) | ✅ DONE | Zero |
| Rungu → TrapFall (errors from feedback) | ✅ DONE | Zero |
| Rungu → Cora Code (MCP feedback → review context) | Week 3 | 2 days |
| Cora Code → Rungu (review findings → feedback posts) | Week 3 | 2 days |
| Rungu → Slack/Discord/n8n (webhooks) | Week 2 | 3 days |

---

## Legal/Compliance Checklist

| Item | Before OSS v0.3.0 | Before Any Cloud Tier | Status |
|------|:-::|:-::|--------|
| Apache-2.0 license | ✅ Keep | ✅ Keep | Done |
| SECURITY.md | ⬜ Week 1 | ✅ | Pending |
| Contributor patent grant sentence | ⬜ | ⬜ | 5 min fix |
| privacy-policy.md | — | 🔴 Required | Defer (no cloud) |
| terms-of-service.md | — | 🔴 Required | Defer (no cloud) |
| DPA template | — | 🔴 Required | Defer (no cloud) |
| Trademark (rungu, DJKI) | Recommended | 🔴 Required | Defer |
| UU PDP DPO | — | 🔴 (>1000 users) | Defer |

---

## Kill Criteria (3-Month Checkpoint: Nov 12, 2026)

Stop active development if ANY:
- [ ] <50 GitHub stars (currently 7)
- [ ] <3 external deployments (currently 0)
- [ ] Zero organic community contributions (issues, PRs)
- [ ] No inbound interest (emails, DMs, HN/Reddit traction)
- [ ] Cora Code or Titen showing revenue traction (opportunity cost)

**Current: 4 of 5 already met.** Rungu gets 4 weeks to prove adoption signal.
