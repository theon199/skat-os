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
- 🦀 **Single Static Binary**: Built with Rust, Ratatui, and Crossterm for zero runtime dependencies and blazing sub-millisecond responsiveness.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Description |
| :--- | :--- |
| **`Ctrl+K`** | Open **Unified Command Palette** (switch docs, search text, actions) |
| **`Ctrl+R`** | **Run Code Block** under cursor (captures output into inline block) |
| **`Ctrl+O`** | **Collapse / Expand** command output block |
| **`Ctrl+B`** | Toggle **Sidebar** (file tree and reverse backlinks) |
| **`Ctrl+W`** or **`Tab`** | Switch focus between **Sidebar** and **Editor** |
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

# Run skat-os with default workspace (./docs)
cargo run --release

# Or launch with a custom directory
cargo run --release -- ~/my-notes
```

On first launch, `skat-os` will automatically initialize seed documents:
- `welcome.md`: Interactive overview and keybindings.
- `architecture.md`: Philosophy of the Document Operating System.
- `runbook.md`: Living executable runbook with sample bash commands.

### 2. Running Tests
```bash
cargo test
```

---

## 🏗 Architecture & Code Structure

```
skat-os/
├── src/
│   ├── main.rs          # Terminal raw mode, panic hooks, alternate screen event loop
│   ├── lib.rs           # Core library crate exports
│   ├── app.rs           # App state machine, focus manager, event routing
│   ├── workspace.rs     # Workspace directory scanner, backlink graph, search
│   ├── runner.rs        # Fenced code block detector, async process runner, output injector
│   ├── palette.rs       # Command palette fuzzy finder & action executor
│   ├── editor/
│   │   ├── mod.rs       # Editor file management & dirty tracking
│   │   └── buffer.rs    # Modeless Ropey text buffer, cursor motion, undo/redo
│   └── ui/
│       ├── mod.rs       # Dynamic split-pane layout & modal dispatcher
│       ├── theme.rs     # Terminal color palette & syntax styling
│       ├── sidebar.rs   # Workspace document tree & backlink viewer
│       ├── editor.rs    # Markdown syntax renderer with hardware cursor positioning
│       ├── palette.rs   # Centered floating command palette modal
│       ├── search.rs    # Full-text workspace search modal
│       ├── status_bar.rs# Contextual status bar with mode, Ln/Col, and hints
│       └── help.rs      # Interactive keybinding cheatsheet overlay
└── tests/
    └── core_tests.rs    # Unit & integration test suite (8 tests)
```

---

## 🔮 Roadmap
- [x] **Phase 1: Local Doc-OS Engine**
  - Modeless Ropey text editor
  - Collapsible dual-pane sidebar with backlinks graph
  - Run-in-place fenced code execution with collapsible outputs
  - Unified fuzzy command palette (`Ctrl+K`)
  - Full-text workspace grep
- [ ] **Phase 2: Built-in Multiplayer Collaboration**
  - Ephemeral TCP/SSH room server (`skat host --port 2222`)
  - Instant join (`skat join <addr>`) with multi-cursor live rendering over terminal sockets
