//! Webhook delivery engine — fires HTTP POST with HMAC-SHA256 signing,
//! SSRF protection, and retry with exponential backoff.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::net::IpAddr;
use std::time::Duration;

use rungu_core::Store;
use rungu_proto::{Webhook, WebhookEventType};

type HmacSha256 = Hmac<Sha256>;

const MAX_ATTEMPTS: u32 = 3;
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
const BACKOFF_BASE: Duration = Duration::from_secs(2);

/// Is this IP globally routable? (i.e. NOT private/loopback/link-local/reserved)
fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || v4.is_unspecified()
                // 100.64.0.0/10 CGNAT (operator/carrier-grade NAT)
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 0b0100_0000)
                // 198.18.0.0/15 benchmark testing (RFC 2544)
                || (v4.octets()[0] == 198 && (v4.octets()[1] & 0xFE) == 18))
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped (::ffff:a.b.c.d) must be checked as IPv4.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                // fc00::/7 unique-local
                || (v6.segments()[0] & 0xFE00) == 0xFC00
                // fe80::/10 link-local
                || (v6.segments()[0] & 0xFFC0) == 0xFE80)
        }
    }
}

/// Delivery-time SSRF guard with IP pinning: resolve the webhook host,
/// verify EVERY resolved address is globally routable, then return the
/// validated address so the caller can pin it (reqwest `resolve`) —
/// closing the TOCTOU/DNS-rebinding gap between check and connect.
/// Returns `None` if the host is unsafe or unresolvable.
async fn resolve_safe_addr(host: &str) -> Option<std::net::SocketAddr> {
    // IP-literal hosts are checked directly (port filled later by caller).
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        return if is_public_ip(ip) { Some(std::net::SocketAddr::new(ip, 0)) } else { None };
    }
    // DNS names are resolved; every A/AAAA record must be public.
    match tokio::net::lookup_host((host, 0)).await {
        Ok(addrs) => {
            let mut pinned: Option<std::net::SocketAddr> = None;
            for addr in addrs {
                if !is_public_ip(addr.ip()) {
                    tracing::warn!("Webhook host {host} resolves to non-public IP {} — blocked", addr.ip());
                    return None;
                }
                if pinned.is_none() {
                    pinned = Some(addr); // first validated address, pinned at connect
                }
            }
            pinned // no addresses → None → treated as unsafe
        }
        Err(e) => {
            tracing::warn!("Webhook host {host} failed to resolve: {e} — blocked");
            None
        }
    }
}

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
            // Delivery-time SSRF guard with IP pinning (#141): resolve the host,
            // require every address to be globally routable, then pin the
            // validated address so the connection cannot be DNS-rebound to an
            // internal address between check and connect.
            let url = url::Url::parse(&webhook.url).ok();
            let host = url.as_ref().and_then(|u| u.host_str());
            let pinned = match host {
                Some(h) => resolve_safe_addr(h).await,
                None => None,
            };
            let Some(pinned) = pinned else {
                tracing::warn!("Webhook {} URL host is not safe — skipping delivery", webhook.id);
                if let Err(e) = store
                    .record_webhook_delivery(
                        &webhook.id,
                        event_str,
                        &payload_str,
                        None,
                        false,
                        MAX_ATTEMPTS as i32,
                        "Blocked by SSRF protection (host resolves to non-public address)",
                    )
                    .await
                {
                    tracing::error!("Failed to record webhook delivery: {e}");
                }
                continue;
            };

            let signature = sign_payload(&payload_str, &secret);

            let result = deliver_with_retry(&http, &webhook, &payload_str, &signature, pinned).await;

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
///
/// Uses a dedicated client with redirects DISABLED: following a 302 would
/// let a valid public URL redirect to an internal address at delivery time,
/// bypassing SSRF validation.
/// Single-attempt delivery for the admin "test webhook" endpoint.
/// Same SSRF guards as `deliver_with_retry` (resolve → validate → pin).
pub async fn deliver_once(
    _http: &reqwest::Client,
    webhook: &Webhook,
    payload: &str,
    signature: &str,
) -> Result<i32, (u32, String)> {
    let url = url::Url::parse(&webhook.url).map_err(|e| (1, format!("Invalid webhook URL: {e}")))?;
    let host = url.host_str().unwrap_or_default().to_string();
    let port = url.port_or_known_default().unwrap_or(443);

    let pinned = resolve_safe_addr(&host)
        .await
        .ok_or_else(|| (1, format!("Host {host} does not resolve to a public address")))?;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&host, std::net::SocketAddr::new(pinned.ip(), port))
        .timeout(DELIVERY_TIMEOUT)
        .build()
        .map_err(|e| (1, format!("Failed to build HTTP client: {e}")))?;

    let resp = client
        .post(url.clone())
        .header("Content-Type", "application/json")
        .header("X-Rungu-Event", "webhook.test")
        .header("X-Rungu-Signature", format!("sha256={signature}"))
        .header("User-Agent", "Rungu-Webhook/1.0")
        .body(payload.to_string())
        .send()
        .await
        .map_err(|e| (1, e.to_string()))?;

    let status = resp.status().as_u16() as i32;
    if resp.status().is_success() { Ok(status) } else { Err((1, format!("HTTP {status}"))) }
}

async fn deliver_with_retry(
    _http: &reqwest::Client,
    webhook: &Webhook,
    payload: &str,
    signature: &str,
    pinned: std::net::SocketAddr,
) -> Result<i32, (u32, String)> {
    let url = url::Url::parse(&webhook.url).map_err(|e| (1, format!("Invalid webhook URL: {e}")))?;
    let host = url.host_str().unwrap_or_default().to_string();
    let port = url.port_or_known_default().unwrap_or(443);

    // Client with the validated IP pinned and redirects disabled: the
    // connection goes to the exact address we SSRF-checked, and a 3xx
    // cannot bounce it to an internal host.
    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&host, std::net::SocketAddr::new(pinned.ip(), port))
        .timeout(DELIVERY_TIMEOUT)
        .build()
        .map_err(|e| (1, format!("Failed to build HTTP client: {e}")))?;

    let mut last_err = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        if attempt > 1 {
            let delay = BACKOFF_BASE * 2u32.pow(attempt - 2);
            tokio::time::sleep(delay).await;
        }

        let result = no_redirect_client
            .post(url.clone())
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
    fn test_is_public_ip_blocks_private_ranges() {
        use std::net::IpAddr;
        let blocked = [
            "10.0.0.1",
            "192.168.1.1",
            "172.16.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "0.0.0.0",
            "100.64.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
        ];
        for ip in blocked {
            assert!(!is_public_ip(ip.parse::<IpAddr>().unwrap()), "{ip} must be blocked");
        }
        let public = ["1.1.1.1", "8.8.8.8", "2606:4700::1111"];
        for ip in public {
            assert!(is_public_ip(ip.parse::<IpAddr>().unwrap()), "{ip} must be public");
        }
    }

    #[tokio::test]
    async fn test_resolve_safe_addr_blocks_private_dns_or_unresolvable() {
        // Unresolvable host → None (unsafe).
        assert!(resolve_safe_addr("nonexistent.invalid.example").await.is_none());
        // IP literals: loopback → None, public → Some.
        assert!(resolve_safe_addr("127.0.0.1").await.is_none());
        let pub_pin = resolve_safe_addr("1.1.1.1").await;
        assert!(pub_pin.is_some());
        assert_eq!(pub_pin.unwrap().ip(), "1.1.1.1".parse::<std::net::IpAddr>().unwrap());
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
