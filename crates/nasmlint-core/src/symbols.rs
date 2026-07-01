//! Symbol and macro tables built from the parsed [`Program`].
//!
//! These are the inputs the structural rules (M2) consume: label resolution
//! (undefined / unused / duplicate), `global`/`extern` reconciliation, and
//! macro-definition tracking. Building them is a single pass over the AST — O(n)
//! in the number of lines, with hash-map lookups — so resolution never degrades
//! to a nested scan over the symbol set.

use std::collections::HashMap;

use crate::ast::{Ident, LineBody, Program};
use crate::diagnostics::Span;

/// Every symbol definition and reference discovered in a file.
#[derive(Debug, Default)]
pub struct SymbolTable {
    /// Label name → each site it is defined at. More than one site is a
    /// duplicate definition (NL003).
    pub definitions: HashMap<String, Vec<Span>>,
    /// Symbol references appearing in instruction/pseudo operands.
    pub references: Vec<Ident>,
    /// Symbols declared `global`.
    pub globals: Vec<Ident>,
    /// Symbols declared `extern` (or `common`) — defined elsewhere.
    pub externs: Vec<Ident>,
}

impl SymbolTable {
    /// Whether `name` is defined locally or declared external — i.e. a reference
    /// to it can be resolved within this translation unit's knowledge.
    pub fn is_known(&self, name: &str) -> bool {
        self.definitions.contains_key(name) || self.externs.iter().any(|e| e.name == name)
    }
}

/// Preprocessor definitions (`%define`, `%macro`, `%assign`, ...) keyed by name.
#[derive(Debug, Default)]
pub struct MacroTable {
    pub defines: HashMap<String, Span>,
}

/// Preprocessor keywords (without the leading `%`) that introduce a name.
fn keyword_defines_name(keyword: &str) -> bool {
    let kw = keyword.trim_start_matches('%').to_ascii_lowercase();
    matches!(
        kw.as_str(),
        "define"
            | "idefine"
            | "xdefine"
            | "ixdefine"
            | "assign"
            | "iassign"
            | "defstr"
            | "defalias"
            | "macro"
            | "imacro"
            | "rmacro"
            | "irmacro"
    )
}

/// Build the symbol and macro tables from a parsed program in one pass.
pub fn resolve(program: &Program) -> (SymbolTable, MacroTable) {
    let mut symbols = SymbolTable::default();
    let mut macros = MacroTable::default();

    for line in &program.lines {
        if let Some(label) = &line.label {
            symbols
                .definitions
                .entry(label.name.clone())
                .or_default()
                .push(label.span);
        }

        match &line.body {
            LineBody::Instruction(instr) => {
                for operand in &instr.operands {
                    symbols.references.extend(operand.idents.iter().cloned());
                }
            }
            LineBody::Pseudo(pseudo) => {
                for operand in &pseudo.operands {
                    symbols.references.extend(operand.idents.iter().cloned());
                }
            }
            LineBody::Directive(dir) => match dir.name.to_ascii_lowercase().as_str() {
                "global" => symbols.globals.extend(dir.args.iter().cloned()),
                "extern" | "common" => symbols.externs.extend(dir.args.iter().cloned()),
                _ => {} // e.g. `section .text` — the arg is a section name, not a symbol
            },
            LineBody::Preproc(pre) => {
                if let Some(name) = &pre.name {
                    if keyword_defines_name(&pre.keyword) {
                        macros.defines.entry(name.name.clone()).or_insert(name.span);
                    }
                }
            }
            LineBody::Empty | LineBody::Error => {}
        }
    }

    (symbols, macros)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn records_definitions_and_references() {
        let prog = parse("_start:\n    jmp _start\n");
        let (symbols, _) = resolve(&prog);
        assert!(symbols.definitions.contains_key("_start"));
        assert_eq!(symbols.references[0].name, "_start");
        assert!(symbols.is_known("_start"));
    }

    #[test]
    fn detects_duplicate_definition_sites() {
        let prog = parse("foo:\nfoo:\n");
        let (symbols, _) = resolve(&prog);
        assert_eq!(symbols.definitions["foo"].len(), 2);
    }

    #[test]
    fn separates_globals_and_externs() {
        let prog = parse("global _start\nextern printf\n");
        let (symbols, _) = resolve(&prog);
        assert_eq!(symbols.globals[0].name, "_start");
        assert_eq!(symbols.externs[0].name, "printf");
        assert!(symbols.is_known("printf")); // extern counts as known
    }

    #[test]
    fn collects_macro_definitions() {
        let prog = parse("%define WIDTH 80\n%macro prologue 0\n");
        let (_, macros) = resolve(&prog);
        assert!(macros.defines.contains_key("WIDTH"));
        assert!(macros.defines.contains_key("prologue"));
    }

    #[test]
    fn section_name_is_not_a_symbol() {
        let prog = parse("section .text\n");
        let (symbols, _) = resolve(&prog);
        assert!(symbols.globals.is_empty());
        assert!(symbols.externs.is_empty());
    }
}
