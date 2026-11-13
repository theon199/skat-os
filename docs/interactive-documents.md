# Interactive Documents & Living Playbooks

In `skat-os`, documents are not static prose—they are **living execution environments**. You can combine explanatory notes, interactive task checklists, parameter forms, chained executable code blocks, structured data tables, and real-time metric charts inside a single standard Markdown file.

---

## 1. Code Blocks & Execution Engine

Fenced code blocks in `skat-os` are first-class execution units.

### Supported Languages & Runtimes

`skat-os` natively supports running code blocks in multiple languages without external plugins:

| Language Identifier | Runtime Invoked | Command Executed |
| :--- | :--- | :--- |
| `bash` (default) | System Bash | `bash -c "<code>"` |
| `zsh` | Z Shell | `zsh -c "<code>"` |
| `sh` | Standard POSIX Shell | `sh -c "<code>"` |
| `python`, `py`, `python3` | Python 3 | `python3 -c "<code>"` |
| `node`, `js`, `javascript` | Node.js | `node -e "<code>"` |

### Fenced Block Attributes

You can attach key-value attributes to the fence opening line to control execution behavior:

````markdown
```bash name="query_cluster" format="table" target="@prod_nodes" confirm="Run production query?"
kubectl get pods -A -o json
```
````

| Attribute | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `name` | string | `None` | Unique name for reactive output chaining (e.g. `name=get_pod`) |
| `format` | string | `text` | Output renderer: `text`, `table`, `chart`, or `sparkline` |
| `target` | string | `None` | Remote target URI (e.g. `target="ssh://user@host:22"`, `target="@group"`) |
| `proxy` | string | `None` | Bastion jump host URI (e.g. `proxy="ssh://bastion.corp.com"`) |
| `parallel` | boolean | `false` | Run concurrently across all target hosts (`parallel=true`) |
| `confirm` | string | `None` | Interactive confirmation prompt before execution |
| `danger` | boolean | `false` | Flags block as dangerous, enforcing safety dialog |
| `blast_radius` | string | `None` | Sets blast radius: `"critical"` or `"high"` |
| `dry_run` | boolean | `false` | Injects `$SKAT_DRY_RUN=1` environment variable for simulation |
| `approvals` | integer | `1` | Number of multiplayer peer approvals required before execution |

---

## 2. Output Blocks & Real-Time PTY Streaming

When you run a code block with **`Ctrl+R`**, `skat-os` allocates a **pseudo-terminal (PTY)** via `portable-pty`. The process runs asynchronously on a background worker thread, ensuring the terminal interface remains completely fluid at 60 FPS.

### Output Block Anatomy

Output is captured directly beneath the code block inside a matching ```` ```output ```` fence:

````markdown
```output [exit: 0]
Cluster connection verified: 3 nodes healthy.
```
````

Output headers reflect the execution state:
- ```` ```output [running] ````: Command is currently executing; output streams in real time.
- ```` ```output [exit: 0] ````: Command completed successfully with returncode 0.
- ```` ```output [exit: 1] ````: Command failed with non-zero exit code.
- ```` ```output [interrupted] ````: Command was cancelled by the user pressing `Ctrl+C`.
- ```` ```output [exit: 0] [collapsed] ````: Output block is collapsed to save screen space.

### ANSI Colors & Terminal Control Codes

The `skat-os` ANSI stream processor parses VT100 / xterm-256 / 24-bit TrueColor escape sequences:
- **Colors**: Standard 16 ANSI colors, 256-color palette indices, and 24-bit RGB truecolor (`\x1b[38;2;r;g;bm`).
- **Styles**: Bold, dim, italic, underline, reversed, and reset.
- **Carriage Return (`\r`) In-Place Overwrites**: Dynamic progress bars and terminal spinners (like `curl`, `pip`, or `docker pull`) update their lines in place without generating thousands of duplicate trailing lines.

### Interactive Keyboard Forwarding (Stdin)

If a running command pauses to ask for user input (e.g., `[y/N]` confirmation, `read username`, password prompt):
- Simply type the response on your keyboard.
- `skat-os` automatically forwards the keystrokes down the PTY master channel to the child process stdin.

### Output Controls
- **Collapse / Expand (`Ctrl+O`)**: Toggle between viewing the full multi-line output or collapsing it into a compact badge.
- **Cancel Process (`Ctrl+C`)**: Immediately send `SIGINT` to the child process. The output block updates to `[interrupted]`.

---

## 3. Interactive Synced Task Checklists

`skat-os` turns standard markdown task items into interactive stateful checkboxes:

```markdown
- [ ] 1. Verify DNS records
- [ ] 2. Flush Redis cache
- [x] 3. Restart application service
```

