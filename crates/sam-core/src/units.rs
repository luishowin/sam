//! Unit conversions typed into the panel (§7): `2.5 GB to MB`,
//! `convert 34°C to fahrenheit`, `10 km in miles`.
//!
//! Units match case-insensitively, so `mb` is a megabyte; bits are spelled
//! out (`mbit`, `megabits`). Data units are decimal (`MB` = 10⁶ bytes) unless
//! written binary (`MiB`).

use crate::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Length,
    Mass,
    Temperature,
    Data,
    Time,
    Speed,
    Volume,
    Area,
}

impl Dimension {
    pub fn name(self) -> &'static str {
        match self {
            Dimension::Length => "length",
            Dimension::Mass => "mass",
            Dimension::Temperature => "temperature",
            Dimension::Data => "data",
            Dimension::Time => "time",
            Dimension::Speed => "speed",
            Dimension::Volume => "volume",
            Dimension::Area => "area",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Unit {
    pub symbol: &'static str,
    pub dimension: Dimension,
    /// base = (value + offset) × factor
    factor: f64,
    offset: f64,
    aliases: &'static [&'static str],
}

impl Unit {
    fn base_of(&self, v: f64) -> f64 {
        (v + self.offset) * self.factor
    }

    fn value_of(&self, b: f64) -> f64 {
        b / self.factor - self.offset
    }
}

const fn unit(
    symbol: &'static str,
    dimension: Dimension,
    factor: f64,
    aliases: &'static [&'static str],
) -> Unit {
    Unit {
        symbol,
        dimension,
        factor,
        offset: 0.0,
        aliases,
    }
}

use Dimension::*;

