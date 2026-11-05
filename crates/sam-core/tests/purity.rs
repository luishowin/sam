//! §0 F2: the core stays platform-free. Adding a dependency or an OS call
//! here means changing the register first.

use std::path::Path;

const ALLOWED: &[&str] = &["serde", "toml", "nucleo-matcher"];

/// Paths that would let the core act on the machine it runs on.
const FORBIDDEN: &[&str] = &[
    "std::fs",
    "std::process",
    "std::net",
    "std::os",
    "std::env",
    "std::thread",
    "std::io::stdin",
    "Command::new",
    "File::open",
    "File::create",
    "extern \"C\"",
];

#[test]
fn dependencies_are_the_allowlist() {
    let manifest: toml::Table = toml::from_str(include_str!("../Cargo.toml")).unwrap();
    for section in ["dependencies", "build-dependencies"] {
        let names: Vec<String> = manifest
            .get(section)
            .and_then(|d| d.as_table())
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default();
        for name in &names {
            assert!(
                section == "dependencies" && ALLOWED.contains(&name.as_str()),
                "sam-core may not depend on {name:?} ({section}); see ARCHITECTURE.md §0 F2"
            );
        }
    }
    assert!(
        manifest.get("target").is_none(),
        "no platform-specific dependencies either"
    );
    assert!(
        !Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("build.rs")
            .exists(),
        "no build script"
    );
}

#[test]
fn source_never_reaches_for_the_os() {
    fn visit(dir: &Path, offences: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, offences);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or("");
                    for bad in FORBIDDEN {
                        if code.contains(bad) {
                            offences.push(format!("{}:{}: {bad}", path.display(), n + 1));
                        }
                    }
                }
            }
        }
    }
    let mut offences = Vec::new();
    visit(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut offences,
    );
    assert!(
        offences.is_empty(),
        "sam-core must not touch the OS (§0 F2):\n{}",
        offences.join("\n")
    );
}
