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
//! | `engine/tree` | request | [`Tree`]: [`TreeParams`] → [`TreeResult`] |
//! | `engine/treeNode` | request | [`TreeNodePage`]: [`TreeNodeParams`] → [`TreeNodeResult`] |
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
//!
//! **The search tree** (spec, 7.8) is the one thing a node keeps for a client: [`Tree`] searches
//! a position, records which moves the search looked at and what it decided about each, and
//! answers with a handle and the root's children; [`TreeNodePage`] reads one node and a page of
//! its children by the moves that lead to it. A tree is too large for one message, so it is
//! read a part at a time. The handle is a cache, not state a client can rely on: it expires,
//! and then the client asks for the tree again.

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
    /// Settling captures before an evaluation (`quiet` on `engine/evaluate`).
    #[serde(default)]
    pub quiet: bool,
    /// What each piece is worth to the search's evaluation (`pieces` on `engine/evaluate`),
    /// and the search's own score (`searchScore`) when `terms` break down another evaluation.
    #[serde(default)]
    pub pieces: bool,
    /// Searching on the opponent's time (`ponder`, `engine/ponderHit`).
    #[serde(default)]
    pub ponder: bool,
    /// A search's tree, kept for a while (`engine/tree`, `engine/treeNode`).
    #[serde(default)]
    pub tree: bool,
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
/// doesn't know that a side is mated. A search's `pv` can end in the middle of an exchange,
/// since the search tries captures past its last move until the position is quiet. To explain
/// a search's score, evaluate the position at the end of its `pv` with
/// [`EvaluateParams::quiet`].
///
/// **Pitfall:** an engine may search with a neural network and break down another evaluation
/// in [`EvaluateResult::terms`]. Then [`EvaluateResult::search_score`] is the search's score,
/// and [`EvaluateResult::pieces`] the search's evaluation's view, piece by piece.
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
    /// Settle captures first, as the engine's search does at its leaves, and evaluate the
    /// position where they stop (capability `quiet`). The moves come back in
    /// [`EvaluateResult::quiet`].
    #[serde(default, skip_serializing_if = "is_false")]
    pub quiet: bool,
    /// Also say what each piece is worth to the search's evaluation (capability `pieces`), in
    /// [`EvaluateResult::pieces`].
    #[serde(default, skip_serializing_if = "is_false")]
    pub pieces: bool,
}

impl EvaluateParams {
    /// An evaluation of `position` as it stands, with nothing more asked.
    pub fn new(position: Position) -> EvaluateParams {
        EvaluateParams {
            position,
            quiet: false,
            pieces: false,
        }
    }
}

/// A position's evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateResult {
    /// The evaluation, in centipawns from the side to move's point of view, always exact.
    /// With [`EvaluateParams::quiet`], mate in 0 when the side to move is checkmated where the
    /// captures stop.
    pub score: Score,
    /// What the score is made of. The side to move's terms less the other side's are the
    /// score, give or take rounding.
    pub terms: Vec<Term>,
    /// When [`EvaluateParams::quiet`] was asked: the moves that settled the position, in
    /// order, in coordinate notation, which may be none. The rest of the result is about the
    /// position they reach. `None` when it wasn't asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet: Option<Vec<String>>,
    /// From a node with capability `pieces`, when its search evaluates the position with
    /// something other than the evaluation [`EvaluateResult::terms`] break down, such as a
    /// neural network: the search's evaluation, from the side to move's point of view.
    /// `None`: the search's evaluation is [`EvaluateResult::score`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_score: Option<Score>,
    /// When [`EvaluateParams::pieces`] was asked: what each piece but the kings is worth to
    /// the search's evaluation, in no set order. Left out when the side to move is
    /// checkmated. `None` when it wasn't asked.
    ///
    /// **Pitfall:** the values don't add up to the score, since pieces work together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pieces: Option<Vec<PieceValue>>,
}

/// What one piece is worth to the search's evaluation of a position: the evaluation from its
/// own side's point of view, less the evaluation of the position with the piece taken off the
/// board.
///
/// It measures a neural network the way a person would test a position's piece: imagine it
/// gone and see how much worse things are. A position without the piece may be one that can't
/// arise in a game (a side left in check); the value is still the evaluation's view of that
/// piece there, and no more than that. The idea is the one Stockfish's `eval` shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceValue {
    /// The piece's square, like `e4`.
    pub square: String,
    /// What it is worth to its own side here, in centipawns from its side's point of view.
    pub value: i32,
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

/// How many plies from the root a tree records when [`TreeParams::plies`] doesn't say: the
/// root's moves and the replies to them.
pub const DEFAULT_TREE_PLIES: u32 = 2;

