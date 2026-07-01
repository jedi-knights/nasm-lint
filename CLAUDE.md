# CLAUDE.md — nasm-lint

Project-specific guidance for Claude Code working in this repository. Read this
before making changes; it captures the *why* behind the structure so you don't
have to re-derive it.

## What this is

A static code analysis tool (linter) for **NASM assembly** (x86 / x86-64 — not
GAS or MASM). It scans `.asm` source and reports correctness and style problems.
It ships three ways from one analysis core:

- a **CLI** (`nasmlint`),
- an editor **LSP** (planned, M5),
- an official **GitHub Action** (composite, SARIF output — planned, M6).

**Who it's for (ICP):** developers and CI pipelines that build NASM sources and
want automated, reviewable feedback — the same role clippy/ruff play for Rust and
Python. Keep that audience in mind before adding surface aimed at anyone else.

## Architecture

Cargo **workspace**. The analysis logic lives in one pure crate so the CLI and the
future LSP share every rule — never fork analysis logic into an interface crate.

```
crates/
  nasmlint-core/   pure library — NO filesystem/network I/O, knows nothing of CLI/SARIF/LSP
  nasmlint-cli/    clap CLI, file discovery, human/JSON/SARIF renderers
  nasmlint-lsp/    (M5) tower-lsp server over the same core
```

**Core pipeline** (`nasmlint-core`), grows one stage per milestone:

```
source → lex (lexer.rs) → parse (parser.rs) → resolve (symbols.rs) → [CFG @ M4] → run rules → diagnostics
         └─────────────── Model::build (analysis.rs) ───────────────┘        └ engine.rs ┘
```

- `diagnostics.rs` — `Severity` (three buckets), `Span`, `Diagnostic`. The
  authoritative severity vocabulary; renderers map onto it, never around it.
- `source.rs` — `SourceFile` (text + pre-split lines). Callers do the I/O.
- `config.rs` — `.nasmlint.toml`: rule on/off, severity override, ignore globs.
- `lexer.rs` — `logos` tokenizer; newlines are significant; totally lossless
  (`Unknown` catch-all) so lexing never fails.
- `ast.rs` — line-oriented model (`Program`/`Line`/`LineBody`).
- `keywords.rs` — directive/pseudo-op/register/size/prefix sets as O(1) `HashSet`s.
- `symbols.rs` — single-pass symbol + macro tables.
- `analysis.rs` — `Model` (owns front-end output) + `Analysis` (borrowed view rules see).
- `rules/` — the **Strategy pattern**: one `Rule` impl per check, registered in
  `builtin_rules()`.
- `engine.rs` — orchestrates build → run rules → grade severity → sort.

## Conventions

- **Rust edition 2021**, `rust-version` pinned in the workspace manifest.
- **Rule codes are `NL0xx`**, grouped: `NL00x` labels, `NL01x` preprocessor,
  `NL02x` sections/directives, `NL03x` instructions, `NL04x` flow, `NL05x` style.
  A code is a permanent identifier (config key + SARIF `ruleId`) — **never reuse a
  code for a different check.**
- **Three severity buckets only** — `MustFix` / `ShouldFix` / `Consider` — mapped
  to SARIF `error` / `warning` / `note`. Do not introduce other severity names.
- **`Analysis` grows additively.** Add fields (e.g. the CFG at M4); never change
  existing ones out from under rules. A rule reads only what it needs.
- **`keywords.rs` is not `insns.dat`.** Its sets are a pragmatic subset for
  structural analysis. Instruction-level completeness comes from vendoring NASM's
  `insns.dat` at M3 — do not grow these lists into a hand-maintained copy of it.
- **Complexity:** front-end passes are single-pass, hash-map backed (O(n)). The
  column tracker in `lexer.rs` is deliberately O(n) (advances by inter-token gap,
  not a rescan) — keep it that way; the comment there is the anti-regression anchor.

## Adding a rule

1. Implement `Rule` in the matching `rules/<category>.rs` (create the file if the
   category is new); pick the next free `NL0xx` code.
2. Register it in `builtin_rules()` (`rules/mod.rs`).
3. Add unit tests in the rule module and, once fixtures exist, a
   `tests/fixtures/*.asm` case.
4. Document the code + default severity in the README rule catalog.

## Testing & pre-flight

```
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

All three must be clean before committing (CI runs them as parallel jobs). Use
`insta` snapshot tests over `tests/fixtures/` for diagnostic output as the rule set
grows.

## Git & PR discipline

- **Conventional Commits**: `type(scope): description` (lowercase, imperative).
- **One PR = one `type(scope)`.** If describing the change needs "and", split it.
  Milestones ship as their own `feat(core)` PRs; CI changes as `ci`; docs as `docs`.
- Branch names mirror the commit: `feat/…`, `ci/…`, `docs/…`.
- `main` is protected — land changes via PR, not direct commits.

## Roadmap (see `TODO.md` for live status)

M0 scaffold ✔ · M1 front end · M2 structural rules · M3 instruction-aware
(`insns.dat`) · M4 CFG/flow · M5 LSP · M6 GitHub Action + release binaries.

## Non-obvious decisions

- **Lint raw source, not preprocessed output.** Source-level issues are what users
  fix. `--preprocess` (shelling out to `nasm -E`) is an opt-in view, added later.
- **Label vs mnemonic is heuristic until M3.** `parser.rs` documents the two
  unambiguous cases it recognizes; full disambiguation needs the mnemonic table.
- **`logos` pinned at 0.14** (0.16 exists) to avoid a derive-API migration
  mid-build; revisit when convenient.
