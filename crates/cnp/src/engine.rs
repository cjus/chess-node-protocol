//! The engine domain (spec, "The engine domain"): searching positions, and what a learning
//! tool needs to know about the search.
//!
//! | Method | Kind | Types |
//! | --- | --- | --- |
//! | `engine/search` | request | [`Search`]: [`SearchParams`] → [`SearchResult`] |
//! | `engine/progress` | notification, node to client | [`EngineProgress`]: [`Progress`] |
//! | `engine/stop` | request | [`Stop`]: [`StopParams`] → `null` |
//! | `engine/techniques` | request | [`Techniques`]: no params → [`TechniquesResult`] |
//! | `engine/levels` | request | [`Levels`]: no params → [`LevelsResult`] |
//! | `engine/evaluate` | request | [`Evaluate`]: [`EvaluateParams`] → [`EvaluateResult`] |
//! | `engine/ponderHit` | request | [`PonderHit`]: [`PonderHitParams`] → `null` |
//!
//! Positions and moves are chess, not any engine's internals: a FEN and the moves played
//! from it, and moves in coordinate notation (`e2e4`, `e7e8q`). Every request stands alone:
//! it carries the whole position and every option, so any node of a cluster can answer it
//! and a request can be sent again.
//!
//! Scores are always from the point of view of the side to move, and say whether they are
//! exact or a bound ([`Score`]).
//!
//! **Pondering** (spec, 7.7) is the one exchange that spans requests: a search with
//! [`SearchParams::ponder`] runs on the opponent's time, without answering, until [`PonderHit`]
//! says the expected move was played. On a miss the client cancels it and starts another. The
//! pitfall is time: a ponder search's time limits, the clock included, count from the hit, not
//! from the request.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::message::Id;
use crate::method::{NoParams, NotificationMethod, RequestMethod};

/// The FEN of chess's starting position.
pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// What an engine node can do beyond plain searches (every engine node answers
/// `engine/search` with a depth, node, time or infinite limit, and `engine/stop`). A missing
/// capability is `false`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapabilities {
    /// Several lines at once (`multipv` above 1), up to the node's `maxMultipv`.
    #[serde(default)]
    pub multipv: bool,
    /// A score window at the root (`window`).
    #[serde(default)]
    pub window: bool,
    /// Only some root moves (`rootMoves`).
    #[serde(default)]
    pub root_moves: bool,
    /// Playing levels (`strength`, `engine/levels`).
    #[serde(default)]
    pub strength: bool,
    /// Switching search techniques (`techniques`, `engine/techniques`).
    #[serde(default)]
    pub techniques: bool,
    /// Reproducible searches (`deterministic`).
    #[serde(default)]
    pub deterministic: bool,
    /// A game clock as a limit (`limits.clock`).
    #[serde(default)]
    pub clock: bool,
    /// Looking for a mate (`limits.mate`).
    #[serde(default)]
    pub mate: bool,
    /// Chess960 positions and castling.
    #[serde(default)]
    pub chess960: bool,
    /// Progress while searching (`progressMs`, `engine/progress`).
    #[serde(default)]
    pub progress: bool,
    /// A position's evaluation, term by term (`engine/evaluate`).
    #[serde(default)]
    pub evaluate: bool,
    /// Searching on the opponent's time (`ponder`, `engine/ponderHit`).
    #[serde(default)]
    pub ponder: bool,
}

/// The `engine/search` request: search a position and answer with the best move.
pub enum Search {}

impl RequestMethod for Search {
    const NAME: &'static str = "engine/search";
    type Params = SearchParams;
    type Result = SearchResult;
}

/// A position: a FEN, and the moves played from it.
///
/// The moves let the engine see repetitions. A client sends at least the moves since the last
/// capture or pawn move, and may send every move since the FEN.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// The position the moves start from, in Forsyth–Edwards Notation. For Chess960, castling
    /// rights are Shredder-FEN (`HAha`) or X-FEN.
    pub fen: String,
    /// The moves played from it, in order, in coordinate notation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moves: Vec<String>,
    /// A Chess960 game (capability `chess960`): castling is written as the king moving onto
    /// its own rook (`e1h1`), in requests and answers.
    #[serde(default, skip_serializing_if = "is_false")]
    pub chess960: bool,
}

impl Position {
    /// Chess's starting position.
    pub fn start() -> Position {
        Position::fen(START_FEN)
    }

    /// A position given by its FEN, with no moves after it.
    pub fn fen(fen: impl Into<String>) -> Position {
        Position {
            fen: fen.into(),
            moves: Vec::new(),
            chess960: false,
        }
    }

    /// The same position, after these moves too.
    pub fn with_moves<S: Into<String>>(mut self, moves: impl IntoIterator<Item = S>) -> Position {
        self.moves.extend(moves.into_iter().map(Into::into));
        self
    }
}

