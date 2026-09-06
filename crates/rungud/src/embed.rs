//! Embed widget — lightweight iframe feedback board.
//!
//! Serves two endpoints:
//!   - `GET /embed/{slug}` → standalone HTML board (no SvelteKit, vanilla JS)
//!   - `GET /embed.js`     → widget loader script that injects an iframe
//!
//! Both endpoints set `X-Frame-Options: ALLOWALL` so the board can be
//! framed by any third-party website. This is intentional — the board is
//! read-only and safe to embed.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

/// Build the embed router.
///
/// Must be registered in `server.rs` **before** the SPA fallback so these
/// routes take priority over client-side routing.
pub fn router() -> Router<rungu_api::AppState> {
    Router::new().route("/embed.js", get(embed_js)).route("/embed/{slug}", get(embed_html))
}

// ── embed.js loader ────────────────────────────────────────────────────

/// Serve the widget loader script.
///
/// The script reads `data-rungu-slug` from its own `<script>` tag, creates an
/// iframe pointing to `/embed/{slug}`, and auto-resizes the iframe height
/// based on messages from the embedded page.
async fn embed_js() -> Response {
    let js = include_str!("../assets/embed.js");
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
            // Allow framing — the loader itself runs on the host page, but
            // advertising ALLOWALL is harmless for a script response and
            // keeps middleware/CDN policies consistent.
            (header::HeaderName::from_static("x-frame-options"), "ALLOWALL"),
        ],
        js,
    )
        .into_response()
}

// ── embed/{slug} HTML board ────────────────────────────────────────────

/// Serve the lightweight HTML board for a given project slug.
///
/// The page is self-contained: minimal inline CSS (dark-mode aware via
/// `prefers-color-scheme`) and vanilla JS that fetches posts from the REST
/// API. No external dependencies, no framework.
async fn embed_html(State(state): State<rungu_api::AppState>, Path(slug): Path<String>) -> Response {
    // Sanitise the slug for safe interpolation into the HTML template.
    // The slug is also used as a URL path segment by the fetch call; we
    // encode it to prevent any HTML/JS injection.
    let slug_escaped = slug.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect::<String>();

    // White-label branding (#185). The badge can only disappear with a valid
    // license (soft gate) — see `rungu_api::meta`.
    let powered_by = state.license.badge_visible(&state.branding).await;
    // serde_json string escaping doubles as safe JS string-literal quoting.
    let brand_json = serde_json::to_string(&state.branding.brand_name).unwrap_or_else(|_| "\"Rungu\"".to_string());
    let html = EMBED_HTML_TEMPLATE
        .replace("__SLUG__", &slug_escaped)
        .replace("__POWERED_BY__", if powered_by { "true" } else { "false" })
        .replace("__BRAND_NAME__", &brand_json);

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
            // Allow embedding from any origin — this is the whole point.
            (header::HeaderName::from_static("x-frame-options"), "ALLOWALL"),
        ],
        Html(html),
    )
        .into_response()
}

/// The HTML template for the embed board.
///
/// Uses `r###` ... `###` delimiters to avoid conflicts with `"#` sequences
/// that appear naturally in HTML attributes like `href="#"` and CSS hex
/// colors like `#ffffff`.
///
/// `__SLUG__` is replaced at request time with the project slug.
const EMBED_HTML_TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Feedback</title>
<style>
  :root {
    --bg: #ffffff;
    --text: #1a1a2e;
    --muted: #6b7280;
    --border: #e5e7eb;
    --card: #f9fafb;
    --accent: #6366f1;
    --accent-fg: #ffffff;
    --badge-open: #e0e7ff;
    --badge-open-fg: #3730a3;
    --badge-planned: #fef3c7;
    --badge-planned-fg: #92400e;
    --badge-progress: #dbeafe;
    --badge-progress-fg: #1e40af;
    --badge-done: #d1fae5;
    --badge-done-fg: #065f46;
    --badge-declined: #fee2e2;
    --badge-declined-fg: #991b1b;
    --btn-bg: #6366f1;
    --btn-fg: #ffffff;
  }
  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #0f172a;
      --text: #f1f5f9;
      --muted: #94a3b8;
      --border: #1e293b;
      --card: #1e293b;
      --accent: #818cf8;
      --accent-fg: #0f172a;
      --badge-open: #312e81;
      --badge-open-fg: #c7d2fe;
      --badge-planned: #78350f;
      --badge-planned-fg: #fcd34d;
      --badge-progress: #1e3a8a;
      --badge-progress-fg: #93c5fd;
      --badge-done: #064e3b;
      --badge-done-fg: #6ee7b7;
      --badge-declined: #7f1d1d;
      --badge-declined-fg: #fca5a5;
      --btn-bg: #818cf8;
      --btn-fg: #0f172a;
    }
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
    background: var(--bg);
    color: var(--text);
    padding: 12px;
    font-size: 14px;
    line-height: 1.5;
  }
  .header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .header h2 { font-size: 16px; font-weight: 700; }
  .btn-feedback {
    background: var(--btn-bg);
    color: var(--btn-fg);
    border: none;
    padding: 6px 14px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    text-decoration: none;
    white-space: nowrap;
  }
  .btn-feedback:hover { opacity: 0.9; }
  .post-list { display: flex; flex-direction: column; gap: 8px; }
  .post {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .vote-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    min-width: 42px;
    padding: 4px 8px;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--border);
  }
  .vote-arrow { font-size: 12px; color: var(--accent); }
  .vote-count { font-size: 14px; font-weight: 700; }
  .post-body { flex: 1; min-width: 0; }
  .post-title { font-weight: 600; font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .post-meta { font-size: 12px; color: var(--muted); margin-top: 2px; }
  .badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 12px;
    font-size: 11px;
    font-weight: 600;
    text-transform: capitalize;
  }
  .badge-open { background: var(--badge-open); color: var(--badge-open-fg); }
  .badge-planned { background: var(--badge-planned); color: var(--badge-planned-fg); }
  .badge-in_progress { background: var(--badge-progress); color: var(--badge-progress-fg); }
  .badge-done { background: var(--badge-done); color: var(--badge-done-fg); }
  .badge-declined { background: var(--badge-declined); color: var(--badge-declined-fg); }
  .empty { text-align: center; padding: 40px 20px; color: var(--muted); }
  .error { text-align: center; padding: 20px; color: var(--badge-declined-fg); }
  .loading { text-align: center; padding: 20px; color: var(--muted); }
