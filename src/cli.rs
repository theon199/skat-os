use anyhow::{anyhow, Result};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum CliCommand {
    Local {
        workspace: PathBuf,
    },
    Host {
        port: u16,
        workspace: PathBuf,
        name: String,
        headless: bool,
    },
    Join {
        addr: String,
        name: String,
    },
    Help,
}

pub fn resolve_default_workspace() -> PathBuf {
    if let Ok(env_ws) = std::env::var("SKAT_WORKSPACE")
        && !env_ws.trim().is_empty()
    {
        return expand_path(env_ws.trim());
    }

    let local_docs = PathBuf::from("./docs");
    if local_docs.exists() && local_docs.is_dir() {
        return local_docs;
    }

    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join("notes");
    }

    PathBuf::from("./docs")
}

pub fn expand_path(raw: &str) -> PathBuf {
    if (raw.starts_with("~/") || raw == "~")
        && let Ok(home) = std::env::var("HOME")
    {
        if raw == "~" {
            return PathBuf::from(home);
        } else {
            return PathBuf::from(home).join(&raw[2..]);
        }
    }
    PathBuf::from(raw)
}

pub fn parse_cli_args(args: &[String]) -> Result<CliCommand> {
    if args.len() <= 1 {
        return Ok(CliCommand::Local {
            workspace: resolve_default_workspace(),
        });
    }

    let subcmd = args[1].as_str();
    match subcmd {
        "-h" | "--help" | "help" => Ok(CliCommand::Help),
        "host" => {
            let mut port: u16 = 2222;
            let mut workspace = resolve_default_workspace();
            let mut name = std::env::var("USER").unwrap_or_else(|_| "Host".to_string());
            let mut headless = false;

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--port" | "-p" => {
                        if i + 1 < args.len() {
                            port = args[i + 1].parse().map_err(|_| anyhow!("Invalid port number"))?;
                            i += 2;
                        } else {
                            return Err(anyhow!("Missing argument for --port"));
                        }
                    }
                    p if p.starts_with("--port=") => {
                        let val = p.strip_prefix("--port=").unwrap();
                        port = val.parse().map_err(|_| anyhow!("Invalid port number"))?;
                        i += 1;
                    }
                    "--workspace" | "-w" => {
                        if i + 1 < args.len() {
                            workspace = expand_path(&args[i + 1]);
                            i += 2;
                        } else {
                            return Err(anyhow!("Missing argument for --workspace"));
                        }
                    }
                    w if w.starts_with("--workspace=") => {
                        let val = w.strip_prefix("--workspace=").unwrap();
                        workspace = expand_path(val);
                        i += 1;
                    }
                    "--name" | "-n" => {
                        if i + 1 < args.len() {
                            name = args[i + 1].clone();
                            i += 2;
                        } else {
                            return Err(anyhow!("Missing argument for --name"));
                        }
                    }
                    n if n.starts_with("--name=") => {
                        name = n.strip_prefix("--name=").unwrap().to_string();
                        i += 1;
                    }
                    "--headless" => {
                        headless = true;
                        i += 1;
                    }
                    other => {
                        return Err(anyhow!("Unknown option for host: {}", other));
                    }
                }
            }

            Ok(CliCommand::Host {
                port,
                workspace,
                name,
                headless,
            })
        }
        "join" => {
            if args.len() < 3 {
                return Err(anyhow!("Usage: skat-os join <host:port> [--name <name>]"));
            }
            let addr = args[2].clone();
            let mut name = std::env::var("USER").unwrap_or_else(|_| "Alice".to_string());

            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "--name" | "-n" => {
                        if i + 1 < args.len() {
                            name = args[i + 1].clone();
                            i += 2;
                        } else {
                            return Err(anyhow!("Missing argument for --name"));
                        }
                    }
                    n if n.starts_with("--name=") => {
                        name = n.strip_prefix("--name=").unwrap().to_string();
                        i += 1;
                    }
                    other => {
                        return Err(anyhow!("Unknown option for join: {}", other));
                    }
                }
            }

            Ok(CliCommand::Join { addr, name })
        }
        workspace_arg => Ok(CliCommand::Local {
            workspace: expand_path(workspace_arg),
        }),
    }
}
