//! Input rules shared by every transport (REST, MCP stdio, MCP over HTTP).
//!
//! These used to live next to the REST handlers, so MCP tools couldn't reach
//! them and drifted. [`crate::Operations`] applies them; adapters never
//! re-implement them.

/// Maximum project name length, in bytes after trimming.
pub const MAX_PROJECT_NAME_LEN: usize = 100;
/// Maximum project slug length.
pub const MAX_SLUG_LEN: usize = 80;
/// Maximum webhook URL length.
pub const MAX_WEBHOOK_URL_LEN: usize = 2048;

/// Derive a URL slug from a project name: lowercase ASCII letters and digits,
/// runs of anything else collapsed to one hyphen, no leading/trailing hyphen.
/// May return an empty string (e.g. for a name of only symbols).
pub fn slugify(name: &str) -> String {
    let slug: String =
        name.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() && c.is_ascii() { c } else { '-' }).collect();

    let mut result = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen && !result.is_empty() {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    if result.ends_with('-') {
        result.pop();
    }

    result
}

pub fn is_valid_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.len() > MAX_SLUG_LEN {
        return false;
    }

    if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return false;
    }

    !slug.starts_with('-') && !slug.ends_with('-')
}

/// Write-time check for a webhook target. Delivery re-checks the resolved IP
/// (see the webhook dispatcher), so this is the first of two guards.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic_and_special_chars() {
        assert_eq!(slugify("My App"), "my-app");
        assert_eq!(slugify("My App! @#$"), "my-app");
        assert_eq!(slugify("C++ Project"), "c-project");
        assert_eq!(slugify("My   Multiple   Spaces"), "my-multiple-spaces");
    }

    #[test]
    fn slugify_trims_and_handles_empty_and_unicode() {
        assert_eq!(slugify("  Hello  "), "hello");
        assert_eq!(slugify("---test---"), "test");
        assert_eq!(slugify("   "), "");
        assert_eq!(slugify("Café Résumé"), "caf-r-sum");
    }

    #[test]
    fn is_valid_slug_rules() {
        assert!(is_valid_slug("my-app"));
        assert!(is_valid_slug("my-app-123"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("-myapp"));
        assert!(!is_valid_slug("myapp-"));
        assert!(!is_valid_slug("MyApp"));
        assert!(!is_valid_slug("my app"));
        assert!(!is_valid_slug(&"a".repeat(MAX_SLUG_LEN + 1)));
    }

    #[test]
    fn webhook_url_rules() {
        assert!(validate_webhook_url("https://example.com/webhook").is_ok());
        assert!(validate_webhook_url("http://localhost:3000/webhook").is_ok());
        assert!(validate_webhook_url("http://127.0.0.1:3000/webhook").is_ok());
        assert!(validate_webhook_url("http://example.com/webhook").is_err());
        assert!(validate_webhook_url("ftp://example.com/webhook").is_err());
        assert!(validate_webhook_url("file:///etc/passwd").is_err());
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_webhook_url("https://metadata.google.internal").is_err());
    }
}
