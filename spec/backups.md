# Backups over CNP — draft 1

This is a **proposed CNP extension**, submitted for review alongside chessdb's implementation
and Solrac's backups (cjus/chessgui#173). It is not an accepted addition to upstream CNP. It
uses CNP 1.0's transport, lifecycle, envelopes and error codes unchanged; the typed Rust
markers and limits are `cnp::backups`. It is served by a node that keeps a program's data,
such as chessdb, beside the [preferences](preferences.md) and [games](games.md) extensions.

A backup is a snapshot of **everything the node keeps for the program**: its preferences and
its games, together, as they were at one moment. A client lists the backups, asks for one,
and restores one. Where the node keeps them is its launch configuration (chessdb's
`--backups-dir`), never a request's: a request names a backup only by the identity the node
gave it.

## Capability

```json
{"capabilities": {"backups": {"list": true, "create": true, "restore": true,
 "keepAutomatic": 7, "contract": "draft-1"}}}
```

`list` offers `backups/list`, `create` offers `backups/create`, and `restore` offers
`backups/restore`. A missing switch is false. `keepAutomatic`, if present, is how many
automatic backups the node keeps (the newest; it deletes older ones). A client sends backups
requests only to a node whose `contract` is `draft-1`. A node without the domain answers them
with Method not found (-32601).

## A backup

```json
{"id": "20261025t164012123z-manual", "kind": "manual",
 "createdAt": "2026-10-25T16:40:12.123Z", "bytes": 2457600, "games": 1203, "settings": 14}
```

| Field | What it is |
| --- | --- |
| `id` | Its identity, the node's choice: 1–64 bytes of lowercase letters, digits and `-` |
| `kind` | `automatic` (the day's), `manual` (the player asked for it) or `beforeRestore` (what the node kept just before a restore) |
| `createdAt` | When it was made: UTC, RFC 3339 with milliseconds |
| `bytes` | How large it is on disk |
| `games`, `settings` | How many games it holds, and how many preferences (those set, not the defaults) |

## Requests

### `backups/list`

No params. Answers with the backups, **newest first**, at most 100:

```json
{"backups": [{ ... a backup ... }, { ... a backup ... }]}
```

### `backups/create`

```json
{"automatic": false}
```

The params may be left out, for a backup the player asked for. Answers once the backup is
complete, checked and durably written, which a backup only then is; a node never lists a
backup it was still making:

```json
{"backup": { ... a backup ... }, "made": true}
```

With `automatic` true, the node makes the day's backup only if its newest automatic backup is
older than its own interval (chessdb's is 20 hours); otherwise it answers with that one and
`made` false. A client asks for it once as it starts, so a day the program is used has a
backup, and a day it isn't, none. After making an automatic backup a node may delete older
automatic ones beyond `keepAutomatic`. It deletes no other kind in draft 1.

### `backups/restore`

```json
{"id": "20261025t164012123z-manual"}
```

Restores everything the node keeps, its preferences and its games, to how it was in the
backup. **First it backs up what it keeps now** (`beforeRestore`), so a restore can be undone
by restoring that one. Answers after both are durable:

```json
{"restored": { ... the backup restored ... }, "before": { ... the backup of what it had ... }}
```

A backup the node doesn't have is -32025; one that doesn't check out (damaged, or not one this
node can read) is -32026. Either way nothing changes.

**Revisions only rise.** A restore is itself a change to the preferences: their revision
becomes one above the revision before the restore, with the backup's values. A client that
has seen the later revisions then takes the restored preferences as the newest, where the
backup's own revision would look stale. Operation receipts made after the backup are gone
with it, so a client holds its preference writes while it restores, reads the preferences
again after (`preferences/get`), and doesn't retry a write sent before the restore.

Games go back as the backup had them: games saved since are gone, games deleted since are
back, and each has the version it had then. A client that is still playing a game saves it
again above the restored version, as it would after any save that found an older one.

## Errors

| Error | Meaning | Client action |
| --- | --- | --- |
| -32602 | Invalid params: not a backup's identity | Correct the request |
| -32013 | Busy: another storage request is active | Retry with backoff |
| -32022 | Storage or I/O failed; a backup may or may not have been made | List the backups again |
| -32025 | No backup with that identity | List the backups again |
| -32026 | The backup doesn't check out: nothing was restored | Choose another backup |
| -32800 | Cancelled before it was done | Nothing changed |

## Not in draft 1

Deleting a backup, copying one off the computer, and restoring only the games or only the
preferences.
