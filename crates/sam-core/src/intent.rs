//! What the intelligence produces: inert descriptions of something the user
//! might want done. An `Intent` cannot do anything; it has to be validated by
//! the [`Registry`](crate::action::Registry) and approved by
//! [`policy`](crate::policy) first (§3, F1).

use std::collections::BTreeMap;
use std::fmt;

/// Where an intent came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A grammar pattern matched the whole input (§6).
    Grammar,
    /// A search provider built it for one of its results.
    Provider,
    /// A local action model proposed it (Needle, once the model tiers land).
    /// Still inert: validation and policy decide whether it may run.
    Model,
}

/// An argument value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
}

impl Value {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            Value::Number(_) => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Text(_) => None,
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Text(s.to_owned())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Text(s)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Text(s) => write!(f, "{s:?}"),
            Value::Number(n) => write!(f, "{n}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    /// An action ID from the registry, such as `app.launch`.
    pub action: String,
    pub args: BTreeMap<String, Value>,
    pub source: Source,
    /// How sure the producer is, 0–1. Advisory: the policy does not read it.
    pub confidence: f32,
}

impl Intent {
    pub fn new(action: impl Into<String>, source: Source) -> Self {
        Intent {
            action: action.into(),
            args: BTreeMap::new(),
            source,
            confidence: 1.0,
        }
    }

    /// Builder: sets an argument.
    pub fn arg(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.args.insert(name.to_owned(), value.into());
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        self.args.get(name).and_then(Value::as_text)
    }

    pub fn number(&self, name: &str) -> Option<f64> {
        self.args.get(name).and_then(Value::as_number)
    }
}

impl fmt::Display for Intent {
    /// `app.launch {app_id: "org.gnome.Calculator.desktop"}`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.action)?;
        if !self.args.is_empty() {
            f.write_str(" {")?;
            for (i, (k, v)) in self.args.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{k}: {v}")?;
            }
            f.write_str("}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_reads_arguments() {
        let intent = Intent::new("audio.volume_set", Source::Grammar)
            .arg("level", 40.0)
            .arg("note", "x");
        assert_eq!(intent.number("level"), Some(40.0));
        assert_eq!(intent.text("note"), Some("x"));
        assert_eq!(intent.text("level"), None, "a number is not text");
        assert_eq!(
            intent.to_string(),
            r#"audio.volume_set {level: 40, note: "x"}"#
        );
    }

    #[test]
    fn confidence_is_clamped() {
        assert_eq!(
            Intent::new("a.b", Source::Model)
                .with_confidence(3.0)
                .confidence,
            1.0
        );
    }
}
