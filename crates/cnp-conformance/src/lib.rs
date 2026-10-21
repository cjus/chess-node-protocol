//! # cnp-conformance: does a node speak CNP?
//!
//! The Chess Node Protocol's conformance sessions (`conformance/` in its repository) are
//! exchanges written as JSON lines: what to send a node, and what must come back. This crate
//! plays them against any node program over its stdin/stdout:
//!
//! ```text
//! cnp-conformance conformance/ -- path/to/node --its --arguments
//! ```
//!
//! It first asks the node what it offers (`initialize`), then runs every session in a fresh
//! node, skipping those that need capabilities the node doesn't offer. A node conforms to CNP
//! 1.0 if it passes every session that applies to it.
//!
//! - [`session`]: the session format, and reading sessions from files;
//! - [`pattern`]: the patterns a session's expected messages are written in;
//! - [`runner`]: starting a node and playing a session against it.
//!
//! The package also has `cnp-example-node`, a node that serves no domain and answers only the
//! lifecycle (`initialize`, `ping`, `shutdown`, `exit`). It is the smallest correct node, an
//! example to start a real one from, and what the lifecycle sessions are tested against here.

pub mod pattern;
pub mod runner;
pub mod session;

/// Where the sessions are: the `conformance` directory of this crate's repository.
///
/// Cargo checks out a git dependency's whole repository, so a node that depends on this crate
/// by git can run every session in its own tests:
///
/// ```no_run
/// use std::path::PathBuf;
/// use cnp_conformance::{SESSIONS, session};
///
/// let sessions = session::load(&[PathBuf::from(SESSIONS)]).unwrap();
/// ```
pub const SESSIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance");
