//! A small frozen evaluation set for the deterministic grammar and actions.
//! The corpus lives in `registry/eval/queries.toml`; this test is the gate.

use sam_core::engine::{Engine, Host, PlaceInfo};
use sam_core::files::Clock;
use sam_core::rank::Kind;

struct TestHost;

impl Host for TestHost {
    fn place(&self, id: &str) -> Option<PlaceInfo> {
        (id == "downloads").then(|| PlaceInfo {
            title: "Downloads".into(),
            subtitle: "~/Downloads".into(),
            uri: "file:///home/test/Downloads".into(),
            icon: "folder-download-symbolic".into(),
        })
    }

    fn clock(&self) -> Clock {
        Clock {
            now: 1_800_000_000,
            utc_offset: 0,
        }
    }
}

#[derive(serde::Deserialize)]
struct Corpus {
    case: Vec<Case>,
}

#[derive(serde::Deserialize)]
struct Case {
    query: String,
    action: String,
    #[serde(default)]
    kind: Option<String>,
}

#[test]
fn the_frozen_corpus_dispatches_to_its_canonical_intent() {
    let text = include_str!("../../../registry/eval/queries.toml");
    let corpus: Corpus = toml::from_str(text).expect("the eval corpus parses");
    assert!(corpus.case.len() >= 20, "the corpus is too small");

    let engine = Engine::new();
    for case in corpus.case {
        let results = engine.search(&case.query, &TestHost, &[]);
        let rows = engine.rank(&results, 8);
        let top = rows.first().unwrap_or_else(|| panic!("{}: no rows", case.query));
        assert_eq!(
            top.intent.action, case.action,
            "{}: got {} ({:?})",
            case.query, top.intent.action, top.kind
        );
        if let Some(kind) = case.kind {
            let expected = match kind.as_str() {
                "Command" => Kind::Command,
                "Calculation" => Kind::Calculation,
                "Conversion" => Kind::Conversion,
                "Site" => Kind::Site,
                "Place" => Kind::Place,
                other => panic!("unknown kind {other}"),
            };
            assert_eq!(top.kind, expected, "{}", case.query);
        }
    }
}

#[test]
fn out_of_domain_text_never_becomes_a_command() {
    let engine = Engine::new();
    for query in [
        "how many planets are in the solar system",
        "who won the election",
        "write me a poem",
    ] {
        let results = engine.search(query, &TestHost, &[]);
        let rows = engine.rank(&results, 8);
        assert!(
            rows.iter().all(|r| r.kind != Kind::Command),
            "{query}: {rows:#?}"
        );
        assert!(
            rows.iter().any(|r| r.kind == Kind::Site),
            "{query}: the web handoff should be offered"
        );
    }
}
