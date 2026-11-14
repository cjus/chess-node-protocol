//! Proposed draft-1 knowledge extension: puzzles, evaluated positions and strategic test
//! positions from outside sources, which a client hands to the node as downloaded files
//! (`spec/knowledge.md`). It doesn't change the CNP 1.0 transport.
//!
//! The import half: [`StartImport`] gives the node a file and a [`Manifest`] saying where it is
//! from, answers at once, and the import runs on; [`ListImports`] reports where each has got
//! to, and [`CancelImport`] stops one. The read half: [`FindPuzzles`] by theme and rating (easiest
//! first, or from a random point, and only the reliable ones if asked), [`FindPuzzleFacets`]
//! for how many there are of each theme and rating band, [`FindAnalyses`] of a position, and
//! [`FindExercises`] by theme, each from complete imports only and naming its [`Source`]s.
//!
//! **The node doesn't download.** The client fetches the file, where it can show the licence,
//! the size and the progress. The node verifies the file against the manifest's checksum before
//! reading a record ([`State::Verifying`]), so a damaged or swapped download is
//! [`FailureCode::Checksum`], not imported.
//!
//! **Pitfall: a counted record is a committed record.** [`Import`]'s counts are what has been
//! durably stored, so a client that shows progress shows what survives a crash.
//!
//! **Pitfall: apply an answer only to the position it is of.** [`AnalysesResult::position`] is
//! the position as the node identifies it; a client compares it with its own current position
//! before it shows the answer.
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
/// The most items a read answers with, and the largest `limit`.
pub const MAX_LIMIT: u32 = 100;
/// How many items a read answers with when `limit` is left out.
pub const DEFAULT_LIMIT: u32 = 20;
/// The longest theme, in bytes.
pub const MAX_THEME_BYTES: usize = 64;
/// The longest cursor, in bytes.
pub const MAX_CURSOR_BYTES: usize = 256;
/// The longest FEN a read takes, in bytes.
pub const MAX_FEN_BYTES: usize = 128;
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
    /// `knowledge/puzzles`, `knowledge/analyses` and `knowledge/exercises` are available.
    #[serde(default)]
    pub read: bool,
    /// `knowledge/puzzles` takes `order`, `seed` and `quality`, and `knowledge/puzzleFacets`
    /// is available. A client sends none of them to a node that doesn't offer it.
    #[serde(default)]
    pub puzzle_selection: bool,
    /// `knowledge/puzzles` takes `daily` and `opening`, and `knowledge/puzzleFacets` answers
    /// with `openings`. Needs [`Capabilities::puzzle_selection`]. A client sends none of them
    /// to a node that doesn't offer it.
    #[serde(default)]
    pub puzzle_filters: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Only the import with this checksum.
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// One imported file, as a read names where its items came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// The file's checksum: what an item's `source` says.
    pub checksum: String,
    /// The kind of file.
    pub kind: String,
    /// The manifest's `name`.
    pub name: String,
    /// The manifest's `release`.
    pub release: String,
    /// The manifest's `license`.
    pub license: String,
    /// The manifest's `attribution`: shown wherever the source's data is.
    pub attribution: String,
}

/// Whether `text` is a theme: 1 to [`MAX_THEME_BYTES`] letters and digits.
pub fn valid_theme(text: &str) -> bool {
    (1..=MAX_THEME_BYTES).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The longest an opening is, in bytes.
pub const MAX_OPENING_BYTES: usize = 128;

/// Whether `text` is an opening as the source writes it: 1 to [`MAX_OPENING_BYTES`] letters,
/// digits, underscores and hyphens, as Lichess's `Sicilian_Defense`.
pub fn valid_opening(text: &str) -> bool {
    (1..=MAX_OPENING_BYTES).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Checks a read's `limit`, `after` and `theme`, or says what is wrong.
fn valid_page(
    limit: Option<u32>,
    after: &Option<String>,
    theme: &Option<String>,
) -> Result<(), String> {
    if limit.is_some_and(|l| !(1..=MAX_LIMIT).contains(&l)) {
        return Err(format!("a limit is 1 to {MAX_LIMIT}"));
    }
    if after.as_ref().is_some_and(|a| {
        a.is_empty() || a.len() > MAX_CURSOR_BYTES || a.chars().any(char::is_control)
    }) {
        return Err("not a cursor the node gave".into());
    }
    if theme.as_ref().is_some_and(|t| !valid_theme(t)) {
        return Err(format!(
            "a theme is 1 to {MAX_THEME_BYTES} letters and digits"
        ));
    }
    Ok(())
}

/// The order `knowledge/puzzles` answers in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Order {
    /// By rating, lowest first, then by identity: the same puzzles in the same order every
    /// time. The default.
    #[default]
    Easiest,
    /// From a point the `seed` picks, in the node's own storage order, wrapping round to the
    /// start, so every matching puzzle comes once. Not a shuffle: see the spec.
    Random,
}

/// A named filter on how far a puzzle's rating can be trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    /// Puzzles the source's own counts support: played enough, liked, and with a settled
    /// rating. The node chooses the thresholds and says them in [`QualityThresholds`].
    Reliable,
}

