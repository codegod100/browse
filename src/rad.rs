//! Load a seeded Radicle repo by RID and return a view snapshot.

use radicle::identity::DocAt;
use radicle::prelude::RepoId;
use radicle::storage::git::Repository;
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::Profile;
use thiserror::Error;

const MAX_README: usize = 12_000;
const MAX_TREE: usize = 64;

#[derive(Debug, Clone)]
pub struct RepoView {
    pub rid: String,
    pub name: String,
    pub description: String,
    pub head: String,
    pub readme: String,
    pub tree: Vec<String>,
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
}

pub fn load_profile() -> Result<Profile, RadError> {
    Profile::load().map_err(|e| RadError::Profile(e.to_string()))
}

pub fn open_repo(profile: &Profile, rid_str: &str) -> Result<RepoView, RadError> {
    let rid: RepoId = rid_str
        .trim()
        .parse()
        .map_err(|e| RadError::Rid(format!("{e}")))?;

    let repo = profile
        .storage
        .repository(rid)
        .map_err(|e| RadError::Open(e.to_string()))?;

    let DocAt { doc, .. } = repo
        .identity_doc()
        .map_err(|e| RadError::Identity(e.to_string()))?;
    let project = doc
        .project()
        .map_err(|e| RadError::Identity(e.to_string()))?;

    let (_, head) = repo
        .head()
        .map_err(|e| RadError::Head(e.to_string()))?;
    let head_s = head.to_string();
    let head_short = if head_s.len() > 8 {
        head_s[..8].to_string()
    } else {
        head_s.clone()
    };

    let (readme, tree) = readme_and_tree(&repo, head)?;

    Ok(RepoView {
        rid: rid.to_string(),
        name: project.name().to_string(),
        description: project.description().to_string(),
        head: head_short,
        readme,
        tree,
    })
}

fn readme_and_tree(
    repo: &Repository,
    head: radicle::git::Oid,
) -> Result<(String, Vec<String>), RadError> {
    let commit = repo
        .backend
        .find_commit(head.into())
        .map_err(|e| RadError::Head(e.to_string()))?;
    let tree = commit
        .tree()
        .map_err(|e| RadError::Head(e.to_string()))?;

    let mut entries = Vec::new();
    for entry in tree.iter() {
        if entries.len() >= MAX_TREE {
            break;
        }
        let name = entry.name().unwrap_or("?").to_string();
        entries.push(name);
    }
    entries.sort();

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
        let Ok(entry) = tree.get_path(std::path::Path::new(path)) else {
            continue;
        };
        let Ok(blob) = entry
            .to_object(&repo.backend)
            .and_then(|o| o.peel_to_blob())
        else {
            continue;
        };
        let bytes = blob.content();
        let text = match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let truncated = if text.len() > MAX_README {
            format!("{}…", &text[..MAX_README])
        } else {
            text.to_string()
        };
        readme = truncated;
        break;
    }

    Ok((readme, entries))
}
