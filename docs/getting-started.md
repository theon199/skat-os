# Getting Started with skat-os

Welcome to **skat-os**, the terminal-based **Document Operating System (Doc-OS)**. In `skat-os`, the traditional model of computing is inverted: instead of applications running inside an operating system to manipulate passive documents, **the document is the operating system**.

Your notes, runbooks, and wiki pages are living interactive workspaces equipped with inline executable code blocks, reactive variable pipelines, parameter forms, data tables, live metric charts, remote fleet orchestration, and real-time multiplayer collaboration.

---

## 1. Installation

`skat-os` is distributed as a single, highly optimized static binary with zero external runtime dependencies.

### Option A: One-Line Installer (Recommended)

Run the automated installer script in your terminal to fetch the latest pre-compiled binary matching your CPU architecture and operating system:

```bash
curl -sSf https://raw.githubusercontent.com/theon199/skat-os/master/install.sh | sh
```

The script automatically detects:
- **Linux**: x86_64 (`x86_64-unknown-linux-gnu`) and ARM64 (`aarch64-unknown-linux-gnu`)
- **macOS**: Apple Silicon (`aarch64-apple-darwin`) and Intel (`x86_64-apple-darwin`)
- Installs the `skat-os` binary to `~/.local/bin` (or `/usr/local/bin` if root).

### Option B: Crates.io via Cargo

