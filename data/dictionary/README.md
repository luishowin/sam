# Offline UK-English dictionary data

The generated files are committed, so building Sam does not need Python or network access.

## Sources

- **Definitions, examples, synonyms, antonyms, UK pronunciation, hyphenation and
  etymology**: English Wiktionary, through the
  [Wiktextract](https://kaikki.org/dictionary/English/) extract. This data is licensed
  **CC BY-SA 3.0**; see [`LICENSE.wiktionary`](LICENSE.wiktionary).
- **Headword list**: WordNet 3.0 lemmas, used only to choose a bounded and familiar set of
  headwords. No WordNet definitions are shipped. See [`LICENSE.wordnet`](LICENSE.wordnet).

The Rust code remains MIT. The dictionary data files are a separate, share-alike work.

## Files

- `dictionary.dat`: one line per sense,
  `lemma<TAB>pos<TAB>definition<TAB>example<TAB>synonyms<TAB>antonyms`, sorted by lemma.
- `sounds.dat`: `lemma<TAB>UK IPA<TAB>hyphenated form`.
- `origin.dat`: `lemma<TAB>etymology`.
- `*.idx`: one little-endian `u32` byte offset and `u32` byte length per line.

Sam binary-searches each index over a memory map. `dictionary.dat` is read for the preview
row; `sounds.dat` and `origin.dat` are only mapped when the detail card opens.

## Regenerate

```sh
python3 -m pip install nltk
python3 -m nltk.downloader wordnet

curl -L -o /tmp/kaikki-en.jsonl.gz \
    https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl.gz

./scripts/build-dictionary.py --wiktextract /tmp/kaikki-en.jsonl.gz
```

The Wiktextract file is about 534 MB compressed and 3.4 GB uncompressed; it is only needed
when regenerating. The build script keeps at most six senses per part of speech and caps
definitions, examples and etymologies so the shipped data stays reasonable.

Current extract: 87,951 headwords, 214,520 senses, 44,708 UK pronunciations and 69,045
etymologies.

WordNet 3.0 Copyright 2006 by Princeton University. Wiktionary content is contributed by
the Wiktionary community and licensed CC BY-SA 3.0.
