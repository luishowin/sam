"""Shared definitions for the Sam Tier 3 model benchmark.

The tool catalogue here mirrors the public descriptions in
`crates/sam-core/src/action.rs`. The benchmark deliberately keeps the schemas
small: the real router sees a pre-filtered subset, not the whole catalogue.
"""

from __future__ import annotations

import json
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SUITE_DIR = Path(__file__).resolve().parent / "suites"

STATES = ["on", "off", "toggle"]
PLACES = [
    "home",
    "desktop",
    "documents",
    "downloads",
    "music",
    "pictures",
    "videos",
    "templates",
    "public",
    "trash",
    "recent",
    "screenshots",
]
MEDIA_COMMANDS = ["play_pause", "play", "pause", "next", "previous", "stop"]


def _obj(properties: dict, required: list[str] | None = None) -> dict:
    return {
        "type": "object",
        "properties": properties,
        "required": required or [],
        "additionalProperties": False,
    }


def _str(description: str = "") -> dict:
    out = {"type": "string"}
    if description:
        out["description"] = description
    return out


def _enum(values: list[str], description: str = "") -> dict:
    out = {"type": "string", "enum": values}
    if description:
        out["description"] = description
    return out


def _num(minimum: float, maximum: float, description: str = "") -> dict:
    out = {"type": "number", "minimum": minimum, "maximum": maximum}
    if description:
        out["description"] = description
    return out


def tool(name: str, description: str, parameters: dict) -> dict:
    return {"name": name, "description": description, "parameters": parameters}