/// What to search, and how.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchParams {
    /// The position to find a move in.
    pub position: Position,
    /// When to stop. With none, the node decides, and stops within its `maxTimeMs`.
    #[serde(default, skip_serializing_if = "SearchLimits::is_empty")]
    pub limits: SearchLimits,
    /// How strongly to play. Missing: full strength.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<Strength>,
    /// Techniques switched on or off, by the ids `engine/techniques` lists. Missing ones keep
    /// their defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub techniques: BTreeMap<String, bool>,
    /// Only these moves at the root. Empty: all of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_moves: Vec<String>,
    /// Search the root within this score window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
    /// How many lines to find, the best first. Missing: 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multipv: Option<u32>,
    /// One thread and a cleared hash table, so the same request gives the same result.
    #[serde(default, skip_serializing_if = "is_false")]
    pub deterministic: bool,
    /// Send `engine/progress` at most this often, in milliseconds. Missing: no progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_ms: Option<u64>,
    /// A hint that searches with the same value belong to one game, so the node may keep its
    /// hash table between them. A node may ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// A ponder search (capability `ponder`): `position` has the expected move played, and the
    /// search doesn't answer until `engine/ponderHit`, `engine/stop` or `$/cancelRequest` (or
    /// the node's `maxTimeMs`). Its time counts from the hit. A node may stop searching at a
    /// depth, node or mate limit before the hit, but still waits for it, and then answers at
    /// once.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ponder: bool,
}

impl SearchParams {
    /// A search of `position` with no options: the node decides how long.
    pub fn new(position: Position) -> SearchParams {
        SearchParams {
            position,
            limits: SearchLimits::default(),
            strength: None,
            techniques: BTreeMap::new(),
            root_moves: Vec::new(),
            window: None,
            multipv: None,
            deterministic: false,
            progress_ms: None,
            game: None,
            ponder: false,
        }
    }

    /// The same search, with these limits.
    pub fn limits(mut self, limits: SearchLimits) -> SearchParams {
        self.limits = limits;
        self
    }
}

/// When a search stops: at the first limit reached. Times are durations from when the node
/// receives the request, never clock times, since clocks on different machines differ.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchLimits {
    /// Search this many plies deep.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// Search about this many nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
    /// Spend this long, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<u64>,
    /// Look for a mate in this many moves (capability `mate`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mate: Option<u32>,
    /// Play to a game clock (capability `clock`): the node decides how much of it to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<Clock>,
    /// Search until `engine/stop` or `$/cancelRequest` (or the node's `maxTimeMs`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub infinite: bool,
}

impl SearchLimits {
    /// Limits that stop at `depth` plies.
    pub fn depth(depth: u32) -> SearchLimits {
        SearchLimits {
            depth: Some(depth),
            ..SearchLimits::default()
        }
    }

    /// Limits that search until stopped.
    pub fn infinite() -> SearchLimits {
        SearchLimits {
            infinite: true,
            ..SearchLimits::default()
        }
    }

    /// Whether no limit is set.
    pub fn is_empty(&self) -> bool {
        *self == SearchLimits::default()
    }
}

/// A game clock: each side's time and increment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clock {
    /// White's clock.
    pub white: SideClock,
    /// Black's clock.
    pub black: SideClock,
    /// Moves until the next time control. Missing: sudden death.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moves_to_go: Option<u32>,
}

/// One side's clock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideClock {
    /// Time left, in milliseconds.
    pub time_ms: u64,
    /// Added after each move, in milliseconds.
    #[serde(default)]
    pub increment_ms: u64,
}

/// How strongly to play (capability `strength`): one of a level from `engine/levels`, or a
/// rating within its `elo` range.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Strength {
    /// A level, which may be fractional if the levels say so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<f64>,
    /// A rating, on the scale `engine/levels` names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elo: Option<u32>,
}

/// A score window at the root, in centipawns from the side to move's point of view: a score at
/// or below `alpha` is an upper bound, one at or above `beta` a lower bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// The lower edge.
    pub alpha: i32,
    /// The upper edge, above `alpha`.
    pub beta: i32,
}

/// A score, from the side to move's point of view.
///
/// In JSON, `{"cp": 18}` or `{"mate": -3}`, with `"bound"` when it isn't exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawScore", into = "RawScore")]
pub struct Score {
    /// Its value.
    pub value: ScoreValue,
    /// Whether it is exact, or a bound.
    pub bound: Bound,
}

impl Score {
    /// An exact score in centipawns.
    pub fn cp(cp: i32) -> Score {
        Score {
            value: ScoreValue::Centipawns(cp),
            bound: Bound::Exact,
        }
    }

