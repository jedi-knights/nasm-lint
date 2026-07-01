# nasm-lint

A static code analysis tool (linter) for NASM assembly — CLI, editor language server, and GitHub Action with SARIF output.

[![CI](https://github.com/jedi-knights/nasm-lint/actions/workflows/ci.yml/badge.svg)](https://github.com/jedi-knights/nasm-lint/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Status](#status)
- [Requirements](#requirements)
- [Installation](#installation)
- [Usage](#usage)
- [Configuration](#configuration)
- [Severity model](#severity-model)
- [Rule catalog](#rule-catalog)
- [How it works](#how-it-works)
- [Development](#development)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)
- [Acknowledgements](#acknowledgements)

## Overview

NASM is line-oriented with a well-defined lexical structure, which makes it a good
target for static analysis — yet there is no widely-used linter for it. `nasm-lint`
fills that gap, giving NASM projects the kind of automated, reviewable feedback that
clippy and ruff give Rust and Python: catch undefined labels, unbalanced
preprocessor blocks, suspicious instructions, and style drift before they reach a
build or a code review. It targets the NASM dialect only (not GAS or MASM), for
x86 / x86-64.

## Features

- **Fast, single static binary** — no runtime, trivial to drop into CI.
- **One analysis core, three front ends** — the CLI and the editor language server
  share the exact same rules, so findings are identical everywhere.
- **Structural analysis** — label resolution (undefined / unused / duplicate),
  `%macro`/`%if`/`%rep` balance, `section`/`global`/`extern` checks.
- **Instruction-aware checks** *(planned, M3)* — mnemonic and operand-form
  validation driven by NASM's own `insns.dat`.
- **Control-flow analysis** *(planned, M4)* — dead code, stack push/pop balance.
- **Configurable** — enable/disable rules and override severities via
  `.nasmlint.toml`.
- **CI-native output** — human, JSON, and SARIF 2.1.0 for GitHub code scanning.

## Status

Early development. The analysis engine, configuration, output renderers, and the
lexer/parser/symbol front end are in place; the full rule catalog, instruction
validation, control-flow analysis, and the language server are being built out
milestone by milestone (see [Roadmap](#roadmap) and the `TODO.md` file). Interfaces
and rule codes are stabilizing but may still change before `1.0`.

## Requirements

- **Rust** 1.85 or newer (2021 edition) and Cargo — to build or install from source.
- **NASM** — optional, and only for the planned `--preprocess` mode that shells out
  to `nasm -E`. The linter itself does not require NASM to be installed.

## Installation

### From source

```bash
git clone https://github.com/jedi-knights/nasm-lint
cd nasm-lint
cargo build --release
# binary at ./target/release/nasmlint
```

### With cargo

```bash
cargo install --path crates/nasmlint-cli
```

Prebuilt cross-platform binaries and `cargo install nasmlint` from crates.io are
planned for the `1.0` release.

## Usage

Lint a file, a directory, or the current directory:

```bash
nasmlint path/to/file.asm      # lint one file
nasmlint src/                  # walk a directory for .asm/.nasm/.s/.inc
nasmlint                       # lint the current directory
```

Flags:

```bash
nasmlint [PATHS]... \
  --format human|json|sarif \
  --config ./.nasmlint.toml \
  --max-severity must-fix|should-fix|consider
```

Example output:

```text
src/boot.asm:12:5: must-fix [NL001] undefined label `_strat`
src/boot.asm:40:1: should-fix [NL010] unbalanced %macro without %endmacro
src/boot.asm:7:15: consider [NL053] trailing whitespace

3 issue(s) found.
```

### GitHub Action

The composite action is planned for M6. It will run the linter and emit SARIF so
findings appear as inline pull-request annotations:

```yaml
- uses: jedi-knights/nasm-lint@v1
  with:
    paths: src/
    format: sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

### Editor language server

The language server (`nasmlint-lsp`) is planned for M5. It will provide inline
diagnostics in any LSP-capable editor (Neovim, VS Code) using the same rules as the
CLI.

## Configuration

Drop a `.nasmlint.toml` in your project root (or point at one with `--config`):

```toml
# Disable or re-grade rules by NL0xx code.
[rules]
NL053 = "off"        # stop flagging trailing whitespace
NL050 = "consider"   # downgrade tabs/spaces mix to advisory
NL001 = "must-fix"   # keep undefined labels as errors

# Paths excluded from analysis (glob syntax).
ignore = ["vendor/**", "third_party/**"]
```

Valid rule settings are `off`, `must-fix`, `should-fix`, and `consider`; the aliases
`error`, `warning`, and `note` are also accepted. Defaults: all rules enabled at
their built-in severity, no paths ignored, config discovered at `./.nasmlint.toml`.

## Severity model

Three buckets, used consistently across every output format:

| Bucket        | Meaning                                        | SARIF level |
|---------------|------------------------------------------------|-------------|
| `must-fix`    | Correctness failure — breaks under real input  | `error`     |
| `should-fix`  | Drift — fragile or inconsistent over time      | `warning`   |
| `consider`    | Non-blocking improvement                       | `note`      |

The `--max-severity` gate uses these to decide the process exit code, so you can
fail CI on `must-fix` only, or tighten it to `should-fix`.

## Rule catalog

Rules are grouped by `NL0xx` code. Codes are permanent identifiers. Entries marked
*(planned)* land in the milestone noted in the [Roadmap](#roadmap).

| Code range | Category      | Examples |
|------------|---------------|----------|
| `NL00x`    | Labels/symbols | undefined reference, duplicate/unused label, unresolved `global` |
| `NL01x`    | Preprocessor   | unbalanced `%macro`/`%if`/`%rep`, missing `%include`, unused macro |
| `NL02x`    | Sections       | instruction outside a section, write to non-writable section, unknown directive |
| `NL03x`    | Instructions *(planned)* | unknown mnemonic, operand count/size mismatch |
| `NL04x`    | Flow *(planned)* | dead code after `jmp`/`ret`, unbalanced `push`/`pop` |
| `NL05x`    | Style          | mixed tabs/spaces, inconsistent casing, magic numbers, `NL053` trailing whitespace |

The authoritative list of implemented codes lives in
`crates/nasmlint-core/src/rules/`.

## How it works

`nasm-lint` is a Cargo workspace. All analysis lives in one pure library crate
(`nasmlint-core`) that does no I/O, so the CLI and the future language server share
every rule. The pipeline:

```text
source -> lex -> parse -> resolve symbols -> [control-flow graph] -> run rules -> diagnostics
```

Each check is an independent `Rule` (the Strategy pattern), registered in one place.
Instruction validation is bootstrapped from NASM's machine-readable `insns.dat`
rather than a hand-coded table. See the `CLAUDE.md` file for full architecture notes.

## Development

```bash
cargo build --workspace                                  # build
cargo test  --workspace --all-targets                    # test
cargo clippy --workspace --all-targets -- -D warnings    # lint
cargo fmt --all --check                                  # format check
```

CI runs `fmt`, `clippy`, and `test` as parallel jobs on every push and pull request.

## Roadmap

| Milestone | Scope |
|-----------|-------|
| **M0** | Workspace scaffold, rule engine, config, human/JSON/SARIF renderers |
| **M1** | Lexer, tolerant parser, symbol/macro tables |
| **M2** | Structural rule catalog (labels, preprocessor, sections, style) |
| **M3** | Instruction-aware rules from vendored `insns.dat` |
| **M4** | Control-flow graph and flow rules; `--preprocess` |
| **M5** | Editor language server (`nasmlint-lsp`) |
| **M6** | GitHub Action, cross-platform release binaries, Marketplace listing |

Live task status lives in the `TODO.md` file.

## Contributing

Contributions are welcome. Please:

1. Keep each pull request to a single concern — one Conventional Commit
   `type(scope)` (e.g. `feat(core):`, `fix(cli):`, `docs:`). If describing your
   change needs the word "and", split it.
2. Add tests for new behavior; keep `fmt`, `clippy -D warnings`, and `test` clean.
3. When adding a rule, follow the checklist in the `CLAUDE.md` file (implement
   `Rule`, register it, pick the next free `NL0xx` code, document it here).

## License

Licensed under the MIT License; see the [LICENSE](LICENSE) file.

## Acknowledgements

- [The Netwide Assembler (NASM)](https://www.nasm.us/) — instruction data derived
  from NASM's `insns.dat` (BSD-2-Clause) is vendored for instruction validation;
  see the [THIRD_PARTY_LICENSES](THIRD_PARTY_LICENSES) file.
- Built with [logos](https://github.com/maciejhirsz/logos),
  [clap](https://github.com/clap-rs/clap), and the Rust ecosystem.
