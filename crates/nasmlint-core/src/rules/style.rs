//! Style rules (NL05x). These operate purely on line text and need no parsing,
//! which keeps them cheap and independent of the AST.

use crate::analysis::Analysis;
use crate::diagnostics::{Diagnostic, Severity, Span};
use crate::rules::Rule;

/// NL053 — trailing whitespace at the end of a line.
///
/// Harmless to the assembler but a common source of noisy diffs; flagged as a
/// non-blocking `Consider`.
pub struct TrailingWhitespace;

impl Rule for TrailingWhitespace {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        for (idx, line) in analysis.file.lines().iter().enumerate() {
            let trimmed = line.trim_end();
            if trimmed.len() == line.len() {
                continue;
            }
            // Column math is in Unicode scalar values to match editor columns.
            let start = trimmed.chars().count() + 1;
            let end = line.chars().count() + 1;
            out.push(Diagnostic::new(
                "NL053",
                Severity::Consider,
                Span::range(idx + 1, start, end),
                "trailing whitespace",
            ));
        }
    }
}

/// NL050 — indentation that mixes tabs and spaces on the same line.
///
/// Mixed leading whitespace renders differently across editors and is the classic
/// source of misaligned assembly; flagged as `ShouldFix` because it drifts silently
/// but does not break the build.
pub struct MixedIndentation;

impl Rule for MixedIndentation {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        for (idx, line) in analysis.file.lines().iter().enumerate() {
            let indent: &str = &line[..line.len() - line.trim_start().len()];
            if indent.contains(' ') && indent.contains('\t') {
                let end = indent.chars().count() + 1;
                out.push(Diagnostic::new(
                    "NL050",
                    Severity::ShouldFix,
                    Span::range(idx + 1, 1, end),
                    "indentation mixes tabs and spaces",
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
        let file = SourceFile::new("test.asm", text);
        let model = Model::build(&file);
        let analysis = Analysis::new(&file, &model);
        let mut out = Vec::new();
        rule.run(&analysis, &mut out);
        out
    }

    #[test]
    fn flags_trailing_space() {
        let diags = run_rule(&TrailingWhitespace, "mov eax, 1  \nret\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL053");
        assert_eq!(diags[0].span.line, 1);
        assert_eq!(diags[0].span.column, 11); // after "mov eax, 1"
    }

    #[test]
    fn clean_source_is_silent() {
        assert!(run_rule(&TrailingWhitespace, "mov eax, 1\nret\n").is_empty());
    }

    #[test]
    fn flags_trailing_tab() {
        assert_eq!(run_rule(&TrailingWhitespace, "ret\t\n").len(), 1);
    }

    #[test]
    fn flags_space_then_tab_indent() {
        let diags = run_rule(&MixedIndentation, " \tmov eax, 1\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL050");
    }

    #[test]
    fn consistent_indent_is_silent() {
        assert!(run_rule(&MixedIndentation, "\tmov eax, 1\n    ret\n").is_empty());
    }
}
