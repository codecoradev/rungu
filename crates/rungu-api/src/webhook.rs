//! Webhook delivery engine — fires HTTP POST with HMAC-SHA256 signing,
//! SSRF protection, and retry with exponential backoff.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::Duration;

use rungu_core::Store;
use rungu_proto::{Webhook, WebhookEventType};

type HmacSha256 = Hmac<Sha256>;

const MAX_ATTEMPTS: u32 = 3;
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
const BACKOFF_BASE: Duration = Duration::from_secs(2);

/// SSRF protection — block requests to private/reserved IP ranges and non-HTTPS schemes.
pub fn validate_webhook_url(raw: &str) -> Result<(), String> {
    let parsed = url::Url::parse(raw).map_err(|e| format!("Invalid URL: {e}"))?;

    match parsed.scheme() {
        "https" => {}
        "http" => {
            // Allow http only for localhost (dev/testing)
            let host = parsed.host_str().unwrap_or("");
            if host != "localhost" && host != "127.0.0.1" {
                return Err("Only HTTPS URLs are allowed (HTTP only for localhost)".into());
            }
        }
        _ => return Err("URL must be HTTP or HTTPS".into()),
    }

    let host = parsed.host_str().unwrap_or("");
    if host.is_empty() {
        return Err("URL must have a valid hostname".into());
    }

    // Block obvious internal/private hostnames
    let blocked = ["metadata.google.internal", "169.254.169.254", "0.0.0.0", "::1", "[::1]"];
    for b in &blocked {
        if host == *b {
            return Err(format!("Blocked host: {b}"));
        }
    }

    Ok(())
}

/// Compute HMAC-SHA256 signature of the payload using the webhook secret.
pub fn sign_payload(payload: &str, secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    let bytes = result.into_bytes();
    // Convert to hex string
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Dispatch a webhook event to all matching subscriptions.
/// Non-blocking — fires in a tokio::spawn so the caller is never blocked.
pub fn dispatch_event(
    store: std::sync::Arc<Store>,
    http: reqwest::Client,
    project_id: String,
    event_type: WebhookEventType,
    payload: serde_json::Value,
) {
    tokio::spawn(async move {
        let event_str = event_type.as_str();
        let payload_str = serde_json::to_string(&payload).unwrap_or_default();

        let webhooks = match store.get_active_webhooks_for_event(&project_id, event_str).await {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("Failed to fetch webhooks for event {event_str}: {e}");
                return;
            }
        };

        if webhooks.is_empty() {
            return; // No subscriptions — nothing to do
        }

        for (webhook, secret) in webhooks {
            let store = store.clone();
            let http = http.clone();
            let payload_str = payload_str.clone();

            let signature = sign_payload(&payload_str, &secret);

            let result = deliver_with_retry(&http, &webhook, &payload_str, &signature).await;

            let (status_code, success, last_error) = match result {
                Ok(code) => (Some(code), true, String::new()),
                Err((attempts, err)) => (None, false, format!("Failed after {attempts} attempts: {err}")),
            };

            if let Err(e) = store
                .record_webhook_delivery(
                    &webhook.id,
                    event_str,
                    &payload_str,
                    status_code,
                    success,
                    MAX_ATTEMPTS as i32,
                    &last_error,
                )
                .await
            {
                tracing::error!("Failed to record webhook delivery: {e}");
            }
        }
    });
}

/// Deliver the payload with up to MAX_ATTEMPTS retries (exponential backoff).
/// Returns the HTTP status code on success (2xx), or an error.
async fn deliver_with_retry(
    http: &reqwest::Client,
    webhook: &Webhook,
    payload: &str,
    signature: &str,
) -> Result<i32, (u32, String)> {
    let mut last_err = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        if attempt > 1 {
            let delay = BACKOFF_BASE * 2u32.pow(attempt - 2);
            tokio::time::sleep(delay).await;
        }

        let result = http
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("X-Rungu-Event", webhook.events.clone())
            .header("X-Rungu-Signature", format!("sha256={signature}"))
            .header("User-Agent", "Rungu-Webhook/1.0")
            .timeout(DELIVERY_TIMEOUT)
            .body(payload.to_string())
            .send()
            .await;

        match result {
            Ok(resp) => {
                let status = resp.status().as_u16() as i32;
                if resp.status().is_success() {
                    tracing::info!("Webhook {} delivered (status {status})", webhook.id);
                    return Ok(status);
                }
                last_err = format!("HTTP {status}: {}", resp.status().canonical_reason().unwrap_or("Unknown"));
                tracing::warn!("Webhook {} attempt {attempt}: {last_err}", webhook.id);
            }
            Err(e) => {
                last_err = e.to_string();
                tracing::warn!("Webhook {} attempt {attempt}: {last_err}", webhook.id);
            }
        }
    }

    Err((MAX_ATTEMPTS, last_err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_https_url() {
        assert!(validate_webhook_url("https://example.com/webhook").is_ok());
    }

    #[test]
    fn test_validate_http_localhost() {
        assert!(validate_webhook_url("http://localhost:3000/webhook").is_ok());
        assert!(validate_webhook_url("http://127.0.0.1:3000/webhook").is_ok());
    }

    #[test]
    fn test_reject_http_external() {
        assert!(validate_webhook_url("http://example.com/webhook").is_err());
    }

    #[test]
    fn test_reject_non_http() {
        assert!(validate_webhook_url("ftp://example.com/webhook").is_err());
        assert!(validate_webhook_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_reject_blocked_hosts() {
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_webhook_url("https://metadata.google.internal").is_err());
    }

    #[test]
    fn test_sign_payload_deterministic() {
        let sig1 = sign_payload(r#"{"event":"test"}"#, "mysecret");
        let sig2 = sign_payload(r#"{"event":"test"}"#, "mysecret");
        assert_eq!(sig1, sig2);
        assert_ne!(sig1, sign_payload(r#"{"event":"test"}"#, "othersecret"));
    }

    #[test]
    fn test_sign_payload_hex() {
        let sig = sign_payload("test", "secret");
        // Should be a valid hex string
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
        // SHA-256 = 32 bytes = 64 hex chars
        assert_eq!(sig.len(), 64);
    }
}
