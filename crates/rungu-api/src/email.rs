//! Email notification engine (issue #73).
//!
//! Design: notifications are **inactive by default**. The presence and
//! completeness of SMTP env vars is the toggle — there is no separate
//! feature flag (rationale documented in issue #73).
//!
//! - No SMTP env at all → module disabled, one INFO at startup.
//! - `SMTP_DRIVER=log` → dev/CI mode: render the email to tracing, never send.
//! - `SMTP_HOST` + `SMTP_FROM` set → active. Partial config → exit(1) at
//!   startup (fail fast, same philosophy as `parse_bool_or_exit`).
//!
//! Delivery uses plain SMTP over a tokio TcpStream (STARTTLS via the
//! rustls stack already in the dependency graph — no new dependencies).
//! Events fire-and-forget via `tokio::spawn`, mirroring webhook dispatch.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use rungu_core::Store;

type HmacSha256 = Hmac<Sha256>;

const SMTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

// ── Configuration ─────────────────────────────────────────────────────

/// Resolved email-notification configuration.
#[derive(Debug, Clone)]
pub enum EmailConfig {
    /// Disabled: no SMTP env present. Nothing is ever sent.
    Disabled,
    /// Dev/CI: render emails to tracing instead of sending.
    Log,
    /// Active SMTP relay.
    Smtp(SmtpSettings),
}

#[derive(Debug, Clone)]
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: String,
    /// Public base URL for deep links (e.g. `https://feedback.example.com`).
    pub app_url: String,
}

impl EmailConfig {
    /// Build from env. Fails fast (exits) on partial/invalid configuration.
    pub fn from_env(app_url: &str) -> Self {
        let driver = std::env::var("SMTP_DRIVER").unwrap_or_default();
        let host = std::env::var("SMTP_HOST").ok().filter(|s| !s.trim().is_empty());
        let from = std::env::var("SMTP_FROM").ok().filter(|s| !s.trim().is_empty());
        let user = std::env::var("SMTP_USER").ok().filter(|s| !s.trim().is_empty());
        let pass = std::env::var("SMTP_PASSWORD").ok().filter(|s| !s.trim().is_empty());

        if driver.eq_ignore_ascii_case("log") {
            return EmailConfig::Log;
        }
        if !driver.is_empty() && !driver.eq_ignore_ascii_case("smtp") {
            eprintln!("Invalid SMTP_DRIVER '{driver}' — use 'smtp' or 'log'");
            std::process::exit(1);
        }

        match (host, from) {
            (None, None) => {
                if user.is_some() || pass.is_some() {
                    eprintln!(
                        "SMTP_USER/SMTP_PASSWORD set but SMTP_HOST is missing — \
                         email notifications need a complete SMTP configuration"
                    );
                    std::process::exit(1);
                }
                EmailConfig::Disabled
            }
            (Some(host), Some(from)) => {
                let port: u16 = match std::env::var("SMTP_PORT") {
                    Ok(p) => p.trim().parse().unwrap_or_else(|_| {
                        eprintln!("Invalid SMTP_PORT '{p}' — expected a number (default 587)");
                        std::process::exit(1);
                    }),
                    Err(_) => 587,
                };
                EmailConfig::Smtp(SmtpSettings {
                    host,
                    port,
                    username: user,
                    password: pass,
                    from,
                    app_url: app_url.trim_end_matches('/').to_string(),
                })
            }
            _ => {
                eprintln!(
                    "Partial SMTP configuration: SMTP_HOST and SMTP_FROM must both be set \
                     (or remove all SMTP vars to disable email notifications)"
                );
                std::process::exit(1);
            }
        }
    }
}

// ── Unsubscribe tokens ────────────────────────────────────────────────

