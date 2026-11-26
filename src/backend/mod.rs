//! Everything that isn't drawing: processes, files, the hook server. Background threads talk to
//! the UI through `BackendEvent`s on one channel.

pub mod account_info;
pub mod chat;
pub mod git;
pub mod history;
pub mod hook_server;
pub mod hooks_config;
pub mod mcp;
pub mod platform;
pub mod profile_settings;
pub mod profiles;
pub mod sessions;
pub mod store;
pub mod terminal;
pub mod types;
pub mod updater;
pub mod usage;
pub mod util;

use serde_json::Value;

pub enum BackendEvent {
    /// A Claude Code hook arrived. `reply` is set for PermissionRequest: send a hook response body
    /// to answer (chat tabs), or drop it for "no decision".
    Hook { tab: String, event: Value, reply: Option<std::sync::mpsc::Sender<Value>> },
    /// A stream-json event from a chat tab's claude process.
    ChatEvent { tab: String, generation: u64, event: Value },
    /// A chat tab's claude process ended.
    ChatExit { tab: String, generation: u64, stderr: String },
    /// New output in a terminal tab.
    TermWakeup { tab: String },
    TermExit { tab: String, code: i32 },
    /// One profile's account/usage refresh finished.
    AccountRefreshed { profile: String, live: account_info::ProfileLive },
    AccountsRefreshDone,
    Update(types::UpdateState),
    NotificationClicked { tab: String },
    /// A second copy of the app was started.
    Activate,
}
