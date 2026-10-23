#!/usr/bin/env python3
import argparse, json
from datetime import datetime, timezone
from pathlib import Path
from provenance import configuration_hash, event_root, file_hash

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--config-dir", type=Path, default=Path(__file__).parents[1])
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--memory", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--genesis-hash", required=True)
    parser.add_argument("--final-state-hash", required=True)
    parser.add_argument("--started-at", required=True)
    parser.add_argument("--ended-at", required=True)
    parser.add_argument("--kernel", default="molt-kernel-0.1-alpha2")
    parser.add_argument("--world", default="molt-world-0.1")
    parser.add_argument("--observatory", default="molt-observatory-0.1")
    parser.add_argument("--analysis-version", default="0.1")
    parser.add_argument("--shells", type=Path)
    parser.add_argument("--capabilities", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not args.run_id.startswith("E001-"):
        raise SystemExit("run_id must begin with E001-")
    manifest = {
        "schema": "MOLT/EMERGENCE-001/run-manifest/0.1",
        "run_id": args.run_id,
        "experiment": "EMERGENCE-001",
        "experiment_version": "0.1",
        "configuration_hash": configuration_hash(args.config_dir),
        "genesis_hash": args.genesis_hash,
        "kernel": args.kernel,
        "world": args.world,
        "observatory": args.observatory,
        "shells": json.loads(args.shells.read_text()) if args.shells else {},
        "capabilities": json.loads(args.capabilities.read_text()) if args.capabilities else {},
        "started_at": args.started_at,
        "ended_at": args.ended_at,
        "event_root": event_root(args.events),
        "event_archive_hash": file_hash(args.events),
        "memory_archive_hash": file_hash(args.memory),
        "final_state_hash": args.final_state_hash,
        "analysis_version": args.analysis_version,
        "observatory_feedback_to_world": False
    }
    args.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")

if __name__ == "__main__": main()
