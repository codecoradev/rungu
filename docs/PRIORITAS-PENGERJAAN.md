# Prioritas Pengerjaan Rungu

Dibuat dari: status kode eksisting + GitHub issues #1–#6.
Repo: `codecoradev/rungu` — branch `develop`.

## Status Kode Saat Ini

| Crate / Bagian | Status |
|----------------|--------|
| `rungu-proto` | ✅ Lengkap — semua wire types, request/response, enum |
| `rungu-auth/config.rs` | ✅ Lengkap — `AuthConfig`, provider dari ENV, `active_providers` |
| `rungu-auth/session.rs` | ✅ Lengkap — `issue_jwt`, `validate_jwt` (HS256, 7 hari) |
| `rungu-auth/middleware.rs` | ⚠️ **Stub** — `CurrentUser` hardcode 401, belum panggil `validate_jwt` |
| `rungu-core/store.rs` | ⚠️ Sebagian — query SQL ada, tapi `list_posts`/`get_post`/`list_comments` masih TODO mapping |
| `rungu-api/auth_routes.rs` | ⚠️ **Stub** — hanya `/auth/providers`. Semua route OAuth di-comment |
| `rungu-api/lib.rs` | ⚠️ Hanya mod `auth_routes`. Belum ada mod posts/vote/comment routes |
| `rungud/server.rs` | ⚠️ Mount hanya `auth_routes()`. App state belum bawa `AuthConfig` |
| `web/` (SvelteKit) | 🔴 **Kosong** — hanya `package.json` + `svelte.config.js` |

## Urutan Pengerjaan (Dependency Order)

Logika: sebelum API butuh auth → middleware harus jalan dulu.
Sebelum frontend bisa di develop → API + auth endpoint harus ada.

---

### FASE 1 — Auth Foundation (blocker semua)

Prioritas: **🔴 KRITIS** — selesaikan paling pertama.

#### 1a. Issue #2 — `CurrentUser` extractor (HIGH, `phase:auth`)
**Kunci semuanya.** Tanpa ini, semua API endpoint tidak bisa tahu siapa user.

Tugas:
- Selesaikan `crates/rungu-auth/src/middleware.rs`:
  - Hapus stub return 401.
  - Panggil `rungu_auth::session::validate_jwt(cookie, app_secret)`.
  - Butuh akses `app_secret` → extractor harus `FromRequestParts<AppState>` (bukan `()`), atau inject `AuthConfig` ke `AppState`.
- Juga implement `OptionalCurrentUser` (variant tidak 401).
- Daftarkan middleware layer di `server.rs` jika butuh global, atau pakai per-route extractor.

File: `crates/rungu-auth/src/middleware.rs`, `crates/rungud/src/server.rs`

Estimasi: 0.5 hari.

#### 1b. Issue #1 — OAuth callback handlers (HIGH, `phase:auth`)
Setelah #2 jalan, implement OAuth flow penuh.

Tugas:
- Uncomment + isi semua route di `crates/rungu-api/src/auth_routes.rs`:
  - `GET /auth/:provider/login` → redirect ke `auth_url` + state CSRF
  - `GET /auth/:provider/callback` → tukar code → token → userinfo → `find_or_create_user` → `upsert_identity` → `issue_jwt` → set cookie
  - `GET /auth/logout` → clear cookie
  - `GET /auth/me` → return `CurrentUser` dari extractor
- Butuh HTTP client (`reqwest`) untuk token exchange & userinfo.
- `AppState` harus bawa `AuthConfig` (saat ini hanya `Store` + `Config`).
- Provider: Google, GitHub, Keycloak — logic sama, beda endpoint saja.

Depends: #2 selesai (callback issue JWT, /auth/me pakai CurrentUser).

Estimasi: 1.5–2 hari.

---

### FASE 2 — Core API (butuh auth jalan)

#### 2a. Issue #3 — Posts CRUD (HIGH, `phase:api`)
Prioritas **🔴 TINGGI** — fitur inti feedback board.

Tugas:
- Selesaikan TODO di `store.rs`:
  - `list_posts`: mapping row → `PostDetail` (saat ini return `vec![]`)
  - `get_post`: implement JOIN dengan user + cek `user_voted`
- Buat module baru `rungu-api/src/post_routes.rs`:
  - `GET /api/projects/:slug/posts` (list + filter + sort + paginate)
  - `POST /api/projects/:slug/posts` (auth required)
  - `GET /api/posts/:id`
  - `PATCH /api/posts/:id` (admin: status change)
  - `DELETE /api/posts/:id` (admin)
