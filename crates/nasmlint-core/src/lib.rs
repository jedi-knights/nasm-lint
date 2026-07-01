//! `nasmlint-core` — the analysis engine behind nasm-lint.
//!
//! This crate is pure: it does no filesystem or network I/O and knows nothing
//! about CLIs, SARIF, or LSP. Callers load a [`SourceFile`], hand it and a
//! [`Config`] to [`analyze`], and render the returned [`Diagnostic`]s however
//! they like. That separation is what lets the CLI and the editor language
//! server share one implementation of every rule.
//!
//! Pipeline (grows with each milestone): source → (lex → parse → resolve → CFG)
//! → run rules → diagnostics. See `engine::analyze`.

pub mod analysis;
pub mod ast;
pub mod cfg;
pub mod config;
pub mod diagnostics;
pub mod engine;
pub mod insns;
pub mod keywords;
pub mod lexer;
pub mod parser;
pub mod rules;
pub mod source;
pub mod symbols;

pub use analysis::{Analysis, Model};
pub use config::Config;
pub use diagnostics::{Diagnostic, Severity, Span};
pub use engine::analyze;
pub use source::SourceFile;
