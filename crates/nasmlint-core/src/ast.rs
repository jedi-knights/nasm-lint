//! Line-oriented AST for NASM source.
//!
//! NASM's grammar is fundamentally one-statement-per-line, so the AST mirrors
//! that: a [`Program`] is a vector of [`Line`]s, each optionally introduced by a
//! label and carrying one [`LineBody`]. This shape keeps diagnostics trivially
//! mappable back to source positions and lets the parser recover at line
//! boundaries — an unparseable line becomes [`LineBody::Error`] without poisoning
//! its neighbours.

use crate::diagnostics::Span;

/// A whole source file parsed into lines, in file order.
#[derive(Debug, Default)]
pub struct Program {
    pub lines: Vec<Line>,
}

/// One logical line: an optional leading label plus its body.
#[derive(Debug)]
pub struct Line {
    pub line_no: usize,
    pub label: Option<Label>,
    pub body: LineBody,
}

/// A named source position — the unit of both symbol definitions and references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A label definition (`foo:` or the leading name of a `foo db 0` line).
#[derive(Debug, Clone)]
pub struct Label {
    pub name: String,
    pub span: Span,
    /// True for NASM local labels, whose name begins with `.`.
    pub is_local: bool,
}

/// The statement on a line, after any leading label.
#[derive(Debug)]
pub enum LineBody {
    /// Blank line, or label-only / comment-only line.
    Empty,
    /// A CPU instruction: mnemonic plus operands.
    Instruction(Instruction),
    /// A NASM directive (`section`, `global`, `extern`, `bits`, ...).
    Directive(Directive),
    /// A data/reservation/definition pseudo-op (`db`, `resb`, `equ`, `times`, ...).
    Pseudo(Pseudo),
    /// A preprocessor line (`%define`, `%macro`, `%if`, ...).
    Preproc(Preproc),
    /// The line could not be parsed; kept so analysis continues past it.
    Error,
}

/// A single operand: its overall span plus the identifier references it contains
/// (registers and size keywords already filtered out by the parser, so what
/// remains are candidate symbol references).
#[derive(Debug)]
pub struct Operand {
    pub span: Span,
    pub idents: Vec<Ident>,
}

#[derive(Debug)]
pub struct Instruction {
    pub mnemonic: String,
    pub mnemonic_span: Span,
    pub operands: Vec<Operand>,
}

#[derive(Debug)]
pub struct Directive {
    pub name: String,
    pub name_span: Span,
    /// Identifier arguments (e.g. the symbol in `global _start`).
    pub args: Vec<Ident>,
}

#[derive(Debug)]
pub struct Pseudo {
    pub op: String,
    pub op_span: Span,
    pub operands: Vec<Operand>,
}

#[derive(Debug)]
pub struct Preproc {
    /// The directive keyword including its `%`, e.g. `%macro`, `%endif`.
    pub keyword: String,
    pub keyword_span: Span,
    /// The first identifier argument, when present (e.g. the name in
    /// `%define NAME ...` or `%macro NAME 1`). Used for the macro table.
    pub name: Option<Ident>,
}
