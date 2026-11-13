# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.1.0] - 2026-11-13

### Added
- **Document Operating System (Doc-OS) Core**:
  - Modeless text editor powered by a high-performance `Ropey` rope buffer supporting seamless editing of large documents.
  - Modeless navigation and text input without modal command switching.
  - Automatic workspace scaffolding with starter templates (`welcome.md`, `daily.md`, `runbook.md`, `fleet.md`, `architecture.md`).
  - Zero vendor lock-in with 100% standard plain Markdown (`.md`) files stored in the local workspace.
  - Workspace path resolution with precedence: CLI argument > `$SKAT_WORKSPACE` > `./docs` > `~/notes`.
  - Unsaved changes guard modal on `Ctrl+Q` and document switching with `[S]ave & Proceed`, `[D]iscard`, and `[C]ancel`.
- **Knowledge Graph & Navigation**:
  - Automatic `[[wiki-link]]` syntax parsing and document jumping.
  - Dynamic reverse backlinks graph generated in real time in the sidebar.
  - Collapsible dual-pane sidebar (`Ctrl+B`) displaying workspace file tree, active backlinks, and connected peers.
  - Focus switching between sidebar and editor with `Ctrl+W` or `Tab`.
  - Unified fuzzy Command Palette (`Ctrl+K`) for switching documents, running actions, and full-text workspace grep.
  - Interactive keybindings cheatsheet modal (`F1` or `?`).
