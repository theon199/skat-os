# skat-os 🪐
### The Terminal Document Operating System (Doc-OS)

`skat-os` is a lightweight, distraction-free **Document Operating System** that runs directly inside your terminal. It flips traditional computing on its head: instead of running applications inside an OS that manage separate files, **the document is the operating system**.

Documents act as living notes, wiki pages, runbooks, and interactive shells with **inline executable code blocks**.

```
+--------------------------+-------------------------------------------------------+
| 📁 Workspace             | 📄 welcome.md [MODIFIED]                              |
+--------------------------+-------------------------------------------------------+
| ▶ Welcome                | # Welcome to skat-os                                  |
|   architecture.md        |                                                       |
|   runbook.md             | Living Runbooks & Code Execution:                     |
|                          | Place cursor inside and press [Ctrl+R]:               |
|                          |                                                       |
|                          | ```bash                                               |
| --- BACKLINKS ---        | uname -srm                                            |
|   <- architecture.md     | uptime                                                |
|   <- runbook.md          | ```                                                   |
|                          | ```output [exit: 0]                                   |
|                          | Darwin 24.6.0 arm64                                   |
|                          | 16:42  up 10 days, 4 users, load averages: 2.14       |
|                          | ```                                                   |
+--------------------------+-------------------------------------------------------+
| [Ctrl+K] Palette | [Ctrl+R] Run | [Ctrl+B] Sidebar | [F1] Help | [Ctrl+Q] Quit   |
+----------------------------------------------------------------------------------+
```

---

## ⚡ Key Capabilities

