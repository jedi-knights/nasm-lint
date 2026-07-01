//! Output renderers. Each takes the full set of `(file, diagnostics)` results and
//! writes one representation:
//!
//! - `human`  — clickable `path:line:col` lines for terminals.
//! - `json`   — a flat array, easy to post-process in scripts.
//! - `sarif`  — SARIF 2.1.0 for GitHub code scanning / PR annotations.
//!
//! The SARIF shape is hand-built (rather than pulling a SARIF crate) to keep the
//! dependency surface small and the exact schema under our control; it is
//! validated against the 2.1.0 schema in CI.

use std::path::Path;

use nasmlint_core::{Diagnostic, Severity};
use serde_json::{json, Value};

/// One analyzed file and its findings.
pub struct FileResult<'a> {
    pub path: &'a Path,
    pub diagnostics: Vec<Diagnostic>,
}

/// Human-readable, one finding per line: `path:line:col: severity[CODE] message`.
pub fn human(results: &[FileResult<'_>]) -> String {
    let mut out = String::new();
    let mut total = 0usize;
    for result in results {
        for d in &result.diagnostics {
            total += 1;
            out.push_str(&format!(
                "{}:{}:{}: {} [{}] {}\n",
                result.path.display(),
                d.span.line,
                d.span.column,
                d.severity.label(),
                d.code,
                d.message,
            ));
        }
    }
    if total == 0 {
        out.push_str("No issues found.\n");
    } else {
        out.push_str(&format!("\n{total} issue(s) found.\n"));
    }
    out
}

/// Flat JSON array of findings, each tagged with its file path.
pub fn json(results: &[FileResult<'_>]) -> String {
    let items: Vec<Value> = results
        .iter()
        .flat_map(|r| {
            r.diagnostics.iter().map(move |d| {
                json!({
                    "file": r.path.display().to_string(),
                    "code": d.code,
                    "severity": d.severity,
                    "line": d.span.line,
                    "column": d.span.column,
                    "endColumn": d.span.end_column,
                    "message": d.message,
                })
            })
        })
        .collect();
    serde_json::to_string_pretty(&items).expect("serializable")
}

/// SARIF 2.1.0 log. Rule metadata is emitted for every code that appears in the
/// results so consumers can render descriptions alongside findings.
pub fn sarif(results: &[FileResult<'_>]) -> String {
    use std::collections::BTreeMap;

    // Deduplicate rule descriptors by code, preserving first-seen message as a
    // stand-in description until richer metadata is wired through (M2+).
    let mut rules: BTreeMap<&str, &str> = BTreeMap::new();
    let mut sarif_results = Vec::new();

    for file in results {
        for d in &file.diagnostics {
            rules.entry(d.code).or_insert(d.message.as_str());
            sarif_results.push(json!({
                "ruleId": d.code,
                "level": d.severity.sarif_level(),
                "message": { "text": d.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": file.path.display().to_string() },
                        "region": {
                            "startLine": d.span.line,
                            "startColumn": d.span.column,
                            "endColumn": d.span.end_column,
                        }
                    }
                }],
            }));
        }
    }

    let rule_descriptors: Vec<Value> = rules
        .iter()
        .map(|(code, desc)| json!({ "id": code, "shortDescription": { "text": desc } }))
        .collect();

    let log = json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "nasm-lint",
                    "informationUri": "https://github.com/jedi-knights/nasm-lint",
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": rule_descriptors,
                }
            },
            "results": sarif_results,
        }],
    });
    serde_json::to_string_pretty(&log).expect("serializable")
}

/// The most severe finding across all results, if any (drives the exit code).
pub fn max_severity(results: &[FileResult<'_>]) -> Option<Severity> {
    results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .map(|d| d.severity)
        .min() // Severity ordering is reversed: MustFix is the minimum.
}
