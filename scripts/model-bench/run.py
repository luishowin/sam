#!/usr/bin/env python3
"""Run the Tier 3 benchmark against one or more GGUF models.

The harness starts a local llama-server per model, streams a chat completion
for each case, and records TTFT, total latency, token count and process RSS.
It never touches the network and never calls a hosted model.
"""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import platform
import shutil
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

from benchlib import (
    ROOT,
    SUITE_DIR,
    answer_messages,
    candidate_tools,
    load_models,
    parse_call,
    response_schema,
    tool_messages,
)

DEFAULT_SERVER = ROOT / "target" / "llama.cpp" / "build" / "bin" / "llama-server"
DEFAULT_MODELS = Path(os.environ.get("SAM_BENCH_MODELS", Path.home() / ".cache" / "sam-bench" / "models"))
RESULTS = ROOT / "target" / "bench"


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def git_rev(cwd: Path) -> str | None:
    try:
        return subprocess.check_output(
            ["git", "-C", str(cwd), "rev-parse", "HEAD"],
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except Exception:
        return None


def battery_pct() -> int | None:
    """Return the first battery percentage, or None if upower is unavailable."""
    try:
        devices = subprocess.check_output(
            ["upower", "-e"], text=True, stderr=subprocess.DEVNULL
        ).split()
        bat = next((d for d in devices if "BAT" in d.upper()), None)
        if not bat:
            return None
        info = subprocess.check_output(
            ["upower", "-i", bat], text=True, stderr=subprocess.DEVNULL
        )
        for line in info.splitlines():
            line = line.strip()
            if line.startswith("percentage:"):
                return int(line.split(":", 1)[1].strip().rstrip("%"))
    except Exception:
        return None
    return None


def read_status(pid: int) -> dict:
    out = {}
    try:
        with open(f"/proc/{pid}/status", encoding="utf-8") as fh:
            for line in fh:
                if line.startswith(("VmRSS:", "VmHWM:", "VmSwap:")):
                    key, value = line.split(":", 1)
                    out[key.lower()] = int(value.split()[0])
    except OSError:
        pass
    return out


def wait_health(port: int, proc: subprocess.Popen, timeout: float = 180.0) -> None:
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"llama-server exited with {proc.returncode}")
        try:
            conn = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
            conn.request("GET", "/health")
            resp = conn.getresponse()
            resp.read()
            conn.close()
            if resp.status == 200:
                return
            last = resp.status
        except OSError as exc:
            last = exc
        time.sleep(0.5)
    raise RuntimeError(f"llama-server did not become healthy (last={last})")


def read_jsonl(path: Path) -> list[dict]:
    with open(path, encoding="utf-8") as fh:
        return [json.loads(line) for line in fh if line.strip()]


def server_version(path: Path) -> str:
    try:
        proc = subprocess.run([str(path), "--version"], capture_output=True, text=True, timeout=10)
        return (proc.stdout + proc.stderr).strip()
    except Exception:
        return ""


def chat(
    port: int,
    messages: list[dict],
    *,
    schema: dict | None,
    max_tokens: int,
    timeout: float,
    thinking: bool,
) -> dict:
    body = {
        "messages": messages,
        "temperature": 0.0,
        "top_p": 1.0,
        "seed": 0,
        "max_tokens": max_tokens,
        "stream": True,
        "cache_prompt": False,
        "chat_template_kwargs": {"enable_thinking": thinking},
    }
    if schema is not None:
        body["response_format"] = {"type": "json_object", "schema": schema}

    payload = json.dumps(body).encode()
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    started = time.perf_counter()
    conn.request(
        "POST",
        "/v1/chat/completions",
        body=payload,
        headers={"Content-Type": "application/json", "Accept": "text/event-stream"},
    )
    resp = conn.getresponse()
    if resp.status != 200:
        text = resp.read().decode("utf-8", "replace")
        conn.close()
        raise RuntimeError(f"HTTP {resp.status}: {text[:1000]}")

    first = None
    pieces: list[str] = []
    reasoning: list[str] = []
    tokens = 0
    usage = None
    while True:
        line = resp.readline()
        if not line:
            break
        if not line.startswith(b"data:"):
            continue
        data = line[5:].strip()
        if data == b"[DONE]":
            break
        try:
            chunk = json.loads(data)
        except json.JSONDecodeError:
            continue
        if chunk.get("usage"):
            usage = chunk["usage"]
        choices = chunk.get("choices") or []
        if not choices:
            continue
        delta = choices[0].get("delta") or {}
        thought = delta.get("reasoning_content")
        if thought:
            reasoning.append(thought)
        text = delta.get("content")
        if text:
            if first is None:
                first = time.perf_counter()
            pieces.append(text)
            tokens += 1
    ended = time.perf_counter()
    conn.close()
    return {
        "text": "".join(pieces),
        "reasoning": "".join(reasoning),
        "ttft_ms": (first - started) * 1000.0 if first else None,
        "total_ms": (ended - started) * 1000.0,
        "tokens": tokens,
        "usage": usage,
    }


