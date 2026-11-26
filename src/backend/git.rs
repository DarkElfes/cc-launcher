//! Git state of a project folder for the side panel. Read-only: never changes the repo.

use super::types::{GitChange, GitInfo, GitStatus};
use super::util::run_hidden;
use std::collections::BTreeMap;

fn git_binary() -> String {
    let bundled = r"C:\Program Files\Git\cmd\git.exe";
    if std::path::Path::new(bundled).exists() { bundled.into() } else { "git".into() }
}

fn git(cwd: &str, args: &[&str]) -> Option<String> {
    let mut all = vec!["-c", "core.quotepath=false", "-C", cwd];
    all.extend_from_slice(args);
    let out = run_hidden(&git_binary(), &all, None, None, std::time::Duration::from_secs(15))?;
    if out.status_ok { Some(out.stdout) } else { None }
}

fn status_of(code: char) -> GitStatus {
    match code {
        'A' | 'C' => GitStatus::Added,
        'D' => GitStatus::Deleted,
        'R' => GitStatus::Renamed,
        'U' => GitStatus::Conflict,
        '?' => GitStatus::Untracked,
        _ => GitStatus::Modified,
    }
}

pub fn get_git_info(cwd: &str) -> GitInfo {
    if git(cwd, &["rev-parse", "--is-inside-work-tree"]).is_none() {
        return GitInfo::default();
    }
    let mut info = GitInfo { is_repo: true, ..Default::default() };
    let status = git(cwd, &["status", "--porcelain=v1", "--branch", "--untracked-files=all"]).unwrap_or_default();
    let ahead_re = regex::Regex::new(r"ahead (\d+)").unwrap();
    let behind_re = regex::Regex::new(r"behind (\d+)").unwrap();
    let mut files: BTreeMap<String, GitChange> = BTreeMap::new();
    let mut order: Vec<String> = vec![];
    for line in status.lines().filter(|l| !l.is_empty()) {
        if let Some(head) = line.strip_prefix("## ") {
            let b = head.split("...").next().unwrap_or("");
            let b = b.strip_prefix("No commits yet on ").unwrap_or(b);
            info.branch = b.split(' ').next().map(String::from);
            info.ahead = ahead_re.captures(head).and_then(|c| c[1].parse().ok()).unwrap_or(0);
            info.behind = behind_re.captures(head).and_then(|c| c[1].parse().ok()).unwrap_or(0);
            continue;
        }
        if line.len() < 4 {
            continue;
        }
        let xy: Vec<char> = line.chars().take(2).collect();
        let mut path = line[3..].to_string();
        if let Some((_, to)) = path.split_once(" -> ") {
            path = to.to_string();
        }
        let path = path.trim_matches('"').to_string();
        let code = if xy == ['?', '?'] { '?' } else { xy.iter().find(|c| **c != ' ').copied().unwrap_or('M') };
        if !files.contains_key(&path) {
            order.push(path.clone());
        }
        files.insert(
            path.clone(),
            GitChange { path, status: status_of(code), staged: xy[0] != ' ' && xy[0] != '?', added: None, removed: None },
        );
    }
    // Line counts vs HEAD (staged + unstaged). Fails in a repo without commits; that's fine.
    if let Some(numstat) = git(cwd, &["diff", "--numstat", "HEAD"]) {
        for l in numstat.lines().filter(|l| !l.is_empty()) {
            let mut parts = l.splitn(3, '\t');
            let (a, r, p) = (parts.next().unwrap_or("0"), parts.next().unwrap_or("0"), parts.next().unwrap_or(""));
            let p = p.rsplit("=> ").next().unwrap_or(p).replace(['{', '}'], "");
            let ai: u64 = a.parse().unwrap_or(0);
            let ri: u64 = r.parse().unwrap_or(0);
            if let Some(f) = files.get_mut(&p) {
                f.added = Some(ai);
                f.removed = Some(ri);
            }
            info.added += ai;
            info.removed += ri;
        }
    }
    if let Some(out) = git(cwd, &["log", "-1", "--format=%h%x09%s%x09%ct"]) {
        let parts: Vec<&str> = out.trim().split('\t').collect();
        if parts.len() == 3 && !parts[0].is_empty() {
            info.last_commit = Some((parts[0].into(), parts[1].into(), parts[2].parse::<i64>().unwrap_or(0) * 1000));
        }
    }
    info.files = order.into_iter().filter_map(|p| files.remove(&p)).collect();
    info
}
