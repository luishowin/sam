# Offline UK-English dictionary

**Status:** Implemented · **Scope:** Word definitions, pronunciation, synonyms, antonyms and
etymology; the automatic preview row and the detail card.

Sam bundles a compact UK-English dictionary. It is fully offline: no lookup, preview or detail
opens a socket.

## Source and licence

| Field | Source | Licence |
|---|---|---|
| Definitions, examples, synonyms, antonyms | English Wiktionary via Wiktextract | CC BY-SA 3.0 |
| UK Received Pronunciation and hyphenation | Wiktionary | CC BY-SA 3.0 |
| Etymology | Wiktionary | CC BY-SA 3.0 |
| Headword list | WordNet 3.0 lemmas | Princeton licence |

No WordNet definitions are shipped. WordNet is used only to bound the headword list to a
familiar set; Wiktionary supplies the content. The data is a separate CC BY-SA work; the Rust
code remains MIT. Both licences ship in the package.

The iOS Dictionary screenshots were used only as layout inspiration. No Apple or Oxford text
is copied.

## Shipped data

`data/dictionary/` contains three indexed TSVs:

- `dictionary.dat` — `lemma<TAB>pos<TAB>definition<TAB>example<TAB>synonyms<TAB>antonyms`
- `sounds.dat` — `lemma<TAB>UK IPA<TAB>hyphenated form`
- `origin.dat` — `lemma<TAB>etymology`

`dictionary.idx`, `sounds.idx` and `origin.idx` hold one little-endian `(u32 offset, u32
length)` record per line. Every file is memory-mapped on first use, and the mapping lives for
the process lifetime.

Current extract: **87,951 headwords**, **214,520 senses**, **44,708 UK pronunciations** and
**69,045 etymologies**.

## Generation

`scripts/build-dictionary.py` streams the English Wiktextract JSONL, keeps at most six senses
per part of speech, cleans wiki markup, prepares UK spelling, and writes the three indexed
files. The Wiktextract download is build-time only; normal builds use the committed data.

Regeneration instructions, including the 534 MB Wiktextract download, are in
[`data/dictionary/README.md`](../data/dictionary/README.md).

### Selection rules

- English entries only; inflected forms are skipped.
- Noun, verb, adjective and adverb; at most six senses per part of speech.
- Obsolete and archaic senses are dropped.
- The most specific Wiktionary gloss is used; parent category glosses are discarded.
- One short usage example is kept per sense.
- Synonyms and antonyms come from the sense, falling back to the entry.
- Alternative-form entries (`okay` → `ok`, `colour` → `color`, `centre` → `center`,
  `organise` → `organize`) borrow the real definitions and pronunciation.
- Definitions and thesaurus words pass through a curated US→UK spelling map. Quotations keep
  their original spelling.

## Panel behaviour

- A **250 ms pause** on a single ordinary word starts a lookup. Typing another word, pressing
  Escape or hiding the panel cancels it.
- The preview row shows the word and `noun · first definition`.
- If files, apps, settings or a command matched, the definition sits at the **bottom** of the
  list.
- If only the generic “Search files” row exists, the definition is shown **first**.
- Clicking or pressing Enter opens the detail card:
  - dotted headword and UK IPA,
  - sections by part of speech,
  - up to six numbered senses each,
  - examples, synonyms and antonyms,
  - an **ORIGIN** section.
- Copy copies a plain-text rendering of the whole entry.

## Lazy loading

The dictionary is never loaded at startup and is not part of ordinary search:

- the preview row reads only one line from `dictionary.dat`;
- `sounds.dat` and `origin.dat` are mapped only when the detail card opens;
- the panel keeps a small preview cache for backspacing and retyping;
- the CLI stays explicit: `sam run "define serendipity" --yes`; plain `sam query` does not touch
  the dictionary.

The first lookup maps and validates the index once; later lookups are binary searches over the
mapped index and read only the matching lemma's lines.

## Actions and policy

- `dictionary.lookup` is `Read`, `Never`, `Os`. It has a required `word` argument.
- A model may propose it like any read-only action; it changes nothing.
- The panel intercepts it to render the rich card; the executor also handles it for the CLI.

## Limits

- Headwords are the WordNet lemma set. Some inflected or very new words fall back through the
  conservative inflection rules, but not every form is an entry.
- UK IPA comes from Wiktionary and is missing for some words; the card then omits the
  pronunciation line.
- Definitions are community-written and can be more verbose than a commercial dictionary.
- UK spelling normalisation is spelling-only; lexical differences such as `apartment`/`flat`
  are not rewritten.

## Tests

`crates/sam-linux/src/dictionary.rs` checks the shipped data opens, definitions and parts of
speech, UK sounds and origin, the first-sense preview, UK variant fallback and inflection
fallback. `crates/sam/src/panel.rs` checks the single-word gate, definition ordering and that
the rich card is valid Pango markup.
