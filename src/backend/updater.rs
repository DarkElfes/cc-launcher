//! Auto-update from the public GitHub repo's releases (no token needed).
//!
//! A release carries `cc-launcher.exe`. When a newer version exists, it's downloaded next to the
//! running exe, and "Restart" swaps it in (self_replace) and starts the new one. Only active in
//! release builds.

use super::types::UpdateState;
use super::BackendEvent;
use std::io::Read;
use std::path::PathBuf;

const REPO: &str = "DarkElfes/cc-launcher";
const ASSET: &str = "cc-launcher.exe";

pub fn enabled() -> bool {
    !cfg!(debug_assertions)
}

fn newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| v.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(latest) > parse(current)
}

fn download_path() -> PathBuf {
    super::store::data_dir().join("update").join(ASSET)
}

fn friendly(e: impl std::fmt::Display) -> String {
    let msg = e.to_string();
    if msg.contains("404") {
        "No release found on GitHub yet.".into()
    } else if msg.to_lowercase().contains("dns") || msg.to_lowercase().contains("connect") {
        "Could not reach GitHub. Check your internet connection.".into()
    } else {
        msg.lines().next().unwrap_or("").chars().take(300).collect()
    }
}

/// Checks for a newer release and downloads it (blocking; run on a background thread).
pub fn check(events: &async_channel::Sender<BackendEvent>) {
    let send = |s: UpdateState| {
        let _ = events.send_blocking(BackendEvent::Update(s));
    };
    if !enabled() {
        return send(UpdateState::Disabled);
    }
    send(UpdateState::Checking);
    let agent = ureq::Agent::new_with_defaults();
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let release: serde_json::Value = match agent
        .get(&url)
        .header("User-Agent", "cc-launcher-updater")
        .header("Accept", "application/vnd.github+json")
        .call()
        .and_then(|mut r| r.body_mut().read_json())
    {
        Ok(v) => v,
        Err(e) => return send(UpdateState::Error(friendly(e))),
    };
    let tag = release.get("tag_name").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let version = tag.trim_start_matches('v').to_string();
    if !newer(&version, env!("CARGO_PKG_VERSION")) {
        return send(UpdateState::None);
    }
    let asset_url = release
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|a| a.iter().find(|x| x.get("name").and_then(|n| n.as_str()) == Some(ASSET)))
        .and_then(|x| x.get("browser_download_url").and_then(|u| u.as_str()))
        .map(String::from);
    let Some(asset_url) = asset_url else {
        return send(UpdateState::Error(format!("Release {tag} has no {ASSET}.")));
    };
    send(UpdateState::Downloading { version: version.clone(), percent: 0 });
    let result = (|| -> Result<(), String> {
        let mut resp = agent.get(&asset_url).header("User-Agent", "cc-launcher-updater").call().map_err(friendly)?;
        let total: u64 = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
        let mut reader = resp.body_mut().with_config().limit(512 * 1024 * 1024).reader();
        let mut data = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        let mut last = 0u8;
        loop {
            let n = reader.read(&mut buf).map_err(friendly)?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
            if total > 0 {
                let pct = ((data.len() as u64 * 100) / total).min(100) as u8;
                if pct != last {
                    last = pct;
                    send(UpdateState::Downloading { version: version.clone(), percent: pct });
                }
            }
        }
        let path = download_path();
        std::fs::create_dir_all(path.parent().unwrap()).map_err(friendly)?;
        std::fs::write(&path, data).map_err(friendly)?;
        Ok(())
    })();
    match result {
        Ok(()) => send(UpdateState::Ready { version }),
        Err(e) => send(UpdateState::Error(e)),
    }
}

/// Swaps in the downloaded exe and starts it. The caller quits the app afterwards.
pub fn install() -> Result<(), String> {
    let path = download_path();
    self_replace::self_replace(&path).map_err(friendly)?;
    let _ = std::fs::remove_file(&path);
    let exe = std::env::current_exe().map_err(friendly)?;
    std::process::Command::new(exe).spawn().map_err(friendly)?;
    Ok(())
}
