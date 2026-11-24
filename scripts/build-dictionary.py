#!/usr/bin/env python3
"""Build Sam's compact UK-English dictionary data.

The generated files are committed, so a normal build does not need Python or
network access. Regenerate them only when changing the source or the format.

Source: the English Wiktextract extract (Wiktionary as structured data).

    curl -L -o /tmp/kaikki-en.jsonl.gz \
        https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl.gz
    ./scripts/build-dictionary.py --wiktextract /tmp/kaikki-en.jsonl.gz

The output is deliberately small and lazy:

    dictionary.dat   lemma<TAB>pos<TAB>definition<TAB>example<TAB>synonyms<TAB>antonyms
    sounds.dat       lemma<TAB>UK IPA<TAB>hyphenated form
    origin.dat       lemma<TAB>etymology
    *.idx            little-endian u32 byte offset + u32 byte length per line

Definitions, examples, synonyms, antonyms, pronunciations, hyphenations and
etymologies come from Wiktionary and are licensed CC BY-SA 3.0. See
``LICENSE.wiktionary`` and ``README.md`` in the output directory.

The word list is the WordNet 3.0 lemma list. WordNet is used only to choose a
bounded, familiar set of headwords; none of its definitions are shipped.
"""

from __future__ import annotations

import argparse
import gzip
import html
import json
import re
import struct
import sys
from pathlib import Path

POS = {
    "noun": "noun",
    "verb": "verb",
    "adj": "adjective",
    "adv": "adverb",
}
POS_ORDER = {"noun": 0, "verb": 1, "adjective": 2, "adverb": 3}

# Senses marked with these tags are not useful in a small everyday dictionary.
DROP_TAGS = {"obsolete", "archaic", "nonstandard", "misspelling"}

