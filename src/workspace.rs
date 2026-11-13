use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DocumentMeta {
    pub path: PathBuf,
    pub relative_path: String,
    pub title: String,
    pub last_modified: SystemTime,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub file_path: PathBuf,
    pub relative_path: String,
    pub line_number: usize,
    pub line_content: String,
}

pub struct Workspace {
    pub root_dir: PathBuf,
    pub docs: Vec<DocumentMeta>,
    pub backlinks: HashMap<String, Vec<String>>,
    link_regex: Regex,
}

impl Workspace {
    pub fn new(root_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root_dir)
            .with_context(|| format!("Failed to create workspace directory at {:?}", root_dir))?;

        let link_regex = Regex::new(r"\[\[([^\]]+)\]\]")?;

        let mut ws = Self {
            root_dir,
            docs: Vec::new(),
            backlinks: HashMap::new(),
            link_regex,
        };

        ws.ensure_seed_documents()?;
        ws.refresh()?;
        Ok(ws)
    }

    pub fn refresh(&mut self) -> Result<()> {
        let mut new_docs = Vec::new();

        for entry in WalkDir::new(&self.root_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "md" {
                        let path = entry.path().to_path_buf();
                        let rel = path
                            .strip_prefix(&self.root_dir)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .to_string();

                        let metadata = fs::metadata(&path).ok();
                        let last_mod = metadata
                            .and_then(|m| m.modified().ok())
                            .unwrap_or(SystemTime::UNIX_EPOCH);

                        let file_stem = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Untitled".to_string());

                        let title = match fs::read_to_string(&path) {
                            Ok(content) => Self::extract_title(&content, &file_stem),
                            Err(_) => file_stem,
                        };

                        new_docs.push(DocumentMeta {
                            path,
                            relative_path: rel,
                            title,
                            last_modified: last_mod,
                        });
                    }
                }
            }
        }

        // Sort alphabetically by relative path
        new_docs.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        self.docs = new_docs;
        self.rebuild_backlinks();

        Ok(())
    }

    pub fn extract_title(content: &str, fallback: &str) -> String {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(heading) = trimmed.strip_prefix("# ") {
                let h = heading.trim();
                if !h.is_empty() {
                    return h.to_string();
                }
            }
        }
        fallback.to_string()
    }

    pub fn rebuild_backlinks(&mut self) {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();

        for doc in &self.docs {
            if let Ok(content) = fs::read_to_string(&doc.path) {
                for cap in self.link_regex.captures_iter(&content) {
                    if let Some(m) = cap.get(1) {
                        let target = m.as_str().trim().to_lowercase();
                        map.entry(target).or_default().push(doc.relative_path.clone());
                    }
                }
            }
        }

        // Deduplicate backlinks
        for sources in map.values_mut() {
            sources.sort();
            sources.dedup();
        }

        self.backlinks = map;
    }

    pub fn get_backlinks_for(&self, path: &Path) -> Vec<String> {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string().to_lowercase())
            .unwrap_or_default();

        self.backlinks.get(&stem).cloned().unwrap_or_default()
    }

    pub fn create_document(&mut self, raw_name: &str) -> Result<PathBuf> {
        let mut name = raw_name.trim().to_string();
        if !name.ends_with(".md") {
            name.push_str(".md");
        }

        let full_path = self.root_dir.join(&name);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if !full_path.exists() {
            let stem = full_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "New Document".to_string());

            let initial_content = format!("# {}\n\nStart typing here...\n", stem);
            fs::write(&full_path, initial_content)?;
        }

        self.refresh()?;
        Ok(full_path)
    }

    pub fn delete_document(&mut self, path: &Path) -> Result<()> {
        if path.exists() {
            fs::remove_file(path)?;
        }
        self.refresh()?;
        Ok(())
    }

    pub fn search_content(&self, query: &str) -> Vec<SearchMatch> {
        if query.trim().is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        for doc in &self.docs {
            if let Ok(content) = fs::read_to_string(&doc.path) {
                for (idx, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&query_lower) {
                        matches.push(SearchMatch {
                            file_path: doc.path.clone(),
                            relative_path: doc.relative_path.clone(),
                            line_number: idx + 1,
                            line_content: line.trim().to_string(),
                        });
                    }
                }
            }
        }

        matches
    }

    fn ensure_seed_documents(&self) -> Result<()> {
        let welcome_path = self.root_dir.join("welcome.md");
        if !welcome_path.exists() {
            let welcome = r#"# Welcome to skat-os

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
Jump over to [[daily]] for tasks, [[runbook]] for living playbooks, [[fleet]] for SSH infra, or [[architecture]] to learn more!
"#;
            fs::write(&welcome_path, welcome)?;
        }

        let arch_path = self.root_dir.join("architecture.md");
        if !arch_path.exists() {
            let arch = r#"# skat-os Architecture

Traditional operating systems treat documents as passive text files opened by applications.
`skat-os` inverts this model: **The Document is the System**.

### Core Tenets
1. **Plain Markdown**: Zero vendor lock-in. 100% Git, Obsidian, and Vim friendly.
2. **Interactive Run-in-Place**: Documents are living playbooks. Code fences are executable targets.
3. **Hyperlinked Knowledge**: Wiki-links like `[[welcome]]`, `[[daily]]`, and `[[runbook]]` automatically build a bidirectional backlink graph in the sidebar.
4. **Single Static Binary**: Powered by Rust, Ratatui, and Crossterm for zero-dependency portability.

See [[daily]] for your active checklist, [[runbook]] for everyday operational commands, and [[fleet]] for multi-node SSH workflows.
"#;
            fs::write(&arch_path, arch)?;
        }

        let daily_path = self.root_dir.join("daily.md");
        if !daily_path.exists() {
            let daily = r#"# Daily Worklog & Checklist

Active Workspace: {{param:input:FOCUS=Development & Infra Maintenance}}

### 📋 Today's Checklist
- [ ] Review pending pull requests and system alerts
- [ ] Run daily fleet health verification in [[fleet]]
- [ ] Update living runbooks and parameters in [[runbook]]

---

### 💻 Quick System Overview
Place cursor inside and press **`Ctrl+R`** to run in-place:

```bash name=quick_diagnostics
uname -srm
uptime
df -h / | awk 'NR==1 || NR==2'
```

---

### 📝 Notes & Scratchpad
Use this space for daily scratch notes, meeting logs, or command snippets.
Link to other topics via `[[welcome]]`, `[[architecture]]`, `[[runbook]]`, or `[[fleet]]`.
"#;
            fs::write(&daily_path, daily)?;
        }

        let fleet_path = self.root_dir.join("fleet.md");
        if !fleet_path.exists() {
            let fleet = r#"# Infrastructure Fleet & Health Playbook

Run parallel diagnostics across your infrastructure mesh or local nodes.

### Target Environment
- Target cluster: {{param:select:CLUSTER=staging,prod-us,prod-eu}}
- Host group: {{param:input:HOSTS=@web_servers}}

---

### Step 1: Fleet Host Status & Uptime
```bash name=fleet_uptime target="{{params.HOSTS}}" parallel=true
uptime
```

### Step 2: System Disk Space Verification
```bash name=fleet_disk target="{{params.HOSTS}}" parallel=true
df -h / | awk 'NR==1 || NR==2'
```

### Step 3: Critical Service Restart (Guardrail Protected)
```bash name=service_restart target="{{params.HOSTS}}" confirm="Restart application services across target fleet?" danger=true
echo "Gracefully reloading service on $(hostname)..."
```

---

Back to [[daily]] or [[welcome]].
"#;
            fs::write(&fleet_path, fleet)?;
        }

        let runbook_path = self.root_dir.join("runbook.md");
        if !runbook_path.exists() {
            let runbook = r#"# Developer Runbook & Deployment Playbook

This document serves as an interactive deployment playbook with parameterized form controls, structured data grids, chained execution, and synced task checklists.

### Runbook Parameters
- Environment: {{param:select:ENV=staging,prod-us-east,prod-eu-west}}
- Namespace: {{param:input:NAMESPACE=production}}
- Dry run: {{param:toggle:DRY_RUN=false}}

(Use **`Tab`** / **`Shift+Tab`** to navigate parameters, **`Space`** to toggle / cycle, or **`Enter`** to edit)

### Pre-flight Checklist
- [ ] Verify local branch and working tree
- [ ] Confirm target cluster and namespace parameters
- [ ] Check available system disk space

---

### Step 1: Detect Active Git Branch
```bash name=get_branch
git rev-parse --abbrev-ref HEAD
```

### Step 2: Query System Architecture
```bash name=get_arch
uname -sm
```

### Step 3: Parameterized Deployment Report
```bash name=deploy_report
echo "Deploying branch '{{get_branch.stdout}}' to {{params.ENV}} (namespace: {{params.NAMESPACE}}, dry_run: {{params.DRY_RUN}})"
echo "Environment variable injection check: $SKAT_PARAM_ENV in namespace $SKAT_PARAM_NAMESPACE"
```

### Step 4: Active Services Inventory (format=table)
```bash name=services format=table
echo '[{"service": "api-gateway", "cluster": "{{params.ENV}}", "replicas": 3, "status": "Healthy"}, {"service": "worker-pool", "cluster": "{{params.ENV}}", "replicas": 8, "status": "Healthy"}]'
```

### Step 5: Live Latency Metric (format=sparkline)
```bash name=latency format=sparkline
echo "12, 18, 15, 23, 19, 34, 28, 22, 19, 15"
```

### Step 6: Zero-Trust Guardrail & Secret Deployment
- Deployment Token: {{param:secret:DEPLOY_TOKEN=sk_live_secret_token_abcdef123456}}

```bash name=safe_deploy dry_run=true confirm="Ready to dispatch rollout to production fleet?"
echo "Executing safe rollout with masked token: {{secret.DEPLOY_TOKEN}}"
echo "Dry run simulation flag: $SKAT_DRY_RUN"
```

---

Back to [[welcome]].
"#;
            fs::write(&runbook_path, runbook)?;
        }

        Ok(())
    }
}
