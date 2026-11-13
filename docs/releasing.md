# Release Engineering & Maintainer Guide

This guide outlines the standard operating procedure for maintainers cutting a new release of **skat-os**. It covers versioning standards, pre-flight verification, git tagging, automated cross-platform CI/CD packaging, crates.io publishing, and post-release validation.

---

## 1. Versioning Standards & Conventions

`skat-os` adheres strictly to:
- **[Semantic Versioning 2.0.0](https://semver.org/)** (`MAJOR.MINOR.PATCH`):
  - **`MAJOR`**: Incompatible breaking changes to CLI arguments, document syntax, or wire protocols.
  - **`MINOR`**: Backward-compatible new features (e.g. new form parameter types, chart formats, SSH capabilities).
  - **`PATCH`**: Backward-compatible bug fixes, performance optimizations, and security patches.
- **[Keep a Changelog 1.1.0](https://keepachangelog.com/)**:
  - Every user-facing change must be documented in `CHANGELOG.md` under `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`.
- **Git Tags**:
  - All releases are tagged using standard lowercase `v` prefix: `v0.1.0`, `v0.2.0`, `v1.0.0`.

---

## 2. Pre-Release Quality Gates Checklist

Before cutting a release tag, ensure the codebase satisfies all quality gates locally:

```bash
# 1. Verify code formatting conforms to rustfmt
cargo fmt --check

# 2. Run clippy linter with warnings treated as errors
cargo clippy --all-targets -- -D warnings

# 3. Execute the full unit, integration, multiplayer, and mesh test suite
cargo test --all-targets

# 4. Verify release compilation succeeds
cargo build --release

# 5. Test installer script syntax (if modified)
sh -n install.sh
bash -n install.sh

# 6. Verify documentation internal links
# Ensure all cross-references between README.md, docs/*.md, and CONTRIBUTING.md resolve cleanly
```

---

## 3. Step-by-Step Release Procedure

### Step 1: Update Version in `Cargo.toml`

Edit `Cargo.toml` and bump the version string:

```toml
[package]
name = "skat-os"
version = "0.1.0" # Change to target release version
edition = "2024"
```

Verify `Cargo.lock` updates automatically:

```bash
cargo check
```

### Step 2: Finalize `CHANGELOG.md`

1. Open `CHANGELOG.md` in the repository root.
2. Move all completed entries from `## [Unreleased]` into a new version section:
   ```markdown
   ## [0.1.0] - 2026-11-13
   ```
3. Update the compare links at the bottom of `CHANGELOG.md`:
   ```markdown
   [Unreleased]: https://github.com/theon199/skat-os/compare/v0.1.0...HEAD
   [0.1.0]: https://github.com/theon199/skat-os/releases/tag/v0.1.0
   ```

### Step 3: Commit Release Metadata

Commit the version bump and changelog update to `master`:

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: prepare release v0.1.0"
git push origin master
```

### Step 4: Create and Push Annotated Git Tag

Create an annotated git tag signed with your GPG or SSH key (recommended):

```bash
# Create annotated tag
git tag -a v0.1.0 -m "Release v0.1.0: Initial public release of skat-os"

# Push the tag to GitHub
git push origin v0.1.0
```

---

## 4. Automated GitHub Actions Release Workflow

Pushing a `v*` tag triggers the automated release pipeline defined in `.github/workflows/release.yml`.

### Matrix Build Targets
The release workflow compiles optimized release binaries across five targets:

| Platform | Rust Target | Binary Package Name |
| :--- | :--- | :--- |
| **Linux x86_64** | `x86_64-unknown-linux-gnu` | `skat-os-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` |
| **Linux ARM64** | `aarch64-unknown-linux-gnu` | `skat-os-v0.1.0-aarch64-unknown-linux-gnu.tar.gz` |
| **macOS Intel** | `x86_64-apple-darwin` | `skat-os-v0.1.0-x86_64-apple-darwin.tar.gz` |
| **macOS Apple Silicon** | `aarch64-apple-darwin` | `skat-os-v0.1.0-aarch64-apple-darwin.tar.gz` |
| **Windows x86_64** | `x86_64-pc-windows-msvc` | `skat-os-v0.1.0-x86_64-pc-windows-msvc.zip` |

### Checksum Generation
For each release asset, the workflow calculates SHA-256 checksums and publishes companion `.sha256` files (e.g. `skat-os-v0.1.0-x86_64-apple-darwin.tar.gz.sha256`).

### GitHub Release Creation
Using `softprops/action-gh-release`, the workflow:
- Extracts the matching release notes from `CHANGELOG.md`.
- Attaches the compressed binary archives and `.sha256` checksums.
- Publishes the GitHub release.

---

## 5. Publishing to Crates.io

Once the GitHub release succeeds, publish the crate to [crates.io](https://crates.io):

```bash
# 1. Perform dry-run package verification
cargo publish --dry-run

# 2. Publish to crates.io
cargo publish
```

> [!NOTE]
> Ensure you are authenticated with `cargo login <api-token>` with publish rights for the `skat-os` crate.

---

## 6. Post-Release Verification

After release publication, verify user installation channels:

### 1. Test Automated Curl Installer
```bash
# In a clean temporary directory or container:
curl -sSf https://raw.githubusercontent.com/theon199/skat-os/master/install.sh | sh

# Verify binary runs and reports correct version
~/.local/bin/skat-os --version
```

### 2. Test Cargo Installation
```bash
cargo install skat-os --force
skat-os --version
```

### 3. Check Docs.rs Build
Visit `https://docs.rs/skat-os/0.1.0` to verify documentation compiles cleanly without broken intra-doc links.

---

## 7. Emergency Hotfix Procedure

If a critical bug is discovered in a production release:

1. Create a hotfix branch from the release tag:
   ```bash
   git checkout -b hotfix/v0.1.1 v0.1.0
   ```
2. Apply the minimal fix and add regression tests.
3. Verify test suite passes (`cargo test --all-targets`).
4. Bump patch version in `Cargo.toml` (`0.1.1`) and add changelog entry under `## [0.1.1]`.
5. Merge hotfix branch into `master` and tag:
   ```bash
   git checkout master
   git merge hotfix/v0.1.1
   git tag -a v0.1.1 -m "Hotfix release v0.1.1"
   git push origin master --tags
   ```
6. The automated workflow will package and release `v0.1.1`.
7. Run `cargo publish` to push the update to crates.io.

---

## 8. Related Documentation

- **[Getting Started](getting-started.md)**: Workspace installation and setup.
- **[Interactive Documents](interactive-documents.md)**: Playbook creation and execution.
- **[Security, Vault & Audit](security-vault-audit.md)**: Security guardrails and audit logs.