# A small, reviewed set of common American spellings and their British forms.
# The definitions occasionally use American spellings even on Wiktionary.
UK_SPELLINGS = {
    "behavior": "behaviour",
    "behaviors": "behaviours",
    "behavioral": "behavioural",
    "color": "colour",
    "colors": "colours",
    "colored": "coloured",
    "coloring": "colouring",
    "colorful": "colourful",
    "colorless": "colourless",
    "favor": "favour",
    "favors": "favours",
    "favored": "favoured",
    "favoring": "favouring",
    "favorable": "favourable",
    "favorite": "favourite",
    "favorites": "favourites",
    "favoritism": "favouritism",
    "unfavorable": "unfavourable",
    "honor": "honour",
    "honors": "honours",
    "honored": "honoured",
    "honoring": "honouring",
    "honorable": "honourable",
    "labor": "labour",
    "labors": "labours",
    "labored": "laboured",
    "laboring": "labouring",
    "laborer": "labourer",
    "laborers": "labourers",
    "neighbor": "neighbour",
    "neighbors": "neighbours",
    "neighboring": "neighbouring",
    "neighborhood": "neighbourhood",
    "neighborhoods": "neighbourhoods",
    "neighborly": "neighbourly",
    "rumor": "rumour",
    "rumors": "rumours",
    "humor": "humour",
    "humors": "humours",
    "harbor": "harbour",
    "harbors": "harbours",
    "armor": "armour",
    "armored": "armoured",
    "endeavor": "endeavour",
    "endeavors": "endeavours",
    "savor": "savour",
    "savory": "savoury",
    "vapor": "vapour",
    "vapors": "vapours",
    "vigor": "vigour",
    "valor": "valour",
    "splendor": "splendour",
    "tumor": "tumour",
    "tumors": "tumours",
    "odor": "odour",
    "odors": "odours",
    "candor": "candour",
    "clamor": "clamour",
    "glamor": "glamour",
    "parlor": "parlour",
    "parlors": "parlours",
    "savior": "saviour",
    "saviors": "saviours",
    "organize": "organise",
    "organized": "organised",
    "organizing": "organising",
    "organization": "organisation",
    "organizations": "organisations",
    "realize": "realise",
    "realized": "realised",
    "realizing": "realising",
    "realization": "realisation",
    "recognize": "recognise",
    "recognized": "recognised",
    "recognizing": "recognising",
    "apologize": "apologise",
    "apologized": "apologised",
    "apologizing": "apologising",
    "criticize": "criticise",
    "criticized": "criticised",
    "criticizing": "criticising",
    "emphasize": "emphasise",
    "emphasized": "emphasised",
    "emphasizing": "emphasising",
    "minimize": "minimise",
    "minimized": "minimised",
    "minimizing": "minimising",
    "maximize": "maximise",
    "maximized": "maximised",
    "maximizing": "maximising",
    "optimize": "optimise",
    "optimized": "optimised",
    "optimizing": "optimising",
    "characterize": "characterise",
    "characterized": "characterised",
    "characterizing": "characterising",
    "summarize": "summarise",
    "summarized": "summarised",
    "summarizing": "summarising",
    "symbolize": "symbolise",
    "symbolized": "symbolised",
    "symbolizing": "symbolising",
    "civilize": "civilise",
    "civilized": "civilised",
    "civilizing": "civilising",
    "civilization": "civilisation",
    "modernize": "modernise",
    "modernized": "modernised",
    "modernizing": "modernising",
    "modernization": "modernisation",
    "standardize": "standardise",
    "standardized": "standardised",
    "standardizing": "standardising",
    "centralize": "centralise",
    "centralized": "centralised",
    "centralizing": "centralising",
    "localize": "localise",
    "localized": "localised",
    "localizing": "localising",
    "globalize": "globalise",
    "globalized": "globalised",
    "globalizing": "globalising",
    "specialize": "specialise",
    "specialized": "specialised",
    "specializing": "specialising",
    "specialization": "specialisation",
    "generalize": "generalise",
    "generalized": "generalised",
    "generalizing": "generalising",
    "normalize": "normalise",
    "normalized": "normalised",
    "normalizing": "normalising",
    "prioritize": "prioritise",
    "prioritized": "prioritised",
    "prioritizing": "prioritising",
    "utilize": "utilise",
    "utilized": "utilised",
    "utilizing": "utilising",
    "visualize": "visualise",
    "visualized": "visualised",
    "visualizing": "visualising",
    "memorize": "memorise",
    "memorized": "memorised",
    "memorizing": "memorising",
    "categorize": "categorise",
    "categorized": "categorised",
    "categorizing": "categorising",
    "authorize": "authorise",
    "authorized": "authorised",
    "authorizing": "authorising",
    "authorization": "authorisation",
    "disorganize": "disorganise",
    "disorganized": "disorganised",
    "disorganization": "disorganisation",
    "reorganize": "reorganise",
    "reorganized": "reorganised",
    "reorganization": "reorganisation",
    "disfavor": "disfavour",
    "disfavored": "disfavoured",
    "unfavor": "unfavour",
    "unfavored": "unfavoured",
    "dishonorable": "dishonourable",
    "uncolored": "uncoloured",
    "multicolored": "multicoloured",
    "discolor": "discolour",
    "discolored": "discoloured",
    "discoloration": "discolouration",
    "misbehavior": "misbehaviour",
    "normalization": "normalisation",
    "optimization": "optimisation",
    "minimization": "minimisation",
    "maximization": "maximisation",
    "characterization": "characterisation",
    "summarization": "summarisation",
    "standardization": "standardisation",
    "centralization": "centralisation",
    "localization": "localisation",
    "globalization": "globalisation",
    "generalization": "generalisation",
    "prioritization": "prioritisation",
    "utilization": "utilisation",
    "visualization": "visualisation",
    "memorization": "memorisation",
    "categorization": "categorisation",
    "analyze": "analyse",
    "analyzed": "analysed",
    "analyzing": "analysing",
    "analyzer": "analyser",
    "paralyze": "paralyse",
    "paralyzed": "paralysed",
    "paralyzing": "paralysing",
    "catalyze": "catalyse",
    "catalyzed": "catalysed",
    "catalyzing": "catalysing",
    "center": "centre",
    "centers": "centres",
    "centered": "centred",
    "centering": "centring",
    "meter": "metre",
    "meters": "metres",
    "liter": "litre",
    "liters": "litres",
    "theater": "theatre",
    "theaters": "theatres",
    "fiber": "fibre",
    "fibers": "fibres",
    "caliber": "calibre",
    "calibers": "calibres",
    "somber": "sombre",
    "specter": "spectre",
    "specters": "spectres",
    "saber": "sabre",
    "meager": "meagre",
    "sepulcher": "sepulchre",
    "maneuver": "manoeuvre",
    "maneuvers": "manoeuvres",
    "catalog": "catalogue",
    "catalogs": "catalogues",
    "cataloged": "catalogued",
    "dialog": "dialogue",
    "dialogs": "dialogues",
    "analog": "analogue",
    "analogs": "analogues",
    "monolog": "monologue",
    "monologs": "monologues",
    "prolog": "prologue",
    "prologs": "prologues",
    "travelog": "travelogue",
    "defense": "defence",
    "defenses": "defences",
    "offense": "offence",
    "offenses": "offences",
    "pretense": "pretence",
    "pretenses": "pretences",
    "traveled": "travelled",
    "traveling": "travelling",
    "traveler": "traveller",
    "travelers": "travellers",
    "canceled": "cancelled",
    "canceling": "cancelling",
    "modeled": "modelled",
    "modeling": "modelling",
    "labeled": "labelled",
    "labeling": "labelling",
    "fueled": "fuelled",
    "fueling": "fuelling",
    "signaled": "signalled",
    "signaling": "signalling",
    "counselor": "counsellor",
    "counselors": "counsellors",
    "jewelry": "jewellery",
    "marvelous": "marvellous",
    "skillful": "skilful",
    "skillfully": "skilfully",
    "fulfill": "fulfil",
    "fulfillment": "fulfilment",
    "enrollment": "enrolment",
    "installment": "instalment",
    "gray": "grey",
    "grays": "greys",
    "skeptic": "sceptic",
    "skeptical": "sceptical",
    "skepticism": "scepticism",
    "mold": "mould",
    "molds": "moulds",
    "molded": "moulded",
    "molding": "moulding",
    "smolder": "smoulder",
    "mustache": "moustache",
    "omelet": "omelette",
    "omelets": "omelettes",
    "pajamas": "pyjamas",
    "aluminum": "aluminium",
    "airplane": "aeroplane",
    "airplanes": "aeroplanes",
    "plow": "plough",
    "plows": "ploughs",
    "plowed": "ploughed",
    "cozy": "cosy",
    "donut": "doughnut",
}

