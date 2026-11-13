use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, LazyLock};
use std::time::Duration;

use crate::editor::TextBuffer;
use crate::runner::{ExecutionResult, StreamingHandle};

static FLEET_HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^===\s*\[([^\]]+)\]\s*===$").unwrap()
});

/// Global default SSH session pool for persistent connections across the application
pub fn global_ssh_pool() -> &'static SshSessionPool {
    static POOL: OnceLock<SshSessionPool> = OnceLock::new();
    POOL.get_or_init(SshSessionPool::new)
}

/// Represents an SSH target endpoint with user, host, port, and optional password
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SshEndpoint {
    pub user: Option<String>,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
}

impl SshEndpoint {
    pub fn new(host: String) -> Self {
        Self {
            user: None,
            host,
            port: 22,
            password: None,
        }
    }

    /// Parses a target URI or string into an SshEndpoint
    /// Examples:
    /// - "ssh://ubuntu@10.0.0.12:22"
    /// - "ssh://app:secret@10.0.1.5"
    /// - "ssh://bastion.corp.com"
    /// - "ubuntu@10.0.0.12:22"
    /// - "ubuntu@10.0.0.12"
    /// - "10.0.0.12:2222"
    /// - "10.0.0.12"
    /// - "ssh://[::1]:2222"
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            anyhow::bail!("SSH target cannot be empty");
        }

        let without_scheme = if let Some(stripped) = trimmed.strip_prefix("ssh://") {
            stripped
        } else {
            trimmed
        };

        // Check for user / password before '@'
        let (auth_part, host_part) = if let Some(at_idx) = without_scheme.find('@') {
            (Some(&without_scheme[..at_idx]), &without_scheme[at_idx + 1..])
        } else {
            (None, without_scheme)
        };

        let mut user = None;
        let mut password = None;
        if let Some(auth) = auth_part {
            if let Some(colon_idx) = auth.find(':') {
                let u = auth[..colon_idx].trim();
                if !u.is_empty() {
                    user = Some(u.to_string());
                }
                password = Some(auth[colon_idx + 1..].to_string());
            } else {
                let u = auth.trim();
                if !u.is_empty() {
                    user = Some(u.to_string());
                }
            }
        }

        // Parse host and port, handling IPv6 bracketed hosts like [::1]:22
        let (host, port) = if let Some(colon_idx) = host_part.rfind(':') {
            let h = &host_part[..colon_idx];
            let p_str = &host_part[colon_idx + 1..];
            if let Ok(p) = p_str.parse::<u16>() {
                let clean_h = h.trim_matches(|c| c == '[' || c == ']');
                (clean_h.to_string(), p)
            } else {
                let clean_h = host_part.trim_matches(|c| c == '[' || c == ']');
                (clean_h.to_string(), 22)
            }
        } else {
            let clean_h = host_part.trim_matches(|c| c == '[' || c == ']');
            (clean_h.to_string(), 22)
        };

        if host.is_empty() {
            anyhow::bail!("SSH host cannot be empty");
        }

        Ok(Self {
            user,
            host,
            port,
            password,
        })
    }

    pub fn to_uri(&self) -> String {
        let user_part = match (&self.user, &self.password) {
            (Some(u), Some(p)) => format!("{}:{}@", u, p),
            (Some(u), None) => format!("{}@", u),
            (None, _) => String::new(),
        };
        format!("ssh://{}{}:{}", user_part, self.host, self.port)
    }

    pub fn display_badge(&self) -> String {
        if self.port == 22 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// Host group resolution: frontmatter and hosts.yaml
pub struct HostRegistry;

impl HostRegistry {
    /// Extracts host groups defined in YAML frontmatter at the top of the markdown buffer
    pub fn parse_frontmatter(buffer: &TextBuffer) -> HashMap<String, Vec<String>> {
        let mut groups = HashMap::new();
        let total = buffer.line_count();
        if total < 2 {
            return groups;
        }

        let first_line = buffer.line_to_string(0);
        if first_line.trim() != "---" {
            return groups;
        }

        let mut end_line = None;
        for i in 1..total {
            if buffer.line_to_string(i).trim() == "---" {
                end_line = Some(i);
                break;
            }
        }

        let end = match end_line {
            Some(e) => e,
            None => return groups,
        };

        let mut yaml_lines = Vec::new();
        for i in 1..end {
            yaml_lines.push(buffer.line_to_string(i));
        }
        let yaml_text = yaml_lines.join("\n");

        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&yaml_text) {
            Self::extract_groups_from_yaml_value(&value, &mut groups);
        }

        groups
    }

    /// Extracts host groups defined in hosts.yaml or hosts.yml in the root dir
    pub fn parse_hosts_file(root_dir: &Path) -> HashMap<String, Vec<String>> {
        let mut groups = HashMap::new();
        let yaml_path = root_dir.join("hosts.yaml");
        let yml_path = root_dir.join("hosts.yml");

        let target_path = if yaml_path.exists() {
            Some(yaml_path)
        } else if yml_path.exists() {
            Some(yml_path)
        } else {
            None
        };

        if let Some(path) = target_path
            && let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content)
        {
            Self::extract_groups_from_yaml_value(&value, &mut groups);
        }

        groups
    }

    fn extract_groups_from_yaml_value(value: &serde_yaml::Value, groups: &mut HashMap<String, Vec<String>>) {
        if let Some(mapping) = value.as_mapping() {
            // Check if there is a top-level "hosts" or "groups" key
            for (k, v) in mapping {
                let key_str = match k.as_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };

                if key_str == "hosts" || key_str == "groups" {
                    if let Some(sub_mapping) = v.as_mapping() {
                        for (sub_k, sub_v) in sub_mapping {
                            if let Some(sub_key_str) = sub_k.as_str() {
                                let hosts = Self::yaml_value_to_string_list(sub_v);
                                if !hosts.is_empty() {
                                    groups.insert(sub_key_str.to_string(), hosts);
                                }
                            }
                        }
                    }
                } else {
                    let hosts = Self::yaml_value_to_string_list(v);
                    if !hosts.is_empty() {
                        groups.insert(key_str, hosts);
                    }
                }
            }
        }
    }

    fn yaml_value_to_string_list(v: &serde_yaml::Value) -> Vec<String> {
        let mut list = Vec::new();
        if let Some(seq) = v.as_sequence() {
            for item in seq {
                if let Some(s) = item.as_str() {
                    list.push(s.to_string());
                } else if let Some(n) = item.as_i64() {
                    list.push(n.to_string());
                }
            }
        } else if let Some(s) = v.as_str() {
            if s.contains(',') {
                for part in s.split(',') {
                    let trimmed = part.trim();
                    if !trimmed.is_empty() {
                        list.push(trimmed.to_string());
                    }
                }
            } else {
                list.push(s.to_string());
            }
        }
        list
    }

    /// Resolves target string (either "@group_name" or a direct host string) to a list of host strings
    pub fn resolve_target(
        target_str: &str,
        buffer: Option<&TextBuffer>,
        root_dir: &Path,
    ) -> Result<Vec<String>> {
        let trimmed = target_str.trim();
        if let Some(group_name) = trimmed.strip_prefix('@') {
            // 1. Check frontmatter
            if let Some(buf) = buffer {
                let fm_groups = Self::parse_frontmatter(buf);
                if let Some(hosts) = fm_groups.get(group_name)
                    && !hosts.is_empty()
                {
                    return Ok(hosts.clone());
                }
            }

            // 2. Check hosts.yaml in root_dir
            let file_groups = Self::parse_hosts_file(root_dir);
            if let Some(hosts) = file_groups.get(group_name)
                && !hosts.is_empty()
            {
                return Ok(hosts.clone());
            }

            anyhow::bail!("Unknown host group '@{}' (not defined in frontmatter or hosts.yaml)", group_name);
        }

        // Direct host target
        Ok(vec![trimmed.to_string()])
    }
}

