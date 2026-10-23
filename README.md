# MOLT Kernel

Experimental protocol substrate for persistent computational agents. The kernel defines mechanisms, not social policy: signed immutable events, deterministic replay, resource accounting, and experiment provenance (`molt-kernel`, v0.1.0-alpha.2).

## Implemented

- Signed Ed25519 events, canonical bytes, BLAKE3 content addressing, append-only in-memory and JSONL stores.
- Deterministic world replay, snapshots, resource pools, hierarchical leases, and conservation checks.
- Experiment policy metadata and replayable `created → frozen → executed → archived` lifecycle.
- **MOLT-ARTIFACT:** local content-addressed objects, signed manifests, parent/source provenance, verified reads, and event-linked creation.
- **MOLT-VALIDATOR:** deterministic claim/evidence rules and externally trust-anchored signed verdicts. Verdicts show which rules ran; they do not prove a claim true.
- **MOLT-CONTROLLER:** generic desired/observed reconciliation, stable idempotency keys, and signed receipts. The external effect and receipt are not atomic; no exactly-once guarantee is claimed.
- **MOLT-CHRONICLE / MOLT-INDEX:** deterministic event summaries and in-memory actor/type/payload filtering.
- **MOLT-LEGACY:** signed archive records for final-state hash, selected memories, contributions, lessons, and bequests. It does not yet prevent later events from the recorded agent.
- **MOLT-OBSERVATORY / MOLT-WORLD:** read-only history and lineage queries, resource views, and a local agent runtime above the frozen Alpha2 substrate.
- **EMERGENCE-001:** a 12-agent controlled experiment package with a frozen baseline.

Architectural law: no world operation may mutate state except through a valid kernel event—the event log is the source of truth and state is derived. These modules are additive; Alpha2 event, replay, resource, and snapshot-hash semantics remain unchanged.

## Build and use

Requires stable Rust.

```sh
cargo build --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo run --bin molt -- history --store events.jsonl
cargo run --bin molt -- chronicle --store events.jsonl
cargo run --bin molt -- search --type CLAIM_CREATED --contains "world" --store events.jsonl
cargo run --bin molt -- verify-history --store events.jsonl
```

The read-only CLI also includes `events`, `lineage`, `memory`, `resources`, `claims`, `evidence`, `challenges`, `replay`, and `state-hash`. Common options include `--store`, `--memory`, `--agent`, `--type`, and `--contains`.

## Project structure

- `kernel/` — kernel library (`kernel/mod.rs`)
- `protocol/schemas/` — protocol schemas
- `docs/` — invariants, Observatory, World, and [module implementation boundaries](docs/modules.md)
- `experiments/emergence-001/` — controlled experiment and analysis tooling
- `tests/` — Alpha2, invariant, Observatory, and World integration tests

## Status and boundaries

This is an active research project at v0.1.0-alpha.2 (Apache-2.0). The sibling `molt-genesis` repository supplies fail-closed Crucible sandbox execution, signed task/patch audit, offline-root peer identity provisioning, and real benchmark scoring. Peer identity provisioning is not network transport.

`molt-net`, `molt-topology`, and `molt-secrets` have no real implementation. Durable capability grants/revocation, contracts, reputation, market exchange, and governance are also not implemented. Resource pools and leases are only an economy foundation. The generic controller is not a substitute for these policy-defined materializers. See [`docs/modules.md`](docs/modules.md) for the full status map.