    /// An exact mate score: in `moves` moves, negative if the side to move is mated.
    pub fn mate(moves: i32) -> Score {
        Score {
            value: ScoreValue::Mate(moves),
            bound: Bound::Exact,
        }
    }
}

/// A score's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoreValue {
    /// Hundredths of a pawn.
    Centipawns(i32),
    /// Mate in this many moves; negative when the side to move is the one mated.
    Mate(i32),
}

/// Whether a score is exact, or only a bound because the search cut off outside its window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Bound {
    /// The score itself.
    #[default]
    Exact,
    /// The true score is at least this.
    Lower,
    /// The true score is at most this.
    Upper,
}

/// A score as JSON has it.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct RawScore {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cp: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mate: Option<i32>,
    #[serde(default, skip_serializing_if = "is_exact")]
    bound: Bound,
}

impl TryFrom<RawScore> for Score {
    type Error = String;

    fn try_from(raw: RawScore) -> Result<Score, String> {
        let value = match (raw.cp, raw.mate) {
            (Some(cp), None) => ScoreValue::Centipawns(cp),
            (None, Some(mate)) => ScoreValue::Mate(mate),
            _ => return Err("a score has either \"cp\" or \"mate\"".to_string()),
        };
        Ok(Score {
            value,
            bound: raw.bound,
        })
    }
}

impl From<Score> for RawScore {
    fn from(score: Score) -> RawScore {
        let (cp, mate) = match score.value {
            ScoreValue::Centipawns(cp) => (Some(cp), None),
            ScoreValue::Mate(mate) => (None, Some(mate)),
        };
        RawScore {
            cp,
            mate,
            bound: score.bound,
        }
    }
}

/// What a search found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// The move to play, or `null` when the side to move has no legal move (checkmate or
    /// stalemate).
    pub best_move: Option<String>,
    /// The reply the engine expects: the move to ponder on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ponder: Option<String>,
    /// The best line's score. Missing when there is no legal move, or when the search was
    /// stopped before it scored a line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<Score>,
    /// The best line, starting with the best move.
    #[serde(default)]
    pub pv: Vec<String>,
    /// The depth completed, in plies.
    pub depth: u32,
    /// The deepest any line went, extensions and quiescence included.
    #[serde(default)]
    pub sel_depth: u32,
    /// Nodes searched, every thread together.
    pub nodes: u64,
    /// Time spent, in milliseconds.
    pub time_ms: u64,
    /// For a ponder search, how much of `time_ms` came before `engine/ponderHit` (all of it if
    /// none came). Missing for other searches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ponder_ms: Option<u64>,
    /// Whether `engine/stop` ended the search before its limits.
    #[serde(default)]
    pub stopped: bool,
    /// Every line, the best first, when more than one was asked for. The first is the same
    /// as `score` and `pv`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<Line>,
    /// More about the search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<SearchStats>,
}

/// One line of a search that found several.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    /// Its rank, from 1 for the best.
    pub multipv: u32,
    /// Its score.
    pub score: Score,
    /// Its moves.
    pub pv: Vec<String>,
    /// The depth it was searched to.
    pub depth: u32,
    /// The deepest it went.
    #[serde(default)]
    pub sel_depth: u32,
}

/// More about a search.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStats {
    /// How full the hash table is, in thousandths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_full: Option<u32>,
    /// Tablebase probes that gave a result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tb_hits: Option<u64>,
}

/// The `engine/progress` notification, from the node: how a search is going. Sent only for a
/// search that asked (`progressMs`), at most that often, and never after its result.
pub enum EngineProgress {}

impl NotificationMethod for EngineProgress {
    const NAME: &'static str = "engine/progress";
    type Params = Progress;
}

/// A search's progress: its best line so far.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// The `engine/search` request it belongs to.
    pub id: Id,
    /// The depth completed.
    pub depth: u32,
    /// The deepest any line went.
    #[serde(default)]
    pub sel_depth: u32,
    /// The line's score.
    pub score: Score,
    /// The line.
    pub pv: Vec<String>,
    /// Which line, from 1, when searching several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multipv: Option<u32>,
    /// Nodes searched so far.
    pub nodes: u64,
    /// Nodes a second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nps: Option<u64>,
    /// Time so far, in milliseconds.
    pub time_ms: u64,
    /// How full the hash table is, in thousandths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_full: Option<u32>,
    /// Tablebase probes that gave a result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tb_hits: Option<u64>,
}

/// The `engine/stop` request: end a search early. The search answers at once with the best it
/// has, marked `stopped`. Stopping a search that has already answered, or never existed, is
/// not an error: the two can cross.
pub enum Stop {}

