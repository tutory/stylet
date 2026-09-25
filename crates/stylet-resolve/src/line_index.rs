use stylet_syntax::TextSize;

/// Zero-based line and column. `col` counts UTF-16 code units (as source maps and LSP do).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Converts byte offsets to line/column positions.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of each line start.
    starts: Vec<u32>,
    text: String,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i as u32 + 1);
            }
        }
        Self {
            starts,
            text: text.to_string(),
        }
    }

    pub fn line_col(&self, offset: TextSize) -> LineCol {
        let offset = u32::from(offset).min(self.text.len() as u32);
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let start = self.starts[line] as usize;
        let col = self.text[start..offset as usize].encode_utf16().count();
        LineCol {
            line: line as u32,
            col: col as u32,
        }
    }

    /// Text of the zero-based `line`, without the line break.
    pub fn line_text(&self, line: u32) -> &str {
        let start = self.starts[line as usize] as usize;
        let end = self
            .starts
            .get(line as usize + 1)
            .map_or(self.text.len(), |&e| e as usize);
        self.text[start..end].trim_end_matches(['\n', '\r'])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions() {
        let index = LineIndex::new("ab\nc€d\r\n\ne");
        let at = |o: u32| index.line_col(o.into());
        assert_eq!(at(0), LineCol { line: 0, col: 0 });
        assert_eq!(at(3), LineCol { line: 1, col: 0 });
        assert_eq!(at(7), LineCol { line: 1, col: 2 });
        assert_eq!(at(11), LineCol { line: 3, col: 0 });
        assert_eq!(index.line_text(1), "c€d");
    }
}