/// The `engine/tree` request (capability `tree`): search a position to a depth, keep the
/// search's tree for a while, and answer with a handle to it, the search's result, and the
/// root's children.
///
/// A tree says which moves the search looked at and what it decided about each: which reply
/// refuted a move, which moves it set aside without a search, which it searched shallower or
/// again. It is the answer to a player's "why not that move?".
///
/// **Pitfall:** a tree is recorded only near the root ([`TreeParams::plies`]), but the search
/// goes on below it; a node's score comes from the whole search. A node may also stop
/// recording at a cap of its own ([`Recorded::truncated`]): the search's later decisions,
/// the later root moves' among them, are then missing.
pub enum Tree {}

impl RequestMethod for Tree {
    const NAME: &'static str = "engine/tree";
    type Params = TreeParams;
    type Result = TreeResult;
}

/// What to search, and how much of its tree to keep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeParams {
    /// The position.
    pub position: Position,
    /// Search this many plies deep, from 1 to the node's `maxTreeDepth`.
    pub depth: u32,
    /// Record the nodes up to this many plies from the root, from 1 to the node's
    /// `maxTreePlies`. Missing: [`DEFAULT_TREE_PLIES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plies: Option<u32>,
    /// How many of the root's children to answer with. Missing: all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl TreeParams {
    /// A tree of `position` searched `depth` plies deep, with the default plies recorded.
    pub fn new(position: Position, depth: u32) -> TreeParams {
        TreeParams {
            position,
            depth,
            plies: None,
            limit: None,
        }
    }

    /// How many plies from the root the tree records: [`TreeParams::plies`], or
    /// [`DEFAULT_TREE_PLIES`].
    pub fn recorded_plies(&self) -> u32 {
        self.plies.unwrap_or(DEFAULT_TREE_PLIES)
    }
}

/// A recorded tree: its handle, the search's result, and the root with its first children.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeResult {
    /// The tree's handle, for `engine/treeNode` on this connection.
    pub tree: String,
    /// The handle expires once this many milliseconds pass with no request for the tree: from
    /// this answer, and again from each `engine/treeNode`. A node may drop it sooner, to keep
    /// newer trees.
    pub expires_ms: u64,
    /// The depth searched, in plies.
    pub depth: u32,
    /// The deepest any line went, extensions and quiescence included.
    #[serde(default)]
    pub sel_depth: u32,
    /// Nodes searched.
    pub nodes: u64,
    /// Time spent, in milliseconds.
    pub time_ms: u64,
    /// The best move, or `null` when the side to move has no legal move.
    pub best_move: Option<String>,
    /// The best move's score. Missing when there is no legal move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<Score>,
    /// The best line, starting with the best move.
    #[serde(default)]
    pub pv: Vec<String>,
    /// What the tree holds.
    pub recorded: Recorded,
    /// The root: the position searched.
    pub root: TreeNode,
    /// The root's children, the first [`TreeParams::limit`] of them, in the order the search
    /// first tried them.
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

/// What a tree holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recorded {
    /// How many plies from the root were recorded.
    pub plies: u32,
    /// How many nodes the tree holds, the root included.
    pub nodes: u64,
    /// How many decisions the tree holds, at every node together.
    pub decisions: u64,
    /// Whether the node stopped recording before the search ended, at a cap of its own. The
    /// tree then holds what the search decided until then, and nothing after.
    #[serde(default)]
    pub truncated: bool,
}

/// One node of a search tree: a position the search reached, how it searched it, and what it
/// decided there.
///
/// Scores and windows are from the point of view of the side to move at the node, so a child's
/// window is its parent's turned round: `[-beta, -alpha]`.
///
/// **Pitfall:** a node may be searched more than once (shallower first, then again; or with
/// a narrow window, then a wider one). Its `depth`, `alpha`, `beta` and `score` are those of
/// the search that gave its score, the last; its [`Decision`]s say what came before.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    /// The move that reaches it from its parent, in coordinate notation. `None` at the root.
    #[serde(rename = "move", default, skip_serializing_if = "Option::is_none")]
    pub mv: Option<String>,
    /// How many plies deep it was searched (the depth still to go when it was reached).
    pub depth: u32,
    /// The lower edge of its window, the score its side to move already had elsewhere. `None`
    /// when the window had no lower edge, and at the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<Score>,
    /// The upper edge of its window, the score the other side already had elsewhere. `None`
    /// when the window had no upper edge, and at the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beta: Option<Score>,
    /// Its score, with its bound: a lower bound at or above `beta`, an upper bound at or
    /// below `alpha`, and otherwise exact. `None` when the recording ended before the node
    /// returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<Score>,
    /// How many of its children the tree holds.
    pub child_count: u32,
    /// How many nodes the tree holds below it, at every recorded ply.
    pub subtree: u64,
    /// What the search decided about it, in the order it decided.
    #[serde(default)]
    pub decisions: Vec<Decision>,
}