/// The thresholds a node used for a [`Quality`] filter, so a client can show them and a
/// puzzle's absence from a result can be explained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityThresholds {
    /// At least this many plays.
    pub min_plays: i64,
    /// At least this popularity.
    pub min_popularity: i64,
    /// At most this rating deviation.
    pub max_rating_deviation: i64,
}

/// `knowledge/puzzles`' params: all optional, and the params may be left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<PuzzlesFields>", rename_all = "camelCase")]
pub struct PuzzlesParams {
    /// Only puzzles with this theme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Only puzzles rated at least this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_rating: Option<u32>,
    /// Only puzzles rated at most this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rating: Option<u32>,
    /// Where the page starts: a `next` the node gave.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// How many, 1 to [`MAX_LIMIT`]; [`DEFAULT_LIMIT`] if left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The order: [`Order::Easiest`] if left out. Needs [`Capabilities::puzzle_selection`]
    /// for anything else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<Order>,
    /// Where a [`Order::Random`] pass starts, so a session can be repeated: the node picks one
    /// and answers with it if this is left out. Only with [`Order::Random`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    /// Only puzzles that pass this filter. Needs [`Capabilities::puzzle_selection`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
    /// Only puzzles of this opening family, as the source writes it (`Sicilian_Defense`).
    /// Needs [`Capabilities::puzzle_filters`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening: Option<String>,
    /// Only puzzles that were the source's puzzle of the day. Needs
    /// [`Capabilities::puzzle_filters`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily: Option<bool>,
}

/// [`PuzzlesParams`] as they are read, from an object or from nothing.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PuzzlesFields {
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    min_rating: Option<u32>,
    #[serde(default)]
    max_rating: Option<u32>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    order: Option<Order>,
    #[serde(default)]
    seed: Option<u32>,
    #[serde(default)]
    quality: Option<Quality>,
    #[serde(default)]
    opening: Option<String>,
    #[serde(default)]
    daily: Option<bool>,
}

impl From<Option<PuzzlesFields>> for PuzzlesParams {
    fn from(fields: Option<PuzzlesFields>) -> PuzzlesParams {
        let f = fields.unwrap_or_default();
        PuzzlesParams {
            theme: f.theme,
            min_rating: f.min_rating,
            max_rating: f.max_rating,
            after: f.after,
            limit: f.limit,
            order: f.order,
            seed: f.seed,
            quality: f.quality,
            opening: f.opening,
            daily: f.daily,
        }
    }
}

impl PuzzlesParams {
    /// Checks the params' shapes, or says what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        valid_page(self.limit, &self.after, &self.theme)?;
        if self.seed.is_some() && self.order != Some(Order::Random) {
            return Err("a seed is for order random".into());
        }
        if let (Some(min), Some(max)) = (self.min_rating, self.max_rating)
            && min > max
        {
            return Err("minRating is no more than maxRating".into());
        }
        if self.opening.as_ref().is_some_and(|o| !valid_opening(o)) {
            return Err(format!(
                "an opening is 1 to {MAX_OPENING_BYTES} letters, digits, underscores and hyphens"
            ));
        }
        Ok(())
    }
}

