# nasm-lint

A static code analysis tool for [NASM](https://www.nasm.us/) assembly. It scans
`.asm` source and reports correctness and style problems — usable as a CLI, an
editor language server, and an official GitHub Action that annotates pull
requests via SARIF.

> **Scope:** NASM dialect only (not GAS or MASM). x86 / x86-64.

## Status

Early development. The current milestone (**M0**) establishes the workspace, the
rule engine, config loading, and the three output renderers, with one rule
(`NL053`, trailing whitespace) wired end to end as the pipeline smoke test.
Subsequent milestones add the lexer/parser, the full rule catalog, instruction
validation from NASM's `insns.dat`, control-flow analysis, and the language
server. See `TODO.md` for the roadmap.

## Install & run

```bash
# From a clone of this repo:
cargo run -p nasmlint-cli -- path/to/file.asm

# Or build the release binary:
cargo build --release
./target/release/nasmlint src/
```

## Usage

```
nasmlint [PATHS]...                 # files or directories (default: .)
  --format human|json|sarif         # output format (default: human)
  --config <FILE>                   # config file (default: ./.nasmlint.toml)
  --max-severity must-fix|should-fix|consider
                                    # exit non-zero when any finding is at least
                                    # this severe (default: must-fix)
```

## Configuration

Drop a `.nasmlint.toml` in your project root:

```toml
# Disable or re-grade rules by NL0xx code.
[rules]
NL053 = "off"        # stop flagging trailing whitespace
NL050 = "consider"   # downgrade tabs/spaces mix to advisory

# Paths excluded from analysis (glob syntax).
ignore = ["vendor/**", "third_party/**"]
```

Severity levels are `must-fix`, `should-fix`, and `consider` (the same three
buckets used everywhere in the output; SARIF maps them to `error` / `warning` /
`note`).

## Severity model

| Bucket        | Meaning                                   | SARIF level |
|---------------|-------------------------------------------|-------------|
| `must-fix`    | Correctness failure — breaks under real input | `error`  |
| `should-fix`  | Drift — fragile or inconsistent over time | `warning`   |
| `consider`    | Non-blocking improvement                  | `note`      |

## License

[MIT](LICENSE). Bundles NASM's `insns.dat` under its BSD-2-Clause license — see
[`THIRD_PARTY_LICENSES`](THIRD_PARTY_LICENSES).
