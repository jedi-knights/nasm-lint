//! The unit of analysis: one NASM source file loaded into memory.
//!
//! Rules receive a borrowed [`SourceFile`] (wrapped in `Analysis`) and never do
//! I/O themselves — loading is the caller's job (CLI walks the filesystem, the
//! LSP hands over the in-editor buffer). Keeping I/O out of the core is what lets
//! the same rules run unchanged in both interfaces.

use std::path::{Path, PathBuf};

/// A source file plus its line index.
///
/// Lines are pre-split once at construction (O(n)) so that line-oriented rules —
/// the majority, since NASM is line-oriented — do not each re-scan the text.
pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    /// Each entry is one line with its terminator stripped, in file order.
    lines: Vec<String>,
}

impl SourceFile {
    pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        let text = text.into();
        // `str::lines` drops the trailing empty element for a text ending in
        // '\n', which matches how editors number lines.
        let lines = text.lines().map(str::to_owned).collect();
        SourceFile {
            path: path.into(),
            text,
            lines,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// All lines, terminators stripped, 0-indexed. Diagnostics report 1-based
    /// line numbers, so add 1 when constructing a [`crate::Span`].
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}
