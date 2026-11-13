use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::LazyLock;

pub const REDACTED: &str = "[REDACTED]";

static NONCE_COUNTER: AtomicU32 = AtomicU32::new(0);

static VAULT_INTERPOLATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*(?:secrets?)[.:]([a-zA-Z0-9_./:#=-]+)\s*\}\}").unwrap()
});

static FRONTMATTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^---\s*\n(.*?)\n---").unwrap()
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretSource {
    Env,
    EnvVault(PathBuf),
    Keyring,
    Pass,
    OnePassword,
    HashiCorpVault,
    AwsSecretsManager,
    EncryptedFrontmatter,
    Static(String),
}

#[derive(Debug, Clone)]
pub struct SecretVault {
    /// In-memory cache of secret keys to resolved values
    secrets: HashMap<String, String>,
    /// Set of sensitive secret values that must be masked from output and logs
    sensitive_values: HashSet<String>,
    /// Search paths for .env.vault or .env
    workspace_root: Option<PathBuf>,
}

impl Default for SecretVault {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretVault {
    pub fn new() -> Self {
        let mut vault = Self {
            secrets: HashMap::new(),
            sensitive_values: HashSet::new(),
            workspace_root: None,
        };
        vault.load_env_vault_from_dir(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        vault
    }

    pub fn with_workspace(workspace_dir: &Path) -> Self {
        let mut vault = Self {
            secrets: HashMap::new(),
            sensitive_values: HashSet::new(),
            workspace_root: Some(workspace_dir.to_path_buf()),
        };
        vault.load_env_vault_from_dir(workspace_dir);
        vault
    }

    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// Explicitly registers a secret key-value pair into the vault
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let k = key.into();
        let v = value.into();
        if v.len() >= 3 {
            self.sensitive_values.insert(v.clone());
        }
        self.secrets.insert(k, v);
    }

    /// Loads `.env.vault` or `.env` file from a directory if it exists
    pub fn load_env_vault_from_dir(&mut self, dir: &Path) {
        let env_vault = dir.join(".env.vault");
        if env_vault.is_file()
            && let Ok(content) = std::fs::read_to_string(&env_vault)
        {
            self.parse_and_load_env_content(&content);
        }

        let env_file = dir.join(".env");
        if env_file.is_file()
            && let Ok(content) = std::fs::read_to_string(&env_file)
        {
            self.parse_and_load_env_content(&content);
        }
    }