/// Output of a single host execution in a fleet
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostOutput {
    pub host: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Aggregated output of multi-host fleet execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetOutput {
    pub group_name: String,
    pub hosts: Vec<HostOutput>,
    pub parallel: bool,
    pub selected_tab: usize,
}

impl FleetOutput {
    pub fn new(group_name: String, hosts: Vec<HostOutput>, parallel: bool) -> Self {
        Self {
            group_name,
            hosts,
            parallel,
            selected_tab: 0,
        }
    }

    /// Formats per-host status badges: e.g. `[10.0.0.1: ✓ exit 0] [10.0.0.2: ✖ exit 1]`
    pub fn status_badges(&self) -> String {
        let mut badges = Vec::new();
        for h in &self.hosts {
            let symbol = if h.exit_code == 0 { "✓" } else { "✖" };
            badges.push(format!("[{}: {} exit {}]", h.host, symbol, h.exit_code));
        }
        badges.join(" ")
    }

    /// Returns list of hosts that failed (exit_code != 0)
    pub fn failed_hosts(&self) -> Vec<String> {
        self.hosts
            .iter()
            .filter(|h| h.exit_code != 0)
            .map(|h| h.host.clone())
            .collect()
    }

    /// Returns true if all hosts succeeded
    pub fn is_success(&self) -> bool {
        self.hosts.iter().all(|h| h.exit_code == 0)
    }

    /// Returns overall exit code (0 if all succeeded, or first failing exit code)
    pub fn overall_exit_code(&self) -> i32 {
        for h in &self.hosts {
            if h.exit_code != 0 {
                return h.exit_code;
            }
        }
        0
    }

    /// Generates full output text for the code block output section
    pub fn to_output_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.status_badges());
        out.push('\n');

        for h in &self.hosts {
            out.push_str(&format!("\n=== [{}] ===\n", h.host));
            if !h.stdout.is_empty() {
                out.push_str(&h.stdout);
                if !h.stdout.ends_with('\n') {
                    out.push('\n');
                }
            }
            if !h.stderr.is_empty() {
                out.push_str("--- stderr ---\n");
                out.push_str(&h.stderr);
                if !h.stderr.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        out
    }

    /// Attempts to parse FleetOutput from output block text
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        if !trimmed.contains("exit ") || !trimmed.contains("===") {
            return None;
        }

        let mut lines = trimmed.lines();
        let badge_line = lines.next()?;
        if !badge_line.starts_with('[') {
            return None;
        }

        let mut hosts = Vec::new();
        let mut current_host: Option<String> = None;
        let mut current_stdout = Vec::new();
        let mut current_stderr = Vec::new();
        let mut in_stderr = false;

        for line in lines {
            if let Some(caps) = FLEET_HEADER_RE.captures(line) {
                if let Some(host) = current_host.take() {
                    let out_str = current_stdout.join("\n");
                    let err_str = current_stderr.join("\n");
                    let exit_code = Self::extract_exit_code_from_badge(badge_line, &host);
                    hosts.push(HostOutput {
                        host,
                        stdout: out_str,
                        stderr: err_str,
                        exit_code,
                    });
                    current_stdout.clear();
                    current_stderr.clear();
                    in_stderr = false;
                }
                current_host = Some(caps[1].to_string());
            } else if current_host.is_some() {
                if line.trim() == "--- stderr ---" {
                    in_stderr = true;
                } else if in_stderr {
                    current_stderr.push(line);
                } else {
                    current_stdout.push(line);
                }
            }
        }

        if let Some(host) = current_host.take() {
            let out_str = current_stdout.join("\n");
            let err_str = current_stderr.join("\n");
            let exit_code = Self::extract_exit_code_from_badge(badge_line, &host);
            hosts.push(HostOutput {
                host,
                stdout: out_str,
                stderr: err_str,
                exit_code,
            });
        }

        if hosts.is_empty() {
            return None;
        }

        Some(Self {
            group_name: "fleet".to_string(),
            hosts,
            parallel: true,
            selected_tab: 0,
        })
    }

    fn extract_exit_code_from_badge(badge_line: &str, host: &str) -> i32 {
        let pattern = format!("[{}:", host);
        if let Some(idx) = badge_line.find(&pattern) {
            let rest = &badge_line[idx + pattern.len()..];
            if let Some(exit_idx) = rest.find("exit ") {
                let code_part = &rest[exit_idx + 5..];
                if let Some(end_bracket) = code_part.find(']')
                    && let Ok(code) = code_part[..end_bracket].trim().parse::<i32>()
                {
                    return code;
                }
            }
        }
        0
    }
}

