//! Tokenizer for NASM source.
//!
//! NASM is line-oriented, so newlines are significant tokens rather than
//! whitespace — the parser (see `parser.rs`) slices the flat stream into logical
//! lines on [`TokenKind::Newline`]. A catch-all [`TokenKind::Unknown`] means the
//! lexer never fails: every byte becomes a token, so a stray character degrades
//! one token rather than aborting the whole file. That tolerance is what lets the
//! linter keep reporting on the rest of a file with a typo in it.
//!
//! The numeric and preprocessor lexemes are a *pragmatic* subset: [`Number`]
//! matches any digit-led run without classifying its base, which is all the M1/M2
//! rules need. Base-precise numeric parsing arrives with instruction validation
//! (M3), where operand encoding actually depends on it.
//!
//! [`Number`]: TokenKind::Number

use logos::Logos;

use crate::diagnostics::Span;

/// Lexical category of a token. Labels, mnemonics, directive names, registers,
/// and symbol references are *all* [`TokenKind::Ident`]; distinguishing them is
/// the parser's job, since it needs line context (and, later, an instruction
/// table) to tell a mnemonic from a label.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
#[logos(skip r"[ \t\f]+")]
pub enum TokenKind {
    #[regex(r"\r?\n")]
    Newline,
    #[regex(r";[^\n]*")]
    Comment,

    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,

    /// `$$` — start of the current section. Matched before `$` by longest match.
    #[token("$$")]
    DollarDollar,
    /// `$` — the current assembly position.
    #[token("$")]
    Dollar,

    /// Arithmetic/logical operators, lumped for M1 (expression parsing is not yet
    /// needed by any rule). Multi-char operators tokenize as adjacent singles.
    #[regex(r"[-+*/&|^~<>=!]")]
    Operator,

    /// A macro parameter reference such as `%1`.
    #[regex(r"%[0-9]+")]
    PreprocParam,
    /// A preprocessor directive or token: `%if`, `%macro`, `%%local`, `%$ctx`.
    #[regex(r"%%?[.$]?[A-Za-z_][A-Za-z0-9_]*")]
    Preproc,

    #[regex(r"'([^'\\\n]|\\.)*'")]
    #[regex(r#""([^"\\\n]|\\.)*""#)]
    #[regex(r"`([^`\\\n]|\\.)*`")]
    Str,

    /// Any digit-led literal (decimal, `0x1F`, `1Fh`, `0b1010`, `3.14`, `10q`).
    /// The base is not classified here — see the module note.
    #[regex(r"[0-9][0-9A-Za-z_.]*")]
    Number,

    /// NASM identifier: starts with a letter, `_`, `.`, or `?`; may contain
    /// `. _ $ # @ ~ ?`. Local labels begin with `.`.
    #[regex(r"[A-Za-z_.?][A-Za-z0-9_.$#@~?]*")]
    Ident,

    /// Any single character not matched above (e.g. a lone `%` or `\`). Lowest
    /// priority so it only fires as a last resort; keeps lexing total.
    #[regex(r".", priority = 0)]
    Unknown,
}

/// A lexed token: its category, the exact source slice, and its 1-based span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

impl Token {
    /// True for tokens that carry no syntactic meaning to the parser.
    pub fn is_trivia(&self) -> bool {
        matches!(self.kind, TokenKind::Comment | TokenKind::Newline)
    }
}

/// Tokenize `source` into a flat token stream including [`TokenKind::Newline`]s.
///
/// Column tracking is O(n) overall: the running column advances by the character
/// width of each inter-token gap (skipped whitespace) rather than rescanning from
/// the line start per token.
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut lexer = TokenKind::lexer(source);

    let mut line = 1usize;
    let mut last_end = 0usize; // byte offset just past the previous token
    let mut col = 1usize; // 1-based char column at `last_end`

    while let Some(result) = lexer.next() {
        let kind = result.unwrap_or(TokenKind::Unknown);
        let bytes = lexer.span();
        let slice = lexer.slice();

        // Advance the column past whitespace skipped since the last token.
        let gap = source[last_end..bytes.start].chars().count();
        let start_col = col + gap;
        let width = slice.chars().count();
        let end_col = start_col + width;

        out.push(Token {
            kind,
            text: slice.to_owned(),
            span: Span::range(line, start_col, end_col),
        });

        if kind == TokenKind::Newline {
            line += 1;
            col = 1;
        } else {
            col = end_col;
        }
        last_end = bytes.end;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_a_labelled_instruction() {
        use TokenKind::*;
        assert_eq!(
            kinds("_start: mov eax, 1\n"),
            [Ident, Colon, Ident, Ident, Comma, Number, Newline]
        );
    }

    #[test]
    fn comment_runs_to_end_of_line() {
        use TokenKind::*;
        assert_eq!(kinds("ret ; done\n"), [Ident, Comment, Newline]);
    }

    #[test]
    fn preprocessor_and_params() {
        use TokenKind::*;
        assert_eq!(kinds("%macro foo 1\n"), [Preproc, Ident, Number, Newline]);
        assert_eq!(
            kinds("mov eax, %1\n"),
            [Ident, Ident, Comma, PreprocParam, Newline]
        );
    }

    #[test]
    fn tracks_line_and_column() {
        let toks = tokenize("nop\n  ret\n");
        let ret = toks.iter().find(|t| t.text == "ret").unwrap();
        assert_eq!(ret.span.line, 2);
        assert_eq!(ret.span.column, 3); // after two spaces of indent
    }

    #[test]
    fn memory_operand_and_numbers() {
        use TokenKind::*;
        assert_eq!(
            kinds("mov eax, [rbx + 0x10]\n"),
            [Ident, Ident, Comma, LBracket, Ident, Operator, Number, RBracket, Newline]
        );
    }

    #[test]
    fn lone_percent_is_unknown_not_fatal() {
        use TokenKind::*;
        // A stray '%' must not abort lexing of the rest of the line.
        assert_eq!(kinds("a % b\n"), [Ident, Unknown, Ident, Newline]);
    }
}
