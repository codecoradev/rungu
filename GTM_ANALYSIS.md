# Rungu GTM / Market Readiness Analysis
**Perspective:** CMO / Go-to-Market
**Date:** 2026-08-12
**Verdict:** ⚠️ **LAUNCH NOW (v0.3.0 beta) — but NOT as "Featurebase alternative" yet**

---

## Market Readiness Score: **5.5/10**

### Scoring breakdown
| Dimension | Score | Notes |
|-----------|-------|-------|
| Product completeness | 6/10 | Core features solid, but email notifications missing, S3 storage stub-only |
| Differentiation | 8/10 | MCP server is genuinely unique, only Rust OSS, Apache-2.0 (not AGPL) |
| Documentation | 7/10 | VitePress docs live, getting-started good, but no screenshots/demo |
| Packaging/Distribution | 5/10 | Single binary + Docker exist, but zero screenshots in README, no demo instance |
| Market timing | 7/10 | Astuto archived Feb 2026 — 2.3K orphans looking for a new home |
| Competitive moat | 4/10 | 7 stars vs Fider 4.4K — no social proof, no user base |
| Messaging clarity | 3/10 | README says "lightweight feedback board" — no positioning hook |

---

## 1. Ideal First User

**Primary: Self-hosting indie/SaaS developers (NOT enterprise)**

