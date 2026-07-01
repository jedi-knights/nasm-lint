//! Turn the user's path arguments into the concrete set of source files to lint.
//!
//! A path may be a single file (linted regardless of extension — the user asked
//! for it explicitly) or a directory (walked for NASM-looking extensions). Ignore
//! globs from config are matched against the path relative to the current working
//! directory, mirroring how ruff and clippy interpret their excludes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

/// Extensions treated as NASM source during directory walks.
const NASM_EXTENSIONS: &[&str] = &["asm", "nasm", "s", "inc"];

/// Build a matcher from config ignore globs. An empty set matches nothing.
pub fn build_ignore(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).with_context(|| format!("invalid ignore glob: {pattern}"))?);
    }
    builder.build().context("failed to compile ignore globs")
}

fn has_nasm_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| NASM_EXTENSIONS.iter().any(|k| k.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

/// Expand `inputs` into the ordered, de-duplicated list of files to analyze.
pub fn collect_files(inputs: &[PathBuf], ignore: &GlobSet) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for input in inputs {
        if input.is_file() {
            if !ignore.is_match(input) {
                files.push(input.clone());
            }
            continue;
        }

        // Directory (or glob-expanded path): walk it for NASM sources.
        for entry in WalkDir::new(input).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if entry.file_type().is_file() && has_nasm_extension(path) && !ignore.is_match(path) {
                files.push(path.to_path_buf());
            }
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}
