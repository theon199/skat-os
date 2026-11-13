# Infrastructure Fleet & Health Playbook

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

### Back to [[daily]] or [[welcome]].
