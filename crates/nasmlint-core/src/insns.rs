//! Instruction table bootstrapped from NASM's vendored `insns.dat`.
//!
//! `insns.dat` is NASM's own machine-readable instruction list (BSD-2-Clause,
//! pinned to NASM 3.02 — see `THIRD_PARTY_LICENSES`). Vendoring it means the
//! unknown-mnemonic and operand-count rules track the real ISA instead of a
//! hand-maintained list that would inevitably drift.
//!
//! Each functional line has four whitespace-separated fields:
//! `MNEMONIC  operands  [encoding]  flags`. We need only the first two: the
//! mnemonic and how many operands the form takes (the operand field is
//! comma-separated, or `void` for none, or `ignore` for the variadic pseudo-ops).
//!
//! ## Condition-code templates
//!
//! Entries like `Jcc` / `SETcc` use a lowercase `cc` marker that NASM expands into
//! every condition-code alias (`je`, `jne`, `jl`, ...). Real mnemonics are all
//! uppercase, so `PFACC` (uppercase `CC`) is left alone while `SETcc` is expanded.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// The condition-code aliases NASM substitutes for a `cc` template, covering all
/// 16 conditions with their synonyms (so `je`, `jz`, `jnz`, `jge`, ... all resolve).
const CONDITIONS: &[&str] = &[
    "o", "no", "b", "c", "nae", "ae", "nb", "nc", "e", "z", "ne", "nz", "be", "na", "a", "nbe",
    "s", "ns", "p", "pe", "np", "po", "l", "nge", "ge", "nl", "le", "ng", "g", "nle",
];

/// What operand arities a mnemonic accepts.
#[derive(Default)]
struct MnemonicInfo {
    /// The distinct operand counts seen across all encodings of this mnemonic.
    arities: HashSet<usize>,
    /// True if any form is variadic (`ignore` operand field — the `db`-style
    /// pseudo-ops), meaning operand count is not constrained.
    any_arity: bool,
}

/// Mnemonic → accepted operand arities, keyed by lowercase name.
pub struct InstructionTable {
    forms: HashMap<String, MnemonicInfo>,
}

impl InstructionTable {
    fn parse(data: &str) -> InstructionTable {
        let mut forms: HashMap<String, MnemonicInfo> = HashMap::new();

        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }

            // Group-macro lines (`$arith`, `$shift`) list a family of mnemonics on
            // one bracket-less line rather than as individual instruction rows
            // (e.g. `$arith  ADD OR ADC ... CMP`). Extract the bare uppercase
            // mnemonic tokens; their arity is not expressed here, so register them
            // as variadic (any operand count) so NL031 does not false-fire.
            if line.starts_with('$') && !line.contains('[') {
                for token in line.split_whitespace().skip(1) {
                    for name in token.split(',').filter(|s| is_group_mnemonic(s)) {
                        forms
                            .entry(name.to_ascii_lowercase())
                            .or_default()
                            .any_arity = true;
                    }
                }
                continue;
            }

            let mut fields = line.split_whitespace();
            let Some(first) = fields.next() else { continue };
            // Many lines are prefixed by an insns.dat size-macro (`$bwdq`, `$zwdq`,
            // ...) that generates size variants; the real mnemonic is the token
            // after it. Skip the prefix.
            let mnemonic = if first.starts_with('$') {
                match fields.next() {
                    Some(m) => m,
                    None => continue,
                }
            } else {
                first
            };
            let Some(operands) = fields.next() else {
                continue;
            };
            // Functional lines have an alphabetic mnemonic.
            if !mnemonic
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            {
                continue;
            }

            let arity = match operands {
                "void" => Some(0),
                "ignore" => None, // variadic pseudo-op
                other => Some(other.split(',').count()),
            };

            for name in expand_mnemonic(mnemonic) {
                let info = forms.entry(name).or_default();
                match arity {
                    Some(n) => {
                        info.arities.insert(n);
                    }
                    None => info.any_arity = true,
                }
            }
        }

        InstructionTable { forms }
    }

    /// Whether `mnemonic` is a known instruction (case-insensitive).
    pub fn contains(&self, mnemonic: &str) -> bool {
        self.forms.contains_key(&mnemonic.to_ascii_lowercase())
    }

    /// Whether `mnemonic` accepts `n` operands. `None` if the mnemonic is unknown
    /// (that is the unknown-mnemonic rule's job, not this one's).
    pub fn accepts_arity(&self, mnemonic: &str, n: usize) -> Option<bool> {
        self.forms
            .get(&mnemonic.to_ascii_lowercase())
            .map(|info| info.any_arity || info.arities.contains(&n))
    }

    /// The sorted operand arities a known mnemonic accepts (empty if variadic-only).
    pub fn arities(&self, mnemonic: &str) -> Option<Vec<usize>> {
        self.forms.get(&mnemonic.to_ascii_lowercase()).map(|info| {
            let mut v: Vec<usize> = info.arities.iter().copied().collect();
            v.sort_unstable();
            v
        })
    }
}