/// Key for pooling SSH sessions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoolKey {
    pub user: Option<String>,
    pub host: String,
    pub port: u16,
    pub proxy: Option<String>,
}

pub type MockHandlerFn = Arc<dyn Fn(&str, &HashMap<String, String>) -> ExecutionResult + Send + Sync>;

/// Connection pool managing authenticated persistent SSH sessions and mock test handlers
#[derive(Clone)]
pub struct SshSessionPool {
    sessions: Arc<Mutex<HashMap<PoolKey, ssh2::Session>>>,
    mock_hosts: Arc<Mutex<HashMap<String, MockHandlerFn>>>,
}

impl Default for SshSessionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl SshSessionPool {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            mock_hosts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Registers a mock host handler for unit / integration testing
    pub fn register_mock_host<F>(&self, host: &str, handler: F)
    where
        F: Fn(&str, &HashMap<String, String>) -> ExecutionResult + Send + Sync + 'static,
    {
        let mut mocks = self.mock_hosts.lock().unwrap();
        mocks.insert(host.to_string(), Arc::new(handler));
    }

    /// Clears mock hosts
    pub fn clear_mock_hosts(&self) {
        let mut mocks = self.mock_hosts.lock().unwrap();
        mocks.clear();
    }

    /// Checks if a host is registered as a mock host
    pub fn is_mock_host(&self, host: &str) -> bool {
        let mocks = self.mock_hosts.lock().unwrap();
        mocks.contains_key(host)
    }

    /// Executes command against a mock host
    pub fn execute_mock(
        &self,
        host: &str,
        command: &str,
        env_vars: &HashMap<String, String>,
    ) -> Option<ExecutionResult> {
        let mocks = self.mock_hosts.lock().unwrap();
        let handler = mocks.get(host)?.clone();
        drop(mocks);
        Some(handler(command, env_vars))
    }

    /// Evicts an endpoint session from the pool (e.g. after network failure or disconnection)
    pub fn evict(&self, endpoint: &SshEndpoint, proxy: Option<&SshEndpoint>) {
        let key = PoolKey {
            user: endpoint.user.clone(),
            host: endpoint.host.clone(),
            port: endpoint.port,
            proxy: proxy.map(|p| p.to_uri()),
        };
        let mut pool = self.sessions.lock().unwrap();
        pool.remove(&key);
    }

    /// Returns an existing authenticated session or establishes and authenticates a new one
    pub fn get_or_connect(&self, endpoint: &SshEndpoint, proxy: Option<&SshEndpoint>) -> Result<ssh2::Session> {
        let key = PoolKey {
            user: endpoint.user.clone(),
            host: endpoint.host.clone(),
            port: endpoint.port,
            proxy: proxy.map(|p| p.to_uri()),
        };

        // 1. Check existing session in pool
        {
            let pool = self.sessions.lock().unwrap();
            if let Some(session) = pool.get(&key)
                && session.authenticated()
            {
                return Ok(session.clone());
            }
        }

        // 2. Establish new connection
        let session = if let Some(proxy_ep) = proxy {
            // Tunnel through bastion jump host
            let bastion_session = self.get_or_connect(proxy_ep, None)
                .context(format!("Failed to connect to bastion jump host {}", proxy_ep.host))?;

            let channel = bastion_session
                .channel_direct_tcpip(&endpoint.host, endpoint.port, None)
                .context(format!("Failed to establish TCP tunnel through bastion to {}:{}", endpoint.host, endpoint.port))?;

            use std::os::unix::net::UnixStream;
            let (sock_local, sock_remote) = UnixStream::pair()
                .context("Failed to create local stream pair for bastion tunnel")?;

            let mut channel_reader = channel.clone();
            let mut channel_writer = channel.clone();
            let mut sock_r = sock_local.try_clone()?;
            let mut sock_w = sock_local;

            // Forward from sock to bastion channel
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match sock_r.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if channel_writer.write_all(&buf[..n]).is_err() || channel_writer.flush().is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // Forward from bastion channel to sock
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match channel_reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if sock_w.write_all(&buf[..n]).is_err() || sock_w.flush().is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            let mut target_session = ssh2::Session::new()
                .context("Failed to initialize target SSH session")?;
            target_session.set_tcp_stream(sock_remote);
            target_session.handshake()
                .context(format!("SSH handshake through bastion failed for {}:{}", endpoint.host, endpoint.port))?;

            Self::authenticate_session(&mut target_session, endpoint)?;
            target_session
        } else {
            // Direct TCP connection
            let addr = format!("{}:{}", endpoint.host, endpoint.port);
            let tcp = std::net::TcpStream::connect(&addr)
                .context(format!("Failed to connect TCP to {}", addr))?;
            tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
            tcp.set_write_timeout(Some(Duration::from_secs(30)))?;

            let mut session = ssh2::Session::new()
                .context("Failed to initialize SSH session")?;
            session.set_tcp_stream(tcp);
            session.handshake()
                .context(format!("SSH handshake failed for {}", addr))?;

            Self::authenticate_session(&mut session, endpoint)?;
            session
        };

        // 3. Store in pool
        {
            let mut pool = self.sessions.lock().unwrap();
            pool.insert(key, session.clone());
        }

        Ok(session)
    }

