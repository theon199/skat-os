use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub execution_id: String,
    pub doc_path: String,
    pub block_id: String,
    pub collaborator: String,
    pub target_hosts: Vec<String>,
    pub command: String,
    pub params: HashMap<String, String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub output_preview: String,
    pub prev_hash: String,
    pub hash: String,
}

impl AuditEntry {
    /// Computes the SHA-256 hash of this audit entry given its fields and previous hash
    pub fn compute_hash(
        prev_hash: &str,
        timestamp: &str,
        execution_id: &str,
        doc_path: &str,
        block_id: &str,
        collaborator: &str,
        target_hosts: &[String],
        command: &str,
        params: &HashMap<String, String>,
        exit_code: i32,
        duration_ms: u64,
        output_preview: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(b"|");
        hasher.update(timestamp.as_bytes());
        hasher.update(b"|");
        hasher.update(execution_id.as_bytes());
        hasher.update(b"|");
        hasher.update(doc_path.as_bytes());
        hasher.update(b"|");
        hasher.update(block_id.as_bytes());
        hasher.update(b"|");
        hasher.update(collaborator.as_bytes());
        hasher.update(b"|");
        hasher.update(target_hosts.join(",").as_bytes());
        hasher.update(b"|");
        hasher.update(command.as_bytes());
        hasher.update(b"|");

        // Sorted params for deterministic hashing
        let mut sorted_params: Vec<(&String, &String)> = params.iter().collect();
        sorted_params.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted_params {
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b";");
        }

        hasher.update(b"|");
        hasher.update(exit_code.to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(duration_ms.to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(output_preview.as_bytes());

        hex::encode(hasher.finalize())
    }
}

pub struct AuditRecorder {
    pub log_path: PathBuf,
    pub last_hash: String,
}

impl AuditRecorder {
    pub fn new(root_dir: &Path) -> Result<Self> {
        let skat_dir = root_dir.join(".skat");
        create_dir_all(&skat_dir).with_context(|| format!("Failed to create directory {:?}", skat_dir))?;
        let log_path = skat_dir.join("audit.log");

        let last_hash = Self::read_last_hash(&log_path).unwrap_or_else(|| GENESIS_HASH.to_string());

        Ok(Self { log_path, last_hash })
    }

    /// Reads the last hash from the log file, or None if empty
    pub fn read_last_hash(log_path: &Path) -> Option<String> {
        if !log_path.exists() {
            return None;
        }

        let file = File::open(log_path).ok()?;
        let reader = BufReader::new(file);
        let mut last_valid_hash = None;

        for line in reader.lines().flatten() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<AuditEntry>(trimmed) {
                last_valid_hash = Some(entry.hash);
            }
        }

        last_valid_hash
    }

    /// Records an execution in the immutable audit log journal
    pub fn record(
        &mut self,
        doc_path: &str,
        block_id: &str,
        collaborator: &str,
        target_hosts: Vec<String>,
        command: &str,
        params: HashMap<String, String>,
        exit_code: i32,
        duration_ms: u64,
        output_preview: &str,
    ) -> Result<AuditEntry> {
        let timestamp: DateTime<Utc> = Utc::now();
        let ts_str = timestamp.to_rfc3339();

        let execution_id = format!(
            "exec_{}_{:x}",
            timestamp.timestamp(),
            fastrand_hex()
        );

        let prev_hash = self.last_hash.clone();
        let hash = AuditEntry::compute_hash(
            &prev_hash,
            &ts_str,
            &execution_id,
            doc_path,
            block_id,
            collaborator,
            &target_hosts,
            command,
            &params,
            exit_code,
            duration_ms,
            output_preview,
        );

        let entry = AuditEntry {
            timestamp: ts_str,
            execution_id,
            doc_path: doc_path.to_string(),
            block_id: block_id.to_string(),
            collaborator: collaborator.to_string(),
            target_hosts,
            command: command.to_string(),
            params,
            exit_code,
            duration_ms,
            output_preview: output_preview.chars().take(500).collect(),
            prev_hash,
            hash: hash.clone(),
        };

        // Append JSON line to log file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .with_context(|| format!("Failed to open audit log at {:?}", self.log_path))?;

        let json_line = serde_json::to_string(&entry)?;
        writeln!(file, "{}", json_line)?;
        file.flush()?;

        self.last_hash = hash;
        Ok(entry)
    }