TOOLS: list[dict] = [
    tool("app.launch", "Open an installed application.", _obj({"app_id": _str("Desktop-entry ID, such as org.gnome.Calculator.desktop")}, ["app_id"])),
    tool("file.open", "Open a file with its default application.", _obj({"uri": _str("Absolute file, trash or recent URI")}, ["uri"])),
    tool("file.reveal", "Show a file in the Files browser.", _obj({"uri": _str("Absolute file URI")}, ["uri"])),
    tool("place.open", "Open a standard folder such as Downloads or Documents.", _obj({"place": _enum(PLACES)}, ["place"])),
    tool("url.open", "Open a web link in the default browser.", _obj({"url": _str("http or https URL")}, ["url"])),
    tool("calendar.open", "Open the GNOME Calendar application.", _obj({})),
    tool("system.lock", "Lock the screen immediately.", _obj({})),
    tool("system.suspend", "Suspend the machine to RAM.", _obj({})),
    tool("session.logout", "Log out; GNOME asks to confirm.", _obj({})),
    tool("session.restart", "Restart; GNOME asks to confirm.", _obj({})),
    tool("session.power_off", "Power off; GNOME asks to confirm.", _obj({})),
    tool("bluetooth.set", "Switch Bluetooth on, off or toggle it.", _obj({"state": _enum(STATES)}, ["state"])),
    tool("wifi.set", "Switch Wi-Fi on, off or toggle it.", _obj({"state": _enum(STATES)}, ["state"])),
    tool("audio.volume", "Turn the volume up or down one step.", _obj({"direction": _enum(["up", "down"])}, ["direction"])),
    tool("audio.volume_set", "Set the volume to a level from 0 to 100.", _obj({"level": _num(0, 100)}, ["level"])),
    tool("display.brightness", "Turn the screen brightness up or down one step.", _obj({"direction": _enum(["up", "down"])}, ["direction"])),
    tool("display.brightness_set", "Set the screen brightness to a level from 0 to 100.", _obj({"level": _num(0, 100)}, ["level"])),
    tool("audio.mute", "Mute or unmute the microphone and speaker.", _obj({"state": _enum(STATES)}, ["state"])),
    tool("media.control", "Control the currently playing media.", _obj({"command": _enum(MEDIA_COMMANDS)}, ["command"])),
    tool("appearance.dark", "Switch the dark style on, off or toggle it.", _obj({"state": _enum(STATES)}, ["state"])),
    tool("display.night_light", "Switch the night light on, off or toggle it.", _obj({"state": _enum(STATES)}, ["state"])),
    tool("display.night_light_temperature", "Set the night light colour temperature in kelvin.", _obj({"temperature": _num(1700, 6500)}, ["temperature"])),
    tool("system.airplane", "Switch airplane mode on, off or toggle it.", _obj({"state": _enum(STATES)}, ["state"])),
    tool("power.profile", "Choose the balanced, performance or power-saver profile.", _obj({"profile": _enum(["balanced", "performance", "power-saver"])}, ["profile"])),
    tool("media.seek", "Seek the current media player forward or back by seconds.", _obj({"direction": _enum(["forward", "back"]), "seconds": _num(1, 60)}, ["direction", "seconds"])),
    tool("bluetooth.connect", "Connect or disconnect a paired Bluetooth device by name.", _obj({"action": _enum(["connect", "disconnect"]), "device": _str("Device name")}, ["action", "device"])),
    tool("terminal.open", "Open the configured terminal.", _obj({})),
    tool("timer.set", "Set a timer for a duration such as 10m or 1h30m.", _obj({"duration": _str("Duration, for example 10m or 1h30m"), "label": _str("Optional label")}, ["duration"])),
    tool("timer.cancel", "Cancel a timer by its label or number.", _obj({"id": _str("Optional timer label or number")})),
    tool("timer.list", "List the running timers.", _obj({})),
    tool("alarm.set", "Set an alarm for a time such as 07:30.", _obj({"time": _str("Time, for example 07:30"), "label": _str("Optional label")}, ["time"])),
    tool("alarm.cancel", "Cancel an alarm by its label or number.", _obj({"id": _str("Optional alarm label or number")})),
    tool("alarm.list", "List the scheduled alarms.", _obj({})),
    tool("notifications.dnd", "Switch Do Not Disturb on, off or toggle it.", _obj({"state": _enum(STATES)}, ["state"])),
    tool("clipboard.copy", "Copy text to the clipboard.", _obj({"text": _str("Text to copy")}, ["text"])),
    tool("info.copy", "Copy a live system answer to the clipboard.", _obj({"title": _str("Answer title"), "text": _str("Answer text")}, ["title", "text"])),
    tool("search.files", "Search indexed files by name and content.", _obj({"query": _str("Search query")}, ["query"])),
    tool("help.show", "Show what SAM can do.", _obj({})),
    tool(
        "clarify",
        "Ask one short question for a missing argument. Do not guess.",
        _obj(
            {
                "slot": _enum(
                    [
                        "app_id",
                        "uri",
                        "place",
                        "url",
                        "state",
                        "direction",
                        "level",
                        "temperature",
                        "command",
                        "profile",
                        "seconds",
                        "device",
                        "duration",
                        "time",
                        "text",
                        "query",
                        "title",
                        "id",
                        "which",
                    ]
                )
            },
            ["slot"],
        ),
    ),
]

TOOL_BY_NAME = {t["name"]: t for t in TOOLS}

# Words that carry no retrieval signal.
STOP = {
    "the",
    "and",
    "for",
    "with",
    "that",
    "this",
    "from",
    "into",
    "your",
    "you",
    "can",
    "could",
    "would",
    "please",
    "now",
    "the",
    "then",
    "than",
}


def load_models(path: Path | str = Path(__file__).resolve().parent / "models.toml") -> list[dict]:
    with open(path, "rb") as fh:
        data = tomllib.load(fh)
    return data["model"]


def tokens(text: str) -> list[str]:
    return [w.lower() for w in re.split(r"[^A-Za-z0-9]+", text) if len(w) > 2 and w.lower() not in STOP]


