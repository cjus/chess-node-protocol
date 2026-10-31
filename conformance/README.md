# Conformance sessions

Each `.jsonl` file here is a session: an exchange with a node, written as JSON lines, saying
what to send and what must come back. `cnp-conformance` plays them against any node program
over its stdin/stdout:

```sh
cargo run -p cnp-conformance -- conformance -- path/to/node --its --arguments
```

It first asks the node what it offers (`initialize`), then runs each session against a fresh
node. A session that needs a capability the node doesn't offer is skipped, not failed. A node
conforms to CNP 1.0 if it passes every session that applies to it.

| Directory | What it checks | Applies to |
| --- | --- | --- |
| `lifecycle/` | `initialize`, `ping`, `shutdown`, `exit`, cancelling, and the errors around them (spec 4, 5, 9) | Every node |
| `engine/` | Searches, stop and cancel, each engine capability, and pondering's hit and miss (spec 7) | Engine nodes |
| `voice/` | Voices, models, refusing bad requests, and audio for a listen that isn't there (spec 8) | Voice nodes |
| `preferences/` | The draft preferences extension's read (`spec/preferences.md`) | Nodes that keep preferences |
| `games/` | The draft games extension: a save, its retry, a reused version, get, list, searching, deleting, importing and a game's source (`spec/games.md`) | Nodes that keep games (searching, deleting, importing and sources: those that offer them) |
| `backups/` | The draft backups extension: the day's automatic backup once, a backup made, restored and the restore undone (`spec/backups.md`) | Nodes that back up what they keep |

The voice sessions check only what doesn't need a model to be ready. A voice node's own tests
check its speech and transcription.

## The format

The first line names the session, says what it checks, and lists what it needs:

```json
{"session": "stop", "description": "engine/stop ends an infinite search, which answers with a move and stopped true (spec 7.3)", "requires": ["engine"], "timeoutMs": 10000}
```

`requires` names a domain (`engine`), one of its capabilities (`engine.rootMoves`), or a limit
the node sets (`limits.maxDepth`). A name after `!` is one the node must not offer
(`!engine.ponder`), for a session about how a node without it refuses its requests. `timeoutMs` is how long each step may wait: 10 s if missing.

Each later line is one step, with optionally a `comment`:

| Step | What it does |
| --- | --- |
| `{"send": message}` | Writes the message to the node, each `"$ref:NAME"` in it replaced by the value bound as `NAME` (below) |
| `{"sendRaw": "text"}` | Writes the text as a line, as it is (for broken messages) |
| `{"expect": pattern}` | Waits for a message matching the pattern |
| `{"expectAll": [pattern, …]}` | Waits for messages matching all of them, in any order |
| `{"expectExit": code}` | Waits for the node to end with this exit code (`null`: any) |
| `{"close": true}` | Closes the node's input |
| `{"comment": "text"}` | Nothing: a note for people |

While a step waits, notifications that match nothing are skipped (a search's progress, a
model's state), but any other message that doesn't match fails the session.

## Patterns

A pattern is JSON that a message must match. An object matches an object that has every key
the pattern has, each matching; other keys may be there too, as unknown fields are ignored. An
array matches an array of the same length, element by element. Numbers, booleans and `null`
match the same value. A string matches the same string, unless it starts with `$`:

| Pattern | Matches |
| --- | --- |
| `"$any"` | Anything present |
| `"$absent"` | A missing field (or `null`, which means the same) |
| `"$string"`, `"$number"`, `"$integer"`, `"$boolean"`, `"$array"`, `"$object"`, `"$null"` | Any value of that type |
| `"$move"` | A move in coordinate notation, like `e2e4` or `e7e8q` |
| `"$bind:NAME"` | Anything, remembered as `NAME` for the rest of the session |
| `"$ref:NAME"` | The value remembered as `NAME` |
| `"$$text"` | The string `$text` |

## Adding a session

A session checks one rule of the spec, and its description names the section. Keep it to the
fewest steps that show the rule, and end it with `shutdown` and `exit` unless it is about how a
node ends. A session that only an engine with a particular capability can pass says so in
`requires`. `cargo test` checks that every session reads and sends valid messages, and that the
lifecycle sessions pass against `cnp-example-node`.