/// Sign a per-user unsubscribe token: HMAC-SHA256 over `user_id`, keyed
/// with `APP_SECRET` (same primitive as webhook signatures).
pub fn unsubscribe_token(user_id: &str, app_secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(app_secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(user_id.as_bytes());
    let bytes = mac.finalize().into_bytes();
    format!("{}.{}", user_id, bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Verify an unsubscribe token. Returns the user ID on success.
pub fn verify_unsubscribe_token(token: &str, app_secret: &str) -> Option<String> {
    let (user_id, sig) = token.split_once('.')?;
    let expected = unsubscribe_token(user_id, app_secret);
    // Constant-time compare: both sides are hex strings of equal length.
    let (_, expected_sig) = expected.split_once('.')?;
    if sig.len() == expected_sig.len()
        && sig.bytes().zip(expected_sig.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    {
        Some(user_id.to_string())
    } else {
        None
    }
}

// ── Rendering ─────────────────────────────────────────────────────────

/// Render a notification email (plain text + minimal HTML) with a deep
/// link and per-recipient unsubscribe link.
pub fn render_email(kind: &EmailKind, app_url: &str, token: &str) -> (String, String, String) {
    // (subject, plaintext body, html body)
    match kind {
        EmailKind::NewComment { post_title, post_id, commenter_name, comment_excerpt } => {
            let link = format!("{app_url}/posts/{post_id}");
            let unsub = format!("{app_url}/api/notifications/unsubscribe?token={token}");
            let subject = format!("New comment on \"{post_title}\"");
            let text = format!(
                "{commenter_name} commented on \"{post_title}\":\n\n  {comment_excerpt}\n\nView the discussion:\n{link}\n\n---\nDon't want these emails? Unsubscribe:\n{unsub}\n"
            );
            let esc_title = html_escape(post_title);
            let esc_excerpt = html_escape(comment_excerpt);
            let esc_name = html_escape(commenter_name);
            let html = format!(
                "<p><strong>{esc_name}</strong> commented on <strong>{esc_title}</strong>:</p><blockquote>{esc_excerpt}</blockquote><p><a href=\"{link}\">View the discussion</a></p><hr><p style=\"font-size:12px;color:#666\">Don't want these emails? <a href=\"{unsub}\">Unsubscribe</a></p>"
            );
            (subject, text, html)
        }
        EmailKind::StatusChanged { post_title, post_id, old_status, new_status } => {
            let link = format!("{app_url}/posts/{post_id}");
            let unsub = format!("{app_url}/api/notifications/unsubscribe?token={token}");
            let subject = format!("\"{post_title}\" moved to {new_status}");
            let text = format!(
                "Status update on \"{post_title}\":\n\n  {old_status} → {new_status}\n\nView the post:\n{link}\n\n---\nDon't want these emails? Unsubscribe:\n{unsub}\n"
            );
            let esc_title = html_escape(post_title);
            let html = format!(
                "<p>Status update on <strong>{esc_title}</strong>:</p><p><code>{old_status}</code> → <code>{new_status}</code></p><p><a href=\"{link}\">View the post</a></p><hr><p style=\"font-size:12px;color:#666\">Don't want these emails? <a href=\"{unsub}\">Unsubscribe</a></p>"
            );
            (subject, text, html)
        }
    }
}

pub enum EmailKind {
    NewComment { post_title: String, post_id: String, commenter_name: String, comment_excerpt: String },
    StatusChanged { post_title: String, post_id: String, old_status: String, new_status: String },
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Truncate on a char boundary for email excerpts (never panics on multibyte).
pub(crate) fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

// ── Dispatch ──────────────────────────────────────────────────────────

/// Notify all recipients of an event. Fire-and-forget; never blocks the
/// caller (mirrors `webhook::dispatch_event`). Individual sends —
/// recipient lists are never exposed to other recipients.
pub fn notify_recipients(
    store: std::sync::Arc<Store>,
    config: EmailConfig,
    app_secret: String,
    post_id: &str,
    actor_id: &str,
    kind: EmailKind,
) {
    if matches!(config, EmailConfig::Disabled) {
        return;
    }
    let post_id = post_id.to_string();
    let actor_id = actor_id.to_string();
    tokio::spawn(async move {
        let recipients = match store.notification_recipients(&post_id, &actor_id).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to fetch notification recipients for post {post_id}: {e}");
                return;
            }
        };
        if recipients.is_empty() {
            return;
        }
        for user in recipients {
            let token = unsubscribe_token(&user.id, &app_secret);
            let (subject, text, html) = render_email(&kind, base_url(&config), &token);
            let to = user.email.clone();
            let result = match &config {
                EmailConfig::Disabled => unreachable!(),
                EmailConfig::Log => {
                    tracing::info!(to = %to, subject = %subject, "email (log driver):\n{text}");
                    Ok(())
                }
                EmailConfig::Smtp(s) => smtp_send(s, &s.from, &to, &subject, &text, &html).await,
            };
            if let Err(e) = result {
                tracing::warn!("Email to {to} failed: {e}");
            }
        }
    });
}

fn base_url(config: &EmailConfig) -> &str {
    match config {
        EmailConfig::Smtp(s) => &s.app_url,
        _ => "",
    }
}

// ── Minimal SMTP client ───────────────────────────────────────────────

/// Send one email via SMTP with STARTTLS (EHLO → STARTTLS → EHLO → AUTH →
/// MAIL/RCPT/DATA). Opportunistic plaintext only when the server did not
/// advertise STARTTLS **and** no credentials are configured — we never
/// send AUTH over a plaintext channel.
async fn smtp_send(
    s: &SmtpSettings,
    from: &str,
    to: &str,
    subject: &str,
    text: &str,
    html: &str,
) -> Result<(), String> {
    use base64::Engine;
    use tokio_rustls::rustls::pki_types::ServerName;

    async fn t<F: std::future::Future>(f: F) -> Result<F::Output, String> {
        tokio::time::timeout(SMTP_TIMEOUT, f).await.map_err(|_| "SMTP operation timed out".to_string())
    }

    let stream = match t(TcpStream::connect((s.host.as_str(), s.port))).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(format!("connect failed: {e}")),
        Err(e) => return Err(e),
    };

    // Phase 1: plaintext channel
    let (mut rl, mut wl) = tokio::io::split(stream);

    expect(&mut rl, "220", "greeting").await?;

    // EHLO — collect capability lines (250-... continuation, final 250 ...)
    let caps = ehlo(&mut rl, &mut wl).await?;
    let starttls = caps.iter().any(|c| c.to_uppercase().starts_with("STARTTLS"));

    let (mut rl, mut wl): (
        Box<dyn tokio::io::AsyncRead + Unpin + Send>,
        Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
    ) = if starttls {
        cmd(&mut wl, "STARTTLS").await?;
        expect(&mut rl, "220", "STARTTLS ack").await?;
        let stream = rl.unsplit(wl);
        let connector = tokio_rustls::TlsConnector::from(arc_client_config());
        let name = ServerName::try_from(s.host.clone()).map_err(|e| format!("invalid TLS name: {e}"))?;
        let tls = t(connector.connect(name, stream))
            .await
            .map_err(|_| "TLS handshake timeout".to_string())?
            .map_err(|e| format!("TLS handshake failed: {e}"))?;
        let (a, b) = tokio::io::split(tls);
        (Box::new(a) as _, Box::new(b) as _)
    } else if s.username.is_some() {
        // Refuse to authenticate (or send mail for an authenticated relay) in the clear.
        return Err("server does not advertise STARTTLS — refusing to send credentials in plaintext".into());
    } else {
        (Box::new(rl) as _, Box::new(wl) as _) // plaintext relay, no credentials
    };

    // Phase 2: EHLO again after TLS (RFC 3207), then AUTH
    let _ = ehlo(&mut rl, &mut wl).await?;
    if let (Some(user), Some(pass)) = (&s.username, &s.password) {
        // AUTH PLAIN: base64("\0user\0pass")
        let creds = base64::engine::general_purpose::STANDARD.encode(format!("\0{user}\0{pass}"));
        cmd(&mut wl, &format!("AUTH PLAIN {creds}")).await?;
        let line = read_reply_line(&mut rl).await?;
        if !line.starts_with("235") {
            return Err(format!("AUTH rejected: {line}"));
        }
    }

    cmd(&mut wl, &format!("MAIL FROM:<{from}>")).await?;
    expect(&mut rl, "250", "MAIL FROM").await?;
    cmd(&mut wl, &format!("RCPT TO:<{to}>")).await?;
    expect(&mut rl, "250", "RCPT TO").await?;
    cmd(&mut wl, "DATA").await?;
    expect(&mut rl, "354", "DATA").await?;

    let boundary = "----=_rungu_part_7f3d";
    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S +0000").to_string();
    let msgid = format!("<{}.{}@{}>", rungu_core::new_id(), chrono::Utc::now().timestamp(), s.host);
    let body = format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: {msgid}\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n--{boundary}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{text}\r\n--{boundary}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{html}\r\n--{boundary}--\r\n.\r\n"
    );
    // Dot-stuffing: escape lines starting with '.' inside the body.
    let stuffed = body
        .split("\r\n")
        .map(|l| if l.starts_with('.') { format!(".{l}") } else { l.to_string() })
        .collect::<Vec<_>>()
        .join("\r\n");
    wl.write_all(stuffed.as_bytes()).await.map_err(|e| e.to_string())?;
    expect(&mut rl, "250", "message accepted").await?;

    cmd(&mut wl, "QUIT").await?;
    let _ = wl.shutdown().await;
    Ok(())
}

async fn cmd<W: tokio::io::AsyncWrite + Unpin>(w: &mut W, line: &str) -> Result<(), String> {
    w.write_all(format!("{line}\r\n").as_bytes()).await.map_err(|e| e.to_string())
}

async fn expect<R: tokio::io::AsyncRead + Unpin>(r: &mut R, code: &str, what: &str) -> Result<String, String> {
    let line = read_reply_line(r).await?;
    if line.starts_with(code) { Ok(line) } else { Err(format!("expected {code} after {what}, got: {line}")) }
}

/// Read one SMTP reply line, consuming `-` continuation lines.
async fn read_reply_line<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> Result<String, String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        loop {
            let n = r.read(&mut byte).await.map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("connection closed mid-reply".into());
            }
            buf.push(byte[0]);
            if buf.ends_with(b"\r\n") {
                break;
            }
        }
        let line = String::from_utf8_lossy(&buf).to_string();
        // "250-..." continuation → keep reading; "250 ..." final → done
        let code: String = line.chars().take(3).collect();
        if line.len() > 3 && (line.as_bytes()[3] == b'-') {
            buf.clear();
            continue;
        }
        let _ = code;
        return Ok(line.trim().to_string());
    }
}