### Checklist Controls
- Navigate your cursor to any checklist line (`- [ ]`, `* [ ]`, `- [x]`, or `* [x]`).
- Press **`Space`** to toggle between unchecked and checked states.
- **Status Bar Progress Indicator**: `skat-os` dynamically counts all task items across the active document and renders a live completion badge in the status bar:
  ```
  ✓ Tasks: 2/3 (66%)
  ```
- **Collaborative Sync**: In multiplayer sessions, checking an item immediately synchronizes across all connected peers over TCP.

---

## 4. Reactive Variable Chaining & Playbooks

You can chain code blocks into dynamic pipelines where downstream blocks consume outputs from upstream blocks.

### Step 1: Name Your Blocks
Assign a `name` attribute to any code block:

````markdown
```bash name=get_pod
kubectl get pods -l app=frontend -o jsonpath='{.items[0].metadata.name}'
```
````

### Step 2: Interpolate in Downstream Blocks

You can access upstream results using either **Template Syntax** or **Environment Variables**:

| Variable Syntax | Description | Example |
| :--- | :--- | :--- |
| `{{name.stdout}}` | Standard output (trailing newline stripped) | `echo "Pod is {{get_pod.stdout}}"` |
| `{{name.stderr}}` | Standard error stream | `echo "Errors: {{get_pod.stderr}}"` |
| `{{name.exit_code}}` | Return code integer | `if [ {{get_pod.exit_code}} -eq 0 ]; then` |
| `{{name[0].stdout}}` | Indexed host output (for multi-host fleet execution) | `echo "Node 0: {{nodes[0].stdout}}"` |
| `{{block_0.stdout}}` | Fallback positional index for unnamed blocks | `echo "Step 1 output: {{block_0.stdout}}"` |
| `$SKAT_<NAME>_STDOUT` | Auto-injected environment variable | `curl "http://api/$SKAT_GET_POD_STDOUT"` |
| `$SKAT_<NAME>_STDERR` | Auto-injected stderr environment variable | `echo "$SKAT_GET_POD_STDERR"` |
| `$SKAT_<NAME>_EXIT_CODE` | Auto-injected exit code environment variable | `test $SKAT_GET_POD_EXIT_CODE -eq 0` |

### Sequential Playbook Execution Modes

Instead of triggering blocks one-by-one, execute entire playbooks top-to-bottom:

- **Run All Blocks (`Ctrl+Shift+R` or `F5`)**: Executes all code blocks in the document in sequential order.
- **Run Up To Cursor** (via **`Ctrl+K`** -> `Run Playbook Up to Cursor`): Executes from the first block down to the block under the cursor.
- **Fail-Fast Safeguard**: If any block exits with a non-zero exit code (`exit_code != 0`), `skat-os` halts the playbook immediately, highlights the failed block in red, and cancels remaining blocks to prevent cascading failures.
- **Dependency Resolution**: `skat-os` builds a directed acyclic graph (DAG) of block references. Circular dependencies are automatically detected and flagged before execution begins.

---

## 5. Inline Parameterized Form Controls

Embed reactive form widgets directly inside markdown prose. Form parameters let teams standardize operational runbooks with safe inputs:

### Form Widget Types

```markdown
Target Environment: {{param:select:ENV=staging,prod-us,prod-eu}}
Cluster Namespace:  {{param:input:NAMESPACE=backend}}
Dry-Run Mode:       {{param:toggle:DRY_RUN=true}}
API Authorization:  {{param:secret:TOKEN}}
```

| Widget Type | Syntax | Description | Interaction |
| :--- | :--- | :--- | :--- |
| **Input** | `{{param:input:NAME=val}}` | Text input field | Press **`Enter`** to open modal edit prompt |
| **Select** | `{{param:select:NAME=opt1,opt2}}` | Dropdown choice | Press **`Space`** or **`Enter`** to cycle choices |
| **Toggle** | `{{param:toggle:NAME=true}}` | Boolean flag | Press **`Space`** or **`Enter`** to flip `true` / `false` |
| **Secret** | `{{param:secret:NAME=val}}` | Masked secret | Displays `••••••••`; resolved from vault |

### Navigation & Editing
- **`Tab`** / **`Shift+Tab`**: Cycle forward or backward through all form parameters in the document.
- **`Space`**: Toggle boolean flags or cycle through dropdown options in place.
- **`Enter`**: On an `input` parameter, opens a centered input modal to enter new text.
- **Multiplayer Param Sync**: When any collaborator modifies a parameter, a `ParamUpdate` event is broadcast across TCP, updating the value on all screens instantly.

