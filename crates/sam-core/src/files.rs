//! What a "find …" asks the file index for (§9). Pure: the clock is passed
//! in by the host.

use crate::text::FILLERS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    Pdf,
    Image,
    Screenshot,
    Video,
    Document,
    Spreadsheet,
    Presentation,
    Music,
    Folder,
}

impl FileKind {
    fn from_word(word: &str) -> Option<FileKind> {
        Some(match word {
            "pdf" | "pdfs" => FileKind::Pdf,
            "image" | "images" | "photo" | "photos" | "picture" | "pictures" | "pic" | "pics"
            | "png" | "pngs" | "jpg" | "jpgs" | "jpeg" | "jpegs" => FileKind::Image,
            "screenshot" | "screenshots" | "screengrab" | "screengrabs" => FileKind::Screenshot,
            "video" | "videos" | "movie" | "movies" | "clip" | "clips" | "recording"
            | "recordings" => FileKind::Video,
            "document" | "documents" | "doc" | "docs" | "docx" | "odt" => FileKind::Document,
            "spreadsheet" | "spreadsheets" | "sheet" | "sheets" | "xlsx" | "ods" | "csv" => {
                FileKind::Spreadsheet
            }
            "presentation" | "presentations" | "slides" | "slideshow" | "deck" | "pptx" | "odp" => {
                FileKind::Presentation
            }
            "music" | "song" | "songs" | "audio" | "track" | "tracks" | "mp3" | "mp3s" => {
                FileKind::Music
            }
            "folder" | "folders" | "directory" | "directories" | "dir" | "dirs" => FileKind::Folder,
            _ => return None,
        })
    }

    pub fn plural(self) -> &'static str {
        match self {
            FileKind::Pdf => "PDFs",
            FileKind::Image => "images",
            FileKind::Screenshot => "screenshots",
            FileKind::Video => "videos",
            FileKind::Document => "documents",
            FileKind::Spreadsheet => "spreadsheets",
            FileKind::Presentation => "presentations",
            FileKind::Music => "music",
            FileKind::Folder => "folders",
        }
    }
}

/// Words that link the parts of a request and name nothing to search for.
const CONNECTORS: &[&str] = &[
    "about",
    "called",
    "named",
    "with",
    "from",
    "since",
    "containing",
    "contains",
    "for",
    "of",
    "in",
    "on",
    "at",
    "that",
    "which",
    "is",
    "are",
    "was",
    "were",
    "titled",
    "mentioning",
    "regarding",
    "file",
    "files",
    "and",
    "or",
    "all",
    "any",
    "recent",
    "latest",
    "newest",
    "new",
    "took",
    "taken",
    "made",
    "created",
    "saved",
    "downloaded",
    "edited",
    "repository",
    "repo",
    "repos",
    "project",
    "projects",
    "stuff",
    "things",
    "last",
    "this",
    "one",
    "ones",
    "every",
];

const MONTHS: &[(&str, &str)] = &[
    ("january", "jan"),
    ("february", "feb"),
    ("march", "mar"),
    ("april", "apr"),
    ("may", "may"),
    ("june", "jun"),
    ("july", "jul"),
    ("august", "aug"),
    ("september", "sep"),
    ("october", "oct"),
    ("november", "nov"),
    ("december", "dec"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    /// Unix seconds.
    pub now: i64,
    /// Seconds east of UTC.
    pub utc_offset: i32,
}

/// `[from, to)` in Unix seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeRange {
    pub from: i64,
    pub to: i64,
    /// "yesterday", "this week", "in June 2026".
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FileQuery {
    /// Letters and digits only (F10).
    pub terms: Vec<String>,
    pub kinds: Vec<FileKind>,
    pub range: Option<TimeRange>,
}

impl FileQuery {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.kinds.is_empty() && self.range.is_none()
    }

    /// The full-text expression: every term must match, as a prefix.
    pub fn fts(&self) -> Option<String> {
        (!self.terms.is_empty()).then(|| {
            self.terms
                .iter()
                .map(|t| format!("{t}*"))
                .collect::<Vec<_>>()
                .join(" ")
        })
    }

    /// The same request with its terms dropped: the relaxed fallback (§9).
    pub fn without_terms(&self) -> FileQuery {
        FileQuery {
            terms: Vec::new(),
            ..self.clone()
        }
    }

    /// "PDFs matching “invoices”", "screenshots from yesterday".
    pub fn describe(&self) -> String {
        let what = if self.kinds.is_empty() {
            "files".to_owned()
        } else {
            self.kinds
                .iter()
                .map(|k| k.plural())
                .collect::<Vec<_>>()
                .join(" and ")
        };
        let mut out = what;
        if !self.terms.is_empty() {
            out.push_str(&format!(" matching “{}”", self.terms.join(" ")));
        }
        if let Some(range) = &self.range {
            out.push_str(&format!(" from {}", range.label));
        }
        out
    }
}

// Howard Hinnant's civil-calendar algorithms.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn month_name(m: u32) -> String {
    let name = MONTHS[(m - 1) as usize].0;
    let mut chars = name.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// The day of the week of a calendar date: `Thursday`.
