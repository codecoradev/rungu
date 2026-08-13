# Security Policy

## Supported Versions

Rungu is pre-1.0 software. Security fixes are applied to the latest release only.

| Version | Supported |
|---------|-----------|
| 0.2.x   | ✅        |
| < 0.2   | ❌        |

## Reporting a Vulnerability

If you discover a security vulnerability in Rungu, please report it responsibly.

**Do NOT open a public GitHub issue.**

Instead, choose one of these methods:

### Option 1: GitHub Private Vulnerability Reporting

Go to [github.com/codecoradev/rungu/security/advisories/new](https://github.com/codecoradev/rungu/security/advisories/new) and create a private security advisory. This is the preferred method — it allows us to collaborate privately and publish a coordinated advisory when the fix is released.

### Option 2: Email

Send details to **security@codecoradev.com** with:

- Description of the vulnerability
- Steps to reproduce (proof of concept)
- Affected versions
- Suggested fix (if any)

## Response Timeline

| Step | Target |
|------|--------|
| Acknowledge receipt | Within 48 hours |
| Initial assessment | Within 5 business days |
| Fix or mitigation | Within 30 days (severity-dependent) |
| Public disclosure | After fix is released, coordinated with reporter |

## Scope

**In scope:**
- Rungu server (Rust API + MCP server)
- Rungu web frontend (SvelteKit SPA)
- Authentication bypass, privilege escalation
- SQL injection, XSS, CSRF
- Path traversal, SSRF
- Sensitive data exposure

**Out of scope:**
- Self-hosted infrastructure misconfiguration (your server, your responsibility)
- Third-party dependencies (report upstream)
- Social engineering attacks

## Self-Hosted Error Reporting

For non-security bugs and errors in self-hosted instances, consider integrating [TrapFall](https://github.com/codecoradev/trapfall) — CodeCora's open-source error capture engine. It works with Sentry SDK-compatible clients and helps you collect diagnostics without exposing sensitive data.

## Security Best Practices for Self-Hosted Deployments

- Run Rungu behind a reverse proxy (Caddy, Nginx, Traefik) with TLS
- Set `RUNGU_ADMIN_PASSWORD` to a strong, unique value
- Use a dedicated database user with minimal privileges
- Keep your system and dependencies updated
- Enable rate limiting at the reverse proxy level
- Restrict network access to the database port
