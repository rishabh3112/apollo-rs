/// Tokens generated by the lexer.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u16)]
pub enum TokenKind {
    Bang,     // !
    Dollar,   // $
    LParen,   // (
    RParen,   // )
    Spread,   // ...
    Comma,    // ,
    Colon,    // :
    Eq,       // =
    At,       // @
    LBracket, // [
    RBracket, // ]
    LBrace,   // {
    Pipe,     // |
    RBrace,   // }
    Fragment,
    Directive,
    Query,
    On,
    Eof,

    // composite nodes
    Node,
    Int,
    Float,

    // Root node
    Root,
}

// TODO: remove me
impl From<TokenKind> for rowan::SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        Self(kind as u16)
    }
}
