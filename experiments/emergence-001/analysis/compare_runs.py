#!/usr/bin/env python3
import argparse, json
from pathlib import Path

METRICS = [
    ("interaction", "messages_total"),
    ("interaction", "unique_contacts"),
    ("interaction", "reciprocal_contacts"),
    ("interaction", "network_density_directed"),
    ("interaction", "network_centralization"),
    ("information", "claims"),
    ("information", "evidence"),
    ("information", "challenges"),
    ("information", "capability_invocations"),
    ("organization_observations", "delegated_leases")
]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--analysis", nargs="+", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    runs = [json.loads(path.read_text(encoding="utf-8")) for path in args.analysis]
    if not runs: raise SystemExit("at least one analysis is required")
    hashes = {run["configuration_hash"] for run in runs}
    if len(hashes) != 1: raise SystemExit("baseline analyses do not share one configuration hash")
    if len({run["run_id"] for run in runs}) != len(runs): raise SystemExit("run IDs must be unique")
    metric_rows = {}
    for section, name in METRICS:
        values = [run.get(section, {}).get(name, 0) for run in runs]
        metric_rows[f"{section}.{name}"] = {
            "values_by_run": {run["run_id"]: value for run, value in zip(runs, values)},
            "minimum": min(values), "maximum": max(values),
            "mean": sum(values) / len(values)
        }
    output = {
        "schema": "MOLT/EMERGENCE-001/cross-run-analysis/0.1",
        "experiment_id": "EMERGENCE-001",
        "configuration_hash": next(iter(hashes)),
        "run_ids": [run["run_id"] for run in runs],
        "run_count": len(runs),
        "measurements": metric_rows,
        "observations": {
            "raw_observations_preserved": True,
            "descriptive_only": True,
            "world_feedback": False,
            "interpretation": "This output summarizes measurements; it does not establish intelligence, cognition, causality, or organizational success."
        }
    }
    args.output.write_text(json.dumps(output, indent=2, sort_keys=True) + "\n", encoding="utf-8")

if __name__ == "__main__": main()
