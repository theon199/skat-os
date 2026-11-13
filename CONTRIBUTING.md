# Contributing to skat-os

First off, thank you for considering contributing to `skat-os`! The Terminal Document Operating System is built for developers and operators who value distraction-free productivity, living runbooks, and real-time multiplayer collaboration directly in their terminal.

Whether you are fixing bugs, proposing new capabilities, improving documentation, or adding tests, your contributions are warmly welcomed.

---

## Code of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md). Please report unacceptable behavior to [charan@users.noreply.github.com](mailto:charan@users.noreply.github.com).

---

## Development Setup

### Prerequisites

1. **Rust Toolchain**: `skat-os` targets Rust Edition 2024. Install the latest stable Rust toolchain via [rustup](https://rustup.rs/):
   ```bash
   rustup update stable
   rustup default stable
   ```
2. **Build Essentials**:
   - macOS: Command Line Tools (`xcode-select --install`)
   - Linux: `build-essential`, `pkg-config`, `libssl-dev` (or OpenSSL headers)
   - Windows: Visual Studio Build Tools with C++ toolchain

### Cloning the Repository

```bash
git clone https://github.com/theon199/skat-os.git
cd skat-os
```

### Building the Project

```bash
# Debug build
cargo build

# Release build (optimized binary)
cargo build --release
```

### Running Locally

```bash
# Run with default workspace resolution
cargo run

# Run pointing to a specific workspace directory
cargo run -- ./docs

# Run multiplayer host
cargo run -- host --port 2222 --workspace ./docs --name Alice

# Run multiplayer client
cargo run -- join 127.0.0.1:2222 --name Bob
```

---

## Running Tests & Quality Verification

Before submitting code, ensure that all tests pass and code quality checks are clean:

```bash
# Run all unit and integration test suites
cargo test

# Run tests with output printed
cargo test -- --nocapture

# Check compiler warnings
cargo check --all-targets

# Run Clippy lints (must pass with zero warnings)
cargo clippy --all-targets -- -D warnings

# Verify code formatting
cargo fmt --all -- --check
```

To automatically format your code:
```bash
cargo fmt --all
```

---

## Repository Architecture

- `src/`
  - `main.rs`: CLI argument parsing, terminal raw mode lifecycle, and alternate screen event loop.
  - `lib.rs`: Library crate root and public module exports.
  - `app.rs`: Core application state machine, focus manager, event dispatch, and collab sync.
  - `cli.rs`: Command-line options (`host`, `join`, local workspace flags).
  - `editor/`: Modeless text editor powered by a `Ropey` rope buffer, dirty tracking, undo/redo, task parsing.
  - `runner.rs`: Fenced code block detector, PTY process runner (`portable-pty`), live streaming injector, sequential playbook engine.
  - `ansi.rs`: ANSI escape code parser and SGR style mapper to Ratatui spans with `\r` line overwrites.
  - `form.rs`: Reactive form parameter parser (`input`, `select`, `toggle`, `secret`) and widget state.
  - `table.rs`: Structured data grid parser (JSON / CSV), arrow-key row navigation, and variable export.
  - `chart.rs`: Live ASCII metric streams and Ratatui `BarChart` / `Sparkline` widgets.
  - `collab/`: Built-in TCP multiplayer room server, client, and wire protocol (`protocol.rs`).
  - `mesh.rs`: Remote SSH mesh, connection pooling, bastion jump tunnels, and parallel fleet fan-out.
  - `guard.rs`: Heuristic destructive command analyzer, safety modals, and multiplayer approval consensus.
  - `vault.rs`: ChaCha20-Poly1305 encrypted frontmatter, pluggable credential resolvers, zero-leak secret masking.
  - `audit.rs`: Append-only `.skat/audit.log` journal with cryptographic SHA-256 hash chaining.
  - `workspace.rs`: Workspace directory scanner, backlink graph builder, and full-text search.
  - `palette.rs`: Unified fuzzy command palette and action executor.
  - `ui/`: Terminal UI rendering with Ratatui split-pane layout, themes, sidebar, modals, and status bar.
- `tests/`
  - `core_tests.rs`: Core editor, forms, tables, runner, and PTY test suite.
  - `collab_tests.rs`: Multiplayer TCP room, cursor broadcast, streaming, and parameter sync integration tests.
  - `mesh_tests.rs`: Remote SSH mesh, connection pooling, and fleet fan-out tests.
  - `guard_vault_audit_tests.rs`: Guardrails, encrypted secrets vault, and audit hash chain integrity tests.
- `docs/`: User guides, technical architecture references, and release manuals.

---

## Coding Guidelines

1. **Idiomatic Rust**:
   - Write clear, idiomatic Rust. Prefer iterator pipelines and pattern matching where readable.
   - Run `cargo fmt` and `cargo clippy -- -D warnings` on all changes.
2. **Terminal Safety**:
   - Always guarantee that terminal raw mode and the alternate screen buffer are restored safely upon termination or panics. Avoid raw panics during interactive runtime.
3. **Error Handling**:
   - Use `anyhow::Result` for application-level operations and provide clear context with `.context(...)`.
   - Never use `.unwrap()` or `.expect()` unless an invariant is strictly proven and documented.
4. **Security & Redaction**:
   - Never print raw secret values to stdout, stderr, or log streams.
   - Ensure any new secret interpolation goes through the masking engine in `src/vault.rs`.
5. **Testing**:
   - Add unit tests for new logic in the corresponding module under `#[cfg(test)] mod tests`.
   - Add integration tests under `tests/` for multi-component workflows, TCP wire protocol changes, or PTY interactions.

---

## Pull Request Process

1. **Fork & Branch**:
   - Fork the repository and create a descriptive feature branch from `master` (e.g. `feature/vim-motions` or `fix/pty-resize`).
2. **Commit Conventions**:
   - Write clear, concise commit messages following standard conventions:
     - `feat: add collapsible table rows in data grids`
     - `fix: resolve PTY stream truncation on Windows`
     - `docs: update multiplayer connection troubleshooting`
     - `test: add edge cases for ChaCha20-Poly1305 key rotation`
3. **Verify Locally**:
   - Run `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt --check`.
4. **Submit PR**:
   - Open a pull request against `master`.
   - Fill out the PR template with a summary of changes, motivation, test verification, and any relevant issue references.
   - Ensure all CI workflow checks pass.
5. **Code Review**:
   - Address feedback constructively. Once approved, your PR will be merged!

---

## Reporting Issues & Feature Requests

- **Bug Reports**: Please open an issue using the [Bug Report Template](https://github.com/theon199/skat-os/issues/new?template=bug_report.yml) detailing your OS, terminal emulator, steps to reproduce, and expected behavior.
- **Feature Requests**: Submit ideas via the [Feature Request Template](https://github.com/theon199/skat-os/issues/new?template=feature_request.yml) explaining the operational workflow or problem it solves.
- **Security Vulnerabilities**: Follow [SECURITY.md](SECURITY.md) to report security issues privately.
