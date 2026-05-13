use anyhow::{anyhow, Result};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use tokio::sync::broadcast;

/// Parses a remote spec of the form `user@host:/path/glob` into (ssh-target, remote-path).
pub fn parse_remote(spec: &str) -> Result<(String, String)> {
    let (target, path) = spec
        .split_once(':')
        .ok_or_else(|| anyhow!("expected user@host:/path"))?;
    if target.is_empty() || path.is_empty() {
        return Err(anyhow!("expected user@host:/path"));
    }
    Ok((target.to_string(), path.to_string()))
}

/// Spawns `ssh user@host tail -F /path/glob` (uses sh -c so globs expand
/// remotely) and forwards each output line onto `tx` as a JSON envelope.
///
/// Returns once ssh exits. Caller usually runs this on a dedicated thread.
pub fn stream(spec: &str, tx: broadcast::Sender<String>) -> Result<()> {
    let (target, path) = parse_remote(spec)?;
    let remote_cmd = format!("tail -F -n 0 {}", shell_quote(&path));
    let mut child = Command::new("ssh")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ServerAliveInterval=30")
        .arg(&target)
        .arg(&remote_cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let reader = BufReader::new(stdout);
    let target_label = target.clone();
    for line in reader.lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        if line.is_empty() { continue; }
        let payload = serde_json::json!({
            "file": format!("ssh://{}", target_label),
            "line": line,
            "remote": true,
        }).to_string();
        let _ = tx.send(payload);
    }
    let _ = child.wait();
    Ok(())
}

/// Tail itself is a shell command, the path may contain a glob, so don't
/// quote the whole path — instead just escape single-quotes so the user
/// can pass `/var/log/*.jsonl` without surprises.
fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '*' | '?' | '[' | ']')) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}