The user who will install Rungu first is:
- Already running their own infrastructure (Docker, VPS, homelab)
- A solo dev or small team (1-10 people) building a SaaS/product
- Currently using a Google Doc, Discord channel, or GitHub Issues for feedback (not Featurebase — they're too cheap for that)
- Values data ownership and privacy (won't put user feedback on a 3rd-party SaaS)
- May already use AI coding agents (Claude Code, Cursor) — this is the wedge

**Secondary: Open-source project maintainers**
- Need a feedback board for their community
- Currently use GitHub Issues (doesn't separate feature requests from bugs well)
- Budget: $0
- Fider is the incumbent but AGPL-3.0 scares some away

**NOT the target yet:**
- Enterprise teams (they're on Featurebase/Canny, and switching cost is too high)
- Non-technical users (self-hosting is a barrier)
- Agencies managing multiple clients

**The wedge persona:** "Indie dev using Claude Code who wants AI-managed feedback"
This is a micro-niche that Rungu can genuinely own because **no competitor has an MCP server**. The pitch: "Your AI agent reads your feedback board, summarizes trends, files issues — all from the editor."

---

## 2. Positioning

### Current problem
The README says "Lightweight, self-hosted feedback board" — this is descriptive, not differentiated. It could describe Fider, Astuto, or 10 other tools.

### Recommended positioning

**Lead positioning: "The feedback board that talks to your AI agents"**
- MCP is the unique wedge. No competitor has it. This is the 10x feature for a specific audience.
- Not the main pitch for everyone, but the differentiator that gets attention.

**Core positioning: "Self-hosted feedback board for developers who own their data"**
- Targets the self-hosting community directly
- "For developers" signals who it's for (not enterprise)
- "Own their data" hits the privacy/sovereignty angle

### Positioning matrix (by channel)

| Audience | Positioning Hook |
|----------|-----------------|
| Show HN / Reddit | "Only self-hosted feedback tool with an MCP server (15 tools)" |
| Self-hosting community (r/selfhosted) | "Rust-native, single binary, Apache-2.0 (not AGPL)" |
| AI/dev tool community | "Your AI agent can read your feedback board and file issues" |
| OSS maintainers | "Apache-2.0 feedback board — embed it, fork it, sell it" |

### What NOT to position as (yet)
- ❌ "Featurebase alternative" — you don't have enough features yet
- ❌ "Canny killer" — you're not, and saying so invites comparison you'll lose
- ❌ "MCP-first" — it's not first, it's one of many features; say "with MCP support"

---

## 3. Distribution Channels & GitHub Star Strategy

### The hard truth
| Metric | Rungu | Fider | Astuto |
|--------|-------|-------|--------|
| GitHub Stars | 7 | 4,463 | 2,350 (archived) |
| Age | ~2 months | ~8 years | ~5 years |
| License | Apache-2.0 | AGPL-3.0 | AGPL-3.0 |

Fider accumulated 4.5K stars over **8 years**. Rungu won't match that in months. But it doesn't need to — **500 stars** makes it credible as "the #2 OSS feedback board" and **100-200 stars** puts it on the radar.

### Realistic star targets
- **30 days post-launch:** 100-200 stars (a good Show HN + r/selfhosted can get 50-150)
- **90 days:** 300-500 stars (if content marketing + migration guide hits)
- **12 months:** 1,000-2,000 stars (if MCP integration story gains traction)

### Channel priority

#### 🔴 Tier 1: Do First (Week 1)
1. **Show HN** — Best single-shot for 50-200 stars. Title: "Rungu: Self-hosted feedback board with an MCP server (Rust, Apache-2.0)"
   - Post Tuesday-Thursday, 8-10 AM ET
   - Have the founder active in comments for 4+ hours
   - Include a live demo link

2. **Reddit r/selfhosted** — 500K+ members, loves Docker, loves single-binary tools
   - Title: "Rungu – self-hosted feedback board (single binary, MCP server, Apache-2.0)"
   - Focus on the self-hosting experience, not just features

3. **Reddit r/rust** — 300K+ members, supports their own. "Built a feedback board in Rust + SvelteKit"
   - Lead with the technical story (Axum, sqlx, SvelteKit 5)

#### 🟡 Tier 2: Do Early (Week 2-4)
4. **r/SideProject / r/SideProjectIdeas** — indie dev audience
5. **dev.to article** — "I built a self-hosted feedback board with Rust and MCP support"
6. **GitHub topic optimization** — add more topics: `feedback-board`, `feature-requests`, `rust`, `axum`, `self-hosting`, `privacy`
7. **Awesome lists** — submit to awesome-selfhosted, awesome-rust

#### 🟢 Tier 3: Sustained (Month 2-3)
8. **MCP ecosystem listing** — get listed on Anthropic's MCP server directory, Cursor's docs, Claude Code marketplace
9. **Astuto migration guide** — direct play for orphaned users (see §7)
10. **codecora.dev blog series** — comparison articles (see §6)

### The MCP ecosystem play (highest leverage)
Rungu's biggest distribution asset isn't GitHub — it's being one of the first tools in the **MCP server ecosystem**. If Rungu gets listed on:
- Anthropic's MCP server registry
- Cursor's MCP integration docs
- Claude Code's tool marketplace

...it becomes the **default feedback tool for the AI-native dev workflow** before competitors even notice. This is a land grab with a ~6-12 month window.

---

## 4. Messaging: Why Switch?

### The honest truth
Nobody will switch FROM Featurebase/Canny TO Rungu for a while. The switch target is:

**FROM:** GitHub Issues / Google Sheets / Discord channels / Trello cards
**TO:** Rungu

### Messaging pillars

#### Pillar 1: "Your feedback board should own its own data"
> "Featurebase and Canny lock your feedback behind a paywall. Rungu runs on your server, in your database, under your control. Apache-2.0 — embed it, fork it, sell it."

#### Pillar 2: "The only feedback tool that talks to your AI agents"
> "Rungu ships with a built-in MCP server. Claude Code, Cursor, and Windsurf can read your feedback board, summarize trends, and file issues — without leaving the editor."

#### Pillar 3: "One binary. Zero dependencies."
> "No Node.js runtime. No Python. No Docker image required (but available). Download, run, done. Rust performance, SQLite simplicity."

### The elevator pitch
> "Rungu is a self-hosted feedback board that's fast, private, and works with your AI coding agents. It's what you'd build if you started a feedback tool today."

### What doesn't work (tested assumptions)
- "Open-source alternative to Featurebase" — too generic, every OSS project says this
- "Free" — Featurebase has a free tier, so "free" isn't enough
- "Rust-powered" — non-Rust devs don't care about the backend language

---

## 5. Launch Strategy

### Can we launch NOW?

**NO — not quite. But in 2-3 weeks of focused work, YES.**

### Pre-launch checklist (the "credible launch" bar)

#### 🔴 Must-have before Show HN (blocks launch)
| Item | Status | Effort |
|------|--------|--------|
| Live demo instance (rungu.codecora.dev) | ❌ Missing | 1 day |
| Screenshots in README (board, post detail, admin, roadmap) | ❌ Missing | 2 hours |
| Demo GIF or 60-sec video walkthrough | ❌ Missing | 4 hours |
| "MCP integration" blog post or doc with working example | ❌ Missing | 1 day |
| Fix MCP tools that return stub data (per README warning) | ❌ Known issue | 3-5 days |
| GitHub topics expanded + description optimized | ⚠️ Partial | 15 min |
| CONTRIBUTING.md with "good first issue" labels | ⚠ Exists | 1 hour |

#### 🟡 Should-have before launch (not blocking, but strongly recommended)
| Item | Status | Effort |
|------|--------|--------|
| Docker image with visible pull count | ⚠ Exists, no visibility | — |
| Comparison page (Rungu vs Fider vs Featurebase) | ❌ Missing | 1 day |
| "Migrate from Astuto" guide | ❌ Missing | 4 hours |
| Featured issue: email notifications (#73) | ❌ Not started | 5-7 days |
| Roadmap page (public) on docs site | ⚠ In-app exists | 2 hours |

#### 🟢 Nice-to-have (post-launch)
- Embed widget (for other sites)
- S3 storage support
- Bulk import (for migration)
- Admin analytics dashboard

### Minimum credible launch = **v0.3.0-beta**
Focus on these fixes/features, then launch:
1. Fix MCP stub data (critical — it's the headline differentiator and it doesn't fully work)
2. Deploy a live demo
3. Add screenshots to README
4. Write the MCP integration blog post
5. Optimize GitHub repo for discovery

**Estimated timeline: 2-3 weeks of focused work, then launch.**

---

## 6. Content Marketing Strategy

### Content pillars (in priority order)

#### 1. "Self-hosted feedback board" comparison content
Target keywords: "self-hosted feedback tool", "Featurebase alternative", "Canny open source"

| Content | Keyword Target | Format |
|---------|---------------|--------|
| "Rungu vs Fider vs Featurebase — 2026 comparison" | "feedback tool comparison" | Blog post + GitHub README section |
| "How to self-host your feedback board with Docker" | "self-host feedback board" | Tutorial + YouTube short |
| "Why I built a feedback tool in Rust" | (brand) | dev.to article |

#### 2. MCP integration content (highest-differentiation)
| Content | Audience | Format |
|---------|----------|--------|
| "How to connect Rungu to Claude Code" | AI devs | Blog post + video |
| "AI-managed feedback: the new dev workflow" | Trend | Thought-leadership article |
| "Building an MCP server in Rust" | Rust devs | Technical deep-dive |

#### 3. Migration content (direct user acquisition)
| Content | Target | Format |
|---------|--------|--------|
| "Migrating from Astuto to Rungu" | Astuto orphans (2.3K stars) | Migration guide + script |
| "Switching from GitHub Issues to Rungu" | OSS maintainers | Tutorial |
| "Move your feedback from Canny to Rungu" | Cost-conscious teams | Guide |

### Content cadence ($0 budget)
- **Week 1 (launch):** Show HN + Reddit + dev.to launch post
- **Week 2:** "Migrating from Astuto" guide (time-sensitive)
- **Week 3:** "Connecting Rungu to Claude Code" MCP tutorial
- **Week 4:** "Rungu vs Fider vs Featurebase" comparison
- **Monthly after:** 1 deep-dive post, 2 short Threads/X posts linking back

---

## 7. Astuto Opportunity Assessment

### The situation
- **Astuto:** 2,350 stars, archived Feb 2026, last push Jan 2026
- License: AGPL-3.0
- Stack: Ruby on Rails + React
- Users: estimated 500-2,000 active self-hosters (star count ≠ users, but signals interest)

### Opportunity score: **7/10 (high)**

### Why this is the highest-leverage GTM move

1. **2,350 stars = ~500-1,000 people who cared enough to star it** — a portion are actively looking for alternatives right now
2. **Time-limited window** — someone else will capture them (Fider, a new project) if Rungu doesn't move fast
3. **Self-selecting audience** — they already self-host feedback tools, understand the concept, and are motivated to migrate
4. **AGPL → Apache-2.0 is an upgrade** — Astuto users who hit AGPL restrictions (can't embed in commercial products) get relief with Apache-2.0

### Capture strategy

#### Immediate (Week 1)
1. **Write "Migrating from Astuto to Rungu" guide**
   - Include a migration script (even partial: export Astuto data → CSV → Rungu import API)
   - Map Astuto features → Rungu features table
   - Address: "What you'll lose" (be honest) and "What you'll gain" (MCP, Rust, Apache-2.0)

2. **Post in Astuto's GitHub Discussions / Issues**
   - "Rungu — Apache-2.0 self-hosted feedback board (for anyone looking for alternatives)"
   - Be respectful, not spammy. Acknowledge Astuto's contribution.

3. **Reddit/forum monitoring**
   - Search for "Astuto alternative", "Astuto archived", "feedback board self-hosted"
   - Set up Google Alerts or use grep on relevant subreddits
   - Respond with Rungu as an option

#### Sustained (Month 1-3)
4. **Bulk import from Astuto**
   - Build a `rungu import astuto` CLI command
   - Read Astuto's PostgreSQL directly or its CSV export
   - This removes the biggest switching cost (data migration)

5. **Feature parity check**
   - Audit Astuto's feature set → identify gaps Rungu should close
   - Key gaps to close: email notifications, custom statuses, SSO/SAML

### Risk
- If Fider adds an MCP server or drops AGPL, Rungu loses its wedge. **Move now.**

---

## Summary: LAUNCH Plan

### Verdict: **WAIT 2-3 weeks → LAUNCH v0.3.0-beta**

Rungu is NOT ready to launch today because:
1. The headline differentiator (MCP) returns stub data
2. No live demo, no screenshots — the repo looks unfinished
3. No content to support the launch

But it IS close. Here's the focused path:

### Week 1: Fix + Polish
- [ ] Fix MCP stub data (make all 15 tools return real data)
- [ ] Deploy live demo at rungu.codecora.dev
- [ ] Add 4-5 screenshots to README (board, post detail, roadmap, admin, dark mode)
- [ ] Write CONTRIBUTING.md with "good first issue" labels
- [ ] Optimize GitHub repo: expand topics, improve description

### Week 2: Content + Positioning
- [ ] Write "Connecting Rungu to Claude Code" MCP tutorial
- [ ] Write "Migrating from Astuto to Rungu" guide
- [ ] Write "Rungu vs Fider vs Featurebase" comparison page
- [ ] Prepare Show HN and Reddit posts

### Week 3: Launch
- [ ] Post to Show HN (Tue-Thu, 8-10 AM ET)
- [ ] Post to r/selfhosted, r/rust, r/SideProject
- [ ] Publish dev.to launch article
- [ ] Post to codecora.dev blog + Threads/X announcement
- [ ] Submit to awesome-selfhosted and awesome-rust

### Month 2-3: Sustain
- [ ] Submit to MCP server registries (Anthropic, Cursor)
- [ ] Build Astuto migration tool
- [ ] Ship email notifications (#73)
- [ ] Content cadence: 1 blog post per week
- [ ] Monitor and respond to feedback (eat your own dog food)

---

## The one-sentence summary
> **Rungu's MCP server is its " wedge into the market — fix it, demo it, and lead with it. Launch in 3 weeks with Astuto migration as the immediate user-acquisition play, Show HN + Reddit as the distribution, and "self-hosted feedback that talks to your AI agents" as the positioning.**
