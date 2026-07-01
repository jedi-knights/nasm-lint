//! Analysis pipeline: source + config in, sorted diagnostics out.
//!
//! The pipeline builds the front-end model once (`Model::build`), runs every rule
//! against the shared borrowed view, then applies config **per diagnostic code**:
//! disabled codes are dropped and the rest are re-graded to their effective
//! severity. Config is code-centric (not rule-centric) because a single rule may
//! emit several codes — see the note in `rules/mod.rs`.

use crate::analysis::{Analysis, Model};
use crate::config::Config;
use crate::diagnostics::Diagnostic;
use crate::rules::builtin_rules;
use crate::source::SourceFile;

/// Analyze one file under `config`, returning findings sorted by position.
pub fn analyze(file: &SourceFile, config: &Config) -> Vec<Diagnostic> {
    // Build the front-end model once; every rule shares this borrowed view.
    let model = Model::build(file);
    let analysis = Analysis::new(file, &model);

    let mut diagnostics = Vec::new();
    for rule in builtin_rules() {
        rule.run(&analysis, &mut diagnostics);
    }

    // Apply config by code: drop disabled codes, re-grade the rest. Each
    // diagnostic already carries its rule's default severity, which is the base
    // the override is applied against.
    diagnostics.retain(|d| config.is_enabled(d.code));
    for diag in &mut diagnostics {
        diag.severity = config.severity_for(diag.code, diag.severity);
    }

    diagnostics.sort_by_key(|d| (d.span.line, d.span.column, d.code));
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    // A section directive keeps NL020 (code-before-section) quiet so these
    // engine-mechanics tests see only the NL053 trailing-whitespace finding.
    const ONE_NL053: &str = "section .text\n    ret  \n";

    #[test]
    fn respects_disabled_code() {
        let file = SourceFile::new("t.asm", ONE_NL053);
        let config = Config::default();
        assert_eq!(analyze(&file, &config).len(), 1);

        let config = Config::from_toml("[rules]\nNL053 = \"off\"\n").unwrap();
        assert!(analyze(&file, &config).is_empty());
    }

    #[test]
    fn applies_severity_override() {
        use crate::diagnostics::Severity;
        let file = SourceFile::new("t.asm", ONE_NL053);
        let config = Config::from_toml("[rules]\nNL053 = \"must-fix\"\n").unwrap();
        let diags = analyze(&file, &config);
        assert_eq!(diags[0].severity, Severity::MustFix);
    }
}
