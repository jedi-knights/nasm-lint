//! Rule engine — the Strategy pattern.
//!
//! Every check implements [`Rule`]. A rule is a pure function of the analysis
//! model to diagnostics: it inspects [`Analysis`] and pushes findings, never
//! doing I/O and never mutating shared state. New checks are added by
//! implementing the trait and registering the type in [`builtin_rules`] — nothing
//! else in the pipeline changes.
//!
//! At M0 the only input a rule sees is the raw source (line text). As the front
//! end lands (M1+), [`Analysis`] gains borrowed AST, symbol tables, and the CFG;
//! existing rules keep compiling because they only read the fields they need.

use crate::diagnostics::{Diagnostic, Severity};
use crate::source::SourceFile;

mod style;

/// Read-only view of everything a rule may inspect for one file.
///
/// Grows over time (AST, symbols, CFG) — always additively, so a rule written
/// against an earlier shape keeps working.
pub struct Analysis<'a> {
    pub file: &'a SourceFile,
}

/// A single static-analysis check. Implementors are zero-sized and cheap to box.
pub trait Rule: Send + Sync {
    /// Stable `NL0xx` identifier. Used as the config key and SARIF `ruleId`;
    /// never reuse a code for a different check.
    fn code(&self) -> &'static str;

    /// Short human name (e.g. "trailing-whitespace").
    fn name(&self) -> &'static str;

    /// One-line description, surfaced in SARIF rule metadata and `--explain`.
    fn description(&self) -> &'static str;

    /// Bucket applied when config does not override it.
    fn default_severity(&self) -> Severity;

    /// Inspect `analysis` and push any findings onto `out`. The engine fills in
    /// the effective severity afterwards, so implementors may leave
    /// [`Diagnostic::severity`] set to `default_severity`.
    fn check(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>);
}

/// The built-in rule set, in code order. This is the single registration point.
pub fn builtin_rules() -> Vec<Box<dyn Rule>> {
    vec![Box::new(style::TrailingWhitespace)]
}
