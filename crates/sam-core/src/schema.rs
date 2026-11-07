//! The one machine-readable catalogue of what Core can do.
//!
//! The catalogue is generated from [`CORE_ACTIONS`](crate::action::CORE_ACTIONS),
//! never hand-written. That matters twice:
//!
//! - the fine-tuning teacher and the router see the same names, fields and
//!   bounds the runtime validates;
//! - a schema fingerprint can travel with a request, so a sidecar built for
//!   another catalogue is refused instead of silently mistranslated.
//!
//! `sam-core` has no JSON dependency on purpose (see `tests/purity.rs`), so
//! this module writes the small, stable subset of JSON Schema the router
//! needs. The output is deterministic: the same registry always produces the
//! same bytes and the same fingerprint.

use std::fmt::Write as _;

use crate::action::{ActionSpec, Kind, Registry};

/// The clarification pseudo-tool. It is not an [`ActionSpec`] because it
/// never reaches the executor: the router turns it into a question. It is in
/// the model catalogue so a missing argument has a first-class label.
const CLARIFY: &str = concat!(
    r#"{"name":"clarify","description":"Ask for one missing required argument instead of guessing.","#,
    r#""parameters":{"type":"object","properties":{"#,
    r#""slot":{"type":"string"},"question":{"type":"string"}},"#,
    r#""required":["slot"],"additionalProperties":false}}"#,
);

/// The action catalogue as one JSON array, without the `clarify` pseudo-tool.
pub fn tools_json(registry: &Registry) -> String {
    let refs: Vec<&ActionSpec> = registry.specs().iter().collect();
    tools_json_for(&refs, false)
}

/// One action's schema object.
pub fn tool_json(spec: &ActionSpec) -> String {
    let mut out = String::with_capacity(512);
    tool(&mut out, spec);
    out
}

/// A selected subset, in the order given. The router never sees the whole
/// catalogue; this is what a request carries.
pub fn tools_json_for(specs: &[&ActionSpec], clarify: bool) -> String {
    let mut out = String::with_capacity(8 * 1024);
    out.push('[');
    for (i, spec) in specs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        tool(&mut out, spec);
    }
    if clarify {
        if !specs.is_empty() {
            out.push(',');
        }
        out.push_str(CLARIFY);
    }
    out.push(']');
    out
}

/// The catalogue the router is given, including the `clarify` pseudo-tool.
pub fn model_tools_json(registry: &Registry) -> String {
    let refs: Vec<&ActionSpec> = registry.specs().iter().collect();
    tools_json_for(&refs, true)
}

fn tool(out: &mut String, spec: &ActionSpec) {
    out.push_str(r#"{"name":"#);
    string(out, spec.id);
    out.push_str(r#","description":"#);
    string(out, spec.description);
    out.push_str(r#","parameters":{"type":"object","properties":{"#);
    for (j, param) in spec.params.iter().enumerate() {
        if j > 0 {
            out.push(',');
        }
        string(out, param.name);
        out.push(':');
        kind(out, param.kind);
    }
    out.push_str(r#"},"required":["#);
    let mut first = true;
    for param in spec.params.iter().filter(|p| p.required) {
        if !first {
            out.push(',');
        }
        first = false;
        string(out, param.name);
    }
    out.push_str(r#"],"additionalProperties":false}"#);
    out.push('}');
}

/// A short, stable fingerprint of the exact model catalogue.
///
/// FNV-1a is deliberately simple: this is a compatibility check, not a
/// security primitive. The sidecar compares it with the catalogue it was
/// built from and refuses a mismatch.
pub fn fingerprint(registry: &Registry) -> String {
    let text = model_tools_json(registry);
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn kind(out: &mut String, kind: Kind) {
    match kind {
        Kind::Text | Kind::LongText => out.push_str(r#"{"type":"string"}"#),
        Kind::Uri(schemes) => {
            out.push_str(r#"{"type":"string","pattern":"^("#);
            for (i, scheme) in schemes.iter().enumerate() {
                if i > 0 {
                    out.push('|');
                }
                out.push_str(scheme);
            }
            out.push_str(r#"):"}"#);
        }
        Kind::AppId => {
            out.push_str(r#"{"type":"string","pattern":"^[A-Za-z0-9._-]+\\.desktop$"}"#)
        }
        Kind::Number { min, max } => {
            let _ = write!(out, r#"{{"type":"number","minimum":{min},"maximum":{max}}}"#);
        }
        Kind::OneOf(words) => {
            out.push_str(r#"{"type":"string","enum":["#);
            for (i, word) in words.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                string(out, word);
            }
            out.push_str("]}");
        }
    }
}

fn string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Registry;

    #[test]
    fn canonical_fields_are_exported() {
        let json = model_tools_json(&Registry::core());
        assert!(json.contains(r#""name":"app.launch""#));
        assert!(json.contains(r#""app_id""#));
        assert!(json.contains(r#""uri""#));
        assert!(json.contains(r#""temperature""#));
        assert!(json.contains(r#""name":"clarify""#));
        // The old hand-written spellings must never come back.
        assert!(!json.contains(r#""name":"path""#));
        assert!(!json.contains(r#""name":"kelvin""#));
        assert!(!json.contains(r#""name":"app""#));
    }

    #[test]
    fn fingerprint_is_stable_and_short() {
        let registry = Registry::core();
        let a = fingerprint(&registry);
        let b = fingerprint(&registry);
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn every_action_has_a_schema() {
        let registry = Registry::core();
        let json = tools_json(&registry);
        for spec in registry.specs() {
            assert!(
                json.contains(&format!(r#""name":"{}""#, spec.id)),
                "{} is missing from the catalogue",
                spec.id
            );
        }
    }
}
