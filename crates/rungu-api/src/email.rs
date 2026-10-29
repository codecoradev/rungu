//! Email notifications (#73) — an [`EventSink`] on the domain event bus.
//!
//! Who gets mail: the post author and everyone who commented on it, minus
//! the person who acted and anyone who opted out. Two events notify:
//! a new comment and a status change.
//!
//! Notifications are **off by default**; SMTP env is the toggle:
//! - no SMTP env → disabled, the sink is not even registered;
//! - `SMTP_DRIVER=log` → dev/CI: render the email to tracing, never send;
//! - `SMTP_HOST` + `SMTP_FROM` → active. Partial config exits at startup
//!   (fail fast, same as `parse_bool_or_exit`).
//!
//! Delivery is a minimal SMTP client (EHLO → STARTTLS → AUTH PLAIN →
//! MAIL/RCPT/DATA) on the rustls stack reqwest already pulls in. Each
//! recipient gets an individual message with a signed unsubscribe link and
//! RFC 8058 one-click headers.

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use rungu_core::{DomainEvent, EventSink, Store};

type HmacSha256 = Hmac<Sha256>;

const SMTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Longest comment excerpt quoted in an email, in characters.
const EXCERPT_CHARS: usize = 280;

// ── Configuration ─────────────────────────────────────────────────────

/// Resolved email-notification configuration.
#[derive(Debug, Clone)]
pub enum EmailConfig {
    /// No SMTP env present. Nothing is ever sent.
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
}

impl EmailConfig {
    /// Build from env. Exits the process on partial or invalid configuration
    /// so a typo can't silently disable notifications in production.
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|s| !s.trim().is_empty());
        let driver = var("SMTP_DRIVER").unwrap_or_default();
        let (host, from, user, pass) = (var("SMTP_HOST"), var("SMTP_FROM"), var("SMTP_USER"), var("SMTP_PASSWORD"));

        if driver.eq_ignore_ascii_case("log") {
            return EmailConfig::Log;
        }
        if !driver.is_empty() && !driver.eq_ignore_ascii_case("smtp") {
            exit_config(&format!("Invalid SMTP_DRIVER '{driver}' — use 'smtp' or 'log'"));
        }

        match (host, from) {
            (None, None) if user.is_none() && pass.is_none() => EmailConfig::Disabled,
            (None, None) => exit_config(
                "SMTP_USER/SMTP_PASSWORD set but SMTP_HOST and SMTP_FROM are missing — \
                 email notifications need a complete SMTP configuration",
            ),
            (Some(host), Some(from)) => {
                let port = match var("SMTP_PORT") {
                    None => 587,
                    Some(p) => p.trim().parse().unwrap_or_else(|_| {
                        exit_config(&format!("Invalid SMTP_PORT '{p}' — expected a number (default 587)"))
                    }),
                };
                if user.is_some() != pass.is_some() {
                    exit_config("SMTP_USER and SMTP_PASSWORD must be set together");
                }
                EmailConfig::Smtp(SmtpSettings { host, port, username: user, password: pass, from })
            }
            _ => exit_config(
                "Partial SMTP configuration: SMTP_HOST and SMTP_FROM must both be set \
                 (or remove all SMTP vars to disable email notifications)",
            ),
        }
    }

    /// One startup line describing the mode.
    pub fn describe(&self) -> String {
        match self {
            EmailConfig::Disabled => "disabled (no SMTP configuration)".into(),
            EmailConfig::Log => "log driver (dev/CI — nothing is sent)".into(),
            EmailConfig::Smtp(s) => format!("SMTP {}:{} from {}", s.host, s.port, s.from),
        }
    }
}

