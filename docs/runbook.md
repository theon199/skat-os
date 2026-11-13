# Developer Runbook & Deployment Playbook

This document serves as an interactive deployment playbook with chained execution and synced task checklists.

### Pre-flight Checklist
- [ ] Verify local branch and working tree
- [ ] Check available system disk space
- [ ] Confirm network connectivity to services

Run individual blocks with **`Ctrl+R`**, or execute the entire playbook sequentially using **`Ctrl+Shift+R`** or **`F5`**!

---

### Step 1: Detect Active Git Branch
```bash name=get_branch
git rev-parse --abbrev-ref HEAD
```

### Step 2: Query System Architecture
```bash name=get_arch
uname -sm
```

### Step 3: Chained Summary & Notification
```bash name=deploy_report
echo "Deploying branch '{{get_branch.stdout}}' on platform '{{get_arch.stdout}}'"
echo "Environment variable injection check: $SKAT_GET_BRANCH_STDOUT"
```

### Step 4: Network Verification
```bash name=health_check
curl -I -s --max-time 3 https://www.google.com | head -n 1
```

---

Back to [[welcome]].
