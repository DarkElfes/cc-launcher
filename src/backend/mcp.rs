//! MCP servers / connectors available to a profile in a project, from `claude mcp list`
//! (https://code.claude.com/docs/en/mcp). It health-checks each server, so it can take a while; the
//! caller caches. The line format isn't documented, so parsing keys off the status markers.

use super::types::{McpServer, McpStatus};
use super::util::run_hidden;
use std::time::Duration;

fn status_of(line: &str) -> McpStatus {
    let l = line.to_lowercase();
    if (line.contains('✔') || l.contains("connected")) && !l.contains("fail") && !l.contains("error") {
        McpStatus::Connected
    } else if l.contains("needs authentication") || l.contains("! needs") {
        McpStatus::NeedsAuth
    } else if line.contains('✘') || l.contains("failed") || l.contains("connection error") {
        McpStatus::Failed
    } else if line.contains('⏸') || l.contains("pending approval") {
        McpStatus::Pending
    } else if line.contains('⊘') || l.contains("disabled") {
        McpStatus::Disabled
    } else {
        McpStatus::Unknown
    }
}

pub fn list_mcp(claude: &str, cwd: &str, env: &[(String, String)]) -> Result<Vec<McpServer>, String> {
    let out = run_hidden(claude, &["mcp", "list"], Some(cwd), Some(env), Duration::from_secs(90))
        .ok_or_else(|| "claude mcp list did not finish".to_string())?;
    let text = out.stdout.trim().to_string();
    if !out.status_ok && text.is_empty() {
        return Err(out.stderr.lines().next().unwrap_or("claude mcp list failed").to_string());
    }
    let line_re = regex::Regex::new(r"^([^:]+?):\s+(.*)$").unwrap();
    let split_re = regex::Regex::new(r"(?i)\s+-\s+([✔✘!⏸⊘]|Connected|Failed|Needs|Pending|Disabled)").unwrap();
    let mut servers = vec![];
    for raw in text.lines() {
        let line = raw.trim();
        let lower = line.to_lowercase();
        if line.is_empty() || lower.starts_with("checking mcp server health") || lower.starts_with("no mcp servers") {
            continue;
        }
        let Some(c) = line_re.captures(line) else { continue };
        let name = c[1].trim().to_string();
        let rest = c[2].to_string();
        let (detail, status_part) = match split_re.find(&rest) {
            Some(m) => (rest[..m.start()].trim().to_string(), rest[m.start()..].to_string()),
            None => (rest.trim().to_string(), rest.clone()),
        };
        servers.push(McpServer { name, detail, status: status_of(&status_part) });
    }
    Ok(servers)
}
