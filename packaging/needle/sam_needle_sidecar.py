#!/usr/bin/env python3
"""Sam's local Needle sidecar.

One JSON object per line over a Unix socket:

    {"query": "...", "tools": [...], "system": "..."}
    {"call": {"name": "...", "arguments": {...}}, "confidence": null}

The sidecar never executes anything. It loads a pinned Cactus archive, asks
Needle for one call from the small tool subset in the request, and returns it.
Sam validates the call and policy decides.

Run it only inside the pinned environment built by the package. It has no
network code and no shell.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import json
import os
import socketserver
import sys
import threading
from collections import OrderedDict

import cactus


class AgentCache:
    """A small LRU of Cactus agents keyed by the tool subset.

    The model binds its tools at construction, so a different subset needs a
    different agent. Four warm agents keep the working set small; a miss is
    slower but still correct.
    """

    def __init__(self, weights: str, limit: int = 4) -> None:
        self.weights = weights
        self.limit = limit
        self._agents: OrderedDict[str, cactus.Cactus] = OrderedDict()
        self._lock = threading.Lock()
        self._day = _dt.date.today()

    def _key(self, tools: list[dict], system: str | None) -> str:
        raw = json.dumps(tools, sort_keys=True, separators=(",", ":"))
        return hashlib.sha256(f"{self._day}|{system or ''}|{raw}".encode()).hexdigest()

    def get(self, tools: list[dict], system: str | None) -> cactus.Cactus:
        with self._lock:
            if _dt.date.today() != self._day:
                self._agents.clear()
                self._day = _dt.date.today()
            key = self._key(tools, system)
            agent = self._agents.get(key)
            if agent is not None:
                self._agents.move_to_end(key)
                return agent
            kwargs = {
                "tools": tools,
                "weights": self.weights,
                "stateless": True,
            }
            if system:
                kwargs["system"] = system
                kwargs["auto_date"] = False
            agent = cactus.Cactus(**kwargs)
            self._agents[key] = agent
            while len(self._agents) > self.limit:
                self._agents.popitem(last=False)
            return agent


class Handler(socketserver.StreamRequestHandler):
    def handle(self) -> None:
        for line in self.rfile:
            line = line.strip()
            if not line:
                continue
            try:
                request = json.loads(line)
                query = request["query"]
                tools = request["tools"]
                system = request.get("system")
                if not isinstance(query, str) or not query.strip():
                    raise ValueError("query must be a non-empty string")
                if not isinstance(tools, list) or not (4 <= len(tools) <= 8):
                    raise ValueError("tools must contain 4 to 8 schemas")
                agent = self.server.cache.get(tools, system)
                with self.server.model_lock:
                    response = agent.complete(query)
                calls = response.get("function_calls") or []
                call = calls[0] if calls else None
                reply = {
                    "call": call,
                    "confidence": response.get("confidence"),
                    "schema": request.get("fingerprint"),
                }
            except Exception as exc:  # noqa: BLE001 - keep the sidecar alive
                reply = {"call": None, "error": str(exc)}
            self.wfile.write(json.dumps(reply, separators=(",", ":")).encode() + b"\n")
            self.wfile.flush()


class Server(socketserver.ThreadingUnixStreamServer):
    daemon_threads = True
    allow_reuse_address = True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--socket",
        default=os.path.join(
            os.environ.get("XDG_RUNTIME_DIR", "/tmp"), "sam", "needle.sock"
        ),
    )
    parser.add_argument(
        "--weights",
        default=os.path.expanduser("~/.local/share/sam/needle/sam-needle.cact"),
    )
    parser.add_argument("--warm", type=int, default=4)
    args = parser.parse_args()

    if not os.path.isfile(args.weights):
        print(f"sam-needle: weights not found: {args.weights}", file=sys.stderr)
        return 2
    os.makedirs(os.path.dirname(args.socket), mode=0o700, exist_ok=True)
    try:
        os.unlink(args.socket)
    except FileNotFoundError:
        pass

    server = Server(args.socket, Handler)
    os.chmod(args.socket, 0o600)
    server.cache = AgentCache(args.weights, args.warm)
    server.model_lock = threading.Lock()
    try:
        server.serve_forever()
    finally:
        server.server_close()
        try:
            os.unlink(args.socket)
        except FileNotFoundError:
            pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
