# Remote Infrastructure Mesh & Multi-Host Fleet Execution

`skat-os` transforms markdown documents into a high-performance **Remote Infrastructure Mesh**. Without leaving your document or configuring external orchestrators like Ansible, you can execute code blocks transparently against remote nodes, bastion-proxied VPCs, and entire multi-host server fleets.

---

## 1. Remote Execution Architecture

When a code block defines a `target` attribute, `skat-os` intercepts the execution path, resolves the SSH endpoint, establishes or retrieves a pooled SSH session, and streams remote PTY output directly into the document:

```mermaid
graph TD
    A["skat-os Document<br>(Runbook / Playbook)"] -->|"target='@web_servers'<br>parallel=true"| B["Mesh Orchestrator"]
    B --> C["SshSessionPool"]

    subgraph "Authentication & Pooling"
        C --> D["ssh-agent ($SSH_AUTH_SOCK)"]
        C --> E["Keys (~/.ssh/id_ed25519)"]
        C --> F["Passwords / Vault Tokens"]
    end

    subgraph "Infrastructure Network"
        C -->|"proxy='ssh://bastion.corp.com'"| G["Bastion Jump Host"]
        G -->|"channel_direct_tcpip"| H["Node 1 (10.0.1.10)"]
        G -->|"channel_direct_tcpip"| I["Node 2 (10.0.1.11)"]
        G -->|"channel_direct_tcpip"| J["Node 3 (10.0.1.12)"]
    end

    H --> K["Aggregated Output Canvas<br>[Tabs: Node 1 | Node 2 | Node 3]"]
    I --> K
    J --> K
```

---

## 2. Target Syntax & Addressing

`skat-os` accepts diverse target formats in code block headers:

### Direct SSH URIs

````markdown
```bash target="ssh://ubuntu@10.0.0.12:22"
uname -a
```
````

Supported endpoint variations:
- Standard URI: `target="ssh://user@host:port"`
- Password URI: `target="ssh://deploy:secret@10.0.1.5"`
- User & Host: `target="ubuntu@10.0.0.12"`
- Host & Port: `target="10.0.0.12:2222"`
- Bare Host / IP: `target="prod-app-01.internal"`
- IPv6 Addresses: `target="ssh://[2001:db8::1]:2222"`

### Parameterized Targets

Combine target attributes with inline form parameters to build flexible runbooks:

````markdown
Active Bastion: {{param:input:BASTION=bastion.corp.internal}}
Target Host:    {{param:input:TARGET_HOST=node-42.internal}}

```bash target="ssh://admin@{{params.TARGET_HOST}}" proxy="ssh://{{params.BASTION}}"
uptime
```
````

### Host Groups (`@group_name`)

Target multiple servers simultaneously by prefixing a group name with `@`:

````markdown
```bash target="@web_servers" parallel=true
systemctl status nginx
```
````

---

## 3. Host Group Definitions

Host groups can be defined locally inside document frontmatter or globally across the workspace.

### Option A: Document YAML Frontmatter

Define groups at the top of any markdown document inside YAML frontmatter fences (`---`):

```yaml
---
web_servers:
  - 10.0.0.1
  - 10.0.0.2
  - 10.0.0.3

db_nodes:
  - postgres-primary.internal
  - postgres-replica.internal
---

# Infrastructure Maintenance Playbook
```

### Option B: Workspace `hosts.yaml` File

Place a `hosts.yaml` (or `hosts.yml`) file in the root of your workspace to share group inventories across all documents:

```yaml
# ./hosts.yaml
hosts:
  web_servers:
    - 192.168.1.101
    - 192.168.1.102
    - 192.168.1.103

  staging_fleet:
    - 10.0.1.10, 10.0.1.11, 10.0.1.12

  k8s_control_plane:
    - k8s-master-01.corp
    - k8s-master-02.corp
```

> [!NOTE]
> **Resolution Priority**: When `@group_name` is referenced, `skat-os` first checks the active document's YAML frontmatter. If not found, it queries `hosts.yaml` in the workspace root.

---

## 4. SSH Session Pooling & Authentication

Establishing a new SSH handshake for every command adds hundreds of milliseconds of latency. `skat-os` uses a persistent global **`SshSessionPool`** to multiplex commands across existing authenticated connections.

### Authentication Mechanisms
When connecting to a target, `skat-os` attempts authentication in the following order:
1. **`ssh-agent`**: Checks `$SSH_AUTH_SOCK` for active identity agents. If available, keys are offered automatically.
2. **Standard SSH Keys**: Searches default user identity files:
   - `~/.ssh/id_ed25519`
   - `~/.ssh/id_rsa`
   - `~/.ssh/id_ecdsa`
   - `~/.ssh/id_dsa`
3. **Password / Credentials**: Uses credentials embedded in the URI (`ssh://user:pass@host`) or resolved from the [Secret Vault](security-vault-audit.md).

### Connection Re-Use & Eviction
- Authenticated `ssh2::Session` handles remain pooled in memory. Subsequent code blocks targeting the same host execute with sub-millisecond start times.
- If a connection drops (e.g. network blip or remote reboot), `skat-os` evicts the dead session from the pool and transparently re-authenticates on the next invocation.

---

## 5. Transparent Bastion Jump Hosts (`proxy`)

