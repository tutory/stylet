//! Conversions between LSP positions/URIs and stylet offsets/paths.

use lsp_types::{Position, Range, Uri};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use stylet_resolve::{LineIndex, normalize};
use stylet_syntax::TextRange;

pub fn range(index: &LineIndex, range: TextRange) -> Range {
    let start = index.line_col(range.start());
    let end = index.line_col(range.end());
    Range::new(
        Position::new(start.line, start.col),
        Position::new(end.line, end.col),
    )
}

/// Byte offset of an LSP position (UTF-16 columns).
pub fn offset(text: &str, position: Position) -> Option<u32> {
    let mut line_start = 0;
    for _ in 0..position.line {
        line_start += text[line_start..].find('\n')? + 1;
    }
    let line = &text[line_start..];
    let mut units = 0;
    for (i, c) in line.char_indices() {
        if units >= position.character || c == '\n' {
            return Some((line_start + i) as u32);
        }
        units += c.len_utf16() as u32;
    }
    Some(text.len() as u32)
}

const UNRESERVED: &[u8] = b"-._~/";

pub fn path_to_uri(path: &Path) -> Option<Uri> {
    let mut encoded = String::from("file://");
    for b in path.to_str()?.bytes() {
        if b.is_ascii_alphanumeric() || UNRESERVED.contains(&b) {
            encoded.push(b as char);
        } else {
            encoded += &format!("%{b:02X}");
        }
    }
    Uri::from_str(&encoded).ok()
}

pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let text = uri.as_str().strip_prefix("file://")?;
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 1 {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(normalize(Path::new(&String::from_utf8(out).ok()?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris() {
        let path = Path::new("/a b/ü.styl");
        let uri = path_to_uri(path).unwrap();
        assert_eq!(uri.as_str(), "file:///a%20b/%C3%BC.styl");
        assert_eq!(uri_to_path(&uri).unwrap(), path);
    }

    #[test]
    fn offsets() {
        let text = "ab\nc€d\n";
        assert_eq!(offset(text, Position::new(0, 1)), Some(1));
        assert_eq!(offset(text, Position::new(1, 2)), Some(7));
        assert_eq!(offset(text, Position::new(1, 9)), Some(8));
    }
}
