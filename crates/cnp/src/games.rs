//! Proposed draft-1 games extension: a library of games a node keeps, for review alongside
//! chessdb's implementation (`spec/games.md`). It doesn't change the CNP 1.0 transport.
//!
//! A client saves each game under an identity of its own choosing, with a version that grows
//! with every save ([`Save`]), lists them newest first ([`List`]), reads one back whole
//! ([`Get`]), and deletes one it no longer wants ([`Delete`], on a node that offers it). A game travels as its PGN, with the few facts a list shows beside it (the
//! players' names, the result, how many moves) and, for carrying on with an unfinished game,
//! who played each side and the clock's times. The node keeps no chess rules: it checks
//! shapes and sizes ([`Game::validate`]), and the client is responsible for the chess.
//!
//! **Versions make saves safe to repeat.** A save with a version above the stored one replaces
//! it; the same version and the same game changes nothing (a retry after a lost answer); a
//! lower version changes nothing either, and the answer's higher version tells the client
//! that what is stored is newer than what it sent. The same version with a different game is
//! refused ([`VERSION_REUSED`]).
//!
//! **Searching.** A node that can search ([`Capabilities::search`]) lists only the games that
//! match a query ([`ListParams::query`]): each of its words, ignoring case, in a player's name
//! or in one of the values of the PGN's tags, such as its event, site or date ([`matches()`]).
//! The moves aren't searched.
//!
//! **Pitfall: one message holds a whole game.** A node serving games reads messages of at
//! least [`MESSAGE_BYTES`] (64 KiB, chessdb's limit), so a game is bounded
//! ([`MAX_GAME_BYTES`], [`MAX_PGN_BYTES`], [`MAX_PLIES`]) with room left for the envelope of
//! a [`Get`] answer. A client checks a game before sending it, as a node does on receiving it.
use crate::RequestMethod;
use serde::{Deserialize, Serialize};

/// Review identifier, in [`Capabilities::contract`]. Clients must check it before sending
/// games requests.
pub const CONTRACT: &str = "draft-1";
/// The message size the bounds are made for: a node serving games reads messages of at least
/// this many bytes (`limits.maxMessageBytes`).
pub const MESSAGE_BYTES: u64 = 64 * 1024;
/// The longest game identity, in bytes.
pub const MAX_ID_BYTES: usize = 64;
/// The longest player's name, in bytes.
pub const MAX_NAME_BYTES: usize = 128;
/// The longest PGN, in bytes.
pub const MAX_PGN_BYTES: usize = 32 * 1024;
/// The most moves (plies) a saved game may have.
pub const MAX_PLIES: u32 = 1000;
/// The longest time control, in bytes.
pub const MAX_CONTROL_BYTES: usize = 32;
/// The most time a clock may show, in milliseconds: 100 hours.
pub const MAX_CLOCK_MS: u64 = 100 * 60 * 60 * 1000;
/// The highest version: the largest whole number JSON keeps exactly everywhere (2^53 − 1).
pub const MAX_VERSION: u64 = (1 << 53) - 1;
/// The longest game as JSON, in bytes: under [`MESSAGE_BYTES`], with room for the envelope
/// and timestamps of a [`Get`] answer.
pub const MAX_GAME_BYTES: usize = 60 * 1024;
/// How many games [`List`] returns when the request doesn't say.
pub const DEFAULT_LIST_LIMIT: u32 = 20;
/// The most games one [`List`] returns.
pub const MAX_LIST_LIMIT: u32 = 100;
/// The longest query, in bytes.
pub const MAX_QUERY_BYTES: usize = 128;
/// No change: the version was already saved with a different game.
pub const VERSION_REUSED: i64 = -32021;
/// Storage failed; the outcome can be uncertain, so retry the exact save.
pub const STORAGE_FAILURE: i64 = -32022;
/// No game has the identity asked for.
pub const NOT_FOUND: i64 = -32025;

/// The optional games domain. Missing switches mean false.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// `games/list` and `games/get` are available.
    #[serde(default)]
    pub read: bool,
    /// `games/save` is available.
    #[serde(default)]
    pub write: bool,
    /// `games/list` takes a [`ListParams::query`]. A node without it ignores the query and
    /// lists every game, so a client sends one only to a node that searches.
    #[serde(default)]
    pub search: bool,
    /// `games/delete` is available ([`Delete`]).
    #[serde(default)]
    pub delete: bool,
    /// The contract the node implements, [`CONTRACT`]; absent means don't send games
    /// requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

