//! Vidya UI components for repo view sections.

mod browser;
mod commits;
mod files;
mod meta;
mod readme;
mod repo_list;

pub use browser::{
    IssueStatus, JobStatus, PatchStatus, RepoBrowser, RepoUi, Tab,
};
pub use commits::Commits;
pub use files::Files;
pub use meta::Meta;
pub use readme::Readme;
pub use repo_list::RepoList;