pub fn weekday_of(y: i64, m: u32, d: u32) -> &'static str {
    // 1970-01-01 was a Thursday.
    WEEKDAYS[(days_from_civil(y, m, d) + 3).rem_euclid(7) as usize]
}

impl Clock {
    fn today(&self) -> i64 {
        (self.now + i64::from(self.utc_offset)).div_euclid(86_400)
    }

    /// The same clock, `seconds` later.
    pub fn plus(&self, seconds: i64) -> Clock {
        Clock {
            now: self.now + seconds,
            ..*self
        }
    }

    /// Local hours and minutes.
    pub fn hour_minute(&self) -> (u32, u32) {
        let seconds = (self.now + i64::from(self.utc_offset)).rem_euclid(86_400);
        ((seconds / 3600) as u32, (seconds / 60 % 60) as u32)
    }

    /// `11:42`, local time.
    pub fn time_of_day(&self) -> String {
        let (h, m) = self.hour_minute();
        format!("{h:02}:{m:02}")
    }

    /// Local year, month and day.
    pub fn date(&self) -> (i64, u32, u32) {
        civil_from_days(self.today())
    }

    /// `Thursday`.
    pub fn weekday(&self) -> &'static str {
        let (y, m, d) = self.date();
        weekday_of(y, m, d)
    }

    /// `Thursday 17 September 2026`.
    pub fn long_date(&self) -> String {
        let (y, m, d) = self.date();
        format!("{} {d} {} {y}", self.weekday(), month_name(m))
    }

    /// `UTC+03:00`.
    pub fn zone(&self) -> String {
        let sign = if self.utc_offset < 0 { '-' } else { '+' };
        let minutes = self.utc_offset.unsigned_abs() / 60;
        format!("UTC{sign}{:02}:{:02}", minutes / 60, minutes % 60)
    }

    /// Unix time of local midnight starting `day` (days since the epoch).
    fn midnight(&self, day: i64) -> i64 {
        day * 86_400 - i64::from(self.utc_offset)
    }

    fn days(&self, from: i64, to: i64, label: &str) -> TimeRange {
        TimeRange {
            from: self.midnight(from),
            to: self.midnight(to),
            label: label.to_owned(),
        }
    }

    fn month(&self, y: i64, m: u32, label: String) -> TimeRange {
        let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        TimeRange {
            from: self.midnight(days_from_civil(y, m, 1)),
            to: self.midnight(days_from_civil(ny, nm, 1)),
            label,
        }
    }

    fn range(&self, words: &[&str]) -> Option<TimeRange> {
        let today = self.today();
        let (y, m, _) = civil_from_days(today);
        let monday = today - (today + 3).rem_euclid(7);
        Some(match words {
            ["today"] => self.days(today, today + 1, "today"),
            ["yesterday"] => self.days(today - 1, today, "yesterday"),
            ["this", "week"] => self.days(monday, today + 1, "this week"),
            ["last", "week"] => self.days(monday - 7, monday, "last week"),
            ["this", "month"] => self.month(y, m, "this month".into()),
            ["last", "month"] => {
                let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
                self.month(py, pm, "last month".into())
            }
            ["this", "year"] => TimeRange {
                from: self.midnight(days_from_civil(y, 1, 1)),
                to: self.midnight(days_from_civil(y + 1, 1, 1)),
                label: "this year".into(),
            },
            _ => return None,
        })
    }

    fn named_month(&self, index: u32) -> TimeRange {
        let today = self.today();
        let (y, m, _) = civil_from_days(today);
        // The most recent one: June asked in March means last June.
        let year = if index <= m { y } else { y - 1 };
        self.month(year, index, format!("{} {year}", month_name(index)))
    }

    fn year(&self, year: i64) -> TimeRange {
        TimeRange {
            from: self.midnight(days_from_civil(year, 1, 1)),
            to: self.midnight(days_from_civil(year + 1, 1, 1)),
            label: year.to_string(),
        }
    }
}

fn month_index(word: &str, after_preposition: bool) -> Option<u32> {
    MONTHS
        .iter()
        .position(|(full, short)| {
            // "may" is a verb too often to count unless it follows from/in.
            (word == *full && (word != "may" || after_preposition))
                || (word == *short && word != "may" && after_preposition)
        })
        .map(|i| i as u32 + 1)
}

