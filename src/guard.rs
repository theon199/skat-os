use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::LazyLock;

static RM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\brm\s+(?:-[a-z0-9]*\s+)*(?:-[a-z0-9]*(?:r[a-z0-9]*f|f[a-z0-9]*r)[a-z0-9]*|--recursive\s+--force|--force\s+--recursive)\s+.*?(?:[~/](?:\s|$|/\*)|/\*|\.\.?(?:\s|$|/)|--no-preserve-root|\$HOME|\$ROOT)",
    )
    .unwrap()
});

static RM_NO_PRESERVE_ROOT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\brm\s+.*?--no-preserve-root\b").unwrap()
});

static MKFS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bmkfs(?:\.[a-z0-9]+)?\b").unwrap()
});

static DD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bdd\s+.*of=["']?/dev/(?:r?disk\d+|sd[a-z]\d*|nvme\d+n\d+(?:p\d+)?|vd[a-z]\d*|mapper/)"#)
        .unwrap()
});

static DROP_DB_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bDROP\s+(?:DATABASE|SCHEMA)\s+(?:IF\s+EXISTS\s+)?([`"']?[a-zA-Z0-9_]+[`"']?)"#).unwrap()
});

static DROP_TBL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bDROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?([`"']?[a-zA-Z0-9_]+[`"']?)"#).unwrap()
});

static TRUNCATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bTRUNCATE\s+(?:TABLE\s+)?(?:ONLY\s+)?([`"']?[a-zA-Z0-9_]+[`"']?)"#).unwrap()
});

static IPTABLES_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\biptables\s+.*(?:-F|--flush|-X|--delete-chain)\b").unwrap()
});

static K8S_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bkubectl\s+delete\s+(?:namespaces?|ns|nodes?|all)\b").unwrap()
});

static FORKBOMB_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":\(\)\s*\{\s*:\|:&\s*\};:").unwrap()
});

static CHMOD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bchmod\s+-[a-z0-9]*r[a-z0-9]*\s+(?:0?777)\s+(?:/|/\*)").unwrap()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DangerLevel {
    Normal,
    Warning,
    High,
    Critical,
}

impl std::fmt::Display for DangerLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DangerLevel::Normal => write!(f, "NORMAL"),
            DangerLevel::Warning => write!(f, "WARNING"),
            DangerLevel::High => write!(f, "HIGH"),
            DangerLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityAnalysisResult {
    pub is_destructive: bool,
    pub matched_patterns: Vec<String>,
    pub danger_level: DangerLevel,
    pub reason: String,
}

impl Default for SecurityAnalysisResult {
    fn default() -> Self {
        Self {
            is_destructive: false,
            matched_patterns: Vec::new(),
            danger_level: DangerLevel::Normal,
            reason: String::new(),
        }
    }
}

pub struct StaticAnalyzer;

impl StaticAnalyzer {
    /// Analyzes a code snippet for destructive shell commands, SQL statements, and infrastructure operations.
    pub fn analyze(code: &str) -> SecurityAnalysisResult {
        let mut matched_patterns = Vec::new();
        let mut highest_level = DangerLevel::Normal;
        let mut reasons = Vec::new();

        // 1. Critical filesystem destruction: rm -rf / or rm -fr / or rm -rf /* or rm -rf .
        if let Some(caps) = RM_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical;
            reasons.push(format!("Recursive filesystem wipe detected ('{}')", &caps[0]));
        } else if let Some(caps) = RM_NO_PRESERVE_ROOT.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical;
            reasons.push(format!("Filesystem wipe with --no-preserve-root detected ('{}')", &caps[0]));
        }

        // 2. Drive formatting and low-level block overwriting: mkfs, dd to device
        if let Some(caps) = MKFS_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical.max(highest_level);
            reasons.push(format!("Filesystem creation/format command detected ('{}')", &caps[0]));
        }

        if let Some(caps) = DD_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical.max(highest_level);
            reasons.push(format!("Raw disk block overwrite detected ('{}')", &caps[0]));
        }

        // 3. Database destructive actions: DROP DATABASE, DROP SCHEMA, DROP TABLE, TRUNCATE
        if let Some(caps) = DROP_DB_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical.max(highest_level);
            reasons.push(format!("Database/schema deletion detected ('{}')", &caps[0]));
        }

        if let Some(caps) = DROP_TBL_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::High.max(highest_level);
            reasons.push(format!("Table drop detected ('{}')", &caps[0]));
        }

        if let Some(caps) = TRUNCATE_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::High.max(highest_level);
            reasons.push(format!("Table truncation detected ('{}')", &caps[0]));
        }

        // 4. Network and firewall resets: iptables -F, iptables --flush
        if let Some(caps) = IPTABLES_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::High.max(highest_level);
            reasons.push(format!("Firewall flush/reset detected ('{}')", &caps[0]));
        }

        // 5. Container & Cloud fleet deletion: kubectl delete namespace / ns / node
        if let Some(caps) = K8S_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical.max(highest_level);
            reasons.push(format!("Kubernetes cluster resource deletion detected ('{}')", &caps[0]));
        }

        // 6. Fork bombs
        if let Some(caps) = FORKBOMB_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical;
            reasons.push("Bash fork bomb denial of service pattern detected".to_string());
        }

        // 7. Wide permissions opening: chmod -R 777 /
        if let Some(caps) = CHMOD_REGEX.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::High.max(highest_level);
            reasons.push("Insecure root filesystem permissions overwrite detected".to_string());
        }

        let is_destructive = !matched_patterns.is_empty();
        let reason = reasons.join("; ");

        SecurityAnalysisResult {
            is_destructive,
            matched_patterns,
            danger_level: highest_level,
            reason,
        }
    }
}

