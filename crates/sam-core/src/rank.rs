//! Candidates, and how well a name matches what was typed (§5).

use std::cell::RefCell;

use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::intent::Intent;

/// What a row is. The order is the tie-break order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Calculation,
    Conversion,
    Path,
    Command,
    /// A live answer about this computer.
    Info,
    /// A local manual or installed document.
    Doc,
    /// "Ask Sam" (§15).
    Answer,
    /// `/help` (§7).
    Help,
    Place,
    App,
    /// A web shortcut (§17).
    Site,
    Setting,
    Project,
    Folder,
    File,
    Search,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Calculation => "Calculation",
            Kind::Conversion => "Conversion",
            Kind::Path => "Path",
            Kind::Command => "Command",
            Kind::Info => "Answer",
            Kind::Doc => "Documentation",
            Kind::Answer => "Ask",
            Kind::Help => "Help",
            Kind::Place => "Folder",
            Kind::App => "Application",
            Kind::Site => "Website",
            Kind::Setting => "Settings",
            Kind::Project => "Project",
            Kind::Folder => "Folder",
            Kind::File => "File",
            Kind::Search => "Search",
        }
    }
}

/// One row the panel can show.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub title: String,
    pub subtitle: String,
    pub kind: Kind,
    /// A GIcon string: a themed icon name, a serialised icon, or a path.
    pub icon: String,
    /// Relevance. Providers give 0–1; [`Engine`](crate::engine::Engine)
    /// weighs it by kind and mode.
    pub score: f32,
    /// Rows with the same key are one row; the higher score wins.
    pub key: String,
    /// What Enter does.
    pub intent: Intent,
    /// What Ctrl+Enter does, if anything.
    pub alt: Option<Intent>,
}

/// Name matching, shared by every provider. Not thread-safe: the panel
/// queries on its main thread.
pub struct Scorer {
    matcher: RefCell<Matcher>,
    buffers: RefCell<(Vec<char>, Vec<char>)>,
}

impl Default for Scorer {
    fn default() -> Self {
        Scorer::new()
    }
}

impl Scorer {
    pub fn new() -> Scorer {
        let mut config = Config::DEFAULT;
        config.prefer_prefix = true;
        Scorer {
            matcher: RefCell::new(Matcher::new(config)),
            buffers: RefCell::default(),
        }
    }

    fn fuzzy(&self, needle: &str, haystack: &str) -> Option<u16> {
        let mut matcher = self.matcher.borrow_mut();
        let mut buffers = self.buffers.borrow_mut();
        let (hb, nb) = &mut *buffers;
        matcher.fuzzy_match(Utf32Str::new(haystack, hb), Utf32Str::new(needle, nb))
    }

    /// How well `query` names `name`, 0–1, or `None` for no match.
    ///
    /// Exact 1.0 · prefix ~0.9 · word prefix ~0.8 · all words prefixes 0.8 ·
    /// initials 0.78 · otherwise a fuzzy score mapped into 0.3–0.7.
    pub fn name(&self, query: &str, name: &str) -> Option<f32> {
        let q = query.trim().to_lowercase();
        let n = name.trim().to_lowercase();
        if q.is_empty() || n.is_empty() {
            return None;
        }
        let q_len = q.chars().count() as f32;
        let n_len = n.chars().count() as f32;
        let ratio = (q_len / n_len).min(1.0);
        if q == n {
            return Some(1.0);
        }
        if n.starts_with(&q) {
            return Some(0.9 + 0.05 * ratio);
        }
        let words: Vec<&str> = n
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        if words.iter().any(|w| w.starts_with(&q)) {
            return Some(0.8 + 0.05 * ratio);
        }
        let q_words: Vec<&str> = q.split_whitespace().collect();
        if q_words.len() > 1
            && q_words
                .iter()
                .all(|qw| words.iter().any(|w| w.starts_with(qw)))
        {
            return Some(0.8);
        }
        if q_len >= 2.0 && !q.contains(' ') {
            let initials: String = words.iter().filter_map(|w| w.chars().next()).collect();
            if initials.len() > 1 && initials.starts_with(&q) {
                return Some(0.78);
            }
        }
        // Fuzzy: nucleo's score against the best this needle could get.
        let perfect = self.fuzzy(&q, &q)?;
        let score = self.fuzzy(&q, &n)?;
        let fraction = (f32::from(score) / f32::from(perfect.max(1))).min(1.0);
        // Three scattered letters match almost anything; ask more of short
        // queries.
        let needed = match q_len as usize {
            0..=3 => 0.6,
            4..=5 => 0.45,
            _ => 0.35,
        };
        (fraction >= needed).then_some(0.3 + 0.4 * fraction)
    }

    /// The best of several names, each weighted: `[(title, 1.0), (keyword, 0.9)]`.
    pub fn best<'a>(
        &self,
        query: &str,
        names: impl IntoIterator<Item = (&'a str, f32)>,
    ) -> Option<f32> {
        names
            .into_iter()
            .filter_map(|(name, weight)| self.name(query, name).map(|s| s * weight))
            .max_by(f32::total_cmp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers() {
        let s = Scorer::new();
        assert_eq!(s.name("photodesk", "PhotoDesk"), Some(1.0));
        let prefix = s.name("photo", "PhotoDesk").unwrap();
        assert!((0.9..1.0).contains(&prefix), "{prefix}");
        let word = s.name("chrome", "Google Chrome").unwrap();
        assert!((0.8..0.9).contains(&word), "{word}");
        assert_eq!(s.name("vsc", "Visual Studio Code"), Some(0.78));
        assert_eq!(s.name("vis stu", "Visual Studio Code"), Some(0.8));
        let fuzzy = s.name("calcltor", "Calculator").unwrap();
        assert!((0.3..0.7).contains(&fuzzy), "{fuzzy}");
        assert_eq!(s.name("zzz", "Calculator"), None);
        assert_eq!(s.name("", "Calculator"), None);
    }

    #[test]
    fn exact_beats_prefix_beats_word_beats_initials_beats_fuzzy() {
        let s = Scorer::new();
        let exact = s.name("files", "Files").unwrap();
        let prefix = s.name("fil", "Files").unwrap();
        let word = s.name("files", "GNOME Files").unwrap();
        let initials = s.name("gf", "GNOME Files").unwrap();
        let fuzzy = s.name("gnfls", "GNOME Files").unwrap();
        assert!(exact > prefix && prefix > word && word > initials && initials > fuzzy);
    }

    #[test]
    fn best_weighs_fields() {
        let s = Scorer::new();
        let got = s
            .best("chrome", [("Google Chrome", 1.0), ("chrome", 0.9)])
            .unwrap();
        assert!(
            (got - 0.9).abs() < 1e-6,
            "the id tail's exact match wins: {got}"
        );
    }
}