- 📝 **Document-as-an-OS**: Modeless, intuitive text editor powered by a high-performance `Ropey` rope buffer.
- 🚀 **Interactive Run-in-Place**: Embed fenced code blocks (`bash`, `sh`, `zsh`, `python`, `node`). Press **`Ctrl+R`** to execute them asynchronously without freezing the UI and inject collapsible output blocks inline.
- 🗂 **Hyperlinked Wiki & Backlinks**: Automatically detects `[[wiki-links]]` between documents and constructs a live bidirectional backlink graph in the sidebar.
- 🔍 **Unified Command Palette (`Ctrl+K`)**: One fuzzy-search modal to jump between documents, perform full-text grep across the workspace, create notes, or execute actions.
- 📂 **Zero Vendor Lock-In**: 100% standard plain Markdown (`.md`) files stored directly in your workspace directory (e.g. `./docs`). Fully compatible with Git, Obsidian, and Vim.
- 🤝 **Real-Time Multiplayer Collaboration**: Pair-write living runbooks over direct TCP connections with zero cloud or 3rd-party dependencies.
- 👥 **Visual Multi-Cursor & Name Badges**: See teammates' cursors live with distinct ANSI colors and name badges (`[Alice]`, `[Bob]`), plus real-time connected peers status indicators.
- ⚡ **Shared Async Co-Execution**: Any collaborator can trigger code execution (`Ctrl+R`); the live command output synchronously updates inline across all connected terminals.
- ✅ **Interactive Synced Task Checklists**: Toggle markdown task items (`- [ ]` / `* [ ]` <-> `- [x]`) with **`Space`**, live synced across all connected collaborators with a status bar completion badge (`✓ Tasks: 3/5 (60%)`).
- 🔗 **Reactive Runbooks & Chained Code Execution**: Name code blocks (`name=get_pod`) and interpolate previous outputs into downstream commands via `{{get_pod.stdout}}` or auto-injected environment variables (`$SKAT_GET_POD_STDOUT`).
- 📜 **Sequential Playbook Modes with Fail-Fast Safeguard**: Run entire runbooks top-to-bottom (**`Ctrl+Shift+R`** / **`F5`**) or up to cursor with automatic sequence halting on non-zero exit codes.
- 📺 **Live Output Streaming & PTY Integration**: Real-time pseudo-terminal (PTY) execution streaming stdout/stderr incrementally into document output blocks with native ANSI colors, bold formatting, and terminal cursor control rendering.
- ⌨️ **Interactive Stdin & Input Prompts**: Send interactive keyboard keystrokes directly to running commands prompting on stdin (`[y/N]`, `read`, passwords).
- ⏹ **Graceful In-Place Cancellation**: Press **`Ctrl+C`** while a command is streaming to immediately terminate the child process and mark the output block as `[interrupted]`.
- 🎛 **Inline Parameterized Form Controls**: Embed reactive form widgets inside prose (`{{param:input:NAME=val}}`, `{{param:select:ENV=prod,stage}}`, `{{param:toggle:DRY_RUN=true}}`). Navigate with **`Tab`** / **`Shift+Tab`**, toggle/cycle with **`Space`** / **`Enter`**, and auto-inject into code blocks via `{{params.NAME}}` and `$SKAT_PARAM_NAME`.
- 📊 **Interactive Structured Data Grids**: Fenced code blocks with `format=table` (or JSON/CSV outputs) render as interactive Ratatui tables with arrow-key row navigation, row inspection modal (**`Enter`**), and downstream cell binding (`{{table.selected.field}}`).
- 📈 **Live Terminal Metric Charts & Sparklines**: Fenced blocks with `format=chart` or `format=sparkline` parse numeric streams in real time and render Ratatui `BarChart` or `Sparkline` canvas widgets.
- 📡 **Multiplayer Parameter Synchronization**: Parameter edits broadcast instantly over TCP (`ParamUpdate`) so teams stay aligned with shared parameters during collaborative runbook execution.
- 🌐 **Remote Infrastructure Mesh & Fleet Execution**: Run code blocks transparently over SSH targeting individual nodes (`target="ssh://user@host:port"`), bastion jump hosts (`proxy="ssh://bastion.corp.com"`), or host groups (`target="@web_servers"`).
- 🗄 **SSH Session Pooling & Persistent Multiplexing**: Reusable authenticated SSH connections via `ssh-agent` (`SSH_AUTH_SOCK`), standard key pairs (`~/.ssh/id_ed25519`, `id_rsa`), and passwords, eliminating per-command handshake overhead.
- 🚀 **Parallel Fleet Fan-Out & Aggregated Output Canvas**: Concurrent cluster execution (`parallel=true`) with interactive Ratatui tabbed sub-views, per-host status badges (`[10.0.0.1: ✓ exit 0]`), and downstream multi-host variable interpolation (`{{web_servers[0].stdout}}`, `{{web_servers.failed}}`).
- 🛡 **Dangerous Command Guardrails & Interactive Approval Gates**: Static heuristic security analyzer detecting destructive operations (`rm -rf /`, `mkfs`, `dd`, SQL destruction, `kubectl delete ns`), interactive Ratatui safety confirmation modals (`confirm="Are you sure?"`), danger banners, required confirmation keywords for critical blast radius, dry-run simulation mode (`dry_run=true` injecting `$SKAT_DRY_RUN=1`), and multiplayer TCP consensus voting (`approvals=2`).
- 🔐 **Dynamic Secret Vault & Zero-Leak Credential Resolvers**: Zero-leak secret interpolation (`{{secret.VAULT_PATH}}`, `{{secrets.KEY}}`), live streaming redaction (`[REDACTED]`), masked form parameters (`{{param:secret:TOKEN}}` -> `••••••••`), and pluggable backend resolvers (.env.vault, CLI agents `op`/`vault`/`aws`/`pass`, system Keychain, and ChaCha20-Poly1305 encrypted frontmatter).
- 📜 **Immutable Execution Audit Trails**: Automatic JSON-Lines journal (`.skat/audit.log`) with SHA-256 cryptographic hash chaining, recording ISO-8601 timestamps, collaborator identity, targets, redacted command, parameters, exit code, duration, and an interactive timeline viewer modal (`Ctrl+K` -> `View Audit Log`).
- 🦀 **Single Static Binary**: Built with Rust, Ratatui, and Crossterm for zero runtime dependencies and blazing sub-millisecond responsiveness.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Description |
| :--- | :--- |
| **`Ctrl+K`** | Open **Unified Command Palette** (switch docs, search text, actions) |
| **`Ctrl+R`** | **Run Code Block** under cursor (interpolates prior outputs & captures output) |
| **`Ctrl+Shift+R`** / **`F5`** | **Run All Blocks** sequentially (Playbook execution with fail-fast safeguard) |
| **`Tab`** / **`Shift+Tab`** | **Navigate Form Parameters** forward / backward across document |
| **`Space`** | **Toggle Task Checkbox** (`- [ ]` <-> `- [x]`) or **Toggle Boolean / Cycle Select** form parameter |
| **`Enter`** | **Edit Input Parameter** / **Cycle Dropdown**, or **Inspect Selected Table Row** in modal |
| **`Up`** / **`Down`** | **Select Table Row** when cursor is inside a structured table output block |
| **`[`** / **`]`** or **`Left`** / **`Right`** | **Switch Host Sub-View Tab** when cursor is inside a multi-host fleet output block |
| **`Ctrl+O`** | **Collapse / Expand** command output block |
| **`Ctrl+C`** | **Interrupt / Cancel** streaming command (`[interrupted]`) without exiting, or quit when idle |
| **`Ctrl+B`** | Toggle **Sidebar** (file tree and reverse backlinks) |
| **`Ctrl+W`** | Switch focus between **Sidebar** and **Editor** |
| **`Ctrl+S`** | **Save** active document to disk |
| **`Ctrl+Z`** | **Undo** last edit |
| **`Ctrl+Y`** | **Redo** last edit |
| **`F1`** | Open interactive **Help & Keybindings Cheatsheet** |
| **`Ctrl+Q`** | **Quit** skat-os safely |

