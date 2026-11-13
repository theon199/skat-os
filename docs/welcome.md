# Welcome to skat-os

`skat-os` is a lightweight, distraction-free **Document Operating System**.
Here, documents are living workspaces, wikis, and interactive shells.

---

### Quick Start & Keybindings
- **`Ctrl+K`**: Open Unified Command Palette (jump files, full-text search, actions)
- **`Ctrl+B`**: Toggle Sidebar (shows file tree and backlinks)
- **`Ctrl+W`** or **`Tab`**: Switch focus between Sidebar and Editor
- **`Ctrl+R`**: Run code block under cursor (outputs inline below)
- **`Ctrl+O`**: Collapse or expand command output blocks
- **`Ctrl+S`**: Save current document
- **`F1`**: Show complete Help & Keybindings cheat-sheet
- **`Ctrl+Q`**: Quit skat-os

---

### Living Runbooks & Code Execution
You can embed executable code blocks directly in your notes.
Try placing your cursor inside the block below and pressing **`Ctrl+R`**:

```bash
echo "=== System Overview ==="
uname -srm
uptime
echo "Current Directory:" $(pwd)
```

Notice how the command output is captured and saved right inside the document!
Jump over to [[runbook]] or [[architecture]] to see more!