/// One decision of the search about a node, in alpha-beta's terms rather than any engine's.
///
/// A decision with a move names a move from its own node: the reply that refuted it, a move
/// left out, a capture that proved a cutoff. One without is about the node itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    /// What was decided.
    #[serde(flatten)]
    pub kind: DecisionKind,
    /// The id of the technique that decided it, as `engine/techniques` lists it, from a node
    /// that offers `techniques`. Missing when no switchable technique applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technique: Option<String>,
}

impl Decision {
    /// A decision of this kind, with no technique named.
    pub fn new(kind: DecisionKind) -> Decision {
        Decision {
            kind,
            technique: None,
        }
    }

    /// The same decision, made by the technique `id`.
    pub fn by(mut self, id: impl Into<String>) -> Decision {
        self.technique = Some(id.into());
        self
    }
}

/// The kinds of decision (spec, 7.8). In JSON, `kind` and the kind's own fields.
///
/// Later minor versions may add kinds: a client ignores one it doesn't know, which this crate
/// reads as [`DecisionKind::Unknown`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DecisionKind {
    /// `mv` scored at or above beta, the `order`-th move searched (from 1), so the node's
    /// other moves weren't searched: `mv` refutes the move that led here.
    Cutoff {
        /// The move, from this node.
        #[serde(rename = "move")]
        mv: String,
        /// Its place among the moves searched at this node, from 1.
        order: u32,
    },
    /// An earlier search's result for this position, kept in the engine's table, settled it
    /// without a search of its own.
    Table,
    /// Endgame tablebases settled it.
    Tablebase,
    /// Even passing, as if the side to move could skip its turn, kept the score at or above
    /// beta, so the node wasn't searched further.
    NullMove,
    /// The node was settled before any of its moves was searched, by an estimate that the
    /// search wouldn't change the outcome. Its score's bound says which side of the window.
    NodePruned {
        /// The capture whose shallower search proved it, when one did.
        #[serde(rename = "move", default, skip_serializing_if = "Option::is_none")]
        mv: Option<String>,
    },
    /// `mv` was left out without a search, as a move that couldn't change the outcome.
    MovePruned {
        /// The move, from this node.
        #[serde(rename = "move")]
        mv: String,
    },
    /// The node was searched this many plies shallower than its place called for, its move
    /// judged unlikely to change the outcome.
    Reduced {
        /// How many plies shallower.
        plies: u32,
    },
    /// The node was searched this many plies deeper than its place called for, its move judged
    /// to matter more than most, such as a forced one.
    Extended {
        /// How many plies deeper.
        plies: u32,
    },
    /// The node was searched again.
    Research {
        /// Why.
        reason: ResearchReason,
    },
    /// A kind this crate doesn't know, from a later version. Only read, never sent.
    #[serde(other)]
    Unknown,
}

/// Why a node was searched again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResearchReason {
    /// A shallower search's score was above alpha, so it was searched again at its full depth.
    Depth,
    /// A search with a narrower window scored inside the full one, so it was searched again
    /// with the full window to find its exact score.
    Window,
    /// A reason this crate doesn't know, from a later version. Only read, never sent.
    #[serde(other)]
    Unknown,
}

/// The `engine/treeNode` request (capability `tree`): one node of a recorded tree, and a page
/// of its children.
///
/// A handle the node no longer keeps is [`code::EXPIRED`](crate::code::EXPIRED), and a path
/// that leads to no node of the tree is `InvalidParams`.
pub enum TreeNodePage {}

impl RequestMethod for TreeNodePage {
    const NAME: &'static str = "engine/treeNode";
    type Params = TreeNodeParams;
    type Result = TreeNodeResult;
}

/// Which node to read, and which of its children.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNodeParams {
    /// The tree's handle, from `engine/tree`.
    pub tree: String,
    /// The moves from the root to the node, in coordinate notation. Empty: the root.
    #[serde(default)]
    pub path: Vec<String>,
    /// How many children to skip, in the order the search first tried them. Missing: 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    /// How many children to answer with after those; 0 for the node alone. Missing: the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl TreeNodeParams {
    /// The node `path` leads to in `tree`, with all its children.
    pub fn new<S: Into<String>>(
        tree: impl Into<String>,
        path: impl IntoIterator<Item = S>,
    ) -> TreeNodeParams {
        TreeNodeParams {
            tree: tree.into(),
            path: path.into_iter().map(Into::into).collect(),
            offset: None,
            limit: None,
        }
    }
}

/// A node, and a page of its children.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNodeResult {
    /// The node. Its `child_count` is how many children it has in all.
    pub node: TreeNode,
    /// Its children from `offset`, at most `limit` of them, in the order the search first
    /// tried them. Empty past the last.
    #[serde(default)]
    pub children: Vec<TreeNode>,
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
