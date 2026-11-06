//! MCP protocol framing: version negotiation, discovery and the standard
//! `tools/call` envelope, shared by the stdio and HTTP transports.
//!
//! Two eras are served from one dispatcher:
//!
//! - **Modern** (`2026-07-28`, stateless): no `initialize` handshake. Every
//!   request carries `_meta["io.modelcontextprotocol/protocolVersion"]`, and
//!   results carry `resultType` plus the server's identity in `_meta`.
//!   Clients open with `server/discover`.
//! - **Legacy** (`2025-03-26` … `2025-11-25`): `initialize` →
//!   `notifications/initialized` → `tools/list` / `tools/call`.
//!
//! The pre-existing bare-method calls (`{"method": "list_posts"}`) keep
//! working as an extension, so stdio scripts written before standard framing
//! do not break.

use serde_json::{Value, json};

/// The stateless protocol revision.
pub const MODERN_VERSION: &str = "2026-07-28";
/// Handshake-based revisions still answered, newest first.
pub const LEGACY_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// How long clients may cache `server/discover` (public: it names no data).
const DISCOVER_TTL_MS: u64 = 3_600_000;
/// How long clients may cache `tools/list`.
const TOOLS_TTL_MS: u64 = 300_000;

/// Which protocol era a request speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Era {
    Modern,
    Legacy,
}

/// The era of a request: modern when its `params._meta` names the stateless
/// protocol version.
pub fn era_of(params: &Value) -> Era {
    match params.pointer("/_meta").and_then(|m| m.get(META_VERSION)).and_then(Value::as_str) {
        Some(MODERN_VERSION) => Era::Modern,
        _ => Era::Legacy,
    }
}

/// Every version this server answers, newest first.
pub fn supported_versions() -> Vec<&'static str> {
    std::iter::once(MODERN_VERSION).chain(LEGACY_VERSIONS.iter().copied()).collect()
}

fn server_info() -> Value {
    json!({ "name": "rungu", "version": env!("CARGO_PKG_VERSION") })
}

const INSTRUCTIONS: &str = "Rungu is a self-hosted feedback board. Use list_projects to find a board, then \
list_posts, search_posts, get_roadmap and get_changelog to read it. Writes act as the authenticated identity.";

/// `server/discover`: what this server speaks, available before any other call.
pub fn discover() -> Value {
    json!({
        "resultType": "complete",
        "supportedVersions": supported_versions(),
        "capabilities": { "tools": {} },
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
        "ttlMs": DISCOVER_TTL_MS,
        "cacheScope": "public",
    })
}

/// Legacy `initialize`: echo the client's version when it is one we serve,
/// otherwise answer with our newest legacy version and let the client decide.
pub fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested.filter(|v| LEGACY_VERSIONS.contains(v)).unwrap_or(LEGACY_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
    })
}

/// Add the fields every modern result carries.
pub fn frame_modern(mut result: Value) -> Value {
    if let Some(obj) = result.as_object_mut() {
        obj.entry("resultType").or_insert_with(|| json!("complete"));
        let meta = obj.entry("_meta").or_insert_with(|| json!({}));
        if let Some(meta) = meta.as_object_mut() {
            meta.insert(META_SERVER_INFO.into(), server_info());
        }
    }
    result
}

/// Cache hints for a modern `tools/list` (per-caller: access may differ).
pub fn with_tools_cache(mut list: Value) -> Value {
    if let Some(obj) = list.as_object_mut() {
        obj.insert("ttlMs".into(), json!(TOOLS_TTL_MS));
        obj.insert("cacheScope".into(), json!("private"));
    }
    list
}

/// A `tools/call` result carrying the tool's JSON as text and as structured
/// content.
pub fn tool_result(value: &Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": value.to_string() }],
        "structuredContent": value,
        "isError": false,
    })
}

/// A tool failure the model can read and react to (as opposed to a protocol error).
pub fn tool_failure(kind: &str, message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "structuredContent": { "error": { "kind": kind, "message": message } },
        "isError": true,
    })
}

/// Why a request's HTTP headers were refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderError {
    /// `MCP-Protocol-Version` names a revision we don't serve (`-32022`).
    UnsupportedVersion(String),
    /// A header is missing or disagrees with the body (`-32020`).
    Mismatch(String),
}

impl HeaderError {
    /// The JSON-RPC error body for the HTTP 400 response.
    pub fn to_json(&self, id: Option<Value>) -> Value {
        let error = match self {
            Self::UnsupportedVersion(v) => json!({
                "code": -32022,
                "message": format!("Unsupported protocol version: {v}"),
                "data": { "supported": supported_versions() },
            }),
            Self::Mismatch(why) => json!({ "code": -32020, "message": format!("Header mismatch: {why}") }),
        };
        json!({ "jsonrpc": "2.0", "error": error, "id": id })
    }
}

/// Decode a header value that may use the `=?base64?…?=` form for non-ASCII.
fn decode_name(raw: &str) -> Option<String> {
    use base64::Engine;
    match raw.strip_prefix("=?base64?").and_then(|r| r.strip_suffix("?=")) {
        Some(encoded) => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
            String::from_utf8(bytes).ok()
        }
        None => Some(raw.to_string()),
    }
}