    /// Loads all audit log entries from disk in chronological order
    pub fn load_entries(&self) -> Result<Vec<AuditEntry>> {
        Self::load_entries_from_path(&self.log_path)
    }

    pub fn load_entries_from_path(log_path: &Path) -> Result<Vec<AuditEntry>> {
        if !log_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(log_path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line_str = line?;
            let trimmed = line_str.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<AuditEntry>(trimmed) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Verifies the cryptographic SHA-256 hash chain integrity of the audit log
    pub fn verify_integrity(&self) -> Result<bool> {
        Self::verify_file_integrity(&self.log_path)
    }

    pub fn verify_file_integrity(log_path: &Path) -> Result<bool> {
        let entries = Self::load_entries_from_path(log_path)?;
        let mut expected_prev_hash = GENESIS_HASH.to_string();

        for entry in &entries {
            if entry.prev_hash != expected_prev_hash {
                return Ok(false);
            }

            let computed = AuditEntry::compute_hash(
                &entry.prev_hash,
                &entry.timestamp,
                &entry.execution_id,
                &entry.doc_path,
                &entry.block_id,
                &entry.collaborator,
                &entry.target_hosts,
                &entry.command,
                &entry.params,
                entry.exit_code,
                entry.duration_ms,
                &entry.output_preview,
            );

            if computed != entry.hash {
                return Ok(false);
            }

            expected_prev_hash = entry.hash.clone();
        }

        Ok(true)
    }
}

fn fastrand_hex() -> u64 {
    let now = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    now ^ 0x58_53_4B_41_54_4F_53_37
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_audit_record_and_hash_chain_integrity() {
        let tmp = tempdir().unwrap();
        let mut recorder = AuditRecorder::new(tmp.path()).unwrap();

        let mut params = HashMap::new();
        params.insert("CLUSTER".to_string(), "prod-us-east".to_string());

        let e1 = recorder
            .record(
                "runbook.md",
                "check_cluster",
                "Alice",
                vec!["10.0.0.1".to_string()],
                "kubectl get nodes",
                params.clone(),
                0,
                142,
                "Ready node-1\nReady node-2",
            )
            .unwrap();

        assert_eq!(e1.prev_hash, GENESIS_HASH);
        assert!(!e1.hash.is_empty());

        let e2 = recorder
            .record(
                "runbook.md",
                "deploy_app",
                "Bob",
                vec!["@web_servers".to_string()],
                "systemctl restart app",
                params,
                0,
                820,
                "Restarted successfully",
            )
            .unwrap();

        assert_eq!(e2.prev_hash, e1.hash);

        // Verify valid chain
        assert!(recorder.verify_integrity().unwrap());

        let entries = recorder.load_entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].collaborator, "Alice");
        assert_eq!(entries[1].collaborator, "Bob");
    }

    #[test]
    fn test_audit_detects_tampering() {
        let tmp = tempdir().unwrap();
        let mut recorder = AuditRecorder::new(tmp.path()).unwrap();

        recorder
            .record(
                "doc.md",
                "b1",
                "Alice",
                vec!["localhost".to_string()],
                "echo safe",
                HashMap::new(),
                0,
                10,
                "safe",
            )
            .unwrap();

        assert!(recorder.verify_integrity().unwrap());

        // Tamper with the file
        let log_content = std::fs::read_to_string(&recorder.log_path).unwrap();
        let tampered = log_content.replace("echo safe", "rm -rf /");
        std::fs::write(&recorder.log_path, tampered).unwrap();

        // Verification should now fail!
        assert!(!recorder.verify_integrity().unwrap());
    }
}
