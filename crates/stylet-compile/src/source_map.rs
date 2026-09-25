//! Source map v3 generation.

use std::fmt::Write;
use stylet_resolve::{FileId, LineCol, LineIndex};

/// Links an output byte offset to a source position.
#[derive(Debug, Clone, Copy)]
pub struct Mapping {
    pub out: usize,
    pub file: FileId,
    pub src: LineCol,
}

pub struct Source<'a> {
    pub path: String,
    pub content: &'a str,
}

/// Builds the source map JSON. `mappings` must be ordered by output offset.
pub fn build<'a>(
    css: &str,
    mappings: &[Mapping],
    sources: impl Fn(FileId) -> Source<'a>,
    file: Option<&str>,
) -> String {
    let out_index = LineIndex::new(css);
    let mut source_ids: Vec<FileId> = Vec::new();
    let mut encoded = String::new();
    let (mut line, mut prev_col, mut prev_src, mut prev_src_line, mut prev_src_col) =
        (0u32, 0i64, 0i64, 0i64, 0i64);
    let mut first_in_line = true;
    for m in mappings {
        let generated = out_index.line_col((m.out as u32).into());
        while line < generated.line {
            encoded.push(';');
            line += 1;
            prev_col = 0;
            first_in_line = true;
        }
        let src = match source_ids.iter().position(|&f| f == m.file) {
            Some(i) => i,
            None => {
                source_ids.push(m.file);
                source_ids.len() - 1
            }
        } as i64;
        if !first_in_line {
            encoded.push(',');
        }
        first_in_line = false;
        vlq(&mut encoded, generated.col as i64 - prev_col);
        vlq(&mut encoded, src - prev_src);
        vlq(&mut encoded, m.src.line as i64 - prev_src_line);
        vlq(&mut encoded, m.src.col as i64 - prev_src_col);
        prev_col = generated.col as i64;
        prev_src = src;
        prev_src_line = m.src.line as i64;
        prev_src_col = m.src.col as i64;
    }

    let sources: Vec<_> = source_ids.iter().map(|&id| sources(id)).collect();
    let mut json = String::from("{\"version\":3");
    if let Some(file) = file {
        json += ",\"file\":";
        string(&mut json, file);
    }
    json += ",\"sources\":[";
    for (i, s) in sources.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        string(&mut json, &s.path);
    }
    json += "],\"sourcesContent\":[";
    for (i, s) in sources.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        string(&mut json, s.content);
    }
    json += "],\"names\":[],\"mappings\":";
    string(&mut json, &encoded);
    json.push('}');
    json
}

fn vlq(out: &mut String, value: i64) {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut v = if value < 0 {
        ((-value) << 1) | 1
    } else {
        value << 1
    } as u64;
    loop {
        let mut digit = (v & 31) as u8;
        v >>= 5;
        if v > 0 {
            digit |= 32;
        }
        out.push(CHARS[digit as usize] as char);
        if v == 0 {
            return;
        }
    }
}

fn string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
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

    #[test]
    fn encodes_vlq() {
        let mut s = String::new();
        for v in [0, 1, -1, 15, 16, -17, 1000] {
            vlq(&mut s, v);
            s.push(' ');
        }
        assert_eq!(s, "A C D e gB jB w+B ");
    }
}
