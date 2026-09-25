//! Stylus values.

use crate::parse::Stmt;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    pub n: f64,
    pub unit: String,
    /// Source text, kept while the number is unmodified.
    pub text: Option<String>,
}

impl Number {
    pub fn new(n: f64, unit: impl Into<String>) -> Self {
        Self {
            n,
            unit: unit.into(),
            text: None,
        }
    }

    pub fn css(&self) -> String {
        if let Some(text) = &self.text {
            return text.clone();
        }
        let mut s = format!("{:.4}", self.n);
        if s.contains('.') {
            s.truncate(s.trim_end_matches('0').trim_end_matches('.').len());
        }
        if s == "-0" {
            s = "0".into();
        }
        s + &self.unit
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
    pub text: Option<String>,
}

impl Color {
    pub fn parse_hex(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#')?;
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digit =
            |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).ok().map(f64::from);
        let (r, g, b, a) = match hex.len() {
            3 | 4 => {
                let d = |i: usize| digit(i, 1).map(|v| v * 17.0);
                (
                    d(0)?,
                    d(1)?,
                    d(2)?,
                    if hex.len() == 4 { d(3)? / 255.0 } else { 1.0 },
                )
            }
            6 | 8 => (
                digit(0, 2)?,
                digit(2, 2)?,
                digit(4, 2)?,
                if hex.len() == 8 {
                    digit(6, 2)? / 255.0
                } else {
                    1.0
                },
            ),
            _ => return None,
        };
        Some(Self {
            r,
            g,
            b,
            a,
            text: Some(text.to_string()),
        })
    }

    pub fn css(&self) -> String {
        if let Some(text) = &self.text {
            return text.clone();
        }
        let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
        if (self.a - 1.0).abs() < 1e-9 {
            format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
        } else {
            format!(
                "rgba({}, {}, {}, {})",
                c(self.r),
                c(self.g),
                c(self.b),
                Number::new(self.a, "").css()
            )
        }
    }

