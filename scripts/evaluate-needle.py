#!/usr/bin/env python3
"""Score Needle tool calls on a frozen held-out set.

Reports tool-name accuracy, raw argument exact match, and argument exact match
after normalisation. Normalisation never touches the tool name: a call with the
wrong capability is always wrong, whatever its arguments look like.

Run with the same cactus-compute version used to build the artifact, for
example:

    /path/to/needle/.venv/bin/python /path/to/sam/scripts/evaluate-needle.py \
        --data "heldout.jsonl" --tools "tools.json" \
        --weights base --weights "sam-needle.cact"

The data files are the fine-tuning resources described in
docs/fine-tuning-runbook.md; they are not part of this repository.
"""

from __future__ import annotations

import argparse
import json
import re
import unicodedata
from collections import Counter
from pathlib import Path
from typing import Any

import cactus

CASE_INSENSITIVE = {
    "app",
    "command",
    "direction",
    "label",
    "place",
    "position",
    "query",
    "question",
    "slot",
    "state",
    "target",
}
NUMERIC = {"id", "kelvin", "level"}

_WORD_NUMBERS = {
    "zero": 0,
    "one": 1,
    "two": 2,
    "three": 3,
    "four": 4,
    "five": 5,
    "six": 6,
    "seven": 7,
    "eight": 8,
    "nine": 9,
    "ten": 10,
    "eleven": 11,
    "twelve": 12,
    "thirteen": 13,
    "fourteen": 14,
    "fifteen": 15,
    "sixteen": 16,
    "seventeen": 17,
    "eighteen": 18,
    "nineteen": 19,
    "twenty": 20,
    "thirty": 30,
    "forty": 40,
    "fifty": 50,
    "sixty": 60,
    "seventy": 70,
    "eighty": 80,
    "ninety": 90,
}

_DURATION_RE = re.compile(
    r"^(?P<n>\d+(?:\.\d+)?)\s*"
    r"(?P<unit>h|hr|hrs|hour|hours|m|min|mins|minute|minutes|s|sec|secs|second|seconds)?$"
)
_DURATION_MULTIPLIER = {
    "h": 3600,
    "hr": 3600,
    "hrs": 3600,
    "hour": 3600,
    "hours": 3600,
    "m": 60,
    "min": 60,
    "mins": 60,
    "minute": 60,
    "minutes": 60,
    "s": 1,
    "sec": 1,
    "secs": 1,
    "second": 1,
    "seconds": 1,
}
_TIME_RE = re.compile(r"^(?P<h>\d{1,2})(?::(?P<m>\d{2}))?\s*(?P<ap>am|pm)?$")


def _text(value: Any) -> str:
    return re.sub(r"\s+", " ", unicodedata.normalize("NFKC", str(value))).strip()


def _number(value: Any) -> float | None:
    raw = _text(value).lower().replace(",", "")
    try:
        return float(raw)
    except ValueError:
        pass
    words = raw.split()
    tens = {"twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"}
    if len(words) == 2 and words[0] in tens:
        tail = _WORD_NUMBERS.get(words[1])
        if tail is not None and 1 <= tail <= 9:
            return float(_WORD_NUMBERS[words[0]] + tail)
    if len(words) == 1 and words[0] in _WORD_NUMBERS:
        return float(_WORD_NUMBERS[words[0]])
    return None


def _duration_seconds(value: Any) -> float | None:
    match = _DURATION_RE.match(_text(value).lower())
    if not match:
        return None
    unit = match.group("unit") or "s"
    return float(match.group("n")) * _DURATION_MULTIPLIER[unit]


def _clock(value: Any) -> str | None:
    match = _TIME_RE.match(_text(value).lower())
    if not match:
        return None
    hour = int(match.group("h"))
    minute = int(match.group("m") or 0)
    meridiem = match.group("ap")
    if meridiem == "pm" and hour < 12:
        hour += 12
    if meridiem == "am" and hour == 12:
        hour = 0
    if hour > 23 or minute > 59:
        return None
    return f"{hour:02d}:{minute:02d}"


def _path(value: Any) -> str:
    path = _text(value).replace("\\", "/")
    path = re.sub(r"/+", "/", path)
    return path[2:] if path.startswith("./") else path


def normalise_argument(name: str, value: Any) -> Any:
    """Canonical form used only for argument comparison."""
    if isinstance(value, list):
        if name == "roots":
            return sorted(_path(item) for item in value)
        return [normalise_argument(name, item) for item in value]
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return float(value) if name in NUMERIC else value
    if not isinstance(value, str):
        return value

    if name == "duration":
        seconds = _duration_seconds(value)
        return ("duration", seconds) if seconds is not None else _text(value).lower()
    if name == "time":
        clock = _clock(value)
        return clock if clock is not None else _text(value).lower()
    if name in NUMERIC:
        number = _number(value)
        return number if number is not None else _text(value).lower()
    if name in {"path", "roots"}:
        return _path(value)
    if name == "url":
        return _text(value)
    if name in CASE_INSENSITIVE:
        return _text(value).casefold()
    return _text(value)


