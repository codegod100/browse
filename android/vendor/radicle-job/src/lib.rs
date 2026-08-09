//! Radicle "job COB".
//!
//! A [`Job`] records results of automated processing of a repository for a
//! given Git commit, by one or more nodes.
//!
//! For any one of these processes there is a corresponding [`Run`]. For
//! example, nodes that run CI for a repository might add `Run` to a `Job` for a
//! specific commit. If there are several nodes running CI, they would each add
//! their own `Run` to the same `Job`.
//!
//! This checkout vendors a read-only subset for browse (list/inspect jobs).

#![deny(missing_docs)]

use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use indexmap::IndexMap;
use log::{debug, warn};
use once_cell::sync::Lazy;
use radicle::cob;
use radicle::cob::store::{access, Cob};
use radicle::cob::{store, Evaluate, ObjectId, Op, TypeName};
use radicle::node::NodeId;
use radicle::prelude::ReadRepository;
use radicle::storage::RepositoryError;
use radicle::{cob::store::CobAction, git::Oid};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

pub mod display;
pub mod error;

/// Type name of a patch.
pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radworks.job").expect("type name is valid"));

/// The identifier for a given [`Job`] collaborative object.
///
/// Identifiers can be used to retrieve a [`Job`] through [`Jobs::get`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JobId(ObjectId);

impl JobId {
    fn as_object_id(&self) -> &ObjectId {
        &self.0
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_string())
    }
}

impl FromStr for JobId {
    type Err = <ObjectId as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ObjectId::from_str(s).map(Self)
    }
}

impl From<JobId> for ObjectId {
    fn from(JobId(oid): JobId) -> Self {
        oid
    }
}

impl From<ObjectId> for JobId {
    fn from(oid: ObjectId) -> Self {
        Self(oid)
    }
}

/// A `Job` describes a generic task run for a given commit [`Oid`] by a set of
/// nodes.
///
/// A node may run the task many times, which are accumulated in its [`Runs`]
/// set. Each [`Run`] is identified by a [`Uuid`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    oid: Oid,
    runs: HashMap<NodeId, Runs>,
}

/// A set of [`Run`]s identified by a [`Uuid`].
///
/// Iteration over this set is guaranteed to come in insertion order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runs(IndexMap<Uuid, Run>);

impl Runs {
    /// Insert a new [`Run`], identified by the given [`Uuid`].
    pub fn insert(&mut self, uuid: Uuid, run: Run) -> Option<Run> {
        self.0.insert(uuid, run)
    }

    /// Check that the set of runs contains the given [`Uuid`].
    pub fn contains_key(&self, uuid: &Uuid) -> bool {
        self.0.contains_key(uuid)
    }

    /// Get the [`Run`] identifier by the given [`Uuid`].
    ///
    /// Return `None` if the `Uuid` does not exist.`
    pub fn get(&self, uuid: &Uuid) -> Option<&Run> {
        self.0.get(uuid)
    }

    /// Get the `nth` [`Run`] of the set.
    pub fn get_index(&self, nth: usize) -> Option<(&Uuid, &Run)> {
        self.0.get_index(nth)
    }

    /// Get the latest [`Run`] and its corresponding [`Uuid`].
    pub fn latest(&self) -> Option<(&Uuid, &Run)> {
        self.0.iter().next_back()
    }

    /// Get all [`Run`]s that have started [`Status`].
    pub fn started(&self) -> Runs {
        self.iter()
            .filter_map(|(uuid, run)| run.is_started().then_some((*uuid, run.clone())))
            .collect()
    }

    /// Get all [`Run`]s that have finished [`Status`].
    pub fn finished(&self) -> Runs {
        self.iter()
            .filter_map(|(uuid, run)| run.is_finished().then_some((*uuid, run.clone())))
            .collect()
    }

    /// Get all [`Run`]s that have finished [`Status`] and have the succeeded
    /// [`Reason`].
    pub fn succeeded(&self) -> Runs {
        self.iter()
            .filter_map(|(uuid, run)| run.succeeded().then_some((*uuid, run.clone())))
            .collect()
    }

    /// Get all [`Run`]s that have finished [`Status`] and have the failed
    /// [`Reason`].
    pub fn failed(&self) -> Runs {
        self.iter()
            .filter_map(|(uuid, run)| run.failed().then_some((*uuid, run.clone())))
            .collect()
    }

    /// Partition all [`Run`]s into started, succeeded, and failed.
    pub fn partition(&self) -> (Runs, Runs, Runs) {
        let mut started = IndexMap::new();
        let mut succeeded = IndexMap::new();
        let mut failed = IndexMap::new();

        for (uuid, run) in self.0.iter() {
            match run.status {
                Status::Started => started.insert(*uuid, run.clone()),
                Status::Finished(Reason::Succeeded) => succeeded.insert(*uuid, run.clone()),
                Status::Finished(Reason::Failed) => failed.insert(*uuid, run.clone()),
            };
        }
        (Runs(started), Runs(succeeded), Runs(failed))
    }

