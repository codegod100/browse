//! Errors that are captured for job related actions.

use radicle::{cob, git};
use thiserror::Error;

/// Errors that can occur when building a [`Job`][job].
///
/// [job]: super::Job
#[derive(Debug, Error)]
pub enum Build {
    /// The initial action in the history of the [`Job`][job] was not a
    /// [`Request`][req].
    ///
    /// [job]: super::Job
    /// [req]: super::Action::Request
    #[error("initial action of job must request an OID")]
    Initial,
    /// The [`Request`][req] referred to a commit that could not be found.
    ///
    /// [req]: super::Action::Request
    #[error("missing commit for job run {oid}: {err}")]
    MissingCommit {
        /// The [`Oid`][oid] of the commit that was requested, but is missing.
        ///
        /// [oid]: git::Oid
        oid: git::Oid,
        /// The underlying error from Git that occurred.
        #[source]
        err: git::raw::Error,
    },
}

/// Errors that can occur when applying an [`Entry`][entry] to the [`Job`][job]
/// collaborative object.
///
/// [entry]: radicle::cob::Entry
/// [job]: super::Job
#[derive(Debug, Error)]
pub enum Apply {
    /// Applying the entry resulted in a [`Build`] error.
    #[error(transparent)]
    Build(#[from] Build),
    /// Error occurred when decoding an [`Entry`][entry] into an [`Op`][op].
    ///
    /// [entry]: radicle::cob::Entry
    /// [op]: radicle::cob::Op
    #[error(transparent)]
    Op(#[from] cob::op::OpEncodingError),
}
