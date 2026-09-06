//! SPA handler — serve embedded static files.
//!
//! Uses rust-embed to embed the SvelteKit build output.
//! Falls back to index.html for client-side routing.

use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../web/build/"]
#[prefix = ""]
struct Assets;

/// Serve SPA static files or fallback to index.html.
///
/// The index.html fallback gets `window.__RUNGU_META__` injected right after
/// `<head>` so a white-labeled instance renders its own brand from the very
/// first paint (the static shell is baked at build time with Rungu defaults).
/// Values mirror `GET /api/meta`; `branding.svelte.ts` reads the global at
/// module init, and a `DOMContentLoaded` patch covers the pre-hydration DOM.
pub async fn spa_handler(State(state): State<rungu_api::AppState>, uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try exact file match first
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let cache_control = if is_immutable_asset(path) { "public, max-age=31536000, immutable" } else { "no-cache" };
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref()), (header::CACHE_CONTROL, cache_control)],
            file.data.to_vec(),
        )
            .into_response();
    }

    // Fallback to index.html for client-side routing
    match Assets::get("index.html") {
        Some(file) => {
            let powered_by = state.license.badge_visible(&state.branding).await;
            let html = String::from_utf8_lossy(&file.data).to_string();
            Html(inject_branding(&state.branding, powered_by, html)).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Inject the white-label boot script into the SPA shell.
///
/// The script (a) exposes `window.__RUNGU_META__` for the Svelte branding
/// store, and (b) patches `<title>` + the header brand span so the
/// pre-hydration markup never shows the baked-in "Rungu" defaults.
fn inject_branding(branding: &rungu_api::meta::InstanceBranding, powered_by: bool, html: String) -> String {
    let meta = serde_json::json!({
        "brandName": branding.brand_name,
        "logoUrl": branding.logo_url,
        "footerText": branding.footer_text,
        "poweredBy": powered_by,
    });
    // `</script>` in a brand value would close the boot <script> early (XSS).
    // serde_json does not escape "/", so do it ourselves: every "/" becomes
    // "\u002F" — valid inside a JSON string, inert inside a <script> block.
    let meta_json = meta.to_string().replace('/', "\\u002F");
    // Plain concatenation (no format!) so the JS braces need no escaping.
    let script = "<script>window.__RUNGU_META__=".to_string()
        + &meta_json
        + ";(function(){function p(){var m=window.__RUNGU_META__;if(!m||!m.brandName)return;"
        + "var b=m.brandName;"
        + "document.title=document.title.replace(/(\\s[\\u2014\\u00b7]\\s)Rungu(\\s[\\u2014\\u00b7]\\s|$)/,'$1'+b+'$2').replace(/^Rungu(\\s[\\u2014\\u00b7]\\s)/,b+'$1');"
        + "var el=document.getElementById('rungu-brand-boot');if(el)el.textContent=b;}"
        + "if(document.readyState==='loading'){document.addEventListener('DOMContentLoaded',p);}else{p();}})();</script>";
    match html.find("<head>") {
        Some(i) => {
            let mut out = String::with_capacity(html.len() + script.len());
            out.push_str(&html[..i + 6]);
            out.push_str(&script);
            out.push_str(&html[i + 6..]);
            out
        }
        None => html,
    }
}

/// Check if path is a hashed/immutable static asset.
fn is_immutable_asset(path: &str) -> bool {
    path.starts_with("_app/")
        || path.ends_with(".js")
        || path.ends_with(".css")
        || path.ends_with(".woff2")
        || path.ends_with(".woff")
        || path.ends_with(".ttf")
        || path.ends_with(".png")
        || path.ends_with(".jpg")
        || path.ends_with(".svg")
        || path.ends_with(".ico")
}