---

## 🚀 Quick Start

### 1. Build and Run
Ensure you have Rust installed (`cargo --version`):

```bash
# Clone the repository
git clone https://github.com/your-username/skat-os.git
cd skat-os

# Run skat-os (resolves: CLI path > $SKAT_WORKSPACE > ./docs > ~/notes)
cargo run --release

# Or launch with a custom directory
cargo run --release -- ~/my-notes
```

On first launch, `skat-os` will automatically initialize starter templates:
- `welcome.md`: Interactive overview and keybindings.
- `daily.md`: Daily worklog, task checklists, and quick system diagnostics.
- `runbook.md`: Living developer playbook with forms, tables, sparklines, and guardrails.
- `fleet.md`: Remote multi-host SSH fleet diagnostics.
- `architecture.md`: Philosophy of the Document Operating System.

### 2. Built-in Multiplayer Collaboration (Phase 2)

Pair-write, review runbooks, and co-execute commands with teammates across terminals with **zero external cloud servers or accounts**:

```bash
# Terminal 1: Host an ephemeral collaboration room
skat-os host --port 2222 --workspace ./docs --name Alice

# Terminal 2: Join the room from anywhere on your local network
skat-os join 127.0.0.1:2222 --name Bob

# Dedicated Headless Server mode (optional)
skat-os host --port 2222 --workspace ./docs --headless
```

- **Visual Multi-Cursor**: Teammates' cursors appear in real-time with vibrant ANSI colors and name tags (e.g. `[Bob]`).
- **Connected Peers Badge**: Shows active collaborators in the status bar and sidebar tree with line/col positions.
- **Shared Code Execution**: When either peer presses `Ctrl+R`, execution output updates synchronously on both screens.
- **Auto-Sync & Resilience**: Full document sync on connect (`DocSync`), non-blocking disconnect handling, and automatic terminal window resize reflow.

### 3. Running Tests
```bash
cargo test
```

---

## 🏗 Architecture & Code Structure

```
skat-os/
├── src/
│   ├── main.rs          # CLI argument parser, terminal raw mode, alternate screen event loop
│   ├── lib.rs           # Core library crate exports
│   ├── ansi.rs          # ANSI escape sequence parser, SGR style mapper & terminal stream processor
│   ├── app.rs           # App state machine, focus manager, event routing & collab sync
│   ├── chart.rs         # Live ASCII & BarChart / Sparkline metric stream parser
│   ├── cli.rs           # CLI subcommands (host, join, local) and argument parsing
│   ├── form.rs          # Inline form parameter parser, badge toggler & cursor navigation
│   ├── table.rs         # Structured data grid parser (JSON / CSV) & cell exporter
│   ├── workspace.rs     # Workspace directory scanner, backlink graph, search
│   ├── runner.rs        # Fenced code block detector, PTY runner, streaming injector, playbook engine
│   ├── mesh.rs          # Remote SSH mesh, connection pooling, bastion tunnels, fleet fan-out
│   ├── palette.rs       # Command palette fuzzy finder & action executor
│   ├── collab/          # Built-in Multiplayer Engine (Phases 2, 4 & 5)
│   │   ├── protocol.rs  # Wire protocol with ParamUpdate, StreamChunk & peer cursors
│   │   ├── server.rs    # Tokio TCP collaboration room server & parameter broadcaster
│   │   ├── client.rs    # Tokio TCP client with background reader/writer channels
│   │   └── mod.rs       # Collaboration module exports
│   ├── editor/
│   │   ├── mod.rs       # Editor file management & dirty tracking
│   │   └── buffer.rs    # Modeless Ropey text buffer, remote edits, cursor motion, undo/redo
│   └── ui/
│       ├── mod.rs       # Dynamic split-pane layout, param prompt & table inspect modal
│       ├── theme.rs     # Terminal color palette, parameter styles, table & chart colors
│       ├── sidebar.rs   # Workspace document tree, connected peers list & backlink viewer
│       ├── editor.rs    # Multi-cursor live renderer, parameter badges, tables, sparklines & fleet tabs
│       ├── palette.rs   # Centered floating command palette modal
│       ├── search.rs    # Full-text workspace search modal
│       ├── status_bar.rs# Status bar with Host/Client badges, Connected Peers, and hints
│       └── help.rs      # Interactive keybinding cheatsheet overlay
└── tests/
    ├── core_tests.rs    # Unit & integration test suite (33 tests: forms, tables, charts, runner, app)
    ├── collab_tests.rs  # Multiplayer integration tests (14 tests: param sync, streaming, cursors)
    └── mesh_tests.rs    # Remote mesh & fleet fan-out tests (10 tests: SSH URI, pooling, fan-out, tabs)
```