    /// Check is the set of [`Runs`] empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the number of [`Runs`] in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Iterate over the [`Run`]s and their corresponding [`Uuid`]s.
    ///
    /// The order of the iteration is guaranteed to be insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&Uuid, &Run)> {
        self.0.iter()
    }
}

impl FromIterator<(Uuid, Run)> for Runs {
    fn from_iter<T: IntoIterator<Item = (Uuid, Run)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a> IntoIterator for &'a Runs {
    type Item = (&'a Uuid, &'a Run);
    type IntoIter = indexmap::map::Iter<'a, Uuid, Run>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for Runs {
    type Item = (Uuid, Run);
    type IntoIter = indexmap::map::IntoIter<Uuid, Run>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// The collaborative object actions that are used for Radicle COB operations.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    /// Request a [`Job`] be created for the given [`Oid`].
    ///
    /// Note this is expected only once for initializing the [`Job`]. Every
    /// other `Request` for the same `Oid` will be ignored.
    Request {
        /// The commit the [`Job`] corresponds to.
        oid: Oid,
    },
    /// Notify that the node has started a [`Run`].
    Run {
        /// The [`Uuid`] that identifies this particular [`Run`] of the node.
        uuid: Uuid,
        /// The [`Url`] where the node will log any information or data.
        log: Url,
    },
    /// Notify that the node has finished a [`Run`].
    Finished {
        /// The [`Uuid`] that identifies the [`Run`] that is finished.
        uuid: Uuid,
        /// The [`Reason`] which the node finished with.
        reason: Reason,
    },
}

impl CobAction for Action {
    fn parents(&self) -> Vec<radicle::git::Oid> {
        match self {
            Action::Request { oid } => vec![*oid],
            _ => Vec::new(),
        }
    }
}

impl Job {
    /// Construct a new [`Job`].
    fn new(oid: Oid) -> Self {
        Self {
            oid,
            runs: HashMap::new(),
        }
    }

    /// Get the [`Oid`] this [`Job`] is running tasks for.
    pub fn oid(&self) -> &Oid {
        &self.oid
    }

    /// Get all the nodes that have started, but not finished, [`Runs`].
    pub fn started(&self) -> HashMap<NodeId, Runs> {
        self.filter_map_by(|runs| runs.started())
    }

    /// Get all the nodes that have started, and finished, [`Runs`].
    pub fn finished(&self) -> HashMap<NodeId, Runs> {
        self.filter_map_by(|runs| runs.finished())
    }

    /// Get all the nodes that have succeeded [`Runs`].
    pub fn succeeded(&self) -> HashMap<NodeId, Runs> {
        self.filter_map_by(|runs| runs.succeeded())
    }

    /// Get all the nodes that have failed [`Runs`].
    pub fn failed(&self) -> HashMap<NodeId, Runs> {
        self.filter_map_by(|runs| runs.failed())
    }

    /// Get all nodes' started, succeeded, and failed runs – respectively.
    pub fn partition(&self) -> HashMap<NodeId, (Runs, Runs, Runs)> {
        self.runs
            .iter()
            .map(|(node, runs)| (*node, runs.partition()))
            .collect()
    }

    /// Get the latest [`Run`] of the given [`NodeId`].
    pub fn latest_of(&self, node: &NodeId) -> Option<(&Uuid, &Run)> {
        self.runs
            .get(node)
            .and_then(|runs| runs.0.iter().next_back())
    }

