//! Recently viewed repos — MRU list persisted under XDG config.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::rad::RepoSummary;

const MAX_RECENT: usize = 10;
const FILE_NAME: &str = "recent.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentRepo {
    pub rid: String,
    pub name: String,
    pub description: String,
    pub opened_at: u64,
}

impl From<&RecentRepo> for RepoSummary {
    fn from(r: &RecentRepo) -> Self {
        RepoSummary {
            rid: r.rid.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `~/.config/browse/recent.json` (or `$XDG_CONFIG_HOME/browse/recent.json`).
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".config");
                p
            })
        })?;
    Some(base.join("browse").join(FILE_NAME))
}

pub fn load() -> Vec<RecentRepo> {
    let Some(path) = config_path() else {
        return Vec::new();
    };
    let Ok(bytes) = fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice::<Vec<RecentRepo>>(&bytes).unwrap_or_default()
}

pub fn save(repos: &[RecentRepo]) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(repos) {
        let _ = fs::write(path, bytes);
    }
}

/// Insert or move `repo` to the front (most recent). Caps at [`MAX_RECENT`].
pub fn record(list: &mut Vec<RecentRepo>, rid: &str, name: &str, description: &str) {
    list.retain(|r| r.rid != rid);
    list.insert(
        0,
        RecentRepo {
            rid: rid.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            opened_at: now_secs(),
        },
    );
    list.truncate(MAX_RECENT);
}

/// Filter recent entries by name/RID/description substring (same rules as local inventory).
pub fn filter<'a>(repos: &'a [RecentRepo], query: &str) -> Vec<&'a RecentRepo> {
    let q = query.trim().to_lowercase();
    repos
        .iter()
        .filter(|r| {
            if q.is_empty() {
                return true;
            }
            r.name.to_lowercase().contains(&q)
                || r.rid.to_lowercase().contains(&q)
                || r.description.to_lowercase().contains(&q)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_dedupes_and_mru() {
        let mut list = Vec::new();
        record(&mut list, "rad:z1", "alpha", "a");
        record(&mut list, "rad:z2", "beta", "b");
        record(&mut list, "rad:z1", "alpha", "a");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].rid, "rad:z1");
        assert_eq!(list[1].rid, "rad:z2");
    }

    #[test]
    fn record_caps_at_max() {
        let mut list = Vec::new();
        for i in 0..(MAX_RECENT + 5) {
            record(&mut list, &format!("rad:z{i}"), &format!("r{i}"), "");
        }
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].rid, format!("rad:z{}", MAX_RECENT + 4));
    }

    #[test]
    fn filter_matches_name_rid_description() {
        let list = vec![
            RecentRepo {
                rid: "rad:zabc".into(),
                name: "Browse".into(),
                description: "viewer".into(),
                opened_at: 1,
            },
            RecentRepo {
                rid: "rad:zxyz".into(),
                name: "Other".into(),
                description: "".into(),
                opened_at: 2,
            },
        ];
        assert_eq!(filter(&list, "browse").len(), 1);
        assert_eq!(filter(&list, "zxyz").len(), 1);
        assert_eq!(filter(&list, "viewer").len(), 1);
        assert_eq!(filter(&list, "").len(), 2);
        assert!(filter(&list, "nope").is_empty());
    }
}
