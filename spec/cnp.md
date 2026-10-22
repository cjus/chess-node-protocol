# The Chess Node Protocol

**Version 1.0, draft** · © 2026 Carlos Justiniano · Licensed under
[Creative Commons Attribution 4.0 International](LICENSE): anyone may copy, implement and
adapt this specification if they credit it.

The Chess Node Protocol (CNP) connects a chess program, the **client**, to the programs that
serve it, the **nodes**: chess engines, and voice modules that speak and listen. It does what
UCI does for engines and more besides. A search can be limited to some root moves or a score
window, the engine's techniques can be switched on and off for teaching, a search can ask for
progress, and every score says whether it is exact or a bound. The same messages reach a node
started on the same computer and, in a later version, one across the network or a cluster of
many.

The words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are used as in
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

## Contents

1. [Design](#1-design)
2. [Conventions](#2-conventions)
3. [Connections](#3-connections)
4. [Messages](#4-messages)
5. [Lifecycle](#5-lifecycle)
6. [Capabilities and limits](#6-capabilities-and-limits)
7. [The engine domain](#7-the-engine-domain)
8. [The voice domain](#8-the-voice-domain)
9. [Errors](#9-errors)
10. [Security](#10-security)
11. [Versions](#11-versions)
12. [Conformance](#12-conformance)
13. [Reserved for later versions](#13-reserved-for-later-versions)
14. [Changes](#14-changes)

## 1. Design

- **Chess concepts, not engine internals.** A position is a FEN and the moves played from it,
  and a move is coordinate text. A score says whether it is exact or a bound. No engine's own
  data structures cross the connection.
- **Requests stand alone.** Each request carries the whole position and every option, and
  nothing depends on an earlier request. So any node of a cluster can answer any request, a
  request can be sent again, and a reconnection loses nothing. Hints that only make a node
  faster are allowed, such as "the same game as before", because a node that ignores them still
  answers correctly. Durations are counted from when the node receives the request, never as
  clock times, since clocks on different machines differ.
- **Each side says what it supports.** A node lists its capabilities and limits when the
  connection starts, so an engine can implement part of the protocol, and a client knows what
  to offer.
- **Questions and answers, not remote control.** A request asks a node for something and gets
  an answer. The protocol leaves out anything that would make two programs one, such as
  stepping a search node by node or sharing a hash table.
- **Readable.** Messages are JSON, one per line. A session can be logged, replayed and typed
  by hand.

## 2. Conventions

- **Field names** are camelCase. **Durations** are whole milliseconds, in fields ending in `Ms`.
- **Unknown fields MUST be ignored**, so that later minor versions can add fields. An optional
  field MAY be left out, and `null` means the same as leaving it out, except where this
  specification gives `null` a meaning.
- **Positions** are a FEN ([Forsyth–Edwards Notation](https://www.chessprogramming.org/Forsyth-Edwards_Notation))
  and the moves played from it, in order:

  ```json
  {"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "moves": ["e2e4", "c7c5"]}
  ```

  The moves let a node see repetitions. A client MUST send at least the moves since the last
  capture or pawn move, and MAY send every move since the FEN. `moves` may be left out when
  there are none. A Chess960 game (capability `chess960`) adds `"chess960": true`.
- **Moves** are coordinate text, as in UCI: the square a piece leaves, the square it reaches,
  and for a promotion the new piece in lower case (`e2e4`, `e7e8q`). Castling is the king's
  move (`e1g1`). In a position with `"chess960": true`, castling is the king moving onto its
  own rook (`e1h1`), both in requests and in answers, and the FEN's castling rights may be
  Shredder-FEN (`HAha`) or X-FEN. A node SHOULD accept either castling notation in a request.
- **Scores** are from the point of view of the side to move: `{"cp": 18}` in centipawns
  (hundredths of a pawn), or `{"mate": 3}` for mate in 3 moves, negative when the side to move
  is the one mated. A score that is only a bound has `"bound": "lower"` (the true score is at
  least this) or `"bound": "upper"` (at most this). Without `bound`, a score is exact.
- **Audio** travels as base64 ([RFC 4648](https://www.rfc-editor.org/rfc/rfc4648#section-4),
  with padding).

In the examples, `→` is a message from the client and `←` one from the node. The arrows are not
part of the messages.

## 3. Connections

### 3.1 stdin/stdout

The client starts the node as a child process and talks to it through the node's standard
input and output.

- Each message is one line of UTF-8 JSON, ending in `\n` (a `\r` before it is ignored). JSON
  escapes line breaks inside strings, so a message never spans lines. Blank lines are ignored.
- The node MUST NOT write anything else to its standard output. It writes its log, if any, to
  standard error.
- When the node's standard input ends, the client has gone: the node MUST end, as if it had
  received `exit`.
- Either side MAY refuse a line longer than the other side's `maxMessageBytes`
  ([6.3](#63-limits)). A node answers it with an `InvalidRequest` error whose `id` is `null`,
  and reads on from the next line.

### 3.2 The network

CNP over WebSocket with TLS, with credentials given when the connection opens, is reserved for
a later minor version ([13](#13-reserved-for-later-versions)). The messages will be the same.

## 4. Messages

Every message is a [JSON-RPC 2.0](https://www.jsonrpc.org/specification) message:

- A **request** has an `id` and a `method`, and MAY have `params`, an object. It is answered by
  exactly one response with the same `id`. An `id` is a whole number or a string. The sender
  MUST NOT reuse an `id` while its request is still unanswered.
- A **notification** has a `method` and no `id`, and is never answered.
- A **response** has the request's `id` and either a `result` or an `error`. `"result": null` is
  a result. The `id` is `null` only when the request's could not be read.

```cnp
→ {"jsonrpc": "2.0", "id": 1, "method": "ping"}
← {"jsonrpc": "2.0", "id": 1, "result": null}
```

In version 1.0 only the client sends requests. A node sends responses, and the notifications
this specification defines. Responses MAY come in any order: a node answers `ping`, `engine/stop`
and other requests while a search runs. Batches (arrays of messages) are not part of CNP; a node
answers one with an `InvalidRequest` error.

A node MUST answer a request for a method it doesn't know with `MethodNotFound`, and MUST ignore
a notification it doesn't know.

### 4.1 Cancelling a request

`$/cancelRequest` is a notification that drops a request:

```cnp
→ {"jsonrpc": "2.0", "id": 7, "method": "engine/search", "params": {"position": {"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"}, "limits": {"infinite": true}}}
→ {"jsonrpc": "2.0", "method": "$/cancelRequest", "params": {"id": 7}}
← {"jsonrpc": "2.0", "id": 7, "error": {"code": -32800, "message": "cancelled"}}
```

Unless the request has already been answered, the node MUST answer it promptly with a
`Cancelled` error, and nothing the request did counts. For a search, that means its move is
never played: a takeback or a new game came while it was thinking. Cancelling a request that has
already been answered, or that never existed, does nothing: the two messages can cross.

## 5. Lifecycle

```text
  client                                       node
  initialize {protocol, client} ────────────▶
                ◀────────────── {protocol, node, capabilities, limits}
  … requests, notifications, ping at any time …
  shutdown ─────────────────────────────────▶  ends its work
                ◀────────────── null
  exit ─────────────────────────────────────▶  ends, with exit code 0
```

### 5.1 `initialize`

The first request of every connection. Before it, a node MUST answer every other request with
`NotInitialized`, and ignore notifications other than `exit`. A second `initialize` is answered
with `InvalidRequest`.

Params:

| Field | Type | |
| --- | --- | --- |
| `protocol` | string | The newest version of the protocol the client speaks, `major.minor` |
| `client` | object | The client: `name` and `version`, strings |

Result:

| Field | Type | |
| --- | --- | --- |
| `protocol` | string | The version both sides speak from now on |
| `node` | object | The node: `name`, `version`, `license` (an [SPDX](https://spdx.org/licenses/) expression), and optionally `author` and `source` (a URL to its source code) |
| `capabilities` | object | What the node can do ([6.1](#61-capabilities)) |
| `limits` | object | How far it will go ([6.3](#63-limits)) |

The version spoken is the client's major version, with the lower of the two minor versions. If
the node doesn't speak the client's major version, it answers with `IncompatibleVersion`, whose
`data` lists the newest version of each major version it does speak. A failed `initialize`
leaves the node waiting for one, so the client may try again with another version:

```cnp
→ {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocol": "2.0", "client": {"name": "Solrac", "version": "3.0.0"}}}
← {"jsonrpc": "2.0", "id": 1, "error": {"code": -32010, "message": "this node speaks CNP 1.x", "data": {"supported": ["1.0"]}}}
```

A node states its licence and where its source is, so that a client can show them.

### 5.2 `ping`

A request with no params, answered with `null`. A node MUST answer it promptly at any time after
`initialize`, even mid-search, so a client can tell a busy node from a dead one.

### 5.3 `shutdown` and `exit`

`shutdown` is a request with no params. The node ends its running work as if each request had
been cancelled ([4.1](#41-cancelling-a-request)), then answers `null`. After it, the node
answers every request with `ShuttingDown`.

`exit` is a notification with no params. The node ends: with exit code 0 if `shutdown` came
first, and 1 otherwise. The end of the node's input means the same as `exit`.

### 5.4 A whole session

```cnp
→ {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocol": "1.0", "client": {"name": "Solrac", "version": "0.2.0"}}}
← {"jsonrpc": "2.0", "id": 1, "result": {"protocol": "1.0", "node": {"name": "cb2026", "version": "0.9.0", "license": "MIT OR Apache-2.0", "source": "https://github.com/cjus/chessengine"}, "capabilities": {"engine": {"multipv": true, "window": true, "rootMoves": true, "strength": true, "techniques": true, "deterministic": true, "clock": true, "mate": true, "chess960": true, "progress": true}}, "limits": {"maxDepth": 128, "maxTimeMs": 3600000, "maxMultipv": 8, "maxConcurrentSearches": 4}}}
→ {"jsonrpc": "2.0", "id": 2, "method": "engine/search", "params": {"position": {"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "moves": ["e2e4", "e7e5"]}, "limits": {"timeMs": 1000}, "progressMs": 250, "game": "g1"}}
← {"jsonrpc": "2.0", "method": "engine/progress", "params": {"id": 2, "depth": 12, "selDepth": 19, "score": {"cp": 24}, "pv": ["g1f3", "b8c6", "f1b5"], "nodes": 412530, "nps": 1650120, "timeMs": 250}}
← {"jsonrpc": "2.0", "id": 2, "result": {"bestMove": "g1f3", "ponder": "b8c6", "score": {"cp": 27}, "pv": ["g1f3", "b8c6", "f1b5", "a7a6"], "depth": 16, "selDepth": 27, "nodes": 1702214, "timeMs": 1000, "stopped": false, "stats": {"hashFull": 87}}}
→ {"jsonrpc": "2.0", "id": 3, "method": "shutdown"}
← {"jsonrpc": "2.0", "id": 3, "result": null}
→ {"jsonrpc": "2.0", "method": "exit"}
```

## 6. Capabilities and limits

### 6.1 Capabilities

`capabilities` has an object for each domain the node serves: `engine`
([7](#7-the-engine-domain)) and `voice` ([8](#8-the-voice-domain)). A domain that is missing is
not served, and its requests are answered with `MethodNotFound`. Inside a domain's object each
capability is a boolean, and a missing one is `false`. A client MUST NOT use a capability the
node didn't offer. A node MUST answer a request that needs one with `Unsupported`.

### 6.2 Engine capabilities

Every engine node answers `engine/search` with the limits `depth`, `nodes`, `timeMs` and
`infinite`, and answers `engine/stop`. Beyond that:

| Capability | What it allows |
| --- | --- |
| `multipv` | `multipv` above 1, up to `maxMultipv` |
| `window` | `window` |
| `rootMoves` | `rootMoves` |
| `strength` | `strength`, and `engine/levels` |
| `techniques` | `techniques`, and `engine/techniques` |
| `deterministic` | `deterministic` |
| `clock` | `limits.clock` |
| `mate` | `limits.mate` |
| `chess960` | Positions with `"chess960": true`: Chess960 starting positions and castling |
| `progress` | `progressMs`, and `engine/progress` |

### 6.3 Limits

`limits` says how far a node will go. A request beyond a limit MUST be refused with
`LimitExceeded`, never trimmed silently. A missing limit has the default below, or none.

| Limit | Type | Default | |
| --- | --- | --- | --- |
| `maxDepth` | integer | none | The deepest search, in plies |
| `maxNodes` | integer | none | The most nodes one search may visit |
| `maxTimeMs` | integer | none | The longest one search may take. A search with no limits, or an infinite one, also ends here |
| `maxMultipv` | integer | 1 | The most lines one search may return |
| `maxConcurrentSearches` | integer | 1 | How many searches may run at once; one more is refused with `Busy` |
| `maxMessageBytes` | integer | 16777216 | The longest message the node reads |

## 7. The engine domain

| Method | Kind | |
| --- | --- | --- |
| `engine/search` | request | Search a position and answer with the best move |
| `engine/progress` | notification, node to client | How a search is going |
| `engine/stop` | request | End a search early, keeping its result |
| `engine/techniques` | request | The search techniques that can be switched |
| `engine/levels` | request | The playing levels and their ratings |

### 7.1 `engine/search`

Params:

| Field | Type | Needs | |
| --- | --- | --- | --- |
| `position` | object | | The position ([2](#2-conventions)) |
| `limits` | object | | When to stop (below). Missing or empty: the node decides, within its `maxTimeMs` |
| `strength` | object | `strength` | How strongly to play: `{"level": 12}`, a level from `engine/levels` (fractional if it says so), or `{"elo": 1600}`, a rating within its `elo` range. One of the two. Missing: full strength |
| `techniques` | object | `techniques` | Techniques switched on or off, by id: `{"nullMove": false}`. Missing ones keep their defaults. An unknown id is `Unsupported` |
| `rootMoves` | array of moves | `rootMoves` | Search only these moves at the root. Missing or empty: all |
| `window` | object | `window` | Search the root within `{"alpha": a, "beta": b}`, in centipawns, `a` < `b`. A score at or below `alpha` comes back as an upper bound, one at or above `beta` as a lower bound |
| `multipv` | integer | `multipv` | How many lines to find, the best first. Missing: 1 |
| `deterministic` | boolean | `deterministic` | One thread and a cleared hash table, so the same request gives the same result, node counts included |
| `progressMs` | integer | `progress` | Send `engine/progress` at most this often. Missing: no progress |
| `game` | string | | A hint that searches with the same `game` belong to one game, so the node may keep its hash table between them. A node MAY ignore it |

`limits`, any of which may be combined; the search stops at the first reached:

| Field | Type | Needs | |
| --- | --- | --- | --- |
| `depth` | integer | | Search this many plies deep |
| `nodes` | integer | | Search about this many nodes |
| `timeMs` | integer | | Spend this long |
| `mate` | integer | `mate` | Look for a mate in this many moves |
| `clock` | object | `clock` | Play to a game clock: `white` and `black`, each `{"timeMs": t, "incrementMs": i}`, and optionally `movesToGo`. The node decides how much of it to use |
| `infinite` | boolean | | Search until `engine/stop` or `$/cancelRequest`, or the node's `maxTimeMs` |

A position whose FEN isn't valid, or with a move that isn't legal, is answered with
`InvalidPosition`, and so is a root move that isn't legal.

Result:

| Field | Type | |
| --- | --- | --- |
| `bestMove` | move or `null` | The move to play. `null` when the side to move has no legal move (checkmate or stalemate) |
| `ponder` | move | The reply the engine expects, if it has one |
| `score` | score | The best line's score. Missing when there is no legal move, or when the search was stopped before it scored a line |
| `pv` | array of moves | The best line, starting with the best move |
| `depth` | integer | The depth completed |
| `selDepth` | integer | The deepest any line went, extensions and quiescence included |
| `nodes` | integer | Nodes searched, every thread together |
| `timeMs` | integer | Time spent |
| `stopped` | boolean | Whether `engine/stop` ended the search before its limits |
| `lines` | array | When `multipv` was above 1: every line, the best first, each with `multipv` (its rank from 1), `score`, `pv`, `depth` and `selDepth`. The first is the same as `score` and `pv` |
| `stats` | object | Optionally `hashFull` (thousandths) and `tbHits` (tablebase probes that gave a result) |

```cnp
→ {"jsonrpc": "2.0", "id": 7, "method": "engine/search", "params": {"position": {"fen": "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3"}, "limits": {"depth": 18}, "rootMoves": ["f1b5", "f1c4"], "multipv": 2}}
← {"jsonrpc": "2.0", "id": 7, "result": {"bestMove": "f1b5", "ponder": "a7a6", "score": {"cp": 31}, "pv": ["f1b5", "a7a6", "b5a4"], "depth": 18, "selDepth": 29, "nodes": 3120554, "timeMs": 1806, "stopped": false, "lines": [{"multipv": 1, "score": {"cp": 31}, "pv": ["f1b5", "a7a6", "b5a4"], "depth": 18, "selDepth": 29}, {"multipv": 2, "score": {"cp": 22}, "pv": ["f1c4", "g8f6", "d2d3"], "depth": 18, "selDepth": 26}]}}
```

A search with no legal move answers at once:

```cnp
→ {"jsonrpc": "2.0", "id": 8, "method": "engine/search", "params": {"position": {"fen": "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3"}, "limits": {"depth": 10}}}
← {"jsonrpc": "2.0", "id": 8, "result": {"bestMove": null, "pv": [], "depth": 0, "nodes": 0, "timeMs": 0, "stopped": false}}
```

### 7.2 `engine/progress`

A notification from the node, sent only for a search that asked with `progressMs`, at most that
often, and never after the search's result. A node SHOULD send one when a depth completes, and
MUST send at least one before the result of a search that completed a depth. Its params:

| Field | Type | |
| --- | --- | --- |
| `id` | id | The `engine/search` request it belongs to |
| `depth`, `selDepth` | integers | As in the result |
| `score` | score | The line's score |
| `pv` | array of moves | The line |
| `multipv` | integer | Which line, from 1, when searching several |
| `nodes`, `timeMs` | integers | So far |
| `nps` | integer | Nodes a second |
| `hashFull`, `tbHits` | integers | As in `stats` |

### 7.3 `engine/stop`

Ends a search early. Params: `{"id": …}`, the search's request id. The result is `null`, and
the search answers at once with the best it has found, with `stopped` true. Even a search stopped
before it finished its first depth answers with a legal move, if the position has one. Stopping a search
that has already answered, or that never existed, is not an error, because the two messages can
cross. Use `engine/stop` for "move now", and `$/cancelRequest` when the result is no longer
wanted. A search stopped before it scored a line answers with its move and no `score`.

```cnp
→ {"jsonrpc": "2.0", "id": 9, "method": "engine/search", "params": {"position": {"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"}, "limits": {"infinite": true}}}
→ {"jsonrpc": "2.0", "id": 10, "method": "engine/stop", "params": {"id": 9}}
← {"jsonrpc": "2.0", "id": 10, "result": null}
← {"jsonrpc": "2.0", "id": 9, "result": {"bestMove": "e2e4", "score": {"cp": 30}, "pv": ["e2e4", "e7e5"], "depth": 21, "selDepth": 33, "nodes": 24000312, "timeMs": 9120, "stopped": true}}
```

### 7.4 `engine/techniques`

Needs `techniques`. No params. The result lists every technique a search can switch:

| Field | Type | |
| --- | --- | --- |
| `techniques` | array | Each with `id` (what `techniques` in a search calls it), `name`, `description` (a sentence or two), `default` (whether it is on unless a search says), and optionally `requires` (ids it needs on to have any effect) and `link` (where to read about it) |

```cnp
→ {"jsonrpc": "2.0", "id": 11, "method": "engine/techniques"}
← {"jsonrpc": "2.0", "id": 11, "result": {"techniques": [{"id": "pvs", "name": "Principal variation search", "description": "Moves after the first get a null-window search first.", "default": true, "link": "https://www.chessprogramming.org/Principal_Variation_Search"}, {"id": "nullMove", "name": "Null-move pruning", "description": "Cut off when passing still keeps the score above beta.", "default": true, "requires": ["pvs"]}]}}
```

Technique ids are the engine's own. Two engines may call the same idea by different ids.

### 7.5 `engine/levels`

Needs `strength`. No params. The result:

| Field | Type | |
| --- | --- | --- |
| `levels` | array | Every level, weakest first, each `{"level": n, "rating": r}`. The last is full strength |
| `ratingScale` | string | What the ratings mean, like `CCRL Blitz` |
| `fractional` | boolean | Whether a level between two listed ones may be asked for |
| `elo` | object | `{"min": a, "max": b}`, the ratings `strength.elo` may ask for. Missing: only levels |

With cb2026's measured levels (three of its twenty-one shown):

```cnp
→ {"jsonrpc": "2.0", "id": 12, "method": "engine/levels"}
← {"jsonrpc": "2.0", "id": 12, "result": {"levels": [{"level": 0, "rating": 629}, {"level": 10, "rating": 1391}, {"level": 20, "rating": 2782}], "ratingScale": "CCRL Blitz", "fractional": true, "elo": {"min": 650, "max": 2450}}}
```

## 8. The voice domain

A voice node turns text into speech and speech into text. It never touches a microphone or a
speaker: the client records and plays. To listen, the client streams what its microphone
hears, and the node says when someone speaks and what they said.

| Method | Kind | |
| --- | --- | --- |
| `voice/voices` | request | The voices it can speak with |
| `voice/speak` | request | Text to speech |
| `voice/transcribe` | request | Speech to text |
| `voice/models` | request | Its models, and how ready each is |
| `voice/modelState` | notification, node to client | A model's state changed |
| `voice/listen` | request | Hear speech as it comes, until the audio ends |
| `voice/audio` | notification, client to node | The next piece of a listen's audio |
| `voice/speechStarted` | notification, node to client | Someone has started speaking |
| `voice/heard` | notification, node to client | What they said, once they paused |

Capabilities:

| Capability | Type | |
| --- | --- | --- |
| `speak` | boolean | `voice/speak` |
| `transcribe` | boolean | `voice/transcribe` |
| `listen` | boolean | `voice/listen`, and the notifications that go with it |
| `wordTimes` | boolean | Each word's start and end, in either direction |
| `formats` | array of strings | The audio formats it reads and writes |

Audio formats:

- `wav`: a RIFF WAVE file, mono, with 16-bit integer or 32-bit float samples;
- `pcmF32`: raw mono samples, 32-bit float, little-endian, at a stated sample rate.

A node downloads, checks and loads its own models. Until a model is ready, a request that
needs it is answered with `NotReady`. A node checks a request's params first, so a request with
wrong params is `InvalidParams` whether or not the models are ready.

### 8.1 `voice/voices`

No params. The result: `voices`, each with `id`, `name`, `language` (a
[BCP 47](https://www.rfc-editor.org/info/bcp47) tag like `en-US`) and optionally `description`.
The first voice is the default.

### 8.2 `voice/speak`

Params:

| Field | Type | |
| --- | --- | --- |
| `text` | string | Plain words, no markup |
| `voice` | string | A voice's id. Missing: the default |
| `speed` | number | 1.0 is the voice's own pace |
| `format` | string | The audio format wanted. Missing: `wav` |
| `wordTimes` | boolean | Return each word's start and end, if the model can tell (needs `wordTimes`) |

An unknown voice is `InvalidParams`. The result:

| Field | Type | |
| --- | --- | --- |
| `format` | string | The audio's format |
| `sampleRate` | integer | Samples a second |
| `audio` | string | The audio, base64 |
| `durationMs` | integer | Its length |
| `words` | array | If asked for and known: each word's `text`, `startMs` and `endMs` |

The audio is shortened here:

```cnp
→ {"jsonrpc": "2.0", "id": 20, "method": "voice/speak", "params": {"text": "Knight to f3.", "voice": "am_michael", "format": "pcmF32", "wordTimes": true}}
← {"jsonrpc": "2.0", "id": 20, "result": {"format": "pcmF32", "sampleRate": 24000, "audio": "AAAAAAAAAAA=", "durationMs": 1150, "words": [{"text": "Knight", "startMs": 40, "endMs": 390}, {"text": "to", "startMs": 390, "endMs": 520}, {"text": "f3", "startMs": 520, "endMs": 1100}]}}
```

### 8.3 `voice/transcribe`

Params:

| Field | Type | |
| --- | --- | --- |
| `audio` | string | The speech, base64 |
| `format` | string | Its format |
| `sampleRate` | integer | For `pcmF32` (a WAV file says its own) |
| `language` | string | The language spoken, a BCP 47 tag. Missing: the node's default |
| `wordTimes` | boolean | Return each word's start and end (needs `wordTimes`) |

Audio that can't be decoded is `InvalidParams`. The result: `text` (empty if nothing was said)
and, if asked for, `words`.

### 8.4 `voice/models` and `voice/modelState`

`voice/models` takes no params. Its result is `models`, each with `id`, `name`, `purpose`
(`speak`, `transcribe`, or `listen` for one that hears when speech starts and ends),
optionally `license`, and `state`. A state has a `stage`, one of
`waiting`, `downloading`, `unpacking`, `loading`, `ready` and `failed` in that order. While
downloading it has `bytes` so far and `of` in all, if known. When failed it has a `message` for
people.

`voice/modelState` is a notification from the node, `{"id": …, "state": …}`, sent at each new
stage, and while downloading at most about once a second.

```cnp
→ {"jsonrpc": "2.0", "id": 21, "method": "voice/models"}
← {"jsonrpc": "2.0", "id": 21, "result": {"models": [{"id": "kokoro", "name": "Kokoro 82M", "purpose": "speak", "license": "Apache-2.0", "state": {"stage": "downloading", "bytes": 52000000, "of": 126000000}}]}}
← {"jsonrpc": "2.0", "method": "voice/modelState", "params": {"id": "kokoro", "state": {"stage": "ready"}}}
```

### 8.5 `voice/listen`

Needs `listen`. The client streams what its microphone hears, and the node hears it as it
comes: it says when someone starts speaking, and, once they pause, what they said. A player
can talk to the client without pressing anything. A listen runs until its audio ends or it is
cancelled.

Params:

| Field | Type | |
| --- | --- | --- |
| `format` | string | The audio's format: `pcmF32`, the format that streams |
| `sampleRate` | integer | Samples a second. A node MUST take 16000, and MAY refuse other rates with `InvalidParams` |
| `language` | string | The language spoken, a BCP 47 tag. Missing: the node's default |
| `endSilenceMs` | integer | How long a pause ends what someone is saying. Missing: the node decides |
| `minSpeechMs` | integer | Shorter sounds aren't speech (a cough, a click): nothing is said of them. Missing: the node decides |
| `maxSpeechMs` | integer | The longest one utterance may be: at this length it ends, pause or not. Missing: the node decides |
| `wordTimes` | boolean | Return each word's start and end in `voice/heard` (needs `wordTimes`) |

Wrong params are `InvalidParams`, a duration of 0 or one the node can't keep to included.
Until the models a listen needs are ready, it is `NotReady`. A node MAY hear one listen at a
time, and answer another with `Busy`.

The audio comes in `voice/audio` notifications from the client, in order:

| Field | Type | |
| --- | --- | --- |
| `id` | id | The listen it belongs to |
| `audio` | string | The next samples, base64, in the listen's format and rate. May be left out with `end` |
| `end` | boolean | The audio has ended: no more comes for this listen |

A node ignores audio for a listen it has already answered, or never had, since the two can
cross. Audio that can't be decoded ends the listen with `InvalidParams`.

Times are milliseconds into the listen's audio, from its first sample, not clock times. As the
node hears each utterance, it sends:

- **`voice/speechStarted`**, `{"id": …, "startMs": …}`, once it is sure someone is speaking
  (after `minSpeechMs` of speech). `startMs` is about where they started.
- **`voice/heard`**, once they have paused for `endSilenceMs` and the node has transcribed what
  they said: `id`, `text` (empty if no words were made out), `startMs` and `endMs` (where the
  speech starts and ends), and `words` if asked for.

Each `voice/speechStarted` is followed by one `voice/heard`, and an utterance's `voice/heard`
comes before the next one's `voice/speechStarted`. When the audio ends (`end`), the node ends
the utterance it is hearing, if any, as if the speaker had paused, sends its `voice/heard`,
and answers the listen. The result is `durationMs`, how much audio it heard in all. A
cancelled listen ([4.1](#41-cancelling-a-request)) drops whatever it was hearing: no
`voice/heard` follows its answer.

The audio is shortened here:

```cnp
→ {"jsonrpc": "2.0", "id": 30, "method": "voice/listen", "params": {"format": "pcmF32", "sampleRate": 16000, "endSilenceMs": 800}}
→ {"jsonrpc": "2.0", "method": "voice/audio", "params": {"id": 30, "audio": "AAAAAAAAAAA="}}
← {"jsonrpc": "2.0", "method": "voice/speechStarted", "params": {"id": 30, "startMs": 1240}}
→ {"jsonrpc": "2.0", "method": "voice/audio", "params": {"id": 30, "audio": "AAAAAAAAAAA="}}
← {"jsonrpc": "2.0", "method": "voice/heard", "params": {"id": 30, "text": "Knight to f3.", "startMs": 1210, "endMs": 2380}}
→ {"jsonrpc": "2.0", "method": "voice/audio", "params": {"id": 30, "end": true}}
← {"jsonrpc": "2.0", "id": 30, "result": {"durationMs": 4000}}
```

## 9. Errors

An error has a `code`, a `message` for people, and optionally `data`. A node MAY use codes of
its own outside the ranges below.

| Code | Name | When |
| --- | --- | --- |
| -32700 | `ParseError` | The line isn't JSON. The `id` is `null` |
| -32600 | `InvalidRequest` | The JSON isn't a valid message, or the request isn't allowed now (a second `initialize`, a batch, a line too long) |
| -32601 | `MethodNotFound` | An unknown method, or one of a domain the node doesn't serve |
| -32602 | `InvalidParams` | Params missing, of the wrong shape, or with a wrong value |
| -32603 | `InternalError` | Something went wrong inside the node |
| -32002 | `NotInitialized` | A request before `initialize` |
| -32800 | `Cancelled` | The request was cancelled ([4.1](#41-cancelling-a-request)) |
| -32010 | `IncompatibleVersion` | The client's major version isn't spoken. `data`: `{"supported": [...]}` |
| -32011 | `InvalidPosition` | The FEN, a move, or a root move isn't legal |
| -32012 | `LimitExceeded` | A request beyond the node's limits ([6.3](#63-limits)) |
| -32013 | `Busy` | Already running `maxConcurrentSearches` searches |
| -32014 | `Unsupported` | A capability the node didn't offer, or an unknown technique |
| -32015 | `Unauthorized` | On the network, without valid credentials |
| -32016 | `ShuttingDown` | A request after `shutdown` |
| -32017 | `NotReady` | What the request needs isn't ready yet, like a model still downloading |

## 10. Security

A node started by a client trusts that client. A node on the network trusts no one. Version 1.0
already holds both to these rules, so that the network can be added without changing them:

- **No file paths in any request.** Opening books, tablebases and models are the node's own
  configuration. A request that can name a file is how a server is turned against itself.
- **Advertised limits** for depth, nodes, time, lines, concurrent searches and message size.
  Requests beyond them are refused, not trimmed.
- **Credentials never travel in messages,** and are never logged.

## 11. Versions

- The protocol has a major and a minor version. Programs with different major versions don't
  talk ([5.1](#51-initialize)).
- A new minor version only adds: optional fields, methods, notifications and capabilities.
  Unknown fields are ignored, unknown requests answered `MethodNotFound` and unknown
  notifications ignored, so older and newer programs work together. A new capability is only
  used when offered.
- Everything else, such as a change of meaning or a removed field, needs a new major version.

## 12. Conformance

The protocol's repository holds conformance sessions: exchanges written as JSON lines, saying
what to send and what must come back. `cnp-conformance` plays them against any node program
over stdin/stdout, and skips those that need capabilities the node doesn't offer. A node
conforms to CNP 1.0 if it passes every session that applies to it. See `conformance/README.md`.

## 13. Reserved for later versions

These names are reserved, so that nothing else uses them:

- **WebSocket over TLS** ([3.2](#32-the-network)), with credentials given when the connection
  opens, never in messages.
- **The search tree:** `engine/tree` searches a position and keeps its tree for a while,
  answering with a handle and the root's children. `engine/treeNode` returns one node's
  children, a page at a time. A handle expires after a stated time.
- **Distributed search:** `worker/register`, which a worker sends after connecting out to a
  coordinator, with its capabilities and hardware. Either side may then send requests.
- **Streamed speech:** `voice/speak` sending its audio piece by piece.
- **A binary encoding** (CBOR) of the same messages, offered in `initialize`, for audio and
  large trees.

## 14. Changes

- **1.0 (draft)**, 2026-10-21: the first version. Before it was final: `chess960` in a
  position, so a Chess960 game says so; and listening (`listen`, `voice/listen`,
  `voice/audio`, `voice/speechStarted`, `voice/heard`, and the purpose `listen`), so a player
  can talk to Solrac without holding a button (cjus/chessgui#134). Its design came from
  Solrac, a chess board that teaches, and cb2026, its engine.
