//! Tolerant, line-oriented parser: tokens → [`Program`].
//!
//! Parsing happens one logical line at a time. Trivia (comments, newlines) is
//! dropped, the remaining tokens for a line are classified, and any line that
//! does not fit a known shape becomes [`LineBody::Error`] rather than aborting
//! the parse. Recovery at line boundaries is the whole point — a linter must keep
//! reporting on the other 999 lines when line 1000 is malformed.
//!
//! ## Label disambiguation (M1 heuristic)
//!
//! Telling a label from a mnemonic is genuinely ambiguous without an instruction
//! table (that arrives at M3). Until then two unambiguous cases are recognized:
//!
//! 1. `name:` — an identifier followed by a colon is always a label.
//! 2. `name db ...` — an identifier followed by a pseudo-op is a label (the very
//!    common data-definition form, e.g. `msg db "hi", 0` or `count equ 5`).
//!
//! A lone identifier (`ret`) is treated as a mnemonic, not a label. This covers
//! the overwhelming majority of real source; the residual cases are refined once
//! the mnemonic table exists.

use crate::ast::*;
use crate::keywords;
use crate::lexer::{tokenize, Token, TokenKind};

/// Parse NASM `source` into a [`Program`].
pub fn parse(source: &str) -> Program {
    let tokens = tokenize(source);
    let mut lines = Vec::new();

    // Group non-trivia tokens by logical line, flushing on each newline.
    let mut current: Vec<Token> = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::Newline => {
                if !current.is_empty() {
                    lines.push(parse_line(std::mem::take(&mut current)));
                }
            }
            TokenKind::Comment => {} // drop trivia
            _ => current.push(token),
        }
    }
    if !current.is_empty() {
        lines.push(parse_line(current));
    }

    Program { lines }
}

/// Parse one line's worth of non-trivia tokens (guaranteed non-empty).
fn parse_line(tokens: Vec<Token>) -> Line {
    let line_no = tokens[0].span.line;
    let (label, rest) = split_label(&tokens);

    let body = if rest.is_empty() {
        LineBody::Empty // label-only line
    } else {
        parse_body(rest)
    };

    Line {
        line_no,
        label,
        body,
    }
}

/// Peel an optional leading label off the token slice, per the heuristic in the
/// module docs. Returns the label (if any) and the remaining tokens.
fn split_label(tokens: &[Token]) -> (Option<Label>, &[Token]) {
    let first = &tokens[0];
    if first.kind != TokenKind::Ident {
        return (None, tokens);
    }

    let colon_form = tokens.get(1).is_some_and(|t| t.kind == TokenKind::Colon);
    let pseudo_form = tokens
        .get(1)
        .is_some_and(|t| t.kind == TokenKind::Ident && keywords::is_pseudo_op(&t.text))
        && !keywords::is_directive(&first.text)
        && !keywords::is_pseudo_op(&first.text);

    if !colon_form && !pseudo_form {
        return (None, tokens);
    }

    let label = Label {
        name: first.text.clone(),
        span: first.span,
        is_local: first.text.starts_with('.'),
    };
    let consumed = if colon_form { 2 } else { 1 };
    (Some(label), &tokens[consumed..])
}

/// Classify and parse the body tokens (after any label), which are non-empty.
fn parse_body(tokens: &[Token]) -> LineBody {
    let head = &tokens[0];

    match head.kind {
        TokenKind::Preproc => LineBody::Preproc(parse_preproc(tokens)),
        TokenKind::Ident => {
            // Skip a single instruction prefix (`rep`, `lock`, ...) so the
            // mnemonic that follows is classified rather than the prefix.
            let (head, rest) = if keywords::is_prefix(&head.text) && tokens.len() > 1 {
                (&tokens[1], &tokens[2..])
            } else {
                (head, &tokens[1..])
            };

            if head.kind != TokenKind::Ident {
                return LineBody::Error;
            }
            if keywords::is_pseudo_op(&head.text) {
                LineBody::Pseudo(Pseudo {
                    op: head.text.clone(),
                    op_span: head.span,
                    operands: parse_operands(rest),
                })
            } else if keywords::is_directive(&head.text) {
                LineBody::Directive(Directive {
                    name: head.text.clone(),
                    name_span: head.span,
                    args: collect_idents(rest),
                })
            } else {
                LineBody::Instruction(Instruction {
                    mnemonic: head.text.clone(),
                    mnemonic_span: head.span,
                    operands: parse_operands(rest),
                })
            }
        }
        _ => LineBody::Error,
    }
}

