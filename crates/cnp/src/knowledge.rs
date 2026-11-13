//! Proposed draft-1 knowledge extension: puzzles, evaluated positions and strategic test
//! positions from outside sources, which a client hands to the node as downloaded files
//! (`spec/knowledge.md`). It doesn't change the CNP 1.0 transport.
//!
//! Draft 1 is the import half: [`StartImport`] gives the node a file and a [`Manifest`] saying
//! where it is from, answers at once, and the import runs on; [`ListImports`] reports where
//! each has got to, and [`CancelImport`] stops one. Reading what was imported is the next
//! draft.
//!
//! **The node doesn't download.** The client fetches the file, where it can show the licence,
//! the size and the progress. The node verifies the file against the manifest's checksum before
//! reading a record ([`State::Verifying`]), so a damaged or swapped download is
//! [`FailureCode::Checksum`], not imported.
//!
//! **Pitfall: a counted record is a committed record.** [`Import`]'s counts are what has been
//! durably stored, so a client that shows progress shows what survives a crash.
//!
//! **Pitfall: variants aren't versions.** A kind's variants (a puzzle database's releases, the
//! Strategic Test Suite's differently scored files) are different sources, told apart by
//! [`Import::checksum`]; a node never converts one's scores to another's scale.
use crate::RequestMethod;
use serde::{Deserialize, Serialize};

/// Review identifier, in [`Capabilities::contract`]. Clients must check it before sending
/// knowledge requests.
pub const CONTRACT: &str = "draft-1";
/// Kind: Lichess's puzzle database (CSV).
pub const KIND_LICHESS_PUZZLES: &str = "lichess-puzzles";
/// Kind: Lichess's evaluations (JSON Lines).
pub const KIND_LICHESS_EVALS: &str = "lichess-evals";
/// Kind: a Strategic Test Suite EPD file.
pub const KIND_STS_EPD: &str = "sts-epd";
/// The longest [`ImportParams::path`], in bytes.
pub const MAX_PATH_BYTES: usize = 4096;
/// The longest manifest text field, in bytes.
pub const MAX_FIELD_BYTES: usize = 1024;
/// The length of a checksum: SHA-256 in hexadecimal.
pub const CHECKSUM_LEN: usize = 64;
/// The most imports one [`ListImports`] answer holds: the newest.
pub const MAX_LISTED: usize = 100;
/// Storage or I/O failed.
pub const STORAGE_FAILURE: i64 = -32022;
/// No import has the identity asked for.
pub const NOT_FOUND: i64 = -32025;
/// The file doesn't exist, isn't a file, or can't be read: no import started.
pub const FILE_UNREADABLE: i64 = -32027;

/// The optional knowledge domain. Missing switches mean false.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// `knowledge/import`, `knowledge/status` and `knowledge/cancel` are available.
    #[serde(default)]
    pub import: bool,
    /// The kinds of file the node can import ([`KIND_LICHESS_PUZZLES`] and the like).
    #[serde(default)]
    pub kinds: Vec<String>,
    /// The contract the node implements, [`CONTRACT`]; absent means don't send knowledge
    /// requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

/// Where a file is from, kept by the node with everything it imports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// The source, such as `Lichess puzzles`.
    pub name: String,
    /// Its release, such as `2026-11`.
    pub release: String,
    /// Where it was downloaded from.
    pub url: String,
    /// When it was downloaded: UTC, RFC 3339 with milliseconds.
    pub retrieved_at: String,
    /// SHA-256 of the file as downloaded (the compressed file, for `.zst`): 64 hexadecimal
    /// digits, either case.
    pub checksum: String,
    /// An SPDX identifier, such as `CC0-1.0`.
    pub license: String,
    /// What to credit, to show wherever the data is used.
    pub attribution: String,
}

/// Whether `text` is a field of a manifest: 1 to [`MAX_FIELD_BYTES`] bytes, no control
/// characters.
fn valid_field(text: &str) -> bool {
    (1..=MAX_FIELD_BYTES).contains(&text.len()) && !text.chars().any(char::is_control)
}

/// Whether `text` is a checksum: [`CHECKSUM_LEN`] hexadecimal digits.
pub fn valid_checksum(text: &str) -> bool {
    text.len() == CHECKSUM_LEN && text.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Manifest {
    /// Checks the manifest's shapes, or says what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        for (name, text) in [
            ("name", &self.name),
            ("release", &self.release),
            ("url", &self.url),
            ("retrievedAt", &self.retrieved_at),
            ("license", &self.license),
            ("attribution", &self.attribution),
        ] {
            if !valid_field(text) {
                return Err(format!(
                    "a manifest's {name} is 1 to {MAX_FIELD_BYTES} bytes without control characters"
                ));
            }
        }
        if !valid_checksum(&self.checksum) {
            return Err(format!(
                "a manifest's checksum is {CHECKSUM_LEN} hexadecimal digits (SHA-256)"
            ));
        }
        Ok(())
    }
}

