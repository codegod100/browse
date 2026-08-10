//! Load a seeded Radicle repo by RID and return a view snapshot.
//! Also create a local Radicle profile when none exists (APK / fresh installs).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use git2::{DiffFormat, DiffOptions, ObjectType};
use radicle::cob::issue::Issues as IssueStore;
use radicle::cob::patch::Patches as PatchStore;
use radicle::cob::store::access::ReadOnly;
use radicle::crypto::ssh::Passphrase;
use radicle::crypto::Seed;
use radicle::identity::DocAt;
use radicle::node::Alias;
use radicle::prelude::RepoId;
use radicle::profile::{self, Profile};
use radicle::storage::git::Repository;
use radicle::storage::{ReadRepository, ReadStorage, RepositoryInfo};
use thiserror::Error;

use crate::view_api::{CommitRow, FileRow, IssueRow, JobRow, JobRunRow, PatchRow};

const MAX_README: usize = 24_000;
const MAX_BLOB: usize = 200_000;
const MAX_TREE: usize = 64;
const MAX_COMMITS: usize = 32;
const MAX_DIFF: usize = 200_000;
const MAX_PATCHES: usize = 64;
const MAX_ISSUES: usize = 64;
const MAX_JOBS: usize = 64;
const MAX_DESC: usize = 8_000;

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
    pub patches: Vec<PatchRow>,
    pub issues: Vec<IssueRow>,
    pub jobs: Vec<JobRow>,
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
    #[error("{0}")]
    Other(String),
}

pub fn load_profile() -> Result<Profile, RadError> {
    Profile::load().map_err(|e| RadError::Profile(e.to_string()))
}

/// Display path for the Radicle home (`RAD_HOME`, else `~/.radicle`).
pub fn home_display() -> String {
    match profile::home() {
        Ok(home) => home.path().display().to_string(),
        Err(_) => std::env::var_os("RAD_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".radicle")))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.radicle".into()),
    }
}

/// Default node alias: `$USER` / `$USERNAME` when valid, else `browse`.
pub fn default_alias() -> String {
    std::env::var("USER")
        .ok()
        .or_else(|| std::env::var("USERNAME").ok())
        .and_then(|u| Alias::from_str(&u).ok().map(|a| a.to_string()))
        .unwrap_or_else(|| "browse".into())
}

fn generate_seed() -> Result<Seed, RadError> {
    if let Some(seed) = profile::env::seed() {
        return Ok(seed);
    }
    let mut bytes = [0u8; Seed::BYTES];
    // `/dev/urandom` is available on Linux and Android NativeActivity hosts.
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| RadError::Profile(format!("generate key seed: {e}")))?;
    Ok(Seed::new(bytes))
}

/// Create a new Radicle profile at `$RAD_HOME` / `~/.radicle` (like `rad auth`).
///
/// Empty `passphrase` leaves the key unencrypted (typical for the APK).
pub fn create_profile(alias: &str, passphrase: Option<&str>) -> Result<Profile, RadError> {
    let alias = Alias::from_str(alias.trim()).map_err(|e| RadError::Profile(e.to_string()))?;
    let home = profile::home().map_err(|e| RadError::Profile(e.to_string()))?;
    let passphrase = passphrase
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| Passphrase::from(s.to_owned()));
    let seed = generate_seed()?;
    Profile::init(home, alias, passphrase, seed).map_err(|e| RadError::Profile(e.to_string()))
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

