//! Display forms of Job data.
//!
//! These can be used in tools that wish to display data, such as the `rad-job`
//! CLI tool.

use chrono::Utc;
use radicle::{cob, git::Oid, node::NodeId};
use serde::Serialize;
use url::Url;
use uuid::Uuid;

use crate::{JobId, Status};

/// A set of [`Job`]s, and how many of them there are, which sorted by their
/// [`JobId`].
#[derive(Serialize)]
pub struct Jobs {
    count: usize,
    jobs: Vec<Job>,
}

impl Jobs {
    /// Construct the set of [`Jobs`] given an iterator of [`JobId`] and
    /// [`Job`][job] pairs.
    ///
    /// [job]: crate::Job.
    pub fn new(jobs: impl Iterator<Item = (JobId, crate::Job)>) -> Self {
        let mut jobs = jobs.map(|(id, job)| Job::new(id, &job)).collect::<Vec<_>>();
        jobs.sort_by_cached_key(|job| job.job_id);

        Self {
            count: jobs.len(),
            jobs,
        }
    }

    /// Pretty print the set of [`Jobs`] and their count.
    pub fn pretty(&self) -> String {
        fn line(s: &mut String, line: String) {
            s.push_str(&line);
            s.push('\n');
        }

        let mut s = String::new();

        line(&mut s, format!("count: {}", self.count));
        for shown in self.jobs.iter() {
            s.push_str(&shown.pretty());
            s.push('\n');
        }

        s
    }
}

/// A display form of a [`Job`][job].
///
/// [job]: crate::Job.
#[derive(Serialize)]
pub struct Job {
    job_id: JobId,
    oid: Oid,
    runs: Vec<Runs>,
}

impl Job {
    /// Construct a new [`Job`] given the [`JobId`] and collaborative object
    /// form of the [`Job`][job].
    ///
    /// [job]: crate::Job.
    pub fn new(job_id: JobId, job: &crate::Job) -> Self {
        let mut runs: Vec<_> = job
            .runs()
            .iter()
            .map(|(node_id, runs)| {
                let mut runs: Vec<Run> = runs
                    .iter()
                    .map(|(run_id, run)| Run {
                        run_id: *run_id,
                        status: *run.status(),
                        log: run.log().clone(),
                        timestamp: *run.timestamp(),
                    })
                    .collect();
                runs.sort_by_cached_key(|run| run.run_id);
                Runs {
                    node_id: *node_id,
                    runs,
                }
            })
            .collect();
        runs.sort_by_cached_key(|run| run.node_id);

        Self {
            job_id,
            oid: *job.oid(),
            runs,
        }
    }

    /// Pretty print the set of [`Jobs`] and their count.
    pub fn pretty(&self) -> String {
        fn line(s: &mut String, line: String) {
            s.push_str(&line);
            s.push('\n');
        }

        let mut s = String::new();

        line(&mut s, format!("job {} (commit {})", self.job_id, self.oid));
        for run in self.runs.iter() {
            line(&mut s, format!("  node {}", run.node_id));
            for run2 in run.runs.iter() {
                let date =
                    chrono::DateTime::<Utc>::from_timestamp_secs(run2.timestamp.as_secs() as i64)
                        .expect("run timestamp is out of range");
                line(
                    &mut s,
                    format!("    run {} {:?} [{}]", run2.run_id, run2.status, date),
                );
                line(&mut s, format!("      log  {}", run2.log));
            }
        }

        s
    }
}

#[derive(Serialize)]
struct Runs {
    node_id: NodeId,
    runs: Vec<Run>,
}

#[derive(Serialize)]
struct Run {
    run_id: Uuid,
    status: Status,
    log: Url,
    timestamp: cob::Timestamp,
}
