//! Operators and built-in functions.

use crate::eval::tracked_vars;
use crate::value::{Color, Number, Value};

/// CSS functions passed through without a warning.
const CSS_FUNCTIONS: &[&str] = &[
    "var",
    "calc",
    "min",
    "max",
    "clamp",
    "env",
    "attr",
    "rgb",
    "rgba",
    "hsl",
    "hsla",
    "hwb",
    "lab",
    "lch",
    "oklab",
    "oklch",
    "color",
    "color-mix",
    "light-dark",
    "linear-gradient",
    "radial-gradient",
    "conic-gradient",
    "repeating-linear-gradient",
    "repeating-radial-gradient",
    "repeating-conic-gradient",
    "url",
    "image-set",
    "cross-fade",
    "translate",
    "translatex",
    "translatey",
    "translatez",
    "translate3d",
    "rotate",
    "rotatex",
    "rotatey",
    "rotatez",
    "rotate3d",
    "scale",
    "scalex",
    "scaley",
    "scalez",
    "scale3d",
    "skew",
    "skewx",
    "skewy",
    "matrix",
    "matrix3d",
    "perspective",
    "cubic-bezier",
    "steps",
    "blur",
    "brightness",
    "contrast",
    "drop-shadow",
    "grayscale",
    "hue-rotate",
    "invert",
    "opacity",
    "saturate",
    "sepia",
    "minmax",
    "repeat",
    "fit-content",
    "counter",
    "counters",
    "format",
    "local",
    "symbols",
    "element",
    "path",
    "polygon",
    "circle",
    "ellipse",
    "inset",
    "rect",
    "xywh",
    "ray",
    "anchor",
    "anchor-size",
    "round",
    "mod",
    "rem",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "atan2",
    "pow",
    "sqrt",
    "hypot",
    "log",
    "exp",
    "abs",
    "sign",
    "tech",
    "supports",
    "layer",
    "selector",
    "view",
    "scroll",
    "leader",
    "target-counter",
    "target-text",
    "content",
    "string",
    "linear",
    "light",
    "dark",
    "-webkit-gradient",
    "-webkit-linear-gradient",
    "-webkit-radial-gradient",
    "-moz-linear-gradient",
    "-webkit-image-set",
    "-webkit-calc",
];

pub enum Result {
    Value(Value),
    /// A value plus a warning for the user.
    Warn(Value, String),
    /// Not a known function: passed through as CSS.
    Unknown(Value),
}

fn tracked(css: String, literal: Value, parts: &[&Value]) -> Value {
    let mut vars = Vec::new();
    for p in parts {
        tracked_vars(p, &mut vars);
    }
    if vars.is_empty() {
        literal
    } else {
        Value::Tracked {
            css,
            value: Box::new(literal),
            vars,
        }
    }
}

/// Builds a raw CSS value from a text template, keeping a symbolic form when
/// any part is tracked.
fn build(parts: &[&Value], f: impl Fn(bool) -> String) -> Value {
    tracked(f(true), Value::Raw(f(false)), parts)
}

pub fn unary(op: &str, v: &Value) -> Value {
    match (op, v.literal()) {
        ("!", _) => Value::Bool(!v.truthy()),
        ("+", _) => v.clone(),
        ("-", Value::Number(n)) => {
            let literal = Value::Number(Number::new(-n.n, n.unit.clone()));
            tracked(format!("calc(-1 * {})", v.css(true)), literal, &[v])
        }
        (_, _) => build(&[v], |s| format!("{op}{}", v.css(s))),
    }
}