fn parse_preproc(tokens: &[Token]) -> Preproc {
    let head = &tokens[0];
    // First identifier argument, if any — the defined name for %define/%macro.
    let name = tokens
        .iter()
        .skip(1)
        .find(|t| t.kind == TokenKind::Ident)
        .map(|t| Ident {
            name: t.text.clone(),
            span: t.span,
        });
    Preproc {
        keyword: head.text.clone(),
        keyword_span: head.span,
        name,
    }
}

/// Split operand tokens on top-level commas (commas nested inside `[]`/`()` do
/// not separate operands) and build one [`Operand`] per group.
fn parse_operands(tokens: &[Token]) -> Vec<Operand> {
    let mut operands = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;

    for (i, tok) in tokens.iter().enumerate() {
        match tok.kind {
            TokenKind::LBracket | TokenKind::LParen => depth += 1,
            TokenKind::RBracket | TokenKind::RParen => depth = (depth - 1).max(0),
            TokenKind::Comma if depth == 0 => {
                push_operand(&mut operands, &tokens[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    push_operand(&mut operands, &tokens[start..]);
    operands
}

fn push_operand(out: &mut Vec<Operand>, group: &[Token]) {
    if group.is_empty() {
        return; // skip empty groups from trailing/leading commas
    }
    let span = crate::diagnostics::Span::range(
        group[0].span.line,
        group[0].span.column,
        group[group.len() - 1].span.end_column,
    );
    out.push(Operand {
        span,
        idents: collect_idents(group),
    });
}

/// Collect the identifier tokens in a group that name symbol references (i.e.
/// excluding registers and size keywords).
fn collect_idents(tokens: &[Token]) -> Vec<Ident> {
    tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Ident && keywords::is_symbol_reference(&t.text))
        .map(|t| Ident {
            name: t.text.clone(),
            span: t.span,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colon_label_with_instruction() {
        let prog = parse("_start: mov eax, 1\n");
        assert_eq!(prog.lines.len(), 1);
        let line = &prog.lines[0];
        assert_eq!(line.label.as_ref().unwrap().name, "_start");
        match &line.body {
            LineBody::Instruction(i) => {
                assert_eq!(i.mnemonic, "mov");
                assert_eq!(i.operands.len(), 2);
            }
            other => panic!("expected instruction, got {other:?}"),
        }
    }

    #[test]
    fn data_label_without_colon() {
        let prog = parse("msg db \"hi\", 0\n");
        let line = &prog.lines[0];
        assert_eq!(line.label.as_ref().unwrap().name, "msg");
        assert!(matches!(line.body, LineBody::Pseudo(_)));
    }

    #[test]
    fn lone_mnemonic_is_not_a_label() {
        let prog = parse("ret\n");
        assert!(prog.lines[0].label.is_none());
        assert!(matches!(prog.lines[0].body, LineBody::Instruction(_)));
    }

    #[test]
    fn directive_args_are_idents() {
        let prog = parse("global _start\n");
        match &prog.lines[0].body {
            LineBody::Directive(d) => {
                assert_eq!(d.name, "global");
                assert_eq!(d.args[0].name, "_start");
            }
            other => panic!("expected directive, got {other:?}"),
        }
    }

    #[test]
    fn operand_idents_exclude_registers() {
        // `arr` is a symbol reference; `rbx` is a register and must be excluded.
        let prog = parse("mov eax, [rbx + arr]\n");
        let LineBody::Instruction(i) = &prog.lines[0].body else {
            panic!("expected instruction");
        };
        let refs: Vec<_> = i
            .operands
            .iter()
            .flat_map(|o| &o.idents)
            .map(|id| id.name.as_str())
            .collect();
        assert_eq!(refs, ["arr"]);
    }

    #[test]
    fn prefix_is_skipped() {
        let prog = parse("rep movsb\n");
        let LineBody::Instruction(i) = &prog.lines[0].body else {
            panic!("expected instruction");
        };
        assert_eq!(i.mnemonic, "movsb");
    }

    #[test]
    fn preproc_line_captures_keyword_and_name() {
        let prog = parse("%define WIDTH 80\n");
        match &prog.lines[0].body {
            LineBody::Preproc(p) => {
                assert_eq!(p.keyword, "%define");
                assert_eq!(p.name.as_ref().unwrap().name, "WIDTH");
            }
            other => panic!("expected preproc, got {other:?}"),
        }
    }

    #[test]
    fn blank_and_comment_lines_are_skipped() {
        let prog = parse("\n  ; just a comment\nret\n");
        assert_eq!(prog.lines.len(), 1);
        assert_eq!(prog.lines[0].line_no, 3);
    }
}
