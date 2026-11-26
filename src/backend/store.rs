//! App data (project list, settings) kept in one JSON file. Never holds credentials.
//! Same file and format as the Electron build: %APPDATA%\cc-launcher\app.json.

use super::types::{write_atomic, Project, Settings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
struct StoreData {
    #[serde(default = "one")]
    version: u32,
    #[serde(default)]
    projects: Vec<Project>,
    #[serde(default)]
    settings: Settings,
}

fn one() -> u32 {
    1
}

pub struct Store {
    file: PathBuf,
    pub projects: Vec<Project>,
    pub settings: Settings,
}

/// %APPDATA%\cc-launcher
pub fn data_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("cc-launcher")
}

impl Store {
    pub fn load() -> Self {
        let file = data_dir().join("app.json");
        let mut store = Store { file: file.clone(), projects: vec![], settings: Settings::default() };
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
            }
            None => {
                // Keep the broken file for inspection and start fresh.
                let _ = std::fs::rename(&file, file.with_extension(format!("json.broken-{}", super::types::now_ms())));
            }
        }
        store
    }

    pub fn save(&self) {
        let data = StoreData { version: 1, projects: self.projects.clone(), settings: self.settings.clone() };
        if let Ok(text) = serde_json::to_string_pretty(&data) {
            let _ = write_atomic(&self.file, &text);
        }
    }
}
