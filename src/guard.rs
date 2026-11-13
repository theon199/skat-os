use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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

        // 1. Critical filesystem destruction: rm -rf / or rm -rf /* or rm -rf .
        let rm_regex = Regex::new(
            r"(?i)\brm\s+-[a-z0-9]*r[a-z0-9]*f[a-z0-9]*\s+([~/]|\.\.?(?:/|\s|$)|/\*|\$HOME|\$ROOT|--no-preserve-root)",
        )
        .unwrap();
        if let Some(caps) = rm_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical;
            reasons.push(format!("Recursive filesystem wipe detected ('{}')", &caps[0]));
        }

        // 2. Drive formatting and low-level block overwriting: mkfs, dd to device
        let mkfs_regex = Regex::new(r"\bmkfs(?:\.[a-z0-9]+)?\b").unwrap();
        if let Some(caps) = mkfs_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical.max(highest_level);
            reasons.push(format!("Filesystem creation/format command detected ('{}')", &caps[0]));
        }

        let dd_regex = Regex::new(r"\bdd\s+.*of=/dev/(?:sd[a-z]|nvme\d+n\d+|vd[a-z]|disk\d+)").unwrap();
        if let Some(caps) = dd_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical.max(highest_level);
            reasons.push(format!("Raw disk block overwrite detected ('{}')", &caps[0]));
        }

        // 3. Database destructive actions: DROP DATABASE, DROP TABLE, TRUNCATE
        let drop_db_regex = Regex::new(r"(?i)\bDROP\s+DATABASE\s+([a-zA-Z0-9_]+)").unwrap();
        if let Some(caps) = drop_db_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical.max(highest_level);
            reasons.push(format!("Database deletion detected ('{}')", &caps[0]));
        }

        let drop_tbl_regex = Regex::new(r"(?i)\bDROP\s+TABLE\s+([a-zA-Z0-9_]+)").unwrap();
        if let Some(caps) = drop_tbl_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::High.max(highest_level);
            reasons.push(format!("Table drop detected ('{}')", &caps[0]));
        }

        let truncate_regex = Regex::new(r"(?i)\bTRUNCATE\s+(?:TABLE\s+)?([a-zA-Z0-9_]+)").unwrap();
        if let Some(caps) = truncate_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::High.max(highest_level);
            reasons.push(format!("Table truncation detected ('{}')", &caps[0]));
        }

        // 4. Network and firewall resets: iptables -F, iptables --flush
        let iptables_regex = Regex::new(r"(?i)\biptables\s+.*(?:-F|--flush|-X|--delete-chain)\b").unwrap();
        if let Some(caps) = iptables_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::High.max(highest_level);
            reasons.push(format!("Firewall flush/reset detected ('{}')", &caps[0]));
        }

        // 5. Container & Cloud fleet deletion: kubectl delete namespace / ns / node
        let k8s_regex = Regex::new(r"(?i)\bkubectl\s+delete\s+(namespace|ns|nodes?|all)\b").unwrap();
        if let Some(caps) = k8s_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical.max(highest_level);
            reasons.push(format!("Kubernetes cluster resource deletion detected ('{}')", &caps[0]));
        }

        // 6. Fork bombs
        let forkbomb_regex = Regex::new(r":\(\)\s*\{\s*:\|:&\s*\};:").unwrap();
        if let Some(caps) = forkbomb_regex.captures(code) {
            matched_patterns.push(caps[0].to_string());
            highest_level = DangerLevel::Critical;
            reasons.push("Bash fork bomb denial of service pattern detected".to_string());
        }

        // 7. Wide permissions opening: chmod -R 777 /
        let chmod_regex = Regex::new(r"\bchmod\s+-[a-z0-9]*r[a-z0-9]*\s+(?:0?777)\s+(?:/|/\*)").unwrap();
        if let Some(caps) = chmod_regex.captures(code) {
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

    /// Records an approval vote from a peer. Returns true if this approval made it reach consensus.
    pub fn add_approval(&mut self, approver_name: String) -> bool {
        self.approvers.insert(approver_name);
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
    }
}
