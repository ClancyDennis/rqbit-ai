# Security Policy

## Supported versions

Security fixes are generally made on the default branch and included in a subsequent release. Users should run the latest available release. Older releases and development snapshots may not receive security fixes.

This policy does not imply that any version is free of vulnerabilities.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through this repository's **Security** tab by selecting **Report a vulnerability**, when private vulnerability reporting is available. Do not include exploit details, credentials, tracker passkeys, private torrent metadata, or other sensitive information in a public issue.

If private reporting is unavailable, open a minimal public issue asking the maintainers to arrange a private reporting channel. Do not describe the vulnerability in that issue.

Include, where possible:

- the affected rqbit version or commit;
- the affected component and configuration;
- reproduction steps or a minimal proof of concept;
- the security impact and required attacker access;
- relevant logs with secrets and private data removed; and
- any suggested mitigation or fix.

Please allow maintainers an opportunity to investigate and coordinate a fix before public disclosure. Maintainers may ask for additional information, coordinate release timing, or determine that a report is not security-sensitive. No response or remediation timeline is guaranteed.

## Security-sensitive deployment

The HTTP API and Web UI are distinct from the BitTorrent peer listener. The server HTTP API defaults to `127.0.0.1:3030`; keep it bound to localhost unless remote access is explicitly required. Do not expose the HTTP API directly to the public internet. For LAN or remote access, use authentication and a trusted network, VPN, or properly configured TLS reverse proxy.

The BitTorrent listener accepts untrusted peer traffic and normally uses port `4240` in server and desktop modes. UPnP may expose that peer port through a home router. Opening the peer port does not require opening the HTTP API port.

Run rqbit with only the filesystem permissions it needs, use a dedicated download directory where practical, and keep rqbit and its dependencies up to date.