/// A puzzle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Puzzle {
    /// The source's identity for it.
    pub id: String,
    /// The checksum of the [`Source`] it came from.
    pub source: String,
    /// The position the solver is given: the setup with the first of `moves` played.
    pub fen: String,
    /// The position before the opponent's move, as the source gave it.
    pub setup_fen: String,
    /// All the moves in UCI as the source wrote them: the first is the opponent's, the second
    /// begins the solution.
    pub moves: Vec<String>,
    /// The source's rating.
    pub rating: i64,
    /// The source's rating deviation.
    pub rating_deviation: i64,
    /// The source's popularity, 100 (best) to -100.
    pub popularity: i64,
    /// How many times it was played.
    pub plays: i64,
    /// Its themes, as the source writes them.
    pub themes: Vec<String>,
    /// The game it comes from, when the source gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_url: Option<String>,
    /// Its opening, when the source gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opening_tags: Option<String>,
}

/// `knowledge/puzzles`' result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PuzzlesResult {
    /// The puzzles, in the order asked for.
    pub puzzles: Vec<Puzzle>,
    /// The sources of these puzzles.
    pub sources: Vec<Source>,
    /// Where the next page starts, if more may follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// For [`Order::Random`]: the seed the pass started from, the one asked for or the one the
    /// node picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    /// When a [`Quality`] filter was asked for: the thresholds it used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<QualityThresholds>,
}

/// Puzzles from complete imports, by theme and rating, easiest first or from a random point.
pub enum FindPuzzles {}
impl RequestMethod for FindPuzzles {
    const NAME: &'static str = "knowledge/puzzles";
    type Params = PuzzlesParams;
    type Result = PuzzlesResult;
}

/// `knowledge/puzzleFacets`' params: may be left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<FacetsFields>", rename_all = "camelCase")]
pub struct FacetsParams {
    /// Count only puzzles that pass this filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
}

/// [`FacetsParams`] as they are read, from an object or from nothing.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FacetsFields {
    #[serde(default)]
    quality: Option<Quality>,
}

impl From<Option<FacetsFields>> for FacetsParams {
    fn from(fields: Option<FacetsFields>) -> FacetsParams {
        FacetsParams {
            quality: fields.unwrap_or_default().quality,
        }
    }
}

/// A rating band the node counts puzzles in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    /// The lowest rating in the band.
    pub min_rating: u32,
    /// The highest rating in the band; absent for the last, which has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rating: Option<u32>,
}

/// How many puzzles a theme has in each [`Band`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeCounts {
    /// The theme, as the source writes it.
    pub theme: String,
    /// One count for each band of [`FacetsResult::bands`], in its order.
    pub counts: Vec<u64>,
}

/// How many puzzles an opening family has in each [`Band`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningCounts {
    /// The opening family, as the source writes it.
    pub opening: String,
    /// One count for each band of [`FacetsResult::bands`], in its order.
    pub counts: Vec<u64>,
}

/// `knowledge/puzzleFacets`' result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacetsResult {
    /// The rating bands the counts are in, lowest first. A client uses these, never its own.
    pub bands: Vec<Band>,
    /// All puzzles in each band, whatever their themes.
    pub total: Vec<u64>,
    /// Each theme that has at least one puzzle, with its counts.
    pub themes: Vec<ThemeCounts>,
    /// The sources of the puzzles counted.
    pub sources: Vec<Source>,
    /// When a [`Quality`] filter was asked for: the thresholds it used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<QualityThresholds>,
    /// Each opening family that has at least one puzzle, with its counts. Only from a node
    /// that offers [`Capabilities::puzzle_filters`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<OpeningCounts>,
}

/// How many puzzles each theme has in each rating band, from complete imports.
pub enum FindPuzzleFacets {}
impl RequestMethod for FindPuzzleFacets {
    const NAME: &'static str = "knowledge/puzzleFacets";
    type Params = FacetsParams;
    type Result = FacetsResult;
}