Many production systems are located in private subnets with no direct public access. `skat-os` supports transparent bastion jump host proxying via the `proxy` attribute:

````markdown
```bash target="ssh://ubuntu@10.0.1.50" proxy="ssh://bastion.corp.com:22"
cat /etc/os-release
```
````

### Under the Hood
1. `skat-os` authenticates to the bastion (`bastion.corp.com`).
2. Opens a direct TCP channel (`channel_direct_tcpip`) through the bastion to the private target (`10.0.1.50:22`).
3. Bridges the channel through a local UNIX socket pair (`UnixStream::pair()`).
4. Establishes the second encrypted SSH handshake through the tunnel.
5. All traffic between your terminal and the target node is end-to-end encrypted; the bastion cannot read payload plaintext.

---

## 6. Parallel Fleet Fan-Out (`parallel=true`)

Run commands concurrently across dozens of machines by setting `parallel=true`:

````markdown
```bash target="@web_servers" parallel=true
df -h / | awk 'NR==2 {print $5}'
```
````

When `parallel=true` is enabled:
- `skat-os` spawns concurrent worker threads for all hosts in the target group.
- Execution runs in parallel, significantly reducing total playbook run time.
- Status badges and per-host outputs are aggregated into a unified output canvas.

---

## 7. Aggregated Output Canvas & Tab Navigation

Multi-host execution results in an interactive tabbed output interface:

````markdown
```output [exit: 0]
[10.0.0.1: ✓ exit 0] [10.0.0.2: ✓ exit 0] [10.0.0.3: ✖ exit 1]

=== [10.0.0.1] ===
Filesystem Size Used Avail Use% Mounted on
/dev/sda1   40G  14G   26G  35% /

=== [10.0.0.2] ===
Filesystem Size Used Avail Use% Mounted on
/dev/sda1   40G  18G   22G  45% /

=== [10.0.0.3] ===
--- stderr ---
df: /: Permission denied
```
````

### Header Status Badges
- `[<host>: ✓ exit 0]`: Indicates successful completion on that host.
- `[<host>: ✖ exit <code\>]`: Indicates execution failure.

### Interactive Sub-View Navigation
When your cursor is placed inside a multi-host output block:
- Press **`[`** or **`Left Arrow`**: Switch to previous host sub-view tab.
- Press **`]`** or **`Right Arrow`**: Switch to next host sub-view tab.

---

## 8. Multi-Host Variable Chaining

Outputs from multi-host blocks can be interpolated downstream using specific host indexes or aggregate variables:

### Host Indexing
| Variable Syntax | Description |
| :--- | :--- |
| `{{web_servers[0].stdout}}` | Standard output of the first host in the group |
| `{{web_servers[1].stdout}}` | Standard output of the second host in the group |
| `$SKAT_WEB_SERVERS_0_STDOUT` | Auto-injected environment variable for host 0 stdout |
| `$SKAT_WEB_SERVERS_1_EXIT_CODE`| Auto-injected environment variable for host 1 exit code |

### Fleet Aggregate Variables
| Aggregate Syntax | Description | Example Value |
| :--- | :--- | :--- |
| `{{web_servers.failed}}` | List of host addresses that exited with non-zero code | `["10.0.0.3"]` |
| `{{web_servers.success}}`| Boolean indicating whether all hosts succeeded | `false` |
| `{{web_servers.count}}`  | Total number of hosts targeted | `3` |
| `$SKAT_WEB_SERVERS_FAILED` | Space-delimited string of failed host addresses | `"10.0.0.3"` |

### Downstream Remediation Example

````markdown
```bash name=check_disk target="@web_servers" parallel=true
df -h / | awk 'NR==2 {print $5}'
```

```bash name=remediation
if [ "{{check_disk.success}}" != "true" ]; then
    echo "Warning: The following hosts failed disk check: {{check_disk.failed}}"
    for host in $SKAT_CHECK_DISK_FAILED; do
        echo "Alerting pagerduty for $host..."
    done
fi
```
````

---

## 9. Complete Fleet Health Playbook

````markdown
---
prod_cluster:
  - 10.0.1.10
  - 10.0.1.11
  - 10.0.1.12
---

# Fleet Rolling Restart & Verification

### Target Selection
Cluster: {{param:select:CLUSTER=prod_cluster}}

### Step 1: Health Diagnostic
```bash name=cluster_uptime target="@{{params.CLUSTER}}" parallel=true
uptime
```

### Step 2: Graceful Rolling Restart (Guardrail Protected)
```bash name=restart_service target="@{{params.CLUSTER}}" confirm="Restart service across @{{params.CLUSTER}}?" danger=true
sudo systemctl reload nginx
```

### Step 3: Verify Status Post-Restart
```bash name=verify_ports target="@{{params.CLUSTER}}" parallel=true
curl -s -o /dev/null -w "%{http_code}" http://localhost:80/healthz
```
````

---

## 10. Related Documentation

- **[Getting Started](getting-started.md)**: Workspace setup and navigation.
- **[Interactive Documents](interactive-documents.md)**: Form parameters, tables, and charts.
- **[Multiplayer Collaboration](multiplayer.md)**: Collaborative co-execution and consensus voting.
- **[Security, Vault & Audit](security-vault-audit.md)**: Command guardrails and secret management.