    /// Parses key=value pairs from .env or .env.vault content
    pub fn parse_and_load_env_content(&mut self, content: &str) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let clean_key = k.trim().to_string();
                let clean_val = v.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
                if !clean_key.is_empty() {
                    self.insert(clean_key, clean_val);
                }
            }
        }
    }

    /// Resolves a secret token (e.g. `API_KEY`, `aws:secret_id`, `op://vault/item/field`, etc.)
    pub fn resolve(&mut self, key_expr: &str) -> Option<String> {
        let key_clean = key_expr.trim();

        // 1. Direct cache check
        if let Some(val) = self.secrets.get(key_clean) {
            return Some(val.clone());
        }

        // 2. Environment variable check
        if let Ok(val) = std::env::var(key_clean) {
            self.insert(key_clean, val.clone());
            return Some(val);
        }

        // 3. CLI Agent / Resolver dispatch based on URI prefix
        if let Some(rest) = key_clean.strip_prefix("env:") {
            if let Ok(val) = std::env::var(rest) {
                self.insert(key_clean, val.clone());
                return Some(val);
            }
        } else if let Some(rest) = key_clean.strip_prefix("pass:") {
            if let Some(val) = Self::resolve_pass_cli(rest) {
                self.insert(key_clean, val.clone());
                return Some(val);
            }
        } else if key_clean.starts_with("op://") || key_clean.starts_with("op:") {
            let path = key_clean.strip_prefix("op:").unwrap_or(key_clean);
            if let Some(val) = Self::resolve_1password_cli(path) {
                self.insert(key_clean, val.clone());
                return Some(val);
            }
        } else if let Some(rest) = key_clean.strip_prefix("vault:") {
            if let Some(val) = Self::resolve_hashicorp_vault_cli(rest) {
                self.insert(key_clean, val.clone());
                return Some(val);
            }
        } else if let Some(rest) = key_clean.strip_prefix("aws:") {
            if let Some(val) = Self::resolve_aws_secrets_cli(rest) {
                self.insert(key_clean, val.clone());
                return Some(val);
            }
        } else if let Some(rest) = key_clean.strip_prefix("keyring:")
            && let Some(val) = Self::resolve_keyring_cli(rest)
        {
            self.insert(key_clean, val.clone());
            return Some(val);
        }

        None
    }

    /// CLI agent passthrough: 1Password (`op read ...`)
    fn resolve_1password_cli(ref_str: &str) -> Option<String> {
        let output = Command::new("op").args(["read", ref_str]).output().ok()?;
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        None
    }

    /// CLI agent passthrough: HashiCorp Vault (`vault kv get -field=... path`)
    fn resolve_hashicorp_vault_cli(spec: &str) -> Option<String> {
        let (path, field) = if let Some((p, f)) = spec.split_once('#') {
            (p, Some(f))
        } else {
            (spec, None)
        };

        let mut cmd = Command::new("vault");
        cmd.args(["kv", "get"]);
        if let Some(f) = field {
            cmd.arg(format!("-field={}", f));
        }
        cmd.arg(path);

        let output = cmd.output().ok()?;
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        None
    }

    /// CLI agent passthrough: AWS Secrets Manager (`aws secretsmanager get-secret-value --secret-id ...`)
    fn resolve_aws_secrets_cli(spec: &str) -> Option<String> {
        let (secret_id, field) = if let Some((id, f)) = spec.split_once('#') {
            (id, Some(f))
        } else {
            (spec, None)
        };

        let output = Command::new("aws")
            .args(["secretsmanager", "get-secret-value", "--secret-id", secret_id, "--query", "SecretString", "--output", "text"])
            .output()
            .ok()?;

        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(f) = field {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&s)
                    && let Some(val) = json.get(f).and_then(|v| v.as_str())
                {
                    return Some(val.to_string());
                }
            } else if !s.is_empty() {
                return Some(s);
            }
        }
        None
    }

    /// CLI agent passthrough: pass (`pass show <path>`)
    fn resolve_pass_cli(path: &str) -> Option<String> {
        let output = Command::new("pass").args(["show", path]).output().ok()?;
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).lines().next()?.trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        None
    }

    /// System keychain passthrough: security (macOS) or secret-tool (Linux)
    fn resolve_keyring_cli(spec: &str) -> Option<String> {
        let (service, account) = spec.split_once('/')?;
        #[cfg(target_os = "macos")]
        {
            let output = Command::new("security")
                .args(["find-generic-password", "-s", service, "-a", account, "-w"])
                .output()
                .ok()?;
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("secret-tool")
                .args(["lookup", "service", service, "account", account])
                .output()
                .ok()?;
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }

    /// Encrypts a key-value dictionary using ChaCha20-Poly1305 with a passphrase
    pub fn encrypt_secrets_payload(
        secrets_map: &HashMap<String, String>,
        passphrase: &str,
    ) -> anyhow::Result<(String, String)> {
        let json_data = serde_json::to_string(secrets_map)?;

        // Derive 32-byte key from passphrase using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(passphrase.as_bytes());
        let key_bytes = hasher.finalize();
        let key = Key::from(key_bytes);

        // Generate 12-byte monotonic/timestamped nonce to prevent nonce reuse
        let mut nonce_bytes = [0u8; 12];
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(1234567890);
        nonce_bytes[..8].copy_from_slice(&now.to_le_bytes());
        let counter = NONCE_COUNTER.fetch_add(1, Ordering::Relaxed);
        nonce_bytes[8..12].copy_from_slice(&counter.to_le_bytes());

        let cipher = ChaCha20Poly1305::new(&key);
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = cipher
            .encrypt(&nonce, json_data.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        let nonce_hex = hex::encode(nonce_bytes);
        let ciphertext_hex = hex::encode(ciphertext);

        Ok((nonce_hex, ciphertext_hex))
    }

    /// Decrypts a ChaCha20-Poly1305 encrypted payload using a passphrase
    pub fn decrypt_secrets_payload(
        nonce_hex: &str,
        ciphertext_hex: &str,
        passphrase: &str,
    ) -> anyhow::Result<HashMap<String, String>> {
        let nonce_bytes = hex::decode(nonce_hex)?;
        let ciphertext = hex::decode(ciphertext_hex)?;

        if nonce_bytes.len() != 12 {
            return Err(anyhow::anyhow!("Invalid nonce length: expected 12 bytes"));
        }

        let mut hasher = Sha256::new();
        hasher.update(passphrase.as_bytes());
        let key_bytes = hasher.finalize();
        let key = Key::from(key_bytes);

        let cipher = ChaCha20Poly1305::new(&key);
        let nonce = Nonce::from(
            <[u8; 12]>::try_from(nonce_bytes.as_slice())
                .map_err(|_| anyhow::anyhow!("Invalid nonce slice"))?,
        );

        let decrypted = cipher
            .decrypt(&nonce, ciphertext.as_ref())
            .map_err(|e| anyhow::anyhow!("Decryption failed (invalid passphrase or corrupted data): {}", e))?;

        let json_str = String::from_utf8(decrypted)?;
        let map: HashMap<String, String> = serde_json::from_str(&json_str)?;

        Ok(map)
    }

    /// Loads encrypted secrets from document frontmatter if present
    pub fn load_from_frontmatter(&mut self, content: &str, passphrase: &str) -> anyhow::Result<usize> {
        if let Some(caps) = FRONTMATTER_RE.captures(content)
            && let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&caps[1])
            && let Some(sec_val) = yaml.get("secrets")
        {
            let nonce_opt = sec_val.get("nonce").and_then(|v| v.as_str());
            let cipher_opt = sec_val.get("ciphertext").or_else(|| sec_val.get("encrypted")).and_then(|v| v.as_str());

            if let (Some(nonce_hex), Some(cipher_hex)) = (nonce_opt, cipher_opt) {
                let decrypted = Self::decrypt_secrets_payload(nonce_hex, cipher_hex, passphrase)?;
                let count = decrypted.len();
                for (k, v) in decrypted {
                    self.insert(k, v);
                }
                return Ok(count);
            }
        }
        Ok(0)
    }

    /// Interpolates secrets in a command string without persisting to disk.
    /// Syntax: `{{secret.VAULT_PATH}}` or `{{secrets.AWS_SECRET_KEY}}`
    pub fn interpolate_secrets(&mut self, code: &str) -> String {
        self.interpolate_secrets_with_fallback(code, None)
    }

    /// Interpolates secrets in a command string, checking an optional fallback map (e.g. runner env_vars).
    pub fn interpolate_secrets_with_fallback(&mut self, code: &str, fallback_envs: Option<&HashMap<String, String>>) -> String {
        VAULT_INTERPOLATE_RE.replace_all(code, |caps: &regex::Captures| {
            let key = &caps[1];
            if let Some(val) = self.resolve(key) {
                val
            } else if let Some(envs) = fallback_envs {
                if let Some(val) = envs.get(key) {
                    self.insert(key, val);
                    val.clone()
                } else {
                    caps[0].to_string()
                }
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
    }

    /// Masks all known sensitive values in the given text, replacing them with `[REDACTED]`.
    /// Zero-leak guarantee: values are sorted descending by length so longer substrings
    /// are replaced before shorter substrings, preventing partial-leak exposure.
    pub fn mask(&self, text: &str) -> String {
        let mut masked = text.to_string();
        let mut values: Vec<&String> = self.sensitive_values.iter().filter(|v| v.len() >= 3).collect();
        values.sort_by_key(|b| std::cmp::Reverse(b.len()));

        for secret_val in values {
            if masked.contains(secret_val.as_str()) {
                masked = masked.replace(secret_val.as_str(), REDACTED);
            }
        }
        masked
    }

    /// Registers a sensitive value so it gets masked in output logs and streams
    pub fn register_sensitive_value(&mut self, value: &str) {
        if value.len() >= 3 {
            self.sensitive_values.insert(value.to_string());
        }
    }

    /// Returns the count of registered secrets
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_interpolation_and_masking() {
        let mut vault = SecretVault::new();
        vault.insert("AWS_SECRET_KEY", "AKIA-SUPER-SECRET-XYZ-12345");
        vault.insert("DB_PASSWORD", "postgresP@ssw0rd999");

        let script = "export AWS_KEY='{{secret.AWS_SECRET_KEY}}'; psql -U admin -W '{{secrets.DB_PASSWORD}}'";
        let interpolated = vault.interpolate_secrets(script);

        assert_eq!(
            interpolated,
            "export AWS_KEY='AKIA-SUPER-SECRET-XYZ-12345'; psql -U admin -W 'postgresP@ssw0rd999'"
        );

        // Verify output masking
        let simulated_output = "Connected using password postgresP@ssw0rd999, key is AKIA-SUPER-SECRET-XYZ-12345.".to_string();
        let masked = vault.mask(&simulated_output);
        assert_eq!(masked, "Connected using password [REDACTED], key is [REDACTED].");
        assert!(!masked.contains("postgresP@ssw0rd999"));
        assert!(!masked.contains("AKIA-SUPER-SECRET-XYZ-12345"));
    }

    #[test]
    fn test_env_vault_file_parsing() {
        let mut vault = SecretVault::new();
        let content = r#"
# Production Vault
PROD_API_KEY="sk_live_998877665544"
DATABASE_URL=postgres://user:secret123@db.prod.internal:5432/main
"#;
        vault.parse_and_load_env_content(content);
        assert_eq!(vault.resolve("PROD_API_KEY"), Some("sk_live_998877665544".to_string()));
        assert_eq!(vault.resolve("DATABASE_URL"), Some("postgres://user:secret123@db.prod.internal:5432/main".to_string()));
    }

    #[test]
    fn test_chacha20poly1305_payload_encryption_and_decryption() {
        let mut map = HashMap::new();
        map.insert("STRIPE_KEY".to_string(), "sk_live_secret_stripe_token".to_string());
        map.insert("DATABASE_PASS".to_string(), "CorrectHorseBatteryStaple".to_string());

        let passphrase = "my-secure-master-passphrase";
        let (nonce_hex, cipher_hex) = SecretVault::encrypt_secrets_payload(&map, passphrase).unwrap();

        let decrypted = SecretVault::decrypt_secrets_payload(&nonce_hex, &cipher_hex, passphrase).unwrap();
        assert_eq!(decrypted.get("STRIPE_KEY"), Some(&"sk_live_secret_stripe_token".to_string()));
        assert_eq!(decrypted.get("DATABASE_PASS"), Some(&"CorrectHorseBatteryStaple".to_string()));

        // Wrong passphrase should fail decryption
        assert!(SecretVault::decrypt_secrets_payload(&nonce_hex, &cipher_hex, "wrong-passphrase").is_err());
    }

    #[test]
    fn test_frontmatter_encrypted_secrets_loading() {
        let mut map = HashMap::new();
        map.insert("DEPLOY_TOKEN".to_string(), "deploy_token_abc_xyz".to_string());
        let (nonce_hex, cipher_hex) = SecretVault::encrypt_secrets_payload(&map, "pass123").unwrap();

        let doc = format!(
            "---\ntitle: Production Runbook\nsecrets:\n  cipher: chacha20-poly1305\n  nonce: {}\n  ciphertext: {}\n---\n\n# Runbook\n",
            nonce_hex, cipher_hex
        );

        let mut vault = SecretVault::new();
        let loaded = vault.load_from_frontmatter(&doc, "pass123").unwrap();
        assert_eq!(loaded, 1);
        assert_eq!(vault.resolve("DEPLOY_TOKEN"), Some("deploy_token_abc_xyz".to_string()));
    }

    #[test]
    fn test_zero_leak_substring_masking() {
        let mut vault = SecretVault::new();
        // Insert both a long secret and a substring that occurs within it
        vault.insert("SHORT_TOKEN", "pass");
        vault.insert("LONG_TOKEN", "password_12345_secure");

        let text = "User logged in with password_12345_secure and pass.";
        let masked = vault.mask(text);
        // The long token should not be partially masked into "[REDACTED]word_12345_secure"
        assert_eq!(masked, "User logged in with [REDACTED] and [REDACTED].");
        assert!(!masked.contains("password_12345_secure"));
        assert!(!masked.contains("word_12345_secure"));
    }

    #[test]
    fn test_unique_nonce_generation() {
        let mut map = HashMap::new();
        map.insert("KEY".to_string(), "VALUE".to_string());

        let (nonce1, _) = SecretVault::encrypt_secrets_payload(&map, "pass").unwrap();
        let (nonce2, _) = SecretVault::encrypt_secrets_payload(&map, "pass").unwrap();
        assert_ne!(nonce1, nonce2, "Consecutive nonces must be strictly distinct");
    }
}
