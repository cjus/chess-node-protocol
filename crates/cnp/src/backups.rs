//! Proposed draft-1 backups extension: snapshots of everything a node keeps for a program
//! (its preferences and its games), listed, made on request, and restored
//! (`spec/backups.md`). It doesn't change the CNP 1.0 transport.
//!
//! A client lists the backups ([`List`]), asks for one now, or for the day's automatic one
//! ([`Create`]), and restores one ([`Restore`]). Where the backups are kept is the node's
//! launch configuration, never a request's: a request names a backup by the identity the
//! node gave it.
//!
//! **A restore can be undone.** Before it restores a backup, a node backs up what it keeps
//! then ([`Kind::BeforeRestore`]), so restoring that one brings it all back.
//!
//! **Pitfall: revisions only rise.** A restore is itself a change to the preferences: their
//! revision goes one above the revision before the restore, with the backup's values. A
//! client that has seen the later revisions then takes the restored preferences as the newest
//! (`preferences::Snapshot::revision`), where the backup's own revision would look stale.
use crate::RequestMethod;
use crate::method::NoParams;
use serde::{Deserialize, Serialize};

/// Review identifier, in [`Capabilities::contract`]. Clients must check it before sending
/// backups requests.
pub const CONTRACT: &str = "draft-1";
/// The longest backup identity, in bytes.
pub const MAX_ID_BYTES: usize = 64;
/// The most backups one [`List`] answer holds: the newest.
pub const MAX_LISTED: usize = 100;
/// Storage failed; a backup may or may not have been made, so list them again.
pub const STORAGE_FAILURE: i64 = -32022;
/// No backup has the identity asked for.
pub const NOT_FOUND: i64 = -32025;
/// The backup doesn't check out (it is damaged, or not one this node can read): nothing was
/// restored.
pub const UNUSABLE: i64 = -32026;

/// The optional backups domain. Missing switches mean false.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// `backups/list` is available ([`List`]).
    #[serde(default)]
    pub list: bool,
    /// `backups/create` is available ([`Create`]).
    #[serde(default)]
    pub create: bool,
    /// `backups/restore` is available ([`Restore`]).
    #[serde(default)]
    pub restore: bool,
    /// How many automatic backups the node keeps, the newest, if it deletes older ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_automatic: Option<u32>,
    /// The contract the node implements, [`CONTRACT`]; absent means don't send backups
    /// requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

/// Why a backup was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// The day's backup, which a client asks for as it starts ([`CreateParams::automatic`]).
    Automatic,
    /// One the player asked for.
    Manual,
    /// What the node kept just before a restore, so the restore can be undone.
    BeforeRestore,
}

/// A backup, as a list shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    /// Its identity, the node's choice: 1 to [`MAX_ID_BYTES`] lowercase letters, digits and
    /// hyphens.
    pub id: String,
    /// Why it was made.
    pub kind: Kind,
    /// When it was made: UTC, RFC 3339 with milliseconds, like `2026-10-25T16:40:12.123Z`.
    pub created_at: String,
    /// How large it is on disk, in bytes.
    pub bytes: u64,
    /// How many games it holds.
    pub games: u64,
    /// How many preferences it holds: the ones set, not the defaults.
    pub settings: u64,
}

/// Whether `id` is a valid backup identity.
pub fn valid_id(id: &str) -> bool {
    (1..=MAX_ID_BYTES).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `backups/list`'s result: the backups, newest first, at most [`MAX_LISTED`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListResult {
    /// The backups.
    pub backups: Vec<Backup>,
}

/// Lists the backups the node keeps, newest first. Its params may be left out.
pub enum List {}
impl RequestMethod for List {
    const NAME: &'static str = "backups/list";
    type Params = NoParams;
    type Result = ListResult;
}

/// `backups/create`'s params. They may be left out, for a backup the player asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<CreateFields>")]
pub struct CreateParams {
    /// The day's automatic backup: made only if the newest automatic one is older than the
    /// node's interval (chessdb's is 20 hours); otherwise that one is the answer.
    #[serde(default)]
    pub automatic: bool,
}

/// [`CreateParams`] as they are read, from an object or from nothing.
#[derive(Default, Deserialize)]
struct CreateFields {
    #[serde(default)]
    automatic: bool,
}

impl From<Option<CreateFields>> for CreateParams {
    fn from(fields: Option<CreateFields>) -> CreateParams {
        CreateParams {
            automatic: fields.unwrap_or_default().automatic,
        }
    }
}

/// `backups/create`'s result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Created {
    /// The backup made; or, for an automatic one not due, the newest automatic one.
    pub backup: Backup,
    /// Whether a backup was made by this request.
    pub made: bool,
}

/// Makes a backup of everything the node keeps, complete and checked before it is listed or
/// answered, or answers with the newest automatic one when an automatic backup isn't due.
pub enum Create {}
impl RequestMethod for Create {
    const NAME: &'static str = "backups/create";
    type Params = CreateParams;
    type Result = Created;
}

/// `backups/restore`'s params.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreParams {
    /// The backup to restore.
    pub id: String,
}

/// `backups/restore`'s result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restored {
    /// The backup restored.
    pub restored: Backup,
    /// What the node kept just before, backed up ([`Kind::BeforeRestore`]): restoring it
    /// undoes this restore.
    pub before: Backup,
}

/// Restores everything the node keeps to how it was in a backup, after backing up what it
/// keeps now; answers after both are durable. A backup that doesn't check out is
/// [`UNUSABLE`], and one the node doesn't have [`NOT_FOUND`]: nothing changes.
pub enum Restore {}
impl RequestMethod for Restore {
    const NAME: &'static str = "backups/restore";
    type Params = RestoreParams;
    type Result = Restored;
}
