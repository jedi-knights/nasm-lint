//! Diagnostic model shared by every rule, renderer, and interface (CLI + LSP).
//!
//! The three severity buckets are deliberate and match the team-wide findings
//! format (Must Fix / Should Fix / Consider). Renderers map them onto their own
//! vocabularies (SARIF levels, LSP severities) rather than inventing new buckets,
//! so a finding means the same thing everywhere it surfaces.

use serde::Serialize;

/// Severity bucket for a diagnostic.
///
/// Ordering matters: `MustFix < ShouldFix < Consider` is intentionally *reversed*
/// from urgency so that "at least this severe" gating reads naturally as
/// `severity <= threshold` (a `MustFix` clears every threshold). See
/// [`Severity::meets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Correctness failure — the code will break under realistic input.
    MustFix,
    /// Drift over time — inconsistent or fragile, will bite later.
    ShouldFix,
    /// Non-blocking improvement.
    Consider,
}

impl Severity {
    /// SARIF 2.1.0 `level` string for this bucket.
    pub fn sarif_level(self) -> &'static str {
        match self {
            Severity::MustFix => "error",
            Severity::ShouldFix => "warning",
            Severity::Consider => "note",
        }
    }

    /// Human-facing label used in the terminal renderer.
    pub fn label(self) -> &'static str {
        match self {
            Severity::MustFix => "must-fix",
            Severity::ShouldFix => "should-fix",
            Severity::Consider => "consider",
        }
    }

    /// True when `self` is at least as severe as `threshold`
    /// (used for exit-code gating: any finding that `meets` the gate fails CI).
    pub fn meets(self, threshold: Severity) -> bool {
        self <= threshold
    }
}

/// 1-based source position. Column counts Unicode scalar values, not bytes, so it
/// lines up with what an editor shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    /// Exclusive end column on the same line; equals `column` for a point span.
    pub end_column: usize,
}

impl Span {
    pub fn point(line: usize, column: usize) -> Self {
        Span {
            line,
            column,
            end_column: column,
        }
    }

    pub fn range(line: usize, column: usize, end_column: usize) -> Self {
        Span {
            line,
            column,
            end_column,
        }
    }
}

/// A single finding produced by a rule against one source file.
///
/// `code` is a stable `NL0xx` identifier — it is what users enable/disable in
/// config and what SARIF reports as the `ruleId`, so it must never be reused for
/// a different check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn new(
        code: &'static str,
        severity: Severity,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Diagnostic {
            code,
            severity,
            span,
            message: message.into(),
        }
    }
}