UK_RE = re.compile(
    r"\b(" + "|".join(sorted(map(re.escape, UK_SPELLINGS), key=len, reverse=True)) + r")\b",
    re.IGNORECASE,
)
UK_TO_US = {uk: us for us, uk in UK_SPELLINGS.items()}

LABELS = {
    "informal": "informal",
    "slang": "slang",
    "colloquial": "colloquial",
    "derogatory": "derogatory",
    "vulgar": "vulgar",
    "dated": "dated",
    "historical": "historical",
    "British": "British",
    "US": "US",
}


def clean(text: str) -> str:
    """Plain single-line text from a Wiktextract string."""
    text = html.unescape(text)
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"\{\{[^{}]*\}\}", "", text)
    text = re.sub(r"\[\[([^|\]]*)\|([^\]]*)\]\]", r"\2", text)
    text = re.sub(r"\[\[([^\]]*)\]\]", r"\1", text)
    text = re.sub(r"\[https?://[^\]]*\]", "", text)
    text = text.replace("ſ", "s")
    text = text.replace("\u00a0", " ")
    text = re.sub(r"\s+", " ", text)
    return text.strip(" \t\n\r;,")


def uk_spelling(text: str) -> str:
    def swap(match: re.Match[str]) -> str:
        word = match.group(0)
        replacement = UK_SPELLINGS.get(word.lower())
        if replacement is None:
            return word
        if word.isupper():
            return replacement.upper()
        if word[:1].isupper():
            return replacement[:1].upper() + replacement[1:]
        return replacement

    return UK_RE.sub(swap, text)


def clip(text: str, limit: int) -> str:
    """Trim to a sentence boundary at or before `limit` characters."""
    text = text.strip()
    if len(text) <= limit:
        return text
    window = text[: limit + 1]
    cut = max(window.rfind(". "), window.rfind("! "), window.rfind("? "))
    if cut >= limit // 2:
        return window[: cut + 1].strip()
    cut = window.rfind("; ")
    if cut >= limit // 2:
        return window[:cut].strip() + "."
    return window[:limit].rstrip(" ,;:") + "…"