    /// Get the latest [`Run`]s of all [`NodeId`]s.
    pub fn latest(&self) -> impl Iterator<Item = (&NodeId, &Uuid, &Run)> + '_ {
        self.runs
            .iter()
            .filter_map(|(node, runs)| runs.latest().map(|(uuid, run)| (node, uuid, run)))
    }

    /// Get the raw `HashMap` of the node runs.
    pub fn runs(&self) -> &HashMap<NodeId, Runs> {
        &self.runs
    }

    /// Get the [`Runs`] of a given [`NodeId`].
    pub fn runs_of(&self, node: &NodeId) -> Option<&Runs> {
        self.runs.get(node)
    }

    fn filter_map_by<P>(&self, p: P) -> HashMap<NodeId, Runs>
    where
        P: Fn(&Runs) -> Runs,
    {
        self.runs
            .iter()
            .filter_map(|(node, runs)| {
                let runs = p(runs);
                (!runs.is_empty()).then_some((*node, runs))
            })
            .collect()
    }

    fn insert(&mut self, node: NodeId, uuid: Uuid, run: Run) -> bool {
        let runs = self.runs.entry(node).or_default();
        if runs.contains_key(&uuid) {
            warn!("Run {uuid} already exists for node {node}, skipping");
            false
        } else {
            debug!("Inserting run {uuid} for node {node}");
            runs.insert(uuid, run);
            true
        }
    }

    fn update(
        &mut self,
        node: NodeId,
        uuid: Uuid,
        reason: Reason,
        timestamp: cob::Timestamp,
    ) -> bool {
        let Some(runs) = self.runs.get_mut(&node) else {
            warn!("Cannot finish run {uuid}: node {node} has no runs");
            return false;
        };
        let mut updated = false;
        runs.0.entry(uuid).and_modify(|run| {
            debug!("Finishing run {uuid} for node {node} with {reason:?}");
            updated = true;
            *run = run.clone().finish(reason, timestamp);
        });
        if !updated {
            warn!("Cannot finish run {uuid}: not found for node {node}");
        }
        updated
    }

    fn action(&mut self, node: NodeId, action: Action, timestamp: cob::Timestamp) {
        match action {
            // Cannot request for another `oid`, so we ignore any superfluous
            // request actions
            Action::Request { .. } => {
                debug!("Ignoring redundant Request action from {node}");
            }
            Action::Run { uuid, log } => {
                self.insert(node, uuid, Run::new(log, timestamp));
            }
            Action::Finished { uuid, reason } => {
                self.update(node, uuid, reason, timestamp);
            }
        }
    }
}

impl store::CobWithType for Job {
    fn type_name() -> &'static TypeName {
        &TYPENAME
    }
}

impl store::Cob for Job {
    type Action = Action;
    type Error = error::Build;

    fn from_root<R: ReadRepository>(op: Op<Self::Action>, repo: &R) -> Result<Self, Self::Error> {
        let mut actions = op.actions.into_iter();
        let Some(Action::Request { oid }) = actions.next() else {
            return Err(error::Build::Initial);
        };
        debug!(
            "Initializing job from root: oid={oid}, author={}",
            op.author
        );
        repo.commit(oid)
            .map_err(|err| error::Build::MissingCommit { oid, err })?;
        let mut runs = Self::new(oid);
        let actions: Vec<_> = actions.collect();
        debug!("Processing {} additional actions from root", actions.len());
        for action in actions {
            runs.action(op.author, action, op.timestamp);
        }
        Ok(runs)
    }

    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a radicle::cob::Entry>>(
        &mut self,
        op: Op<Self::Action>,
        _concurrent: I,
        _repo: &R,
    ) -> Result<(), Self::Error> {
        debug!(
            "Applying op from {} with {} actions",
            op.author,
            op.actions.len()
        );
        for action in op.actions {
            self.action(op.author, action, op.timestamp);
        }
        Ok(())
    }
}

impl<R: ReadRepository> Evaluate<R> for Job {
    type Error = error::Apply;

    fn init(entry: &radicle::cob::Entry, store: &R) -> Result<Self, Self::Error> {
        let op = Op::try_from(entry)?;
        let object = Job::from_root(op, store)?;
        Ok(object)
    }

    fn apply<'a, I: Iterator<Item = (&'a Oid, &'a radicle::cob::Entry)>>(
        &mut self,
        entry: &radicle::cob::Entry,
        concurrent: I,
        store: &R,
    ) -> Result<(), Self::Error> {
        let op = Op::try_from(entry)?;
        self.op(op, concurrent.map(|(_, e)| e), store)
            .map_err(error::Apply::from)
    }
}

/// A `Run` represents a task run for a [`Job`]. A `Run` is initially created in
/// with a [`Status::Started`], before processing has started.
///
/// The `Run` can then be marked as [`Status::Finished`] with a given
/// [`Reason`].
///
/// The `Run` also contains a [`Url`] so that any extra metadata, for example
/// logs, can be tracked outside of the `Run` itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// The status of the run.
    ///
    /// A run can be in one of three states:
    ///   - Started
    ///   - Succeeded – implies that it has finished
    ///   - Failed – implies that it has finished
    status: Status,
    /// The [`Url`] of the [`Run`] where information is logged by the node.
    log: Url,
    /// The timestamp of the last operation to affect the run.
    timestamp: cob::Timestamp,
}

impl Run {
    /// Create a new `Run` with a [`Url`] that ideally points to the log of that
    /// process.
    pub fn new(log: Url, timestamp: cob::Timestamp) -> Self {
        Self {
            status: Status::Started,
            log,
            timestamp,
        }
    }

    /// Mark the `Run` as finished.
    fn finish(self, reason: Reason, timestamp: cob::Timestamp) -> Self {
        Self {
            status: Status::Finished(reason),
            log: self.log,
            timestamp,
        }
    }

