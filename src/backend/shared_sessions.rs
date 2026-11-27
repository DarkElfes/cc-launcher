//! One session folder per project, shared by all accounts.
//!
//! Claude Code keeps a project's sessions (and its auto memory) in
//! `<CLAUDE_CONFIG_DIR>/projects/<project dir name>/`, so every account had its own history. We
//! keep the real folder in `~/.claude-profiles/_shared/projects/<project dir name>/` and make each
//! profile's folder a directory junction to it (no admin rights needed). Then any account lists
//! and resumes the same sessions, and a continued session keeps writing to the same file.
//!
//! Turning a profile's existing folder into a link moves its contents into the shared folder
//! first. Nothing is deleted: a name clash with different content keeps both (the incoming file
//! gets a `.from-<profile>.bak` suffix). Errors leave that profile as it was; the next call retries.
//! `_shared` can't be a profile name (names start with a letter or digit).

use super::history::project_dir_name;
use super::profiles::{list_profile_names, profile_dir, profiles_root};
use std::path::{Path, PathBuf};

pub fn shared_root() -> PathBuf {
    profiles_root().join("_shared").join("projects")
}

/// The shared session folder of a project (None for names Claude Code hashes).
pub fn shared_dir(cwd: &str) -> Option<PathBuf> {
    let name = project_dir_name(cwd);
    (!name.is_empty() && name.len() <= 200).then(|| shared_root().join(name))
}

/// Links every profile's session folder for this project to the shared one. Returns the
/// problems it ran into (one line each); an empty list means all profiles share it.
pub fn ensure_shared(cwd: &str) -> Vec<String> {
    let Some(shared) = shared_dir(cwd) else { return vec![] };
    if let Err(e) = std::fs::create_dir_all(&shared) {
        return vec![format!("{}: {e}", shared.display())];
    }
    let name = project_dir_name(cwd);
    let mut errors = vec![];
    for p in list_profile_names() {
        if let Err(e) = link_profile(&profile_dir(&p).join("projects").join(&name), &shared, &p) {
            errors.push(format!("{p}: {e}"));
        }
    }
    errors
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (dunce_canon(a), dunce_canon(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

fn dunce_canon(p: &Path) -> Option<String> {
    let c = std::fs::canonicalize(p).ok()?;
    Some(c.to_string_lossy().trim_start_matches(r"\\?\").to_lowercase())
}

fn link_profile(link: &Path, shared: &Path, profile: &str) -> Result<(), String> {
    if let Ok(meta) = std::fs::symlink_metadata(link) {
        if meta.file_type().is_symlink() {
            // Ours already, or a link the user made: either way, leave it.
            return Ok(());
        }
        if !meta.is_dir() {
            return Err(format!("{} is a file, not a folder", link.display()));
        }
        if same_dir(link, shared) {
            return Ok(());
        }
        merge_into(link, shared, profile)?;
        std::fs::remove_dir(link).map_err(|e| format!("couldn't remove {} after moving its contents: {e}", link.display()))?;
    }
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    junction::create(shared, link).map_err(|e| format!("couldn't link {}: {e}", link.display()))
}

/// Moves everything in `src` into `dst` (same drive, so these are renames).
fn merge_into(src: &Path, dst: &Path, profile: &str) -> Result<(), String> {
    let rd = std::fs::read_dir(src).map_err(|e| e.to_string())?;
    for e in rd.flatten() {
        let from = e.path();
        let to = dst.join(e.file_name());
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if !to.exists() {
            std::fs::rename(&from, &to).map_err(|err| format!("couldn't move {}: {err}", from.display()))?;
        } else if is_dir && to.is_dir() {
            merge_into(&from, &to, profile)?;
            let _ = std::fs::remove_dir(&from);
        } else if !is_dir && same_content(&from, &to) {
            std::fs::remove_file(&from).map_err(|err| err.to_string())?;
        } else {
            let mut keep = to.clone().into_os_string();
            keep.push(format!(".from-{profile}.bak"));
            std::fs::rename(&from, PathBuf::from(keep)).map_err(|err| format!("couldn't move {}: {err}", from.display()))?;
        }
    }
    Ok(())
}

fn same_content(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else { return false };
    ma.len() == mb.len() && std::fs::read(a).ok() == std::fs::read(b).ok()
}

/// The newest non-empty session of a project in the shared folder.
pub fn latest_session(cwd: &str) -> Option<String> {
    let dir = shared_dir(cwd)?;
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let id = name.strip_suffix(".jsonl")?.to_string();
            let meta = e.metadata().ok().filter(|m| m.is_file() && m.len() > 0)?;
            Some((meta.modified().ok()?, id))
        })
        .max()
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_merges_a_profile_folder() {
        let root = std::env::temp_dir().join(format!("ccl-share-test-{}", uuid::Uuid::new_v4()));
        let shared = root.join("_shared");
        let link = root.join("prof").join("projects").join("C--proj");
        std::fs::create_dir_all(shared.join("memory")).unwrap();
        std::fs::create_dir_all(link.join("memory")).unwrap();
        std::fs::write(shared.join("b.jsonl"), "shared session").unwrap();
        std::fs::write(shared.join("memory").join("MEMORY.md"), "shared memory").unwrap();
        std::fs::write(link.join("a.jsonl"), "profile session").unwrap();
        std::fs::write(link.join("b.jsonl"), "shared session").unwrap(); // same content: dropped
        std::fs::write(link.join("memory").join("MEMORY.md"), "profile memory").unwrap(); // clash: kept as .bak

        link_profile(&link, &shared, "prof").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(same_dir(&link, &shared));
        assert_eq!(std::fs::read_to_string(link.join("a.jsonl")).unwrap(), "profile session");
        assert_eq!(std::fs::read_to_string(shared.join("memory").join("MEMORY.md")).unwrap(), "shared memory");
        assert_eq!(std::fs::read_to_string(shared.join("memory").join("MEMORY.md.from-prof.bak")).unwrap(), "profile memory");
        assert!(!shared.join("b.jsonl.from-prof.bak").exists());

        // Writing through the link lands in the shared folder; a second call changes nothing.
        std::fs::write(link.join("c.jsonl"), "new").unwrap();
        assert!(shared.join("c.jsonl").exists());
        link_profile(&link, &shared, "prof").unwrap();

        // A missing profile folder just gets the link.
        let link2 = root.join("prof2").join("projects").join("C--proj");
        link_profile(&link2, &shared, "prof2").unwrap();
        assert!(link2.join("a.jsonl").exists());

        std::fs::remove_dir(&link).unwrap(); // removes only the link
        std::fs::remove_dir(&link2).unwrap();
        assert!(shared.join("a.jsonl").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
