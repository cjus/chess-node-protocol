# Knowledge over CNP — draft 1

This is a **proposed CNP extension**, submitted for review alongside chessdb's knowledge
library (cjus/chessdb#2) and Solrac's downloads (cjus/chessgui). It is not an accepted addition
to upstream CNP. It uses CNP 1.0's transport, lifecycle, envelopes and error codes unchanged;
the typed Rust markers and limits are `cnp::knowledge`. It is served by a node that keeps a
program's data, such as chessdb, beside the [preferences](preferences.md), [games](games.md)
and [backups](backups.md) extensions.

The knowledge is what the player didn't make: puzzles, evaluated positions and strategic test
positions from outside sources. Draft 1 has the first half of the domain: **a client hands the
node a file it has downloaded, and the node imports it.** Asking for what was imported (puzzles
by theme and rating, the analyses of a position) is the next draft.

The node doesn't download. The client fetches the file, where it can show the licence, the
size and the progress, and tells the node where the file is. The node checks the file against
the checksum the client says it has, so a damaged or swapped download is refused, not imported.

## Capability

```json
{"capabilities": {"knowledge": {"import": true,
 "kinds": ["lichess-puzzles", "lichess-evals", "sts-epd"], "contract": "draft-1"}}}
```

`import` offers `knowledge/import`, `knowledge/status` and `knowledge/cancel`. A missing switch
is false. `kinds` are the kinds of file the node can import; a request for another kind is
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
| -32602 | Invalid params: an unknown kind, or a malformed path, manifest or checksum | Correct the request |
| -32013 | Busy: another import is running | Wait for it, or cancel it |
| -32022 | Storage or I/O failed | Ask `knowledge/status` for the import's state |
| -32025 | No import with that identity | List the imports |
| -32027 | The file doesn't exist, isn't a file, or can't be read | Ask the player for the file again |
| -32800 | Cancelled before the request was done | Nothing changed |

## Not in draft 1

Reading the knowledge (puzzles by theme and rating, the analyses of a position, strategic
exercises), progress notifications in place of polling, reading the records an import
rejected, removing an import's knowledge, and Lumbra's OTB games.