    /// Authenticates session using ssh-agent, standard keys, password, or none
    fn authenticate_session(session: &mut ssh2::Session, endpoint: &SshEndpoint) -> Result<()> {
        let username = endpoint.user.clone().unwrap_or_else(|| {
            std::env::var("USER").unwrap_or_else(|_| "root".to_string())
        });

        // 1. Password if provided in endpoint URI
        if let Some(pass) = &endpoint.password
            && session.userauth_password(&username, pass).is_ok()
            && session.authenticated()
        {
            return Ok(());
        }

        // 2. Password from SKAT_SSH_PASSWORD or SSH_PASSWORD environment variables
        if let Ok(pass) = std::env::var("SKAT_SSH_PASSWORD").or_else(|_| std::env::var("SSH_PASSWORD"))
            && session.userauth_password(&username, &pass).is_ok()
            && session.authenticated()
        {
            return Ok(());
        }

        // 3. SSH_AUTH_SOCK (ssh-agent automatic authentication)
        if std::env::var("SSH_AUTH_SOCK").is_ok()
            && let Ok(mut agent) = session.agent()
            && agent.connect().is_ok()
            && agent.list_identities().is_ok()
            && let Ok(identities) = agent.identities()
        {
            for identity in identities {
                if agent.userauth(&username, &identity).is_ok() && session.authenticated() {
                    return Ok(());
                }
            }
        }

        // 4. Standard key pairs (~/.ssh/id_ed25519, ~/.ssh/id_rsa, ~/.ssh/id_ecdsa)
        if let Ok(home) = std::env::var("HOME") {
            let key_names = ["id_ed25519", "id_rsa", "id_ecdsa"];
            for name in &key_names {
                let priv_path = PathBuf::from(&home).join(".ssh").join(name);
                if priv_path.exists() {
                    let pub_path = PathBuf::from(&home).join(".ssh").join(format!("{}.pub", name));
                    let pub_arg = if pub_path.exists() { Some(pub_path.as_path()) } else { None };
                    if session.userauth_pubkey_file(&username, pub_arg, &priv_path, None).is_ok()
                        && session.authenticated()
                    {
                        return Ok(());
                    }
                }
            }
        }

        if session.authenticated() {
            return Ok(());
        }

        anyhow::bail!(
            "SSH authentication failed for {}@{}:{} (tried ssh-agent, ~/.ssh keys, and password)",
            username,
            endpoint.host,
            endpoint.port
        )
    }
}