fn open_storage(profile: &Profile, rid_str: &str) -> Result<(RepoId, Repository), RadError> {
    let rid: RepoId = rid_str
        .trim()
        .parse()
        .map_err(|e| RadError::Rid(format!("{e}")))?;
    let repo = profile
        .storage
        .repository(rid)
        .map_err(|e| RadError::Open(e.to_string()))?;
    Ok((rid, repo))
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
    // Prefer live git COB stores so Open / tab re-press reloads pick up newly
    // synced patches & issues even when the SQLite COB cache is stale.
    let patches = list_patches(&repo).unwrap_or_default();
    let issues = list_issues(&repo).unwrap_or_default();
    let jobs = list_jobs(&repo).unwrap_or_default();

    Ok(RepoView {
        rid: rid.to_string(),
        name: project.name().to_string(),
        description: project.description().to_string(),
        head: head_short,
        head_oid,
        readme,
        files,
        commits,
        patches,
        issues,
        jobs,
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

fn list_patches(repo: &Repository) -> Result<Vec<PatchRow>, RadError> {
    let patches = PatchStore::open(repo, ReadOnly).map_err(|e| RadError::Other(e.to_string()))?;
    let mut rows = Vec::new();
    for item in patches
        .all()
        .map_err(|e| RadError::Other(e.to_string()))?
    {
        if rows.len() >= MAX_PATCHES {
            break;
        }
        let (id, patch) = item.map_err(|e| RadError::Other(e.to_string()))?;
        let id_str = id.to_string();
        rows.push(PatchRow {
            short_id: short_oid(&id_str),
            id: id_str,
            title: patch.title().to_string(),
            state: patch.state().to_string(),
            author: short_did(&patch.author().to_string()),
            description: truncate_desc(patch.description()),
            head: patch.head().to_string(),
            base: patch.base().to_string(),
            revisions: patch.revisions().count(),
            updated_ms: patch.updated_at().as_millis() as u64,
        });
    }
    rows.sort_by(|a, b| {
        open_first(&a.state, &b.state).then_with(|| b.updated_ms.cmp(&a.updated_ms))
    });
    Ok(rows)
}

fn list_issues(repo: &Repository) -> Result<Vec<IssueRow>, RadError> {
    let issues = IssueStore::open(repo, ReadOnly).map_err(|e| RadError::Other(e.to_string()))?;
    let mut rows = Vec::new();
    for item in issues
        .all()
        .map_err(|e| RadError::Other(e.to_string()))?
    {
        if rows.len() >= MAX_ISSUES {
            break;
        }
        let (id, issue) = item.map_err(|e| RadError::Other(e.to_string()))?;
        let id_str = id.to_string();
        let replies = issue.replies().count();
        rows.push(IssueRow {
            short_id: short_oid(&id_str),
            id: id_str,
            title: issue.title().to_string(),
            state: issue.state().to_string(),
            author: short_did(&issue.author().to_string()),
            description: truncate_desc(issue.description()),
            replies,
            updated_ms: issue.timestamp().as_millis() as u64,
        });
    }
    rows.sort_by(|a, b| {
        open_first(&a.state, &b.state).then_with(|| b.updated_ms.cmp(&a.updated_ms))
    });
    Ok(rows)
}

fn list_jobs(repo: &Repository) -> Result<Vec<JobRow>, RadError> {
    use radicle_job::{Jobs, Status};

    let jobs = Jobs::open_readonly(repo).map_err(|e| RadError::Other(e.to_string()))?;
    let mut rows = Vec::new();

    for item in jobs
        .all()
        .map_err(|e| RadError::Other(e.to_string()))?
    {
        if rows.len() >= MAX_JOBS {
            break;
        }
        let (oid, job) = item.map_err(|e| RadError::Other(e.to_string()))?;
        let id = oid.to_string();
        let commit = job.oid().to_string();

        let mut runs = Vec::new();
        let mut updated_secs = 0u64;
        let mut latest_status = "none".to_string();
        let mut latest_ts = 0u64;

        for (node, node_runs) in job.runs() {
            for (run_id, run) in node_runs.iter() {
                let ts = run.timestamp().as_secs();
                if ts >= latest_ts {
                    latest_ts = ts;
                    latest_status = match run.status() {
                        Status::Started => "started".into(),
                        Status::Finished(radicle_job::Reason::Succeeded) => "succeeded".into(),
                        Status::Finished(radicle_job::Reason::Failed) => "failed".into(),
                    };
                }
                updated_secs = updated_secs.max(ts);
                runs.push(JobRunRow {
                    node: short_nid(&node.to_string()),
                    run_id: run_id.to_string(),
                    status: match run.status() {
                        Status::Started => "started".into(),
                        Status::Finished(radicle_job::Reason::Succeeded) => "succeeded".into(),
                        Status::Finished(radicle_job::Reason::Failed) => "failed".into(),
                    },
                    log: run.log().to_string(),
                    timestamp_secs: ts,
                });
            }
        }

        runs.sort_by(|a, b| b.timestamp_secs.cmp(&a.timestamp_secs));

        rows.push(JobRow {
            short_id: short_oid(&id),
            id,
            short_commit: short_oid(&commit),
            commit,
            status: latest_status,
            run_count: runs.len(),
            node_count: job.runs().len(),
            updated_secs,
            runs,
        });
    }

    rows.sort_by(|a, b| {
        b.updated_secs
            .cmp(&a.updated_secs)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(rows)
}

fn open_first(a: &str, b: &str) -> std::cmp::Ordering {
    (b == "open").cmp(&(a == "open"))
}

fn short_oid(id: &str) -> String {
    if id.len() > 7 {
        id[..7].to_string()
    } else {
        id.to_string()
    }
}

fn short_did(did: &str) -> String {
    // Prefer the multibase key suffix over the did:key: prefix.
    let key = did.strip_prefix("did:key:").unwrap_or(did);
    if key.len() > 12 {
        format!("{}…{}", &key[..6], &key[key.len() - 4..])
    } else {
        key.to_string()
    }
}

fn short_nid(nid: &str) -> String {
    let s = nid.strip_prefix("did:key:").unwrap_or(nid);
    if s.len() > 12 {
        format!("{}…", &s[..12])
    } else {
        s.to_string()
    }
}

fn truncate_desc(text: &str) -> String {
    let text = text.trim();
    if text.len() > MAX_DESC {
        format!("{}…", &text[..MAX_DESC])
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Profile APIs touch process-wide env; serialize tests that mutate RAD_HOME.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn create_profile_roundtrip() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "browse-rad-profile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: guarded by ENV_LOCK; no other test touches RAD_HOME concurrently.
        unsafe {
            std::env::set_var("RAD_HOME", &dir);
        }

        assert!(load_profile().is_err());
        let profile = create_profile("browse-test", None).expect("create profile");
        assert_eq!(profile.config.alias().as_str(), "browse-test");
        let loaded = load_profile().expect("load after create");
        assert_eq!(loaded.id(), profile.id());

        unsafe {
            std::env::remove_var("RAD_HOME");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_profile_rejects_empty_alias() {
        let err = create_profile("  ", None).unwrap_err();
        assert!(err.to_string().contains("alias") || err.to_string().contains("empty"));
    }
}