static UNITS: &[Unit] = &[
    // Length, in metres.
    unit(
        "nm",
        Length,
        1e-9,
        &["nm", "nanometer", "nanometers", "nanometre", "nanometres"],
    ),
    unit(
        "µm",
        Length,
        1e-6,
        &["µm", "um", "micrometer", "micrometers", "micron", "microns"],
    ),
    unit(
        "mm",
        Length,
        1e-3,
        &[
            "mm",
            "millimeter",
            "millimeters",
            "millimetre",
            "millimetres",
        ],
    ),
    unit(
        "cm",
        Length,
        1e-2,
        &[
            "cm",
            "centimeter",
            "centimeters",
            "centimetre",
            "centimetres",
        ],
    ),
    unit(
        "m",
        Length,
        1.0,
        &["m", "meter", "meters", "metre", "metres"],
    ),
    unit(
        "km",
        Length,
        1e3,
        &[
            "km",
            "kilometer",
            "kilometers",
            "kilometre",
            "kilometres",
            "kms",
        ],
    ),
    unit("in", Length, 0.0254, &["in", "inch", "inches", "\""]),
    unit("ft", Length, 0.3048, &["ft", "foot", "feet", "'"]),
    unit("yd", Length, 0.9144, &["yd", "yard", "yards", "yds"]),
    unit("mi", Length, 1609.344, &["mi", "mile", "miles"]),
    unit(
        "nmi",
        Length,
        1852.0,
        &["nmi", "nautical mile", "nautical miles"],
    ),
    // Mass, in kilograms.
    unit("mg", Mass, 1e-6, &["mg", "milligram", "milligrams"]),
    unit("g", Mass, 1e-3, &["g", "gram", "grams", "gr"]),
    unit(
        "kg",
        Mass,
        1.0,
        &["kg", "kilogram", "kilograms", "kilo", "kilos", "kgs"],
    ),
    unit(
        "t",
        Mass,
        1e3,
        &["t", "tonne", "tonnes", "metric ton", "metric tons"],
    ),
    unit("oz", Mass, 0.028349523125, &["oz", "ounce", "ounces"]),
    unit("lb", Mass, 0.45359237, &["lb", "lbs", "pound", "pounds"]),
    unit("st", Mass, 6.35029318, &["st", "stone", "stones"]),
    // Temperature, in kelvin.
    Unit {
        symbol: "°C",
        dimension: Temperature,
        factor: 1.0,
        offset: 273.15,
        aliases: &[
            "c",
            "°c",
            "celsius",
            "centigrade",
            "degc",
            "degrees c",
            "degrees celsius",
            "degree celsius",
        ],
    },
    Unit {
        symbol: "°F",
        dimension: Temperature,
        factor: 5.0 / 9.0,
        offset: 459.67,
        aliases: &[
            "f",
            "°f",
            "fahrenheit",
            "degf",
            "degrees f",
            "degrees fahrenheit",
            "degree fahrenheit",
        ],
    },
    unit("K", Temperature, 1.0, &["k", "kelvin", "kelvins"]),
    // Data, in bytes.
    unit("bit", Data, 0.125, &["bit", "bits"]),
    unit("B", Data, 1.0, &["b", "byte", "bytes"]),
    unit("kB", Data, 1e3, &["kb", "kilobyte", "kilobytes"]),
    unit("MB", Data, 1e6, &["mb", "megabyte", "megabytes"]),
    unit(
        "GB",
        Data,
        1e9,
        &["gb", "gigabyte", "gigabytes", "gig", "gigs"],
    ),
    unit("TB", Data, 1e12, &["tb", "terabyte", "terabytes"]),
    unit("PB", Data, 1e15, &["pb", "petabyte", "petabytes"]),
    unit("KiB", Data, 1024.0, &["kib", "kibibyte", "kibibytes"]),
    unit("MiB", Data, 1_048_576.0, &["mib", "mebibyte", "mebibytes"]),
    unit(
        "GiB",
        Data,
        1_073_741_824.0,
        &["gib", "gibibyte", "gibibytes"],
    ),
    unit(
        "TiB",
        Data,
        1_099_511_627_776.0,
        &["tib", "tebibyte", "tebibytes"],
    ),
    unit("kbit", Data, 125.0, &["kbit", "kilobit", "kilobits"]),
    unit("Mbit", Data, 125_000.0, &["mbit", "megabit", "megabits"]),
    unit(
        "Gbit",
        Data,
        125_000_000.0,
        &["gbit", "gigabit", "gigabits"],
    ),
    // Time, in seconds.
    unit("ms", Time, 1e-3, &["ms", "millisecond", "milliseconds"]),
    unit("s", Time, 1.0, &["s", "sec", "secs", "second", "seconds"]),
    unit("min", Time, 60.0, &["min", "mins", "minute", "minutes"]),
    unit("h", Time, 3600.0, &["h", "hr", "hrs", "hour", "hours"]),
    unit("d", Time, 86_400.0, &["d", "day", "days"]),
    unit("wk", Time, 604_800.0, &["wk", "wks", "week", "weeks"]),
    unit("mo", Time, 2_629_746.0, &["mo", "month", "months"]),
    unit("yr", Time, 31_556_952.0, &["yr", "yrs", "year", "years"]),
    // Speed, in metres per second.
    unit(
        "m/s",
        Speed,
        1.0,
        &["m/s", "mps", "meters per second", "metres per second"],
    ),
    unit(
        "km/h",
        Speed,
        1.0 / 3.6,
        &[
            "km/h",
            "kmh",
            "kph",
            "kilometers per hour",
            "kilometres per hour",
        ],
    ),
    unit("mph", Speed, 0.44704, &["mph", "miles per hour"]),
    unit("kn", Speed, 1852.0 / 3600.0, &["kn", "kt", "knot", "knots"]),
    unit("ft/s", Speed, 0.3048, &["ft/s", "fps", "feet per second"]),
    // Volume, in litres.
    unit(
        "mL",
        Volume,
        1e-3,
        &[
            "ml",
            "milliliter",
            "milliliters",
            "millilitre",
            "millilitres",
        ],
    ),
    unit(
        "cL",
        Volume,
        1e-2,
        &[
            "cl",
            "centiliter",
            "centiliters",
            "centilitre",
            "centilitres",
        ],
    ),
    unit(
        "L",
        Volume,
        1.0,
        &["l", "liter", "liters", "litre", "litres", "ltr"],
    ),
    unit(
        "m³",
        Volume,
        1e3,
        &[
            "m3",
            "m³",
            "cubic meter",
            "cubic meters",
            "cubic metre",
            "cubic metres",
        ],
    ),
    unit(
        "gal",
        Volume,
        3.785411784,
        &["gal", "gallon", "gallons", "us gallon", "us gallons"],
    ),
    unit("qt", Volume, 0.946352946, &["qt", "quart", "quarts"]),
    unit("pt", Volume, 0.473176473, &["pt", "pint", "pints"]),
    unit("cup", Volume, 0.2365882365, &["cup", "cups"]),
    unit(
        "fl oz",
        Volume,
        0.0295735295625,
        &["fl oz", "floz", "fluid ounce", "fluid ounces"],
    ),
    unit(
        "tbsp",
        Volume,
        0.01478676478125,
        &["tbsp", "tablespoon", "tablespoons"],
    ),
    unit(
        "tsp",
        Volume,
        0.00492892159375,
        &["tsp", "teaspoon", "teaspoons"],
    ),
    // Area, in square metres.
    unit(
        "cm²",
        Area,
        1e-4,
        &[
            "cm2",
            "cm²",
            "square centimeter",
            "square centimeters",
            "sq cm",
        ],
    ),
    unit(
        "m²",
        Area,
        1.0,
        &[
            "m2",
            "m²",
            "sqm",
            "sq m",
            "square meter",
            "square meters",
            "square metre",
            "square metres",
        ],
    ),
    unit(
        "km²",
        Area,
        1e6,
        &[
            "km2",
            "km²",
            "sq km",
            "square kilometer",
            "square kilometers",
            "square kilometre",
            "square kilometres",
        ],
    ),
    unit("ha", Area, 1e4, &["ha", "hectare", "hectares"]),
    unit("ac", Area, 4046.8564224, &["ac", "acre", "acres"]),
    unit(
        "ft²",
        Area,
        0.09290304,
        &["ft2", "ft²", "sqft", "sq ft", "square foot", "square feet"],
    ),
    unit(
        "in²",
        Area,
        0.00064516,
        &["in2", "in²", "sq in", "square inch", "square inches"],
    ),
    unit(
        "mi²",
        Area,
        2_589_988.110336,
        &["mi2", "mi²", "sq mi", "square mile", "square miles"],
    ),
];