- Mount di `server.rs` under `/api`.
- Pakai `CurrentUser` extractor untuk POST/PATCH/DELETE.

Depends: #2 (auth), #3 store fix.

Estimasi: 1–1.5 hari.

#### 2b. Issue #5 — Votes API (MEDIUM, `phase:api`)
Prioritas **🟡 MENENGAH** — query store sudah ada (`toggle_vote`, `has_voted`), tinggal wiring.

Tugas:
- Tambah `rungu-api/src/vote_routes.rs`:
  - `POST /api/posts/:id/vote` (auth) → toggle, return `{voted: bool, vote_count: i64}`
  - `GET /api/posts/:id/vote` (auth) → `{voted: bool}`
- Mount di router.
- Update `PostDetail.user_voted` di `get_post` biar konsisten.

Depends: #2 (auth), #3 (`get_post` untuk return count benar).

Estimasi: 0.5 hari.

#### 2c. Issue #6 — Comments API (MEDIUM, `phase:api`)
Prioritas **🟡 MENENGAH** — `create_comment`/`delete_comment` sudah jalan, `list_comments` masih TODO.

Tugas:
- Selesaikan `store.rs::list_comments` (JOIN user, threaded via `parent_id`).
- Tambah `rungu-api/src/comment_routes.rs`:
  - `GET /api/posts/:id/comments`
  - `POST /api/posts/:id/comments` (auth)
  - `DELETE /api/comments/:id` (auth, owner atau admin)
- Mount di router.

Depends: #2 (auth), #3.

Estimasi: 0.5–1 hari.

---

### FASE 3 — Frontend (butuh API semua jalan)

#### 3. Issue #4 — SvelteKit frontend (HIGH, `phase:frontend`)
Prioritas **🔴 TINGGI** tapi **terakhir** — tidak bisa di-develop sebelum API stabil.

Tugas:
- Init SvelteKit 5 + Tailwind v4 + shadcn-svelte di `web/`.
- Pages:
  - `/` landing
  - `/board/:slug` board (list posts, sort/filter, vote button)
  - `/post/:id` detail + vote + threaded comments
  - `/login` OAuth buttons (consume `/auth/providers`)
  - `/admin/:slug` admin (status manage)
- Components: `PostCard`, `VoteButton`, `CommentThread`, `AuthProviderButtons`, `PostForm`.
- Embed SPA via `rust-embed` di `rungud/src/spa.rs` (sudah ada handler, tinggal isi build artifact).

Depends: #1, #2, #3, #5, #6 (semua API + auth).

Estimasi: 3–5 hari.

---

## Dependency Graph

```
#2 CurrentUser ──┬──▶ #1 OAuth handlers
                 │
                 ├──▶ #3 Posts CRUD ──┬──▶ #5 Votes
                 │                    └──▶ #6 Comments
                 │
                 └──▶ (semua API butuh ini)

#1 + #3 + #5 + #6 ──▶ #4 Frontend
```

## Ringkasan Prioritas (Eksekusi Berurutan)

| # | Issue | Estimasi | Blocker Untuk |
|---|-------|----------|---------------|
| 1 | #2 — CurrentUser extractor | 0.5 hari | #1, #3, #5, #6 |
| 2 | #1 — OAuth callback | 1.5 hari | #4 frontend |
| 3 | #3 — Posts CRUD | 1.5 hari | #5, #6, #4 |
| 4 | #5 — Votes API | 0.5 hari | #4 |
| 5 | #6 — Comments API | 0.75 hari | #4 |
| 6 | #4 — Frontend | 4 hari | — |

**Total estimasi: ~8.5–9 hari kerja.**

## Catatan Teknis

- `AppState` di `server.rs` harus diperluas: tambah `auth_config: AuthConfig` (saat ini tidak ada).
- Tambah dependency `reqwest` di `rungu-api` untuk OAuth token exchange.
- Middleware `CurrentUser` sebaiknya `FromRequestParts<AppState>` bukan `FromRequestParts<()>` agar bisa akses `app_secret` untuk `validate_jwt`.
- `list_posts` di `store.rs` punya **SQL injection risk**: `format!` string interpolation untuk filter. Refactor ke parameterized query sebelum production.
- `web/` masih kosong total — butuh `npm create svelte` dulu sebelum issue #4.
