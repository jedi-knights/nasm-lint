# nasm-lint

[![CI](https://github.com/jedi-knights/nasm-lint/actions/workflows/ci.yml/badge.svg)](https://github.com/jedi-knights/nasm-lint/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)

A static code analysis tool (linter) for [NASM](https://www.nasm.us/) assembly. It
scans `.asm` source and reports correctness and style problems — usable as a
command-line tool, an editor language server, and an official GitHub Action that
annotates pull requests via SARIF.

> **Scope:** the NASM dialect only (not GAS or MASM), targeting x86 / x86-64.

## Table of contents

- [Why](#why)
- [Features](#features)
- [Status](#status)
- [Installation](#installation)
- [Usage](#usage)
  - [Command line](#command-line)
  - [GitHub Action](#github-action)
  - [Editor (LSP)](#editor-lsp)
- [Configuration](#configuration)
- [Severity model](#severity-model)
- [Rule catalog](#rule-catalog)
- [How it works](#how-it-works)
- [Roadmap](#roadmap)
- [Development](#development)
- [Contributing](#contributing)
- [License](#license)
- [Acknowledgements](#acknowledgements)

## Why

NASM is line-oriented with a well-defined lexical structure, which makes it a good
target for static analysis — yet there is no widely-used linter for it. `nasm-lint`
fills that gap, giving NASM projects the kind of automated, reviewable feedback that
clippy and ruff give Rust and Python: catch undefined labels, unbalanced
preprocessor blocks, suspicious instructions, and style drift before they reach a
build or a code review.

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
- **CI-native output** — human, JSON, and **SARIF 2.1.0** for GitHub code scanning.

## Status

Early development. The analysis engine, configuration, output renderers, and the
lexer/parser/symbol front end are in place; the full rule catalog, instruction
validation, control-flow analysis, and the language server are being built out
milestone by milestone (see [Roadmap](#roadmap) and `TODO.md`). Interfaces and rule
codes are stabilizing but may still change before `1.0`.

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

### Command line

```bash
nasmlint path/to/file.asm      # lint one file
nasmlint src/                  # walk a directory for .asm/.nasm/.s/.inc
nasmlint                       # lint the current directory
```

```
nasmlint [PATHS]...
  --format human|json|sarif    Output format (default: human)
  --config <FILE>              Config file (default: ./.nasmlint.toml)
  --max-severity must-fix|should-fix|consider
                               Exit non-zero when any finding is at least this
                               severe (default: must-fix)
```

Example output:

```
src/boot.asm:12:5: must-fix [NL001] undefined label `_strat`
src/boot.asm:40:1: should-fix [NL010] unbalanced %macro without %endmacro
src/boot.asm:7:15: consider [NL053] trailing whitespace

3 issue(s) found.
```

### GitHub Action

*(Planned, M6.)* Add the linter as a step and upload its SARIF so findings appear as
inline PR annotations:

```yaml
- uses: jedi-knights/nasm-lint@v1
  with:
    paths: src/
    format: sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

### Editor (LSP)

*(Planned, M5.)* A language server (`nasmlint-lsp`) will provide inline diagnostics
in any LSP-capable editor (Neovim, VS Code) using the same rules as the CLI.

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

Valid rule settings: `off`, `must-fix`, `should-fix`, `consider` (the aliases
`error`/`warning`/`note` are also accepted).

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

Rules are grouped by `NL0xx` code. Codes are permanent identifiers. Those marked
*(planned)* land in the milestone noted in the [Roadmap](#roadmap).

| Code range | Category      | Examples |
|------------|---------------|----------|
| `NL00x`    | Labels/symbols | undefined reference, duplicate/unused label, unresolved `global` |
| `NL01x`    | Preprocessor   | unbalanced `%macro`/`%if`/`%rep`, missing `%include`, unused macro |
| `NL02x`    | Sections       | instruction outside a section, write to non-writable section, unknown directive |
| `NL03x`    | Instructions *(planned)* | unknown mnemonic, operand count/size mismatch |
| `NL04x`    | Flow *(planned)* | dead code after `jmp`/`ret`, unbalanced `push`/`pop` |
| `NL05x`    | Style          | mixed tabs/spaces, inconsistent casing, magic numbers, `NL053` trailing whitespace |

Run `nasmlint --help` for the flags; the authoritative list of implemented codes is
in `crates/nasmlint-core/src/rules/`.

## How it works

`nasm-lint` is a Cargo workspace. All analysis lives in one pure library crate
(`nasmlint-core`) that does no I/O, so the CLI and the future language server share
every rule. The pipeline:

```
source → lex → parse → resolve symbols → [control-flow graph] → run rules → diagnostics
```

Each check is an independent `Rule` (the Strategy pattern), registered in one place.
Instruction validation is bootstrapped from NASM's machine-readable `insns.dat`
rather than a hand-coded table. See `CLAUDE.md` for the full architecture notes.

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

Live task status lives in `TODO.md`.

## Development

```bash
cargo build --workspace           # build
cargo test  --workspace --all-targets   # test
cargo clippy --workspace --all-targets -- -D warnings   # lint
cargo fmt --all --check           # format check
```

CI runs `fmt`, `clippy`, and `test` as parallel jobs on every push and pull
request.

## Contributing

Contributions are welcome. Please:

1. Keep each pull request to a single concern — one Conventional Commit
   `type(scope)` (e.g. `feat(core):`, `fix(cli):`, `docs:`). If describing your
   change needs the word "and", split it.
2. Add tests for new behavior; keep `fmt`, `clippy -D warnings`, and `test` clean.
3. When adding a rule, follow the checklist in `CLAUDE.md` (implement `Rule`,
   register it, pick the next free `NL0xx` code, document it here).

## License

Licensed under the [MIT License](LICENSE).

## Acknowledgements

- [The Netwide Assembler (NASM)](https://www.nasm.us/) — instruction data derived
  from NASM's `insns.dat` (BSD-2-Clause) is vendored for instruction validation;
  see [`THIRD_PARTY_LICENSES`](THIRD_PARTY_LICENSES).
- Built with [`logos`](https://github.com/maciejhirsz/logos),
  [`clap`](https://github.com/clap-rs/clap), and the Rust ecosystem.
