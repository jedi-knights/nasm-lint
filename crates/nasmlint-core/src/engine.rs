//! Analysis pipeline: source + config in, sorted diagnostics out.
//!
//! At M0 the pipeline is just "run every enabled rule". As the front end lands,
//! this is where lexing → parsing → symbol resolution → CFG construction will be
//! threaded, with the resulting model handed to rules via `Analysis`.

use crate::analysis::{Analysis, Model};
use crate::config::Config;
use crate::diagnostics::Diagnostic;
use crate::rules::builtin_rules;
use crate::source::SourceFile;

/// Analyze one file under `config`, returning findings sorted by position.
///
/// Each rule's findings are re-graded to their effective severity (config
/// override or built-in default) before being collected, so downstream code
/// never has to consult the config again.
pub fn analyze(file: &SourceFile, config: &Config) -> Vec<Diagnostic> {
    // Build the front-end model once; every rule shares this borrowed view.
    let model = Model::build(file);
    let analysis = Analysis::new(file, &model);
    let mut diagnostics = Vec::new();

    for rule in builtin_rules() {
        if !config.is_enabled(rule.code()) {
            continue;
        }
        let effective = config.severity_for(rule.code(), rule.default_severity());

        let before = diagnostics.len();
        rule.check(&analysis, &mut diagnostics);
        for diag in &mut diagnostics[before..] {
            diag.severity = effective;
        }
    }

    diagnostics.sort_by_key(|d| (d.span.line, d.span.column, d.code));
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn respects_disabled_rule() {
        let file = SourceFile::new("t.asm", "ret  \n");
        let mut config = Config::default();
        assert_eq!(analyze(&file, &config).len(), 1);

        config = Config::from_toml("[rules]\nNL053 = \"off\"\n").unwrap();
        assert!(analyze(&file, &config).is_empty());
    }

    #[test]
    fn applies_severity_override() {
        use crate::diagnostics::Severity;
        let file = SourceFile::new("t.asm", "ret  \n");
        let config = Config::from_toml("[rules]\nNL053 = \"must-fix\"\n").unwrap();
        let diags = analyze(&file, &config);
        assert_eq!(diags[0].severity, Severity::MustFix);
    }
}