fn lookup(phrase: &str) -> Option<&'static Unit> {
    let mut p = phrase.trim().to_lowercase();
    // "degrees celsius", "°c", "deg f" — the degree word is noise except
    // in the aliases that spell it.
    for noise in ["degrees ", "degree ", "deg "] {
        if let Some(rest) = p.strip_prefix(noise)
            && !UNITS.iter().any(|u| u.aliases.contains(&p.as_str()))
        {
            p = rest.to_owned();
        }
    }
    let p = p.split_whitespace().collect::<Vec<_>>().join(" ");
    UNITS.iter().find(|u| u.aliases.contains(&p.as_str()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub value: f64,
    pub from: &'static Unit,
    pub to: &'static Unit,
    pub result: f64,
}

impl Conversion {
    /// `2,500 MB`, `93.2 °F`.
    pub fn display(&self) -> String {
        format!("{} {}", format::number(self.result), self.to.symbol)
    }

    /// `2.5 GB → MB`.
    pub fn expression(&self) -> String {
        format!(
            "{} {} → {}",
            format::number(self.value),
            self.from.symbol,
            self.to.symbol
        )
    }

    /// What Enter copies: `2500`.
    pub fn plain(&self) -> String {
        format::plain(self.result)
    }
}

/// A number at the start of `s`, allowing `1,000` thousands separators; the
/// rest of `s` after it.
fn leading_number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim_start();
    let (sign, body) = match s.strip_prefix('-').or_else(|| s.strip_prefix('−')) {
        Some(rest) => (-1.0, rest),
        None => (1.0, s),
    };
    let end = body
        .char_indices()
        .find(|&(_, c)| !(c.is_ascii_digit() || c == '.' || c == ','))
        .map_or(body.len(), |(i, _)| i);
    let raw = body[..end].trim_end_matches([',', '.']);
    if raw.is_empty() || !raw.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return None;
    }
    // Commas only as thousands separators: 1,000 and 12,345.6, not 1,5.
    if raw.contains(',') {
        let int = raw.split('.').next().unwrap_or("");
        let groups: Vec<&str> = int.split(',').collect();
        let ok = !groups[0].is_empty()
            && groups[0].len() <= 3
            && groups[1..].iter().all(|g| g.len() == 3);
        if !ok {
            return None;
        }
    }
    let n: f64 = raw.replace(',', "").parse().ok()?;
    Some((sign * n, &body[raw.len()..]))
}

