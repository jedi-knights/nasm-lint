//! The analysis model handed to rules.
//!
//! [`Model`] owns the results of the front end (parsed program + symbol/macro
//! tables); [`Analysis`] is the borrowed view a [`Rule`](crate::rules::Rule)
//! sees. Splitting ownership from the view means the model is built once per file
//! and every rule shares it — no rule re-parses.
//!
//! The model grows additively across milestones (the CFG lands at M4). Rules only
//! read the fields they need, so adding a field never breaks an existing rule.

use crate::ast::Program;
use crate::parser::parse;
use crate::source::SourceFile;
use crate::symbols::{resolve, MacroTable, SymbolTable};

/// Everything the front end derives from one source file.
pub struct Model {
    pub program: Program,
    pub symbols: SymbolTable,
    pub macros: MacroTable,
}

impl Model {
    /// Run the front end over a file: tokenize → parse → resolve.
    pub fn build(file: &SourceFile) -> Model {
        let program = parse(&file.text);
        let (symbols, macros) = resolve(&program);
        Model {
            program,
            symbols,
            macros,
        }
    }
}

/// Read-only view of everything a rule may inspect for one file.
pub struct Analysis<'a> {
    pub file: &'a SourceFile,
    pub program: &'a Program,
    pub symbols: &'a SymbolTable,
    pub macros: &'a MacroTable,
}

impl<'a> Analysis<'a> {
    /// Borrow a view over `file` backed by `model`.
    pub fn new(file: &'a SourceFile, model: &'a Model) -> Self {
        Analysis {
            file,
            program: &model.program,
            symbols: &model.symbols,
            macros: &model.macros,
        }
    }
}
