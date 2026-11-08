# The Chess Node Protocol

**Version 1.0, draft** · © 2026 Carlos Justiniano · Licensed under
[Creative Commons Attribution 4.0 International](LICENSE): anyone may copy, implement and
adapt this specification if they credit it.

The Chess Node Protocol (CNP) connects a chess program, the **client**, to the programs that
serve it, the **nodes**: chess engines, and voice modules that speak and listen. It does what
UCI does for engines and more besides. A search can be limited to some root moves or a score
window, the engine's techniques can be switched on and off for teaching, a search can ask for
progress, every score says whether it is exact or a bound, and an engine can say what its
evaluation of a position is made of. The same messages reach a node
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
  clock times, since clocks on different machines differ. The one exception is a search tree's
  handle ([7.8](#78-the-search-tree)), which names a tree an earlier request recorded, so that a
  client can read a large tree a part at a time. It is a cache: a client that has lost it asks
  for the tree again.
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

### 3.2 The network: TLS over TCP

A node MAY also serve clients on other computers. The messages, and their framing, are those of
[3.1](#31-stdinstdout): one line of UTF-8 JSON each, ending in `\n`. They travel over a TCP
connection protected by TLS 1.3. A node is started with this transport only by a person who
asks for it; **a node MUST NOT listen beyond its own computer without TLS and the checks below**.

- **Port.** A node SHOULD listen on TCP port **4026** unless told another. A client is told the
  port by the person, or by discovery ([3.3](#33-finding-a-node-on-the-local-network)).
- **TLS 1.3 only,** with a certificate on **both** sides. A node MUST require a client
  certificate. Certificates are self-signed and made on first use; there is no authority.
- **A node is recognised by its fingerprint,** the SHA-256 of its certificate's DER encoding,
  written as 32 pairs of uppercase hexadecimal digits separated by colons
  (`AB:CD:…`, 95 characters). No name, chain or date in a certificate is checked, but each
  side MUST verify the handshake's signature, which proves the peer holds the certificate's
  private key. A client that has paired with a node (below) MUST refuse a connection whose
  certificate has any other fingerprint.
- **A node serves only approved clients.** It keeps a list of client fingerprints and checks the
  client's after the handshake. A person removes a client from the list to revoke it, and the
  node MUST apply that at the client's next connection. A client that isn't on the list MAY send
  one pairing request (below) and nothing else; the node otherwise closes the connection
  **without replying**, so that a refusal tells a guesser nothing.
- **Pairing** puts a client on the list, once:
  1. The person at the node starts pairing. The node shows its fingerprint and a **one-time
     code** of eight digits. The code MUST work for one client only, for at most five minutes,
     and the window MUST close after at most three wrong codes.
  2. The client connects, not yet knowing the node's fingerprint, and reads the fingerprint
     from the certificate it is shown. **The client MUST show it to the person and wait for
     them to confirm it is the one the node displays, before it sends the code.** This
     comparison is what stops someone in the middle; the code alone does not.
  3. The client sends one line, at most 1,024 bytes, and the node answers `paired` and closes
     the connection, or closes it without a word. The client then connects again as an approved
     client:

```transport
→ {"cnp": "pair", "code": "12345678", "name": "Carlos's iMac"}
← {"cnp": "paired"}
```

  `name` is optional, a label the node keeps beside the fingerprint to tell clients apart.
  The code is typed by a person, so a node SHOULD ignore spaces and hyphens in it.
- **One client at a time.** A node MAY serve one approved client at once. When another
  approved client connects, the node sends one `Busy` error with a `null` id and closes the
  connection:

```transport
← {"jsonrpc": "2.0", "id": null, "error": {"code": -32013, "message": "this node is serving another client"}}
```

- **Disconnecting cancels.** When a client's connection ends, for any reason, the node cancels
  the client's searches and forgets its state. Nothing resumes on a later connection, and ids
  belong to one connection.
- **Deadlines.** A node SHOULD close a connection that has not finished its handshake and sent
  its first message within about ten seconds, counting the whole of it, not each read. It
  MUST NOT close a connection merely for being silent afterwards, because a client waiting
  for a long search sends nothing. It SHOULD use TCP keepalive to find a client that has
  vanished.
- **Limits.** A node on the network SHOULD advertise a `maxMessageBytes` much smaller than the
  default ([6.3](#63-limits)), and SHOULD limit how many connections it greets at once, in
  all and from one address, and ignore an address that keeps failing.

WebSocket over TLS, for programs that can't open a TCP connection, is still reserved
([13](#13-reserved-for-later-versions)).

### 3.3 Finding a node on the local network

A client MAY look for nodes with a UDP probe. This only says *where a node claims to be*:
a client never trusts it, and connects and checks the fingerprint as in
[3.2](#32-the-network-tls-over-tcp). A node answers probes only when its owner has asked it to.

- **Probe.** A datagram of exactly **320 bytes**: the JSON below, padded with spaces. The client
  sends it to UDP port **4026**, at the broadcast address of each of its network interfaces
  and at `255.255.255.255` (which on some systems reaches only one interface), and MAY send it
  again once. A node ignores a datagram of any other size or content.
- **Reply.** One JSON object, sent to the address and port the probe came from, never longer
  than the probe:

```transport
→ {"cnp": "probe", "v": 1}
← {"cnp": "here", "v": 1, "name": "the study", "protocol": "1.0", "port": 4026, "fingerprint": "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89", "pairing": false}
```

  | Field | Type | |
  | --- | --- | --- |
  | `name` | string | A name its owner chose, at most 32 bytes. Not the computer's name |
  | `protocol` | string | The CNP version it speaks, `major.minor` |
  | `port` | integer | The TCP port, on the address the reply came from |
  | `fingerprint` | string | Its certificate's fingerprint |
  | `pairing` | boolean | Whether a pairing window is open now |

  A client ignores a reply that is longer than a probe, isn't this JSON, has a `port` of zero,
  or has a fingerprint that doesn't read.
- **No amplification and no leaks.** A source address can be forged, so a reply MUST NOT be
  longer than the probe it answers. A node SHOULD answer at most a few probes a second to one
  address (cb2026: 4) and a few dozen in all (40), and bound the memory that counts them. A
  reply MUST NOT contain anything private: no hostname, user name, path or hardware.
- **Opt-in.** A node MUST NOT answer probes unless it also requires TLS and approved clients.
- **Limits of broadcast.** It reaches the local network segment only. It does not cross a
  router or VLAN, many Wi-Fi access points keep clients from seeing each other, and a firewall
  can block the port; a client MUST also let a person type an address. IPv4 only for now.

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
                ◀────────────── {protocol, node, capabilities, limits, resources?}
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
| `resources` | object | Optional. What the node was started with, for a client that chooses among nodes: `threads`, the search threads it uses, and `hashMb`, its hash memory in megabytes. Either may be missing. They are facts, not limits: a request can't change them, and none is refused for them |

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
← {"jsonrpc": "2.0", "id": 1, "result": {"protocol": "1.0", "node": {"name": "cb2026", "version": "0.9.0", "license": "MIT OR Apache-2.0", "source": "https://github.com/cjus/chessengine"}, "capabilities": {"engine": {"multipv": true, "window": true, "rootMoves": true, "strength": true, "techniques": true, "deterministic": true, "clock": true, "mate": true, "chess960": true, "progress": true}}, "limits": {"maxDepth": 128, "maxTimeMs": 3600000, "maxMultipv": 8, "maxConcurrentSearches": 4}, "resources": {"threads": 8, "hashMb": 1024}}}
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
| `evaluate` | `engine/evaluate` |
| `quiet` | `quiet` on `engine/evaluate`: settling captures before evaluating ([7.6](#76-engineevaluate)) |
| `pieces` | `pieces` on `engine/evaluate`, and `searchScore` in its result: what each piece is worth to the search's evaluation ([7.6](#76-engineevaluate)) |
| `ponder` | `ponder`, and `engine/ponderHit`: searching on the opponent's time ([7.7](#77-pondering)) |
| `tree` | `engine/tree` and `engine/treeNode`: a search's tree, kept for a while ([7.8](#78-the-search-tree)) |
| `root` | `rootMs` on `engine/search`, and `engine/root`: which root move a search is on, and where its effort goes ([7.9](#79-engineroot)) |

### 6.3 Limits

`limits` says how far a node will go. A request beyond a limit MUST be refused with
`LimitExceeded`, never trimmed silently. A missing limit has the default below, or none.

| Limit | Type | Default | |
| --- | --- | --- | --- |
| `maxDepth` | integer | none | The deepest search, in plies |
| `maxNodes` | integer | none | The most nodes one search may visit |
| `maxTimeMs` | integer | none | The longest one search may take. A search with no limits, or an infinite one, also ends here |
| `maxMultipv` | integer | 1 | The most lines one search may return |
| `maxConcurrentSearches` | integer | 1 | How many searches may run at once, ponder searches included; one more is refused with `Busy` |
| `maxMessageBytes` | integer | 16777216 | The longest message the node reads |
| `maxTreeDepth` | integer | `maxDepth` | The deepest `engine/tree` search, in plies |
| `maxTreePlies` | integer | none | The most plies from the root an `engine/tree` records |
| `maxConcurrentTrees` | integer | 1 | How many `engine/tree` searches may run at once, besides the searches; one more is refused with `Busy` |

## 7. The engine domain

| Method | Kind | |
| --- | --- | --- |
| `engine/search` | request | Search a position and answer with the best move |
| `engine/progress` | notification, node to client | How a search is going |
| `engine/stop` | request | End a search early, keeping its result |
| `engine/techniques` | request | The search techniques that can be switched |
| `engine/levels` | request | The playing levels and their ratings |
| `engine/evaluate` | request | A position's evaluation without a search, term by term |
| `engine/ponderHit` | request | The move a ponder search expected was played: search on the clock now |
| `engine/tree` | request | Search a position and keep its tree for a while |
| `engine/treeNode` | request | One node of a kept tree, and a page of its children |
| `engine/root` | notification, node to client | Which root move a search is on, and where its effort has gone |

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
| `rootMs` | integer | `root` | Send `engine/root` at most this often ([7.9](#79-engineroot)). Missing: none |
| `game` | string | | A hint that searches with the same `game` belong to one game, so the node may keep its hash table between them. A node MAY ignore it |
| `ponder` | boolean | `ponder` | Search on the opponent's time, until `engine/ponderHit` ([7.7](#77-pondering)) |

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
| `ponder` | move | The reply the engine expects, if it has one: the move to ponder on ([7.7](#77-pondering)) |
| `score` | score | The best line's score. Missing when there is no legal move, or when the search was stopped before it scored a line |
| `pv` | array of moves | The best line, starting with the best move |
| `depth` | integer | The depth completed |
| `selDepth` | integer | The deepest any line went, extensions and quiescence included |
| `nodes` | integer | Nodes searched, every thread together |
| `timeMs` | integer | Time spent |
| `ponderMs` | integer | For a ponder search, how much of `timeMs` came before `engine/ponderHit`: all of it if none came |
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

### 7.6 `engine/evaluate`

Needs `evaluate`. The engine's evaluation of a position as it stands, without a search, and
what it is made of, so that a client can say why the engine likes a position. Params:

| Field | Type | Needs | |
| --- | --- | --- | --- |
| `position` | object | | The position ([2](#2-conventions)) |
| `quiet` | boolean | `quiet` | Settle captures first, as the engine's search does at its leaves, and evaluate the position where they stop (below). Missing: `false` |
| `pieces` | boolean | `pieces` | Also say what each piece is worth to the search's evaluation (`pieces` in the result, below). Missing: `false` |

A position whose FEN isn't valid, or with a move that isn't legal, is answered with
`InvalidPosition`.

Result:

| Field | Type | |
| --- | --- | --- |
| `score` | score | The evaluation, in centipawns from the side to move's point of view, always exact. With `quiet`, `{"mate": 0}` when the side to move is checkmated where the captures stop |
| `terms` | array | What the score is made of, each with `id` (the engine's own, like `mobility`), `name` (for people), optionally `description` (a sentence), and `white` and `black`: what the term is worth to each side in this position, in centipawns, each from its own side's point of view. The side to move's terms less the other side's are the score, give or take rounding |
| `quiet` | array of moves | When `quiet` was asked: the moves that settled the position, in order, which may be none. The rest of the result is about the position they reach |
| `searchScore` | score | From a node with `pieces`, when its search evaluates this position with something other than the evaluation `terms` break down, such as a neural network: the search's evaluation, from the side to move's point of view, always exact. Missing: the search's evaluation is `score` |
| `pieces` | array | When `pieces` was asked: each piece but the kings, in no set order, with `square` and `value`: what the piece is worth to its own side in this position, by the search's evaluation, in centipawns from its side's point of view. That is the evaluation less the evaluation of the position with the piece taken off the board. Left out, with `searchScore`, when the side to move is checkmated |

With cb2026's evaluation (three of its eight terms shown):

```cnp
→ {"jsonrpc": "2.0", "id": 13, "method": "engine/evaluate", "params": {"position": {"fen": "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4"}}}
← {"jsonrpc": "2.0", "id": 13, "result": {"score": {"cp": -1}, "terms": [{"id": "material", "name": "Material", "white": 3940, "black": 3940}, {"id": "mobility", "name": "Mobility", "description": "Safe squares each piece can reach.", "white": -69, "black": -72}, {"id": "tempo", "name": "Tempo", "white": 10, "black": 0}]}}
```

Term ids are the engine's own, as technique ids are: two engines may split their evaluations
differently.

An evaluation doesn't search, so it misses a piece about to be captured and doesn't know that a
side is mated. A search's score comes from the end of its `pv`, but not always from the
position the `pv` reaches: a search goes on past its line's last move, trying captures until
the position is quiet (a quiescence search), so a `pv` can end in the middle of an exchange,
with a piece about to be taken back. To explain a search's score, evaluate the position at the
end of its `pv` with `quiet`.

**`quiet`.** The node plays out captures (and queen promotions, and evasions when in check) as
its search does at its leaves, until the side to move does better to stop, and evaluates the
position there. `quiet` in the result lists the moves it played. A capture the side to move
needn't make is left alone when stopping is as good, as a search's leaf leaves it, so a
position with captures on offer may need none played. The moves settle what is hanging, as
the search would; they aren't a search of the position.

**`pieces`.** Some engines evaluate with a neural network, which judges a position as a whole
and has no terms to break it into; `terms` then come from another evaluation, which the search
doesn't use (`searchScore` says what the search's evaluation is). What each piece is worth to
the search's evaluation can still be measured: evaluate the position again without the piece.
The values don't add up to the score, since pieces work together: a piece that defends another
is worth what its loss would cost. A position without one of its pieces may be one that can't
arise in a game, such as a side left in check: the value is still what the evaluation thinks
of that piece there, and it is no more than that. Stockfish's `eval` shows the same measure.

With cb2026's network after 1.e4 d5 2.exd5, where Black's queen takes back on d5 (three of its
eight terms and three of its 28 pieces shown):

```cnp
→ {"jsonrpc": "2.0", "id": 14, "method": "engine/evaluate", "params": {"position": {"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "moves": ["e2e4", "d7d5", "e4d5"]}, "quiet": true, "pieces": true}}
← {"jsonrpc": "2.0", "id": 14, "result": {"score": {"cp": -17}, "terms": [{"id": "material", "name": "Material", "white": 3850, "black": 3850}, {"id": "mobility", "name": "Mobility", "description": "Safe squares each piece can reach.", "white": -89, "black": -75}, {"id": "tempo", "name": "Tempo", "white": 10, "black": 0}], "quiet": ["d8d5"], "searchScore": {"cp": 34}, "pieces": [{"square": "d1", "value": 918}, {"square": "d5", "value": 912}, {"square": "f1", "value": 338}]}}
```

The hand-crafted evaluation that `terms` break down thinks White is slightly worse (−17), and
the network the search uses that White is better (+34): a client explaining the search's
choices uses `searchScore` and `pieces`.

### 7.7 Pondering

Needs `ponder`. While the opponent thinks, an engine can search the position after the reply it
expects (its last result's `ponder`). If the opponent plays that move, the engine goes on with
the same search, on its own clock now, and keeps everything it has found: it has had the
opponent's time as well as its own. This is UCI's `go ponder` and `ponderhit`.

**A ponder search** is an `engine/search` with `"ponder": true`. Its `position` already has the
expected move played, and its `limits` are the ones the search should keep to once the move is
played, `limits.clock` included, with the engine's own time as it stands, since its clock doesn't
run on the opponent's time. Until the hit, the search's time doesn't run, it sends
`engine/progress` as usual, and it MUST NOT answer, even when it has nothing more to search. It
MAY stop searching at a depth, node or mate limit and wait for the hit, which then ends it at
once: the client can't tell, and the node saves the work. Only the node's `maxTimeMs`
([6.3](#63-limits)) still holds, from the request, and the search answers at it as an infinite
one does. A request that is wrong is answered at once, as for any search.

**A hit.** `engine/ponderHit` says the expected move was played. Params: `{"id": …}`, the ponder
search's request id. The result is `null`. From the moment the node receives it, the search keeps
to its limits: its `timeMs`, and its use of the clock, count from the hit, and a depth, node or
mate limit it has already reached ends it at once. It keeps its tree, and answers the
`engine/search` as any search does, with `ponderMs` saying how long it pondered.

```cnp
→ {"jsonrpc": "2.0", "id": 14, "method": "engine/search", "params": {"position": {"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "moves": ["e2e4", "e7e5", "g1f3", "b8c6"]}, "limits": {"clock": {"white": {"timeMs": 175000, "incrementMs": 2000}, "black": {"timeMs": 168000, "incrementMs": 2000}}}, "ponder": true}}
→ {"jsonrpc": "2.0", "id": 15, "method": "engine/ponderHit", "params": {"id": 14}}
← {"jsonrpc": "2.0", "id": 15, "result": null}
← {"jsonrpc": "2.0", "id": 14, "result": {"bestMove": "f1b5", "ponder": "a7a6", "score": {"cp": 33}, "pv": ["f1b5", "a7a6", "b5a4"], "depth": 22, "selDepth": 34, "nodes": 41200555, "timeMs": 9300, "ponderMs": 6100, "stopped": false}}
```

**A miss.** The opponent played something else. The client cancels the ponder search
([4.1](#41-cancelling-a-request)) and starts a new one on the real position. A node that has read
the `$/cancelRequest` MUST NOT refuse the next search with `Busy` because of the cancelled one,
so the new search can follow the cancel at once.

`engine/stop` ends a ponder search as it ends any other ([7.3](#73-enginestop)), before the hit or
after it. A hit for a search that has already answered, or that never existed, or that has already
had its hit, is not an error, because the messages can cross; its result is `null` and it does
nothing. A hit for a running search that isn't a ponder search is `InvalidParams`.

### 7.8 The search tree

Needs `tree`. A search's tree says which moves the search looked at and what it decided about
each: which reply refuted a move, which moves it left out without a search, which it searched
shallower, deeper or again. It answers a player's "why not that move?". A tree has hundreds or
thousands of nodes, so the node keeps it for a while and the client reads it a part at a time:
`engine/tree` searches a position and answers with a handle, the search's result and the root's
children, and `engine/treeNode` reads any node of the tree and a page of its children.

The handle is the one exception to requests standing alone ([1](#1-design)): it names a tree an
earlier request recorded. It is a cache, good only on the connection that asked for it, and it
expires. A client whose handle has expired, or that has reconnected, asks for the tree again. A
node SHOULD record a tree as a deterministic search does (one thread, a cleared hash table, the
default techniques), so that the same request gives the same tree.

**`engine/tree`.** Params:

| Field | Type | |
| --- | --- | --- |
| `position` | object | The position ([2](#2-conventions)) |
| `depth` | integer | Search this many plies deep, from 1 to `maxTreeDepth` ([6.3](#63-limits)) |
| `plies` | integer | Record the nodes up to this many plies from the root, from 1 to `maxTreePlies`. Missing: 2, the root's moves and the replies to them |
| `limit` | integer | How many of the root's children to answer with. Missing: all of them |

A tree's search is a full-strength search with the node's default techniques. A position whose
FEN isn't valid, or with a move that isn't legal, is answered with `InvalidPosition`; a `depth`
or `plies` of 0 with `InvalidParams`, and one beyond the node's limits with `LimitExceeded`.

A tree's search counts toward `maxConcurrentTrees`, not `maxConcurrentSearches`, so that a
client can ask for a tree while the engine plays. One more is refused with `Busy`, but, as for
searches ([7.7](#77-pondering)), not because of a tree whose `$/cancelRequest` the node has read.
A cancelled tree answers `Cancelled` and keeps nothing. `engine/stop` doesn't end a tree's
search, since a tree is wanted whole or not at all. A tree's search keeps to the node's
`maxTimeMs` as every search does, and one that reaches it is answered with `LimitExceeded`.

Result:

| Field | Type | |
| --- | --- | --- |
| `tree` | string | The tree's handle, for `engine/treeNode` on this connection |
| `expiresMs` | integer | The handle expires once this long passes with no request for the tree: from this answer, and again from each `engine/treeNode` for it. A node MAY drop a tree sooner, to keep newer ones |
| `depth`, `selDepth`, `nodes`, `timeMs` | integers | As in a search's result ([7.1](#71-enginesearch)) |
| `bestMove` | move or `null` | The best move. `null` when the side to move has no legal move |
| `score` | score | The best move's score. Missing when there is no legal move |
| `pv` | array of moves | The best line, starting with the best move |
| `recorded` | object | What the tree holds: `plies`; `nodes`, the root included; `decisions`, at every node together; and `truncated`, whether the node stopped recording at a cap of its own before the search ended. A truncated tree holds what the search decided until then and nothing after, the later root moves' nodes among it |
| `root` | node | The root, the position searched (below) |
| `children` | array of nodes | The root's children, the first `limit` of them, in the order the search first tried them |

A position with no legal move is answered at once, as a search is: `bestMove` is `null`, there
is no `score`, `depth` is 0, and the tree is its root alone.

**A node** of the tree is a position the search reached. The search may search a node more than
once: shallower first and then again, or with a narrow window and then a wider one. A node is
shown as its last search, the one whose score counted: its window, its score, its children and
its decisions are that search's, after the decisions that say how it came to be searched that
way.

| Field | Type | |
| --- | --- | --- |
| `move` | move | The move that reaches it from its parent. Missing at the root |
| `depth` | integer | How many plies deep it was searched: the depth still to go when it was reached |
| `alpha`, `beta` | scores | Its window: the score the side to move already had elsewhere (`alpha`), and the score the other side already had (`beta`), always exact. An edge is left out when the window had none on that side |
| `score` | score | Its score, with its bound: `lower` at or above `beta` (the side to move found enough, so the move that led here is refuted), `upper` at or below `alpha` (no better for the side to move than what it already had), and otherwise exact. Missing when the recording ended before the node returned |
| `childCount` | integer | How many of its children the tree holds |
| `subtree` | integer | How many nodes the tree holds below it, at every recorded ply |
| `decisions` | array | What the search decided about it, in the order it decided (below) |

Scores and windows are from the point of view of the side to move at the node, as everywhere
([2](#2-conventions)), so a child's window is its parent's turned round, and a mate is counted
from the node. A node's score comes from the whole search, below the recorded plies too.

**Decisions** are alpha-beta's, not any engine's, so a client can say what each means without
knowing the engine. Each has a `kind`, the fields of its kind, and optionally `technique`: the
id of the technique that decided it, as `engine/techniques` lists it ([7.4](#74-enginetechniques)),
from a node that offers `techniques`, so a client can name it and link to it. A decision's
`move` is always a move from its own node.

| Kind | Fields | What the search decided |
| --- | --- | --- |
| `cutoff` | `move`, `order` | `move` scored at or above `beta`, the `order`-th move searched (from 1), so the node's other moves weren't searched: `move` refutes the move that led here |
| `table` | | An earlier search's result for this position, kept in the engine's table, settled it without a search of its own |
| `tablebase` | | Endgame tablebases settled it |
| `nullMove` | | Even passing, as if the side to move could skip its turn, kept its score at or above `beta`, so it wasn't searched further |
| `nodePruned` | `move` (optional) | It was settled before any of its moves was searched, by an estimate that a search wouldn't change the outcome. `move`, when there is one, is the capture whose shallower search proved it. Its score's bound says on which side of the window |
| `movePruned` | `move` | `move` was left out without a search, as a move that couldn't change the outcome |
| `reduced` | `plies` | It was searched this many plies shallower than its place called for, its move judged unlikely to change the outcome |
| `extended` | `plies` | It was searched this many plies deeper than its place called for, its move judged to matter more than most, such as a forced one |
| `research` | `reason` | It was searched again: `depth`, at its full depth, after a shallower search scored above `alpha`; or `window`, with a wider window, after a narrower one couldn't give its exact score |

A client MUST ignore a decision of a kind it doesn't know, and a `research` with a `reason` it
doesn't know: later minor versions may add them.

With cb2026's tree of the position in [7.1](#71-enginesearch), three of its 27 root moves asked
for, and its line shortened:

```cnp
→ {"jsonrpc": "2.0", "id": 16, "method": "engine/tree", "params": {"position": {"fen": "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3"}, "depth": 16, "plies": 3, "limit": 3}}
← {"jsonrpc": "2.0", "id": 16, "result": {"tree": "t1", "expiresMs": 300000, "depth": 16, "selDepth": 26, "nodes": 1066155, "timeMs": 939, "bestMove": "f1b5", "score": {"cp": 27}, "pv": ["f1b5", "a7a6", "b5c6", "d7c6"], "recorded": {"plies": 3, "nodes": 559, "decisions": 956, "truncated": false}, "root": {"depth": 16, "alpha": {"cp": 4}, "beta": {"cp": 54}, "score": {"cp": 27}, "childCount": 27, "subtree": 558, "decisions": []}, "children": [{"move": "d2d4", "depth": 15, "alpha": {"cp": -54}, "beta": {"cp": -4}, "score": {"cp": -16}, "childCount": 32, "subtree": 87, "decisions": []}, {"move": "f1b5", "depth": 15, "alpha": {"cp": -54}, "beta": {"cp": -16}, "score": {"cp": -27}, "childCount": 30, "subtree": 91, "decisions": [{"kind": "research", "reason": "window", "technique": "pvs"}]}, {"move": "f1c4", "depth": 15, "alpha": {"cp": -28}, "beta": {"cp": -27}, "score": {"cp": -27, "bound": "lower"}, "childCount": 1, "subtree": 34, "decisions": [{"kind": "cutoff", "move": "g8f6", "order": 1, "technique": "alphabeta"}]}]}}
```

The search tried d4 first, which scored 16 for White (−16 for Black, who is to move after it).
Bb5, searched first with a null window just above d4's score, beat it, so it was searched again
with the root's whole window: 27, exact, the best. Bc4 was refuted by the first reply tried, Nf6: at least −27 for
Black, so no better for White than Bb5.

**`engine/treeNode`.** Params:

| Field | Type | |
| --- | --- | --- |
| `tree` | string | The tree's handle |
| `path` | array of moves | The moves from the root to the node. Missing or empty: the root |
| `offset` | integer | How many of the node's children to skip, in the order the search first tried them. Missing: 0 |
| `limit` | integer | How many children to answer with after those; 0 for the node alone. Missing: the rest |

The result is `node`, the node, whose `childCount` says how many children it has in all, and
`children`, those from `offset`, at most `limit` of them, which is empty past the last. A node
answers `engine/treeNode` promptly, even while a search or a tree's search runs.

A handle the node doesn't keep is answered with `Expired`: the tree expired, or the node dropped
it for newer ones, or never gave the handle. A `path` that leads to no node of the tree is
`InvalidParams`.

The reply that refuted Bc4, searched a ply shallower than its place called for (cb2026 does
that to a hash move that beats `beta` without being the only good move), and then two of Bb5's
replies, the second and the third:

```cnp
→ {"jsonrpc": "2.0", "id": 17, "method": "engine/treeNode", "params": {"tree": "t1", "path": ["f1c4"]}}
← {"jsonrpc": "2.0", "id": 17, "result": {"node": {"move": "f1c4", "depth": 15, "alpha": {"cp": -28}, "beta": {"cp": -27}, "score": {"cp": -27, "bound": "lower"}, "childCount": 1, "subtree": 34, "decisions": [{"kind": "cutoff", "move": "g8f6", "order": 1, "technique": "alphabeta"}]}, "children": [{"move": "g8f6", "depth": 13, "alpha": {"cp": 27}, "beta": {"cp": 28}, "score": {"cp": 27, "bound": "upper"}, "childCount": 33, "subtree": 33, "decisions": [{"kind": "reduced", "plies": 1, "technique": "negext"}]}]}}
→ {"jsonrpc": "2.0", "id": 18, "method": "engine/treeNode", "params": {"tree": "t1", "path": ["f1b5"], "offset": 1, "limit": 2}}
← {"jsonrpc": "2.0", "id": 18, "result": {"node": {"move": "f1b5", "depth": 15, "alpha": {"cp": -54}, "beta": {"cp": -16}, "score": {"cp": -27}, "childCount": 30, "subtree": 91, "decisions": [{"kind": "research", "reason": "window", "technique": "pvs"}]}, "children": [{"move": "f8e7", "depth": 14, "alpha": {"cp": 26}, "beta": {"cp": 27}, "score": {"cp": 27, "bound": "lower"}, "childCount": 1, "subtree": 1, "decisions": [{"kind": "cutoff", "move": "b1c3", "order": 1, "technique": "alphabeta"}]}, {"move": "g8f6", "depth": 14, "alpha": {"cp": 26}, "beta": {"cp": 27}, "score": {"cp": 28, "bound": "lower"}, "childCount": 4, "subtree": 4, "decisions": [{"kind": "cutoff", "move": "e1g1", "order": 4, "technique": "alphabeta"}]}]}}
```

Five minutes later, with no request for the tree in between:

```cnp
→ {"jsonrpc": "2.0", "id": 19, "method": "engine/treeNode", "params": {"tree": "t1", "path": ["f1b5"]}}
← {"jsonrpc": "2.0", "id": 19, "error": {"code": -32018, "message": "no tree t1: it has expired; ask for it again"}}
```

### 7.9 `engine/root`

Needs `root`. A notification from the node saying which root move a search is on and where its
effort has gone, sent only for a search that asked with `rootMs`, at most that often, and never
after the search's result. A node MUST send at least one before the result of a search that
searched a root move. UCI's `currmove`, `currmovenumber`, `lowerbound` and `upperbound` say the
same. A client can show what the engine is looking at while it looks, and when it may be
changing its mind. It is separate from `engine/progress` ([7.2](#72-engineprogress)) and
throttled on its own, so following the root never holds the lines back.

A node MAY follow its root on one thread only, the one that reports its lines. The nodes in
`moves` are then that thread's, and what they show is each move's share of the effort, while
`nodes` counts every thread, as in `engine/progress`. Its params:

| Field | Type | |
| --- | --- | --- |
| `id` | id | The `engine/search` request it belongs to |
| `depth` | integer | The depth being searched: one more than the last completed |
| `multipv` | integer | Which line is being searched, from 1, when searching several. Missing otherwise |
| `move` | move | The root move being searched now |
| `number` | integer | Its place in this depth's order of root moves for this line, from 1 |
| `of` | integer | How many root moves this line searches |
| `best` | move | The best root move so far for this line: this depth's, or the last completed depth's until this depth has one. Missing before there is any |
| `moves` | array | Every root move searched so far in this search, each `{"move": m, "nodes": n}`: the nodes spent below it, at every depth and for every line together, in no particular order |
| `nodes`, `timeMs` | integers | So far, as in `engine/progress` |
| `research` | object | What the search is going back over, when it is (below). Missing when not |

A `research` is sent when it happens, without waiting for `rootMs`, and the interval then counts
from it. Its `kind` says what the search is going back over, and the fields of its kind:

| Kind | Fields | When |
| --- | --- | --- |
| `better` | `move` | A root move searched after the best scored above it on its quick search, with a null window, and is being searched again in full to see whether it is the new best. The next notifications' `best` say whether it was |
| `failHigh` | `score`, with `bound` `lower` | The depth's score came out above its aspiration window, the window around the last depth's score that the depth was searched in: the best move looks better than the last depth said. The depth is searched again with a wider window |
| `failLow` | `score`, with `bound` `upper` | The depth's score came out below its aspiration window: worse than the last depth said. The depth is searched again with a wider window |

After a `failHigh` or a `failLow`, `move` and `number` are those the depth's next search starts
with. A client MUST ignore a `research` of a kind it doesn't know: later minor versions may add
them.

With cb2026, following the root of a search of the position in [7.1](#71-enginesearch), with
three of the 27 moves' counts shown in each notification and the line shortened:

```cnp
→ {"jsonrpc": "2.0", "id": 20, "method": "engine/search", "params": {"position": {"fen": "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3"}, "limits": {"depth": 18}, "rootMs": 250}}
← {"jsonrpc": "2.0", "method": "engine/root", "params": {"id": 20, "depth": 15, "move": "d2d4", "number": 1, "of": 27, "best": "d2d4", "moves": [{"move": "f1b5", "nodes": 243851}, {"move": "d2d4", "nodes": 180650}, {"move": "f1c4", "nodes": 133439}], "nodes": 651264, "timeMs": 601}}
← {"jsonrpc": "2.0", "method": "engine/root", "params": {"id": 20, "depth": 16, "move": "f1b5", "number": 2, "of": 27, "best": "d2d4", "moves": [{"move": "d2d4", "nodes": 304188}, {"move": "f1b5", "nodes": 263912}, {"move": "f1c4", "nodes": 133945}], "nodes": 816665, "timeMs": 809, "research": {"kind": "better", "move": "f1b5"}}}
← {"jsonrpc": "2.0", "method": "engine/root", "params": {"id": 20, "depth": 16, "move": "h2h3", "number": 26, "of": 27, "best": "f1b5", "moves": [{"move": "f1b5", "nodes": 447814}, {"move": "d2d4", "nodes": 304188}, {"move": "f1c4", "nodes": 143193}], "nodes": 1060864, "timeMs": 1059}}
← {"jsonrpc": "2.0", "id": 20, "result": {"bestMove": "f1b5", "ponder": "a7a6", "score": {"cp": 30}, "pv": ["f1b5", "a7a6", "b5c6", "d7c6"], "depth": 18, "selDepth": 32, "nodes": 2171171, "timeMs": 2292, "stopped": false}}
```

At depth 15 the search starts with d4, the last depth's best, though Bb5 has had more of its
effort so far. At depth 16 Bb5, searched second, scores above d4 on its quick search and is
searched again in full. A quarter of a second later the search is on its 26th move and Bb5 is
the best: the second search confirmed it. It stays the best to the end.

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
| `voice/hearing` | notification, node to client | What they have said so far, while they still speak (capability `partials`) |
| `voice/heard` | notification, node to client | What they said, once they paused |

Capabilities:

| Capability | Type | |
| --- | --- | --- |
| `speak` | boolean | `voice/speak` |
| `transcribe` | boolean | `voice/transcribe` |
| `listen` | boolean | `voice/listen`, and the notifications that go with it |
| `wordTimes` | boolean | Each word's start and end, in either direction |
| `hotwords` | boolean | Words the client expects to be said, favoured in what is heard (`hotwords`, below) |
| `partials` | boolean | `voice/hearing`: what is said so far, while it is still being said (`partialMs`, below) |
| `formats` | array of strings | The audio formats it reads and writes |

Audio formats:

- `wav`: a RIFF WAVE file, mono, with 16-bit integer or 32-bit float samples;
- `pcmF32`: raw mono samples, 32-bit float, little-endian, at a stated sample rate.

**Hotwords.** A client that knows what is likely to be said (the words of a chess move, a
product's names) MAY give them with `voice/transcribe` and `voice/listen`, if the node offers
`hotwords`. Each is a word or a phrase, and the node favours it where the sound allows it:
"knight" over "night", "pawn" over "on". How much is the node's choice. A hotword that is empty
or holds a line break is `InvalidParams`, and a node MAY refuse more hotwords than it can take
the same way. A node without `hotwords` ignores them.

The voice domain stands on its own: a voice node needs only this section, the lifecycle
([5](#5-lifecycle)) and the errors ([9](#9-errors)), and none of the engine domain.
[audio-chat-box](https://github.com/cjus/audio-chat-box), the voice node Solrac uses,
describes that subset for voice clients that aren't chess programs.

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
| `hotwords` | array of strings | Words and phrases likely to be said (needs `hotwords`). Missing: none |

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
| `leadInMs` | integer | How much of the audio from before an utterance's start it is heard with, so that a short first word said fast isn't cut off; 0 for none. Missing: the node decides |
| `wordTimes` | boolean | Return each word's start and end in `voice/heard` (needs `wordTimes`) |
| `partialMs` | integer | Send `voice/hearing` about this often while someone speaks, so the client can show their words as they are said (needs `partials`). Missing or 0: never |
| `hotwords` | array of strings | Words and phrases likely to be said, for every utterance of the listen (needs `hotwords`). Missing: none |

Wrong params are `InvalidParams`, a duration of 0 (but `leadInMs`) or one the node can't keep
to included.
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
- **`voice/hearing`**, `{"id": …, "text": …, "startMs": …}`, only if the listen asked for it
  (`partialMs`): what the node makes out of the utterance so far, again about every `partialMs`
  while it goes on. Each replaces the one before, which it may revise: a word can change as
  more is heard. It is for showing, not for acting on, and the utterance's `voice/heard`,
  which is always the node's best hearing of all of it, replaces the last. It comes after
  `voice/speechStarted` and before the utterance's `voice/heard`, and is empty if no words
  are made out yet. A node MAY skip one when it is busy, and sends none after the utterance's
  `voice/heard`.
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
→ {"jsonrpc": "2.0", "id": 30, "method": "voice/listen", "params": {"format": "pcmF32", "sampleRate": 16000, "endSilenceMs": 800, "partialMs": 500, "hotwords": ["knight", "bishop", "castle"]}}
→ {"jsonrpc": "2.0", "method": "voice/audio", "params": {"id": 30, "audio": "AAAAAAAAAAA="}}
← {"jsonrpc": "2.0", "method": "voice/speechStarted", "params": {"id": 30, "startMs": 1240}}
→ {"jsonrpc": "2.0", "method": "voice/audio", "params": {"id": 30, "audio": "AAAAAAAAAAA="}}
← {"jsonrpc": "2.0", "method": "voice/hearing", "params": {"id": 30, "text": "Knight to", "startMs": 1210}}
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
| -32013 | `Busy` | Already running `maxConcurrentSearches` searches, or `maxConcurrentTrees` trees' searches |
| -32014 | `Unsupported` | A capability the node didn't offer, or an unknown technique |
| -32015 | `Unauthorized` | On the network, without valid credentials |
| -32016 | `ShuttingDown` | A request after `shutdown` |
| -32017 | `NotReady` | What the request needs isn't ready yet, like a model still downloading |
| -32018 | `Expired` | A handle the node no longer keeps, or never gave, such as an expired tree's ([7.8](#78-the-search-tree)): ask again |

## 10. Security

A node started by a client trusts that client. A node on the network trusts no one: it uses TLS
with both sides' certificates, serves only clients a person approved, and says nothing to anyone
else ([3.2](#32-the-network-tls-over-tcp)). Both are held to these rules, which are the same on
any transport:

- **No file paths in any request.** Opening books, tablebases and models are the node's own
  configuration. A request that can name a file is how a server is turned against itself.
- **Advertised limits** for depth, nodes, time, lines, concurrent searches, trees and message
  size. Requests beyond them are refused, not trimmed. A node bounds what it keeps for a client,
  the trees of [7.8](#78-the-search-tree), and drops the oldest rather than run out of memory.
- **Credentials never travel in messages,** and are never logged. A pairing code is the one
  secret a client sends, once, in the pairing request of [3.2](#32-the-network-tls-over-tcp),
  after the person has confirmed the node's fingerprint; it is never kept after use.

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

- **WebSocket over TLS**, for programs that can't open a TCP connection ([3.2](#32-the-network-tls-over-tcp)
  specifies TLS over TCP), with credentials given when the connection opens, never in messages.
- **Distributed search:** `worker/register`, which a worker sends after connecting out to a
  coordinator, with its capabilities and hardware. Either side may then send requests.
- **Streamed speech:** `voice/speak` sending its audio piece by piece.
- **A binary encoding** (CBOR) of the same messages, offered in `initialize`, for audio and
  large trees.

## 14. Changes

- **1.0 (draft), 2026-11-08**, added to the draft: **live partials** (capability `partials`, a
  listen's `partialMs` and the notification `voice/hearing`), so a client can show a player's
  words as they are said. All optional: a node that doesn't offer it, or a listen that doesn't
  ask, is unchanged (cjus/chessgui#500).
- **1.0 (draft), 2026-11-05**, added to the draft as pondering was: **the network**
  ([3.2](#32-the-network-tls-over-tcp), [3.3](#33-finding-a-node-on-the-local-network)): TLS 1.3
  over TCP with self-signed certificates pinned by fingerprint on both sides, pairing with a
  one-time code, one client at a time, and discovery by a UDP probe; and `resources` in
  `initialize`'s result, so a client can tell a node with 64 threads from one with 1. Both are
  optional, so a node or client that doesn't know them is unaffected (cjus/chessengine#209).
- **1.0 (draft)**, 2026-10-21: the first version. Before it was final: `chess960` in a
  position, so a Chess960 game says so; and listening (`listen`, `voice/listen`,
  `voice/audio`, `voice/speechStarted`, `voice/heard`, and the purpose `listen`), so a player
  can talk to Solrac without holding a button (cjus/chessgui#134); and `engine/evaluate`
  (capability `evaluate`), so a client can say why an engine likes a position
  (cjus/chessgui#145); and hotwords (capability `hotwords`, on `voice/transcribe` and
  `voice/listen`) and a listen's `leadInMs`, so the client says what is likely to be said
  and how a short first word is kept, rather than the voice node knowing chess
  (cjus/chessgui#226); and pondering (capability `ponder`, a search's `ponder` and its result's
  `ponderMs`, and `engine/ponderHit`), so an engine can search on the opponent's time under a
  game clock (#18, cjus/chessgui#319); and an evaluation's `quiet` and `pieces`, with
  `searchScore` (capabilities `quiet` and `pieces`), so a client can explain the score of a
  search that evaluates with a neural network, from the quiet position it came from
  (cjus/chessgui#340); and the search tree (capability `tree`, `engine/tree` and
  `engine/treeNode`, the limits `maxTreeDepth`, `maxTreePlies` and `maxConcurrentTrees`, and
  the error `Expired`), reserved until then, so a client can show which moves a search looked
  at and why it set each aside (cjus/chessgui#350); and following the root (capability `root`,
  `rootMs` and `engine/root`), so a client can show which root move a search is on, where its
  effort goes, and when it may be changing its mind (cjus/chessgui#357). Its design came from
  Solrac, a chess board that teaches, and cb2026, its engine.
