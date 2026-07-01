//! Conversion from nasm-lint's core diagnostics to LSP diagnostics.
//!
//! Kept separate from the server so it can be unit-tested without a running LSP
//! session. The only real work is the coordinate shift: core spans are 1-based
//! (matching editors' displayed line/column), while LSP positions are 0-based.

use std::path::PathBuf;

use nasmlint_core::{analyze, Config, Diagnostic as CoreDiagnostic, Severity, SourceFile};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

fn severity(sev: Severity) -> DiagnosticSeverity {
    match sev {
        Severity::MustFix => DiagnosticSeverity::ERROR,
        Severity::ShouldFix => DiagnosticSeverity::WARNING,
        Severity::Consider => DiagnosticSeverity::INFORMATION,
    }
}

/// Map one core diagnostic to an LSP diagnostic, converting 1-based spans to
/// 0-based LSP positions.
pub fn to_lsp(diag: &CoreDiagnostic) -> Diagnostic {
    let line = diag.span.line.saturating_sub(1) as u32;
    let start = diag.span.column.saturating_sub(1) as u32;
    let end = diag.span.end_column.saturating_sub(1) as u32;
    Diagnostic {
        range: Range::new(Position::new(line, start), Position::new(line, end)),
        severity: Some(severity(diag.severity)),
        code: Some(NumberOrString::String(diag.code.to_string())),
        source: Some("nasm-lint".to_string()),
        message: diag.message.clone(),
        ..Default::default()
    }
}

/// Analyze a document's text and return LSP diagnostics.
///
/// Uses the default configuration; honoring a workspace `.nasmlint.toml` is a
/// follow-up (it needs workspace-root resolution the server does not track yet).
pub fn analyze_text(path: impl Into<PathBuf>, text: &str) -> Vec<Diagnostic> {
    let file = SourceFile::new(path, text);
    analyze(&file, &Config::default())
        .iter()
        .map(to_lsp)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nasmlint_core::Span;

    #[test]
    fn shifts_to_zero_based_positions() {
        let core = CoreDiagnostic::new("NL001", Severity::MustFix, Span::range(3, 5, 9), "x");
        let lsp = to_lsp(&core);
        assert_eq!(lsp.range.start.line, 2);
        assert_eq!(lsp.range.start.character, 4);
        assert_eq!(lsp.range.end.character, 8);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(lsp.code, Some(NumberOrString::String("NL001".to_string())));
        assert_eq!(lsp.source.as_deref(), Some("nasm-lint"));
    }

    #[test]
    fn severity_mapping() {
        assert_eq!(severity(Severity::MustFix), DiagnosticSeverity::ERROR);
        assert_eq!(severity(Severity::ShouldFix), DiagnosticSeverity::WARNING);
        assert_eq!(
            severity(Severity::Consider),
            DiagnosticSeverity::INFORMATION
        );
    }

    #[test]
    fn analyzes_a_document() {
        // A trailing-whitespace line (NL053) after a section: exactly one finding.
        let diags = analyze_text("t.asm", "section .text\n    ret  \n");
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].code,
            Some(NumberOrString::String("NL053".to_string()))
        );
        assert_eq!(diags[0].range.start.line, 1); // second line, 0-based
    }
}
