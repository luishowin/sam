//! Turning what someone typed into words the grammar can match (§6).

/// Words that carry politeness or grammar but no meaning for a command.
pub const FILLERS: &[&str] = &[
    "please", "pls", "plz", "can", "could", "would", "will", "you", "me", "my", "the", "a", "an",
    "hey", "hi", "sam", "just", "i", "want", "like", "kindly", "quickly", "some", "our", "your",
];

fn expand(word: &str) -> Vec<String> {
    let w = word;
    let pieces: &[&str] = match w {
        "what's" | "whats" => &["what", "is"],
        "where's" | "wheres" => &["where", "is"],
        "let's" | "lets" => &["let", "us"],
        "i'm" | "im" => &["i", "am"],
        "i'd" => &["i", "would"],
        "i'll" => &["i", "will"],
        "it's" => &["it", "is"],
        "don't" | "dont" => &["do", "not"],
        "can't" | "cant" => &["can", "not"],
        "won't" | "wont" => &["will", "not"],
        "turn-on" => &["turn", "on"],
        "turn-off" => &["turn", "off"],
        _ => {
            // A possessive carries nothing: "today's" → "today".
            let base = w.strip_suffix("'s").unwrap_or(w);
            return vec![base.to_owned()];
        }
    };
    pieces.iter().map(|p| (*p).to_owned()).collect()
}

/// Lowercased words with punctuation trimmed from their ends, contractions
/// expanded, and nothing removed. `50%` and `wi-fi` survive.
pub fn words(input: &str) -> Vec<String> {
    let lowered = input.to_lowercase().replace(['’', '‘', '`'], "'");
    lowered
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '%' && c != '\''))
        .map(|w| w.trim_matches('\''))
        .filter(|w| !w.is_empty())
        .flat_map(expand)
        .filter(|w| !w.is_empty())
        .collect()
}

/// [`words`] without fillers, a leading "to", or a trailing "now" — what the
/// grammar matches against.
pub fn normalize(input: &str) -> Vec<String> {
    let mut out: Vec<String> = words(input)
        .into_iter()
        .filter(|w| !FILLERS.contains(&w.as_str()))
        .collect();
    while out.first().is_some_and(|w| w == "to") {
        out.remove(0);
    }
    if out.len() > 1 && out.last().is_some_and(|w| w == "now") {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> String {
        normalize(s).join(" ")
    }

    #[test]
    fn normalizes_natural_phrasing() {
        assert_eq!(n("Lock my computer"), "lock computer");
        assert_eq!(n("Can you please lock the computer?"), "lock computer");
        assert_eq!(n("show me today’s calendar"), "show today calendar");
        assert_eq!(n("open my Downloads"), "open downloads");
        assert_eq!(n("Turn off Bluetooth."), "turn off bluetooth");
        assert_eq!(n("I want to open a terminal"), "open terminal");
        assert_eq!(n("I'd like to suspend now"), "suspend");
        assert_eq!(n("what's on today"), "what is on today");
        assert_eq!(n("set volume to 50%"), "set volume to 50%");
        assert_eq!(n("turn wi-fi off"), "turn wi-fi off");
        assert_eq!(n("find the PDF about invoices"), "find pdf about invoices");
    }

    #[test]
    fn keeps_what_it_cannot_judge() {
        assert_eq!(n("now"), "now", "a lone 'now' stays");
        assert_eq!(n("to"), "", "a lone 'to' is only filler");
        assert_eq!(n("   "), "");
        assert_eq!(n("!!!"), "");
    }
}
