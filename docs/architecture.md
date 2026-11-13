# skat-os Architecture

Traditional operating systems treat documents as passive text files opened by applications.
`skat-os` inverts this model: **The Document is the System**.

### Core Tenets
1. **Plain Markdown**: Zero vendor lock-in. 100% Git, Obsidian, and Vim friendly.
2. **Interactive Run-in-Place**: Documents are living playbooks. Code fences are executable targets.
3. **Hyperlinked Knowledge**: Wiki-links like `[[welcome]]` and `[[runbook]]` automatically build a bidirectional backlink graph in the sidebar.
4. **Single Static Binary**: Powered by Rust, Ratatui, and Crossterm for zero-dependency portability.

See [[runbook]] for everyday operational commands.
