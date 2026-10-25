# Games over CNP — draft 1

This is a **proposed CNP extension**, submitted for review alongside chessdb's implementation
and Solrac's game library (cjus/chessgui#173). It is not an accepted addition to upstream CNP.
It uses CNP 1.0's transport, lifecycle, envelopes and error codes unchanged; the typed Rust
markers and limits are `cnp::games`. Like the [preferences extension](preferences.md), it is
served by a node that keeps a program's data, such as chessdb, and its first client is
Solrac.

A client keeps a library of games in the node: it saves each game it plays under an identity
of its own, lists them newest first, and reads one back whole to replay it or carry on with it.
The node keeps no chess rules. A game travels as its PGN with the few facts a list shows beside
it, and the node checks their shapes and sizes, not their chess: the client is responsible for
the moves.

## Capability

Initialize's `capabilities` names the domain, with its contract:

```json
{"capabilities": {"games": {"read": true, "write": true, "search": true, "delete": true,
 "import": true, "contract": "draft-1"}}}
```

`read` offers `games/list` and `games/get`, `write` offers `games/save`, `search` says
that `games/list` takes a `query`, `delete` offers `games/delete`, and `import` offers
`games/import`. A missing switch is false: a node without `search` ignores
a query, as it ignores any unknown field, and lists every game, so a client sends a query only
to a node that searches. A client sends
games requests only to a node whose `contract` is `draft-1`, and a node serving games reads
messages of at least 64 KiB (`limits.maxMessageBytes` of 65,536 or more), which the bounds
below are made for. A node without the domain answers
them with Method not found (-32601), as CNP does for any domain it doesn't serve.

## A game

```json
{
  "id": "5f0c2a9e7b1d4c3a8e6f0b2d1c3a4e5f",
  "version": 12,
  "white": "Player",
  "black": "cb2026 0.1.0",
  "result": "*",
  "plies": 23,
  "pgn": "[Event \"Solrac game\"]\n[Date \"2026.10.24\"]\n...\n\n1. e4 e5 2. Nf3 Nc6 ... *\n",
  "players": {"white": "human", "black": "engine"},
  "clock": {"control": "5+3", "timesMs": [[300000, 300000], [300000, 300000], [298200, 300000]]}
}
```

| Field | What it is | Bounds |
| --- | --- | --- |
| `id` | The game's identity, chosen by the client and kept for the game's life | 1–64 bytes: lowercase letters, digits, `-` |
| `version` | The client's count of this game's saves, higher with every save | 1 to 2^53 − 1 |
| `white`, `black` | The players' names, as a list shows them | 1–128 bytes, no control characters |
| `result` | `1-0`, `0-1`, `1/2-1/2`, or `*` for a game not over | |
| `plies` | How many moves (plies) have been played | 0–1000 |
| `pgn` | The whole game in PGN | 1–32,768 bytes |
| `players` | Optional: who played each side, `human` or `engine` | |
| `clock` | Optional: the time control as the client writes it, and each side's time left in milliseconds in every position from the start | control 1–32 bytes; `plies` + 1 pairs; each at most 360,000,000 (100 hours) |

The whole game, as JSON, is at most 61,440 bytes, so that it and the envelope of a `games/get`
answer fit a 64 KiB message. A game outside these bounds is refused with Invalid params
(-32602), never trimmed. Unknown fields are ignored, as CNP ignores them everywhere.

## Requests

### `games/save`

```json
{"game": { ... a game ... }}
```

Answers, after the commit, with what is now stored (a **summary**):

```json
{"id": "5f0c2a9e7b1d4c3a8e6f0b2d1c3a4e5f", "version": 12, "white": "Player",
 "black": "cb2026 0.1.0", "result": "*", "plies": 23,
 "createdAt": "2026-10-24T14:05:12.123Z", "updatedAt": "2026-10-24T14:31:40.002Z"}
```

Versions make a save safe to repeat:

| Stored | What the save does | Answer |
| --- | --- | --- |
| Nothing with this `id` | Stores the game; `createdAt` and `updatedAt` are now | The new summary |
| A lower version | Replaces it; `updatedAt` is now, `createdAt` stays | The new summary |
| This version, the same game | Nothing: an exact retry after a lost answer | The stored summary |
| This version, a different game | Nothing | Error -32021 (version reused) |
| A higher version | Nothing | The stored summary, whose higher `version` tells the client that what is stored is newer than what it sent |

