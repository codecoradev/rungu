//! Instance branding + license state — white-label support (#185).
//!
//! Instance-level branding is ENV-driven (`RUNGU_INSTANCE_NAME`,
//! `RUNGU_LOGO_URL`, `RUNGU_FOOTER_TEXT`, `RUNGU_POWERED_BY`). The SPA and the
//! embed board read it via `GET /api/meta` and `x-rungu-*` headers so every
//! surface can present itself under the operator's own brand.
//!
//! The "Powered by Rungu" badge is intentionally NOT removable via env — it is
//! the OSS growth loop. It can only be hidden by a **valid license**:
//! `$49` per major version, `$70` lifetime (owner decision 2026-09-06).
//! License validation talks to the Polar license-key API. The key IS the
//! auth for that endpoint, so no vendor secret lives on the user's server.
//! Apache-2.0 reality: this is a soft gate — an invalid/expired license only
//! makes the badge reappear; the board itself is never throttled or locked.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tokio::sync::RwLock;
use utoipa::ToSchema;

/// Where license keys are validated. Polar is the Merchant of Record for the
/// white-label tier; the customer-portal endpoints accept the key itself as
/// the only credential.
const POLAR_LICENSE_VALIDATE_URL: &str = "https://api.polar.sh/v1/customer-portal/license-keys/validate";

/// How long a validated license stays trusted before revalidation.
const LICENSE_REVALIDATE_SECS: i64 = 7 * 24 * 60 * 60;

// ── Instance branding ──────────────────────────────────────────────────

/// Static per-instance branding, resolved from env at startup.
#[derive(Debug, Clone)]
pub struct InstanceBranding {
    /// Display name in the SPA header, page titles, and embed board.
    pub brand_name: String,
    /// Optional logo URL shown next to the brand name in the SPA header.
    pub logo_url: Option<String>,
    /// Custom footer line shown when the badge is hidden by a license.
    pub footer_text: String,
    /// Whether the badge would show *absent* a license (always `true` for
    /// operators — kept as a field so the badge logic has a single knob).
    pub powered_by: bool,
}

impl Default for InstanceBranding {
    fn default() -> Self {
        Self { brand_name: "Rungu".to_string(), logo_url: None, footer_text: String::new(), powered_by: true }
    }
}

impl InstanceBranding {
    /// Constructor used by the daemon at startup. Input strings must already
    /// be sanitised (see `sanitize_config_str`).
    pub fn new(brand_name: String, logo_url: Option<String>, footer_text: String, powered_by: bool) -> Self {
        Self { brand_name, logo_url, footer_text, powered_by }
    }

    /// Strip control characters and cap length. Brand strings end up in HTML,
    /// HTTP headers, and JSON — keep them boring. Values are interpolated into
    /// templates as text nodes / text content only (never attribute-injected
    /// without escaping), so no HTML-level escaping happens here; the SPA and
    /// embed both treat these as plain text.
    pub fn sanitize_config_str(raw: &str, max: usize) -> String {
        raw.chars().filter(|c| !c.is_control()).take(max).collect()
    }
}

// ── License state ──────────────────────────────────────────────────────

/// Result of the latest successful (or last-known-good) license validation.
#[derive(Debug, Clone)]
pub struct LicenseInfo {
    pub licensed: bool,
    /// When this status was established (unix seconds).
    pub checked_at: i64,
    /// License expiry from Polar, if any (RFC3339 → unix seconds). `None`
    /// means no expiry (lifetime tier). An expired license is `licensed: false`.
    pub expires_at: Option<i64>,
}

/// Shared, interior-mutable license status. `None` = never validated
/// (fresh install without `RUNGU_LICENSE_KEY`) — treated as unlicensed.
#[derive(Default)]
pub struct LicenseStatus(pub RwLock<Option<LicenseInfo>>);

impl LicenseStatus {
    pub fn new() -> Self {
        Self(RwLock::new(None))
    }

    /// Current badge visibility: brand env can't hide it, only a live license can.
    pub async fn badge_visible(&self, branding: &InstanceBranding) -> bool {
        if !branding.powered_by {
            return true;
        }
        let guard = self.0.read().await;
        match guard.as_ref() {
            Some(info) => !info.licensed || Self::expired(info),
            None => true,
        }
    }

    fn expired(info: &LicenseInfo) -> bool {
        matches!(info.expires_at, Some(exp) if exp <= chrono::Utc::now().timestamp())
    }

    /// Cache duration if network validation fails (graceful offline).
    pub fn cache_ttl_secs() -> i64 {
        LICENSE_REVALIDATE_SECS
    }
}

// ── License validation (Polar) ─────────────────────────────────────────

#[derive(serde::Deserialize)]
struct PolarLicenseKey {
    status: String,
    expires_at: Option<String>,
}

/// Validate a license key against the Polar customer-portal API.
///
/// Returns `Ok(LicenseInfo)` when the API answered (granted or not), and
/// `Err(network_message)` when Polar itself was unreachable — callers decide
/// the fail-open/closed policy (we fail open: keep last-known-good).
pub async fn validate_license(http: &reqwest::Client, key: &str, org_id: &str) -> Result<LicenseInfo, String> {
    let resp = http
        .post(POLAR_LICENSE_VALIDATE_URL)
        .json(&serde_json::json!({ "key": key, "organization_id": org_id }))
        .send()
        .await
        .map_err(|e| format!("license API unreachable: {e}"))?;

    if !resp.status().is_success() {
        // 404/402/etc → key not found, revoked, or blocked. Not a network
        // failure — record definitively as unlicensed.
        return Ok(LicenseInfo { licensed: false, checked_at: chrono::Utc::now().timestamp(), expires_at: None });
    }

    let body: PolarLicenseKey = resp.json().await.map_err(|e| format!("license API bad response: {e}"))?;
    let expires_at =
        body.expires_at.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok()).map(|dt| dt.timestamp());

    Ok(LicenseInfo {
        licensed: body.status == "granted"
            && expires_at.map(|exp| exp > chrono::Utc::now().timestamp()).unwrap_or(true),
        checked_at: chrono::Utc::now().timestamp(),
        expires_at,
    })
}

// ── Public meta endpoint ───────────────────────────────────────────────

/// Public instance branding descriptor served by `GET /api/meta`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMeta {
    pub brand_name: String,
    pub logo_url: Option<String>,
    /// Footer line used when the Powered-by badge is licensed away. Empty =
    /// no footer text.
    pub footer_text: String,
    /// Whether the "Powered by Rungu" badge is currently visible.
    pub powered_by: bool,
}

/// `GET /api/meta` — public instance branding (no auth, no secrets).
#[utoipa::path(
    get,
    path = "/api/meta",
    responses(
        (status = 200, description = "Instance branding", body = InstanceMeta),
    ),
    tag = "meta",
)]
pub async fn get_meta(State(state): State<crate::AppState>) -> impl IntoResponse {
    let powered_by = state.license.badge_visible(&state.branding).await;
    let meta = InstanceMeta {
        brand_name: state.branding.brand_name.clone(),
        logo_url: state.branding.logo_url.clone(),
        footer_text: state.branding.footer_text.clone(),
        powered_by,
    };
    (StatusCode::OK, [(header::CACHE_CONTROL, "no-cache")], Json(meta)).into_response()
}

/// Meta routes — merged into the `/api` router.
pub fn meta_routes() -> Router<crate::AppState> {
    Router::new().route("/meta", get(get_meta))
}
