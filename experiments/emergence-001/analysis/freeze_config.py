#!/usr/bin/env python3
import json
from pathlib import Path
from provenance import configuration_hash

root = Path(__file__).parents[1]
freeze = {
    "schema": "MOLT/EMERGENCE-001/freeze/0.1",
    "experiment": "EMERGENCE-001",
    "experiment_version": "0.1",
    "configuration_hash": configuration_hash(root),
    "immutable_inputs": ["genesis.json", "objective.json", "population.json", "capabilities.json", "resources.json", "policies.json", "analysis/analyze.py", "analysis/compare_runs.py"],
    "baseline_run_ids": ["E001-baseline-001", "E001-baseline-002", "E001-baseline-003", "E001-baseline-004"],
    "observatory_feedback_to_world": False,
    "status": "frozen"
}
(root / "freeze.json").write_text(json.dumps(freeze, indent=2, sort_keys=True) + "\n", encoding="utf-8")
