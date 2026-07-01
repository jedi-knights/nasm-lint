//! Style rules (NL05x). These operate purely on line text and need no parsing,
//! which makes them the natural first rules to land and the smoke test for the
//! whole engine + renderer pipeline.

use crate::diagnostics::{Diagnostic, Severity, Span};
use crate::rules::{Analysis, Rule};

/// NL053 — trailing whitespace at the end of a line.
///
/// Harmless to the assembler but a common source of noisy diffs; flagged as a
/// non-blocking `Consider`.
pub struct TrailingWhitespace;

impl Rule for TrailingWhitespace {
    fn code(&self) -> &'static str {
        "NL053"
    }

    fn name(&self) -> &'static str {
        "trailing-whitespace"
    }

    fn description(&self) -> &'static str {
        "Line has trailing whitespace."
    }

    fn default_severity(&self) -> Severity {
        Severity::Consider
    }

    fn check(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        for (idx, line) in analysis.file.lines().iter().enumerate() {
            let trimmed = line.trim_end();
            if trimmed.len() == line.len() {
                continue;
            }
            // Column math is in Unicode scalar values to match editor columns.
            let start = trimmed.chars().count() + 1;
            let end = line.chars().count() + 1;
            out.push(Diagnostic::new(
                self.code(),
                self.default_severity(),
                Span::range(idx + 1, start, end),
                "trailing whitespace",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceFile;

    fn run(text: &str) -> Vec<Diagnostic> {
        let file = SourceFile::new("test.asm", text);
        let analysis = Analysis { file: &file };
        let mut out = Vec::new();
        TrailingWhitespace.check(&analysis, &mut out);
        out
    }

    #[test]
    fn flags_trailing_space() {
        let diags = run("mov eax, 1  \nret\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL053");
        assert_eq!(diags[0].span.line, 1);
        assert_eq!(diags[0].span.column, 11); // after "mov eax, 1"
    }

    #[test]
    fn clean_source_is_silent() {
        assert!(run("mov eax, 1\nret\n").is_empty());
    }

    #[test]
    fn flags_trailing_tab() {
        let diags = run("ret\t\n");
        assert_eq!(diags.len(), 1);
    }
}