- **Interactive Code Execution & PTY Streaming**:
  - In-place fenced code block execution with `Ctrl+R` across `bash`, `sh`, `zsh`, `python`, and `node`.
  - Collapsible output blocks (```` ```output [exit: 0] ````) with `Ctrl+O`.
  - Real-time pseudo-terminal (PTY) output streaming via `portable-pty`.
  - Native ANSI terminal escape parser supporting 16-color, 256-color, and 24-bit TrueColor rendering, bold, italic, and underline styling.
  - Terminal stream carriage return (`\r`) in-place line overwriting for dynamic progress bars and spinners.
  - Interactive keyboard forwarding (stdin) for input prompts (`[y/N]`, `read`, password entries).
  - Graceful command interruption via `Ctrl+C` marking blocks as `[interrupted]` without quitting the application.
- **Synced Task Checklists**:
  - Interactive task checklist toggling with `Space` (`- [ ]` / `* [ ]` <-> `- [x]`).
  - Real-time wire protocol synchronization (`ToggleTask`) across multiplayer collaborators.
  - Dynamic status bar completion badge (`Tasks: X/Y (Z%)`).
- **Reactive Chaining & Playbooks**:
  - Named code blocks (`name="block_id"`).
  - Reactive variable interpolation for downstream blocks (`{{name.stdout}}`, `{{name.stderr}}`, `{{name.exit_code}}`).
  - Trailing newline stripping for clean variable composition.
  - Automatic environment variable injection (`$SKAT_<NAME>_STDOUT`, `$SKAT_<NAME>_STDERR`, `$SKAT_<NAME>_EXIT_CODE`).
  - Sequential Playbook execution modes: "Run All Blocks" (`Ctrl+Shift+R` / `F5`) and "Run Up To Cursor".
  - Fail-fast sequence safeguard halting execution upon non-zero exit codes.
  - Dependency graph resolution with cycle detection.
- **Inline Form Parameters**:
  - Inline form parameter widgets embedded in prose:
    - Text inputs: `{{param:input:NAME=val}}`
    - Select dropdowns: `{{param:select:NAME=opt1,opt2}}`
    - Boolean toggles: `{{param:toggle:NAME=true}}`
    - Masked secrets: `{{param:secret:NAME=val}}` (rendered as `••••••••`)
  - Form navigation with `Tab` / `Shift+Tab`.
  - Toggle / cycle with `Space` or `Enter`, and centered text prompt modal for input fields.
  - Parameter variable interpolation in code blocks (`{{params.NAME}}`, `$SKAT_PARAM_NAME`, `$SKAT_NAME`).
  - Multiplayer parameter synchronization via `ParamUpdate` TCP wire events.
- **Structured Data Grids & Terminal Canvas**:
  - Structured data tables (`format=table`) with auto-detected JSON array and CSV parsing.
  - Interactive Ratatui table grid with arrow-key row navigation (`Up` / `Down`).
  - Table row inspector modal (`Enter`) for detailed column/key inspection.
  - Downstream cell variable binding (`{{table.selected.field}}`, `$SKAT_{TABLE}_SELECTED_{FIELD}`, `$SKAT_{TABLE}_SELECTED`).
  - Terminal metric bar charts (`format=chart` / `format=bar`) with numeric and key-value stream parsing.
  - Real-time terminal sparklines (`format=sparkline` / `format=spark`) rendering continuous numerical streams.
- **Built-in Multiplayer Collaboration**:
  - Peer-to-peer TCP collaboration room engine with zero external cloud or SaaS dependencies.
  - Room hosting (`skat-os host [--port 2222] [--workspace ./docs] [--name Name]`).
  - Direct peer join (`skat-os join <host:port> [--name Name]`).
  - Dedicated headless server mode (`skat-os host --headless`).
  - Real-time multi-cursor rendering with ANSI palette colors and floating name badges (`[Alice]`, `[Bob]`).
  - Connected peers status list in sidebar and status bar.
  - Synchronous shared code execution broadcasting (`RunBlock`, `StreamStart`, `StreamChunk`, `StreamEnd`, `OutputUpdate`).
  - Window resize resilience with character-coordinate reflow.
- **Remote Infrastructure Mesh & Fleet Fan-Out**:
  - Remote code block execution over SSH via `target` URIs (`ssh://user@host:port`, `user@host`, `host:port`).
  - Parameterized targets (`target="ssh://{{params.USER}}@{{params.HOST}}"`).
  - Host group routing (`target="@group_name"`) defined in document YAML frontmatter or workspace `hosts.yaml`.
  - Persistent SSH connection pooling (`SshSessionPool`) with `ssh-agent` (`$SSH_AUTH_SOCK`), private keys (`~/.ssh/id_ed25519`, `id_rsa`), and password authentication.
  - Transparent bastion jump host tunneling (`proxy="ssh://bastion.corp.com"`) using UNIX socket pair channel bridging.
  - Parallel fleet fan-out (`parallel=true`) executing across cluster nodes concurrently.
  - Aggregated multi-host output canvas with per-host status badges (`[host: ✓ exit 0]`) and horizontal tab navigation (`[` / `]` or `Left` / `Right`).
  - Multi-host variable interpolation (`{{group[0].stdout}}`, `{{group.failed}}`, `{{group.success}}`, `{{group.count}}`, `$SKAT_GROUP_0_STDOUT`, `$SKAT_GROUP_FAILED`).
- **Zero-Trust Guardrails & Security**:
  - Static heuristic security analyzer detecting destructive operations (`rm -rf /`, `mkfs`, `dd`, `DROP DATABASE`, `DROP TABLE`, `TRUNCATE`, `iptables -F`, `kubectl delete ns/nodes/all`, fork bombs, root `chmod 777`).
  - Fenced block safety attributes (`confirm="prompt"`, `danger=true`, `blast_radius="critical"`, `dry_run=true`).
  - Interactive safety confirmation modal with command preview, target endpoints, and confirmation keyword gating.
  - Multiplayer consensus approval workflow (`approvals=N`, `ApprovalRequest`, `ApprovalVote`, `ApprovalStatus`).
- **Dynamic Secret Vault**:
  - Zero-leak secret interpolation (`{{secret.PATH}}`, `{{secrets.KEY}}`).
  - In-memory credential injection preventing plaintext persistence to disk.
  - Pluggable resolvers: environment variables (`env:`), workspace `.env.vault` / `.env`, 1Password CLI (`op://`), HashiCorp Vault CLI (`vault:`), AWS Secrets Manager CLI (`aws:`), pass (`pass:`), and system Keyrings (`keyring:`).
  - ChaCha20-Poly1305 authenticated symmetric encryption for secrets stored directly in document frontmatter.
  - Runtime live streaming redaction (`[REDACTED]`) with longest-string-first replacement algorithm.
  - Masked form parameters (`{{param:secret:TOKEN}}` displaying `••••••••`).
- **Immutable Execution Audit Trail**:
  - Automatic execution journaling in `.skat/audit.log` (JSON-Lines format).
  - Cryptographic SHA-256 hash chaining linking each entry to previous entry hash from genesis.
  - Records execution ID, timestamp, collaborator, target hosts, redacted command, form parameters, exit code, duration, output preview, and hashes.
  - Command Palette audit viewer (`Ctrl+K` -> `View Audit Log`) with cryptographic hash chain integrity validation badge.
- **Distribution & Quality**:
  - Single-line automated cross-platform curl installer script (`install.sh`).
  - GitHub Actions CI/CD workflows for matrix testing and multi-architecture binary release generation.
  - GitHub community issue and PR templates.
  - 136 comprehensive unit, integration, multiplayer, mesh, and security tests with 100% pass rate.

[Unreleased]: https://github.com/theon199/skat-os/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/theon199/skat-os/releases/tag/v0.1.0
