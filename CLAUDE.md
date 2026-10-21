# chess-node-protocol — instructions for Claude

The Chess Node Protocol (CNP, repo `cjus/chess-node-protocol`) connects chess programs to the
nodes that serve them: engines and voice modules. It is milestone M1b of Solrac
(`cjus/chessgui`, tracked in cjus/chessgui#98; the design is `docs/protocol.md` there). Its first
client is Solrac, and its first nodes are cb2026 (`cjus/chessengine`) and `cjus/solrac-voice`.
Owner: Carlos.

## Rules

1. **The spec and the code change together.** `spec/cnp.md` is the protocol. A change to a
   message changes the spec, the `cnp` types, and the conformance sessions in the same PR.
   `crates/cnp/tests/spec_examples.rs` checks every example in the spec against the types.
2. **Versions.** A minor version only adds optional fields, methods and capabilities. Anything
   else is a new major version, and needs Carlos's say. Record each version in the spec's
   "Changes".
3. **Chess concepts, not engine internals** (spec, 1): positions as FEN and moves, scores with
   their bound, nothing of any engine's own types. Requests stand alone.
4. **Permissive only.** The code is `MIT OR Apache-2.0`, the spec and sessions CC-BY-4.0, and
   `cnp` must stay usable by closed-source programs: `deny.toml` refuses GPL with no exceptions.
   `cnp` depends on `serde` and `serde_json` only; think hard before adding to that.
5. **Every public item has rustdoc** (`missing_docs = "deny"`). Module docs say what the part
   does, why, and its pitfalls. No `unsafe` (`unsafe_code = "forbid"`). Named constants, not
   magic numbers.
6. **The toolchain is the lowest the protocol's users pin** (cb2026's, `rust-toolchain.toml`), so
   they can all build `cnp`.

## Definition of done for a change

1. `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`,
   `RUSTDOCFLAGS=-D\ warnings cargo doc --workspace --no-deps` and `cargo deny check` pass.
2. The spec, the types and the sessions agree (rule 1).
3. Work is on a `claude/<topic>` branch from the latest `main`, with a PR to `main` and green
   CI. Open PRs as drafts and mark them ready when their work is done: CI checks only PRs
   that are ready (Actions minutes are limited, cjus/chessgui#73).

## Items for Carlos go in GitHub issues

Anything Carlos needs to do, decide or answer is an issue assigned to `cjus` with the
`needs-carlos` label, in this repository or in `cjus/chessgui`, saying why it matters, the
exact steps, the expected result, and "Done when".