If you have the Rust toolchain installed, install directly from [crates.io](https://crates.io/crates/skat-os):

```bash
cargo install skat-os
```

### Option C: Building from Source

To compile the latest release or contribute to development:

```bash
# Clone the repository
git clone https://github.com/theon199/skat-os.git
cd skat-os

# Build release binary
cargo build --release

# The compiled binary will be located at:
./target/release/skat-os --version
```

---

## 2. Workspace Concepts

In `skat-os`, all documents are standard Markdown (`.md`) files stored directly in a local directory called a **Workspace**. There are no proprietary databases, hidden SQLite files, or binary blobs. Every document is 100% compatible with Git, Obsidian, Neovim, and standard UNIX CLI tools.

### Workspace Directory Resolution Hierarchy

When you launch `skat-os`, it determines the active workspace directory using the following precedence:

1. **CLI Path Argument**: `skat-os /path/to/my-notes`
2. **Environment Variable**: `$SKAT_WORKSPACE` (e.g. `export SKAT_WORKSPACE=~/work-notes`)
3. **Local Directory**: `./docs` (if it exists in the current working directory)
4. **Fallback Default**: `~/notes` in your user home directory

```bash
# Launch with default resolution
skat-os

# Launch with an explicit workspace directory
skat-os ~/projects/ops-runbooks
```

### Automatic Starter Scaffolding

If the resolved workspace directory is empty, `skat-os` automatically generates five interactive starter templates to help you get started immediately:

| Document | Description |
| :--- | :--- |
| **`welcome.md`** | Interactive introduction, quick start guide, and keybindings cheat-sheet. |
| **`daily.md`** | Daily operational scratchpad, task checklists, and instant system diagnostics. |
| **`runbook.md`** | Interactive developer deployment runbook showcasing chained execution and parameter forms. |
| **`fleet.md`** | Multi-host remote SSH fleet diagnostics and parallel cluster health checks. |
| **`architecture.md`** | Core tenets and design philosophy of the Terminal Document Operating System. |

---

## 3. User Interface & Layout

The `skat-os` terminal interface is split into two primary panes:

```
+--------------------------+-------------------------------------------------------+
| Workspace                | welcome.md [MODIFIED]                                 |
+--------------------------+-------------------------------------------------------+
| ▶ Welcome                | # Welcome to skat-os                                  |
|   daily.md               |                                                       |
|   runbook.md             | Living Runbooks & Code Execution:                     |
|   fleet.md               | Place cursor inside and press [Ctrl+R]:               |
|   architecture.md        |                                                       |
|                          | ```bash                                               |
| --- BACKLINKS ---        | uname -srm                                            |
|   <- architecture.md     | uptime                                                |
|   <- runbook.md          | ```                                                   |
|                          | ```output [exit: 0]                                   |
| --- CONNECTED PEERS ---  | Darwin 24.6.0 arm64                                   |
|   ● Alice (host)         | 16:42  up 10 days, 4 users, load averages: 2.14       |
|   ● Bob [L14:C5]         | ```                                                   |
+--------------------------+-------------------------------------------------------+
| [Ctrl+K] Palette | [Ctrl+R] Run | [Ctrl+B] Sidebar | [F1] Help | [Ctrl+Q] Quit   |
+----------------------------------------------------------------------------------+
```

### Left Pane: Collapsible Sidebar (`Ctrl+B`)
- **Document Tree**: Lists all `.md` files in the workspace. The active document is marked with `▶`.
- **Backlinks Graph**: Automatically displays all documents that reference the active document via `[[wiki-links]]`.
- **Connected Peers**: When running in multiplayer mode, displays active teammates, their connection status, and live cursor positions.

### Right Pane: Modeless Document Editor
- Powered by a high-performance `Ropey` rope buffer supporting seamless editing of massive files with sub-millisecond responsiveness.
- Modeless editing: No modal switching required—type naturally as you would in VS Code or Nano.
- Syntax styling: Inline markdown formatting for headers, bold/italic, code blocks, blockquotes, and lists.

---

## 4. Basic Navigation & Essential Shortcuts

| Shortcut | Action | Description |
| :--- | :--- | :--- |
| **`Ctrl+K`** | **Command Palette** | Open unified fuzzy modal to jump files, grep workspace, or run actions |
| **`Ctrl+B`** | **Toggle Sidebar** | Show or hide the left workspace sidebar |
| **`Ctrl+W`** / **`Tab`** | **Switch Focus** | Switch active focus between the Sidebar and the Editor |
| **`Ctrl+S`** | **Save Document** | Persist modifications of active document to disk |
| **`Ctrl+Z`** | **Undo** | Revert the last typing or editing action |
| **`Ctrl+Y`** | **Redo** | Reapply the previously undone action |
| **`F1`** or **`?`** | **Help Cheatsheet** | Display interactive modal with all keybindings |
| **`Ctrl+Q`** | **Quit** | Safely exit `skat-os` (prompts if unsaved changes exist) |

> [!TIP]
> Press **`Ctrl+K`** at any time. It is your single cockpit to search files, perform full-text grep across the entire workspace, create notes, inspect audit trails, and run commands.

---

## 5. Running Your First Command

`skat-os` turns code fences into runnable execution targets.

### Step 1: Open `welcome.md` or Create a New Note
Press **`Ctrl+K`**, type `welcome`, and press **`Enter`**.

### Step 2: Write an Executable Fenced Code Block
Add a standard fenced code block to the document:

````markdown
```bash
echo "=== Host Diagnostics ==="
hostname
uname -srm
uptime
```
````

### Step 3: Execute In-Place (`Ctrl+R`)
Position your cursor anywhere inside or on the fence boundary of the code block and press **`Ctrl+R`**.

1. `skat-os` immediately spawns an asynchronous pseudo-terminal (PTY) runner.
2. The output stream renders directly below the code block inside a matching ```` ```output ```` block:

````markdown
```bash
echo "=== Host Diagnostics ==="
hostname
uname -srm
uptime
```
```output [exit: 0]
=== Host Diagnostics ===
workstation-mac.local
Darwin 24.6.0 arm64
10:14  up 3 days, 1 user, load averages: 1.45, 1.62, 1.58
```
````

### Output Management
- **Collapsing / Expanding**: Place cursor on or inside the output block and press **`Ctrl+O`** to collapse the block to a single line badge (` ```output [exit: 0] [collapsed] ````) or expand it.
- **Graceful Cancellation**: If a command is running or hanging, press **`Ctrl+C`** to immediately terminate the process without quitting the editor. The block will be marked as `[interrupted]`.

---

## 6. Wiki-Links & The Knowledge Graph

Link documents together using double bracket notation:

```markdown
Review our production recovery steps in [[runbook]] or check cluster nodes in [[fleet]].
```

- **Instant Jumping**: Place cursor on `[[runbook]]` and open via Command Palette or press `Enter` to jump directly into `docs/runbook.md`.
- **Automatic Bidirectional Backlinks**: `skat-os` continuously analyzes links across all workspace files and dynamically renders incoming backlinks in the sidebar under `--- BACKLINKS ---`.

---

## 7. Next Steps

Explore the full capabilities of `skat-os`:

- **[Interactive Documents Guide](interactive-documents.md)**: Master code chaining, reactive variables (`{{block.stdout}}`), PTY streaming, form parameters, data tables, and live terminal charts.
- **[Multiplayer Collaboration Guide](multiplayer.md)**: Host or join peer-to-peer TCP collaboration rooms with live multi-cursor, co-execution, and consensus voting.
- **[Mesh & Fleet Orchestration Guide](mesh-and-fleet.md)**: Run commands remotely over SSH across bastions and server clusters with parallel fan-out.
- **[Security, Vault & Audit Guide](security-vault-audit.md)**: Command guardrails, ChaCha20-Poly1305 encrypted frontmatter, secret redaction, and SHA-256 audit logs.
- **[Maintainer Releasing Guide](releasing.md)**: Step-by-step procedures for cutting releases and publishing crates.
