# Chess Node Protocol (CNP)

An open protocol between chess programs and the nodes that serve them: chess engines, and
voice modules that speak and listen. It is JSON-RPC 2.0 over a child process's stdin/stdout,
one message per line, and later over WebSocket with TLS.

CNP goes further than UCI. A search can be limited to some root moves or to a score window. An
engine's techniques can be switched on and off for teaching. A search can ask for progress,
and every score says whether it is exact or a bound. Every request stands alone, so the same
messages can reach one engine on the same computer or, later, a cluster of many.

```text
→ {"jsonrpc": "2.0", "id": 2, "method": "engine/search", "params": {"position": {"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "moves": ["e2e4"]}, "limits": {"depth": 18}}}
← {"jsonrpc": "2.0", "id": 2, "result": {"bestMove": "c7c5", "score": {"cp": -21}, "pv": ["c7c5", "g1f3"], "depth": 18, "nodes": 2310544, "timeMs": 1290, "stopped": false}}
```

**Status: version 1.0, draft.** Solrac, a chess board that teaches, is its first client. Its
first nodes are cb2026, Solrac's engine, and `solrac-voice`.

## What is here

| Path | What it is |
| --- | --- |
| [`spec/cnp.md`](spec/cnp.md) | The specification |
| [`crates/cnp`](crates/cnp) | The `cnp` Rust crate: the protocol's types and its stdin/stdout connection, for clients and nodes. It depends only on `serde` and `serde_json` |
| [`conformance/`](conformance) | Conformance sessions: exchanges written as JSON lines that a node must pass |
| [`crates/cnp-conformance`](crates/cnp-conformance) | `cnp-conformance`, which plays the sessions against a node program, and `cnp-example-node`, the smallest correct node |

## Proposed extensions

[Draft-1 preferences](spec/preferences.md) and `cnp::preferences` add typed reads,
revision-checked patches and durable retry receipts for review with chessdb.
[Draft-1 games](spec/games.md) and `cnp::games` add a library of games a node keeps: saves
that are safe to repeat, a list newest first that can be searched by the players' names and
the PGN's tags, a game read back whole, a game deleted, and games imported in batches. These are
proposals, not released CNP domains.

## Using it

From Rust, depend on the crate by git:

```toml
[dependencies]
cnp = { git = "https://github.com/cjus/chess-node-protocol", rev = "<commit>" }
```

To check a node, from this repository:

```sh
cargo run -p cnp-conformance -- conformance -- path/to/node --its --arguments
```

Any language can implement CNP without this crate: it is JSON, one message per line.

## Licence

The code is licensed under either of the Apache License, Version 2.0
([`LICENSE-APACHE`](LICENSE-APACHE)) or the MIT license ([`LICENSE-MIT`](LICENSE-MIT)), at your
option. The specification and the conformance sessions are licensed under Creative Commons
Attribution 4.0 International ([`spec/LICENSE`](spec/LICENSE)): anyone may copy, implement and
adapt them if they credit them.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any
additional terms or conditions.

Copyright (c) 2026 Carlos Justiniano.