def run_model(
    model: dict,
    *,
    server: Path,
    model_dir: Path,
    out_dir: Path,
    suite: str,
    threads: int,
    threads_batch: int,
    ctx: int,
    batch: int,
    ubatch: int,
    max_tokens: int,
    timeout: float,
    thinking: bool,
    constrain: bool,
    limit: int | None,
    min_battery: int,
    battery_every: int,
) -> dict:
    model_path = model_dir / model["file"]
    if not model_path.exists():
        raise FileNotFoundError(model_path)
    port = free_port()
    log_path = out_dir / f"{model['id']}.server.log"
    log = open(log_path, "w", encoding="utf-8")
    cmd = [
        str(server),
        "-m",
        str(model_path),
        "--host",
        "127.0.0.1",
        "--port",
        str(port),
        "-c",
        str(ctx),
        "-np",
        "1",
        "-b",
        str(batch),
        "-ub",
        str(ubatch),
        "-t",
        str(threads),
        "-tb",
        str(threads_batch),
        "-ngl",
        "0",
        "--jinja",
        "--no-webui",
        "--metrics",
    ]
    if thinking:
        cmd += ["--reasoning", "on"]
    else:
        # Gemma 4 E2B/E4B otherwise emit a thought block even when the
        # template's enable_thinking is false. A zero budget is the only
        # reliable way to get the final answer only.
        cmd += ["--reasoning", "off", "--reasoning-budget", "0"]
    started = time.perf_counter()
    proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)
    raw_path = out_dir / f"{model['id']}.{suite}.jsonl"
    rows = []
    stopped_for_battery = False
    last_battery = None
    raw_fh = open(raw_path, "w", encoding="utf-8")
    try:
        wait_health(port, proc)
        load_s = time.perf_counter() - started
        time.sleep(0.5)
        load_status = read_status(proc.pid)
        cases = []
        if suite in ("tool", "both"):
            cases += [("tool", c) for c in read_jsonl(SUITE_DIR / "tool_calls.jsonl")]
        if suite in ("answer", "both"):
            cases += [("answer", c) for c in read_jsonl(SUITE_DIR / "answers.jsonl")]
        if limit:
            cases = cases[:limit]
        for index, (kind, case) in enumerate(cases, 1):
            if min_battery and (index == 1 or index % battery_every == 0):
                last_battery = battery_pct()
                if last_battery is not None and last_battery <= min_battery:
                    print(
                        f"  battery {last_battery}% <= {min_battery}%; stopping {model['id']}",
                        flush=True,
                    )
                    stopped_for_battery = True
                    break
            if kind == "tool":
                expected_action = case["expected"].get("action") or case["expected"].get("intent")
                include_clarify = case["expected"].get("kind") == "clarify"
                tools = candidate_tools(
                    case["query"], expected_action, include_clarify=include_clarify
                )
                messages = tool_messages(case["query"], tools)
                schema = response_schema(tools) if constrain else None
                query = case["query"]
            else:
                messages = answer_messages(case["question"])
                schema = None
                query = case["question"]
            record = {
                "model": model["id"],
                "family": model["family"],
                "size": model["size"],
                "quant": model["quant"],
                "suite": kind,
                "case_id": case["id"],
                "category": case["category"],
                "query": query,
                "thinking": thinking,
                "constrained": bool(schema),
                "load_s": load_s,
                "load_rss_kb": load_status.get("vmrss"),
                "load_peak_kb": load_status.get("vmhwm"),
            }
            try:
                result = chat(
                    port,
                    messages,
                    schema=schema,
                    max_tokens=max_tokens,
                    timeout=timeout,
                    thinking=thinking,
                )
                record.update(result)
                if kind == "tool":
                    record["parsed"] = parse_call(result["text"])
            except Exception as exc:
                record["error"] = repr(exc)
            status = read_status(proc.pid)
            record["rss_kb"] = status.get("vmrss")
            record["peak_kb"] = status.get("vmhwm")
            record["swap_kb"] = status.get("vmswap")
            rows.append(record)
            raw_fh.write(json.dumps(record, ensure_ascii=False) + "\n")
            raw_fh.flush()
            if index % 10 == 0 or index == 1:
                print(
                    f"  {model['id']} {kind} {index}/{len(cases)} "
                    f"ttft={record.get('ttft_ms')} total={record.get('total_ms')} "
                    f"rss={record.get('rss_kb')}",
                    flush=True,
                )
    finally:
        status = read_status(proc.pid)
        proc.terminate()
        try:
            proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        raw_fh.close()
        log.close()
    return {
        "model": model["id"],
        "raw": str(raw_path),
        "server_log": str(log_path),
        "rows": len(rows),
        "load_s": load_s,
        "load_rss_kb": load_status.get("vmrss"),
        "peak_kb": status.get("vmhwm"),
        "swap_kb": status.get("vmswap"),
        "stopped_for_battery": stopped_for_battery,
        "battery_pct": last_battery,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", default="", help="comma-separated model ids; default: all screen models")
    ap.add_argument("--models-file", default="", help="alternate models.toml")
    ap.add_argument("--stage", default="screen", choices=["screen", "quants", "all"])
    ap.add_argument("--suite", default="both", choices=["tool", "answer", "both"])
    ap.add_argument("--server", default=str(DEFAULT_SERVER))
    ap.add_argument("--model-dir", default=str(DEFAULT_MODELS))
    ap.add_argument("--out", default="")
    ap.add_argument("--threads", type=int, default=6)
    ap.add_argument("--threads-batch", type=int, default=6)
    ap.add_argument("--ctx", type=int, default=2048)
    ap.add_argument("--batch", type=int, default=256)
    ap.add_argument("--ubatch", type=int, default=256)
    ap.add_argument("--max-tokens", type=int, default=128)
    ap.add_argument("--timeout", type=float, default=300.0)
    ap.add_argument("--thinking", action="store_true")
    ap.add_argument("--no-constrain", action="store_true")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--min-battery", type=int, default=0, help="stop before the next case at or below this battery percentage")
    ap.add_argument("--battery-every", type=int, default=5, help="check the battery every N cases")
    args = ap.parse_args()

    server = Path(args.server).expanduser()
    model_dir = Path(args.model_dir).expanduser()
    if not server.exists():
        print(f"missing llama-server: {server}", file=sys.stderr)
        return 2

    models = load_models(args.models_file) if args.models_file else load_models()
    if args.models:
        wanted = {x.strip() for x in args.models.split(",") if x.strip()}
        models = [m for m in models if m["id"] in wanted]
    else:
        models = [m for m in models if m.get("stage") == args.stage]
    if not models:
        print("no models selected", file=sys.stderr)
        return 2

    run_id = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out_dir = RESULTS / run_id
    if args.out:
        out_dir = Path(args.out).expanduser()
    out_dir = out_dir.resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    manifest = {
        "run_id": run_id,
        "started": datetime.now(timezone.utc).isoformat(),
        "host": platform.node(),
        "os": platform.platform(),
        "python": sys.version,
        "cpu": platform.processor(),
        "git": git_rev(ROOT),
        "llama_cpp": git_rev(ROOT / "target" / "llama.cpp")
        or git_rev(server.resolve().parents[2]),
        "server": str(server),
        "server_version": server_version(server),
        "threads": args.threads,
        "threads_batch": args.threads_batch,
        "ctx": args.ctx,
        "parallel": 1,
        "batch": args.batch,
        "ubatch": args.ubatch,
        "max_tokens": args.max_tokens,
        "thinking": args.thinking,
        "cache_prompt": False,
        "constrained": not args.no_constrain,
        "suite": args.suite,
        "min_battery": args.min_battery,
        "models": [],
    }
    for m in models:
        path = model_dir / m["file"]
        if path.exists():
            m = dict(m)
            m["path"] = str(path)
            m["bytes"] = path.stat().st_size
            m["sha256"] = sha256(path)
        manifest["models"].append(m)
    with open(out_dir / "manifest.json", "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, indent=2)
        fh.write("\n")

    if args.min_battery:
        pct = battery_pct()
        if pct is not None and pct <= args.min_battery:
            print(
                f"battery {pct}% <= {args.min_battery}%; refusing to start",
                file=sys.stderr,
                flush=True,
            )
            return 42

    summaries = []
    for model in models:
        print(f"\n== {model['id']} ({model['file']})", flush=True)
        try:
            summaries.append(
                run_model(
                    model,
                    server=server,
                    model_dir=model_dir,
                    out_dir=out_dir,
                    suite=args.suite,
                    threads=args.threads,
                    threads_batch=args.threads_batch,
                    ctx=args.ctx,
                    batch=args.batch,
                    ubatch=args.ubatch,
                    max_tokens=args.max_tokens,
                    timeout=args.timeout,
                    thinking=args.thinking,
                    constrain=not args.no_constrain,
                    limit=args.limit or None,
                    min_battery=args.min_battery,
                    battery_every=args.battery_every,
                )
            )
        except Exception as exc:
            print(f"  FAILED: {exc!r}", file=sys.stderr, flush=True)
            summaries.append({"model": model["id"], "error": repr(exc)})

    summary_path = out_dir / "summary.json"
    with open(summary_path, "w", encoding="utf-8") as fh:
        json.dump({"manifest": manifest, "results": summaries}, fh, indent=2)
        fh.write("\n")
    print(f"\nwrote {summary_path}")
    if any(r.get("stopped_for_battery") for r in summaries):
        return 42
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
