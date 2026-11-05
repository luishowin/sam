//! Web shortcuts (§17): the table of sites that become `url.open` rows.
//!
//! `data/sites.toml` is compiled in, as the grammar is (§6). The table is
//! names and URLs and nothing else: SAM hands the URL to the browser and
//! never fetches anything itself (F5).

use serde::Deserialize;

const CORE_SITES: &str = include_str!("../../../data/sites.toml");

/// One site: what it is called, where it is, and what else to call it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    site: Vec<Site>,
}

/// The table SAM ships.
pub fn core() -> Vec<Site> {
    parse(CORE_SITES).expect("data/sites.toml is valid (tested)")
}

/// Reads a table, and checks every entry before anything can rank it.
pub fn parse(text: &str) -> Result<Vec<Site>, String> {
    let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
    for site in &file.site {
        if site.name.trim().is_empty() {
            return Err("a site has no name".into());
        }
        // The registry checks this too (`Kind::Uri`); failing here means the
        // table is wrong, rather than one row quietly never opening.
        if !site.url.starts_with("https://") {
            return Err(format!("{}: the URL is not https", site.name));
        }
        if let Some(keyword) = site.keywords.iter().find(|k| k.trim().is_empty()) {
            return Err(format!("{}: an empty keyword {keyword:?}", site.name));
        }
    }
    Ok(file.site)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Registry;
    use crate::intent::{Intent, Source};

    #[test]
    fn the_table_parses_and_every_row_validates() {
        let sites = core();
        assert!(sites.len() > 10, "{} sites", sites.len());
        for site in &sites {
            let intent = Intent::new("url.open", Source::Provider).arg("url", site.url.as_str());
            Registry::core()
                .validate(intent)
                .unwrap_or_else(|e| panic!("{}: {e}", site.name));
        }
    }

    #[test]
    fn names_are_unique_and_so_are_urls() {
        let sites = core();
        let mut names: Vec<_> = sites.iter().map(|s| s.name.to_lowercase()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), sites.len(), "two sites share a name");
        let mut urls: Vec<_> = sites.iter().map(|s| s.url.as_str()).collect();
        urls.sort_unstable();
        urls.dedup();
        assert_eq!(urls.len(), sites.len(), "two sites share a URL");
    }

    #[test]
    fn a_table_that_is_wrong_is_refused() {
        assert!(parse("[[site]]\nname = \"X\"\nurl = \"http://x.example\"\n").is_err());
        assert!(parse("[[site]]\nname = \" \"\nurl = \"https://x.example\"\n").is_err());
        assert!(parse("[[site]]\nname = \"X\"\nurl = \"https://x.example\"\nwhat = 1\n").is_err());
        assert!(parse("[[site]]\nname = \"X\"\nurl = \"https://x.example\"\n").is_ok());
    }
}