async fn ehlo<R: tokio::io::AsyncRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(
    r: &mut R,
    w: &mut W,
) -> Result<Vec<String>, String> {
    cmd(w, "EHLO rungu").await?;
    let mut lines = Vec::new();
    loop {
        let line = read_reply_line(r).await?;
        if !line.starts_with("250") {
            return Err(format!("EHLO failed: {line}"));
        }
        lines.push(line[3..].trim().to_string());
        if line.len() > 3 && line.as_bytes()[3] == b'-' {
            continue;
        }
        return Ok(lines);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

/// Shared rustls client config (webpki roots) for the SMTP client.
fn arc_client_config() -> std::sync::Arc<tokio_rustls::rustls::ClientConfig> {
    let mut roots = tokio_rustls::rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    std::sync::Arc::new(
        tokio_rustls::rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";

    #[test]
    fn unsubscribe_token_roundtrip() {
        let token = unsubscribe_token("user-123", SECRET);
        assert_eq!(verify_unsubscribe_token(&token, SECRET).as_deref(), Some("user-123"));
    }

    #[test]
    fn unsubscribe_token_rejects_tampered() {
        let token = unsubscribe_token("user-123", SECRET);
        let (id, sig) = token.split_once('.').unwrap();
        let bad = format!("user-999.{sig}");
        assert!(verify_unsubscribe_token(&bad, SECRET).is_none());
        assert_ne!(id, "user-999");
        // wrong secret
        assert!(verify_unsubscribe_token(&token, "other").is_none());
    }

    #[test]
    fn render_new_comment_contains_link_and_unsub() {
        let (subject, text, html) = render_email(
            &EmailKind::NewComment {
                post_title: "Dark <mode> bug".into(),
                post_id: "p1".into(),
                commenter_name: "Anaz \"Doe\"".into(),
                comment_excerpt: "It breaks on <svg onload=alert(1)>".into(),
            },
            "https://fb.example.com",
            "tok.sig",
        );
        assert!(subject.contains("Dark"));
        assert!(text.contains("https://fb.example.com/posts/p1"));
        assert!(text.contains("unsubscribe?token=tok.sig"));
        // HTML escaping
        assert!(html.contains("&lt;mode&gt;"));
        assert!(html.contains("&quot;Doe&quot;"));
        assert!(!html.contains("<svg onload"));
    }

    #[test]
    fn render_status_change_has_both_statuses() {
        let (subject, text, _) = render_email(
            &EmailKind::StatusChanged {
                post_title: "T".into(),
                post_id: "p2".into(),
                old_status: "open".into(),
                new_status: "done".into(),
            },
            "https://x.io",
            "t",
        );
        assert!(subject.contains("done"));
        assert!(text.contains("open → done"));
    }

    #[test]
    fn html_escape_covers_all() {
        assert_eq!(html_escape("<a href=\"x\">&</a>"), "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;");
    }
}