    fn to_hsl(&self) -> (f64, f64, f64) {
        let (r, g, b) = (self.r / 255.0, self.g / 255.0, self.b / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if (max - min).abs() < 1e-12 {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h * 60.0, s, l)
    }

    fn from_hsl(h: f64, s: f64, l: f64, a: f64) -> Self {
        let h = h.rem_euclid(360.0) / 360.0;
        let hue = |p: f64, q: f64, mut t: f64| {
            if t < 0.0 {
                t += 1.0;
            }
            if t > 1.0 {
                t -= 1.0;
            }
            if t < 1.0 / 6.0 {
                p + (q - p) * 6.0 * t
            } else if t < 0.5 {
                q
            } else if t < 2.0 / 3.0 {
                p + (q - p) * (2.0 / 3.0 - t) * 6.0
            } else {
                p
            }
        };
        let (r, g, b) = if s == 0.0 {
            (l, l, l)
        } else {
            let q = if l < 0.5 {
                l * (1.0 + s)
            } else {
                l + s - l * s
            };
            let p = 2.0 * l - q;
            (
                hue(p, q, h + 1.0 / 3.0),
                hue(p, q, h),
                hue(p, q, h - 1.0 / 3.0),
            )
        };
        Self {
            r: r * 255.0,
            g: g * 255.0,
            b: b * 255.0,
            a,
            text: None,
        }
    }

    /// Stylus `lighten`/`darken`: moves lightness by `amount` percent of the
    /// remaining range (Stylus semantics for percentages).
    pub fn adjust_lightness(&self, amount: f64) -> Self {
        let (h, s, l) = self.to_hsl();
        let l = l * 100.0;
        let l = if amount >= 0.0 {
            l + (100.0 - l) * amount / 100.0
        } else {
            l + l * amount / 100.0
        };
        Self::from_hsl(h, s, (l / 100.0).clamp(0.0, 1.0), self.a)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    Color(Color),
    /// A string; `quote` is `None` for unquoted (literal) strings.
    Str {
        s: String,
        quote: Option<char>,
    },
    Ident(String),
    /// CSS text that is passed through.
    Raw(String),
    List {
        items: Vec<Value>,
        comma: bool,
    },
    Hash(Vec<(String, Value)>),
    /// A block assigned to a variable (`name =` + indented block).
    Block(Rc<Vec<Stmt>>),
    /// A value derived from global variables that become custom properties:
    /// `css` is the symbolic form (`var(--x)`, `calc(var(--x) * 2)`).
    Tracked {
        css: String,
        value: Box<Value>,
        /// The global variables it derives from.
        vars: Vec<String>,
    },
}

impl Value {
    pub fn literal(&self) -> &Value {
        match self {
            Self::Tracked { value, .. } => value.literal(),
            other => other,
        }
    }

    pub fn is_tracked(&self) -> bool {
        match self {
            Self::Tracked { .. } => true,
            Self::List { items, .. } => items.iter().any(Value::is_tracked),
            _ => false,
        }
    }

    pub fn truthy(&self) -> bool {
        match self.literal() {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Number(n) => n.n != 0.0,
            Self::Str { s, .. } => !s.is_empty(),
            Self::List { items, .. } => !items.is_empty() && items.iter().any(Value::truthy),
            Self::Hash(pairs) => !pairs.is_empty(),
            _ => true,
        }
    }

    /// CSS text. With `symbolic`, tracked values print their `var(--…)` form.
    pub fn css(&self, symbolic: bool) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(b) => b.to_string(),
            Self::Number(n) => n.css(),
            Self::Color(c) => c.css(),
            Self::Str { s, quote: Some(q) } => format!("{q}{s}{q}"),
            Self::Str { s, quote: None } => s.clone(),
            Self::Ident(s) | Self::Raw(s) => s.clone(),
            Self::List { items, comma } => {
                let sep = if *comma { ", " } else { " " };
                items
                    .iter()
                    .map(|v| v.css(symbolic))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(sep)
            }
            Self::Hash(pairs) => {
                let inner: Vec<_> = pairs
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.css(symbolic)))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Self::Block(_) => String::new(),
            Self::Tracked { css, value, .. } => {
                if symbolic {
                    css.clone()
                } else {
                    value.css(false)
                }
            }
        }
    }

    /// Text for interpolation and string building: strings without quotes.
    pub fn text(&self, symbolic: bool) -> String {
        match self {
            Self::Str { s, .. } => s.clone(),
            Self::Tracked { value, .. } if !symbolic => value.text(false),
            Self::List { items, comma } => {
                let sep = if *comma { ", " } else { " " };
                items
                    .iter()
                    .map(|v| v.text(symbolic))
                    .collect::<Vec<_>>()
                    .join(sep)
            }
            other => other.css(symbolic),
        }
    }

    /// Stylus equality on literal values.
    pub fn equals(&self, other: &Value) -> bool {
        match (self.literal(), other.literal()) {
            (Self::Number(a), Self::Number(b)) => {
                (a.n - b.n).abs() < 1e-9
                    && (a.unit == b.unit || a.unit.is_empty() || b.unit.is_empty())
            }
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Color(a), Self::Color(b)) => {
                a.css() == b.css() || (a.r, a.g, a.b, a.a) == (b.r, b.g, b.b, b.a)
            }
            (Self::List { items: a, .. }, Self::List { items: b, .. }) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.equals(y))
            }
            (a, b) if a.is_stringish() && b.is_stringish() => a.text(false) == b.text(false),
            _ => false,
        }
    }

    fn is_stringish(&self) -> bool {
        matches!(self, Self::Str { .. } | Self::Ident(_) | Self::Raw(_))
    }

    pub fn items(&self) -> Vec<Value> {
        match self {
            Self::List { items, .. } => items.clone(),
            Self::Tracked { value, .. } => value.items(),
            Self::Null => Vec::new(),
            other => vec![other.clone()],
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self.literal() {
            Self::Null => "null",
            Self::Bool(_) => "boolean",
            Self::Number(_) => "unit",
            Self::Color(_) => "rgba",
            Self::Str { .. } => "string",
            Self::Ident(_) => "ident",
            Self::Raw(_) => "string",
            Self::List { .. } => "expression",
            Self::Hash(_) => "object",
            Self::Block(_) => "block",
            Self::Tracked { .. } => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(Number::new(21.0, "px").css(), "21px");
        assert_eq!(Number::new(0.5, "").css(), "0.5");
        assert_eq!(Number::new(1.0 / 3.0, "em").css(), "0.3333em");
        assert_eq!(Number::new(-0.0, "").css(), "0");
    }

    #[test]
    fn colors() {
        let c = Color::parse_hex("#336699").unwrap();
        assert_eq!(c.adjust_lightness(10.0).css(), "#3b75b0");
        assert_eq!(Color::parse_hex("#fff").unwrap().r, 255.0);
        let mut white = Color::parse_hex("#fff").unwrap();
        white.a = 0.5;
        white.text = None;
        assert_eq!(white.css(), "rgba(255, 255, 255, 0.5)");
    }
}
