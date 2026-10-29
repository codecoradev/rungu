# Email Notifications

Rungu emails the people taking part in a post when something happens to it, so they don't have to keep checking the board.

Notifications are off until SMTP is configured — see [Configuration](/configuration#email-notifications).

## Who gets an email

| Event | Recipients |
|-------|------------|
| New comment | The post's author and everyone who has commented on it |
| Status change (e.g. Planned → In Progress) | The post's author and everyone who has commented on it |

The person who made the change is never emailed about their own action, and users who opted out are skipped. Each recipient gets an individual message, so nobody sees anyone else's address.

## Unsubscribing

Every email has an unsubscribe link and the standard `List-Unsubscribe` headers, so mail clients show a one-click **Unsubscribe** button (RFC 8058).

Opening the link shows a confirmation page; nothing changes until the button is pressed. This keeps email security scanners, which open every link in a message, from unsubscribing people by accident.

## API

```bash
# Read your preference (auth required)
curl -b cookies.txt https://feedback.example.com/api/me/notifications/preferences
# → {"notificationsOptOut": false}

# Opt out (or back in with false)
curl -X POST -b cookies.txt \
  -H "Content-Type: application/json" \
  -d '{"notificationsOptOut": true}' \
  https://feedback.example.com/api/me/notifications/preferences
```

There is no settings page for this yet; opting back in uses the API above.