### Injecting Parameters into Code
Reference parameters in any code block using template syntax or environment variables:

````markdown
```bash
echo "Deploying to cluster {{params.ENV}} in namespace {{params.NAMESPACE}}"
if [ "$SKAT_PARAM_DRY_RUN" = "true" ]; then
    echo "Dry-run enabled: skipping real changes"
fi
```
````

---

## 6. Structured Data Grids (`format=table`)

Code blocks that emit structured JSON arrays or CSV data can be rendered as interactive data tables.

### Declaration

Add `format="table"` to the code block fence:

````markdown
```bash name=k8s_pods format=table
kubectl get pods -n kube-system -o json
```
````

`skat-os` parses the JSON output and constructs an interactive Ratatui table:

```
+------------------------------------+-----------+----------+
| NAME                               | READY     | STATUS   |
+------------------------------------+-----------+----------+
| ▶ coredns-558bd4d5db-7l2kx         | 1/1       | Running  |
|   etcd-minikube                    | 1/1       | Running  |
|   kube-apiserver-minikube          | 1/1       | Running  |
+------------------------------------+-----------+----------+
```

### Table Navigation & Downstream Cell Binding
- **Select Rows (`Up` / `Down`)**: When the cursor is inside the table output, use arrow keys to navigate rows.
- **Inspect Row Modal (`Enter`)**: Opens a full-screen inspector displaying all JSON fields and values for the selected row.
- **Downstream Cell Binding**: Downstream blocks can bind to the currently selected row:
  - `{{k8s_pods.selected.NAME}}`
  - `$SKAT_K8S_PODS_SELECTED_NAME`
  - `$SKAT_K8S_PODS_SELECTED` (full row as JSON)

---

## 7. Real-Time Terminal Charts & Sparklines

Transform raw numerical stdout streams into visual ASCII/ANSI charts.

### Bar Charts (`format=chart` or `format=bar`)

Fenced blocks with `format=chart` parse key-value lines (`key: 42`, `key = 42`) or JSON objects and render Ratatui `BarChart` widgets:

````markdown
```bash format=chart
echo "cpu_user: 35"
echo "cpu_system: 20"
echo "cpu_idle: 45"
```
````

```
+------------------------------------+
| 100 ┤                              |
|  50 ┤          ██                  |
|     |  ██      ██      ██          |
|   0 ┴──user────system──idle────────┤
+------------------------------------+
```

### Sparklines (`format=sparkline` or `format=spark`)

Parse continuous numeric streams into compact sparkline graphs:

````markdown
```bash format=sparkline
cat /proc/loadavg | awk '{print $1*100, $2*100, $3*100}'
```
````

```
Load History: ▂▃▅▆▇█▇▆▅▃  [max: 250]
```

---

## 8. Complete Living Runbook Example

Here is an end-to-end example combining parameters, checklists, chained commands, tables, and guardrails:

````markdown
# Production Database Maintenance Playbook

### Configuration
- Target Environment: {{param:select:CLUSTER=staging,prod-us,prod-eu}}
- Dry-Run Execution:  {{param:toggle:SIMULATE=false}}
- Database Superuser: {{param:input:DB_USER=postgres}}

### Operational Checklist
- [ ] Notify team in Slack #ops channel
- [ ] Verify replica lag is under 100ms
- [ ] Create snapshot backup before proceeding

---

### Step 1: Check Active Connections
```bash name=db_conns format=table
psql -U {{params.DB_USER}} -c "SELECT pid, usename, client_addr, state FROM pg_stat_activity;" -o json
```

### Step 2: Replica Synchronization Check
```bash name=replica_lag format=sparkline
psql -U {{params.DB_USER}} -c "SELECT replay_lag_bytes FROM pg_stat_replication;"
```

### Step 3: Run Maintenance vacuum (Guardrail Protected)
```bash name=vacuum_db confirm="Run VACUUM ANALYZE on cluster {{params.CLUSTER}}?" danger=true
if [ "$SKAT_PARAM_SIMULATE" = "true" ]; then
    echo "[DRY-RUN] Would run VACUUM on {{params.CLUSTER}}"
else
    psql -U {{params.DB_USER}} -c "VACUUM ANALYZE;"
fi
```
````

---

## 9. Related Documentation

- **[Getting Started](getting-started.md)**: Workspace setup, navigation, and shortcuts.
- **[Multiplayer Collaboration](multiplayer.md)**: Real-time peer-to-peer editing and co-execution.
- **[Mesh & Fleet Orchestration](mesh-and-fleet.md)**: Remote execution over SSH with bastion jump hosts.
- **[Security, Vault & Audit](security-vault-audit.md)**: Dangerous command guardrails and secret management.