def looks_like_form(definition: str) -> bool:
    if re.match(
        r"^(alternative|standard|obsolete|archaic|nonstandard|variant|misspelling)\s+"
        r"(spelling|form)\s+of\b",
        definition,
        re.IGNORECASE,
    ):
        return True
    if re.match(
        r"^(Commonwealth|British|American|US|Non-Oxford)\b.*\bspelling of\b",
        definition,
        re.IGNORECASE,
    ):
        return True
    # "Australia, UK, Canada, ... standard spelling of centre."
    if re.match(
        r"^[A-Z][A-Za-z ,-]{0,90}\bstandard spelling of\b",
        definition,
    ):
        return True
    return bool(
        re.match(
            r"^(plural|past tense|present participle|simple past|third-person singular) of\b",
            definition,
            re.IGNORECASE,
        )
    )


def form_target(sense: dict) -> str:
    """The headword an alternative-form sense points at, if any."""
    for key in ("alt_of", "form_of", "inflection_of"):
        items = sense.get(key)
        if isinstance(items, dict):
            items = [items]
        for item in items or []:
            if isinstance(item, dict):
                word = clean(str(item.get("word", ""))).lower()
                if word:
                    return word
    for gloss in sense.get("glosses") or []:
        text = clean(str(gloss))
        if not looks_like_form(text):
            continue
        match = re.search(
            r"\b(?:spelling|form) of\s+(?:the\s+)?([A-Za-z][A-Za-z'’ -]{0,40})",
            text,
            re.IGNORECASE,
        )
        if match:
            return clean(match.group(1)).strip(" .,;:)]").lower()
    return ""


def words_from(items: list[dict] | None, limit: int, skip: str) -> str:
    if not items:
        return ""
    seen: list[str] = []
    for item in items:
        raw = item if isinstance(item, str) else item.get("word", "")
        word = uk_spelling(clean(re.sub(r"\[.*$", "", str(raw))))
        if not word or word.lower() == skip or word in seen:
            continue
        seen.append(word)
        if len(seen) >= limit:
            break
    return ", ".join(seen)


PROSE_STARTS = (
    "From ",
    "A ",
    "An ",
    "The ",
    "Uncertain",
    "Possibly",
    "Perhaps",
    "Borrowed",
    "Learned",
    "Via ",
    "By ",
    "Short for",
    "Abbreviation",
    "Compound",
    "Blend",
    "Clipping",
    "Back-formation",
    "Alteration",
    "Variant",
    "Plural",
    "Eye dialect",
    "Imitative",
    "Onomatopoeic",
    "Named",
    "Coined",
    "First attested",
    "Attested",
    "Ultimately",
    "Cognate",
    "Doublet",
    "Inherited",
    "Arose",
    "Formed",
    "Contraction",
    "Reduplication",
)


def origin_from(raw: str, word: str) -> str:
    if not raw.strip():
        return ""
    lines = raw.splitlines()
    start = 0
    if lines and lines[0].strip().lower() == "etymology tree":
        # Wiktextract puts an "Etymology tree" before the prose. The tree's
        # last line is "English <word>"; cut through it. If that line is not
        # there, start at the first line that looks like prose.
        marker = f"English {word}"
        at = -1
        for i, line in enumerate(lines):
            if line.strip() == marker:
                at = i
        if at >= 0:
            start = at + 1
        else:
            start = 1
            for i, line in enumerate(lines[1:], start=1):
                if line.lstrip().startswith(PROSE_STARTS):
                    start = i
                    break
    text = clean(" ".join(lines[start:]))
    if not text:
        return ""
    # The prose often ends with a long cognate list; keep the origin itself.
    for marker in (" cognates, etc", ". Cognates", ". Germanic cognates"):
        at = text.find(marker)
        if at >= 0:
            text = text[:at].rstrip(" .;:,") + "."
    return clip(uk_spelling(text), 400)


def ipa_from(sounds: list[dict] | None) -> str:
    if not sounds:
        return ""
    fallback = ""
    for sound in sounds:
        ipa = clean(str(sound.get("ipa", "")))
        if not ipa:
            continue
        tags = [str(tag) for tag in sound.get("tags", [])]
        joined = " ".join(tags).lower()
        if "received-pronunciation" in joined or "uk" in joined or "british" in joined:
            return ipa
        if not fallback:
            fallback = ipa
    return fallback


def hyphen_from(entries: list[dict] | None) -> str:
    if not entries:
        return ""
    for entry in entries:
        parts = [clean(str(part)) for part in entry.get("parts", [])]
        if parts and all(parts):
            return "·".join(parts)
    return ""