/// Tracks multiplayer consensus approval for dangerous code block execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalConsensus {
    pub block_id: String,
    pub command_preview: String,
    pub required_approvals: usize,
    pub approvers: HashSet<String>,
}

impl ApprovalConsensus {
    pub fn new(block_id: String, command_preview: String, required_approvals: usize) -> Self {
        Self {
            block_id,
            command_preview,
            required_approvals,
            approvers: HashSet::new(),
        }
    }

    /// Records an approval vote from a peer. Returns true if consensus is met.
    pub fn add_approval(&mut self, approver_name: String) -> bool {
        let trimmed = approver_name.trim();
        if !trimmed.is_empty() {
            self.approvers.insert(trimmed.to_string());
        }
        self.is_approved()
    }

    /// Checks if a peer has already voted approval
    pub fn has_approved(&self, approver_name: &str) -> bool {
        self.approvers.contains(approver_name.trim())
    }

    /// Removes an approval vote from a peer. Returns true if consensus is still held.
    pub fn remove_approval(&mut self, approver_name: &str) -> bool {
        self.approvers.remove(approver_name.trim());
        self.is_approved()
    }

    /// Whether the required number of approvals has been reached
    pub fn is_approved(&self) -> bool {
        self.approvers.len() >= self.required_approvals
    }

    /// Current count of approvals gathered
    pub fn current_count(&self) -> usize {
        self.approvers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_rm_rf_root() {
        let res = StaticAnalyzer::analyze("echo 'cleaning'; rm -rf /");
        assert!(res.is_destructive);
        assert_eq!(res.danger_level, DangerLevel::Critical);
        assert!(res.reason.contains("Recursive filesystem wipe"));
    }

    #[test]
    fn test_detect_mkfs_and_dd() {
        let res1 = StaticAnalyzer::analyze("mkfs.ext4 /dev/sdb1");
        assert!(res1.is_destructive);
        assert_eq!(res1.danger_level, DangerLevel::Critical);

        let res2 = StaticAnalyzer::analyze("dd if=/dev/zero of=/dev/sda bs=1M");
        assert!(res2.is_destructive);
        assert_eq!(res2.danger_level, DangerLevel::Critical);
    }

    #[test]
    fn test_detect_sql_and_k8s_destruction() {
        let res1 = StaticAnalyzer::analyze("DROP DATABASE production_cluster;");
        assert!(res1.is_destructive);
        assert_eq!(res1.danger_level, DangerLevel::Critical);

        let res2 = StaticAnalyzer::analyze("kubectl delete ns production");
        assert!(res2.is_destructive);
        assert_eq!(res2.danger_level, DangerLevel::Critical);

        let res3 = StaticAnalyzer::analyze("iptables -F");
        assert!(res3.is_destructive);
        assert_eq!(res3.danger_level, DangerLevel::High);
    }

    #[test]
    fn test_safe_commands_pass() {
        let res = StaticAnalyzer::analyze("kubectl get pods -A\nls -la /var/log\necho 'hello'");
        assert!(!res.is_destructive);
        assert_eq!(res.danger_level, DangerLevel::Normal);
        assert!(res.matched_patterns.is_empty());
    }

    #[test]
    fn test_approval_consensus() {
        let mut consensus = ApprovalConsensus::new("deploy_prod".to_string(), "deploy.sh".to_string(), 2);
        assert!(!consensus.is_approved());
        assert_eq!(consensus.current_count(), 0);

        let reached1 = consensus.add_approval("Alice".to_string());
        assert!(!reached1);
        assert!(!consensus.is_approved());
        assert_eq!(consensus.current_count(), 1);

        let reached2 = consensus.add_approval("Bob".to_string());
        assert!(reached2);
        assert!(consensus.is_approved());
        assert_eq!(consensus.current_count(), 2);
        assert!(consensus.has_approved("Alice"));
        assert!(consensus.has_approved("Bob"));
        assert!(!consensus.has_approved("Charlie"));

        // Removing an approval drops below consensus threshold
        let still_approved = consensus.remove_approval("Alice");
        assert!(!still_approved);
        assert!(!consensus.is_approved());
        assert_eq!(consensus.current_count(), 1);
    }

    #[test]
    fn test_detect_chmod_uppercase_and_rm_fr() {
        // Uppercase -R chmod
        let res1 = StaticAnalyzer::analyze("chmod -R 777 /");
        assert!(res1.is_destructive);
        assert_eq!(res1.danger_level, DangerLevel::High);

        // rm -fr reverse flag order
        let res2 = StaticAnalyzer::analyze("rm -fr /");
        assert!(res2.is_destructive);
        assert_eq!(res2.danger_level, DangerLevel::Critical);

        // DROP SCHEMA IF EXISTS
        let res3 = StaticAnalyzer::analyze("DROP SCHEMA IF EXISTS analytics;");
        assert!(res3.is_destructive);
        assert_eq!(res3.danger_level, DangerLevel::Critical);
    }
}