fn exit_config(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

// ── Unsubscribe tokens ────────────────────────────────────────────────

/// Domain separator: the HMAC key is `APP_SECRET`, which also signs session
/// JWTs, so the signed message must never be a bare user id.
const UNSUBSCRIBE_DOMAIN: &str = "rungu-unsubscribe-v1:";

fn unsubscribe_sig(user_id: &str, app_secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(app_secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(UNSUBSCRIBE_DOMAIN.as_bytes());
    mac.update(user_id.as_bytes());
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Per-user unsubscribe token: `<user_id>.<hmac>`.
pub fn unsubscribe_token(user_id: &str, app_secret: &str) -> String {
    format!("{user_id}.{}", unsubscribe_sig(user_id, app_secret))
}

/// Verify an unsubscribe token (constant-time). Returns the user id.
pub fn verify_unsubscribe_token(token: &str, app_secret: &str) -> Option<String> {
    let (user_id, sig) = token.rsplit_once('.')?;
    let expected = unsubscribe_sig(user_id, app_secret);
    let same =
        sig.len() == expected.len() && sig.bytes().zip(expected.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
    same.then(|| user_id.to_string())
}

// ── What to send ──────────────────────────────────────────────────────

/// The content of one notification, shared by every recipient.
#[derive(Debug, Clone, PartialEq)]
pub enum EmailKind {
    NewComment { post_title: String, commenter_name: String, comment_excerpt: String },
    StatusChanged { post_title: String, old_status: String, new_status: String },
}

/// A notification resolved from a domain event, ready to fan out.
#[derive(Debug, Clone)]
pub struct Notification {
    pub post_id: String,
    pub project_slug: String,
    pub actor_id: String,
    pub kind: EmailKind,
}

/// Map a domain event to a notification, or `None` for events that don't
/// email anyone. Looks up the project slug (for the deep link) and the
/// commenter's display name.
pub async fn notification_for(store: &Store, event: &DomainEvent) -> anyhow::Result<Option<Notification>> {
    let (post_id, kind) = match event {
        DomainEvent::CommentCreated { comment, post_title, .. } => {
            let commenter_name = match store.get_user(&comment.created_by).await? {
                Some(u) if !u.name.trim().is_empty() => u.name,
                Some(u) => u.email.split('@').next().unwrap_or("Someone").to_string(),
                None => "Someone".to_string(),
            };
            let kind = EmailKind::NewComment {
                post_title: post_title.clone(),
                commenter_name,
                comment_excerpt: truncate(&comment.content, EXCERPT_CHARS),
            };
            (comment.post_id.clone(), kind)
        }
        DomainEvent::PostStatusChanged { post, old_status, new_status, .. } => {
            let kind = EmailKind::StatusChanged {
                post_title: post.title.clone(),
                old_status: status_label(*old_status).to_string(),
                new_status: status_label(*new_status).to_string(),
            };
            (post.id.clone(), kind)
        }
        _ => return Ok(None),
    };
    let Some(project) = store.get_project_by_id(event.project_id()).await? else {
        return Ok(None);
    };
    Ok(Some(Notification { post_id, project_slug: project.slug, actor_id: event.actor_id().to_string(), kind }))
}

fn status_label(status: rungu_proto::PostStatus) -> &'static str {
    use rungu_proto::PostStatus as S;
    match status {
        S::Open => "Open",
        S::Planned => "Planned",
        S::InProgress => "In Progress",
        S::Done => "Done",
        S::Declined => "Declined",
    }
}

/// A rendered email: subject plus plain-text and HTML bodies.
#[derive(Debug, Clone)]
pub struct RenderedEmail {
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// Render a notification for one recipient (`unsubscribe_url` is per user).
pub fn render_email(n: &Notification, app_url: &str, unsubscribe_url: &str) -> RenderedEmail {
    let link = format!("{app_url}/board/{}/post/{}", n.project_slug, n.post_id);
    let footer_text = format!("\n\n---\nDon't want these emails? Unsubscribe:\n{unsubscribe_url}\n");
    let footer_html = format!(
        "<hr><p style=\"font-size:12px;color:#666\">Don't want these emails? <a href=\"{}\">Unsubscribe</a></p>",
        html_escape(unsubscribe_url)
    );
    let esc_link = html_escape(&link);
    match &n.kind {
        EmailKind::NewComment { post_title, commenter_name, comment_excerpt } => RenderedEmail {
            subject: format!("New comment on \"{post_title}\""),
            text: format!(
                "{commenter_name} commented on \"{post_title}\":\n\n  {comment_excerpt}\n\nView the discussion:\n{link}{footer_text}"
            ),
            html: format!(
                "<p><strong>{}</strong> commented on <strong>{}</strong>:</p><blockquote>{}</blockquote>\
                 <p><a href=\"{esc_link}\">View the discussion</a></p>{footer_html}",
                html_escape(commenter_name),
                html_escape(post_title),
                html_escape(comment_excerpt).replace('\n', "<br>"),
            ),
        },
        EmailKind::StatusChanged { post_title, old_status, new_status } => RenderedEmail {
            subject: format!("\"{post_title}\" moved to {new_status}"),
            text: format!(
                "Status update on \"{post_title}\":\n\n  {old_status} → {new_status}\n\nView the post:\n{link}{footer_text}"
            ),
            html: format!(
                "<p>Status update on <strong>{}</strong>:</p><p>{old_status} → <strong>{new_status}</strong></p>\
                 <p><a href=\"{esc_link}\">View the post</a></p>{footer_html}",
                html_escape(post_title),
            ),
        },
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// Truncate on a char boundary (never panics on multibyte).
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

// ── Event sink ────────────────────────────────────────────────────────

/// Emails post participants about new comments and status changes.
pub struct EmailSink {
    store: Store,
    config: EmailConfig,
    app_url: String,
    app_secret: String,
}

impl EmailSink {
    /// `None` when notifications are disabled — nothing to subscribe.
    pub fn new(store: Store, config: EmailConfig, app_url: &str, app_secret: &str) -> Option<Self> {
        if matches!(config, EmailConfig::Disabled) {
            return None;
        }
        Some(Self {
            store,
            config,
            app_url: app_url.trim_end_matches('/').to_string(),
            app_secret: app_secret.to_string(),
        })
    }
}

impl EventSink for EmailSink {
    fn publish(&self, event: &DomainEvent) {
        if !matches!(event, DomainEvent::CommentCreated { .. } | DomainEvent::PostStatusChanged { .. }) {
            return;
        }
        let (store, config, app_url, app_secret) =
            (self.store.clone(), self.config.clone(), self.app_url.clone(), self.app_secret.clone());
        let event = event.clone();
        crate::background::spawn(async move {
            let notification = match notification_for(&store, &event).await {
                Ok(Some(n)) => n,
                Ok(None) => return,
                Err(e) => return tracing::error!("Email notification lookup failed: {e:#}"),
            };
            let recipients = match store.notification_recipients(&notification.post_id, &notification.actor_id).await {
                Ok(r) => r,
                Err(e) => return tracing::error!("Failed to fetch notification recipients: {e:#}"),
            };
            for user in recipients {
                let unsubscribe_url = format!(
                    "{app_url}/api/notifications/unsubscribe?token={}",
                    unsubscribe_token(&user.id, &app_secret)
                );
                let email = render_email(&notification, &app_url, &unsubscribe_url);
                let result = match &config {
                    EmailConfig::Smtp(s) => smtp_send(s, &user.email, &email, &unsubscribe_url).await,
                    EmailConfig::Log | EmailConfig::Disabled => {
                        tracing::info!(to = %user.email, subject = %email.subject, "email (log driver):\n{}", email.text);
                        Ok(())
                    }
                };
                if let Err(e) = result {
                    tracing::warn!("Email to {} failed: {e}", user.email);
                }
            }
        });
    }
}

// ── Message assembly ──────────────────────────────────────────────────

/// A header value must be one line: CR/LF in user content (a post title)
/// would otherwise inject headers.
fn header_safe(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// RFC 2047 encoded-word for non-ASCII header text.
fn encode_header(s: &str) -> String {
    let s = header_safe(s);
    if s.is_ascii() { s } else { format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(s)) }
}

/// Base64 in 76-char CRLF lines: no bare LF, no over-long lines, and no
/// line can start with '.', so DATA needs no dot-stuffing.
fn base64_body(s: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(s);
    encoded.as_bytes().chunks(76).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("\r\n")
}

/// The full RFC 5322 message, terminated for SMTP DATA.
fn build_message(from: &str, to: &str, email: &RenderedEmail, unsubscribe_url: &str) -> String {
    let boundary = format!("rungu-{}", rungu_core::new_id());
    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S +0000");
    let domain = from.rsplit_once('@').map(|(_, d)| d.trim_end_matches('>')).unwrap_or("rungu.local");
    let msgid = format!("<{}@{}>", rungu_core::new_id(), header_safe(domain));
    format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: {msgid}\r\n\
         List-Unsubscribe: <{unsub}>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n\
         MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n\
         --{boundary}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{text}\r\n\
         --{boundary}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{html}\r\n\
         --{boundary}--\r\n.\r\n",
        from = header_safe(from),
        to = header_safe(to),
        subject = encode_header(&email.subject),
        unsub = header_safe(unsubscribe_url),
        text = base64_body(&email.text),
        html = base64_body(&email.html),
    )
}

// ── Minimal SMTP client ───────────────────────────────────────────────

/// Send one email (EHLO → STARTTLS → EHLO → AUTH → MAIL/RCPT/DATA).
/// Plaintext is allowed only when the server doesn't offer STARTTLS **and**
/// no credentials are configured — AUTH is never sent in the clear.
async fn smtp_send(s: &SmtpSettings, to: &str, email: &RenderedEmail, unsubscribe_url: &str) -> Result<(), String> {
    use tokio_rustls::rustls::pki_types::ServerName;

    async fn timed<F: std::future::Future>(f: F) -> Result<F::Output, String> {
        tokio::time::timeout(SMTP_TIMEOUT, f).await.map_err(|_| "SMTP operation timed out".to_string())
    }

    let stream =
        timed(TcpStream::connect((s.host.as_str(), s.port))).await?.map_err(|e| format!("connect failed: {e}"))?;
    let (mut rl, mut wl) = tokio::io::split(stream);

    expect(&mut rl, "220", "greeting").await?;
    let caps = ehlo(&mut rl, &mut wl).await?;
    let starttls = caps.iter().any(|c| c.to_uppercase().starts_with("STARTTLS"));

    let (mut rl, mut wl): (
        Box<dyn tokio::io::AsyncRead + Unpin + Send>,
        Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
    ) = if starttls {
        cmd(&mut wl, "STARTTLS").await?;
        expect(&mut rl, "220", "STARTTLS").await?;
        let stream = rl.unsplit(wl);
        let connector = tokio_rustls::TlsConnector::from(tls_client_config());
        let name = ServerName::try_from(s.host.clone()).map_err(|e| format!("invalid TLS name: {e}"))?;
        let tls = timed(connector.connect(name, stream)).await?.map_err(|e| format!("TLS handshake failed: {e}"))?;
        let (a, b) = tokio::io::split(tls);
        (Box::new(a), Box::new(b))
    } else if s.username.is_some() {
        return Err("server does not offer STARTTLS — refusing to send credentials in plaintext".into());
    } else {
        (Box::new(rl), Box::new(wl))
    };

    if starttls {
        ehlo(&mut rl, &mut wl).await?; // RFC 3207: capabilities reset after TLS
    }
    if let (Some(user), Some(pass)) = (&s.username, &s.password) {
        let creds = base64::engine::general_purpose::STANDARD.encode(format!("\0{user}\0{pass}"));
        cmd(&mut wl, &format!("AUTH PLAIN {creds}")).await?;
        expect(&mut rl, "235", "AUTH").await?;
    }

    cmd(&mut wl, &format!("MAIL FROM:<{}>", header_safe(addr_spec(&s.from)))).await?;
    expect(&mut rl, "250", "MAIL FROM").await?;
    cmd(&mut wl, &format!("RCPT TO:<{}>", header_safe(addr_spec(to)))).await?;
    expect(&mut rl, "25", "RCPT TO").await?; // 250 ok, 251 will forward
    cmd(&mut wl, "DATA").await?;
    expect(&mut rl, "354", "DATA").await?;
    let message = build_message(&s.from, to, email, unsubscribe_url);
    timed(wl.write_all(message.as_bytes())).await?.map_err(|e| e.to_string())?;
    expect(&mut rl, "250", "message").await?;

    let _ = cmd(&mut wl, "QUIT").await;
    let _ = wl.shutdown().await;
    Ok(())
}

async fn cmd<W: tokio::io::AsyncWrite + Unpin>(w: &mut W, line: &str) -> Result<(), String> {
    tokio::time::timeout(SMTP_TIMEOUT, w.write_all(format!("{line}\r\n").as_bytes()))
        .await
        .map_err(|_| "SMTP write timed out".to_string())?
        .map_err(|e| e.to_string())
}

/// The bare address SMTP envelopes need: `Rungu <a@b.com>` → `a@b.com`.
/// The `From:` header keeps the display name.
fn addr_spec(mailbox: &str) -> &str {
    match (mailbox.rfind('<'), mailbox.rfind('>')) {
        (Some(open), Some(close)) if open < close => mailbox[open + 1..close].trim(),
        _ => mailbox.trim(),
    }
}

async fn expect<R: tokio::io::AsyncRead + Unpin>(r: &mut R, code: &str, what: &str) -> Result<String, String> {
    let line = read_reply(r).await?;
    if line.starts_with(code) { Ok(line) } else { Err(format!("expected {code} after {what}, got: {line}")) }
}

/// Read one CRLF-terminated line (capped at 4 KiB) with a timeout.
async fn read_line<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> Result<String, String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n") && buf.len() <= 4096 {
        let n = tokio::time::timeout(SMTP_TIMEOUT, r.read(&mut byte))
            .await
            .map_err(|_| "SMTP read timed out".to_string())?
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connection closed mid-reply".into());
        }
        buf.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&buf).trim_end().to_string())
}

/// `250-SIZE` continues a multi-line reply; `250 OK` ends it.
fn is_continuation(line: &str) -> bool {
    line.as_bytes().get(3) == Some(&b'-')
}

/// Read one SMTP reply, returning its final line.
async fn read_reply<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> Result<String, String> {
    loop {
        let line = read_line(r).await?;
        if !is_continuation(&line) {
            return Ok(line);
        }
    }
}

/// EHLO and return the advertised capabilities.
async fn ehlo<R: tokio::io::AsyncRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(
    r: &mut R,
    w: &mut W,
) -> Result<Vec<String>, String> {
    cmd(w, "EHLO rungu").await?;
    let mut caps = Vec::new();
    loop {
        let line = read_line(r).await?;
        if !line.starts_with("250") {
            return Err(format!("EHLO failed: {line}"));
        }
        caps.push(line.get(4..).unwrap_or_default().to_string());
        if !is_continuation(&line) {
            return Ok(caps);
        }
    }
}

fn tls_client_config() -> std::sync::Arc<tokio_rustls::rustls::ClientConfig> {
    let mut roots = tokio_rustls::rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    std::sync::Arc::new(
        tokio_rustls::rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth(),
    )
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";

    fn notification(kind: EmailKind) -> Notification {
        Notification { post_id: "p1".into(), project_slug: "acme".into(), actor_id: "u1".into(), kind }
    }

    #[test]
    fn unsubscribe_token_roundtrip_and_tamper() {
        let token = unsubscribe_token("user-123", SECRET);
        assert_eq!(verify_unsubscribe_token(&token, SECRET).as_deref(), Some("user-123"));

        let (_, sig) = token.rsplit_once('.').unwrap();
        assert!(verify_unsubscribe_token(&format!("user-999.{sig}"), SECRET).is_none());
        assert!(verify_unsubscribe_token(&token, "other-secret").is_none());
        assert!(verify_unsubscribe_token("no-dot", SECRET).is_none());
    }

    #[test]
    fn new_comment_links_to_the_board_post_and_escapes_html() {
        let n = notification(EmailKind::NewComment {
            post_title: "Dark <mode> bug".into(),
            commenter_name: "Anaz \"Doe\"".into(),
            comment_excerpt: "It breaks on <svg onload=alert(1)>".into(),
        });
        let email = render_email(&n, "https://fb.example.com", "https://fb.example.com/unsub?t=x");
        assert!(email.text.contains("https://fb.example.com/board/acme/post/p1"));
        assert!(email.text.contains("https://fb.example.com/unsub?t=x"));
        assert!(email.html.contains("&lt;mode&gt;") && email.html.contains("&quot;Doe&quot;"));
        assert!(!email.html.contains("<svg onload"));
    }

    #[test]
    fn status_change_uses_readable_labels() {
        let n = notification(EmailKind::StatusChanged {
            post_title: "T".into(),
            old_status: status_label(rungu_proto::PostStatus::Open).into(),
            new_status: status_label(rungu_proto::PostStatus::InProgress).into(),
        });
        let email = render_email(&n, "https://x.io", "u");
        assert_eq!(email.subject, "\"T\" moved to In Progress");
        assert!(email.text.contains("Open → In Progress"));
    }

    #[test]
    fn message_headers_cannot_be_injected() {
        let email = RenderedEmail {
            subject: "Hi\r\nBcc: victim@example.com".into(),
            text: "line one\nline two\n.\nnot the end".into(),
            html: "<p>x</p>".into(),
        };
        let msg = build_message("Rungu <noreply@fb.example.com>", "to@example.com", &email, "https://x/u?t=1");
        let headers = msg.split("\r\n\r\n").next().unwrap();
        assert!(!headers.lines().any(|l| l.starts_with("Bcc:")), "CRLF in the subject must not start a header");
        assert!(headers.contains("List-Unsubscribe: <https://x/u?t=1>"));
        assert!(headers.contains("List-Unsubscribe-Post: List-Unsubscribe=One-Click"));
        // Body is base64: no bare LF and no line that could end DATA early.
        assert!(!msg.replace("\r\n", "").contains('\n'));
        assert_eq!(msg.matches("\r\n.\r\n").count(), 1);
        assert!(msg.ends_with("\r\n.\r\n"));
    }

    #[test]
    fn non_ascii_subject_is_rfc2047_encoded() {
        assert_eq!(encode_header("plain"), "plain");
        let encoded = encode_header("Mode gelap — tolong");
        assert!(encoded.starts_with("=?UTF-8?B?") && encoded.ends_with("?="));
    }

    #[test]
    fn envelope_uses_the_bare_address() {
        assert_eq!(addr_spec("Rungu <feedback@example.com>"), "feedback@example.com");
        assert_eq!(addr_spec("\"A <b>\" <real@example.com>"), "real@example.com");
        assert_eq!(addr_spec("  plain@example.com "), "plain@example.com");
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("héllo", 3), "hél…");
        assert_eq!(truncate("hi", 3), "hi");
    }
}