---

## 🔮 Roadmap
- [x] **Phase 1: Local Doc-OS Engine**
  - Modeless Ropey text editor
  - Collapsible dual-pane sidebar with backlinks graph
  - Run-in-place fenced code execution with collapsible outputs
  - Unified fuzzy command palette (`Ctrl+K`)
  - Full-text workspace grep
- [x] **Phase 2: Built-in Multiplayer Collaboration**
  - Ephemeral TCP room server (`skat-os host [--port 2222] [--workspace ./docs]`)
  - Instant direct peer join (`skat-os join <host:port> [--name Alice]`)
  - Multi-cursor live rendering with distinctive peer colors & name badges (`[Alice]`)
  - Connected Peers badges in status bar and sidebar
  - Synchronous live co-execution broadcast (`RunBlock` and `OutputUpdate`)
  - Graceful disconnect handling and window resize resilience without desyncing
- [x] **Phase 3: Reactive Runbooks & Chained Code Execution**
  - Interactive Task Checklist toggling via **`Space`** (`- [ ]` / `* [ ]` <-> `- [x]`)
  - Real-time wire protocol synchronization of task completions (`ToggleTask`)
  - Live status bar task completion badge (`✓ Tasks: 3/5 (60%)`)
  - Named code block headers (```` ```bash name=get_pod ````)
  - Reactive variable interpolation (`{{get_pod.stdout}}`, `{{block_0.stdout}}`, `$SKAT_GET_POD_STDOUT`)
  - Trailing newline stripping for seamless string/env injection
  - Sequential Playbook execution modes: "Run All Blocks" (**`Ctrl+Shift+R`** / **`F5`**) & "Run Up To Cursor"
  - Fail-fast sequence safeguard halting upon non-zero exit codes
  - Live step-by-step progress and output broadcasting (`RunBlock` + `OutputUpdate`)
  - Block dependency resolution, topological execution ordering, and circular dependency detection
- [x] **Phase 4: Live Output Streaming & Interactive PTY Support**
  - Real-time incremental stdout/stderr output streaming into document output blocks (`[running]`)
  - Pseudo-Terminal (PTY) integration via `portable-pty`
  - ANSI escape code parser and styled Ratatui span renderer (16/256/RGB colors, bold, dim, underline, italic, reset)
  - Native terminal stream processor with carriage return (`\r`) in-place line overwrites for progress bars
  - Interactive stdin keyboard forwarding for input prompts (`[y/N]`, `read`, password inputs)
  - Graceful command interruption via **`Ctrl+C`** marking output blocks as `[interrupted]` without exiting the app
  - Multiplayer live stream synchronization (`StreamStart`, `StreamChunk`, `StreamEnd`) broadcast over TCP
  - Viewport auto-scrolling to follow live streaming command progress
  - Comprehensive unit and integration test suite covering ANSI parsing, stream chunking, PTY execution, interruption, and multi-peer synchronization
- [x] **Phase 5: Parameterized Runbook Forms & Dynamic Terminal Canvas**
  - Inline Form Parameter Widgets: Text input (`{{param:input:NAME=val}}`), Select dropdown (`{{param:select:ENV=prod,stage}}`), and Boolean toggle (`{{param:toggle:DRY_RUN=true}}`)
  - Interactive Parameter Navigation: **`Tab`** / **`Shift+Tab`** navigation, **`Space`** / **`Enter`** toggle and cycle, and inline edit modal
  - Auto-injected parameter variables (`{{params.NAME}}`) and environment variables (`$SKAT_PARAM_NAME`, `$SKAT_NAME`)
  - Interactive Structured Data Grids: `format=table` fenced blocks (or auto-detected JSON/CSV) rendered as interactive Ratatui tables with row selection and modal inspector (**`Enter`**)
  - Downstream table variable binding: `{{table_name.selected.field}}`, `$SKAT_{TABLE}_SELECTED_{FIELD}`, and `$SKAT_{TABLE}_SELECTED`
  - Dynamic Terminal Canvas: Real-time Ratatui `BarChart` and `Sparkline` widgets rendered directly inside output blocks (`format=chart`, `format=sparkline`)
  - Multiplayer Parameter Synchronization: Wire protocol `ParamUpdate` broadcasts parameter modifications across all connected peers and server state in real time
- [x] **Phase 6: Remote Infrastructure Mesh & Multi-Host Fleet Execution**
  - Targeted Code Block Routing: Direct SSH URIs (`target="ssh://user:pass@host:port"`), Bastion jump hosts (`proxy="ssh://bastion.corp.com"`), parameterized targets (`target="ssh://{{params.USER}}@{{params.HOST}}"`), and host groups (`target="@web_servers"`)
  - Host Group Configuration: YAML frontmatter definitions (`web_servers: [10.0.0.1, 10.0.0.2]`) and workspace `hosts.yaml` resolution
  - SSH Session Pooling & Authentication: Persistent multiplexed sessions via `ssh-agent` (`SSH_AUTH_SOCK`), standard key pairs (`~/.ssh/id_ed25519`, `id_rsa`), and passwords
  - Transparent Bastion Jump Tunneling: UNIX socket pair bridged channel port forwarding (`channel_direct_tcpip`)
  - Parallel Cluster Fan-Out & Aggregated Output Canvas: Concurrent execution (`parallel=true`) with per-host status badges (`[10.0.0.1: ✓ exit 0] [10.0.0.2: ✖ exit 1]`) and interactive horizontal tab navigation (`[` / `]` or `Left` / `Right`)
  - Multi-Host Variable Interpolation & Chaining: Downstream indexing (`{{web_servers[0].stdout}}`), status aggregates (`{{web_servers.failed}}`, `{{web_servers.success}}`, `{{web_servers.count}}`), and auto-injected environment variables (`$SKAT_WEB_SERVERS_0_STDOUT`, `$SKAT_WEB_SERVERS_FAILED`)
  - Remote Interactive PTY & Multiplayer Live Streaming: Live remote ANSI terminal streaming, stdin forwarding, and real-time wire broadcasting (`StreamChunk`) with host metadata badges
  - 76 total tests (19 unit tests, 33 core tests, 14 multiplayer integration tests, 10 remote mesh tests) ensuring zero regressions across all phases
- [x] **Phase 7: Zero-Trust Guardrails, Dynamic Secret Vaults & Immutable Execution Audit Trails**
  - Dangerous Command Guardrails: Heuristic static analyzer flagging destructive patterns (`rm -rf /`, `mkfs`, `dd`, `DROP DATABASE`, `iptables -F`, `kubectl delete ns`)
  - Fenced Block Safety Attributes: `confirm="prompt"`, `danger=true`, `blast_radius="critical"`, and `dry_run=true` injecting `$SKAT_DRY_RUN=1`
  - Interactive Safety Confirmation Dialog: Ratatui modal with command preview, target hosts, danger warning banner, and keyword confirmation for critical operations
  - Multiplayer Consensus Approval: Collaborative TCP workflow (`approvals=2`, `ApprovalRequest`, `ApprovalVote`, `ApprovalStatus`) requiring peer consensus before execution
  - Dynamic Secret Vault: Zero-leak interpolation (`{{secret.PATH}}`, `{{secrets.KEY}}`), runtime live streaming masking (`[REDACTED]`), and masked form parameter widget (`{{param:secret:TOKEN}}` -> `••••••••`)
  - Pluggable Credential Resolvers: `.env.vault` / `.env`, CLI agent passthroughs (1Password `op`, HashiCorp Vault `vault`, AWS Secrets Manager `aws`, pass `pass`), system Keychain (`security`/`secret-tool`), and ChaCha20-Poly1305 encrypted frontmatter
  - Immutable Execution Audit Trail: `.skat/audit.log` journal with SHA-256 hash chaining, tracking execution ID, timestamp, initiator, target nodes, redacted command, parameters, exit code, duration, and input/output integrity hashes
  - Command Palette Audit Viewer: Searchable execution history modal (`Ctrl+K` -> `📜 View Audit Log`) with cryptographic hash integrity verification badge
  - Unsaved Changes Guard & Prompt: Interactive modal (`[S]ave & Proceed` / `[D]iscard` / `[C]ancel`) guarding against accidental data loss on `Ctrl+Q` and document switching
  - 102 total tests across all suites (31 unit tests, 14 multiplayer tests, 37 core tests, 10 remote mesh tests, 10 guard/vault/audit integration tests) with 100% pass rate
