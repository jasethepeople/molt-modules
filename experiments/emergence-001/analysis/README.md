# EMERGENCE-001 Analysis v0.1

The analysis tools are read-only and deterministic. They consume exported canonical event JSONL and memory JSONL archives; they do not submit actions to the world.

## Per-run analysis

```bash
PYTHONPATH=. python3 analyze.py \
  --config-dir .. \
  --events run/events.jsonl \
  --memory run/memory.jsonl \
  --run-id E001-baseline-001 \
  --output run/analysis.json
```

## Run manifest

Every run must also record provenance:

```bash
PYTHONPATH=. python3 create_manifest.py \
  --config-dir .. \
  --events run/events.jsonl \
  --memory run/memory.jsonl \
  --run-id E001-baseline-001 \
  --genesis-hash GENESIS_HASH \
  --final-state-hash STATE_HASH \
  --started-at 2026-09-18T00:00:00Z \
  --ended-at 2026-09-18T01:00:00Z \
  --output run/manifest.json
```

The manifest records the frozen configuration hash, kernel/world/Observatory versions, shell and capability versions, genesis hash, event root, archive hashes, final state hash, analysis version, and the explicit `observatory_feedback_to_world: false` control.

## Cross-run baseline comparison

After four independent runs with the same frozen configuration:

```bash
PYTHONPATH=. python3 compare_runs.py \
  --analysis E001-baseline-001/analysis.json \
  --analysis E001-baseline-002/analysis.json \
  --analysis E001-baseline-003/analysis.json \
  --analysis E001-baseline-004/analysis.json \
  --output baseline-comparison.json
```

The comparison is descriptive only. It preserves run-level measurements, reports minima, maxima, and means, rejects mixed configuration hashes, and never feeds analysis output back into the world. Raw observations remain separate from statistical patterns, hypotheses, and future interventions.