def candidate_tools(
    query: str,
    expected_action: str | None = None,
    limit: int = 5,
    include_clarify: bool = False,
) -> list[dict]:
    """A deterministic, keyword-overlap candidate subset.

    The expected action is always included when one exists; that models the
    retrieval contract (the correct tool is in the shortlist) and lets the
    benchmark score selection rather than retrieval. `clarify` is only in the
    list for rows that are allowed to ask a question. Out-of-domain rows must
    choose `[]`, not use clarify as a generic fallback.
    """
    qwords = tokens(query)
    scored: list[tuple[int, str]] = []
    for t in TOOLS:
        if t["name"] == "clarify":
            continue
        haystack = (t["name"] + " " + t["description"] + " " + json.dumps(t["parameters"])).lower()
        score = sum(1 for w in qwords if w in haystack)
        if score:
            scored.append((score, t["name"]))
    scored.sort(key=lambda x: (-x[0], x[1]))

    chosen: list[str] = []
    if expected_action and expected_action in TOOL_BY_NAME and expected_action != "clarify":
        chosen.append(expected_action)
    for _, name in scored:
        if len(chosen) >= limit:
            break
        if name not in chosen:
            chosen.append(name)
    # One stable irrelevant tool keeps the list from being only confusions.
    for filler in ("help.show", "alarm.list", "calendar.open", "timer.list"):
        if len(chosen) >= limit:
            break
        if filler not in chosen:
            chosen.append(filler)
    chosen = chosen[:limit]
    out = [TOOL_BY_NAME[n] for n in chosen]
    if include_clarify and "clarify" not in chosen:
        out.append(TOOL_BY_NAME["clarify"])
    return out


def tool_messages(query: str, tools: list[dict]) -> list[dict]:
    system = (
        "You are Sam's fallback router. Select at most one tool from the list. "
        "Return exactly one JSON object with keys \"name\" and \"arguments\", or "
        "return [] when no listed tool applies. Every argument value must be "
        "grounded in the query; do not assume a default. If a required argument "
        "is missing, call clarify with that slot. Return [] for general "
        "knowledge, out-of-domain or destructive requests, requests to run a "
        "specific shell command, read a credential file, or send data to a URL. "
        "Opening a user-supplied web link, app, folder or terminal is allowed. "
        "Never invent a file, app, device, number or URL. Do not explain. Do not "
        "use markdown."
    )
    user = (
        "Tools:\n"
        + json.dumps(tools, separators=(",", ":"))
        + "\n\nQuery: "
        + query
        + "\n\nAnswer with one JSON object or []."
    )
    return [{"role": "system", "content": system}, {"role": "user", "content": user}]


def answer_messages(question: str) -> list[dict]:
    system = (
        "You are a small local answer model for Sam. Answer in at most three "
        "short sentences. If you do not know, say so. You cannot see the user's "
        "machine, browse the web, read files or remember earlier conversations. "
        "Never invent a citation, URL, current event or local system fact."
    )
    return [{"role": "system", "content": system}, {"role": "user", "content": question}]


def response_schema(tools: list[dict]) -> dict:
    """JSON Schema for one call from the candidate set, or []."""
    calls = []
    for t in tools:
        calls.append(
            {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "enum": [t["name"]]},
                    "arguments": t["parameters"],
                },
                "required": ["name", "arguments"],
                "additionalProperties": False,
            }
        )
    return {
        "oneOf": calls
        + [
            {
                "type": "array",
                "items": {"type": "string"},
                "maxItems": 0,
            }
        ]
    }


def parse_call(text: str) -> tuple[str, dict] | None:
    """Parse a model response into ('call', {name, arguments}) or ('none', {})."""
    text = text.strip()
    if not text:
        return None
    # Strip a markdown fence if a model ignores the instruction.
    if text.startswith("```"):
        text = re.sub(r"^```[a-zA-Z]*\s*", "", text)
        text = re.sub(r"\s*```$", "", text).strip()
    try:
        obj = json.loads(text)
    except json.JSONDecodeError:
        # Take the first JSON value in the text.
        m = re.search(r"(\{.*\}|\[\s*\])", text, re.S)
        if not m:
            return None
        try:
            obj = json.loads(m.group(1))
        except json.JSONDecodeError:
            return None
    if obj == []:
        return ("none", {})
    if isinstance(obj, dict) and isinstance(obj.get("name"), str):
        args = obj.get("arguments", {})
        if not isinstance(args, dict):
            return None
        return ("call", {"name": obj["name"], "arguments": args})
    return None


def norm_value(value):
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        if isinstance(value, float) and value.is_integer():
            return int(value)
        return value
    if isinstance(value, str):
        return value.strip().lower()
    return value


def norm_args(args: dict) -> dict:
    return {str(k): norm_value(v) for k, v in args.items()}