pub fn binary(op: &str, l: &Value, r: &Value) -> Option<Value> {
    let (ll, rl) = (l.literal(), r.literal());
    Some(match op {
        "==" => Value::Bool(l.equals(r)),
        "!=" => Value::Bool(!l.equals(r)),
        "in" => Value::Bool(match rl {
            Value::Hash(pairs) => pairs.iter().any(|(k, _)| *k == l.text(false)),
            other => other.items().iter().any(|i| i.equals(l)),
        }),
        "<" | ">" | "<=" | ">=" => {
            let (Value::Number(a), Value::Number(b)) = (ll, rl) else {
                return None;
            };
            Value::Bool(match op {
                "<" => a.n < b.n,
                ">" => a.n > b.n,
                "<=" => a.n <= b.n,
                _ => a.n >= b.n,
            })
        }
        ".." | "..." => {
            let (Value::Number(a), Value::Number(b)) = (ll, rl) else {
                return None;
            };
            let end = if op == ".." { b.n } else { b.n - 1.0 };
            let mut items = Vec::new();
            let mut n = a.n;
            while n <= end + 1e-9 && items.len() < 10_000 {
                items.push(Value::Number(Number::new(n, a.unit.clone())));
                n += 1.0;
            }
            Value::List {
                items,
                comma: false,
            }
        }
        "+" if matches!(ll, Value::Str { .. }) => {
            let Value::Str { s, quote } = ll else {
                unreachable!()
            };
            let literal = Value::Str {
                s: format!("{s}{}", r.text(false)),
                quote: *quote,
            };
            let q = quote.map(String::from).unwrap_or_default();
            tracked(format!("{q}{s}{}{q}", r.text(true)), literal, &[l, r])
        }
        "%" if matches!(ll, Value::Str { .. }) => {
            let Value::Str { s: template, .. } = ll else {
                unreachable!()
            };
            let items = r.items();
            let format = |symbolic: bool| sprintf(template, &items, symbolic, |v, s| v.text(s));
            build(&[r], format)
        }
        "+" | "-" | "*" | "/" | "%" | "**" => {
            let (Value::Number(a), Value::Number(b)) = (ll, rl) else {
                return None;
            };
            let unit = if a.unit.is_empty() {
                b.unit.clone()
            } else {
                a.unit.clone()
            };
            let n = match op {
                "+" => a.n + b.n,
                "-" => a.n - b.n,
                "*" => a.n * b.n,
                "/" if b.n == 0.0 => return None,
                "/" => a.n / b.n,
                "%" => a.n % b.n,
                _ => a.n.powf(b.n),
            };
            let literal = Value::Number(Number::new(n, unit));
            if op == "%" || op == "**" {
                return Some(literal);
            }
            // `calc(12mm + 2)` is invalid CSS: a unitless addend takes the other unit.
            let side = |v: &Value, n: &Number, other: &Number| {
                let plain = matches!(op, "+" | "-") && n.unit.is_empty() && !other.unit.is_empty();
                if plain && !v.is_tracked() {
                    format!("{}{}", Number::new(n.n, "").css(), other.unit)
                } else {
                    strip_calc(&v.css(true))
                }
            };
            tracked(
                format!("calc({} {op} {})", side(l, a, b), side(r, b, a)),
                literal,
                &[l, r],
            )
        }
        _ => return None,
    })
}

/// `calc(x)` → `(x)` when nested in another `calc()`.
fn strip_calc(css: &str) -> String {
    match css.strip_prefix("calc(") {
        Some(inner) => format!("({inner}"),
        None => css.to_string(),
    }
}

fn sprintf(
    template: &str,
    args: &[Value],
    symbolic: bool,
    show: impl Fn(&Value, bool) -> String,
) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("%s") {
        out += &rest[..i];
        if let Some(arg) = args.next() {
            out += &show(arg, symbolic);
        }
        rest = &rest[i + 2..];
    }
    out + rest
}

pub fn index(v: &Value, i: &Value) -> Value {
    match (v.literal(), i.literal()) {
        (Value::Hash(pairs), key) => {
            let key = key.text(false);
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map_or(Value::Null, |(_, v)| v.clone())
        }
        (list, Value::Number(n)) => {
            let items = list.items();
            let idx = if n.n < 0.0 {
                items.len() as f64 + n.n
            } else {
                n.n
            };
            items.get(idx as usize).cloned().unwrap_or(Value::Null)
        }
        _ => Value::Null,
    }
}

fn number(v: &Value) -> Option<&Number> {
    match v.literal() {
        Value::Number(n) => Some(n),
        _ => None,
    }
}

