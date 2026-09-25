//! Helpers for placeholders and `@extend`: resolving nested selectors and
//! patching the output once all extenders are known.

use crate::source_map::Mapping;
use std::ops::Range;

/// Splits a selector list at top-level commas.
pub fn split_list(selector: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut start = 0;
    let mut escaped = false;
    for (i, c) in selector.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, c) {
            (_, '\\') => escaped = true,
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                parts.push(selector[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(selector[start..].trim());
    parts
}

/// Replaces `&` outside strings with `parent`.
fn replace_amp(selector: &str, parent: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in selector.chars() {
        if escaped {
            escaped = false;
            out.push(c);
            continue;
        }
        match (quote, c) {
            (_, '\\') => escaped = true,
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '&') => {
                out += parent;
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
    out
}

fn is_complex(selector: &str) -> bool {
    selector.contains([' ', '>', '+', '~'])
}

/// The selector matched by `child` nested in `parent`, e.g.
/// `.a, .b` + `&:hover > .c` → `:is(.a, .b):hover > .c`.
pub fn nest(parent: &str, child: &str, list_separator: &str) -> String {
    let parents = split_list(parent);
    let resolved: Vec<String> = split_list(child)
        .into_iter()
        .map(|item| {
            let leading_amp = item.starts_with('&') && !item[1..].contains('&');
            let wrap = parents.len() > 1 || (!leading_amp && is_complex(parent));
            let parent = if wrap {
                format!(":is({parent})")
            } else {
                parent.to_string()
            };
            if item.contains('&') {
                replace_amp(item, &parent)
            } else {
                format!("{parent} {item}")
            }
        })
        .collect();
    resolved.join(list_separator)
}

/// Applies non-overlapping `edits` to `out`, moving or dropping source mappings.
pub fn apply_edits(
    out: &mut String,
    mappings: &mut Vec<Mapping>,
    mut edits: Vec<(Range<usize>, String)>,
) {
    edits.sort_by_key(|(range, _)| range.start);
    let mut result = String::with_capacity(out.len());
    let mut cursor = 0;
    for (range, replacement) in &edits {
        result += &out[cursor..range.start];
        result += replacement;
        cursor = range.end;
    }
    result += &out[cursor..];

    mappings.retain_mut(|m| {
        let mut shift: isize = 0;
        for (range, replacement) in &edits {
            if m.out >= range.end {
                shift += replacement.len() as isize - range.len() as isize;
            } else if m.out > range.start || (m.out == range.start && replacement.is_empty()) {
                if replacement.is_empty() {
                    return false;
                }
                m.out = range.start;
                break;
            } else {
                break;
            }
        }
        m.out = (m.out as isize + shift) as usize;
        true
    });
    *out = result;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits() {
        assert_eq!(
            split_list(".a, :is(.b, .c), [x=\",\"]"),
            [".a", ":is(.b, .c)", "[x=\",\"]"]
        );
    }

    #[test]
    fn nests() {
        let n = |p, c| nest(p, c, ", ");
        assert_eq!(n(".a", ".b"), ".a .b");
        assert_eq!(n(".a", "&:hover"), ".a:hover");
        assert_eq!(n(".a", "> .b, + .c"), ".a > .b, .a + .c");
        assert_eq!(n(".a, .b", ".c"), ":is(.a, .b) .c");
        assert_eq!(n(".a, .b", "&.c"), ":is(.a, .b).c");
        assert_eq!(n(".a .b", "&.c"), ".a .b.c");
        assert_eq!(n(".a .b", ".x &"), ".x :is(.a .b)");
        assert_eq!(n(".a", "[data-x='&'] &"), "[data-x='&'] .a");
    }

    #[test]
    fn edits_move_mappings() {
        use stylet_resolve::{FileId, LineCol, Loader, MemoryFs, ResolveConfig};
        let mut fs = MemoryFs::default();
        fs.insert("/a", "");
        let mut loader = Loader::new(fs, ResolveConfig::default());
        let file: FileId = loader.load("/a".as_ref()).unwrap();
        let at = |out| Mapping {
            out,
            file,
            src: LineCol { line: 0, col: 0 },
        };
        let mut out = "0123456789".to_string();
        let mut mappings = vec![at(0), at(2), at(3), at(5), at(8)];
        apply_edits(
            &mut out,
            &mut mappings,
            vec![(2..4, "XYZ".into()), (5..7, String::new())],
        );
        assert_eq!(out, "01XYZ4789");
        let offsets: Vec<_> = mappings.iter().map(|m| m.out).collect();
        assert_eq!(offsets, [0, 2, 2, 7]);
    }
}
