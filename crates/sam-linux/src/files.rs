//! File search (§9): LocalSearch for everything it indexes (F7, F18), a
//! bounded walk inside a project it cannot see (F8), and the relaxed retry
//! when a typed search finds nothing.

use std::collections::HashMap;
use std::sync::Mutex;

use sam_core::files::{FileKind, FileQuery};
use sam_core::intent::{Intent, Source};
use sam_core::rank::{Candidate, Kind};

use crate::config::tilde;
use crate::projects::{Project, ProjectIndex, SKIP};
use crate::tinysparql::{Connection, Statement};

const LIMIT: usize = 20;
const WALK_ENTRIES: usize = 5000;
const WALK_DEPTH: usize = 6;
/// 2100-01-01, the open end of an unbounded range.
const FAR_FUTURE: i64 = 4_102_444_800;

/// Score for a file-name hit and a content hit (§5); newer rows get a hair more.
const NAME_HIT: f32 = 0.75;
const CONTENT_HIT: f32 = 0.6;

const MATCH_UNION: &str = r#"
  { GRAPH tracker:FileSystem { ?f fts:match ~match . BIND (2 AS ?hit) } }
  UNION { GRAPH tracker:FileSystem { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Audio { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Documents { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Video { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Software { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
  UNION { GRAPH tracker:Pictures { ?e fts:match ~match ; nie:isStoredAs ?f . BIND (1 AS ?hit) } }
"#;

const DOCUMENTS: &[&str] = &[
    "application/pdf",
    "application/msword",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.oasis.opendocument.text",
    "application/rtf",
    "text/plain",
    "text/markdown",
];
const SPREADSHEETS: &[&str] = &[
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.ms-excel",
    "text/csv",
];
const PRESENTATIONS: &[&str] = &[
    "application/vnd.oasis.opendocument.presentation",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.ms-powerpoint",
];

fn is_screenshot(name: &str, path: &str) -> bool {
    name.to_lowercase().contains("screenshot") || path.contains("/Screenshots/")
}

/// Whether a content type is of `kind` (used by the walk; the SPARQL filter
/// below says the same thing).
pub fn kind_matches(kind: FileKind, mime: &str, name: &str, path: &str) -> bool {
    match kind {
        FileKind::Pdf => mime == "application/pdf",
        FileKind::Image => mime.starts_with("image/"),
        FileKind::Screenshot => mime.starts_with("image/") && is_screenshot(name, path),
        FileKind::Video => mime.starts_with("video/"),
        FileKind::Document => DOCUMENTS.contains(&mime),
        FileKind::Spreadsheet => SPREADSHEETS.contains(&mime),
        FileKind::Presentation => PRESENTATIONS.contains(&mime),
        FileKind::Music => mime.starts_with("audio/"),
        FileKind::Folder => mime == "inode/directory",
    }
}

fn quoted(list: &[&str]) -> String {
    list.iter()
        .map(|m| format!("\"{m}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A SPARQL condition for `kind`. Built only from the constants above — no
/// user text (F10).
fn kind_filter(kind: FileKind) -> String {
    match kind {
        FileKind::Pdf => r#"?mime = "application/pdf""#.into(),
        FileKind::Image => r#"STRSTARTS(?mime, "image/")"#.into(),
        FileKind::Screenshot => {
            r#"(STRSTARTS(?mime, "image/") && (CONTAINS(LCASE(?name), "screenshot") || CONTAINS(?url, "/Screenshots/")))"#.into()
        }
        FileKind::Video => r#"STRSTARTS(?mime, "video/")"#.into(),
        FileKind::Document => format!("?mime IN ({})", quoted(DOCUMENTS)),
        FileKind::Spreadsheet => format!("?mime IN ({})", quoted(SPREADSHEETS)),
        FileKind::Presentation => format!("?mime IN ({})", quoted(PRESENTATIONS)),
        FileKind::Music => r#"STRSTARTS(?mime, "audio/")"#.into(),
        FileKind::Folder => r#"?mime = "inode/directory""#.into(),
    }
}

/// The query for `kinds`, with or without a full-text match. Its only
/// parameters are `~match`, `~from` and `~to`.
pub fn sparql(with_terms: bool, kinds: &[FileKind]) -> String {
    let kind_clause = if kinds.is_empty() {
        String::new()
    } else {
        let any = kinds
            .iter()
            .map(|k| kind_filter(*k))
            .collect::<Vec<_>>()
            .join(" || ");
        format!("FILTER (BOUND(?mime) && ({any}))")
    };
    if with_terms {
        format!(
            r#"SELECT ?url ?name ?mime ?modified (MAX(?hit) AS ?rank) WHERE {{
{MATCH_UNION}
  GRAPH tracker:FileSystem {{ ?f nie:url ?url ; nfo:fileName ?name ; nfo:fileLastModified ?modified . }}
  OPTIONAL {{ ?any nie:isStoredAs ?f ; nie:mimeType ?mime . }}
  FILTER (?modified >= ~from && ?modified < ~to)
  {kind_clause}
}}
GROUP BY ?url ?name ?mime ?modified
ORDER BY DESC(?rank) DESC(?modified)
LIMIT {LIMIT}"#
        )
    } else {
        format!(
            r#"SELECT ?url ?name ?mime ?modified (1 AS ?rank) WHERE {{
  GRAPH tracker:FileSystem {{ ?f nie:url ?url ; nfo:fileName ?name ; nfo:fileLastModified ?modified . }}
  OPTIONAL {{ ?any nie:isStoredAs ?f ; nie:mimeType ?mime . }}
  FILTER (?modified >= ~from && ?modified < ~to)
  {kind_clause}
}}
ORDER BY DESC(?modified)
LIMIT {LIMIT}"#
        )
    }
}

struct Hit {
    uri: String,
    name: String,
    mime: Option<String>,
    modified: Option<i64>,
    rank: i64,
}

fn describe_date(unix: i64) -> String {
    let (Ok(then), Ok(now)) = (
        glib::DateTime::from_unix_local(unix),
        glib::DateTime::now_local(),
    ) else {
        return String::new();
    };
    let day = |d: &glib::DateTime| (d.to_unix() + d.utc_offset().as_seconds()).div_euclid(86_400);
    let format = match day(&now) - day(&then) {
        0 => "today %H:%M",
        1 => "yesterday %H:%M",
        _ if then.year() == now.year() => "%-d %b",
        _ => "%-d %b %Y",
    };
    then.format(format)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn candidate(hit: Hit, index: usize, note: &str) -> Option<Candidate> {
    let (path, _) = glib::filename_from_uri(&hit.uri).ok()?;
    let mime = hit.mime.unwrap_or_else(|| {
        gio::content_type_guess(Some(&path), None::<&[u8]>)
            .0
            .to_string()
    });
    let is_dir = mime == "inode/directory";
    let icon = gio::prelude::IconExt::to_string(&gio::content_type_get_icon(&mime))
        .map(|s| s.to_string())
        .unwrap_or_default();
    let parent = path.parent().map(tilde).unwrap_or_default();
    let when = hit.modified.map(describe_date).unwrap_or_default();
    let mut subtitle = [note, parent.as_str(), when.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if subtitle.is_empty() {
        subtitle = gio::content_type_get_description(&mime).to_string();
    }
    let base = if hit.rank >= 2 { NAME_HIT } else { CONTENT_HIT };
    Some(Candidate {
        title: hit.name,
        subtitle,
        kind: if is_dir { Kind::Folder } else { Kind::File },
        icon,
        score: base - index as f32 * 0.001,
        key: format!("uri:{}", hit.uri),
        intent: Intent::new("file.open", Source::Provider).arg("uri", hit.uri.as_str()),
        alt: Some(Intent::new("file.reveal", Source::Provider).arg("uri", hit.uri.as_str())),
    })
}

/// LocalSearch, plus the walk. Safe to share with worker threads.
pub struct FileIndex {
    connection: Mutex<Option<Connection>>,
    statements: Mutex<HashMap<String, Statement>>,
    projects: ProjectIndex,
}

impl FileIndex {
    pub fn new(projects: ProjectIndex) -> FileIndex {
        FileIndex {
            connection: Mutex::default(),
            statements: Mutex::default(),
            projects,
        }
    }

    /// Connects to LocalSearch now, so the first search does not.
    pub fn warm(&self) {
        if let Ok(mut connection) = self.connection.lock()
            && connection.is_none()
        {
            match Connection::localsearch() {
                Ok(c) => *connection = Some(c),
                Err(e) => crate::log::warn("files-unavailable", None, e),
            }
        }
    }

    fn localsearch(&self, query: &FileQuery) -> Result<Vec<Hit>, String> {
        let fts = query.fts();
        let text = sparql(fts.is_some(), &query.kinds);
        let mut statements = self.statements.lock().map_err(|e| e.to_string())?;
        if !statements.contains_key(&text) {
            let mut connection = self.connection.lock().map_err(|e| e.to_string())?;
            if connection.is_none() {
                *connection =
                    Some(Connection::localsearch().map_err(|e| format!("LocalSearch: {e}"))?);
            }
            let statement = connection
                .as_ref()
                .expect("set above")
                .statement(&text)
                .map_err(|e| e.to_string())?;
            statements.insert(text.clone(), statement);
        }
        let statement = statements.get_mut(&text).expect("inserted above");
        statement.clear();
        if let Some(fts) = &fts {
            statement.bind_str("match", fts);
        }
        let (from, to) = query
            .range
            .as_ref()
            .map_or((0, FAR_FUTURE), |r| (r.from, r.to));
        statement
            .bind_unix_time("from", from)
            .map_err(|e| e.to_string())?;
        statement
            .bind_unix_time("to", to)
            .map_err(|e| e.to_string())?;
        let mut cursor = statement.execute().map_err(|e| e.to_string())?;
        let mut hits = Vec::new();
        while cursor.next_row().map_err(|e| e.to_string())? {
            let (Some(uri), Some(name)) = (cursor.string(0), cursor.string(1)) else {
                continue;
            };
            hits.push(Hit {
                uri,
                name,
                mime: cursor.string(2),
                modified: cursor
                    .string(3)
                    .and_then(|m| glib::DateTime::from_iso8601(&m, None).ok())
                    .map(|d| d.to_unix()),
                rank: cursor.string(4).and_then(|r| r.parse().ok()).unwrap_or(1),
            });
        }
        Ok(hits)
    }

    fn walk(project: &Project, query: &FileQuery) -> Vec<Hit> {
        let terms: Vec<String> = query.terms.iter().map(|t| t.to_lowercase()).collect();
        let mut hits = Vec::new();
        let mut budget = WALK_ENTRIES;
        let mut stack = vec![(project.path.clone(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if budget == 0 {
                    break;
                }
                budget -= 1;
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') || SKIP.contains(&name.as_str()) {
                    continue;
                }
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() && depth < WALK_DEPTH {
                    stack.push((path.clone(), depth + 1));
                }
                let lower = name.to_lowercase();
                if !terms.iter().all(|t| lower.contains(t.as_str())) {
                    continue;
                }
                let mime = if kind.is_dir() {
                    "inode/directory".to_owned()
                } else {
                    gio::content_type_guess(Some(&path), None::<&[u8]>)
                        .0
                        .to_string()
                };
                let shown = path.to_string_lossy();
                if !query.kinds.is_empty()
                    && !query
                        .kinds
                        .iter()
                        .any(|k| kind_matches(*k, &mime, &name, &shown))
                {
                    continue;
                }
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                if let Some(range) = &query.range
                    && !modified.is_some_and(|m| (range.from..range.to).contains(&m))
                {
                    continue;
                }
                let Ok(uri) = glib::filename_to_uri(&path, None) else {
                    continue;
                };
                hits.push(Hit {
                    uri: uri.into(),
                    name,
                    mime: Some(mime),
                    modified,
                    rank: 2,
                });
            }
        }
        hits.sort_by_key(|h| std::cmp::Reverse(h.modified));
        hits.truncate(LIMIT);
        hits
    }

    /// The rows for `query`. Blocking; run it off the main thread.
    pub fn search(&self, query: &FileQuery) -> Result<Vec<Candidate>, String> {
        let rows = |hits: Vec<Hit>, note: &str| -> Vec<Candidate> {
            hits.into_iter()
                .enumerate()
                .filter_map(|(i, h)| candidate(h, i, note))
                .collect()
        };
        // A term that names a project scopes the search to it, when there is
        // something left to look for inside.
        let scope = query.terms.iter().find_map(|t| self.projects.named(t));
        if let Some(project) = &scope {
            let inner = FileQuery {
                terms: query
                    .terms
                    .iter()
                    .filter(|t| !t.eq_ignore_ascii_case(&project.name))
                    .cloned()
                    .collect(),
                ..query.clone()
            };
            if !inner.kinds.is_empty() || !inner.terms.is_empty() {
                let found = Self::walk(project, &inner);
                if !found.is_empty() {
                    return Ok(rows(found, &format!("In {}", project.name)));
                }
                if !inner.kinds.is_empty() || inner.range.is_some() {
                    let note = format!("No match in {}", project.name);
                    return Ok(rows(self.localsearch(&inner.without_terms())?, &note));
                }
            }
        }
        let found = self.localsearch(query)?;
        if found.is_empty()
            && !query.terms.is_empty()
            && (!query.kinds.is_empty() || query.range.is_some())
        {
            let note = format!("No match for “{}”", query.terms.join(" "));
            return Ok(rows(self.localsearch(&query.without_terms())?, &note));
        }
        Ok(rows(found, ""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::tests::TempTree;
    use sam_core::files::{self as core_files, Clock};

    const EVERY_KIND: &[FileKind] = &[
        FileKind::Pdf,
        FileKind::Image,
        FileKind::Screenshot,
        FileKind::Video,
        FileKind::Document,
        FileKind::Spreadsheet,
        FileKind::Presentation,
        FileKind::Music,
        FileKind::Folder,
    ];

    #[test]
    fn queries_have_only_the_three_parameters() {
        for with_terms in [true, false] {
            for kinds in [&[][..], &[FileKind::Pdf], EVERY_KIND] {
                let q = sparql(with_terms, kinds);
                let params: std::collections::BTreeSet<&str> = q
                    .match_indices('~')
                    .map(|(i, _)| {
                        q[i + 1..]
                            .split(|c: char| !c.is_alphanumeric())
                            .next()
                            .unwrap()
                    })
                    .collect();
                let expected: std::collections::BTreeSet<&str> = if with_terms {
                    ["from", "match", "to"].into()
                } else {
                    ["from", "to"].into()
                };
                assert_eq!(params, expected, "{q}");
            }
        }
    }

    #[test]
    fn walk_scopes_to_the_project() {
        let t = TempTree::new("walk");
        t.dir("photodesk/.git");
        t.file("photodesk/docs/Screenshot from 2026-09-01.png");
        t.file("photodesk/app/icons/128x128.png");
        t.file("photodesk/node_modules/pkg/Screenshot.png");
        t.file("photodesk/.cache/Screenshot.png");
        let project = Project {
            name: "photodesk".into(),
            path: t.0.join("photodesk"),
            uri: glib::filename_to_uri(t.0.join("photodesk"), None)
                .unwrap()
                .into(),
        };
        let clock = Clock {
            now: 1_789_581_600,
            utc_offset: 0,
        };
        let words: Vec<String> = ["screenshots"].map(String::from).to_vec();
        let q = core_files::parse(&words, clock);
        let names: Vec<String> = FileIndex::walk(&project, &q)
            .into_iter()
            .map(|h| h.name)
            .collect();
        assert_eq!(names, ["Screenshot from 2026-09-01.png"]);

        let words: Vec<String> = ["icons"].map(String::from).to_vec();
        let q = core_files::parse(&words, clock);
        let names: Vec<String> = FileIndex::walk(&project, &q)
            .into_iter()
            .map(|h| h.name)
            .collect();
        assert_eq!(names, ["icons"], "a folder whose name matches");
    }

    #[test]
    fn kinds_by_type() {
        assert!(kind_matches(
            FileKind::Screenshot,
            "image/png",
            "Screenshot From 2026.png",
            "/x"
        ));
        assert!(kind_matches(
            FileKind::Screenshot,
            "image/png",
            "a.png",
            "/home/l/Pictures/Screenshots/a.png"
        ));
        assert!(!kind_matches(
            FileKind::Screenshot,
            "image/png",
            "icon.png",
            "/x/icon.png"
        ));
        assert!(kind_matches(FileKind::Document, "application/pdf", "", ""));
        assert!(!kind_matches(FileKind::Image, "application/pdf", "", ""));
    }

    fn live_index() -> FileIndex {
        let projects = ProjectIndex::new(crate::config::Config::load().project_roots(), 3);
        projects.refresh_now();
        FileIndex::new(projects)
    }

    fn live(text: &str) -> Vec<Candidate> {
        let words = sam_core::text::normalize(text);
        let q = core_files::parse(
            &words,
            sam_core::engine::Host::clock(&crate::host::LinuxHost),
        );
        live_index().search(&q).expect("search runs")
    }

    #[test]
    #[ignore = "needs LocalSearch and this machine's files"]
    fn live_the_plan_requests() {
        let pdf = live("PDF about invoices");
        assert!(
            pdf.iter()
                .any(|c| c.title.ends_with(".pdf") && c.subtitle.contains("nanyukicountryhome")),
            "{pdf:#?}"
        );
        assert!(pdf.iter().all(|c| c.title.ends_with(".pdf")));

        let shots = live("PhotoDesk screenshots");
        assert!(!shots.is_empty());
        assert!(
            shots[0].subtitle.starts_with("No match in photodesk"),
            "{:#?}",
            shots[0]
        );
        assert!(
            shots
                .iter()
                .all(|c| c.title.to_lowercase().contains("screenshot"))
        );

        let yesterday = live("screenshots from today");
        assert!(
            yesterday.iter().all(|c| c.subtitle.contains("today")),
            "{yesterday:#?}"
        );
    }
}
