//! Label and symbol rules (NL00x): resolution, duplication, and unused
//! declarations, driven by the [`SymbolTable`](crate::symbols::SymbolTable) that
//! the front end builds.
//!
//! ## Why some rules skip files that `%include`
//!
//! `NL001` (undefined reference) and `NL005` (undefined global) only fire when the
//! file contains no `%include`. Without processing included files (deferred — it
//! needs I/O and path resolution), a symbol defined in another file would look
//! undefined here and produce false positives. Gating on "self-contained file"
//! keeps these `MustFix` rules trustworthy: in a file that includes nothing, an
//! unresolved reference genuinely is an error.

use std::collections::HashSet;

use crate::analysis::Analysis;
use crate::ast::LineBody;
use crate::diagnostics::{Diagnostic, Severity};
use crate::rules::Rule;

/// Whether the file pulls in other files, in which case cross-file symbols are
/// unknown to us and resolution rules must not fire.
fn has_include(analysis: &Analysis) -> bool {
    analysis.program.lines.iter().any(|line| {
        matches!(&line.body, LineBody::Preproc(p)
            if p.keyword.trim_start_matches('%').eq_ignore_ascii_case("include"))
    })
}

/// Set of every symbol name referenced in an operand.
fn referenced_names<'a>(analysis: &Analysis<'a>) -> HashSet<&'a str> {
    analysis
        .symbols
        .references
        .iter()
        .map(|r| r.name.as_str())
        .collect()
}

/// NL001 — reference to a label that is neither defined nor declared `extern`
/// (nor a macro/define). Skipped when the file `%include`s others.
pub struct UndefinedReference;

impl Rule for UndefinedReference {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        if has_include(analysis) {
            return;
        }
        for reference in &analysis.symbols.references {
            let name = &reference.name;
            if analysis.symbols.is_known(name) || analysis.macros.defines.contains_key(name) {
                continue;
            }
            out.push(Diagnostic::new(
                "NL001",
                Severity::MustFix,
                reference.span,
                format!("undefined label `{name}`"),
            ));
        }
    }
}

/// NL002 — a label defined but never referenced. Labels declared `global` are
/// excluded: they are intentionally used from outside this file.
pub struct UnusedLabel;

impl Rule for UnusedLabel {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        let referenced = referenced_names(analysis);
        let exported: HashSet<&str> = analysis
            .symbols
            .globals
            .iter()
            .map(|g| g.name.as_str())
            .collect();

        for (name, sites) in &analysis.symbols.definitions {
            if referenced.contains(name.as_str()) || exported.contains(name.as_str()) {
                continue;
            }
            out.push(Diagnostic::new(
                "NL002",
                Severity::Consider,
                sites[0],
                format!("label `{name}` is defined but never used"),
            ));
        }
    }
}

/// NL003 — a non-local label defined more than once. Local labels (starting with
/// `.`) are scoped to their enclosing non-local label and may legitimately repeat,
/// so they are not flagged here.
pub struct DuplicateLabel;

impl Rule for DuplicateLabel {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        for (name, sites) in &analysis.symbols.definitions {
            if name.starts_with('.') || sites.len() < 2 {
                continue;
            }
            // Flag every redefinition after the first.
            for site in &sites[1..] {
                out.push(Diagnostic::new(
                    "NL003",
                    Severity::MustFix,
                    *site,
                    format!("duplicate definition of label `{name}`"),
                ));
            }
        }
    }
}

/// NL004 — a symbol declared `extern` but never referenced (dead declaration).
pub struct UnusedExtern;

impl Rule for UnusedExtern {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        let referenced = referenced_names(analysis);
        for ext in &analysis.symbols.externs {
            if !referenced.contains(ext.name.as_str()) {
                out.push(Diagnostic::new(
                    "NL004",
                    Severity::Consider,
                    ext.span,
                    format!("extern `{}` is declared but never used", ext.name),
                ));
            }
        }
    }
}

/// NL005 — a symbol declared `global` but never defined in this file. Skipped when
/// the file `%include`s others (the definition may live in an included file).
pub struct UndefinedGlobal;

impl Rule for UndefinedGlobal {
    fn run(&self, analysis: &Analysis, out: &mut Vec<Diagnostic>) {
        if has_include(analysis) {
            return;
        }
        for global in &analysis.symbols.globals {
            if !analysis.symbols.definitions.contains_key(&global.name) {
                out.push(Diagnostic::new(
                    "NL005",
                    Severity::MustFix,
                    global.span,
                    format!("global `{}` is declared but never defined", global.name),
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
        let file = SourceFile::new("t.asm", text);
        let model = Model::build(&file);
        let analysis = Analysis::new(&file, &model);
        let mut out = Vec::new();
        rule.run(&analysis, &mut out);
        out
    }

    #[test]
    fn undefined_reference_is_flagged() {
        let diags = run_rule(&UndefinedReference, "_start:\n    jmp missing\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NL001");
        assert!(diags[0].message.contains("missing"));
    }

    #[test]
    fn defined_and_extern_references_are_ok() {
        assert!(run_rule(&UndefinedReference, "_start:\n    jmp _start\n").is_empty());
        assert!(run_rule(&UndefinedReference, "extern printf\n    call printf\n").is_empty());
    }

    #[test]
    fn undefined_reference_skipped_with_include() {
        // Symbol may come from the included file — do not false-positive.
        assert!(run_rule(
            &UndefinedReference,
            "%include \"defs.inc\"\n    jmp helper\n"
        )
        .is_empty());
    }

    #[test]
    fn duplicate_global_label_flagged_but_local_ok() {
        assert_eq!(run_rule(&DuplicateLabel, "foo:\nfoo:\n").len(), 1);
        assert!(run_rule(&DuplicateLabel, ".loop:\n.loop:\n").is_empty());
    }

    #[test]
    fn unused_label_excludes_globals() {
        assert_eq!(run_rule(&UnusedLabel, "helper:\n    nop\n").len(), 1);
        assert!(run_rule(&UnusedLabel, "global _start\n_start:\n    nop\n").is_empty());
    }

    #[test]
    fn unused_extern_flagged() {
        assert_eq!(run_rule(&UnusedExtern, "extern printf\n    nop\n").len(), 1);
        assert!(run_rule(&UnusedExtern, "extern printf\n    call printf\n").is_empty());
    }

    #[test]
    fn undefined_global_flagged() {
        assert_eq!(
            run_rule(&UndefinedGlobal, "global _start\n    nop\n").len(),
            1
        );
        assert!(run_rule(&UndefinedGlobal, "global _start\n_start:\n    nop\n").is_empty());
    }
}