impl RequestMethod for Stop {
    const NAME: &'static str = "engine/stop";
    type Params = StopParams;
    type Result = ();
}

/// Which search to stop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopParams {
    /// The `engine/search` request's id.
    pub id: Id,
}

/// The `engine/ponderHit` request (capability `ponder`): the move a ponder search expected was
/// played, so it keeps to its limits from now on, keeping what it has searched, and answers as
/// any search does.
///
/// A hit for a search that has already answered, never existed, or has already had its hit is
/// not an error: the messages can cross. A hit for a running search that isn't a ponder search
/// is `InvalidParams`.
pub enum PonderHit {}

impl RequestMethod for PonderHit {
    const NAME: &'static str = "engine/ponderHit";
    type Params = PonderHitParams;
    type Result = ();
}

/// Which ponder search the expected move was played for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PonderHitParams {
    /// The ponder search's `engine/search` request id.
    pub id: Id,
}

/// The `engine/techniques` request (capability `techniques`): the search techniques a search
/// can switch on or off.
pub enum Techniques {}

impl RequestMethod for Techniques {
    const NAME: &'static str = "engine/techniques";
    type Params = NoParams;
    type Result = TechniquesResult;
}

/// The techniques.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TechniquesResult {
    /// Every switchable technique.
    pub techniques: Vec<Technique>,
}

/// A search technique that can be switched on or off.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Technique {
    /// What `techniques` in a search calls it, like `nullMove`.
    pub id: String,
    /// Its name for people, like `Null-move pruning`.
    pub name: String,
    /// What it does, in a sentence or two.
    pub description: String,
    /// Whether it is on when a search doesn't say.
    pub default: bool,
    /// Techniques it needs on to have any effect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// Where to read about it, a URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// The `engine/levels` request (capability `strength`): the playing levels and their ratings.
pub enum Levels {}

impl RequestMethod for Levels {
    const NAME: &'static str = "engine/levels";
    type Params = NoParams;
    type Result = LevelsResult;
}

/// The playing levels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelsResult {
    /// Every level, weakest first. The last is full strength.
    pub levels: Vec<Level>,
    /// What the ratings mean, like `CCRL Blitz, 40 moves in 0.4 s`.
    pub rating_scale: String,
    /// Whether a level between two listed ones may be asked for (`12.5`).
    #[serde(default)]
    pub fractional: bool,
    /// The ratings `strength.elo` may ask for. Missing: only levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elo: Option<EloRange>,
}

/// A playing level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    /// Its number.
    pub level: u32,
    /// Its measured rating.
    pub rating: u32,
}

/// The ratings a search may ask for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EloRange {
    /// The lowest.
    pub min: u32,
    /// The highest.
    pub max: u32,
}

/// The `engine/evaluate` request (capability `evaluate`): the engine's evaluation of a
/// position as it stands, without a search, and what it is made of.
///
/// **Pitfall:** an evaluation doesn't search, so it misses a piece about to be captured and
/// doesn't know that a side is mated. To explain a search's score, evaluate the position at
/// the end of its `pv`, which is where the score came from.
pub enum Evaluate {}

impl RequestMethod for Evaluate {
    const NAME: &'static str = "engine/evaluate";
    type Params = EvaluateParams;
    type Result = EvaluateResult;
}

/// What to evaluate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateParams {
    /// The position.
    pub position: Position,
}

/// A position's evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateResult {
    /// The evaluation, in centipawns from the side to move's point of view, always exact.
    pub score: Score,
    /// What the score is made of. The side to move's terms less the other side's are the
    /// score, give or take rounding.
    pub terms: Vec<Term>,
}

/// One term of an evaluation, like material or mobility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Term {
    /// The engine's own id for it, like `mobility`. Two engines may split their evaluations
    /// differently.
    pub id: String,
    /// Its name for people, like `Mobility`.
    pub name: String,
    /// What it measures, in a sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What it is worth to White in this position, in centipawns from White's point of view.
    pub white: i32,
    /// What it is worth to Black, in centipawns from Black's point of view.
    pub black: i32,
}

/// Whether `text` has the form of a move in coordinate notation: two squares, and a promotion
/// piece in lower case (`e2e4`, `e7e8q`). It says nothing about legality.
pub fn is_move_text(text: &str) -> bool {
    let b = text.as_bytes();
    let square =
        |file: u8, rank: u8| (b'a'..=b'h').contains(&file) && (b'1'..=b'8').contains(&rank);
    match b.len() {
        4 => square(b[0], b[1]) && square(b[2], b[3]),
        5 => square(b[0], b[1]) && square(b[2], b[3]) && b"qrbn".contains(&b[4]),
        _ => false,
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_exact(bound: &Bound) -> bool {
    *bound == Bound::Exact
}
