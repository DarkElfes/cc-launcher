//! Process helpers.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Output {
    pub status_ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Builds a Command that never flashes a console window.
pub fn hidden_command(file: &str) -> Command {
    let mut cmd = Command::new(file);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Runs a program without a console window and with a timeout. None if it couldn't start or timed out.
pub fn run_hidden(
    file: &str,
    args: &[&str],
    cwd: Option<&str>,
    env: Option<&[(String, String)]>,
    timeout: Duration,
) -> Option<Output> {
    let mut cmd = hidden_command(file);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    if let Some(e) = env {
        cmd.env_clear();
        cmd.envs(e.iter().map(|(k, v)| (k, v)));
    }
    let mut child = cmd.spawn().ok()?;
    let mut out = child.stdout.take()?;
    let mut err = child.stderr.take()?;
    let t_out = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out.read_to_end(&mut s);
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err.read_to_end(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if start.elapsed() > timeout => {
                kill_tree(child.id());
                let _ = child.kill();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break None,
        }
    };
    let stdout = String::from_utf8_lossy(&t_out.join().unwrap_or_default()).to_string();
    let stderr = String::from_utf8_lossy(&t_err.join().unwrap_or_default()).to_string();
    let status = status?;
    Some(Output { status_ok: status.success(), stdout, stderr })
}

/// Ends a process and its children (claude starts helpers).
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = hidden_command("taskkill").args(["/pid", &pid.to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").args(["-INT", &pid.to_string()]).status();
    }
}

/// Opens a file or folder with its default program.
pub fn open_path(path: &str) -> Result<(), String> {
    open::that_detached(path).map_err(|e| e.to_string())
}

/// Runs `<editor> "<path>"` through the shell so .cmd launchers on PATH work.
pub fn open_in_editor(editor: &str, path: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Windows paths cannot contain quotes.
        hidden_command("cmd")
            .raw_arg(format!("/C {editor} \"{path}\""))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        Command::new("sh").arg("-c").arg(format!("{editor} \"{path}\"")).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}
