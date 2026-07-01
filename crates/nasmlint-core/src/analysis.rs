//! The analysis model handed to rules.
//!
//! [`Model`] owns the results of the front end (parsed program + symbol/macro
//! tables); [`Analysis`] is the borrowed view a [`Rule`](crate::rules::Rule)
//! sees. Splitting ownership from the view means the model is built once per file
//! and every rule shares it — no rule re-parses.
//!
//! The model grows additively across milestones. Rules only read the fields they
//! need, so adding a field (the CFG at M4) never breaks an existing rule.

use crate::ast::Program;
use crate::cfg::Cfg;
use crate::parser::parse;
use crate::source::SourceFile;
use crate::symbols::{resolve, MacroTable, SymbolTable};

/// Everything the front end derives from one source file.
pub struct Model {
    pub program: Program,
    pub symbols: SymbolTable,
    pub macros: MacroTable,
    pub cfg: Cfg,
}

impl Model {
    /// Run the front end over a file: tokenize → parse → resolve → build CFG.
    pub fn build(file: &SourceFile) -> Model {
        let program = parse(&file.text);
        let (symbols, macros) = resolve(&program);
        let globals: Vec<String> = symbols.globals.iter().map(|g| g.name.clone()).collect();
        let cfg = Cfg::build(&program, &globals);
        Model {
            program,
            symbols,
            macros,
            cfg,
        }
    }
}

/// Read-only view of everything a rule may inspect for one file.
pub struct Analysis<'a> {
    pub file: &'a SourceFile,
    pub program: &'a Program,
    pub symbols: &'a SymbolTable,
    pub macros: &'a MacroTable,
    pub cfg: &'a Cfg,
}

impl<'a> Analysis<'a> {
    /// Borrow a view over `file` backed by `model`.
    pub fn new(file: &'a SourceFile, model: &'a Model) -> Self {
        Analysis {
            file,
            program: &model.program,
            symbols: &model.symbols,
            macros: &model.macros,
            cfg: &model.cfg,
        }
    }
}
