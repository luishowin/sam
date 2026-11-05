//! Git repositories under the configured roots (F8). LocalSearch cannot see
//! them, so SAM looks for itself: depth-limited, descending past repos,
//! refreshed on summon when older than five minutes (F12).

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use sam_core::engine::{Mode, Provider, Query};
use sam_core::intent::{Intent, Source};
use sam_core::rank::{Candidate, Kind, Scorer};

use crate::config::tilde;

pub const STALE_AFTER: Duration = Duration::from_secs(5 * 60);

/// Directories never worth descending into.
pub const SKIP: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    "__pycache__",
    "venv",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub uri: String,
}

/// Every directory holding a `.git` (directory or worktree file) within
/// `depth` levels of a root. Roots may themselves be repos, and repos may
/// hold repos: the walk does not stop at one.
pub fn scan(roots: &[PathBuf], depth: usize) -> Vec<Project> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        walk(root, 0, depth, &mut out, &mut seen);
    }
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.path.cmp(&b.path))
    });
    out
}

fn walk(
    dir: &Path,
    level: usize,
    depth: usize,
    out: &mut Vec<Project>,
    seen: &mut std::collections::HashSet<PathBuf>,
) {
    let Ok(canonical) = dir.canonicalize() else {
        return;
    };
    if !seen.insert(canonical) {
        return;
    }
    if dir.join(".git").exists()
        && let (Some(name), Ok(uri)) = (dir.file_name(), glib::filename_to_uri(dir, None))
    {
        out.push(Project {
            name: name.to_string_lossy().into_owned(),
            path: dir.to_path_buf(),
            uri: uri.into(),
        });
    }
    if level >= depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIP.contains(&name.as_ref()) {
            continue;
        }
        // No symlinks: they loop, and they duplicate.
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&entry.path(), level + 1, depth, out, seen);
        }
    }
}

/// The project list, shared with the thread that refreshes it.
#[derive(Clone)]
pub struct ProjectIndex {
    roots: Arc<Vec<PathBuf>>,
    depth: usize,
    state: Arc<RwLock<(Vec<Project>, Option<Instant>)>>,
}

impl ProjectIndex {
    pub fn new(roots: Vec<PathBuf>, depth: usize) -> ProjectIndex {
        ProjectIndex {
            roots: Arc::new(roots),
            depth,
            state: Arc::default(),
        }
    }

    pub fn refresh_now(&self) {
        let projects = scan(&self.roots, self.depth);
        if let Ok(mut state) = self.state.write() {
            *state = (projects, Some(Instant::now()));
        }
    }

    /// Rescans on a thread of its own if the list is older than
    /// [`STALE_AFTER`]; the panel never waits for it.
    pub fn refresh_if_stale(&self) {
        let fresh = self
            .state
            .read()
            .ok()
            .and_then(|s| s.1)
            .is_some_and(|at| at.elapsed() < STALE_AFTER);
        if fresh {
            return;
        }
        if let Ok(mut state) = self.state.write() {
            // Mark it fresh now, so a burst of summons starts one scan.
            state.1 = Some(Instant::now());
        }
        let index = self.clone();
        std::thread::spawn(move || index.refresh_now());
    }

    pub fn projects(&self) -> Vec<Project> {
        self.state.read().map(|s| s.0.clone()).unwrap_or_default()
    }

    /// The project called exactly `name`, ignoring case.
    pub fn named(&self, name: &str) -> Option<Project> {
        self.state
            .read()
            .ok()?
            .0
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .cloned()
    }
}

impl Provider for ProjectIndex {
    fn provide(&self, query: &Query, scorer: &Scorer, out: &mut Vec<Candidate>) {
        if !matches!(query.mode, Mode::Normal | Mode::Open | Mode::Find) || query.subject.is_empty()
        {
            return;
        }
        let Ok(state) = self.state.read() else { return };
        for p in &state.0 {
            let Some(score) = scorer.name(&query.subject, &p.name) else {
                continue;
            };
            out.push(Candidate {
                title: p.name.clone(),
                subtitle: format!("Git repository · {}", tilde(&p.path)),
                kind: Kind::Project,
                icon: "folder-symbolic".into(),
                score,
                key: format!("uri:{}", p.uri),
                intent: Intent::new("file.open", Source::Provider).arg("uri", p.uri.as_str()),
                alt: Some(Intent::new("file.reveal", Source::Provider).arg("uri", p.uri.as_str())),
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh directory under the system temp dir, removed on drop.
    pub struct TempTree(pub PathBuf);

    impl TempTree {
        pub fn new(tag: &str) -> TempTree {
            let dir = std::env::temp_dir().join(format!(
                "sam-test-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempTree(dir)
        }

        pub fn dir(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            p
        }

        pub fn file(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
            p
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_repos_inside_a_repo_and_skips_the_noise() {
        let t = TempTree::new("scan");
        // ~/Documents/GitHub is itself a repo, holding the others.
        t.dir("GitHub/.git");
        t.dir("GitHub/photodesk/.git");
        t.dir("GitHub/photodesk/node_modules/dep/.git");
        t.dir("GitHub/fastpotify/.git");
        t.file("GitHub/worktree/.git");
        t.dir("GitHub/.hidden/secret/.git");
        t.dir("GitHub/a/b/c/too-deep/.git");
        t.dir("GitHub/notes");
        let names: Vec<String> = scan(&[t.0.join("GitHub")], 3)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["fastpotify", "GitHub", "photodesk", "worktree"]);
    }

    #[test]
    fn overlapping_roots_count_once() {
        let t = TempTree::new("overlap");
        t.dir("code/app/.git");
        let projects = scan(&[t.0.clone(), t.0.join("code")], 3);
        assert_eq!(projects.len(), 1);
        assert!(projects[0].uri.starts_with("file:///"));
    }

    #[test]
    fn named_lookup_and_refresh() {
        let t = TempTree::new("index");
        t.dir("photodesk/.git");
        let index = ProjectIndex::new(vec![t.0.clone()], 2);
        assert!(
            index.named("PhotoDesk").is_none(),
            "nothing before the first scan"
        );
        index.refresh_now();
        assert_eq!(index.named("PhotoDesk").unwrap().name, "photodesk");
    }
}
