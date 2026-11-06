#!/usr/bin/env python3
"""Merge per-model benchmark directories into one scoreable run directory."""

from __future__ import annotations

import argparse
import json
import shutil
from datetime import datetime, timezone
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("inputs", nargs="+")
    a = ap.parse_args()
    out = Path(a.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    manifests = []
    for raw_dir in a.inputs:
        d = Path(raw_dir).resolve()
        mpath = d / "manifest.json"
        if mpath.exists():
            manifests.append(json.loads(mpath.read_text()))
        for f in d.glob("*.jsonl"):
            shutil.copy2(f, out / f.name)
        for f in d.glob("*.server.log"):
            shutil.copy2(f, out / f.name)
    if manifests:
        base = dict(manifests[0])
        base["merged"] = [m.get("run_id") for m in manifests]
        base["models"] = [m for man in manifests for m in man.get("models", [])]
        base["merged_at"] = datetime.now(timezone.utc).isoformat()
        (out / "manifest.json").write_text(json.dumps(base, indent=2) + "\n")
    print(f"merged {len(a.inputs)} run(s) into {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