/// A game's result, as PGN writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameResult {
    /// White won.
    #[serde(rename = "1-0")]
    WhiteWon,
    /// Black won.
    #[serde(rename = "0-1")]
    BlackWon,
    /// A draw.
    #[serde(rename = "1/2-1/2")]
    Drawn,
    /// Not over (yet).
    #[serde(rename = "*")]
    Unfinished,
}

/// Who played a side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Player {
    /// Someone at the board.
    Human,
    /// An engine.
    Engine,
}

/// Who played each side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Players {
    /// White.
    pub white: Player,
    /// Black.
    pub black: Player,
}

/// A game's clock: its time control and each side's time in every position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clock {
    /// The time control, as the client writes it, like `5+3`.
    pub control: String,
    /// White's and Black's time left, in milliseconds, in each position from the start: one
    /// pair more than the game has moves.
    pub times_ms: Vec<[u64; 2]>,
}

/// A game as a client saves it, and as [`Get`] gives it back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    /// The game's identity, chosen by the client: 1 to [`MAX_ID_BYTES`] lowercase letters,
    /// digits and hyphens.
    pub id: String,
    /// The client's count of this game's saves: 1 to [`MAX_VERSION`], higher with every save.
    pub version: u64,
    /// White's name, as a list shows it.
    pub white: String,
    /// Black's name.
    pub black: String,
    /// The result, or [`GameResult::Unfinished`].
    pub result: GameResult,
    /// How many moves (plies) have been played.
    pub plies: u32,
    /// The whole game, in PGN.
    pub pgn: String,
    /// Who played each side, if the client says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players: Option<Players>,
    /// The clock, if the game had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<Clock>,
}

impl Game {
    /// Whether the game keeps within the contract's shapes and sizes, and why not. It checks
    /// no chess: the PGN is the client's.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!(
                "a game's id is 1 to {MAX_ID_BYTES} lowercase letters, digits and hyphens"
            ));
        }
        if !(1..=MAX_VERSION).contains(&self.version) {
            return Err(format!("a version is 1 to {MAX_VERSION}"));
        }
        for name in [&self.white, &self.black] {
            if !valid_text(name, MAX_NAME_BYTES) {
                return Err(format!(
                    "a player's name is 1 to {MAX_NAME_BYTES} bytes, without control characters"
                ));
            }
        }
        if self.plies > MAX_PLIES {
            return Err(format!("a saved game has at most {MAX_PLIES} moves"));
        }
        if self.pgn.is_empty() || self.pgn.len() > MAX_PGN_BYTES {
            return Err(format!("a game's PGN is 1 to {MAX_PGN_BYTES} bytes"));
        }
        if let Some(clock) = &self.clock {
            if !valid_text(&clock.control, MAX_CONTROL_BYTES) {
                return Err(format!(
                    "a time control is 1 to {MAX_CONTROL_BYTES} bytes, without control \
                     characters"
                ));
            }
            if clock.times_ms.len() != self.plies as usize + 1 {
                return Err("a clock has one pair of times more than the game has moves".into());
            }
            if clock.times_ms.iter().flatten().any(|ms| *ms > MAX_CLOCK_MS) {
                return Err(format!("a clock shows at most {MAX_CLOCK_MS} ms"));
            }
        }
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?.len();
        if bytes > MAX_GAME_BYTES {
            return Err(format!(
                "a game is at most {MAX_GAME_BYTES} bytes as JSON, and this one is {bytes}"
            ));
        }
        Ok(())
    }
}

/// Whether `id` is a valid game identity.
pub fn valid_id(id: &str) -> bool {
    (1..=MAX_ID_BYTES).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether `query` is a valid [`ListParams::query`]: 1 to [`MAX_QUERY_BYTES`] bytes, without
/// control characters, with at least one word.
pub fn valid_query(query: &str) -> bool {
    valid_text(query, MAX_QUERY_BYTES) && query.split_whitespace().next().is_some()
}

/// Whether a game matches `query`, as a node that searches decides it: each of the query's
/// words (split at whitespace), ignoring case, is part of White's or Black's name or of one of
/// the values of the PGN's tags ([`tag_values`]). The moves aren't searched.
pub fn matches(query: &str, white: &str, black: &str, pgn: &str) -> bool {
    let texts: Vec<String> = [white.to_string(), black.to_string()]
        .into_iter()
        .chain(tag_values(pgn))
        .map(|text| text.to_lowercase())
        .collect();
    query.split_whitespace().all(|word| {
        let word = word.to_lowercase();
        texts.iter().any(|text| text.contains(&word))
    })
}

/// The values of the tag pairs at the head of `pgn`, such as `Solrac game` from
/// `[Event "Solrac game"]`, unescaped (`\"` and `\\`). They end where the moves start; a
/// malformed pair ends them too.
pub fn tag_values(pgn: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut chars = pgn.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next_if_eq(&'[').is_none() {
            return values;
        }
        // The tag's name, up to its value's opening quote.
        loop {
            match chars.next() {
                Some('"') => break,
                Some(']') | None => return values,
                Some(_) => {}
            }
        }
        let mut value = String::new();
        loop {
            match chars.next() {
                Some('\\') => value.extend(chars.next()),
                Some('"') => break,
                Some(c) => value.push(c),
                None => return values,
            }
        }
        values.push(value);
        // Up to the pair's closing bracket.
        if !chars.by_ref().any(|c| c == ']') {
            return values;
        }
    }
}