/// Executes a targeted code block on a remote host over SSH with interactive PTY and streaming
#[allow(clippy::too_many_arguments)]
pub async fn execute_remote_ssh_streaming<FChunk, FEnd>(
    pool: SshSessionPool,
    target: SshEndpoint,
    proxy: Option<SshEndpoint>,
    code: String,
    language: String,
    env_vars: HashMap<String, String>,
    mut on_chunk: FChunk,
    on_end: FEnd,
) -> Result<StreamingHandle>
where
    FChunk: FnMut(String) + Send + 'static,
    FEnd: FnOnce(ExecutionResult) + Send + 'static,
{
    // Check if this host is a registered mock host
    if pool.is_mock_host(&target.host) {
        let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let host_name = target.host.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = &mut cancel_rx => {
                    on_end(ExecutionResult {
                        stdout: String::new(),
                        stderr: "Remote execution interrupted".to_string(),
                        exit_code: 130,
                        timed_out: false,
                    });
                }
                _ = tokio::time::sleep(Duration::from_millis(15)) => {
                    // Check if stdin was received
                    while let Ok(_bytes) = stdin_rx.try_recv() {}

                    if let Some(res) = pool.execute_mock(&host_name, &code, &env_vars) {
                        if !res.stdout.is_empty() {
                            on_chunk(res.stdout.clone());
                        }
                        if !res.stderr.is_empty() {
                            on_chunk(res.stderr.clone());
                        }
                        on_end(res);
                    } else {
                        on_end(ExecutionResult {
                            stdout: String::new(),
                            stderr: format!("Unknown mock host {}", host_name),
                            exit_code: -1,
                            timed_out: false,
                        });
                    }
                }
            }
        });

        return Ok(StreamingHandle {
            stdin_tx,
            cancel_tx: Some(cancel_tx),
        });
    }

    // Connect or reuse pooled SSH session
    let session = pool.get_or_connect(&target, proxy.as_ref())?;

    // Open multiplexed channel and allocate PTY
    let mut channel = session.channel_session()
        .context("Failed to open SSH channel session")?;
    channel.request_pty("xterm-256color", None, Some((80, 24, 0, 0)))
        .context("Failed to request PTY on remote SSH channel")?;

    // Build shell command with environment variables
    let mut env_prefix = String::new();
    for (k, v) in &env_vars {
        let escaped_val = v.replace('\'', "'\\''");
        env_prefix.push_str(&format!("export {}='{}'; ", k, escaped_val));
    }

    let remote_cmd = match language.to_lowercase().as_str() {
        "python" | "py" | "python3" => {
            let escaped_code = code.replace('\'', "'\\''");
            format!("{}python3 -c '{}'", env_prefix, escaped_code)
        }
        "node" | "js" | "javascript" => {
            let escaped_code = code.replace('\'', "'\\''");
            format!("{}node -e '{}'", env_prefix, escaped_code)
        }
        "zsh" => {
            let escaped_code = code.replace('\'', "'\\''");
            format!("{}zsh -c '{}'", env_prefix, escaped_code)
        }
        "sh" => {
            let escaped_code = code.replace('\'', "'\\''");
            format!("{}sh -c '{}'", env_prefix, escaped_code)
        }
        _ => {
            let escaped_code = code.replace('\'', "'\\''");
            format!("{}bash -c '{}'", env_prefix, escaped_code)
        }
    };

    channel.exec(&remote_cmd)
        .context(format!("Failed to exec command on remote host {}", target.host))?;

    let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();

    let mut channel_writer = channel.clone();
    let mut channel_reader = channel.clone();

    // Reader thread
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match channel_reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if chunk_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = done_tx.send(());
    });

    // Stdin writer thread
    std::thread::spawn(move || {
        while let Some(bytes) = stdin_rx.blocking_recv() {
            if channel_writer.write_all(&bytes).is_err() || channel_writer.flush().is_err() {
                break;
            }
        }
    });

    // Supervision task
    tokio::spawn(async move {
        let mut full_output = Vec::new();
        let mut interrupted = false;
        let mut timed_out = false;
        let start_time = tokio::time::Instant::now();
        let timeout_duration = Duration::from_secs(60);

        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    interrupted = true;
                    let _ = channel.close();
                    break;
                }
                Some(bytes) = chunk_rx.recv() => {
                    full_output.extend_from_slice(&bytes);
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    on_chunk(text);
                }
                _ = &mut done_rx => {
                    break;
                }
                _ = tokio::time::sleep_until(start_time + timeout_duration) => {
                    timed_out = true;
                    let _ = channel.close();
                    break;
                }
            }
        }

        while let Ok(bytes) = chunk_rx.try_recv() {
            full_output.extend_from_slice(&bytes);
            let text = String::from_utf8_lossy(&bytes).to_string();
            on_chunk(text);
        }

        let _ = channel.send_eof();
        let exit_code = if interrupted {
            130
        } else if timed_out {
            -1
        } else {
            let _ = channel.wait_eof();
            let _ = channel.wait_close();
            channel.exit_status().unwrap_or(0)
        };

        let stdout = String::from_utf8_lossy(&full_output).to_string();
        let stderr = if timed_out {
            "Execution timed out after 60 seconds.".to_string()
        } else {
            String::new()
        };

        on_end(ExecutionResult {
            stdout,
            stderr,
            exit_code,
            timed_out,
        });
    });

    Ok(StreamingHandle {
        stdin_tx,
        cancel_tx: Some(cancel_tx),
    })
}

