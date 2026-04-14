# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **DO NOT** open a public GitHub issue
2. Email: [security contact or use GitHub private vulnerability reporting]
3. Include: description, steps to reproduce, potential impact
4. You will receive a response within 48 hours

## Security Architecture

NeuroVault is designed with security-first principles:

- **Local-only**: All data stays on your machine. No cloud, no telemetry, no accounts.
- **Localhost binding**: HTTP API binds to 127.0.0.1 only -- not accessible from the network.
- **CORS restricted**: Only localhost and tauri:// origins accepted.
- **Rate limiting**: 300 requests/minute on write endpoints.
- **SQL injection prevention**: All table names validated against allowlist, all parameters use prepared statements.
- **Path traversal protection**: File paths validated before access.
- **Input validation**: Length limits on all API inputs.
- **No hardcoded secrets**: All credentials come from user configuration.
- **WAL mode**: SQLite WAL mode prevents data corruption from crashes.

## Dependencies

We use `cargo audit` to check for known vulnerabilities in dependencies. Run:

```bash
cargo install cargo-audit
cargo audit
```