</style>
</head>
<body>
  <div class="header">
    <h2 id="brand-name">__BRAND__</h2>
    <a class="btn-feedback" id="share-btn" href="#" target="_blank" rel="noopener">Share Feedback</a>
  </div>
  <div id="board" class="post-list">
    <div class="loading">Loading…</div>
  </div>
  <script>
  (function() {
    var SLUG = '__SLUG__';
    var POWERED_BY = '__POWERED_BY__' === 'true';
    var BRAND = document.getElementById('brand-name');
    if (BRAND) {
      BRAND.textContent = __BRAND_NAME__;
      document.title = 'Feedback · ' + BRAND.textContent;
    }
    if (POWERED_BY) {
      var badge = document.createElement('div');
      badge.style.cssText = 'text-align:center;padding:10px 0;font-size:11px;opacity:.65;';
      badge.innerHTML = 'Powered by <a href="https://github.com/codecoradev/rungu" target="_blank" rel="noopener" style="color:inherit;">Rungu</a>';
      document.body.appendChild(badge);
    }
    var board = document.getElementById('board');
    var shareBtn = document.getElementById('share-btn');

    // Resolve the Rungu server origin (same host that served this embed page).
    var origin = window.location.origin;
    var boardUrl = origin + '/p/' + SLUG;
    shareBtn.href = boardUrl;

    function escapeHtml(str) {
      var div = document.createElement('div');
      div.textContent = str;
      return div.innerHTML;
    }

    function statusBadge(status) {
      var cls = 'badge badge-' + (status || 'open');
      var label = (status || 'open').replace(/_/g, ' ');
      return '<span class="' + cls + '">' + escapeHtml(label) + '</span>';
    }

    function renderPosts(data) {
      var posts = data.data || [];
      if (posts.length === 0) {
        board.innerHTML = '<div class="empty">No feedback yet. Be the first!</div>';
        return;
      }
      board.innerHTML = posts.map(function(p) {
        return '<div class="post">' +
          '<div class="vote-box"><span class="vote-arrow">&#9650;</span><span class="vote-count">' + (p.vote_count || 0) + '</span></div>' +
          '<div class="post-body">' +
            '<div class="post-title">' + escapeHtml(p.title || '') + '</div>' +
            '<div class="post-meta">' + statusBadge(p.status) + ' &middot; ' + (p.comment_count || 0) + ' comments</div>' +
          '</div>' +
        '</div>';
      }).join('');
    }

    function renderError(msg) {
      board.innerHTML = '<div class="error">' + escapeHtml(msg) + '</div>';
    }

    // Notify parent window of height changes for auto-resize.
    function notifyHeight() {
      var h = document.documentElement.scrollHeight;
      if (window.parent && window.parent !== window) {
        window.parent.postMessage({ runguEmbed: true, height: h }, '*');
      }
    }

    fetch(origin + '/api/projects/' + SLUG + '/posts?sort=top&per_page=20')
      .then(function(res) {
        if (!res.ok) {
          if (res.status === 404) throw new Error('Project not found');
          throw new Error('Failed to load (' + res.status + ')');
        }
        return res.json();
      })
      .then(function(data) {
        renderPosts(data);
        // Notify after DOM has settled.
        setTimeout(notifyHeight, 50);
      })
      .catch(function(err) {
        renderError(err.message || 'Failed to load feedback');
        setTimeout(notifyHeight, 50);
      });

    // Observe DOM changes and re-notify parent of height.
    if (typeof MutationObserver !== 'undefined') {
      var observer = new MutationObserver(function() { notifyHeight(); });
      observer.observe(board, { childList: true, subtree: true });
    }
    // Also notify on window resize.
    window.addEventListener('resize', notifyHeight);
  })();
  </script>
</body>
</html>"##;
