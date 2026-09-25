macro_rules! syntax_kinds {
    ($($kind:ident),* $(,)?) => {
        /// Kind of a token or node in the stylet syntax tree.
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $($kind),*
        }

        const ALL_KINDS: &[SyntaxKind] = &[$(SyntaxKind::$kind),*];
    };
}

syntax_kinds! {
    // Trivia
    WHITESPACE,
    NEWLINE,
    LINE_COMMENT,
    BLOCK_COMMENT,

    // Tokens
    IDENT,
    AT_KEYWORD,
    HASH,
    PLACEHOLDER_NAME,
    STRING,
    URL,
    NUMBER,
    L_BRACE,
    R_BRACE,
    L_PAREN,
    R_PAREN,
    L_BRACK,
    R_BRACK,
    COLON,
    SEMICOLON,
    COMMA,
    DOT,
    AMP,
    GT,
    PLUS,
    TILDE,
    STAR,
    SLASH,
    EQ,
    BANG,
    PIPE,
    DELIM,
    EOF,

    // Nodes
    ROOT,
    RULE,
    SELECTOR,
    PLACEHOLDER,
    BLOCK,
    DECLARATION,
    PROPERTY,
    VALUE,
    AT_RULE,
    PRELUDE,
    IMPORT,
    EXTEND,
    ERROR,
}

impl SyntaxKind {
    /// Whitespace, newlines and comments.
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::WHITESPACE | Self::NEWLINE | Self::LINE_COMMENT | Self::BLOCK_COMMENT
        )
    }

    fn from_raw(raw: u16) -> Self {
        ALL_KINDS[raw as usize]
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stylet {}

impl rowan::Language for Stylet {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_raw(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<Stylet>;
pub type SyntaxToken = rowan::SyntaxToken<Stylet>;
pub type SyntaxElement = rowan::SyntaxElement<Stylet>;