const CONNECTORS: &[&str] = &[" to ", " in ", " as ", " into ", " = ", " -> ", " → "];

/// The conversion `input` asks for, if it is one.
pub fn convert(input: &str) -> Option<Conversion> {
    let lowered = input.trim().to_lowercase();
    let text = lowered.strip_prefix("convert ").unwrap_or(&lowered);
    // The last connector splits, so "1 in to cm" means inches → centimetres.
    let (at, len) = CONNECTORS
        .iter()
        .filter_map(|c| text.rfind(c).map(|i| (i, c.len())))
        .max_by_key(|&(i, _)| i)?;
    let (left, right) = (&text[..at], &text[at + len..]);
    let (value, unit_text) = leading_number(left)?;
    let from = lookup(unit_text)?;
    let to = lookup(right)?;
    if from.dimension != to.dimension {
        return None;
    }
    let result = to.value_of(from.base_of(value));
    result.is_finite().then_some(Conversion {
        value,
        from,
        to,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(input: &str) -> Option<String> {
        convert(input).map(|c| c.display())
    }

    #[test]
    fn conversions() {
        for (input, want) in [
            ("2.5 GB to MB", "2,500 MB"),
            ("2.5GB to MB", "2,500 MB"),
            ("1 GiB in MiB", "1,024 MiB"),
            ("1 GB to MiB", "953.6743164 MiB"),
            ("100 mbit to mb", "12.5 MB"),
            ("convert 34°C to fahrenheit", "93.2 °F"),
            ("34 degrees celsius in fahrenheit", "93.2 °F"),
            ("-40 f to c", "-40 °C"),
            ("0 k to c", "-273.15 °C"),
            ("10 km in miles", "6.213711922 mi"),
            ("1 mile to km", "1.609344 km"),
            ("6 ft to cm", "182.88 cm"),
            ("1,000 m to km", "1 km"),
            ("70 kg in lb", "154.3235835 lb"),
            ("90 min to h", "1.5 h"),
            ("2 weeks to days", "14 d"),
            ("100 km/h to mph", "62.13711922 mph"),
            ("1 gallon to liters", "3.785411784 L"),
            ("1 acre to m2", "4,046.856422 m²"),
            ("1 in to cm", "2.54 cm"),
            ("5 kg as g", "5,000 g"),
        ] {
            assert_eq!(show(input).as_deref(), Some(want), "{input}");
        }
    }

    #[test]
    fn not_conversions() {
        for input in [
            "",
            "2.5 GB",
            "GB to MB",
            "2.5 GB to kg",
            "go to downloads",
            "lock my computer",
            "open photos in photodesk",
            "1,5 m to cm",
            "34 c to banana",
            "5 to 6",
        ] {
            assert_eq!(show(input), None, "{input:?}");
        }
    }

    #[test]
    fn expression_and_plain() {
        let c = convert("2.5 GB to MB").unwrap();
        assert_eq!(c.expression(), "2.5 GB → MB");
        assert_eq!(c.plain(), "2500");
        assert_eq!(c.from.dimension.name(), "data");
        assert_eq!(convert("34 c to f").unwrap().expression(), "34 °C → °F");
    }

    #[test]
    fn aliases_are_unique() {
        let mut seen = std::collections::HashMap::new();
        for u in UNITS {
            for a in u.aliases {
                assert!(seen.insert(*a, u.symbol).is_none(), "{a:?} names two units");
                assert_eq!(*a, a.to_lowercase(), "aliases are lowercase");
            }
        }
    }
}
