//! Load a seeded Radicle repo by RID and return a view snapshot.
//! Also clone (seed+fetch) missing RIDs into local storage and checkout to ~/code.

use std::path::{Path, PathBuf};
use std::time::Duration;

use git2::{DiffFormat, DiffOptions, ObjectType};
use radicle::identity::DocAt;
use radicle::node::{ConnectOptions, FetchResult, Handle as _, Node};
use radicle::prelude::RepoId;
use radicle::storage::git::Repository;
use radicle::storage::{ReadRepository, ReadStorage, RepositoryInfo};
use radicle::{rad, Profile};
use thiserror::Error;

use crate::view_api::{CommitRow, FileRow};

const MAX_README: usize = 24_000;
const MAX_BLOB: usize = 200_000;
const MAX_TREE: usize = 64;
const MAX_COMMITS: usize = 32;
const MAX_DIFF: usize = 200_000;
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(9);

#[derive(Debug, Clone)]
pub struct RepoSummary {
    pub rid: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct RepoView {
    pub rid: String,
    pub name: String,
    pub description: String,
    pub head: String,
    pub head_oid: String,
    pub readme: String,
    pub files: Vec<FileRow>,
    pub commits: Vec<CommitRow>,
}

#[derive(Debug, Clone)]
pub struct CloneOutcome {
    pub rid: String,
    pub name: String,
    /// Working copy under ~/code/<name>, when created or already present.
    pub path: PathBuf,
    pub fetched: bool,
    pub checked_out: bool,
}

#[derive(Debug, Error)]
pub enum RadError {
    #[error("load profile: {0}")]
    Profile(String),
    #[error("invalid RID: {0}")]
    Rid(String),
    #[error("open repo: {0}")]
    Open(String),
    #[error("identity: {0}")]
    Identity(String),
    #[error("head: {0}")]
    Head(String),
    #[error("clone: {0}")]
    Clone(String),
    #[error("node not running — start with `rad node start` to clone from the network")]
    NodeOffline,
    #[error("{0}")]
    Other(String),
}

pub fn load_profile() -> Result<Profile, RadError> {
    Profile::load().map_err(|e| RadError::Profile(e.to_string()))
}

/// Local storage inventory (seeded / replicated repos on this node).
pub fn list_local_repos(profile: &Profile) -> Result<Vec<RepoSummary>, RadError> {
    let repos = profile
        .storage
        .repositories()
        .map_err(|e| RadError::Open(e.to_string()))?;
    let mut out = Vec::new();
    for RepositoryInfo { rid, doc, .. } in repos {
        let (name, description) = match doc.project() {
            Ok(p) => (p.name().to_string(), p.description().to_string()),
            Err(_) => (rid.to_string(), String::new()),
        };
        out.push(RepoSummary {
            rid: rid.to_string(),
            name,
            description,
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

fn parse_rid(rid_str: &str) -> Result<RepoId, RadError> {
    rid_str
        .trim()
        .parse()
        .map_err(|e| RadError::Rid(format!("{e}")))
}

fn open_storage(profile: &Profile, rid_str: &str) -> Result<(RepoId, Repository), RadError> {
    let rid = parse_rid(rid_str)?;
    let repo = profile
        .storage
        .repository(rid)
        .map_err(|e| RadError::Open(e.to_string()))?;
    Ok((rid, repo))
}

/// Whether the RID is already present (and readable) in local storage.
pub fn has_local(profile: &Profile, rid_str: &str) -> Result<bool, RadError> {
    let rid = parse_rid(rid_str)?;
    profile
        .storage
        .contains(&rid)
        .map_err(|e| RadError::Open(e.to_string()))
}

fn default_checkout_root() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("code")
}

fn path_is_usable_checkout(path: &Path) -> bool {
    path.join(".git").exists() || path.join("HEAD").exists()
}

fn path_is_empty(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut it) => it.next().is_none(),
        Err(_) => false,
    }
}

/// Seed + fetch a RID into local storage when missing, then checkout (move) to `~/code/<name>`.
pub fn clone_to_local(profile: &Profile, rid_str: &str) -> Result<CloneOutcome, RadError> {
    let rid = parse_rid(rid_str)?;
    let mut fetched = false;

    if !profile
        .storage
        .contains(&rid)
        .map_err(|e| RadError::Open(e.to_string()))?
    {
        fetch_into_storage(profile, rid)?;
        fetched = true;
    }

    let (_, repo) = open_storage(profile, &rid.to_string())?;
    let DocAt { doc, .. } = repo
        .identity_doc()
        .map_err(|e| RadError::Identity(e.to_string()))?;
    let project = doc
        .project()
        .map_err(|e| RadError::Identity(e.to_string()))?;
    let name = project.name().to_string();
    let path = default_checkout_root().join(&name);

    let checked_out = ensure_checkout(profile, rid, &doc, &path)?;

    Ok(CloneOutcome {
        rid: rid.to_string(),
        name,
        path,
        fetched,
        checked_out,
    })
}

fn fetch_into_storage(profile: &Profile, rid: RepoId) -> Result<(), RadError> {
    let mut node = Node::new(profile.socket_from_env());
    if !node.is_running() {
        return Err(RadError::NodeOffline);
    }

    profile
        .seed(rid, radicle::node::policy::Scope::All, &mut node)
        .map_err(|e| RadError::Clone(e.to_string()))?;

    let seeds = node
        .seeds_for(rid, [*profile.id()])
        .map_err(|e| RadError::Clone(e.to_string()))?;
    if seeds.is_empty() {
        return Err(RadError::Clone(format!(
            "no seeds found for {rid} — is the RID seeded on the network?"
        )));
    }

    let (connected, disconnected) = seeds.partition();
    let mut last_err = None;

    for seed in connected {
        match node.fetch(rid, seed.nid, FETCH_TIMEOUT, None) {
            Ok(FetchResult::Success { .. }) => {
                if profile.storage.contains(&rid).unwrap_or(false) {
                    let _ = profile.add_inventory(rid, &mut node);
                    return Ok(());
                }
            }
            Ok(FetchResult::Failed { reason }) => last_err = Some(reason),
            Err(e) => last_err = Some(e.to_string()),
        }
    }

    for seed in disconnected {
        for ka in &seed.addrs {
            let opts = ConnectOptions {
                persistent: false,
                timeout: CONNECT_TIMEOUT,
            };
            match node.connect(seed.nid, ka.addr.clone(), opts) {
                Ok(radicle::node::ConnectResult::Connected) => {}
                Ok(radicle::node::ConnectResult::Disconnected { reason }) => {
                    last_err = Some(reason);
                    continue;
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                    continue;
                }
            }
            match node.fetch(rid, seed.nid, FETCH_TIMEOUT, None) {
                Ok(FetchResult::Success { .. }) => {
                    if profile.storage.contains(&rid).unwrap_or(false) {
                        let _ = profile.add_inventory(rid, &mut node);
                        return Ok(());
                    }
                }
                Ok(FetchResult::Failed { reason }) => last_err = Some(reason),
                Err(e) => last_err = Some(e.to_string()),
            }
        }
    }

    if profile.storage.contains(&rid).unwrap_or(false) {
        let _ = profile.add_inventory(rid, &mut node);
        return Ok(());
    }

    Err(RadError::Clone(last_err.unwrap_or_else(|| {
        format!("could not fetch {rid} from any seed")
    })))
}

fn ensure_checkout(
    profile: &Profile,
    rid: RepoId,
    doc: &radicle::identity::Doc,
    path: &Path,
) -> Result<bool, RadError> {
    if path_is_usable_checkout(path) {
        return Ok(false);
    }
    if path.exists() && !path_is_empty(path) {
        return Err(RadError::Clone(format!(
            "checkout path {} exists and is not empty",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| RadError::Clone(e.to_string()))?;
    }

    let remote = *doc.delegates().first().as_key();
    rad::checkout(rid, &remote, path, &profile.storage, false)
        .map_err(|e| RadError::Clone(e.to_string()))?;
    Ok(true)
}

pub fn open_repo(profile: &Profile, rid_str: &str) -> Result<RepoView, RadError> {
    let (rid, repo) = open_storage(profile, rid_str)?;

    let DocAt { doc, .. } = repo
        .identity_doc()
        .map_err(|e| RadError::Identity(e.to_string()))?;
    let project = doc
        .project()
        .map_err(|e| RadError::Identity(e.to_string()))?;

    let (_, head) = repo.head().map_err(|e| RadError::Head(e.to_string()))?;
    let head_oid = head.to_string();
    let head_short = if head_oid.len() > 8 {
        head_oid[..8].to_string()
    } else {
        head_oid.clone()
    };

    let (readme, files) = readme_and_files(&repo, head)?;
    let commits = list_commits(&repo, head)?;

    Ok(RepoView {
        rid: rid.to_string(),
        name: project.name().to_string(),
        description: project.description().to_string(),
        head: head_short,
        head_oid,
        readme,
        files,
        commits,
    })
}

pub fn read_file(
    profile: &Profile,
    rid: &str,
    rev: &str,
    path: &str,
) -> Result<String, RadError> {
    let (_, repo) = open_storage(profile, rid)?;
    let oid = git2::Oid::from_str(rev).map_err(|e| RadError::Other(e.to_string()))?;
    let commit = repo
        .backend
        .find_commit(oid)
        .map_err(|e| RadError::Other(e.to_string()))?;
    let tree = commit
        .tree()
        .map_err(|e| RadError::Other(e.to_string()))?;
    let entry = tree
        .get_path(Path::new(path))
        .map_err(|e| RadError::Other(format!("{path}: {e}")))?;
    if matches!(entry.kind(), Some(ObjectType::Tree)) {
        return Err(RadError::Other("path is a directory".into()));
    }
    let blob = entry
        .to_object(&repo.backend)
        .and_then(|o| o.peel_to_blob())
        .map_err(|e| RadError::Other(e.to_string()))?;
    if blob.size() > MAX_BLOB {
        return Err(RadError::Other(format!(
            "file too large ({} bytes)",
            blob.size()
        )));
    }
    let bytes = blob.content();
    if bytes.contains(&0) {
        return Ok(format!("(binary file, {} bytes)", bytes.len()));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| RadError::Other("file is not valid UTF-8".into()))?;
    Ok(text.to_string())
}

pub fn commit_paths(profile: &Profile, rid: &str, commit_oid: &str) -> Result<Vec<String>, RadError> {
    let (_, repo) = open_storage(profile, rid)?;
    let diff = tree_diff(&repo.backend, commit_oid)?;
    let mut paths = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.display().to_string());
            if let Some(p) = path {
                if !paths.contains(&p) {
                    paths.push(p);
                }
            }
            true
        },
        None,
        None,
        None,
    )
    .map_err(|e| RadError::Other(e.to_string()))?;
    paths.sort();
    Ok(paths)
}

pub fn file_patch(
    profile: &Profile,
    rid: &str,
    commit_oid: &str,
    path: &str,
) -> Result<String, RadError> {
    let (_, repo) = open_storage(profile, rid)?;
    let mut opts = DiffOptions::new();
    opts.pathspec(path);
    opts.context_lines(3);
    let oid = git2::Oid::from_str(commit_oid).map_err(|e| RadError::Other(e.to_string()))?;
    let commit = repo
        .backend
        .find_commit(oid)
        .map_err(|e| RadError::Other(e.to_string()))?;
    let new_tree = commit
        .tree()
        .map_err(|e| RadError::Other(e.to_string()))?;
    let old_tree = commit
        .parents()
        .next()
        .map(|p| p.tree())
        .transpose()
        .map_err(|e| RadError::Other(e.to_string()))?;

    let diff = repo
        .backend
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))
        .map_err(|e| RadError::Other(e.to_string()))?;

    let mut out = String::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if matches!(origin, '+' | '-' | ' ' | '\\') {
            out.push(origin);
        }
        if let Ok(s) = std::str::from_utf8(line.content()) {
            out.push_str(s);
        }
        if out.len() < MAX_DIFF {
            true
        } else {
            out.push_str("\n… (diff truncated)");
            false
        }
    })
    .map_err(|e| RadError::Other(e.to_string()))?;

    if out.is_empty() {
        out = "(empty patch)".into();
    }
    Ok(out)
}

