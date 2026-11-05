//! Numbers as people read them, and as they paste them.

/// Ten significant digits: enough for any answer worth reading, and it turns
/// `sqrt(2)^2 = 2.0000000000000004` back into `2`.
fn rounded(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    format!("{x:.9e}").parse().unwrap_or(x)
}

fn group_thousands(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// For display: `1,048,576`, `93.2`, `0.000123`, `1.5e21`.
pub fn number(x: f64) -> String {
    let x = rounded(x);
    let text = plain_of_rounded(x);
    if text.contains('e') {
        return text;
    }
    let (sign, rest) = match text.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", text.as_str()),
    };
    match rest.split_once('.') {
        Some((int, frac)) => format!("{sign}{}.{frac}", group_thousands(int)),
        None => format!("{sign}{}", group_thousands(rest)),
    }
}

/// For the clipboard: `1048576`, `93.2`, `1.5e21`.
pub fn plain(x: f64) -> String {
    plain_of_rounded(rounded(x))
}

fn plain_of_rounded(x: f64) -> String {
    let x = if x == 0.0 { 0.0 } else { x }; // no "-0"
    let magnitude = x.abs();
    if x == 0.0 || (1e-6..1e15).contains(&magnitude) {
        // Rust prints the shortest representation that round-trips, and
        // never uses an exponent for Display.
        format!("{x}")
    } else {
        format!("{x:e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display() {
        for (x, want) in [
            (714.0, "714"),
            (1048576.0, "1,048,576"),
            (2500.0, "2,500"),
            (-1234.5, "-1,234.5"),
            (93.2, "93.2"),
            (93.20000000000002, "93.2"),
            (2.0000000000000004, "2"),
            (0.30000000000000004, "0.3"),
            (1.0 / 3.0, "0.3333333333"),
            (0.000123, "0.000123"),
            (1.5e21, "1.5e21"),
            (1.5e-9, "1.5e-9"),
            (0.0, "0"),
            (-0.0, "0"),
            (999.0, "999"),
            (1000.0, "1,000"),
        ] {
            assert_eq!(number(x), want, "{x}");
        }
    }

    #[test]
    fn plain_has_no_separators() {
        assert_eq!(plain(1048576.0), "1048576");
        assert_eq!(plain(2.0000000000000004), "2");
        assert_eq!(plain(-0.0), "0");
    }
}