/// Executes a code block across multiple hosts in parallel or sequentially
#[allow(clippy::too_many_arguments)]
pub async fn execute_fleet_fanout<FChunk, FEnd>(
    pool: SshSessionPool,
    hosts: Vec<String>,
    group_name: String,
    proxy_endpoint: Option<SshEndpoint>,
    code: String,
    language: String,
    env_vars: HashMap<String, String>,
    parallel: bool,
    mut on_chunk: FChunk,
    on_end: FEnd,
) -> Result<StreamingHandle>
where
    FChunk: FnMut(String) + Send + 'static,
    FEnd: FnOnce(FleetOutput, ExecutionResult) + Send + 'static,
{
    let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

    let total_hosts = hosts.len();
    if total_hosts == 0 {
        let fleet = FleetOutput::new(group_name, Vec::new(), parallel);
        let res = ExecutionResult {
            stdout: "(No hosts in group)".to_string(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
        };
        on_end(fleet, res);
        return Ok(StreamingHandle {
            stdin_tx,
            cancel_tx: Some(cancel_tx),
        });
    }

    // Channels to coordinate fanout tasks
    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        let mut host_outputs: Vec<Option<HostOutput>> = vec![None; total_hosts];
        let mut stdin_sub_txs = Vec::new();
        let mut cancel_sub_txs = Vec::new();

        if parallel {
            // Concurrent execution across all hosts
            let mut join_set = tokio::task::JoinSet::new();

            for (idx, host_str) in hosts.iter().enumerate() {
                let pool_c = pool.clone();
                let host_str_c = host_str.clone();
                let proxy_c = proxy_endpoint.clone();
                let code_c = code.clone();
                let lang_c = language.clone();
                let env_c = env_vars.clone();
                let chunk_tx_c = chunk_tx.clone();

                let (sub_stdin_tx, mut sub_stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
                let (sub_cancel_tx, sub_cancel_rx) = tokio::sync::oneshot::channel::<()>();
                stdin_sub_txs.push(sub_stdin_tx);
                cancel_sub_txs.push(sub_cancel_tx);

                join_set.spawn(async move {
                    let ep = match SshEndpoint::parse(&host_str_c) {
                        Ok(e) => e,
                        Err(err) => {
                            return (
                                idx,
                                HostOutput {
                                    host: host_str_c,
                                    stdout: String::new(),
                                    stderr: format!("Target parse error: {}", err),
                                    exit_code: 1,
                                },
                            );
                        }
                    };

                    let badge = ep.display_badge();
                    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
                    let b_name = badge.clone();

                    let on_c = move |c: String| {
                        let _ = chunk_tx_c.send(format!("[{}] {}", b_name, c));
                    };

                    let handle_res = execute_remote_ssh_streaming(
                        pool_c,
                        ep.clone(),
                        proxy_c,
                        code_c,
                        lang_c,
                        env_c,
                        on_c,
                        move |res| {
                            let _ = res_tx.send(res);
                        },
                    )
                    .await;

                    match handle_res {
                        Ok(handle) => {
                            // Relay stdin
                            tokio::spawn(async move {
                                while let Some(b) = sub_stdin_rx.recv().await {
                                    if handle.stdin_tx.send(b).is_err() {
                                        break;
                                    }
                                }
                            });
                            // Relay cancel
                            if let Some(act_cancel) = handle.cancel_tx {
                                tokio::spawn(async move {
                                    if sub_cancel_rx.await.is_ok() {
                                        let _ = act_cancel.send(());
                                    }
                                });
                            }

                            let exec_res = res_rx.await.unwrap_or(ExecutionResult {
                                stdout: String::new(),
                                stderr: "Host task dropped".to_string(),
                                exit_code: -1,
                                timed_out: false,
                            });

                            (
                                idx,
                                HostOutput {
                                    host: badge,
                                    stdout: exec_res.stdout,
                                    stderr: exec_res.stderr,
                                    exit_code: exec_res.exit_code,
                                },
                            )
                        }
                        Err(e) => (
                            idx,
                            HostOutput {
                                host: badge,
                                stdout: String::new(),
                                stderr: format!("Connection error: {}", e),
                                exit_code: 1,
                            },
                        ),
                    }
                });
            }

            // Loop to handle incoming chunks, stdin, and cancel while waiting for all tasks
            let mut completed_tasks = 0;
            let mut was_cancelled = false;
            let start_time = tokio::time::Instant::now();
            let fanout_timeout = Duration::from_secs(120);

            while completed_tasks < total_hosts {
                tokio::select! {
                    _ = &mut cancel_rx, if !was_cancelled => {
                        was_cancelled = true;
                        for ctx in cancel_sub_txs.drain(..) {
                            let _ = ctx.send(());
                        }
                    }
                    Some(stdin_bytes) = stdin_rx.recv() => {
                        for stx in &stdin_sub_txs {
                            let _ = stx.send(stdin_bytes.clone());
                        }
                    }
                    Some(chunk) = chunk_rx.recv() => {
                        on_chunk(chunk);
                    }
                    Some(res) = join_set.join_next() => {
                        completed_tasks += 1;
                        match res {
                            Ok((idx, host_out)) => {
                                host_outputs[idx] = Some(host_out);
                            }
                            Err(join_err) => {
                                // Task panicked or was cancelled
                                for (i, slot) in host_outputs.iter_mut().enumerate() {
                                    if slot.is_none() {
                                        *slot = Some(HostOutput {
                                            host: hosts.get(i).cloned().unwrap_or_else(|| format!("host_{}", i)),
                                            stdout: String::new(),
                                            stderr: format!("Task execution failed: {}", join_err),
                                            exit_code: -1,
                                        });
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(start_time + fanout_timeout) => {
                        // Fan-out join timeout reached: cancel remaining subtasks
                        for ctx in cancel_sub_txs.drain(..) {
                            let _ = ctx.send(());
                        }
                        join_set.abort_all();
                        for (i, slot) in host_outputs.iter_mut().enumerate() {
                            if slot.is_none() {
                                *slot = Some(HostOutput {
                                    host: hosts.get(i).cloned().unwrap_or_else(|| format!("host_{}", i)),
                                    stdout: String::new(),
                                    stderr: "Fleet fan-out execution timed out after 120 seconds".to_string(),
                                    exit_code: -1,
                                });
                            }
                        }
                        break;
                    }
                }
            }

            // Flush remaining chunks
            while let Ok(chunk) = chunk_rx.try_recv() {
                on_chunk(chunk);
            }
        } else {
            // Sequential execution host by host
            let mut was_cancelled = false;

            for (idx, host_str) in hosts.iter().enumerate() {
                if was_cancelled {
                    host_outputs[idx] = Some(HostOutput {
                        host: host_str.clone(),
                        stdout: String::new(),
                        stderr: "Execution interrupted".to_string(),
                        exit_code: 130,
                    });
                    continue;
                }

                let ep = match SshEndpoint::parse(host_str) {
                    Ok(e) => e,
                    Err(err) => {
                        host_outputs[idx] = Some(HostOutput {
                            host: host_str.clone(),
                            stdout: String::new(),
                            stderr: format!("Target parse error: {}", err),
                            exit_code: 1,
                        });
                        continue;
                    }
                };

                let badge = ep.display_badge();
                let (res_tx, res_rx) = tokio::sync::oneshot::channel();
                let b_name = badge.clone();
                let chunk_tx_c = chunk_tx.clone();

                let on_c = move |c: String| {
                    let _ = chunk_tx_c.send(format!("[{}] {}", b_name, c));
                };

                let handle_res = execute_remote_ssh_streaming(
                    pool.clone(),
                    ep.clone(),
                    proxy_endpoint.clone(),
                    code.clone(),
                    language.clone(),
                    env_vars.clone(),
                    on_c,
                    move |res| {
                        let _ = res_tx.send(res);
                    },
                )
                .await;

                match handle_res {
                    Ok(mut handle) => {
                        let mut sub_done = res_rx;
                        loop {
                            tokio::select! {
                                _ = &mut cancel_rx, if !was_cancelled => {
                                    was_cancelled = true;
                                    if let Some(ctx) = handle.cancel_tx.take() {
                                        let _ = ctx.send(());
                                    }
                                }
                                Some(stdin_bytes) = stdin_rx.recv() => {
                                    let _ = handle.stdin_tx.send(stdin_bytes);
                                }
                                Some(chunk) = chunk_rx.recv() => {
                                    on_chunk(chunk);
                                }
                                res = &mut sub_done => {
                                    let exec_res = res.unwrap_or(ExecutionResult {
                                        stdout: String::new(),
                                        stderr: "Host task dropped".to_string(),
                                        exit_code: -1,
                                        timed_out: false,
                                    });
                                    host_outputs[idx] = Some(HostOutput {
                                        host: badge,
                                        stdout: exec_res.stdout,
                                        stderr: exec_res.stderr,
                                        exit_code: exec_res.exit_code,
                                    });
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        host_outputs[idx] = Some(HostOutput {
                            host: badge,
                            stdout: String::new(),
                            stderr: format!("Connection error: {}", e),
                            exit_code: 1,
                        });
                    }
                }
            }

            while let Ok(chunk) = chunk_rx.try_recv() {
                on_chunk(chunk);
            }
        }

        // Build fleet output
        let valid_outputs: Vec<HostOutput> = host_outputs
            .into_iter()
            .enumerate()
            .map(|(idx, opt)| {
                opt.unwrap_or_else(|| HostOutput {
                    host: hosts.get(idx).cloned().unwrap_or_else(|| format!("host_{}", idx)),
                    stdout: String::new(),
                    stderr: "Execution aborted".to_string(),
                    exit_code: -1,
                })
            })
            .collect();

        let fleet = FleetOutput::new(group_name, valid_outputs, parallel);
        let exec_result = ExecutionResult {
            stdout: fleet.to_output_text(),
            stderr: String::new(),
            exit_code: fleet.overall_exit_code(),
            timed_out: false,
        };

        on_end(fleet, exec_result);
    });

    Ok(StreamingHandle {
        stdin_tx,
        cancel_tx: Some(cancel_tx),
    })
}

/// Unified entrypoint for executing targeted code blocks (single host or fleet fan-out)
#[allow(clippy::too_many_arguments)]
pub async fn execute_targeted_streaming<FChunk, FEnd>(
    pool: SshSessionPool,
    target_str: &str,
    proxy_str: Option<&str>,
    parallel: bool,
    buffer: Option<&TextBuffer>,
    root_dir: &Path,
    code: String,
    language: String,
    env_vars: HashMap<String, String>,
    on_chunk: FChunk,
    on_end: FEnd,
) -> Result<StreamingHandle>
where
    FChunk: FnMut(String) + Send + 'static,
    FEnd: FnOnce(ExecutionResult) + Send + 'static,
{
    let proxy_endpoint = proxy_str.map(SshEndpoint::parse).transpose()?;

    if target_str.starts_with('@') {
        let group_name = target_str.trim_start_matches('@').to_string();
        let hosts = HostRegistry::resolve_target(target_str, buffer, root_dir)?;

        execute_fleet_fanout(
            pool,
            hosts,
            group_name,
            proxy_endpoint,
            code,
            language,
            env_vars,
            parallel,
            on_chunk,
            move |_fleet, result| {
                on_end(result);
            },
        )
        .await
    } else {
        let endpoint = SshEndpoint::parse(target_str)?;
        execute_remote_ssh_streaming(
            pool,
            endpoint,
            proxy_endpoint,
            code,
            language,
            env_vars,
            on_chunk,
            on_end,
        )
        .await
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_ssh_uri_parsing() {
        let ep1 = SshEndpoint::parse("ssh://ubuntu@10.0.0.12:22").unwrap();
        assert_eq!(ep1.user, Some("ubuntu".to_string()));
        assert_eq!(ep1.host, "10.0.0.12");
        assert_eq!(ep1.port, 22);
        assert_eq!(ep1.password, None);
        assert_eq!(ep1.to_uri(), "ssh://ubuntu@10.0.0.12:22");

        let ep2 = SshEndpoint::parse("ssh://app:secret@10.0.1.5").unwrap();
        assert_eq!(ep2.user, Some("app".to_string()));
        assert_eq!(ep2.password, Some("secret".to_string()));
        assert_eq!(ep2.host, "10.0.1.5");

        let ep3 = SshEndpoint::parse("ssh://bastion.corp.com").unwrap();
        assert_eq!(ep3.user, None);
        assert_eq!(ep3.host, "bastion.corp.com");
        assert_eq!(ep3.port, 22);

        let ep4 = SshEndpoint::parse("deploy@prod-host:2222").unwrap();
        assert_eq!(ep4.user, Some("deploy".to_string()));
        assert_eq!(ep4.host, "prod-host");
        assert_eq!(ep4.port, 2222);

        let ep5 = SshEndpoint::parse("192.168.1.100").unwrap();
        assert_eq!(ep5.user, None);
        assert_eq!(ep5.host, "192.168.1.100");
        assert_eq!(ep5.port, 22);

        // IPv6 bracketed host parsing
        let ep6 = SshEndpoint::parse("ssh://ubuntu@[::1]:2222").unwrap();
        assert_eq!(ep6.user, Some("ubuntu".to_string()));
        assert_eq!(ep6.host, "::1");
        assert_eq!(ep6.port, 2222);

        let ep7 = SshEndpoint::parse("[fe80::1]").unwrap();
        assert_eq!(ep7.host, "fe80::1");
        assert_eq!(ep7.port, 22);

        assert!(SshEndpoint::parse("").is_err());
    }

    #[test]
    fn test_frontmatter_host_group_parsing() {
        let doc = r#"---
web_servers: [10.0.0.1, 10.0.0.2]
db_servers:
  - 10.0.1.10
  - 10.0.1.11
---
# Infrastructure Runbook
"#;
        let buf = TextBuffer::from_str(doc);
        let groups = HostRegistry::parse_frontmatter(&buf);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups.get("web_servers"), Some(&vec!["10.0.0.1".to_string(), "10.0.0.2".to_string()]));
        assert_eq!(groups.get("db_servers"), Some(&vec!["10.0.1.10".to_string(), "10.0.1.11".to_string()]));
    }

    #[test]
    fn test_hosts_yaml_file_parsing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let hosts_file = temp_dir.path().join("hosts.yaml");
        let content = r#"
hosts:
  cluster_nodes:
    - 192.168.1.1
    - 192.168.1.2
    - 192.168.1.3
"#;
        std::fs::write(&hosts_file, content).unwrap();

        let groups = HostRegistry::parse_hosts_file(temp_dir.path());
        assert_eq!(groups.get("cluster_nodes"), Some(&vec![
            "192.168.1.1".to_string(),
            "192.168.1.2".to_string(),
            "192.168.1.3".to_string(),
        ]));
    }

    #[test]
    fn test_resolve_target_hosts() {
        let doc = r#"---
web_servers: [10.0.0.1, 10.0.0.2]
---
"#;
        let buf = TextBuffer::from_str(doc);
        let temp_dir = tempfile::tempdir().unwrap();

        // 1. Resolve host group
        let resolved = HostRegistry::resolve_target("@web_servers", Some(&buf), temp_dir.path()).unwrap();
        assert_eq!(resolved, vec!["10.0.0.1", "10.0.0.2"]);

        // 2. Resolve direct host
        let direct = HostRegistry::resolve_target("ssh://ubuntu@10.0.0.5", Some(&buf), temp_dir.path()).unwrap();
        assert_eq!(direct, vec!["ssh://ubuntu@10.0.0.5"]);

        // 3. Unknown host group
        let err = HostRegistry::resolve_target("@unknown_group", Some(&buf), temp_dir.path());
        assert!(err.is_err());
    }

    #[test]
    fn test_fleet_output_aggregation_and_parsing() {
        let hosts = vec![
            HostOutput {
                host: "10.0.0.1".to_string(),
                stdout: "node-1 ok".to_string(),
                stderr: String::new(),
                exit_code: 0,
            },
            HostOutput {
                host: "10.0.0.2".to_string(),
                stdout: "node-2 failed".to_string(),
                stderr: "Connection refused".to_string(),
                exit_code: 1,
            },
        ];

        let fleet = FleetOutput::new("web_servers".to_string(), hosts, true);
        assert_eq!(fleet.status_badges(), "[10.0.0.1: ✓ exit 0] [10.0.0.2: ✖ exit 1]");
        assert_eq!(fleet.failed_hosts(), vec!["10.0.0.2"]);
        assert!(!fleet.is_success());
        assert_eq!(fleet.overall_exit_code(), 1);

        let out_text = fleet.to_output_text();
        assert!(out_text.contains("[10.0.0.1: ✓ exit 0] [10.0.0.2: ✖ exit 1]"));
        assert!(out_text.contains("=== [10.0.0.1] ==="));
        assert!(out_text.contains("node-1 ok"));
        assert!(out_text.contains("=== [10.0.0.2] ==="));
        assert!(out_text.contains("node-2 failed"));

        // Round-trip parse
        let parsed = FleetOutput::parse(&out_text).unwrap();
        assert_eq!(parsed.hosts.len(), 2);
        assert_eq!(parsed.hosts[0].host, "10.0.0.1");
        assert_eq!(parsed.hosts[0].exit_code, 0);
        assert_eq!(parsed.hosts[0].stdout.trim(), "node-1 ok");
        assert_eq!(parsed.hosts[1].host, "10.0.0.2");
        assert_eq!(parsed.hosts[1].exit_code, 1);
        assert_eq!(parsed.hosts[1].stdout.trim(), "node-2 failed");
        assert_eq!(parsed.hosts[1].stderr.trim(), "Connection refused");
    }
}
