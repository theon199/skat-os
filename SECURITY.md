# Security Policy

The `skat-os` team takes the security of our Terminal Document Operating System, code execution engines, multiplayer protocols, and credential vaults very seriously. We appreciate your efforts to responsibly disclose vulnerabilities to us.

---

## Supported Versions

We provide security updates for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

We strongly recommend always running the latest released version of `skat-os`.

---

## Reporting a Vulnerability

**Please do not report security vulnerabilities via public GitHub issues, discussions, or pull requests.**

Instead, please report vulnerabilities using one of the following methods:

1. **GitHub Private Vulnerability Advisory (Preferred)**:
   - Go to [https://github.com/theon199/skat-os/security/advisories/new](https://github.com/theon199/skat-os/security/advisories/new).
   - Fill out the details describing the vulnerability, affected components, reproduction steps, and potential impact.

2. **Security Email**:
   - Send an email to [charan@users.noreply.github.com](mailto:charan@users.noreply.github.com) with the subject `[SECURITY] skat-os vulnerability report`.
   - Please include:
     - Component affected (e.g., Command Guardrails, ChaCha20-Poly1305 Vault, TCP Collaboration Protocol, SSH Session Pool, Audit Log).
     - Detailed description of the vulnerability.
     - Minimal, reproducible proof-of-concept (PoC) markdown document or code snippet.
     - Potential impact (e.g., privilege escalation, secret leak, remote code execution bypass).
     - Any suggested mitigations or fixes.

---

## Response Process & SLA

When you submit a vulnerability report:
1. **Acknowledgement**: You will receive an initial response acknowledging receipt within **48 hours**.
2. **Triage & Assessment**: We will assess the severity and impact, verifying the report in an isolated environment within **5 business days**.
3. **Fix Development**: We will develop and test a patch in a private repository. If needed, we will invite you to collaborate on the advisory.
4. **Coordinated Disclosure**: We adhere to coordinated vulnerability disclosure. Once a fix is verified, we will release a patch release and publish a GitHub Security Advisory detailing the issue and crediting the reporter (unless you prefer anonymity).

---

## Security Architecture & Scope

`skat-os` features several security subsystems designed for operational safety:

- **Command Guardrails Heuristic Analyzer**: Scans fenced code blocks for destructive commands (`rm -rf /`, `mkfs`, destructive SQL, namespace deletion).
- **Dynamic Secret Vault & Streaming Masking**: Redacts sensitive values (`{{secret.*}}`) in terminal output and logs with zero-leak guarantees.
- **ChaCha20-Poly1305 Encrypted Frontmatter**: Authenticated symmetric encryption for local document secrets.
- **Cryptographic Audit Log**: Append-only execution journal (`.skat/audit.log`) protected with SHA-256 hash chaining.
- **Multiplayer Consensus Gate**: Requires peer quorum approval for sensitive actions over TCP.

Vulnerabilities of particular interest include:
- Bypasses of command guardrail heuristics leading to unexpected destructive execution.
- Secret exposure or unmasked leakages in streaming PTY output blocks or audit logs.
- Flaws in ChaCha20-Poly1305 key derivation, nonce reuse, or ciphertext authentication.
- Unauthenticated command injection or privilege escalation in remote SSH mesh or TCP multiplayer protocols.
- Cryptographic tampering or collision attacks against SHA-256 audit hash chains.

Thank you for helping keep `skat-os` and its community safe!
