//! Rule engine — the Strategy pattern.
//!
//! Every check implements [`Rule`]. A rule is a pure function of the analysis
//! model to diagnostics: it inspects [`Analysis`] and pushes findings, never
//! doing I/O and never mutating shared state. New checks are added by
//! implementing the trait and registering the type in [`builtin_rules`].
//!
//! ## Codes are the unit of configuration, not rules
//!
//! Each [`Diagnostic`] carries its own `NL0xx` code and default severity. The
//! engine enables/disables and re-grades findings by **code**, because that is
//! how users think (`NL011 = "off"`), and because one rule may legitimately emit
//! several codes — the preprocessor-balance rule reports `NL010`/`NL011`/`NL012`
//! from a single pass. Rule structs therefore carry no code of their own; the
//! [`catalog`] maps every code to its metadata for SARIF descriptors and docs.

use crate::analysis::Analysis;
use crate::diagnostics::{Diagnostic, Severity};

mod labels;
mod preprocessor;
mod sections;
mod style;

/// A single static-analysis pass. Implementors set each diagnostic's code and
/// default severity as they emit it; the engine applies config afterwards.
pub trait Rule: Send + Sync {
    /// Inspect `analysis` and push any findings onto `out`.
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>);
}

/// The built-in rule set. This is the single registration point.
pub fn builtin_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(labels::UndefinedReference),
        Box::new(labels::UnusedLabel),
        Box::new(labels::DuplicateLabel),
        Box::new(labels::UnusedExtern),
        Box::new(labels::UndefinedGlobal),
        Box::new(preprocessor::BlockBalance),
        Box::new(preprocessor::UnusedMacro),
        Box::new(sections::CodeBeforeSection),
        Box::new(style::MixedIndentation),
        Box::new(style::TrailingWhitespace),
    ]
}

/// Static metadata for one rule code, used for SARIF rule descriptors, `--explain`
/// output, and documentation. The default severity lives at each rule's emit site
/// (the rule is the source of truth); this table describes the code.
pub struct RuleInfo {
    pub code: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub default_severity: Severity,
}

/// Every diagnostic code the linter can emit, in code order.
pub fn catalog() -> &'static [RuleInfo] {
    use Severity::*;
    &[
        RuleInfo {
            code: "NL001",
            name: "undefined-reference",
            description: "Reference to a label that is neither defined nor declared external.",
            default_severity: MustFix,
        },
        RuleInfo {
            code: "NL002",
            name: "unused-label",
            description: "Label is defined but never referenced.",
            default_severity: Consider,
        },
        RuleInfo {
            code: "NL003",
            name: "duplicate-label",
            description: "Non-local label is defined more than once.",
            default_severity: MustFix,
        },
        RuleInfo {
            code: "NL004",
            name: "unused-extern",
            description: "Symbol declared `extern` but never referenced.",
            default_severity: Consider,
        },
        RuleInfo {
            code: "NL005",
            name: "undefined-global",
            description: "Symbol declared `global` but never defined.",
            default_severity: MustFix,
        },
        RuleInfo {
            code: "NL010",
            name: "unbalanced-macro",
            description: "`%macro` without a matching `%endmacro` (or vice versa).",
            default_severity: MustFix,
        },
        RuleInfo {
            code: "NL011",
            name: "unbalanced-if",
            description: "`%if` without a matching `%endif` (or vice versa).",
            default_severity: MustFix,
        },
        RuleInfo {
            code: "NL012",
            name: "unbalanced-rep",
            description: "`%rep` without a matching `%endrep` (or vice versa).",
            default_severity: MustFix,
        },
        RuleInfo {
            code: "NL014",
            name: "unused-macro",
            description: "Macro or define is declared but never used.",
            default_severity: Consider,
        },
        RuleInfo {
            code: "NL020",
            name: "code-before-section",
            description: "Code or data appears before any `section` directive.",
            default_severity: ShouldFix,
        },
        RuleInfo {
            code: "NL050",
            name: "mixed-indentation",
            description: "Leading indentation mixes tabs and spaces.",
            default_severity: ShouldFix,
        },
        RuleInfo {
            code: "NL053",
            name: "trailing-whitespace",
            description: "Line has trailing whitespace.",
            default_severity: Consider,
        },
    ]
}

/// Look up a code's metadata, if it is a known rule.
pub fn lookup(code: &str) -> Option<&'static RuleInfo> {
    catalog().iter().find(|r| r.code == code)
}
