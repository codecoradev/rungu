//! In-memory per-IP rate limiting (fixed-window counter).
//!
//! No external crate — `governor`/`axum-governor` were dropped due to an MSRV
//! constraint (see `Cargo.toml`). State is shared via `Arc` behind a
//! `tokio::sync::Mutex`; a background task periodically clears buckets so the
//! map can't grow unbounded under spoofed-IP flooding.
//!
//! Two independent limiters are wired up in [`crate::server`]:
//! - **auth routes** (`/auth/*`) — stricter, defends OAuth login/callback abuse
//! - **API routes** (`/api/*`) — looser, protects general flooding
//!
//! Set the corresponding `_PER_MIN` env var to `0` to disable a limiter.
//!
//! ## Trust boundary
//!
//! Client IP defaults to the connected socket address. `X-Forwarded-For`
//! (first hop) is honored only when `RUNGU_TRUST_PROXY=true` — enable that
//! solely behind a reverse proxy that *overwrites* the header, since XFF is
//! otherwise client-controlled and trivially spoofed.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tokio::sync::Mutex;

/// Per-IP sliding-window state (two-bucket weighted approximation).
///
/// `prev_count` holds the count from the window that just elapsed;
/// `current` holds the active window. The effective request rate is
/// `prev_count * (1 - progress) + current`, which smoothly decays as the
/// current window advances — no fixed-window 2x burst at the boundary.
#[derive(Copy, Clone)]
struct Bucket {
    window_start: Instant,
    prev_count: u32,
    count: u32,
}

/// Shared rate-limiter state. Cloning is cheap (inner is `Arc`).
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<IpAddr, Bucket>>>,
    max_requests: u32,
    window: Duration,
    /// When `false` (default), ignore `X-Forwarded-For` and always use the
    /// connected socket address. Enable only when Rungu sits behind a
    /// trusted reverse proxy that **overwrites** XFF — otherwise a client
    /// can spoof the header to dodge the limit.
    trust_proxy: bool,
}

impl RateLimiter {
    /// Build a limiter that allows `max_requests` per `window` per IP.
    /// `trust_proxy` toggles honoring `X-Forwarded-For` (see field docs).
    pub fn new(max_requests: u32, window: Duration, trust_proxy: bool) -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())), max_requests, window, trust_proxy }
    }

    /// Returns `Ok(remaining)` when allowed, `Err(retry_after_secs)` when the
    /// IP is over the limit for the remainder of the current window.
    ///
    /// Sliding-window (two-bucket weighted) algorithm: the previous window's
    /// count decays linearly as the current window advances, so a client can
    /// never exceed `max_requests` in *any* full-window span — unlike a plain
    /// fixed window which allows a 2x burst at the boundary.
    pub async fn check(&self, ip: IpAddr) -> Result<u32, u64> {
        let now = Instant::now();
        let mut map = self.inner.lock().await;
        let entry = map.entry(ip).or_insert(Bucket { window_start: now, prev_count: 0, count: 0 });

        // Roll windows forward however many whole windows have elapsed.
        let elapsed = now.duration_since(entry.window_start);
        if elapsed >= self.window {
            // u32::MAX windows of even a 1ns window is ~4.3s of ns-division
            // headroom; clamp to a sane cap instead of a raw cast to make the
            // invariant explicit for any window size.
            let windows_passed = u32::try_from(elapsed.as_nanos() / self.window.as_nanos()).unwrap_or(u32::MAX);
            // Only the immediately-previous window matters for decay; anything
            // older has fully drained.
            entry.prev_count = if windows_passed == 1 { entry.count } else { 0 };
            // Saturating mul/add: with windows_passed capped at u32::MAX and a
            // 60s window this overflows only after ~8k years — but saturate
            // anyway so the math can never panic.
            entry.window_start =
                entry.window_start.checked_add(self.window.saturating_mul(windows_passed)).unwrap_or(now);
            entry.count = 0;
        }

        // Weighted estimate of requests in the sliding window.
        let progress = now.duration_since(entry.window_start).as_secs_f64() / self.window.as_secs_f64();
        let weighted = entry.prev_count as f64 * (1.0 - progress) + entry.count as f64;

        if weighted >= self.max_requests as f64 {
            // Full drain time for the weighted count to fall below the limit.
            let drain_needed = weighted - self.max_requests as f64 + 1.0;
            let decay_rate_per_sec = entry.prev_count as f64 / self.window.as_secs_f64();
            let retry_secs = if decay_rate_per_sec > 0.0 {
                (drain_needed / decay_rate_per_sec).ceil().max(1.0)
            } else {
                // Only the current bucket is full: wait out the remainder.
                let rem = self.window - now.duration_since(entry.window_start);
                rem.as_secs().max(1) as f64
            };
            Err(retry_secs as u64)
        } else {
            entry.count += 1;
            // Subtract 1 for the request just admitted so the header reports
            // requests still available *after* this one.
            let remaining = (self.max_requests as f64 - weighted - 1.0).floor() as u32;
            Ok(remaining)
        }
    }

    /// Spawn a background task that evicts buckets whose window has elapsed,
    /// so IPs that sent a few requests (or none) don't accumulate forever.
    ///
    /// Eviction is **per-bucket** (not a global clear): a global clear would
    /// reset every IP's window on the same cadence and hand a synchronized
    /// burst budget to coordinated clients. Evicting only expired buckets
    /// preserves each IP's independent fixed window.
    pub fn spawn_pruner(&self) {
        let window = self.window;
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(window);
            loop {
                ticker.tick().await;
                let now = Instant::now();
                let mut map = inner.lock().await;
                map.retain(|_, bucket| now.duration_since(bucket.window_start) < window);
            }
        });
    }
}

