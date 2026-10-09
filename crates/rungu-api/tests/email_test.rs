//! End-to-end email notifications (#73): a domain operation publishes an
//! event, `EmailSink` resolves recipients and delivers over SMTP to a fake
//! server on localhost.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use rungu_api::email::{EmailConfig, EmailSink, SmtpSettings};
use rungu_core::ops::{NewPost, PostChanges};
use rungu_core::{Actor, EventBus, NoopStorage, Operations, Store, open_pool, run_migrations};
use rungu_proto::UserRole;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// One received message: envelope recipient + raw DATA.
#[derive(Debug, Clone)]
struct Mail {
    mail_from: String,
    rcpt: String,
    data: String,
}

/// Minimal SMTP server: no STARTTLS, no AUTH, accepts every message.
async fn fake_smtp() -> (u16, Arc<Mutex<Vec<Mail>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let inbox = Arc::new(Mutex::new(Vec::new()));
    let sink = inbox.clone();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else { return };
            let sink = sink.clone();
            tokio::spawn(async move {
                let (r, mut w) = socket.into_split();
                let mut lines = BufReader::new(r).lines();
                w.write_all(b"220 fake ESMTP\r\n").await.unwrap();
                let (mut mail_from, mut rcpt, mut data, mut in_data) =
                    (String::new(), String::new(), String::new(), false);
                while let Ok(Some(line)) = lines.next_line().await {
                    if in_data {
                        if line == "." {
                            in_data = false;
                            sink.lock().unwrap().push(Mail {
                                mail_from: mail_from.clone(),
                                rcpt: rcpt.clone(),
                                data: std::mem::take(&mut data),
                            });
                            w.write_all(b"250 queued\r\n").await.unwrap();
                        } else {
                            data.push_str(&line);
                            data.push('\n');
                        }
                        continue;
                    }
                    let reply: &[u8] = match line.split(' ').next().unwrap_or_default() {
                        "EHLO" => b"250-fake\r\n250 SIZE 1000000\r\n",
                        "MAIL" => {
                            mail_from = line.trim_start_matches("MAIL FROM:").to_string();
                            b"250 ok\r\n"
                        }
                        "RCPT" => {
                            rcpt = line.trim_start_matches("RCPT TO:<").trim_end_matches('>').to_string();
                            b"250 ok\r\n"
                        }
                        "DATA" => {
                            in_data = true;
                            b"354 go\r\n"
                        }
                        "QUIT" => {
                            let _ = w.write_all(b"221 bye\r\n").await;
                            return;
                        }
                        _ => b"250 ok\r\n",
                    };
                    w.write_all(reply).await.unwrap();
                }
            });
        }
    });
    (port, inbox)
}

fn decoded_text_part(data: &str) -> String {
    // The first base64 block after the text/plain part header.
    let after = data.split("Content-Type: text/plain").nth(1).unwrap();
    let b64: String = after.split("\n\n").nth(1).unwrap().split("\n--").next().unwrap().replace('\n', "");
    String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b64).unwrap()).unwrap()
}

#[tokio::test]
async fn comment_and_status_change_email_the_other_participants() {
    let (port, inbox) = fake_smtp().await;

    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    let store = Store::new_with_kind(pool, true);
    store.create_project("Acme", "acme", "").await.unwrap();

    let user = |email: &'static str, name: &'static str| {
        let store = store.clone();
        async move { store.find_or_create_user(email, Some(name), None, &[]).await.unwrap() }
    };
    let author = user("author@x.test", "Author").await;
    let alice = user("alice@x.test", "Alice").await;
    let quiet = user("quiet@x.test", "Quiet").await;
    store.set_notifications_opt_out(&quiet.id, true).await.unwrap();

    let smtp = SmtpSettings {
        host: "127.0.0.1".into(),
        port,
        username: None,
        password: None,
        from: "Rungu <noreply@fb.example.com>".into(),
    };
    let sink = EmailSink::new(store.clone(), EmailConfig::Smtp(smtp), "https://fb.example.com/", "secret").unwrap();
    let ops = Operations::new(store.clone(), Arc::new(NoopStorage), EventBus::new(vec![Arc::new(sink)]));
    let as_actor = |u: &rungu_proto::User| Actor { id: u.id.clone(), role: UserRole::Member };

    // Recipients are resolved at delivery time, so settle each event before
    // the next one to keep the expected recipients deterministic.
    let settle = || async { assert_eq!(rungu_api::background::drain(Duration::from_secs(10)).await, 0) };
    let post = ops
        .create_post(&as_actor(&author), "acme", NewPost { title: "Dark mode", ..Default::default() })
        .await
        .unwrap();
    ops.create_comment(&as_actor(&quiet), &post.id, "me too", None).await.unwrap();
    settle().await;
    ops.create_comment(&as_actor(&alice), &post.id, "Line one\nLine two", None).await.unwrap();
    settle().await;

    // Quiet's comment → author only (Alice hadn't commented yet).
    // Alice's comment → author only (Quiet opted out, Alice is the actor).
    let mails = inbox.lock().unwrap().clone();
    assert_eq!(mails.len(), 2, "{mails:#?}");
    assert!(mails.iter().all(|m| m.rcpt == "author@x.test"));
    // Envelope gets the bare address; the From: header keeps the name.
    assert_eq!(mails[0].mail_from, "<noreply@fb.example.com>");
    assert!(mails[0].data.contains("From: Rungu <noreply@fb.example.com>"));
    let text = decoded_text_part(&mails[1].data);
    assert!(text.contains("Alice commented on \"Dark mode\""), "{text}");
    assert!(text.contains("Line one\nLine two"));
    assert!(text.contains(&format!("https://fb.example.com/board/acme/post/{}", post.id)));
    assert!(mails[1].data.contains("List-Unsubscribe-Post: List-Unsubscribe=One-Click"));

    // The author changes the status → Alice hears about it, the author doesn't.
    ops.update_post(&as_actor(&author), &post.id, PostChanges { status: Some("in_progress"), category: None })
        .await
        .unwrap();
    settle().await;
    let mails = inbox.lock().unwrap().clone();
    assert_eq!(mails.len(), 3);
    assert_eq!(mails[2].rcpt, "alice@x.test");
    assert!(mails[2].data.contains("Subject: \"Dark mode\" moved to In Progress"));
}

#[test]
fn disabled_config_registers_no_sink() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let pool = open_pool("sqlite::memory:").await.unwrap();
        let store = Store::new_with_kind(pool, true);
        assert!(EmailSink::new(store, EmailConfig::Disabled, "https://x", "s").is_none());
    });
}
