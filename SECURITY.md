# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Please report privately to the maintainer via GitHub Security Advisories:

1. Go to https://github.com/syrilj/Strata/security/advisories/new
2. Describe the vulnerability, impact, and reproduction steps
3. Allow up to 7 days for initial triage

We will:

- Acknowledge receipt within 7 days
- Provide a fix timeline depending on severity
- Credit reporters (unless anonymity is requested)

## Security Scope

In scope:

- Coordinator gRPC / HTTP API (`crates/coordinator`)
- Checkpoint storage backends (`crates/storage`, `crates/checkpoint`)
- Python bindings (`crates/python-bindings`, `python/`)
- Dashboard (`dashboard/`) — XSS, API exposure
- Docker / compose configs

Out of scope (current limitations, see README):

- Single coordinator is not HA — DoS resilience is best-effort
- No built-in auth/authz — deploy behind mTLS / VPN / API gateway in production
- Local backend has no encryption at rest — use S3 SSE-KMS for sensitive checkpoints

## Production Hardening Checklist

- [ ] Run coordinator behind TLS-terminating reverse proxy
- [ ] Set `RUST_LOG=warn` or `info` (never `trace` with sensitive data)
- [ ] Use S3 with SSE-S3 or SSE-KMS (`CHECKPOINT_BUCKET` with bucket policy)
- [ ] Restrict ports 50051/51051 to VPC / private network
- [ ] Run Docker image as non-root (default `dtruntime` user — do not override)
- [ ] Rotate `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` regularly
- [ ] Enable Docker healthchecks and restart policies (see `docker-compose.prod.yml`)