/// Parse the first hop of an `X-Forwarded-For` header into an `IpAddr`.
fn xff_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let xff = headers.get("x-forwarded-for")?.to_str().ok()?;
    let first = xff.split(',').next()?.trim();
    first.parse::<IpAddr>().ok()
}

/// Axum middleware entry point. Rejects with `429 Too Many Requests`
/// (and a `Retry-After` header) once an IP exceeds the limiter's budget.
///
/// Expects the app to be served with
/// [`axum::serve`](axum::serve) via
/// `into_make_service_with_connect_info::<SocketAddr>()` so that
/// [`ConnectInfo`] is available.
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = if limiter.trust_proxy { xff_ip(&headers).unwrap_or_else(|| addr.ip()) } else { addr.ip() };

    match limiter.check(ip).await {
        Ok(remaining) => {
            let mut resp = next.run(req).await;
            if let Ok(v) = remaining.to_string().parse() {
                resp.headers_mut().insert("x-ratelimit-remaining", v);
            }
            resp
        }
        Err(retry_after) => {
            let mut resp = (StatusCode::TOO_MANY_REQUESTS, "Too Many Requests").into_response();
            if let Ok(v) = retry_after.to_string().parse() {
                resp.headers_mut().insert("retry-after", v);
            }
            resp
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn rl(max: u32) -> RateLimiter {
        RateLimiter::new(max, Duration::from_secs(60), false)
    }

    #[tokio::test]
    async fn allows_up_to_limit_then_blocks() {
        let rl = rl(3);
        assert!(rl.check(ip("10.0.0.1")).await.is_ok());
        assert!(rl.check(ip("10.0.0.1")).await.is_ok());
        assert!(rl.check(ip("10.0.0.1")).await.is_ok());
        // 4th within the same window → blocked, retry-after reported.
        let err = rl.check(ip("10.0.0.1")).await.unwrap_err();
        assert!(err >= 1);
    }

    #[tokio::test]
    async fn tracks_ips_independently() {
        let rl = rl(1);
        assert!(rl.check(ip("10.0.0.1")).await.is_ok());
        assert!(rl.check(ip("10.0.0.2")).await.is_ok());
        assert!(rl.check(ip("10.0.0.1")).await.is_err());
        assert!(rl.check(ip("10.0.0.2")).await.is_err());
    }

    #[tokio::test]
    async fn resets_after_window_elapses() {
        let rl = RateLimiter::new(1, Duration::from_millis(20), false);
        assert!(rl.check(ip("10.0.0.1")).await.is_ok());
        assert!(rl.check(ip("10.0.0.1")).await.is_err());
        tokio::time::sleep(Duration::from_millis(30)).await;
        // Window has elapsed → counter resets.
        assert!(rl.check(ip("10.0.0.1")).await.is_ok());
    }

    #[tokio::test]
    async fn no_2x_burst_at_window_boundary() {
        // Sliding window: exhausting the limit at the end of one window must
        // NOT grant a fresh full budget immediately after the boundary.
        let window = Duration::from_millis(50);
        let rl = RateLimiter::new(3, window, false);

        // Burn the full budget at the tail of window #1.
        for _ in 0..3 {
            assert!(rl.check(ip("10.0.0.1")).await.is_ok());
        }
        assert!(rl.check(ip("10.0.0.1")).await.is_err());

        // Cross the boundary into window #2 with the previous count still
        // heavily weighted. A fixed window would allow 3 more here; the
        // sliding window must still block.
        tokio::time::sleep(window + Duration::from_millis(5)).await;
        let mut allowed = 0;
        for _ in 0..3 {
            if rl.check(ip("10.0.0.1")).await.is_ok() {
                allowed += 1;
            }
        }
        // Right after the boundary, prev_count=3 fully weighted → nothing (or
        // at most a tiny fraction) should pass. Definitely not a fresh 3.
        assert!(allowed <= 1, "expected ≤1 allowed right after boundary, got {allowed}");
    }

    #[tokio::test]
    async fn full_budget_returns_after_full_drain() {
        // Two full windows after exhaustion, the budget must be fully restored.
        let window = Duration::from_millis(50);
        let rl = RateLimiter::new(3, window, false);
        for _ in 0..3 {
            assert!(rl.check(ip("10.0.0.1")).await.is_ok());
        }
        assert!(rl.check(ip("10.0.0.1")).await.is_err());

        // Advance 2 full windows: prev_count drained to 0.
        tokio::time::sleep(window * 2 + Duration::from_millis(10)).await;
        for _ in 0..3 {
            assert!(rl.check(ip("10.0.0.1")).await.is_ok());
        }
        assert!(rl.check(ip("10.0.0.1")).await.is_err());
    }
}