/// Check the MCP request headers against the JSON-RPC body (HTTP transport).
///
/// No `MCP-Protocol-Version` means a legacy client and nothing is required.
/// A modern version requires `Mcp-Method` (and `Mcp-Name` for `tools/call`)
/// to match the body, and the body's `_meta` to name the same version.
/// Present headers are always checked, in any era.
pub fn validate_headers(
    version: Option<&str>,
    method_header: Option<&str>,
    name_header: Option<&str>,
    body: &Value,
) -> Result<(), HeaderError> {
    if let Some(v) = version
        && !supported_versions().contains(&v)
    {
        return Err(HeaderError::UnsupportedVersion(v.to_string()));
    }
    let modern = version == Some(MODERN_VERSION);
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");

    match method_header {
        Some(h) if h != method => {
            return Err(HeaderError::Mismatch(format!("Mcp-Method '{h}' does not match body method '{method}'")));
        }
        None if modern => return Err(HeaderError::Mismatch("missing Mcp-Method header".into())),
        _ => {}
    }

    let params = body.get("params").cloned().unwrap_or(Value::Null);
    let body_name = match method {
        "tools/call" | "prompts/get" => params.get("name").and_then(Value::as_str),
        "resources/read" => params.get("uri").and_then(Value::as_str),
        _ => None,
    };
    if let Some(body_name) = body_name {
        match name_header.map(decode_name) {
            Some(Some(h)) if h != body_name => {
                return Err(HeaderError::Mismatch(format!("Mcp-Name '{h}' does not match body '{body_name}'")));
            }
            Some(None) => return Err(HeaderError::Mismatch("Mcp-Name is not valid base64 text".into())),
            None if modern => return Err(HeaderError::Mismatch("missing Mcp-Name header".into())),
            _ => {}
        }
    }

    if modern && era_of(&params) != Era::Modern {
        return Err(HeaderError::Mismatch(format!("body _meta does not name protocol version {MODERN_VERSION}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_body(name: &str) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": name, "_meta": { META_VERSION: MODERN_VERSION } } })
    }

    #[test]
    fn eras_are_told_apart_by_meta() {
        assert_eq!(era_of(&json!({ "_meta": { META_VERSION: MODERN_VERSION } })), Era::Modern);
        assert_eq!(era_of(&json!({})), Era::Legacy);
        assert_eq!(era_of(&json!({ "_meta": { META_VERSION: "2025-06-18" } })), Era::Legacy);
    }

    #[test]
    fn initialize_echoes_known_versions_only() {
        assert_eq!(initialize(&json!({ "protocolVersion": "2025-06-18" }))["protocolVersion"], "2025-06-18");
        assert_eq!(initialize(&json!({ "protocolVersion": "1999-01-01" }))["protocolVersion"], LEGACY_VERSIONS[0]);
        assert_eq!(initialize(&json!({}))["protocolVersion"], LEGACY_VERSIONS[0]);
    }

    #[test]
    fn modern_headers_must_match_the_body() {
        let body = call_body("list_projects");
        assert!(validate_headers(Some(MODERN_VERSION), Some("tools/call"), Some("list_projects"), &body).is_ok());
        // Non-ASCII names arrive base64-wrapped.
        assert!(
            validate_headers(Some(MODERN_VERSION), Some("tools/call"), Some("=?base64?bGlzdF9wcm9qZWN0cw==?="), &body)
                .is_ok()
        );

        for (method, name) in [
            (None, Some("list_projects")),
            (Some("tools/list"), Some("list_projects")),
            (Some("tools/call"), None),
            (Some("tools/call"), Some("other")),
            (Some("tools/call"), Some("=?base64?%%%?=")),
        ] {
            let err = validate_headers(Some(MODERN_VERSION), method, name, &body).unwrap_err();
            assert!(matches!(err, HeaderError::Mismatch(_)), "{method:?} {name:?}");
            assert_eq!(err.to_json(None)["error"]["code"], -32020);
        }
    }

    #[test]
    fn modern_header_needs_modern_meta_in_the_body() {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
        assert!(validate_headers(Some(MODERN_VERSION), Some("tools/list"), None, &body).is_err());
    }

    #[test]
    fn unsupported_versions_list_what_is_supported() {
        let err = validate_headers(Some("2031-01-01"), None, None, &json!({})).unwrap_err();
        let json = err.to_json(Some(json!(7)));
        assert_eq!(json["error"]["code"], -32022);
        assert_eq!(json["error"]["data"]["supported"][0], MODERN_VERSION);
        assert_eq!(json["id"], 7);
    }

    #[test]
    fn legacy_clients_need_no_headers() {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        assert!(validate_headers(None, None, None, &body).is_ok());
        assert!(validate_headers(Some("2025-06-18"), None, None, &body).is_ok());
        // …but a header that is present must still be honest.
        assert!(validate_headers(Some("2025-06-18"), Some("tools/call"), None, &body).is_err());
    }
}