pub fn call(name: &str, args: &[(Option<String>, Value)]) -> Result {
    let values: Vec<&Value> = args.iter().map(|(_, v)| v).collect();
    let arg = |i: usize| values.get(i).copied().cloned().unwrap_or(Value::Null);
    let passthrough = || {
        let css = |symbolic: bool| {
            let inner: Vec<String> = values.iter().map(|v| v.css(symbolic)).collect();
            format!("{name}({})", inner.join(", "))
        };
        build(&values, css)
    };
    let lower = name.to_ascii_lowercase();
    let (a0, a1) = (arg(0), arg(1));
    Result::Value(match lower.as_str() {
        "s" => {
            let template = arg(0).text(false);
            let rest: Vec<Value> = values.iter().skip(1).map(|v| (*v).clone()).collect();
            build(&values, |symbolic| {
                sprintf(&template, &rest, symbolic, |v, s| v.css(s))
            })
        }
        "unquote" => {
            let v = arg(0);
            build(&[&v], |s| v.text(s))
        }
        "quote" => Value::Str {
            s: arg(0).text(false),
            quote: Some('\''),
        },
        "join" => {
            let delim = arg(0).text(false);
            let items: Vec<Value> = if values.len() == 2 {
                arg(1).items()
            } else {
                values.iter().skip(1).map(|v| (*v).clone()).collect()
            };
            let joined = |s: bool| {
                items
                    .iter()
                    .map(|v| v.text(s))
                    .collect::<Vec<_>>()
                    .join(&delim)
            };
            let refs: Vec<&Value> = items.iter().collect();
            match build(&refs, joined) {
                Value::Raw(s) => Value::Str {
                    s,
                    quote: Some('\''),
                },
                other => other,
            }
        }
        "length" => Value::Number(Number::new(
            match arg(0).literal() {
                Value::Hash(pairs) => pairs.len(),
                other => other.items().len(),
            } as f64,
            "",
        )),
        "type" | "typeof" | "type-of" => Value::Str {
            s: arg(0).type_name().into(),
            quote: Some('\''),
        },
        "unit" => match (number(&arg(0)), values.get(1)) {
            (Some(n), Some(unit)) => Value::Number(Number::new(n.n, unit.text(false))),
            (Some(n), None) => Value::Str {
                s: n.unit.clone(),
                quote: Some('\''),
            },
            _ => return Result::Warn(passthrough(), "`unit()` needs a number".into()),
        },
        "round" | "floor" | "ceil" if number(&arg(0)).is_some() && values.len() <= 2 => {
            let n = number(&a0).unwrap();
            let precision = number(&a1).map_or(0.0, |p| p.n);
            let m = 10f64.powf(precision);
            let v = match lower.as_str() {
                "round" => (n.n * m).round(),
                "floor" => (n.n * m).floor(),
                _ => (n.n * m).ceil(),
            } / m;
            Value::Number(Number::new(v, n.unit.clone()))
        }
        "percentage" if number(&arg(0)).is_some() => {
            Value::Number(Number::new(number(&arg(0)).unwrap().n * 100.0, "%"))
        }
        "lighten" | "darken" => {
            let (Value::Color(color), Some(amount)) = (a0.literal().clone(), number(&a1)) else {
                return Result::Warn(passthrough(), format!("`{name}()` needs a literal color"));
            };
            let amount = if lower == "darken" {
                -amount.n
            } else {
                amount.n
            };
            let result = Value::Color(color.adjust_lightness(amount));
            if arg(0).is_tracked() {
                return Result::Warn(
                    result,
                    format!("`{name}()` of a variable: the result no longer follows the variable"),
                );
            }
            result
        }
        "rgba" | "rgb" if values.len() == 2 && matches!(arg(0).literal(), Value::Color(_)) => {
            let (Value::Color(color), Some(alpha)) = (a0.literal().clone(), number(&a1)) else {
                return Result::Value(passthrough());
            };
            let alpha = if alpha.unit == "%" {
                alpha.n / 100.0
            } else {
                alpha.n
            };
            let result = Value::Color(Color {
                a: alpha,
                text: None,
                ..color
            });
            if arg(0).is_tracked() {
                return Result::Warn(
                    result,
                    format!("`{name}()` of a variable: the result no longer follows the variable"),
                );
            }
            result
        }
        // `rgba(color alpha)`: one space-separated argument.
        "rgba" | "rgb"
            if values.len() == 1
                && matches!(a0.literal(), Value::List { items, comma: false }
                    if items.len() == 2 && matches!(items[0].literal(), Value::Color(_))) =>
        {
            let items = a0.items();
            let args = [(None, items[0].clone()), (None, items[1].clone())];
            return call(name, &args);
        }
        "rgba"
            if values.len() == 2
                && matches!(a0.literal(), Value::List { items, comma: false } if items.len() == 3) =>
        {
            let channels: Vec<String> = a0.items().iter().map(|v| v.css(false)).collect();
            Value::Raw(format!("rgba({}, {})", channels.join(", "), a1.css(false)))
        }
        "alpha" => match arg(0).literal() {
            Value::Color(c) => Value::Number(Number::new(c.a, "")),
            _ => return Result::Warn(passthrough(), "`alpha()` needs a literal color".into()),
        },
        // webapp's `encodeSvg` plugin (without its svgo optimization): escapes
        // SVG markup for `url("data:image/svg+xml,…")`.
        "encodesvg" => {
            let svg = arg(0).text(false);
            let mut out = String::new();
            let mut space = false;
            for ch in svg.chars() {
                if ch.is_whitespace() {
                    space = true;
                    continue;
                }
                if space {
                    out.push(' ');
                    space = false;
                }
                match ch {
                    '"' => out.push('\''),
                    '%' => out += "%25",
                    '#' => out += "%23",
                    '{' => out += "%7B",
                    '}' => out += "%7D",
                    '<' => out += "%3C",
                    '>' => out += "%3E",
                    c => out.push(c),
                }
            }
            return Result::Warn(
                Value::Str {
                    s: out,
                    quote: None,
                },
                "`encodeSvg()` (JS plugin) was evaluated without svgo optimization".into(),
            );
        }
        "embedurl" => {
            let path = arg(0).text(false);
            Value::Raw(format!("url('{path}?inline')"))
        }
        "url" => build(&values, |s| format!("url({})", arg(0).css(s))),
        _ if CSS_FUNCTIONS.contains(&lower.as_str()) => passthrough(),
        _ => return Result::Unknown(passthrough()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: f64, unit: &str) -> Value {
        Value::Number(Number::new(n, unit))
    }

    fn tracked_num(name: &str, n: f64, unit: &str) -> Value {
        Value::Tracked {
            css: format!("var(--{name})"),
            value: Box::new(num(n, unit)),
            vars: vec![name.into()],
        }
    }

    #[test]
    fn arithmetic() {
        assert_eq!(
            binary("*", &num(10.0, "px"), &num(2.0, ""))
                .unwrap()
                .css(false),
            "20px"
        );
        assert_eq!(
            binary("+", &num(1.0, "em"), &num(2.0, "px"))
                .unwrap()
                .css(false),
            "3em"
        );
        let t = binary("*", &tracked_num("gap", 4.0, "px"), &num(2.0, "")).unwrap();
        assert_eq!(t.css(true), "calc(var(--gap) * 2)");
        assert_eq!(t.css(false), "8px");
        let nested = binary("+", &t, &num(1.0, "px")).unwrap();
        assert_eq!(nested.css(true), "calc((var(--gap) * 2) + 1px)");
    }

    #[test]
    fn strings() {
        let s = Value::Str {
            s: "a %s b".into(),
            quote: Some('"'),
        };
        assert_eq!(
            binary("%", &s, &num(3.0, "px")).unwrap().css(false),
            "a 3px b"
        );
        let Result::Value(v) = call(
            "s",
            &[
                (
                    None,
                    Value::Str {
                        s: "%s-%s".into(),
                        quote: Some('"'),
                    },
                ),
                (None, num(1.0, "px")),
                (
                    None,
                    Value::Str {
                        s: "a".into(),
                        quote: Some('"'),
                    },
                ),
            ],
        ) else {
            panic!()
        };
        assert_eq!(v.css(false), "1px-\"a\"");
    }

    #[test]
    fn colors() {
        let c = Value::Color(Color::parse_hex("#336699").unwrap());
        let Result::Value(v) = call("darken", &[(None, c.clone()), (None, num(10.0, "%"))]) else {
            panic!()
        };
        assert_eq!(v.css(false), "#2e5c8a");
        let Result::Value(v) = call("rgba", &[(None, c), (None, num(0.5, ""))]) else {
            panic!()
        };
        assert_eq!(v.css(false), "rgba(51, 102, 153, 0.5)");
    }
}