A client that finds its game older than what is stored, but knows its own is the one in play
(it has the board), saves again above the stored version.

Timestamps are the node's, in UTC, RFC 3339 with milliseconds. A success is sent only after
the change is durably committed, as for preferences. A lost connection leaves the outcome
uncertain: retry the exact save.

### `games/list`

```json
{"limit": 20, "after": "opaque cursor", "query": "carlsen 2019"}
```

Each is optional: `limit` is 1–100 and 20 when missing, `after` is the `next` of the page
before, passed back unchanged with the same query, and `query`, for a node that searches,
lists only the games that match it. Answers with the games the most recently changed first
(by `updatedAt`, then `id`), as summaries:

```json
{"games": [ ... summaries ... ], "next": "opaque cursor"}
```

`next` is missing on the last page. A cursor that isn't one the node gave is Invalid params.

A **query** is 1–128 bytes, with no control characters and at least one word (words are split
at whitespace). A game matches it when each word, ignoring case (Unicode's lowercase), is part
of White's or Black's name or of the value of one of the tag pairs that start its PGN, such as
`[Event "Rapid Open"]` or `[Date "2019.06.04"]` (`\"` and `\\` in a value read as `"` and `\`).
The moves, and their comments, aren't searched. A query out of bounds is Invalid params.
`cnp::games::matches` is the rule as code.

### `games/get`

```json
{"id": "5f0c2a9e7b1d4c3a8e6f0b2d1c3a4e5f"}
```

Answers with the game as it was last saved, and its timestamps:

```json
{"game": { ... a game ... }, "createdAt": "2026-10-24T14:05:12.123Z",
 "updatedAt": "2026-10-24T14:31:40.002Z"}
```

An identity the node doesn't keep is error -32025 (not found).

### `games/delete`

```json
{"id": "5f0c2a9e7b1d4c3a8e6f0b2d1c3a4e5f"}
```

For a node whose capability says `delete`. Deletes the game, every version of it, and
answers after the commit, as a save does:

```json
{"id": "5f0c2a9e7b1d4c3a8e6f0b2d1c3a4e5f"}
```

An identity the node doesn't keep is -32025, so a client that retries after losing the
answer finds the game gone and can take that as done. A save of the same identity afterwards
stores it again, as a new game: a client deletes only a game it is no longer saving.

### `games/import`

```json
{"games": [{ ... a game ... }, { ... a game ... }]}
```

For a node whose capability says `import`. Saves every game in **one commit**, each as
`games/save` would, and answers after the commit with what it did with each, in the order
sent:

```json
{"games": ["saved", "kept", "saved"]}
```

| Answer | What happened |
| --- | --- |
| `saved` | Stored: nothing had this `id`, or a lower version did, which it replaced |
| `kept` | Nothing: this version of the same game, or a higher version, is stored |
| `refused` | Nothing: this version was saved with a different game (-32021, for a save) |

A batch is 1–100 games, no `id` twice, and its games together are at most 61,440 bytes as
JSON, each counted as for one game: so a batch fits the message one game does. A batch
outside these bounds, or with a game outside a game's, saves nothing: Invalid params
(-32602). A file of thousands of games is imported batch by batch, so a node commits once
a batch, not once a game (a durable commit waits for the disk). Repeating a batch is
harmless, as repeating a save is: after a lost answer, retry it exactly. `cnp::games::Batcher`
gathers a client's games into batches that keep within the bounds.

## Errors

| Error | Meaning | Client action |
| --- | --- | --- |
| -32602 | Invalid params: a game, batch, limit, cursor, query or identity out of bounds | Correct the request |
| -32013 | Busy: another storage request is active | Retry with backoff |
| -32021 | The version was already saved with a different game | A programming error: save above the stored version, never over it |
| -32022 | Storage or I/O failed; the outcome can be uncertain | Keep the game; retry the exact save |
| -32025 | No game with that identity | Refresh the list; for a delete, it is already gone |
| -32800 | Cancelled before the commit | Nothing was saved by this attempt |

## Not in draft 1

Searching by moves or positions, and the positions of each game, are later work. A node that adds them adds methods or capabilities,
and an older client ignores them.