/// Whether `token` is a bare uppercase mnemonic on a group-macro line (rejecting
/// flag/placeholder tokens like `nf=`, `!evex`, `evex=0`, and the `-` gap marker).
fn is_group_mnemonic(token: &str) -> bool {
    token.len() >= 2
        && token.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Size-suffix letters a `%` mnemonic placeholder can expand to (e.g. `RET%` →
/// `ret`/`retw`/`retd`/`retq`). Over-generating a few nonsense names (e.g. `retb`)
/// is harmless — it only means we would fail to flag one bogus mnemonic — whereas
/// *missing* a real variant would be a false positive on valid code.
const SIZE_SUFFIXES: &[&str] = &["w", "d", "q", "b"];

/// Expand a mnemonic token from `insns.dat` into every concrete mnemonic it
/// stands for. Two templating mechanisms compose:
///
/// - a lowercase `cc` marker (`Jcc`, `SETcc`) → each condition-code alias;
/// - a trailing `%` size placeholder (`RET%`) → the bare stem plus size suffixes.
///
/// All-uppercase real mnemonics such as `PFACC` contain no lowercase `cc` and no
/// `%`, so they pass through unchanged (lowercased).
fn expand_mnemonic(mnemonic: &str) -> Vec<String> {
    let cc_variants = if mnemonic.contains("cc") {
        let lower = mnemonic.to_ascii_lowercase();
        CONDITIONS
            .iter()
            .map(|cc| lower.replacen("cc", cc, 1))
            .collect()
    } else {
        vec![mnemonic.to_ascii_lowercase()]
    };

    let mut out = Vec::new();
    for variant in cc_variants {
        if variant.contains('%') {
            let stem: String = variant.chars().filter(|c| *c != '%').collect();
            out.push(stem.clone());
            out.extend(SIZE_SUFFIXES.iter().map(|s| format!("{stem}{s}")));
        } else {
            out.push(variant);
        }
    }
    out
}

/// The parsed instruction table, built once from the vendored `insns.dat`.
pub fn table() -> &'static InstructionTable {
    static TABLE: OnceLock<InstructionTable> = OnceLock::new();
    TABLE.get_or_init(|| InstructionTable::parse(include_str!("../vendor/insns.dat")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_mnemonics_are_known() {
        let t = table();
        for m in ["mov", "MOV", "ret", "push", "lea", "syscall", "nop", "int"] {
            assert!(t.contains(m), "{m} should be a known mnemonic");
        }
    }

    #[test]
    fn condition_codes_are_expanded() {
        let t = table();
        // Jcc / SETcc expansion must yield the concrete conditional mnemonics.
        for m in ["je", "jne", "jz", "jnz", "jge", "jl", "jg", "seto", "setne"] {
            assert!(t.contains(m), "{m} should resolve via cc expansion");
        }
    }

    #[test]
    fn group_macro_mnemonics_are_known() {
        let t = table();
        // Arithmetic group ($arith) and shift group ($shift) are declared on
        // bracket-less macro lines, not as individual rows.
        for m in [
            "add", "or", "adc", "sbb", "and", "sub", "xor", "cmp", "shl", "sar", "rol",
        ] {
            assert!(t.contains(m), "{m} should be known via a group macro");
        }
    }

    #[test]
    fn uppercase_cc_mnemonic_is_not_expanded() {
        // PFACC is a real 3DNow! mnemonic, not a template.
        assert!(table().contains("pfacc"));
    }

    #[test]
    fn nonsense_is_unknown() {
        assert!(!table().contains("mxv"));
        assert!(!table().contains("notarealinstruction"));
    }

    #[test]
    fn arity_checks() {
        let t = table();
        assert_eq!(t.accepts_arity("mov", 2), Some(true));
        assert_eq!(t.accepts_arity("mov", 1), Some(false));
        assert_eq!(t.accepts_arity("ret", 0), Some(true)); // ret and ret imm16
        assert_eq!(t.accepts_arity("nosuchinsn", 2), None);
    }
}