def arguments_match(expected: dict[str, Any], actual: dict[str, Any]) -> bool:
    if set(expected) != set(actual):
        return False
    return all(
        normalise_argument(key, expected[key]) == normalise_argument(key, actual[key])
        for key in expected
    )


def run_model(rows, weights, limit, show_wrong, system):
    selected = rows if limit <= 0 else rows[:limit]
    positives = sum(1 for row in selected if row["answers"])
    negatives = len(selected) - positives
    name_ok = args_raw = args_norm = none_ok = 0
    false_pos = false_neg = 0
    recovered: list[tuple[str, Any, Any]] = []
    wrong: list[tuple[str, Any, Any]] = []
    confusion = Counter()

    for row in selected:
        tools = row["tools"]
        row_system = row.get("system") or system
        kwargs: dict[str, Any] = {"tools": tools}
        if weights:
            kwargs["weights"] = weights
        if row_system == "auto":
            kwargs["auto_date"] = True
        else:
            kwargs["system"] = row_system
            kwargs["auto_date"] = False
        try:
            agent = cactus.Cactus(**kwargs)
            calls = agent.complete(row["query"]).get("function_calls", []) or []
        except Exception as exc:  # noqa: BLE001 - keep scoring the rest
            wrong.append((row["query"], f"ERROR {exc}", row["answers"]))
            continue

        expected = row["answers"]
        if not expected:
            if not calls:
                none_ok += 1
            else:
                false_pos += 1
                wrong.append((row["query"], calls, expected))
            continue

        name_correct = bool(calls) and calls[0].get("name") == expected[0].get("name")
        if name_correct:
            name_ok += 1
            exp_args = expected[0].get("arguments") or {}
            got_args = calls[0].get("arguments") or {}
            if exp_args == got_args:
                args_raw += 1
                args_norm += 1
            elif arguments_match(exp_args, got_args):
                args_norm += 1
                recovered.append((row["query"], calls[0], expected[0]))
            else:
                wrong.append((row["query"], calls[0], expected[0]))
        else:
            if not calls:
                false_neg += 1
            got = calls[0].get("name") if calls else "[]"
            confusion[(expected[0].get("name"), got)] += 1
            wrong.append((row["query"], calls, expected))

    label = weights or "base"
    print(f"\n{label}")
    print(
        f"  name {name_ok:2d}/{positives}   "
        f"args raw {args_raw:2d}   args normalised {args_norm:2d}   "
        f"none {none_ok:2d}/{negatives}   "
        f"false_pos {false_pos}   false_neg {false_neg}"
    )
    if recovered:
        print(f"  recovered by normalisation: {len(recovered)}")
        for query, got, exp in recovered[:show_wrong]:
            print(f"    {query!r}: {got} == {exp} (normalised)")
    if confusion:
        print("  name confusions (expected -> got):")
        for (expected_name, got_name), count in confusion.most_common(show_wrong):
            print(f"    {expected_name} -> {got_name}  x{count}")
    if wrong:
        print(f"  still wrong: {len(wrong)}")
        for query, got, exp in wrong[:show_wrong]:
            print(f"    {query!r}: got {got} expected {exp}")
    return {
        "name_ok": name_ok,
        "args_raw": args_raw,
        "args_norm": args_norm,
        "none_ok": none_ok,
        "false_pos": false_pos,
        "false_neg": false_neg,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", required=True, type=Path, help="heldout.jsonl")
    parser.add_argument("--tools", required=True, type=Path, help="tools.json (unused by the model, kept for context)")
    parser.add_argument(
        "--weights",
        action="append",
        default=[],
        help="repeat for each model; the literal 'base' means the default base weights",
    )
    parser.add_argument("--limit", type=int, default=0, help="score only the first N rows (0 = all)")
    parser.add_argument("--show-wrong", type=int, default=6, help="examples to print per section")
    parser.add_argument(
        "--system",
        default="date: 2026-11-06 Fri 09:00; locale: en-GB",
        help="system prompt facts; pass 'auto' to use the current date and time",
    )
    args = parser.parse_args()

    rows = [json.loads(line) for line in args.data.read_text().splitlines() if line.strip()]
    if not rows:
        raise SystemExit(f"no rows in {args.data}")

    # Parse the catalogue so a malformed tools.json fails early, even though the
    # per-row subsets are what the model actually sees.
    json.loads(args.tools.read_text())

    positives = sum(1 for row in rows if row["answers"])
    negatives = len(rows) - positives
    print(f"{args.data}: {len(rows)} rows ({positives} positive, {negatives} negative)")

    for spec in args.weights or ["base"]:
        weights = None if spec == "base" else spec
        run_model(rows, weights, args.limit, args.show_wrong, args.system)


if __name__ == "__main__":
    main()
