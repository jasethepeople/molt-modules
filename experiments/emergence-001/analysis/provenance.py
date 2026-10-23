import hashlib
import json
from pathlib import Path

CONFIG_FILES = ["genesis.json", "objective.json", "population.json", "capabilities.json", "resources.json", "policies.json"]

def configuration_hash(config_dir: Path) -> str:
    digest = hashlib.blake2b(digest_size=32)
    for name in CONFIG_FILES:
        data = json.loads((config_dir / name).read_text(encoding="utf-8"))
        digest.update(json.dumps(data, sort_keys=True, separators=(",", ":")).encode())
    return digest.hexdigest()

def file_hash(path: Path) -> str:
    digest = hashlib.blake2b(digest_size=32)
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def event_root(events_path: Path) -> str:
    digest = hashlib.blake2b(digest_size=32)
    with events_path.open(encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                event = json.loads(line)
                digest.update(event["event_id"].encode())
                digest.update(b"\n")
    return digest.hexdigest()
