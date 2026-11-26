//! App data (project list, settings, tabs to reopen) kept in one JSON file. Never holds credentials.
//! Same file and format as the Electron build: %APPDATA%\cc-launcher\app.json.

use super::types::{write_atomic, Project, SavedTab, Settings};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
struct StoreData {
    #[serde(default = "one")]
    version: u32,
    #[serde(default)]
    projects: Vec<Project>,
    #[serde(default)]
    settings: Settings,
    /// Tabs that were open when the app last closed. Kept as raw JSON so one bad entry can't make
    /// the whole file unreadable.
    #[serde(default, rename = "openTabs")]
    open_tabs: Vec<Value>,
}

fn one() -> u32 {
    1
}

pub struct Store {
    file: PathBuf,
    pub projects: Vec<Project>,
    pub settings: Settings,
    pub open_tabs: Vec<SavedTab>,
}

/// %APPDATA%\cc-launcher
pub fn data_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("cc-launcher")
}

impl Store {
    pub fn load() -> Self {
        let file = data_dir().join("app.json");
        let mut store = Store { file: file.clone(), projects: vec![], settings: Settings::default(), open_tabs: vec![] };
        if !file.exists() {
            return store;
        }
        let parsed = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str::<StoreData>(t.trim_start_matches('\u{feff}')).ok());
        match parsed {
            Some(d) => {
                store.projects = d.projects;
                store.settings = d.settings;
                store.open_tabs = d.open_tabs.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect();
            }
            None => {
                // Keep the broken file for inspection and start fresh.
                let _ = std::fs::rename(&file, file.with_extension(format!("json.broken-{}", super::types::now_ms())));
            }
        }
        store
    }

    pub fn save(&self) {
        let data = StoreData {
            version: 1,
            projects: self.projects.clone(),
            settings: self.settings.clone(),
            open_tabs: self.open_tabs.iter().filter_map(|t| serde_json::to_value(t).ok()).collect(),
        };
        if let Ok(text) = serde_json::to_string_pretty(&data) {
            let _ = write_atomic(&self.file, &text);
        }
    }
}