/// Letters and digits, split at anything else.
fn sanitize(word: &str) -> Vec<String> {
    word.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Builds a file query from the words after "find".
pub fn parse(words: &[String], clock: Clock) -> FileQuery {
    let mut query = FileQuery::default();
    let mut i = 0;
    while i < words.len() {
        let w = words[i].as_str();
        let after_preposition =
            i > 0 && matches!(words[i - 1].as_str(), "from" | "in" | "since" | "during");
        if let Some(next) = words.get(i + 1)
            && let Some(range) = clock.range(&[w, next.as_str()])
        {
            query.range = Some(range);
            i += 2;
            continue;
        }
        if let Some(range) = clock.range(&[w]) {
            query.range = Some(range);
        } else if let Some(m) = month_index(w, after_preposition) {
            query.range = Some(clock.named_month(m));
        } else if after_preposition
            && w.len() == 4
            && let Ok(year) = w.parse::<i64>()
            && (1970..=2100).contains(&year)
        {
            query.range = Some(clock.year(year));
        } else if let Some(kind) = FileKind::from_word(w) {
            if !query.kinds.contains(&kind) {
                query.kinds.push(kind);
            }
        } else if !CONNECTORS.contains(&w) && !FILLERS.contains(&w) {
            for term in sanitize(w) {
                if !query.terms.contains(&term) {
                    query.terms.push(term);
                }
            }
        }
        i += 1;
    }
    query
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_clock_reads_like_a_clock() {
        // 2026-09-17 11:42:00 at UTC+3 is 08:42:00 UTC.
        let c = Clock {
            now: 1_789_634_520,
            utc_offset: 3 * 3600,
        };
        assert_eq!(c.time_of_day(), "11:42");
        assert_eq!(c.date(), (2026, 9, 17));
        assert_eq!(c.weekday(), "Thursday");
        assert_eq!(c.long_date(), "Thursday 17 September 2026");
        assert_eq!(c.zone(), "UTC+03:00");
        assert_eq!(
            c.plus(13 * 3600).weekday(),
            "Friday",
            "local midnight passed"
        );
        let west = Clock {
            now: 1_789_634_520,
            utc_offset: -(9 * 3600 + 30 * 60),
        };
        assert_eq!(west.time_of_day(), "23:12");
        assert_eq!(west.long_date(), "Wednesday 16 September 2026");
        assert_eq!(west.zone(), "UTC-09:30");
    }

    use super::*;

    /// 2026-09-16 21:00 in Nairobi (UTC+3), a Wednesday.
    const CLOCK: Clock = Clock {
        now: 1_789_581_600,
        utc_offset: 3 * 3600,
    };

    fn q(s: &str) -> FileQuery {
        let words: Vec<String> = crate::text::normalize(s);
        parse(&words, CLOCK)
    }

    #[test]
    fn calendar_arithmetic() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(days_from_civil(2026, 9, 16)), (2026, 9, 16));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(civil_from_days(CLOCK.today()), (2026, 9, 16));
        assert_eq!((CLOCK.today() + 3).rem_euclid(7), 2, "a Wednesday");
    }

    #[test]
    fn the_plan_requests() {
        let pdf = q("the PDF about invoices");
        assert_eq!(pdf.kinds, [FileKind::Pdf]);
        assert_eq!(pdf.terms, ["invoices"]);
        assert_eq!(pdf.fts().as_deref(), Some("invoices*"));
        assert_eq!(pdf.describe(), "PDFs matching “invoices”");

        let shots = q("the PhotoDesk screenshots");
        assert_eq!(shots.kinds, [FileKind::Screenshot]);
        assert_eq!(shots.terms, ["photodesk"]);
        assert_eq!(shots.without_terms().describe(), "screenshots");

        let repo = q("the PhotoDesk repository");
        assert_eq!(repo.terms, ["photodesk"]);
        assert!(repo.kinds.is_empty());

        let june = q("the invoice from June");
        assert_eq!(june.terms, ["invoice"]);
        assert_eq!(june.range.as_ref().unwrap().label, "June 2026");
    }

    #[test]
    fn time_ranges_are_local_days() {
        let today = q("screenshots from today").range.unwrap();
        // Local midnight in Nairobi is 21:00 UTC the day before.
        assert_eq!(today.from, 1_789_506_000);
        assert_eq!(today.to - today.from, 86_400);
        let yesterday = q("screenshot I took yesterday").range.unwrap();
        assert_eq!(yesterday.to, today.from);
        let week = q("files this week").range.unwrap();
        assert_eq!((today.from - week.from) / 86_400, 2, "Monday to Wednesday");
        assert_eq!(week.to, today.to);
        let last = q("pdfs last week").range.unwrap();
        assert_eq!(last.to, week.from);
        let march = q("photos from march").range.unwrap();
        assert_eq!(march.label, "March 2026");
        let december = q("videos in december").range.unwrap();
        assert_eq!(december.label, "December 2025", "the most recent December");
        assert_eq!(q("invoices from 2025").range.unwrap().label, "2025");
        assert_eq!(q("files from sep").range.unwrap().label, "September 2026");
        assert_eq!(q("may notes").range, None, "a leading 'may' is not a month");
        assert_eq!(q("notes from may").range.unwrap().label, "May 2026");
    }

    #[test]
    fn terms_are_letters_and_digits_only() {
        let evil = q(r#"x" } } DELETE { ?s ?p ?o }"#);
        assert!(
            evil.terms
                .iter()
                .all(|t| t.chars().all(char::is_alphanumeric)),
            "{:?}",
            evil.terms
        );
        assert_eq!(q("wi-fi notes").terms, ["wi", "fi", "notes"]);
        assert_eq!(q("pdfs").fts(), None);
        assert!(q("the").is_empty());
    }
}
