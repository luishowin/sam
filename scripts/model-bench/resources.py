#!/usr/bin/env python3
"""Measure raw prompt-processing and token-generation throughput with llama-bench."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from benchlib import ROOT, load_models

DEFAULT_BENCH = ROOT / "target" / "llama.cpp" / "build" / "bin" / "llama-bench"
DEFAULT_MODELS = Path.home() / ".cache" / "sam-bench" / "models"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", default="", help="comma-separated model ids")
    ap.add_argument("--models-file", default="", help="alternate models.toml")
    ap.add_argument("--stage", default="screen", choices=["screen", "quants", "all"])
    ap.add_argument("--bench", default=str(DEFAULT_BENCH))
    ap.add_argument("--model-dir", default=str(DEFAULT_MODELS))
    ap.add_argument("--threads", default="4,6,8,12")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--out", default="target/bench/resources.json")
    a = ap.parse_args()

    bench = Path(a.bench).expanduser()
    if not bench.exists():
        print(f"missing llama-bench: {bench}", file=sys.stderr)
        return 2
    model_dir = Path(a.model_dir).expanduser()
    models = load_models(a.models_file) if a.models_file else load_models()
    if a.models:
        wanted = {x.strip() for x in a.models.split(",") if x.strip()}
        models = [m for m in models if m["id"] in wanted]
    else:
        models = [m for m in models if m.get("stage") == a.stage]

    results = []
    for model in models:
        path = model_dir / model["file"]
        if not path.exists():
            print(f"skip missing {model['id']}: {path}", file=sys.stderr)
            continue
        for threads in str(a.threads).split(","):
            cmd = [
                str(bench),
                "-m",
                str(path),
                "-p",
                "512",
                "-n",
                "128",
                "-t",
                threads.strip(),
                "-r",
                str(a.runs),
                "-o",
                "json",
            ]
            print(f"== {model['id']}: {' '.join(cmd)}", flush=True)
            proc = subprocess.run(cmd, capture_output=True, text=True)
            if proc.returncode != 0:
                print(proc.stderr[-2000:], file=sys.stderr)
                results.append({"model": model["id"], "threads": threads, "error": proc.stderr[-2000:]})
                continue
            try:
                rows = json.loads(proc.stdout)
            except json.JSONDecodeError:
                print(proc.stdout[-2000:], file=sys.stderr)
                results.append({"model": model["id"], "threads": threads, "error": "invalid llama-bench JSON"})
                continue
            for row in rows:
                row["model_id"] = model["id"]
                row["family"] = model["family"]
                row["size"] = model["size"]
                row["quant"] = model["quant"]
                row["file_bytes"] = path.stat().st_size
                row["threads_requested"] = int(threads)
            results.extend(rows)

    out = Path(a.out).expanduser()
    out = out if out.is_absolute() else ROOT / out
    out.parent.mkdir(parents=True, exist_ok=True)
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(results, fh, indent=2)
        fh.write("\n")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