fn tree_diff<'a>(
    repo: &'a git2::Repository,
    commit_oid: &str,
) -> Result<git2::Diff<'a>, RadError> {
    let oid = git2::Oid::from_str(commit_oid).map_err(|e| RadError::Other(e.to_string()))?;
    let commit = repo
        .find_commit(oid)
        .map_err(|e| RadError::Other(e.to_string()))?;
    let new_tree = commit
        .tree()
        .map_err(|e| RadError::Other(e.to_string()))?;
    let old_tree = commit
        .parents()
        .next()
        .map(|p| p.tree())
        .transpose()
        .map_err(|e| RadError::Other(e.to_string()))?;
    let mut opts = DiffOptions::new();
    opts.patience(true).minimal(true);
    repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))
        .map_err(|e| RadError::Other(e.to_string()))
}

fn readme_and_files(
    repo: &Repository,
    head: radicle::git::Oid,
) -> Result<(String, Vec<FileRow>), RadError> {
    let commit = repo
        .backend
        .find_commit(head.into())
        .map_err(|e| RadError::Head(e.to_string()))?;
    let tree = commit
        .tree()
        .map_err(|e| RadError::Head(e.to_string()))?;

    let files = list_tree_entries(&repo.backend, &tree, "")?;

    let candidates = [
        "README.md",
        "README",
        "README.markdown",
        "README.txt",
        "README.rst",
        "Readme.md",
        "readme.md",
    ];
    let mut readme = String::from("(no README)");
    for path in candidates {
        let Ok(entry) = tree.get_path(Path::new(path)) else {
            continue;
        };
        let Ok(blob) = entry
            .to_object(&repo.backend)
            .and_then(|o| o.peel_to_blob())
        else {
            continue;
        };
        let bytes = blob.content();
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let truncated = if text.len() > MAX_README {
            format!("{}…", &text[..MAX_README])
        } else {
            text.to_string()
        };
        readme = truncated;
        break;
    }

    Ok((readme, files))
}