def headwords() -> set[str]:
    try:
        from nltk.corpus import wordnet as wn
    except ImportError:
        print(
            "build-dictionary: NLTK is not installed.\n"
            "Install it with `python3 -m pip install nltk` and fetch the corpus\n"
            "with `python3 -m nltk.downloader wordnet`.",
            file=sys.stderr,
        )
        raise SystemExit(2)
    try:
        return {
            str(name).replace("_", " ").strip().lower()
            for name in wn.all_lemma_names()
            if str(name).strip()
        }
    except LookupError:
        print(
            "build-dictionary: the WordNet corpus is missing.\n"
            "Run `python3 -m nltk.downloader wordnet`.",
            file=sys.stderr,
        )
        raise SystemExit(2)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--wiktextract",
        type=Path,
        required=True,
        help="the English Wiktextract .jsonl.gz file",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "data" / "dictionary",
        help="output directory (default: data/dictionary)",
    )
    parser.add_argument(
        "--max-senses",
        type=int,
        default=6,
        help="maximum senses per part of speech (default: 6)",
    )
    args = parser.parse_args()

    wanted = headwords()
    for us, uk in UK_SPELLINGS.items():
        if us in wanted:
            wanted.add(uk)
        if uk in wanted:
            wanted.add(us)
    words: dict[str, dict[str, list[tuple[int, str, str, str, str, int]]]] = {}
    forms: dict[str, str] = {}
    sounds: dict[str, tuple[str, str]] = {}
    origins: dict[str, str] = {}
    order = 0
    seen_lines = 0

    with gzip.open(args.wiktextract, "rt", encoding="utf-8") as handle:
        try:
            for line in handle:
                seen_lines += 1
                try:
                    entry = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if entry.get("lang_code") != "en":
                    continue
                word = clean(str(entry.get("word", ""))).lower()
                if len(word) < 2 or word not in wanted:
                    continue
                pos = POS.get(str(entry.get("pos", "")))
                if pos:
                    for sense in entry.get("senses") or []:
                        target = form_target(sense)
                        if target:
                            if target != word:
                                forms.setdefault(word, target)
                            continue
                        tags = [str(tag) for tag in sense.get("tags") or []]
                        if DROP_TAGS.intersection(tags):
                            continue
                        # Wiktionary glosses are hierarchical: the last one is
                        # the specific sense, earlier ones are its parents.
                        glosses = [
                            clean(str(gloss))
                            for gloss in sense.get("glosses") or []
                        ]
                        glosses = [
                            gloss
                            for gloss in glosses
                            if gloss and not gloss.startswith("Terms relating to ")
                        ]
                        if not glosses:
                            continue
                        definition = clip(uk_spelling(glosses[-1]), 260)
                        if not definition or looks_like_form(definition):
                            continue
                        labels = [LABELS[tag] for tag in tags if tag in LABELS]
                        if labels and not definition.startswith("("):
                            definition = f"({', '.join(labels)}) {definition}"
                        examples = []
                        for item in sense.get("examples") or []:
                            text = clean(
                                str(item.get("text") or item.get("english") or "")
                            )
                            if len(text) >= 10:
                                examples.append(clip(uk_spelling(text), 220))
                        example = min(examples, key=len, default="")
                        synonyms = words_from(
                            sense.get("synonyms") or entry.get("synonyms") or [],
                            8,
                            word,
                        )
                        antonyms = words_from(
                            sense.get("antonyms") or entry.get("antonyms") or [],
                            4,
                            word,
                        )
                        bucket = words.setdefault(word, {})
                        existing = bucket.setdefault(pos, [])
                        if any(item[1] == definition for item in existing):
                            continue
                        order += 1
                        existing.append(
                            (
                                order,
                                definition,
                                example,
                                synonyms,
                                antonyms,
                                len(examples),
                            )
                        )

                if word not in sounds:
                    ipa = ipa_from(entry.get("sounds"))
                    hyphen = hyphen_from(entry.get("hyphenations"))
                    if ipa or hyphen:
                        sounds[word] = (ipa, hyphen)
                if word not in origins:
                    origin = origin_from(str(entry.get("etymology_text") or ""), word)
                    if origin:
                        origins[word] = origin
                if seen_lines % 200_000 == 0:
                    print(
                        f"  read {seen_lines:,} lines, kept {len(words):,} words",
                        file=sys.stderr,
                    )
        except EOFError:
            print(
                "warning: input ended before the gzip end marker; using what was read",
                file=sys.stderr,
            )

    # Wiktionary often has only "British spelling of color" under "colour".
    # Borrow the definitions and pronunciation from the other spelling.
    for word in sorted(wanted):
        if word in words and words[word]:
            continue
        candidates = []
        if word in forms:
            candidates.append(forms[word])
        if word in UK_TO_US:
            candidates.append(UK_TO_US[word])
        if word in UK_SPELLINGS:
            candidates.append(UK_SPELLINGS[word])
        for candidate in candidates:
            if candidate in words and words[candidate]:
                words[word] = {
                    pos: [
                        (
                            order,
                            re.sub(
                                r"^\((?:US|American|US English)(?:, [^)]*)?\)\s*",
                                "",
                                definition,
                            ),
                            example,
                            synonyms,
                            antonyms,
                            examples,
                        )
                        for (
                            order,
                            definition,
                            example,
                            synonyms,
                            antonyms,
                            examples,
                        ) in items
                        # A one-word gloss is usually an abbreviation or a
                        # regional curiosity, not a useful borrowed sense.
                        if len(definition.rstrip(".").split()) >= 2
                    ]
                    for pos, items in words[candidate].items()
                }
                if candidate in sounds:
                    current_ipa, current_hyph = sounds.get(word, ("", ""))
                    candidate_ipa, _ = sounds[candidate]
                    if not current_ipa and candidate_ipa:
                        sounds[word] = (candidate_ipa, current_hyph)
                if word not in origins and candidate in origins:
                    origins[word] = origins[candidate]
                break

    rows: list[tuple[str, str, str, str, str, str]] = []
    for word in sorted(words):
        for pos in sorted(words[word], key=lambda p: POS_ORDER[p]):
            senses = words[word][pos]
            # Wiktionary page order puts the usual sense first; keep it.
            senses.sort(key=lambda sense: sense[0])
            for _, definition, example, synonyms, antonyms, _ in senses[
                : args.max_senses
            ]:
                rows.append((word, pos, definition, example, synonyms, antonyms))

    out = args.out
    out.mkdir(parents=True, exist_ok=True)

    def write_indexed(path: Path, lines: list[str]) -> None:
        offset = 0
        with path.open("wb") as data, path.with_suffix(".idx").open("wb") as index:
            for line in lines:
                encoded = line.encode("utf-8")
                data.write(encoded)
                data.write(b"\n")
                index.write(struct.pack("<II", offset, len(encoded)))
                offset += len(encoded) + 1

    write_indexed(
        out / "dictionary.dat",
        [f"{w}\t{p}\t{d}\t{e}\t{s}\t{a}" for w, p, d, e, s, a in rows],
    )
    write_indexed(
        out / "sounds.dat",
        [f"{w}\t{ipa}\t{hyph}" for w, (ipa, hyph) in sorted(sounds.items())],
    )
    write_indexed(
        out / "origin.dat",
        [f"{w}\t{origin}" for w, origin in sorted(origins.items())],
    )

    (out / "LICENSE.wiktionary").write_text(
        """Dictionary definitions, examples, synonyms, antonyms, pronunciations,
hyphenations and etymologies in this directory are derived from English
Wiktionary (https://en.wiktionary.org) via the Wiktextract extract
(https://kaikki.org/dictionary/English/).

They are licensed under the Creative Commons Attribution-ShareAlike 3.0
Unported License (CC BY-SA 3.0):
https://creativecommons.org/licenses/by-sa/3.0/

Attribution: Wiktionary contributors. The Wiktextract extraction is by
Tatu Ylonen; see https://kaikki.org for details.
""",
        encoding="utf-8",
    )

    print(
        f"{len(rows)} senses, {len(words)} headwords\n"
        f"  {len(sounds)} pronunciations, {len(origins)} etymologies\n"
        f"  {out / 'dictionary.dat'}  {(out / 'dictionary.dat').stat().st_size / 1e6:.2f} MB\n"
        f"  {out / 'sounds.dat'}      {(out / 'sounds.dat').stat().st_size / 1e6:.2f} MB\n"
        f"  {out / 'origin.dat'}      {(out / 'origin.dat').stat().st_size / 1e6:.2f} MB"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
