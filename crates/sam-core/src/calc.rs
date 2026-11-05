//! Arithmetic typed into the panel (§7): `17 * 42`, `sqrt(2)^2`, `2pi`,
//! `15% * 200`, `5!`. A bare number is not a calculation.

use crate::format;

#[derive(Debug, Clone, PartialEq)]
pub struct Calculation {
    /// The expression as typed, respaced: `17 × 42`.
    pub expression: String,
    pub value: f64,
}

impl Calculation {
    pub fn display(&self) -> String {
        format::number(self.value)
    }

    pub fn plain(&self) -> String {
        format::plain(self.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Num(f64),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    Bang,
    Open,
    Close,
    Const(f64),
    Func(Func),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Func {
    Sqrt,
    Cbrt,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Ln,
    Log,
    Log2,
    Exp,
    Abs,
    Round,
    Floor,
    Ceil,
}

impl Func {
    fn named(name: &str) -> Option<Func> {
        Some(match name {
            "sqrt" | "√" => Func::Sqrt,
            "cbrt" => Func::Cbrt,
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "asin" | "arcsin" => Func::Asin,
            "acos" | "arccos" => Func::Acos,
            "atan" | "arctan" => Func::Atan,
            "ln" => Func::Ln,
            "log" | "log10" => Func::Log,
            "log2" => Func::Log2,
            "exp" => Func::Exp,
            "abs" => Func::Abs,
            "round" => Func::Round,
            "floor" => Func::Floor,
            "ceil" => Func::Ceil,
            _ => return None,
        })
    }

    fn apply(self, x: f64) -> f64 {
        match self {
            Func::Sqrt => x.sqrt(),
            Func::Cbrt => x.cbrt(),
            Func::Sin => x.sin(),
            Func::Cos => x.cos(),
            Func::Tan => x.tan(),
            Func::Asin => x.asin(),
            Func::Acos => x.acos(),
            Func::Atan => x.atan(),
            Func::Ln => x.ln(),
            Func::Log => x.log10(),
            Func::Log2 => x.log2(),
            Func::Exp => x.exp(),
            Func::Abs => x.abs(),
            Func::Round => x.round(),
            Func::Floor => x.floor(),
            Func::Ceil => x.ceil(),
        }
    }
}

/// Tokens, and the respaced text for display.
fn lex(input: &str) -> Option<(Vec<Tok>, String)> {
    let chars: Vec<char> = input.chars().collect();
    let mut toks = Vec::new();
    let mut shown = String::new();
    let mut i = 0;
    let push_op = |shown: &mut String, op: &str| {
        shown.push(' ');
        shown.push_str(op);
        shown.push(' ');
    };
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // An exponent only when digits follow, so `2e` stays 2·e.
            if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                let mut j = i + 1;
                if j < chars.len() && matches!(chars[j], '+' | '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    i = j;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text: String = chars[start..i].iter().collect();
            toks.push(Tok::Num(text.parse().ok()?));
            shown.push_str(&text);
            continue;
        }
        if c.is_alphabetic() || c == 'π' || c == 'τ' || c == '√' {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_alphanumeric())
                && c != '√'
                && c != 'π'
                && c != 'τ'
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
            let tok = match word.as_str() {
                "pi" | "π" => Tok::Const(std::f64::consts::PI),
                "tau" | "τ" => Tok::Const(std::f64::consts::TAU),
                "e" => Tok::Const(std::f64::consts::E),
                // `17 x 42`: x is multiplication between two operands.
                "x" if matches!(toks.last(), Some(Tok::Num(_) | Tok::Close | Tok::Const(_))) => {
                    push_op(&mut shown, "×");
                    toks.push(Tok::Star);
                    continue;
                }
                w => Tok::Func(Func::named(w)?),
            };
            shown.push_str(&word);
            toks.push(tok);
            continue;
        }
        let (tok, op) = match c {
            '+' => (Tok::Plus, "+"),
            '-' | '−' => (Tok::Minus, "−"),
            '*' if chars.get(i + 1) == Some(&'*') => {
                i += 1;
                (Tok::Caret, "^")
            }
            '*' | '×' | '·' => (Tok::Star, "×"),
            '/' | '÷' => (Tok::Slash, "÷"),
            '^' => (Tok::Caret, "^"),
            '%' => (Tok::Percent, "%"),
            '!' => (Tok::Bang, "!"),
            '(' => (Tok::Open, "("),
            ')' => (Tok::Close, ")"),
            _ => return None,
        };
        i += 1;
        match tok {
            Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash => push_op(&mut shown, op),
            _ => shown.push_str(op),
        }
        toks.push(tok);
    }
    let shown = shown.split_whitespace().collect::<Vec<_>>().join(" ");
    Some((toks, shown))
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<Tok> {
        self.toks.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<Tok> {
        self.toks.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.peek();
        self.pos += 1;
        t
    }

    fn starts_operand(t: Option<Tok>) -> bool {
        matches!(
            t,
            Some(Tok::Num(_) | Tok::Const(_) | Tok::Func(_) | Tok::Open | Tok::Minus | Tok::Plus)
        )
    }

    fn expr(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        loop {
            match self.peek() {
                Some(Tok::Plus) => {
                    self.bump();
                    value += self.term()?;
                }
                Some(Tok::Minus) => {
                    self.bump();
                    value -= self.term()?;
                }
                _ => return Some(value),
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        loop {
            match self.peek() {
                Some(Tok::Star) => {
                    self.bump();
                    value *= self.unary()?;
                }
                Some(Tok::Slash) => {
                    self.bump();
                    value /= self.unary()?;
                }
                Some(Tok::Percent) => {
                    self.bump();
                    value %= self.unary()?;
                }
                // `2pi`, `3(4)`, `2 sqrt 9` — but never `2 3`.
                Some(Tok::Const(_) | Tok::Func(_) | Tok::Open) => value *= self.unary()?,
                _ => return Some(value),
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.bump();
                Some(-self.unary()?)
            }
            Some(Tok::Plus) => {
                self.bump();
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.postfix()?;
        if self.peek() == Some(Tok::Caret) {
            self.bump();
            // Right-associative, and `2^-1` works.
            let exponent = self.unary()?;
            return Some(base.powf(exponent));
        }
        Some(base)
    }

    fn postfix(&mut self) -> Option<f64> {
        let mut value = self.primary()?;
        loop {
            match self.peek() {
                Some(Tok::Bang) => {
                    self.bump();
                    value = factorial(value)?;
                }
                // A percent sign followed by an operand is modulo (in `term`);
                // otherwise it means hundredths.
                Some(Tok::Percent) if !Self::starts_operand(self.peek_at(1)) => {
                    self.bump();
                    value /= 100.0;
                }
                _ => return Some(value),
            }
        }
    }

    fn primary(&mut self) -> Option<f64> {
        match self.bump()? {
            Tok::Num(n) | Tok::Const(n) => Some(n),
            Tok::Func(f) => Some(f.apply(self.postfix()?)),
            Tok::Open => {
                let value = self.expr()?;
                (self.bump()? == Tok::Close).then_some(value)
            }
            _ => None,
        }
    }
}

fn factorial(n: f64) -> Option<f64> {
    if n.fract() != 0.0 || !(0.0..=170.0).contains(&n) {
        return None;
    }
    Some((1..=n as u32).fold(1.0, |acc, k| acc * f64::from(k)))
}

/// The value of `input`, if the whole of it is arithmetic and more than a
/// lone number.
pub fn evaluate(input: &str) -> Option<Calculation> {
    let (toks, expression) = lex(input.trim())?;
    let does_something = toks
        .iter()
        .any(|t| !matches!(t, Tok::Num(_) | Tok::Open | Tok::Close));
    if !does_something {
        return None;
    }
    // A leading minus alone ("-5") is a number, not a sum.
    if toks.len() == 2 && toks[0] == Tok::Minus && matches!(toks[1], Tok::Num(_)) {
        return None;
    }
    let mut parser = Parser { toks, pos: 0 };
    let value = parser.expr()?;
    if parser.pos != parser.toks.len() || !value.is_finite() {
        return None;
    }
    Some(Calculation { expression, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(s: &str) -> Option<String> {
        evaluate(s).map(|c| c.display())
    }

    #[test]
    fn arithmetic() {
        for (input, want) in [
            ("17 * 42", "714"),
            ("17 x 42", "714"),
            ("17×42", "714"),
            ("2 + 2", "4"),
            ("2 + 3 * 4", "14"),
            ("(2 + 3) * 4", "20"),
            ("10 / 4", "2.5"),
            ("7 % 3", "1"),
            ("15% * 200", "30"),
            ("50%", "0.5"),
            ("2^10", "1,024"),
            ("2**10", "1,024"),
            ("2^3^2", "512"),
            ("-2^2", "-4"),
            ("2^-1", "0.5"),
            ("sqrt(2)^2", "2"),
            ("sqrt 16", "4"),
            ("√16", "4"),
            ("2pi", "6.283185307"),
            ("pi", "3.141592654"),
            ("e", "2.718281828"),
            ("3(4 + 1)", "15"),
            ("5!", "120"),
            ("0.1 + 0.2", "0.3"),
            ("1e3 + 1", "1,001"),
            ("ln(e)", "1"),
            ("log(1000)", "3"),
            ("abs(-3)", "3"),
            ("1 − 3", "-2"),
            ("1/3", "0.3333333333"),
        ] {
            assert_eq!(eval(input).as_deref(), Some(want), "{input}");
        }
    }

    #[test]
    fn not_calculations() {
        for input in [
            "",
            "714",
            "-5",
            "(5)",
            "firefox",
            "2 3",
            "1 / 0",
            "sqrt(-1)",
            "2 +",
            "(2",
            "2)",
            "1,000 * 3",
            "x",
            "17 x",
            "5.5!",
            "200!",
            "chrome",
            "e2e",
            "log",
            "vs code",
            "2.5 GB to MB",
        ] {
            assert_eq!(eval(input), None, "{input:?}");
        }
    }

    #[test]
    fn shows_the_expression_respaced() {
        assert_eq!(evaluate("17*42").unwrap().expression, "17 × 42");
        assert_eq!(evaluate("2^10").unwrap().expression, "2^10");
        assert_eq!(evaluate("sqrt(2)^2").unwrap().expression, "sqrt(2)^2");
        assert_eq!(evaluate("1048576/1").unwrap().plain(), "1048576");
    }
}