    /// Return URL for the log of this run.
    pub fn log(&self) -> &Url {
        &self.log
    }

    /// Return latest [`Status`] of the `Run`.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Return the timestamp of the last update to the `Run`.
    pub fn timestamp(&self) -> &cob::Timestamp {
        &self.timestamp
    }

    /// Returns `true` if the status of the `Run` is [`Status::Started`].
    pub fn is_started(&self) -> bool {
        match self.status {
            Status::Started => true,
            Status::Finished(_) => false,
        }
    }

    /// Returns `true` if the status of the `Run` is [`Status::Finished`].
    pub fn is_finished(&self) -> bool {
        !self.is_started()
    }

    /// Returns `true` if the status of the `Run` is [`Status::Finished`] and
    /// the reason for finishing is [`Reason::Succeeded`].
    pub fn succeeded(&self) -> bool {
        match self.status {
            Status::Started => false,
            Status::Finished(Reason::Failed) => false,
            Status::Finished(Reason::Succeeded) => true,
        }
    }

    /// Returns `true` if the status of the `Run` is [`Status::Finished`] and
    /// the reason for finishing is [`Reason::Failed`].
    pub fn failed(&self) -> bool {
        match self.status {
            Status::Started => false,
            Status::Finished(Reason::Failed) => true,
            Status::Finished(Reason::Succeeded) => false,
        }
    }
}

/// The status of a [`Run`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    /// The [`Run`] has started.
    Started,
    /// The [`Run`] has finished with the given [`Reason`].
    Finished(Reason),
}

/// The reason for a [`Status`] to have finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Reason {
    /// The [`Run`] finished and failed.
    Failed,
    /// The [`Run`] finished and succeeded.
    Succeeded,
}

/// The storage for all [`Job`] items.
///
/// To get a handle for [`Jobs`] use [`Jobs::open`].
///
/// The read-only operations for [`Jobs`] are:
///
///   - [`Jobs::counts`]
///   - [`Jobs::get`]
///
pub struct Jobs<'a, R, Access> {
    raw: store::Store<'a, Job, R, Access>,
}

impl<'a, R, A> Deref for Jobs<'a, R, A> {
    type Target = store::Store<'a, Job, R, A>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a, R, A> Jobs<'a, R, A>
where
    R: ReadRepository + cob::Store<Namespace = NodeId>,
    A: access::Access + 'a,
{
    /// Open a jobs store.
    pub fn open(repository: &'a R, access: A) -> Result<Self, RepositoryError> {
        let identity = repository.identity_head()?;
        let raw = store::Store::open_for(&TYPENAME, repository, access)?.identity(identity);

        Ok(Self { raw })
    }

    /// Return the number of [`Job`]s in the store.
    pub fn counts(&self) -> Result<usize, store::Error> {
        Ok(self.all()?.count())
    }

    /// Get a [`Job`], given its [`JobId`] identifier.
    pub fn get(&self, id: &JobId) -> Result<Option<Job>, store::Error> {
        self.raw.get(id.as_object_id())
    }

    /// Find the [`Job`]s that are associated with the `wanted` commit.
    pub fn find_by_commit(&self, wanted: Oid) -> Result<FindByCommit<'a>, store::Error> {
        FindByCommit::new(self, wanted)
    }
}


impl<'a, R> Jobs<'a, R, access::ReadOnly>
where
    R: ReadRepository + cob::Store<Namespace = NodeId>,
{
    /// Open a read-only jobs store.
    pub fn open_readonly(repository: &'a R) -> Result<Self, RepositoryError> {
        Self::open(repository, access::ReadOnly)
    }
}

/// [`Iterator`] for finding each [`Job`] where the [`Job::oid`] matches the
/// wanted commit. See [`Jobs::find_by_commit`].
pub struct FindByCommit<'a> {
    jobs: Box<dyn Iterator<Item = Result<(ObjectId, Job), cob::store::Error>> + 'a>,
    needle: Oid,
}

impl<'a> FindByCommit<'a> {
    fn new<R, A>(jobs: &Jobs<'a, R, A>, needle: Oid) -> Result<Self, cob::store::Error>
    where
        R: ReadRepository + cob::Store<Namespace = NodeId>,
        A: access::Access + 'a,
    {
        Ok(Self {
            jobs: Box::new(jobs.all()?),
            needle,
        })
    }

    fn wanted(&self, job: &Job) -> bool {
        self.needle == *job.oid()
    }
}

impl Iterator for FindByCommit<'_> {
    type Item = Result<(JobId, Job), cob::store::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let job = self.jobs.next()?;
            match job {
                Ok((id, job)) if self.wanted(&job) => return Some(Ok((JobId::from(id), job))),
                Ok(_) => continue,
                Err(err) => return Some(Err(err)),
            }
        }
    }
}
