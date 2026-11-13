# Daily Worklog & Checklist

Active Workspace: {{param:input:FOCUS=Development & Infra Maintenance}}

### Today's Checklist
- [ ] Review pending pull requests and system alerts
- [ ] Run daily fleet health verification in [[fleet]]
- [ ] Update living runbooks and parameters in [[runbook]]

---

### Quick System Overview
Place cursor inside and press **`Ctrl+R`** to run in-place:

```bash name=quick_diagnostics
uname -srm
uptime
df -h / | awk 'NR==1 || NR==2'
```

---

### Notes & Scratchpad
Use this space for daily scratch notes, meeting logs, or command snippets.
Link to other topics via `[[welcome]]`, `[[architecture]]`, `[[runbook]]`, or `[[fleet]]`.