/// List entries in `dir` (empty string = repo root) at `rev`.
pub fn list_dir(
    profile: &Profile,
    rid: &str,
    rev: &str,
    dir: &str,
) -> Result<Vec<FileRow>, RadError> {
    let (_, repo) = open_storage(profile, rid)?;
    let oid = git2::Oid::from_str(rev).map_err(|e| RadError::Other(e.to_string()))?;
    let commit = repo
        .backend
        .find_commit(oid)
        .map_err(|e| RadError::Other(e.to_string()))?;
    let root = commit
        .tree()
        .map_err(|e| RadError::Other(e.to_string()))?;
    list_tree_entries(&repo.backend, &root, dir)
}

fn list_tree_entries(
    backend: &git2::Repository,
    root: &git2::Tree<'_>,
    dir: &str,
) -> Result<Vec<FileRow>, RadError> {
    let tree = if dir.is_empty() {
        root.clone()
    } else {
        let entry = root
            .get_path(Path::new(dir))
            .map_err(|e| RadError::Other(format!("{dir}: {e}")))?;
        if !matches!(entry.kind(), Some(ObjectType::Tree)) {
            return Err(RadError::Other(format!("{dir} is not a directory")));
        }
        entry
            .to_object(backend)
            .and_then(|o| o.peel_to_tree())
            .map_err(|e| RadError::Other(e.to_string()))?
    };

    let mut files = Vec::new();
    for entry in tree.iter() {
        if files.len() >= MAX_TREE {
            break;
        }
        let name = entry.name().unwrap_or("?").to_string();
        let is_tree = matches!(entry.kind(), Some(ObjectType::Tree));
        files.push(FileRow { name, is_tree });
    }
    files.sort_by(|a, b| match (a.is_tree, b.is_tree) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(files)
}

fn list_commits(
    repo: &Repository,
    head: radicle::git::Oid,
) -> Result<Vec<CommitRow>, RadError> {
    let mut revwalk = repo
        .backend
        .revwalk()
        .map_err(|e| RadError::Head(e.to_string()))?;
    revwalk
        .push(head.into())
        .map_err(|e| RadError::Head(e.to_string()))?;

    let mut commits = Vec::new();
    for oid in revwalk {
        if commits.len() >= MAX_COMMITS {
            break;
        }
        let oid = oid.map_err(|e| RadError::Head(e.to_string()))?;
        let commit = repo
            .backend
            .find_commit(oid)
            .map_err(|e| RadError::Head(e.to_string()))?;
        let msg = commit.message().unwrap_or("").trim();
        let summary = msg.lines().next().unwrap_or("(no message)").to_string();
        let id = commit.id().to_string();
        let short_id = if id.len() > 7 {
            id[..7].to_string()
        } else {
            id.clone()
        };
        let author = commit
            .author()
            .name()
            .unwrap_or("unknown")
            .to_string();
        commits.push(CommitRow {
            id,
            summary,
            short_id,
            author,
        });
    }
    Ok(commits)
}
