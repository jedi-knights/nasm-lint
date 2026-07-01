//! Section/directive rules (NL02x).
//!
//! Only NL020 lands in M2. NL021 (write to a non-writable section), NL022
//! (missing `bits`/`default`), and NL023 (unknown directive) are deferred: the
//! first needs section-write semantics, and NL023 needs the mnemonic/directive
//! table that arrives with `insns.dat` at M3 (until then an unknown directive is
//! indistinguishable from a mnemonic).

use crate::analysis::Analysis;
use crate::ast::LineBody;
use crate::diagnostics::{Diagnostic, Severity};
use crate::rules::Rule;

/// NL020 — code or data that appears before any `section` directive.
///
/// NASM emits such code into an implicit default section, which is almost always a
/// mistake. Instructions and data-defining pseudo-ops trigger it; `equ` (a pure
/// constant) does not, since it emits nothing. Reported once, at the first
/// offending line.
pub struct CodeBeforeSection;

impl Rule for CodeBeforeSection {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        let mut seen_section = false;
        // Lines inside a `%macro` body are not emitted until the macro is invoked,
        // so they must not count as code before a section.
        let mut macro_depth: i32 = 0;

        for line in &analysis.program.lines {
            match &line.body {
                LineBody::Preproc(pre) => {
                    match pre
                        .keyword
                        .trim_start_matches('%')
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "macro" | "imacro" | "rmacro" | "irmacro" => macro_depth += 1,
                        "endmacro" => macro_depth = (macro_depth - 1).max(0),
                        _ => {}
                    }
                }
                LineBody::Directive(dir)
                    if dir.name.eq_ignore_ascii_case("section")
                        || dir.name.eq_ignore_ascii_case("segment") =>
                {
                    seen_section = true;
                }
                LineBody::Instruction(instr) if !seen_section && macro_depth == 0 => {
                    out.push(Diagnostic::new(
                        "NL020",
                        Severity::ShouldFix,
                        instr.mnemonic_span,
                        "code appears before any `section` directive",
                    ));
                    return;
                }
                // `equ` binds a constant and emits nothing, so it is fine here.
                LineBody::Pseudo(pseudo)
                    if !seen_section
                        && macro_depth == 0
                        && !pseudo.op.eq_ignore_ascii_case("equ") =>
                {
                    out.push(Diagnostic::new(
                        "NL020",
                        Severity::ShouldFix,
                        pseudo.op_span,
                        "data appears before any `section` directive",
                    ));
                    return;
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Model;
    use crate::source::SourceFile;

    fn run_rule(text: &str) -> Vec<Diagnostic> {
        let file = SourceFile::new("t.asm", text);
        let model = Model::build(&file);
        let analysis = Analysis::new(&file, &model);
        let mut out = Vec::new();
        CodeBeforeSection.run(&analysis, &mut out);
        out
    }

    #[test]
    fn instruction_before_section_flagged() {
        let diags = run_rule("    mov eax, 1\nsection .text\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL020");
    }

    #[test]
    fn code_after_section_is_ok() {
        assert!(run_rule("section .text\n    mov eax, 1\n").is_empty());
    }

    #[test]
    fn directives_and_equ_before_section_are_ok() {
        // global/extern/bits and equ emit nothing, so they may precede a section.
        assert!(run_rule(
            "global _start\nbits 64\nWIDTH equ 80\nsection .text\n_start:\n    ret\n"
        )
        .is_empty());
    }

    #[test]
    fn only_reports_once() {
        let diags = run_rule("    mov eax, 1\n    mov ebx, 2\n");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn code_inside_macro_body_is_not_flagged() {
        // `push rbp` is part of the macro definition, not emitted code.
        let src = "%macro prologue 0\n    push rbp\n%endmacro\nsection .text\n    ret\n";
        assert!(run_rule(src).is_empty());
    }
}
