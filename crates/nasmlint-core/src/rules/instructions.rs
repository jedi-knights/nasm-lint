//! Instruction rules (NL03x), driven by the vendored `insns.dat` table.
//!
//! M3 lands NL030 (unknown mnemonic) and NL031 (operand-count mismatch). NL032
//! (operand size mismatch) and NL033 (invalid operand form) are deferred: they
//! require inferring each operand's type (register width, memory, immediate) and
//! matching it against the encoded operand forms — a sizable subsystem whose
//! false positives would erode trust if shipped half-built.

use crate::analysis::Analysis;
use crate::ast::LineBody;
use crate::diagnostics::{Diagnostic, Severity};
use crate::insns::table;
use crate::keywords;
use crate::rules::Rule;

/// NL030 — an instruction mnemonic that NASM does not recognize.
///
/// A macro invocation (a user `%macro`/`%define` name) and a bare instruction
/// prefix (`rep`, `lock`, ...) are not mnemonics and are excluded, so only genuine
/// unknowns — typos and truly invalid instructions — are flagged.
pub struct UnknownMnemonic;

impl Rule for UnknownMnemonic {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        for line in &analysis.program.lines {
            let LineBody::Instruction(instr) = &line.body else {
                continue;
            };
            let name = &instr.mnemonic;
            if table().contains(name)
                || analysis.macros.defines.contains_key(name)
                || keywords::is_prefix(name)
            {
                continue;
            }
            out.push(Diagnostic::new(
                "NL030",
                Severity::MustFix,
                instr.mnemonic_span,
                format!("unknown instruction `{name}`"),
            ));
        }
    }
}

/// NL031 — a known instruction used with an operand count it never accepts.
///
/// Only known mnemonics are checked; unknowns are left to NL030. Variadic
/// pseudo-ops accept any count and never trip this rule.
pub struct OperandCount;

impl Rule for OperandCount {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        for line in &analysis.program.lines {
            let LineBody::Instruction(instr) = &line.body else {
                continue;
            };
            let got = instr.operands.len();
            if table().accepts_arity(&instr.mnemonic, got) != Some(false) {
                continue; // Some(true) = fine; None = unknown mnemonic (NL030's job)
            }
            let accepted = table().arities(&instr.mnemonic).unwrap_or_default();
            out.push(Diagnostic::new(
                "NL031",
                Severity::MustFix,
                instr.mnemonic_span,
                format!(
                    "`{}` accepts {} operand(s), but got {}",
                    instr.mnemonic,
                    describe(&accepted),
                    got
                ),
            ));
        }
    }
}

/// Render an arity set for a message: `[2]` → "2", `[0, 1]` → "0 or 1",
/// `[1, 2, 3]` → "1, 2, or 3".
fn describe(counts: &[usize]) -> String {
    match counts {
        [] => "no".to_string(),
        [n] => n.to_string(),
        _ => {
            let parts: Vec<String> = counts.iter().map(|n| n.to_string()).collect();
            let (last, rest) = parts.split_last().unwrap();
            format!("{} or {}", rest.join(", "), last)
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
    fn valid_instructions_are_silent() {
        let src = "section .text\n    mov eax, 1\n    ret\n    je near_label\nnear_label:\n";
        assert!(run_rule(&UnknownMnemonic, src).is_empty());
        assert!(run_rule(&OperandCount, src).is_empty());
    }

    #[test]
    fn typo_mnemonic_is_flagged() {
        let diags = run_rule(&UnknownMnemonic, "section .text\n    mxv eax, 1\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL030");
        assert!(diags[0].message.contains("mxv"));
    }

    #[test]
    fn user_macro_invocation_is_not_unknown() {
        // `prologue` is a defined macro, not an unknown instruction.
        let src = "%macro prologue 0\n    push rbp\n%endmacro\nsection .text\n    prologue\n";
        assert!(run_rule(&UnknownMnemonic, src).is_empty());
    }

    #[test]
    fn prefix_is_not_flagged() {
        assert!(run_rule(&UnknownMnemonic, "section .text\n    rep movsb\n").is_empty());
    }

    #[test]
    fn wrong_operand_count_is_flagged() {
        // `ret` takes 0 or 1 operands, never 2.
        let diags = run_rule(&OperandCount, "section .text\n    ret eax, ebx\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL031");
    }

    #[test]
    fn correct_operand_count_is_silent() {
        assert!(run_rule(&OperandCount, "section .text\n    mov eax, 1\n").is_empty());
        assert!(run_rule(&OperandCount, "section .text\n    ret\n").is_empty());
    }
}
