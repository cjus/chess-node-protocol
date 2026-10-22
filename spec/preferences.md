# Preferences over CNP — draft 1

This is a **proposed CNP extension**, submitted for review alongside chessdb PR #1. It is not an
accepted addition to upstream CNP and current Solrac does not consume it. The chessdb implementation originally pinned `cnp` at
`aaff14eff2d8ebef1f1c905cc5c301940188eed5` for envelopes,
bounded line reading, version negotiation, lifecycle types and error codes. It does
not introduce a second transport. Upstream CNP still specifies engine and voice only.

## Lifecycle and limits

Launch `chessdb --cnp --data-dir EXISTING_DIRECTORY`. Paths are trusted launch/CLI
configuration, never protocol parameters. Stdout carries only JSON-RPC 2.0 lines;
diagnostics go to stderr. Initialize, ping, shutdown, exit, malformed messages,
unknown methods/fields and version negotiation follow CNP 1.0.

Initialize additionally returns:

```json
{
  "capabilities": {"preferences": {"read": true, "write": true}},
  "limits": {"maxMessageBytes": 65536},
  "preferencesSchemaVersion": 1,
  "preferencesContract": "draft-1"
}
```

Clients must check the contract identifier, schema and capabilities before use.
`node.license` is `LicenseRef-Unspecified`: a placeholder indicating no project
license has been selected, not a license grant. Release/distribution is pending.

There is one active storage request, with no application work queue; another gets
CNP Busy (-32013). Ping runs on the input thread while storage runs on a worker.
SQLite contention waits at most 100 ms before returning a storage error. OS I/O
can still stall: the host must have deadlines and keep I/O off its render thread.
Lines above 64 KiB are rejected and drained using upstream CNP framing. Setting
text and operation identities are at most 128 bytes. No SQL, paths, secrets,
imports, backup or restore operations are exposed through CNP in this draft.

Cancellation accepted before the commit gate rolls back the transaction and
returns Cancelled. Commit and writing the response hold the same cancellation
gate; cancellation cannot claim rollback after commit. A lost connection can
therefore leave an uncertain outcome: retry the exact operation. Shutdown cancels
unfinished work, joins the worker, then responds; exit/EOF closes the store without
restoring a historical snapshot. A stalled OS write still requires host timeout
and termination; do not assume Solrac's existing 500 ms grace is sufficient.

## Requests

`preferences/get` takes no parameters and returns:

```json
{"schemaVersion":1,"revision":0,"values":{}}
```

Values are **sparse overrides**. An absent setting means the GUI supplies its
current default. A new database contains no stored defaults. The local/device
scope is one data directory; there are no accounts or profile identifiers.

`preferences/set` accepts an atomic patch:

```json
{
  "schemaVersion": 1,
  "operationId": "unique-client-operation-1",
  "expectedRevision": 0,
  "changes": {
    "sound.volume": {"type":"number","value":0.4},
    "board.labels": {"type":"boolean","value":false},
    "camera.follow": null
  }
}
```

A nonempty patch returns the complete committed snapshot with revision incremented
once. Null removes an override. Unknown top-level fields are ignored per CNP;
unknown preference keys/types and invalid values are rejected, never discarded.
The typed Rust markers are `cnp::preferences::Get` and `cnp::preferences::Set`.
The wire map uses application-defined string keys; the Solrac schema below defines
which keys and values a node accepts. Unknown fields on request/result objects are
ignored; unknown preference keys are rejected, not silently discarded.

A success is emitted only after FULL-synchronous SQLite commit (`fullfsync` is
also enabled where supported). The transaction records the canonical request and
its original result alongside the preference changes. An exact retry with the
same operation ID returns that original receipt, even after subsequent writes;
it does not increment the revision again. Reusing an ID with different contents
is an error. Receipt history is bounded to 100,000 new operations, after which new
writes fail explicitly; existing retries still work. No unsafe expiry/pruning is
implemented. A production retention/epoch design remains needed.

| Error | Meaning | Client action |
| --- | --- | --- |
| -32602 | Invalid parameters/schema/value | Correct request |
| -32013 | Another storage request active | Retry with backoff |
| -32020 | Expected revision differs | Explicitly reconcile; use a new operation ID for a changed patch |
| -32021 | Operation ID reused with different payload | Programming error; do not overwrite |
| -32022 | SQLite or I/O failure | Keep pending; exact retry after recovery |
| -32024 | Operation history full | Keep pending; maintenance required |
| -32800 | Cancelled before commit | No new write from this attempt |

## Preference inventory

Inventory sources: remote Solrac `673a066db26ecae2383d4aaae39d9cb69f211c09`,
`crates/app/src/settings.rs`, `core/src/clock.rs`, `core/src/strength.rs`, plus
read-only inspection of the local branch at
`c896c1f9efc8e803ba77aad0d769f4cf6484f9f2` (adds `camera.follow`).
No voice preference was present in those settings schemas. Unknown future settings
abort import rather than disappear. Last-game/session state is outside this slice.

| Keys | Type / accepted values |
| --- | --- |
| window.width, window.height | integer, 1–32768 points |
| board.labels, game.takebacks, sound.pieces, sound.clock, camera.follow | boolean |
| board.clock_side | text, left/right |
| game.time_control | text: none, minutes+increment, seconds s+increment, seconds s per move |
| game.against_you, game.white_itself, game.black_itself | text: full, level unsigned-integer, rating unsigned-integer |
| game.think_seconds, light.sun_lux, light.fill | finite number, 0–1,000,000 |
| light.board_shine, light.piece_clearcoat, sound.volume | finite number, 0–1 |
| light.board_reflection | finite number, 0–2 |

These are storage validation bounds, not changed GUI slider ranges. No value is
clamped during import. Original TOML, including comments, is retained in the
migration record and the source file is untouched. An unsupported value or field
aborts the whole import. Subsequent import of identical source is a no-op;
different source or an already populated database requires reconciliation.

## Client integration still required

chessdb’s `client::PreferencesState` is a tested, transport-independent state component. UI
edits are immediate; confirmed data advances only on a matching receipt. Newer
edits stay pending after an older acknowledgement. Exact requests survive a
connection retry in memory; defaults are not sent before the initial read.
This is not yet a managed Solrac node adapter or a durable offline outbox.

The future GUI adapter must locate/spawn chessdb on a worker, bound its channels,
apply deadlines/restart backoff, show pending/failed persistence, reconcile
revision conflicts explicitly, and keep the board usable on absence/crash/hang.
It must retain settings.toml through verified migration, then stop writing it as
an authority. Do not enable a second authoritative store or automatically persist
defaults following a failed read. Process-exit recovery of unconfirmed UI edits,
shutdown policy, packaging and actual render-loop tests remain M2 work.

Rust compatibility note: `lifecycle::Capabilities` gains an optional `preferences`
field. Exhaustive Rust struct literals must add `preferences: None` (or a default
update). Existing wire messages and the base initialize result remain compatible.
