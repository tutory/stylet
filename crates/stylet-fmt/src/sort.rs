//! Property order for the optional sort: `all` first, custom properties next
//! (in source order), then alphabetically by family, with every shorthand
//! before the longhands it sets and vendor-prefixed before unprefixed.

/// Longhands whose name doesn't start with their shorthand: (property, family, depth).
const EXCEPTIONS: &[(&str, &str, usize)] = &[
    ("line-height", "font", 1),
    ("top", "inset", 1),
    ("right", "inset", 1),
    ("bottom", "inset", 1),
    ("left", "inset", 1),
    ("row-gap", "gap", 1),
    ("column-gap", "gap", 1),
    ("place-content", "place", 0),
    ("place-items", "place", 0),
    ("place-self", "place", 0),
    ("align-content", "place", 1),
    ("align-items", "place", 1),
    ("align-self", "place", 1),
    ("justify-content", "place", 1),
    ("justify-items", "place", 1),
    ("justify-self", "place", 1),
    ("flex-direction", "flex", 2),
    ("flex-wrap", "flex", 2),
    ("column-count", "columns", 1),
    ("column-width", "columns", 1),
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortKey {
    class: u8,
    family: String,
    depth: usize,
    name: String,
    /// Prefixed (0) before unprefixed (1).
    vendor: u8,
}

/// Sort key of a property. Equal keys (repeated properties, custom properties)
/// keep their order because the sort is stable.
pub fn key(property: &str) -> SortKey {
    let lower = property.to_ascii_lowercase();
    if lower.starts_with("--") {
        return class_key(1);
    }
    if lower == "all" {
        return class_key(0);
    }
    let (name, vendor) = match strip_vendor(&lower) {
        Some(name) => (name, 0),
        None => (lower.as_str(), 1),
    };
    let (family, depth) = match EXCEPTIONS.iter().find(|(p, _, _)| *p == name) {
        Some((_, family, depth)) => (family.to_string(), *depth),
        None => {
            let mut segments = name.split('-');
            let family = segments.next().unwrap_or_default().to_string();
            (family, segments.count())
        }
    };
    SortKey {
        class: 2,
        family,
        depth,
        name: name.to_string(),
        vendor,
    }
}

fn class_key(class: u8) -> SortKey {
    SortKey {
        class,
        family: String::new(),
        depth: 0,
        name: String::new(),
        vendor: 0,
    }
}

fn strip_vendor(name: &str) -> Option<&str> {
    let rest = name.strip_prefix('-')?;
    let dash = rest.find('-')?;
    Some(&rest[dash + 1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(props: &[&str]) -> Vec<String> {
        let mut props: Vec<_> = props.iter().map(|p| (key(p), *p)).collect();
        props.sort_by(|a, b| a.0.cmp(&b.0));
        props.into_iter().map(|(_, p)| p.to_string()).collect()
    }

    #[test]
    fn orders() {
        assert_eq!(
            sorted(&[
                "margin-top",
                "color",
                "--b",
                "margin",
                "display",
                "--a",
                "all",
                "display",
                "-webkit-transition",
                "transition",
                "line-height",
                "font",
                "border-top-color",
                "border-color",
                "border",
                "top",
                "inset",
                "flex-direction",
                "flex-flow",
            ]),
            [
                "all",
                "--b",
                "--a",
                "border",
                "border-color",
                "border-top-color",
                "color",
                "display",
                "display",
                "flex-flow",
                "flex-direction",
                "font",
                "line-height",
                "inset",
                "top",
                "margin",
                "margin-top",
                "-webkit-transition",
                "transition",
            ]
        );
    }
}
