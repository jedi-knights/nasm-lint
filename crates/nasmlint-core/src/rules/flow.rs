//! Control-flow rules (NL04x), driven by the [`Cfg`](crate::cfg::Cfg).
//!
//! M4 lands NL040 (unreachable code). NL041 (push/pop stack balance), NL042
//! (fall-through past a routine's end), and NL043 (register liveness) are
//! deferred: each needs cross-block dataflow whose false positives would be
//! costly, and they build naturally on the CFG this milestone establishes.

use crate::analysis::Analysis;
use crate::ast::LineBody;
use crate::diagnostics::{Diagnostic, Severity};
use crate::rules::Rule;

/// Whether the file pulls in other files. With includes, a routine may be called
/// from code we cannot see, so reachability analysis is unreliable and skipped.
fn has_include(analysis: &Analysis) -> bool {
    analysis.program.lines.iter().any(|line| {
        matches!(&line.body, LineBody::Preproc(p)
            if p.keyword.trim_start_matches('%').eq_ignore_ascii_case("include"))
    })
}

/// NL040 — code that cannot be reached along any control-flow path.
///
/// Reported once per contiguous unreachable run, at its first instruction. The
/// rule bows out when reachability cannot be trusted: any computed/unresolved jump
/// target (`jmp rax`, jump tables) or any `%include` disables it, so it never
/// invents a false positive from missing edges.
pub struct UnreachableCode;

impl Rule for UnreachableCode {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        let cfg = analysis.cfg;
        if cfg.has_unresolved_transfer || has_include(analysis) {
            return;
        }

        let mut in_run = false;
        for (i, node) in cfg.nodes.iter().enumerate() {
            if cfg.is_reachable(i) {
                in_run = false;
                continue;
            }
            if !in_run {
                out.push(Diagnostic::new(
                    "NL040",
                    Severity::ShouldFix,
                    node.span,
                    "unreachable code",
                ));
                in_run = true; // one finding per contiguous dead run
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
        UnreachableCode.run(&analysis, &mut out);
        out
    }

    #[test]
    fn dead_code_after_jmp_is_flagged() {
        let diags = run_rule("section .text\n    jmp done\n    mov eax, 1\ndone:\n    ret\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL040");
        assert_eq!(diags[0].span.line, 3); // the `mov`
    }

    #[test]
    fn code_after_ret_is_flagged() {
        let diags = run_rule("section .text\n    ret\n    mov eax, 1\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL040");
    }

    #[test]
    fn reachable_code_is_silent() {
        let src = "section .text\nglobal _start\n_start:\n    cmp eax, 0\n    je .zero\n    mov eax, 1\n.zero:\n    ret\n";
        assert!(run_rule(src).is_empty());
    }

    #[test]
    fn one_finding_per_dead_run() {
        let diags =
            run_rule("section .text\n    jmp e\n    mov eax, 1\n    mov ebx, 2\ne:\n    ret\n");
        assert_eq!(diags.len(), 1); // two dead insns, one finding
    }

    #[test]
    fn computed_jump_disables_the_rule() {
        // Can't resolve `jmp rax`, so we must not guess anything is dead.
        assert!(run_rule("section .text\n    jmp rax\n    mov eax, 1\n").is_empty());
    }

    #[test]
    fn include_disables_the_rule() {
        assert!(run_rule("%include \"x.inc\"\n    jmp done\n    nop\ndone:\n    ret\n").is_empty());
    }
}
