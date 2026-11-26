//! A profile is one Claude account = one config folder under ~/.claude-profiles, passed to
//! Claude Code as CLAUDE_CONFIG_DIR. Each folder holds that account's login, settings and session
//! history.
//!
//! The normal ~/.claude folder is deliberately NOT a profile: plain `claude` in any terminal uses
//! it too, so a login there could silently switch "which account" a profile means.
//!
//! This module only checks whether a credentials file exists. It never opens, copies or sends it.

use super::account_info::ProfileLive;
use super::types::{now_ms, ProfileInfo, ProfileUsage, UsageWindow};
use super::usage::{ensure_status_line, read_usage};
use std::collections::HashMap;
use std::path::PathBuf;

pub fn profiles_root() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".claude-profiles")
}

fn name_ok(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 32
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-' || *c == b'_')
}

pub fn validate_profile_name(name: &str) -> Option<&'static str> {
    if name_ok(name) {
        None
    } else {
        Some("Use 1–32 lowercase letters, digits, \"-\" or \"_\", starting with a letter or digit.")
    }
}

pub fn profile_dir(name: &str) -> PathBuf {
    profiles_root().join(name)
}

/// Marks windows whose reset time has passed: their numbers belong to the previous window.
fn freshen(u: Option<ProfileUsage>) -> Option<ProfileUsage> {
    let mut u = u?;
    let fix = |w: &mut Option<UsageWindow>| {
        if let Some(x) = w {
            if matches!(x.resets_at, Some(t) if t <= now_ms()) {
                *x = UsageWindow { used_percent: 0.0, resets_at: None, resets_label: x.resets_label.clone(), stale: true };
            }
        }
    };
    fix(&mut u.five_hour);
    fix(&mut u.week);
    Some(u)
}

fn info(name: &str, live: Option<&ProfileLive>) -> ProfileInfo {
    let dir = profile_dir(name);
    let from_status = read_usage(&dir);
    let from_cmd = live.and_then(|l| l.usage.clone());
    // Prefer whichever measurement is newer.
    let usage = match (from_status, from_cmd) {
        (Some(s), Some(c)) => Some(if s.updated_at > c.updated_at { s } else { c }),
        (s, c) => s.or(c),
    };
    let account = live.and_then(|l| l.account.clone());
    ProfileInfo {
        name: name.to_string(),
        dir: dir.to_string_lossy().to_string(),
        signed_in: match &account {
            Some(a) => a.logged_in,
            None => dir.join(".credentials.json").exists(),
        },
        account,
        usage: freshen(usage),
        error: live.and_then(|l| l.error.clone()),
    }
}

pub fn list_profile_names() -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(profiles_root()) else { return vec![] };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| name_ok(n))
        .collect();
    names.sort();
    names
}

pub fn list_profiles(live: &HashMap<String, ProfileLive>) -> Vec<ProfileInfo> {
    list_profile_names().iter().map(|n| info(n, live.get(n))).collect()
}

pub fn profile_exists(name: &str) -> bool {
    name_ok(name) && profile_dir(name).is_dir()
}

pub fn create_profile(name: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(profile_dir(name))?;
    ensure_status_line(&profile_dir(name));
    Ok(())
}

/// Makes sure every profile reports its plan usage (see usage.rs). Safe to call often.
pub fn ensure_all_status_lines() {
    for n in list_profile_names() {
        ensure_status_line(&profile_dir(&n));
    }
}

/// Environment for a Claude Code process running under the given profile.
pub fn profile_env(name: &str) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| {
            // Don't leak settings from whatever launched this app (for example a Claude Code session).
            !matches!(
                k.to_ascii_uppercase().as_str(),
                "CLAUDECODE" | "CLAUDE_CODE_ENTRYPOINT" | "CLAUDE_CODE_CHILD_SESSION" | "CLAUDE_CODE_SSE_PORT" | "CLAUDE_CONFIG_DIR" | "COLORTERM"
            )
        })
        .collect();
    env.push(("CLAUDE_CONFIG_DIR".into(), profile_dir(name).to_string_lossy().to_string()));
    env.push(("COLORTERM".into(), "truecolor".into()));
    env
}

/// Stable color hue per profile name so each account is easy to recognise (0..360).
pub fn profile_hue(name: &str) -> f32 {
    let mut h: u32 = 0;
    for ch in name.encode_utf16() {
        h = (h * 31 + ch as u32) % 360;
    }
    h as f32
}
