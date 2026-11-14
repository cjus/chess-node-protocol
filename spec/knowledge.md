# Knowledge over CNP — draft 1

This is a **proposed CNP extension**, submitted for review alongside chessdb's knowledge
library (cjus/chessdb#2) and Solrac's downloads (cjus/chessgui). It is not an accepted addition
to upstream CNP. It uses CNP 1.0's transport, lifecycle, envelopes and error codes unchanged;
the typed Rust markers and limits are `cnp::knowledge`. It is served by a node that keeps a
program's data, such as chessdb, beside the [preferences](preferences.md), [games](games.md)
and [backups](backups.md) extensions.

The knowledge is what the player didn't make: puzzles, evaluated positions and strategic test
positions from outside sources. The domain has two halves: **a client hands the node a file it
has downloaded, and the node imports it** (`import`), and **a client asks for what was
imported** (`read`): puzzles by theme and rating (in order of difficulty or from a random
point, and how many there are of each theme and rating), the analyses of a position, and strategic
exercises by theme.

The node doesn't download. The client fetches the file, where it can show the licence, the
size and the progress, and tells the node where the file is. The node checks the file against
the checksum the client says it has, so a damaged or swapped download is refused, not imported.

## Capability

```json
{"capabilities": {"knowledge": {"import": true, "read": true, "puzzleSelection": true,
 "kinds": ["lichess-puzzles", "lichess-evals", "sts-epd"], "contract": "draft-1"}}}
```

`import` offers `knowledge/import`, `knowledge/status` and `knowledge/cancel`; `read` offers
`knowledge/puzzles`, `knowledge/analyses` and `knowledge/exercises`; `puzzleSelection` offers
`knowledge/puzzles`' `order`, `seed` and `quality`, and `knowledge/puzzleFacets`. A missing
switch is false. `kinds` are the kinds of file the node can import; a request for another kind is
refused. A client sends knowledge requests only to a node whose `contract` is `draft-1`. A node
without the domain answers them with Method not found (-32601).

## Kinds of file

| Kind | The file |
| --- | --- |
| `lichess-puzzles` | Lichess's puzzle database, CSV, as published (`lichess_db_puzzle.csv.zst`) |
| `lichess-evals` | Lichess's evaluations, JSON Lines (`lichess_db_eval.jsonl.zst`) |
| `sts-epd` | A Strategic Test Suite EPD file, one of STS-Rating's variants |

A file whose name ends in `.zst` is Zstandard-compressed and the node reads it so; the client
never decompresses a file to give it to the node. A kind's variants are different sources, not
versions of one: the checksum tells them apart, and a node doesn't convert one's scores to
another's scale.

## The manifest

Every file comes with where it is from, which the node keeps with everything it imports:

```json
{"name": "Lichess puzzles", "release": "2026-11", "url": "https://database.lichess.org/",
 "retrievedAt": "2026-11-13T10:02:11.000Z",
 "checksum": "3f1c0d…64 hex digits…9a", "license": "CC0-1.0", "attribution": "Lichess"}
```

| Field | What it is |
| --- | --- |
| `name`, `release`, `url` | The source, its release, and where it was downloaded from |
| `retrievedAt` | When it was downloaded: UTC, RFC 3339 with milliseconds |
| `checksum` | SHA-256 of the file **as downloaded** (the compressed file, for `.zst`), 64 hexadecimal digits, either case |
| `license` | An SPDX identifier, such as `CC0-1.0` |
| `attribution` | What to credit, to show wherever the data is used |

Each is required, 1–1024 bytes without control characters (`checksum` is exactly 64). A client
must not offer a corpus whose licence it hasn't cleared for the player's use.

## An import

```json
{"kind": "lichess-puzzles", "checksum": "3f1c0d…9a", "name": "Lichess puzzles",
 "release": "2026-11", "state": "importing", "read": 120000, "accepted": 119871,
 "rejected": 3, "duplicate": 126, "startedAt": "2026-11-13T10:05:00.000Z"}
```

| Field | What it is |
| --- | --- |
| `kind`, `checksum` | Its identity: one file of one kind. Another variant or release has another checksum, so is another import |
| `name`, `release` | From the manifest |
| `state` | `verifying`, `importing`, `complete`, `cancelled` or `failed` |
| `read` | Records read so far, the rejected ones included |
| `accepted`, `rejected`, `duplicate` | What became of them: stored, found invalid (kept apart by the node, with where and why, not stored as knowledge), and already stored |
| `failure` | When `failed`: `{"code": …, "message": …}`, see below |
| `startedAt`, `finishedAt` | UTC, RFC 3339 with milliseconds; `finishedAt` once `complete` |

The counts are of what has been **durably committed**; a node never reports a record it could
still lose. `read` = `accepted` + `rejected` + `duplicate`.

States: `verifying` (the node is reading the whole file to check its checksum, before any
record is read), `importing`, `complete` (the file was read to its end), `cancelled` (stopped
by a client or by the node's shutdown, or found interrupted when the node started again), and
`failed`. `cancelled` and `failed` imports keep everything committed and can be resumed.

A `failure`'s `code` is one of `checksum` (the file isn't the one the manifest describes),
`unreadable` (it stopped being readable), `format` (it isn't a file of this kind), `disk` (not
enough free space), `stream` (reading it failed part-way, as a truncated download does) or
`storage` (the node's own storage failed).

## Requests

### `knowledge/import`

```json
{"kind": "lichess-puzzles", "path": "/Users/carlos/Downloads/lichess_db_puzzle.csv.zst",
 "manifest": { ... a manifest ... }}
```

`path` is an absolute path, on the computer the node runs on, of the file as downloaded
(1–4096 bytes, no NUL). The node only reads it: it never writes, moves or deletes a client's
file.

The request starts the import and **answers at once**, without waiting for it; an import runs
for minutes or hours. It answers with the import:

```json
{"import": { ... an import, verifying ... }}
```

**Imports are safe to repeat.** The identity is the kind and the checksum:

- not seen before: the import starts, `verifying`;
- `complete`: nothing is read again, and the answer is the complete import;
- running (`verifying` or `importing`): the answer is its state, and `path` is ignored;
- `cancelled` or `failed`: the import **resumes**, after the last record it committed, with
  `path` (which may differ from before, and is verified again).

A resumed or repeated import never stores a record twice.

The node runs one import at a time, and answers a second with Busy (-32013) until the first is
finished or cancelled. An import doesn't slow the node's other requests: preferences, games and
backups are answered while it runs.

A kind the node doesn't list is -32602, as is a malformed manifest or path. A file that doesn't
exist, isn't a file, or can't be read is -32027, and no import starts.

### `knowledge/status`

```json
{"kind": "lichess-puzzles", "checksum": "3f1c0d…9a"}
```

Both fields are optional and the params may be left out. With both, it answers with that import
(-32025 if there is none); with one, those that match; with neither, every import. Imports come
newest first, at most 100:

```json
{"imports": [{ ... an import ... }]}
```

A client polls while it shows progress, no more often than once a second.

### `knowledge/cancel`

```json
{"kind": "lichess-puzzles", "checksum": "3f1c0d…9a"}
```

Stops an import at its next batch (a node's batches are short) and answers when it has stopped,
with the import, `cancelled`. What it committed stays, and importing the file again resumes it.
Cancelling an import that isn't running answers with its state unchanged; one the node has no
record of is -32025.

## Reading

Reads answer from **complete imports only**: a file that is half imported, cancelled or failed
doesn't make a corpus that looks whole. Every result names where its items came from, so a
client can show the licence and attribution the manifest gave.

A **source** is one imported file:

```json
{"checksum": "3f1c0d…9a", "kind": "lichess-puzzles", "name": "Lichess puzzles",
 "release": "2026-11", "license": "CC0-1.0", "attribution": "Lichess"}
```

Each result has `sources`, the sources of the items in it, and each item's `source` is a
source's `checksum`. A client must show a source's `attribution` wherever it shows its data.

Lists are **paged by an opaque cursor**: a result's `next`, present when more may follow, is
passed back as `after`. A cursor is the node's own; one it didn't give, or one from another
kind of request, is -32602. `limit` is 1–100, 20 if left out.

### `knowledge/puzzles`

```json
{"theme": "mateIn1", "minRating": 1000, "maxRating": 1400, "limit": 10}
```

Every field is optional. `theme` is a theme as the source writes it (1–64 letters and digits);
the ratings are whole numbers, `minRating` no more than `maxRating`. Puzzles come **easiest
first** (by rating, then by identity), unless `order` says otherwise (below). Answers:

```json
{"puzzles": [{"id": "00008", "source": "3f1c0d…9a",
 "fen": "r6k/pp2r2p/4Rp1Q/3p4/8/1N1P2b1/PqP3PP/7K w - - 0 25",
 "setupFen": "r6k/pp2r2p/4Rp1Q/3p4/8/1N1P2R1/PqP2bPP/7K b - - 0 24",
 "moves": ["f2g3", "e6e7", "b2b1", "b3c1", "b1c1", "h6c1"], "rating": 1819,
 "ratingDeviation": 77, "popularity": 95, "plays": 10446,
 "themes": ["crushing", "hangingPiece", "long", "middlegame"],
 "gameUrl": "https://lichess.org/game/787zsVup/black#48"}],
 "sources": [{ ... a source ... }], "next": "…"}
```

`fen` is the position **the solver is given**: the setup with the first of `moves` played, the
opponent's move. `setupFen` is the position before it, as the source gave it, and `moves` are
all the moves in UCI as the source wrote them: the first is the opponent's, and the second
begins the solution. (Every move of the solution is the only move, but for a mate in one, where
any mate does; that is the source's rule, and the node keeps no chess to check it.)
`ratingDeviation`, `popularity` and `plays` are the source's. `gameUrl` and `openingTags` are
there when the source gave them.

#### Order, seed and quality

A node that offers `puzzleSelection` also takes three optional fields; a client sends them to no
other node.

```json
{"theme": "fork", "minRating": 1200, "maxRating": 1599, "quality": "reliable",
 "order": "random", "seed": 7, "limit": 1}
```

- `order` is `easiest` (the default, as above) or `random`. A random pass starts at a point the
  `seed` picks in **the node's own storage order** and goes on from there, wrapping round to
  the start, so a pass reaches every matching puzzle once, however few match. `seed` is a whole
  number, 0 to 4294967295; the same seed over the same imports starts at the same place, so a
  session can be repeated. Left out, the node picks one, and **the answer's `seed` says which**.
  `next` continues the pass, and is absent once every matching puzzle has been given. A
  `seed` with any other order is -32602.
- Random is **not a shuffle**. The puzzles of one page are neighbours in the node's storage
  order, and which puzzle a seed starts at is not even over the matching puzzles when they sit
  in clusters of that order (a puzzle after a long gap of non-matches is likelier to start a
  pass). For a file whose order has nothing to do with rating or theme, as Lichess's puzzle file
  does not, a start is close to even. A client that wants one varied puzzle asks for `limit: 1`
  with a new seed each time; one that wants a shuffled set of twenty asks for several and
  shuffles them.
- `quality` is `reliable` or absent. It leaves out puzzles whose rating the source's own counts
  don't support. **The node chooses the thresholds** and the answer's `quality` names them
  (`{"minPlays": 100, "minPopularity": 80, "maxRatingDeviation": 110}`: at least that many
  plays, at least that popularity, at most that rating deviation), so a client can say what
  *reliable* meant. The answer has `quality` only when it was asked for. A client doesn't
  assume one node's thresholds are another's.

### `knowledge/puzzleFacets`

How many puzzles each theme has in each rating band: what a client needs to offer a choice of
theme and difficulty without finding out by asking that a combination is empty. Needs
`puzzleSelection`.

```json
{"quality": "reliable"}
```

`quality` is as in `knowledge/puzzles`, and the params may be left out. Answers (abridged, with
illustrative counts):

```json
{"bands": [{"minRating": 0, "maxRating": 799}, {"minRating": 800, "maxRating": 1199},
 {"minRating": 1200, "maxRating": 1599}, {"minRating": 1600, "maxRating": 1999},
 {"minRating": 2000, "maxRating": 2399}, {"minRating": 2400}],
 "total": [500000, 1700000, 1500000, 1200000, 800000, 300000],
 "themes": [{"theme": "fork", "counts": [31694, 213387, 158289, 113726, 53232, 15428]}],
 "sources": [{ ... a source ... }],
 "quality": {"minPlays": 100, "minPopularity": 80, "maxRatingDeviation": 110}}
```

- `bands` are **the node's**: a client uses the answer's bands and never its own, since a node
  may count in others. They are in order from the lowest rating, each starting where the last
  ended plus one; the last has no `maxRating`.
- `total` and each theme's `counts` have one count for each band, in the order of `bands`.
  `total` is every puzzle in the band, and a puzzle with several themes is counted under each
  of them, so the themes' counts add up to more than `total`.
- A theme with no puzzle in any band is not listed. A band with none has a 0.
- Counts are of the puzzles of **complete imports** and are the same as asking
  `knowledge/puzzles` for that theme, band and quality would reach, which a node can keep as it
  imports, not work out at each request. A client doesn't treat a count as current after a new
  import: it asks again.

### `knowledge/analyses`

```json
{"fen": "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq -", "limit": 5}
```

`fen` is a position of four to six fields; the counters, if any, are ignored. Answers with what
the sources hold of **that position**, deepest first:

```json
{"position": "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq -",
 "analyses": [{"source": "3f1c0d…9a", "depth": 20, "knodes": 10, "perspective": "unspecified",
 "lines": [{"mate": 1, "moves": "h5f7"}]}], "sources": [{ ... a source ... }]}
```

`position` is the position as the node identifies it: placement, side to move, castling rights
and an en-passant square only if a pawn can use it (the counters aren't part of it). **A client
applies an answer only to the position it asked about**: it compares `position` with its own
current position, since a game can move on while the answer is on its way, and an analysis of
another position must not be shown against it.

An analysis is the source's search, not this node's: its `depth`, `knodes` and `lines` as the
source gave them, each line with a `cp` (centipawns) or a `mate` score and its moves in UCI.
`perspective` says whose point of view a score is from: `white`, `sideToMove`, or
`unspecified` when the source doesn't say (Lichess doesn't). A client doesn't treat an
`unspecified` score's sign as either. An analysis is of a position without its history, so a
score may differ from the same position reached in a game where repetition matters. A position
with no analysis answers with an empty `analyses`. An illegal position is -32602.

### `knowledge/exercises`

```json
{"theme": "Undermine", "limit": 10}
```

Strategic exercises, by theme if given, in the order of their sources and identities:

```json
{"exercises": [{"id": "STS(v1.0) Undermine.001", "source": "3f1c0d…9a", "suite": "STS(v1.0)",
 "theme": "Undermine", "fen": "1kr5/3n4/q3p2p/p2n2p1/PppB1P2/5BP1/1P2Q2P/3R2K1 w - -",
 "best": "f5", "candidates": [{"move": "f4f5", "points": 10}, {"move": "d4e5", "points": 2}],
 "scoresText": "f5=10, Be5+=2"}], "sources": [{ ... a source ... }], "next": "…"}
```

`candidates` are the graded moves in UCI with the points **the source's file gave them**, in its
order; a move may appear twice with different points, as the published files have it. Points
on the files' different scales (0 to 10, 1 to 100) are not comparable, which is why every
exercise names its `source`, and a client never mixes the points of two sources.

## Pitfalls

- **The path is the client's.** A node reads whatever file its client names, and the client
  already has the node's own privileges, so this adds nothing a client couldn't do. A node
  never *writes* where a request says, and where it keeps its knowledge is its launch
  configuration, as it is for backups.
- **A counted record is a committed record.** A client that shows "120,000 of 4,000,000" shows
  what survives a crash, not what was parsed.
- **Not everything in a file is knowledge.** A record that fails the node's checks (an illegal
  move, a bad score) is counted as `rejected` and kept apart; a file with a few rejects is still
  `complete`. A file that isn't of the kind at all, or whose rejects are all of it, is `failed`
  with `format`.
- **Knowledge is rebuildable.** It is not the player's data and is not in a backup
  ([backups](backups.md)); importing the files again makes it.

## Errors

| Error | Meaning | Client action |
| --- | --- | --- |
| -32602 | Invalid params: an unknown kind, or a malformed path, manifest, checksum, theme, rating, limit, cursor, FEN, order or quality, or a seed with any order but random | Correct the request |
| -32013 | Busy: another import is running | Wait for it, or cancel it |
| -32022 | Storage or I/O failed | Ask `knowledge/status` for the import's state |
| -32025 | No import with that identity | List the imports |
| -32027 | The file doesn't exist, isn't a file, or can't be read | Ask the player for the file again |
| -32800 | Cancelled before the request was done | Nothing changed |

## Not in draft 1

Several themes in one request (and excluding a theme), choosing the puzzles of the day,
progress notifications in place of polling, reading the records an import rejected, removing
an import's knowledge, searching puzzles by opening or by a position, and Lumbra's OTB games.

## Changes

- **draft 1, 2026-11-14**, added to the draft: **puzzle selection** (capability
  `puzzleSelection`, `knowledge/puzzles`' `order`, `seed` and `quality`, and
  `knowledge/puzzleFacets`), so a client can offer varied puzzles of a theme and rating band
  that rate reliably, and say beforehand which combinations are empty (cjus/chessgui#575,
  cjus/chess-node-protocol#29). All optional: a node that doesn't offer it, or a request that
  doesn't send it, is unchanged.
