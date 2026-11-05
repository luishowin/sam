//! Typed paths: `~/Downloads` opens the folder; `~/Down` completes (§5).

use std::path::{Path, PathBuf};

use sam_core::engine::{Mode, Provider, Query};
use sam_core::intent::{Intent, Source};
use sam_core::rank::{Candidate, Kind, Scorer};

use crate::config::{expand_home, tilde};

const COMPLETIONS: usize = 6;

pub struct PathProvider;

fn uri(path: &Path) -> Option<String> {
    glib::filename_to_uri(path, None).ok().map(Into::into)
}

fn row(path: &Path, kind: Kind, score: f32) -> Option<Candidate> {
    let uri = uri(path)?;
    let is_dir = path.is_dir();
    let (content_type, _) = gio::content_type_guess(Some(path), None::<&[u8]>);
    let content_type = if is_dir {
        "inode/directory".into()
    } else {
        content_type
    };
    let icon = gio::prelude::IconExt::to_string(&gio::content_type_get_icon(&content_type))
        .map(|s| s.to_string())
        .unwrap_or_default();
    let what = gio::content_type_get_description(&content_type);
    let mut shown = tilde(path);
    if is_dir && !shown.ends_with('/') {
        shown.push('/');
    }
    Some(Candidate {
        title: shown,
        subtitle: what.to_string(),
        kind,
        icon,
        score,
        key: format!("uri:{uri}"),
        intent: Intent::new("file.open", Source::Provider).arg("uri", uri.as_str()),
        alt: Some(Intent::new("file.reveal", Source::Provider).arg("uri", uri.as_str())),
    })
}

/// Entries of `dir` whose names start with `prefix` (ignoring case), hidden
/// ones only when the prefix asks for them; directories first.
pub fn completions(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let wanted = prefix.to_lowercase();
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.starts_with(&wanted) && (!name.starts_with('.') || wanted.starts_with('.'))
        })
        .map(|e| e.path())
        .collect();
    out.sort_by_key(|p| {
        (
            !p.is_dir(),
            p.file_name().map(|n| n.to_string_lossy().to_lowercase()),
        )
    });
    out.truncate(COMPLETIONS);
    out
}

impl Provider for PathProvider {
    fn provide(&self, query: &Query, _: &Scorer, out: &mut Vec<Candidate>) {
        if query.mode != Mode::Path
            || query.raw.starts_with("~") && !(query.raw == "~" || query.raw.starts_with("~/"))
        {
            return;
        }
        let typed = expand_home(&query.raw);
        let exact = typed.exists();
        if exact {
            out.extend(row(&typed, Kind::Path, 1.0));
        }
        // Complete the last component, or list a directory typed with a slash.
        let (dir, prefix) = if query.raw.ends_with('/') {
            (typed.clone(), String::new())
        } else {
            let prefix = typed
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (
                typed
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| typed.clone()),
                prefix,
            )
        };
        for (i, path) in completions(&dir, &prefix).into_iter().enumerate() {
            if exact && path == typed {
                continue;
            }
            let kind = if path.is_dir() {
                Kind::Folder
            } else {
                Kind::File
            };
            out.extend(row(&path, kind, 0.9 - i as f32 * 0.01));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::LinuxHost;
    use crate::projects::tests::TempTree;
    use sam_core::engine::Engine;

    #[test]
    fn completes_in_order_and_hides_dotfiles() {
        let t = TempTree::new("paths");
        t.file("Downloads-list.txt");
        t.dir("Downloads");
        t.dir("Documents");
        t.dir(".dotdir");
        let names = |prefix: &str| -> Vec<String> {
            completions(&t.0, prefix)
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            names("do"),
            ["Documents", "Downloads", "Downloads-list.txt"]
        );
        assert_eq!(names("DOWN"), ["Downloads", "Downloads-list.txt"]);
        assert_eq!(names(".d"), [".dotdir"]);
        assert!(!names("").contains(&".dotdir".to_owned()));
    }

    #[test]
    fn a_typed_home_path_is_pinned_first() {
        let engine = Engine::new();
        let results = engine.search("~", &LinuxHost, &[&PathProvider]);
        let rows = engine.rank(&results, 8);
        assert_eq!(rows[0].kind, Kind::Path);
        assert_eq!(rows[0].title, "~/");
        assert!(
            rows[0]
                .intent
                .to_string()
                .starts_with("file.open {uri: \"file:///")
        );
        // "~root" is someone else's home; SAM does not guess.
        assert!(
            engine
                .rank(&engine.search("~root", &LinuxHost, &[&PathProvider]), 8)
                .is_empty()
        );
    }
}
