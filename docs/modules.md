# MOLT module map

This map distinguishes implemented code from the architecture still to build. A kernel type or projection is not a deployed network, secret broker, or autonomous service.

## Added in this iteration

| Module | Implemented behavior | Boundary |
|---|---|---|
| `molt-artifact` | Local BLAKE3 content-addressed objects; signed manifests; parent-manifest and source-event references; optional trusted-key verification; `ARTIFACT_MANIFEST_CREATED` event append. | One local store; no replication, build runner, or external checkpointing. An object can be left orphaned if its event append fails; retry is safe. |
| `molt-validator` | Deterministic JSON claim/evidence rules; hashes evidence content; emits Ed25519-signed verdicts; verification requires a trusted public key. | Rules establish only that the named deterministic checks passed. They do not prove scientific truth or validator honesty. |
| `molt-controller` | Generic desired/observed adapter; hashes state into an idempotency key; applies a target and appends a signed success/failure/drift receipt. | No platform effector, scheduler, or exactly-once guarantee. Adapters must make retries idempotent; effects and receipt writes are not one atomic transaction. |
| `molt-experiment` | Replays `EXPERIMENT_CREATED → EXPERIMENT_FROZEN → EXPERIMENT_EXECUTED → EXPERIMENT_ARCHIVED`, pins an immutable context hash, and validates result artifact IDs. | Projection expects already accepted events; it does not verify event signatures or run experiments. |
| `molt-chronicle` | Deterministic human-readable summaries retaining each event ID and sequence; unknown event types fall back to an explicit event reference. | No inferred motivations or generative narrative. |
| `molt-index` | In-memory actor/type/payload filtering and source-freshness verification. | Rebuilt in memory; not a durable or continuously maintained index. |
| `molt-legacy` | Signed archive record for final-state hash, selected memory IDs, contribution artifact IDs, lessons, and bequests; verification takes a trusted public key. | It does not yet prevent later events from that agent, so it is an archive format—not enforcement of irreversible termination. |

The kernel's Alpha2 `WorldState`, event canonicalization, and replay semantics remain unchanged; these additions are separate protocol-facing primitives.

## Reconciliation contract

1. Derive desired state from accepted history.
2. Observe external state.
3. Materialize through a caller-provided adapter using the stable idempotency key.
4. Observe again and compare canonical hashes.
5. Append a signed receipt for an attempted change; unchanged targets emit no event.

Receipts describe what the controller observed and recorded; they do not prove external effects were atomic, safe, or durable. If materialization succeeds but receipt append fails, retries depend on an idempotent adapter. The effect and receipt are not one transaction.

## Existing foundations

| Module | Current implementation |
|---|---|
| `molt-sandbox` | The sibling `molt-genesis` repository has Crucible: generated Rust is parsed and executed only inside fail-closed bubblewrap isolation. It blocks rather than falling back when isolation is unavailable. |
| `molt-memory` | `kernel/memory.rs` provides persistent memory records; `MEMORY_RECORDED` events and the memory store are separate. Expiry, redaction, sharing authority, and secret handling are not implemented. |
| `molt-lineage` | Observatory reconstructs event ancestry and descendants from causal parents. Agent/memory/lease ancestry is not yet a dedicated authoritative projection. |
| `molt-capability` | MOLT-WORLD has explicit local allowlists and records invocations. Grants and revocations are not yet event-sourced authority, and there is no signed delegation/revocation policy. |
| `molt-economy` | Resource pools and hierarchical leases enforce quantity conservation. Bids, pricing, exchange, and lease transfer are not implemented. |
| `molt-validator` / benchmark validation | `molt-genesis` has signed benchmark suites and scores candidates through the real Crucible. This is a concrete test workflow, separate from the new general deterministic rule evaluator. |

## Not implemented yet

- **`molt-net`** — no lease-driven nftables rules or network lease expiration materializer.
- **`molt-topology`** — no veth allocation or relationship-controlled network teardown.
- **`molt-secrets`** — no secret store, encryption/key lifecycle, or grant/revoke access broker.
- **`molt-contract`** — no proposal/acceptance/obligation/completion/breach state machine.
- **`molt-reputation`** — no trust score projection; history must not be collapsed into a self-asserted score.
- **`molt-governance`** — no proposal/vote/ratification protocol.

These need explicit authority rules and real effectors before they can safely become materializers. In particular, inventing secret key custody or network-policy semantics would create security claims the current repositories cannot support. The generic controller is the seam for those adapters, not a simulation of them.

## Repository responsibilities

- `molt` is the protocol/kernel crate: event history, replay, resources, Observatory, and the additive modules above.
- `molt-genesis` is the local runtime: signed task and patch audit, Crucible isolation, offline-root peer identity provisioning, and real benchmark scoring. Peer identity provisioning is not network transport or consensus.
