//! Preprocessor rules (NL01x): block balance and unused definitions.

use std::collections::HashSet;

use crate::analysis::Analysis;
use crate::ast::LineBody;
use crate::diagnostics::{Diagnostic, Severity, Span};
use crate::rules::Rule;
use crate::symbols::keyword_defines_name;

/// The three kinds of balanced preprocessor block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Macro,
    If,
    Rep,
}

impl Block {
    /// The `NL0xx` code reported for an imbalance of this block kind.
    fn code(self) -> &'static str {
        match self {
            Block::Macro => "NL010",
            Block::If => "NL011",
            Block::Rep => "NL012",
        }
    }

    fn opener(self) -> &'static str {
        match self {
            Block::Macro => "%macro",
            Block::If => "%if",
            Block::Rep => "%rep",
        }
    }
}

/// Classify a preprocessor keyword (with its leading `%`) as opening a block,
/// closing one, or neither.
enum Role {
    Open(Block),
    Close(Block),
    Other,
}

fn classify(keyword: &str) -> Role {
    let kw = keyword.trim_start_matches('%').to_ascii_lowercase();
    match kw.as_str() {
        "macro" | "imacro" | "rmacro" | "irmacro" => Role::Open(Block::Macro),
        "endmacro" => Role::Close(Block::Macro),
        "rep" => Role::Open(Block::Rep),
        "endrep" => Role::Close(Block::Rep),
        "endif" => Role::Close(Block::If),
        // Every conditional opener starts with `if` (%if, %ifdef, %ifndef, %ifidn, ...).
        // `%elif`/`%else` are continuations, not openers, and start with `e`.
        s if s.starts_with("if") => Role::Open(Block::If),
        _ => Role::Other,
    }
}

/// NL010 / NL011 / NL012 — unbalanced `%macro`, `%if`, or `%rep` blocks. A single
/// stack walk detects unclosed openers, stray closers, and mismatched nesting.
pub struct BlockBalance;

impl Rule for BlockBalance {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        // Stack of open blocks: (kind, span of the opener) in source order.
        let mut stack: Vec<(Block, Span)> = Vec::new();

        for line in &analysis.program.lines {
            let LineBody::Preproc(pre) = &line.body else {
                continue;
            };
            match classify(&pre.keyword) {
                Role::Open(block) => stack.push((block, pre.keyword_span)),
                Role::Close(block) => match stack.last() {
                    Some((open, _)) if *open == block => {
                        stack.pop();
                    }
                    Some((open, _)) => {
                        // Closer does not match the innermost open block.
                        out.push(Diagnostic::new(
                            block.code(),
                            Severity::MustFix,
                            pre.keyword_span,
                            format!(
                                "`{}` does not match the innermost open `{}` block",
                                pre.keyword,
                                open.opener()
                            ),
                        ));
                        stack.pop(); // recover so we don't cascade
                    }
                    None => out.push(Diagnostic::new(
                        block.code(),
                        Severity::MustFix,
                        pre.keyword_span,
                        format!("`{}` without a matching `{}`", pre.keyword, block.opener()),
                    )),
                },
                Role::Other => {}
            }
        }

        // Anything still open at end of file is unclosed.
        for (block, span) in stack {
            out.push(Diagnostic::new(
                block.code(),
                Severity::MustFix,
                span,
                format!("`{}` is never closed", block.opener()),
            ));
        }
    }
}

/// NL014 — a macro or define that is declared but never used anywhere in the file.
///
/// "Used" means the name appears as an instruction mnemonic (a macro invocation),
/// inside an operand (a constant), as a directive argument, or in a non-defining
/// preprocessor reference such as `%ifdef GUARD` (so include guards are not
/// mis-flagged). The defining occurrence itself is excluded.
pub struct UnusedMacro;

impl UnusedMacro {
    fn used_names(analysis: &Analysis) -> HashSet<String> {
        let mut used = HashSet::new();
        for line in &analysis.program.lines {
            match &line.body {
                LineBody::Instruction(instr) => {
                    used.insert(instr.mnemonic.clone());
                    for op in &instr.operands {
                        used.extend(op.idents.iter().map(|id| id.name.clone()));
                    }
                }
                LineBody::Pseudo(pseudo) => {
                    for op in &pseudo.operands {
                        used.extend(op.idents.iter().map(|id| id.name.clone()));
                    }
                }
                LineBody::Directive(dir) => {
                    used.extend(dir.args.iter().map(|id| id.name.clone()));
                }
                LineBody::Preproc(pre) => {
                    // Count references (e.g. %ifdef GUARD) but not the definition itself.
                    if !keyword_defines_name(&pre.keyword) {
                        if let Some(name) = &pre.name {
                            used.insert(name.name.clone());
                        }
                    }
                }
                LineBody::Empty | LineBody::Error => {}
            }
        }
        used
    }
}

impl Rule for UnusedMacro {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        let used = Self::used_names(analysis);
        for (name, span) in &analysis.macros.defines {
            if !used.contains(name) {
                out.push(Diagnostic::new(
                    "NL014",
                    Severity::Consider,
                    *span,
                    format!("`{name}` is defined but never used"),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Model;
    use crate::source::SourceFile;

    fn run_rule(rule: &dyn Rule, text: &str) -> Vec<Diagnostic> {
        let file = SourceFile::new("t.asm", text);
        let model = Model::build(&file);
        let analysis = Analysis::new(&file, &model);
        let mut out = Vec::new();
        rule.run(&analysis, &mut out);
        out
    }

    #[test]
    fn balanced_blocks_are_silent() {
        let src = "%macro prologue 0\n    push rbp\n%endmacro\n%ifdef X\n    nop\n%endif\n";
        assert!(run_rule(&BlockBalance, src).is_empty());
    }

    #[test]
    fn unclosed_macro_flagged_as_nl010() {
        let diags = run_rule(&BlockBalance, "%macro prologue 0\n    push rbp\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL010");
    }

    #[test]
    fn stray_endif_flagged_as_nl011() {
        let diags = run_rule(&BlockBalance, "%endif\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL011");
    }

    #[test]
    fn unclosed_rep_flagged_as_nl012() {
        let diags = run_rule(&BlockBalance, "%rep 4\n    nop\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL012");
    }

    #[test]
    fn unused_macro_flagged() {
        let diags = run_rule(&UnusedMacro, "%define WIDTH 80\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL014");
    }

    #[test]
    fn used_define_is_ok() {
        assert!(run_rule(&UnusedMacro, "%define WIDTH 80\n    mov eax, WIDTH\n").is_empty());
    }

    #[test]
    fn include_guard_is_not_flagged() {
        // GUARD is referenced only via %ifndef — must count as used.
        let src = "%ifndef GUARD\n%define GUARD 1\n%endif\n";
        assert!(run_rule(&UnusedMacro, src).is_empty());
    }
}