fn valid_text(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.len()) && !text.chars().any(char::is_control)
}

/// A stored game, as a list shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Its identity.
    pub id: String,
    /// The version stored.
    pub version: u64,
    /// White's name.
    pub white: String,
    /// Black's name.
    pub black: String,
    /// Its result.
    pub result: GameResult,
    /// How many moves it has.
    pub plies: u32,
    /// When the node first stored it: UTC, RFC 3339 with milliseconds, like
    /// `2026-10-24T14:05:12.123Z`.
    pub created_at: String,
    /// When the node last changed it, the same way.
    pub updated_at: String,
}

/// `games/save`'s params.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveParams {
    /// The game.
    pub game: Game,
}

/// Saves a game: stores it, replaces an older version of it, or, for a version already
/// stored or older, changes nothing. Answers with what is stored, after its commit.
pub enum Save {}
impl RequestMethod for Save {
    const NAME: &'static str = "games/save";
    type Params = SaveParams;
    type Result = Summary;
}

/// `games/list`'s params. Each is optional, and so are the params themselves: a request
/// without them asks for the newest page of every game.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<ListFields>")]
pub struct ListParams {
    /// How many games, 1 to [`MAX_LIST_LIMIT`]; [`DEFAULT_LIST_LIMIT`] if missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Where the last page ended: its [`ListResult::next`], passed back unchanged with the
    /// same query. Missing for the newest games.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Only the games that match these words ([`matches()`], [`valid_query`]). Missing for
    /// every game. Sent only to a node that searches ([`Capabilities::search`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

/// [`ListParams`] as they are read, from an object or from nothing.
#[derive(Default, Deserialize)]
struct ListFields {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default)]
    query: Option<String>,
}

impl From<Option<ListFields>> for ListParams {
    fn from(fields: Option<ListFields>) -> ListParams {
        let fields = fields.unwrap_or_default();
        ListParams {
            limit: fields.limit,
            after: fields.after,
            query: fields.query,
        }
    }
}

/// `games/list`'s result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListResult {
    /// The games, the most recently changed first.
    pub games: Vec<Summary>,
    /// Where to carry on, if there are more: an opaque cursor for [`ListParams::after`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// Lists the stored games, the most recently changed first, a page at a time.
pub enum List {}
impl RequestMethod for List {
    const NAME: &'static str = "games/list";
    type Params = ListParams;
    type Result = ListResult;
}

/// `games/get`'s params.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetParams {
    /// The game's identity.
    pub id: String,
}

/// A stored game, whole: `games/get`'s result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stored {
    /// The game, as it was last saved.
    pub game: Game,
    /// When the node first stored it.
    pub created_at: String,
    /// When the node last changed it.
    pub updated_at: String,
}

/// Reads one stored game whole, or answers [`NOT_FOUND`].
pub enum Get {}
impl RequestMethod for Get {
    const NAME: &'static str = "games/get";
    type Params = GetParams;
    type Result = Stored;
}

/// `games/delete`'s params.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteParams {
    /// The game's identity.
    pub id: String,
}

/// `games/delete`'s result: the game that is gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deleted {
    /// The deleted game's identity.
    pub id: String,
}

/// Deletes one stored game, every version of it, and answers after the commit; a game the
/// node doesn't keep is [`NOT_FOUND`], which a client retrying after a lost answer can take
/// as done. Offered by a node whose capabilities say [`Capabilities::delete`].
pub enum Delete {}
impl RequestMethod for Delete {
    const NAME: &'static str = "games/delete";
    type Params = DeleteParams;
    type Result = Deleted;
}