/// `knowledge/analyses`' params.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysesParams {
    /// A position of four to six fields; counters are ignored.
    pub fen: String,
    /// How many analyses, 1 to [`MAX_LIMIT`]; [`DEFAULT_LIMIT`] if left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl AnalysesParams {
    /// Checks the params' shapes (not whether the position is legal), or says what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        valid_page(self.limit, &None, &None)?;
        let fields = self.fen.split_ascii_whitespace().count();
        if self.fen.len() > MAX_FEN_BYTES || !matches!(fields, 4 | 6) {
            return Err("a FEN has four or six fields".into());
        }
        Ok(())
    }
}

/// Whose point of view a score is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Perspective {
    /// White's.
    White,
    /// The side to move's.
    SideToMove,
    /// The source doesn't say. Neither sign is to be assumed.
    Unspecified,
}

/// A principal variation of an analysis: a score and the moves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// Centipawns, if it isn't a mate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cp: Option<i64>,
    /// Moves to mate, if it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mate: Option<i64>,
    /// The moves in UCI, space-separated, as the source wrote them.
    pub moves: String,
}

/// One analysis of a position: a source's search, with its lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Analysis {
    /// The checksum of the [`Source`] it came from.
    pub source: String,
    /// The depth the source reached.
    pub depth: i64,
    /// Kilonodes the source searched.
    pub knodes: i64,
    /// Whose point of view the scores are from.
    pub perspective: Perspective,
    /// The lines, best first as the source ordered them.
    pub lines: Vec<Line>,
}

/// `knowledge/analyses`' result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysesResult {
    /// The position as the node identifies it: placement, side to move, castling rights and an
    /// en-passant square only if a pawn can use it. A client applies the answer only to a
    /// position it matches.
    pub position: String,
    /// The analyses, deepest first; empty if the sources hold none.
    pub analyses: Vec<Analysis>,
    /// The sources of these analyses.
    pub sources: Vec<Source>,
}

/// What the sources hold of a position.
pub enum FindAnalyses {}
impl RequestMethod for FindAnalyses {
    const NAME: &'static str = "knowledge/analyses";
    type Params = AnalysesParams;
    type Result = AnalysesResult;
}

/// `knowledge/exercises`' params: all optional, and the params may be left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<ExercisesFields>")]
pub struct ExercisesParams {
    /// Only exercises of this theme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Where the page starts: a `next` the node gave.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// How many, 1 to [`MAX_LIMIT`]; [`DEFAULT_LIMIT`] if left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// [`ExercisesParams`] as they are read, from an object or from nothing.
#[derive(Default, Deserialize)]
struct ExercisesFields {
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

impl From<Option<ExercisesFields>> for ExercisesParams {
    fn from(fields: Option<ExercisesFields>) -> ExercisesParams {
        let f = fields.unwrap_or_default();
        ExercisesParams {
            theme: f.theme,
            after: f.after,
            limit: f.limit,
        }
    }
}

impl ExercisesParams {
    /// Checks the params' shapes, or says what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        valid_page(self.limit, &self.after, &self.theme)
    }
}

/// A graded candidate move of an exercise.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// The move in UCI.
    #[serde(rename = "move")]
    pub uci: String,
    /// The points the source's file gave it, on that file's scale.
    pub points: i64,
}

/// A strategic exercise.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exercise {
    /// The source's identity for it, such as `STS(v1.0) Undermine.001`.
    pub id: String,
    /// The checksum of the [`Source`] it came from.
    pub source: String,
    /// The suite's own label inside the id.
    pub suite: String,
    /// Its theme.
    pub theme: String,
    /// The position: placement, side to move, castling and en passant.
    pub fen: String,
    /// The best move as the source wrote it (SAN).
    pub best: String,
    /// The graded candidates in the source's order; a move may appear twice.
    pub candidates: Vec<Candidate>,
    /// The source's scores as it wrote them, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scores_text: Option<String>,
}

/// `knowledge/exercises`' result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExercisesResult {
    /// The exercises.
    pub exercises: Vec<Exercise>,
    /// The sources of these exercises.
    pub sources: Vec<Source>,
    /// Where the next page starts, if more may follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// Strategic exercises from complete imports, by theme.
pub enum FindExercises {}
impl RequestMethod for FindExercises {
    const NAME: &'static str = "knowledge/exercises";
    type Params = ExercisesParams;
    type Result = ExercisesResult;
}
