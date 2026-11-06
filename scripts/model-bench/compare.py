#!/usr/bin/env python3
"""Combine scored run directories into one Markdown comparison."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+", help="run directories or a parent directory")
    ap.add_argument("--out", default="")
    a = ap.parse_args()

    run_dirs: list[Path] = []
    for raw in a.paths:
        p = Path(raw).resolve()
        if (p / "scores.json").exists():
            run_dirs.append(p)
        elif p.is_dir():
            run_dirs.extend(sorted(d for d in p.iterdir() if (d / "scores.json").exists()))
    if not run_dirs:
        raise SystemExit("no scores.json found")

    rows = []
    for d in run_dirs:
        scores = json.loads((d / "scores.json").read_text())
        for model, s in scores.get("models", {}).items():
            meta = s.get("meta", {})
            rows.append(
                {
                    "model": model,
                    "family": meta.get("family", ""),
                    "size": meta.get("size", ""),
                    "quant": meta.get("quant", ""),
                    "tool": s["tool"]["pass_pct"] if s["tool"]["n"] else None,
                    "answer": s["answer"]["pass_pct"] if s["answer"]["n"] else None,
                    "conf_wrong": s["tool"]["confident_wrong"] if s["tool"]["n"] else None,
                    "unsafe": s["tool"]["unsafe"] if s["tool"]["n"] else None,
                    "ttft_p50": s["latency"]["p50_ms"],
                    "ttft_p95": s["latency"]["p95_ms"],
                    "load_gib": (meta.get("load_rss_kb") or 0) / 1024 / 1024 if meta.get("load_rss_kb") else None,
                    "peak_gib": (meta.get("peak_kb") or 0) / 1024 / 1024 if meta.get("peak_kb") else None,
                    "run": d.name,
                }
            )
    rows.sort(key=lambda r: (r["family"], r["size"], r["quant"]))

    def fmt(v, digits=1, suffix=""):
        return "-" if v is None else f"{v:.{digits}f}{suffix}"

    lines = [
        "# Stage A comparison",
        "",
        "| Model | Family | Size | Quant | Tool pass | Answer pass | Conf-wrong | Unsafe | TTFT p50 ms | TTFT p95 ms | Load RSS GiB | Peak RSS GiB |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for r in rows:
        lines.append(
            "| `{model}` | {family} | {size} | {quant} | {tool} | {answer} | {cw} | {unsafe} | {p50} | {p95} | {load} | {rss} |".format(
                model=r["model"],
                family=r["family"],
                size=r["size"],
                quant=r["quant"],
                tool=fmt(r["tool"], suffix="%"),
                answer=fmt(r["answer"], suffix="%"),
                cw="-" if r["conf_wrong"] is None else r["conf_wrong"],
                unsafe="-" if r["unsafe"] is None else r["unsafe"],
                p50=fmt(r["ttft_p50"], 0),
                p95=fmt(r["ttft_p95"], 0),
                load=fmt(r["load_gib"], 2),
                rss=fmt(r["peak_gib"], 2),
            )
        )
    text = "\n".join(lines) + "\n"
    if a.out:
        out = Path(a.out).resolve()
        out.write_text(text, encoding="utf-8")
        print(f"wrote {out}")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