/// `knowledge/import`'s params.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportParams {
    /// The kind of file ([`KIND_LICHESS_PUZZLES`] and the like): one the node lists.
    pub kind: String,
    /// An absolute path, on the computer the node runs on, of the file as downloaded. The node
    /// only reads it.
    pub path: String,
    /// Where the file is from.
    pub manifest: Manifest,
}

impl ImportParams {
    /// Checks the params' shapes (not whether the node offers the kind), or says what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        if self.kind.is_empty()
            || self.kind.len() > 32
            || !self
                .kind
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err("a kind is up to 32 lowercase letters, digits and hyphens".into());
        }
        if self.path.is_empty() || self.path.len() > MAX_PATH_BYTES || self.path.contains('\0') {
            return Err(format!("a path is 1 to {MAX_PATH_BYTES} bytes without NUL"));
        }
        if !is_absolute(&self.path) {
            return Err("a path is absolute".into());
        }
        self.manifest.validate()
    }
}

/// Whether `path` is absolute on Unix (`/…`) or Windows (`C:\…`, `C:/…`, `\\server\…`).
fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'))
}

/// Where an import is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// The node is reading the whole file to check its checksum, before any record.
    Verifying,
    /// Records are being read and stored.
    Importing,
    /// The file was read to its end.
    Complete,
    /// Stopped by a client or the node's shutdown, or found interrupted when the node started.
    /// Importing the file again resumes it.
    Cancelled,
    /// Stopped by a failure ([`Import::failure`]). Importing the file again resumes it.
    Failed,
}

/// Why an import failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureCode {
    /// The file isn't the one the manifest describes.
    Checksum,
    /// The file stopped being readable.
    Unreadable,
    /// The file isn't of the kind, or all of it was rejected.
    Format,
    /// There isn't enough free space.
    Disk,
    /// Reading the file failed part-way, as a truncated download does.
    Stream,
    /// The node's own storage failed.
    Storage,
}

/// A failed import's reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// The reason, for a client to act on.
    pub code: FailureCode,
    /// The reason, for a person.
    pub message: String,
}

/// An import: one file of one kind, and what has become of it so far.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    /// The kind of file.
    pub kind: String,
    /// The file's checksum, from its manifest: with the kind, the import's identity.
    pub checksum: String,
    /// The manifest's `name`.
    pub name: String,
    /// The manifest's `release`.
    pub release: String,
    /// Where it is.
    pub state: State,
    /// Records read so far, the rejected included: `accepted + rejected + duplicate`.
    pub read: u64,
    /// Records stored.
    pub accepted: u64,
    /// Records found invalid, kept apart by the node and not stored as knowledge.
    pub rejected: u64,
    /// Records already stored.
    pub duplicate: u64,
    /// Why it failed, when [`State::Failed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
    /// When it started: UTC, RFC 3339 with milliseconds.
    pub started_at: String,
    /// When it completed, once [`State::Complete`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

/// The result of `knowledge/import` and `knowledge/cancel`: the import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportResult {
    /// The import, as it is when the node answers.
    pub import: Import,
}

/// Starts an import of a downloaded file, or resumes or reports the one with the same kind and
/// checksum. Answers at once; the import runs on. One import runs at a time: another is Busy
/// (-32013). A file that can't be read is [`FILE_UNREADABLE`].
pub enum StartImport {}
impl RequestMethod for StartImport {
    const NAME: &'static str = "knowledge/import";
    type Params = ImportParams;
    type Result = ImportResult;
}

/// `knowledge/status`'s params: both fields optional, and the params may be left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<StatusFields>")]
pub struct StatusParams {
    /// Only imports of this kind.
    pub kind: Option<String>,
    /// Only the import with this checksum.
    pub checksum: Option<String>,
}

/// [`StatusParams`] as they are read, from an object or from nothing.
#[derive(Default, Deserialize)]
struct StatusFields {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    checksum: Option<String>,
}

impl From<Option<StatusFields>> for StatusParams {
    fn from(fields: Option<StatusFields>) -> StatusParams {
        let fields = fields.unwrap_or_default();
        StatusParams {
            kind: fields.kind,
            checksum: fields.checksum,
        }
    }
}

/// `knowledge/status`'s result: the imports, newest first, at most [`MAX_LISTED`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResult {
    /// The imports.
    pub imports: Vec<Import>,
}

/// Reports imports. With a kind and a checksum, that import ([`NOT_FOUND`] if there is none).
pub enum ListImports {}
impl RequestMethod for ListImports {
    const NAME: &'static str = "knowledge/status";
    type Params = StatusParams;
    type Result = StatusResult;
}

/// `knowledge/cancel`'s params: the import's identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelParams {
    /// The kind of file.
    pub kind: String,
    /// The file's checksum.
    pub checksum: String,
}

/// Stops an import at its next batch and answers when it has stopped. What it committed stays.
/// Cancelling one that isn't running answers with its state unchanged.
pub enum CancelImport {}
impl RequestMethod for CancelImport {
    const NAME: &'static str = "knowledge/cancel";
    type Params = CancelParams;
    type Result = ImportResult;
}
