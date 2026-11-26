//! Account details and plan usage per profile, from Claude Code's own CLI (no credentials are read):
//!
//! - `claude auth status --json`: email, plan (subscriptionType), org, loggedIn.
//! - `claude -p "/usage" --output-format json`: the /usage screen as text. It runs locally as a
//!   slash command (no model call, $0 cost, doesn't touch the limits), e.g.
//!     "Current session: 33% used · resets 11pm (Europe/Kyiv)"
//!     "Current week (all models): 5% used · resets Nov 10, 7pm (Europe/Kyiv)"
//!   The text format isn't a documented API, so parsing is defensive.

use super::types::{now_ms, AccountInfo, ProfileUsage, UsageSource, UsageWindow};
use super::util::run_hidden;
use chrono::{Datelike, Duration as CDuration, Local, NaiveDate, TimeZone};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct ProfileLive {
    pub account: Option<AccountInfo>,
    pub usage: Option<ProfileUsage>,
    pub error: Option<String>,
}

fn run(file: &str, args: &[&str], env: &[(String, String)], cwd: &str, timeout: u64) -> Result<String, String> {
    let out = run_hidden(file, args, Some(cwd), Some(env), Duration::from_secs(timeout)).ok_or_else(|| format!("claude {} timed out", args.join(" ")))?;
    if !out.status_ok && out.stdout.trim().is_empty() {
        return Err(out.stderr.lines().next().unwrap_or("command failed").chars().take(200).collect());
    }
    Ok(out.stdout)
}

pub fn fetch_account(claude: &str, env: &[(String, String)], cwd: &str) -> Result<AccountInfo, String> {
    let j: Value = serde_json::from_str(&run(claude, &["auth", "status", "--json"], env, cwd, 30)?).map_err(|e| e.to_string())?;
    let s = |k: &str| j.get(k).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Ok(AccountInfo {
        logged_in: j.get("loggedIn").and_then(|v| v.as_bool()) == Some(true),
        email: s("email"),
        plan: s("subscriptionType").or_else(|| s("planTier")),
        org_name: s("orgName"),
    })
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// "Nov 10, 7pm" / "11pm" / "10:30pm" (local time, as shown by Claude Code) → epoch ms, or None.
pub fn parse_reset_time(text: &str) -> Option<i64> {
    let re = regex::Regex::new(r"(?i)^(?:([A-Za-z]{3})[a-z]*\.? (\d{1,2}),? )?(\d{1,2})(?::(\d{2}))?\s*(am|pm)$").unwrap();
    let c = re.captures(text.trim())?;
    let mut hour: u32 = c[3].parse::<u32>().ok()? % 12;
    if c[5].eq_ignore_ascii_case("pm") {
        hour += 12;
    }
    let minute: u32 = c.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
    let now = Local::now();
    let mut date = now.date_naive();
    if let Some(mon) = c.get(1) {
        let month = MONTHS.iter().position(|m| m.eq_ignore_ascii_case(mon.as_str()))? as u32 + 1;
        let day: u32 = c[2].parse().ok()?;
        date = NaiveDate::from_ymd_opt(now.year(), month, day)?;
        let dt = Local.from_local_datetime(&date.and_hms_opt(hour, minute, 0)?).single()?;
        if dt < now - CDuration::days(1) {
            date = NaiveDate::from_ymd_opt(now.year() + 1, month, day)?;
        }
    } else {
        let dt = Local.from_local_datetime(&date.and_hms_opt(hour, minute, 0)?).single()?;
        if dt <= now {
            date = date.succ_opt()?;
        }
    }
    Some(Local.from_local_datetime(&date.and_hms_opt(hour, minute, 0)?).single()?.timestamp_millis())
}

fn parse_line(line: &str) -> Option<UsageWindow> {
    let re = regex::Regex::new(r"(?i):\s*(\d+(?:\.\d+)?)%\s*used(?:\s*[·•-]\s*resets\s+(.+))?$").unwrap();
    let c = re.captures(line)?;
    let paren = regex::Regex::new(r"\s*\([^)]*\)\s*$").unwrap();
    let label = c.get(2).map(|m| paren.replace(m.as_str(), "").trim().to_string()); // drop "(Europe/Kyiv)"
    Some(UsageWindow {
        used_percent: c[1].parse::<f64>().ok()?.min(100.0),
        resets_at: label.as_deref().and_then(parse_reset_time),
        resets_label: label,
        stale: false,
    })
}

/// Parses the text of /usage into the 5-hour (session) and weekly windows.
pub fn parse_usage_text(text: &str) -> Option<ProfileUsage> {
    let mut five = None;
    let mut week = None;
    for raw in text.lines() {
        let line = raw.trim();
        let l = line.to_lowercase();
        if l.starts_with("current session") {
            five = parse_line(line);
        } else if l.starts_with("current week (all models)") || (week.is_none() && l.starts_with("current week")) {
            week = parse_line(line).or(week);
        }
    }
    if five.is_none() && week.is_none() {
        return None;
    }
    Some(ProfileUsage { updated_at: now_ms(), five_hour: five, week, source: UsageSource::UsageCommand })
}

pub fn fetch_usage(claude: &str, env: &[(String, String)], cwd: &str) -> Result<Option<ProfileUsage>, String> {
    let out = run(claude, &["-p", "/usage", "--output-format", "json"], env, cwd, 45)?;
    let j: Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    let result = j.get("result").and_then(|r| r.as_str());
    if j.get("is_error").and_then(|v| v.as_bool()) == Some(true) || result.is_none() {
        return Err(result.unwrap_or("usage command failed").to_string());
    }
    Ok(parse_usage_text(result.unwrap()))
}

/// Refreshes one profile (blocking; run on a background thread).
pub fn refresh_profile(claude: &str, env: &[(String, String)], work_dir: &std::path::Path, prev: Option<ProfileLive>) -> ProfileLive {
    let cwd = work_dir;
    let _ = std::fs::create_dir_all(cwd);
    let cwd = cwd.to_string_lossy().to_string();
    let prev = prev.unwrap_or_default();
    let mut next = ProfileLive { error: None, ..prev.clone() };
    match fetch_account(claude, env, &cwd) {
        Ok(acc) => {
            if acc.logged_in {
                match fetch_usage(claude, env, &cwd) {
                    Ok(u) => next.usage = u.or(prev.usage),
                    Err(e) => next.error = Some(e.chars().take(200).collect()),
                }
            } else {
                next.usage = None;
            }
            next.account = Some(acc);
        }
        Err(e) => next.error = Some(e.chars().take(200).collect()),
    }
    next
}
