//! Rule engine — the Strategy pattern.
//!
//! Every check implements [`Rule`]. A rule is a pure function of the analysis
//! model to diagnostics: it inspects [`Analysis`] and pushes findings, never
//! doing I/O and never mutating shared state. New checks are added by
//! implementing the trait and registering the type in [`builtin_rules`] — nothing
//! else in the pipeline changes.
//!
//! The input a rule sees is [`Analysis`] (defined in `crate::analysis`), which
//! carries the raw source plus the parsed program and symbol/macro tables. It
//! grows additively as milestones land (the CFG arrives at M4); existing rules
//! keep compiling because they only read the fields they need.

use crate::analysis::Analysis;
use crate::diagnostics::{Diagnostic, Severity};

mod style;

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
