//! Case-insensitive keyword sets used by the parser to classify lines and to
//! decide which identifiers in an operand are *symbol references* versus
//! registers or size specifiers.
//!
//! These are membership tests on the hot parse path, so each set is built once
//! into a `HashSet` (O(1) lookup) rather than scanned linearly per token.
//!
//! The sets are intentionally not exhaustive — they cover the common x86-64
//! surface enough for structural analysis (M2). Instruction-level completeness
//! comes from NASM's `insns.dat` at M3; these lists are not that table and should
//! not grow into a hand-maintained copy of it.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Assembler directives (not CPU instructions).
const DIRECTIVES: &[&str] = &[
    "section", "segment", "global", "extern", "common", "bits", "default", "cpu", "org",
    "absolute", "align", "alignb", "group", "use16", "use32", "use64", "static", "export",
    "import", "required",
];

/// Data-definition, reservation, and assembly pseudo-instructions.
const PSEUDO_OPS: &[&str] = &[
    "db", "dw", "dd", "dq", "dt", "do", "dy", "dz", "resb", "resw", "resd", "resq", "rest", "reso",
    "resy", "resz", "equ", "times", "incbin",
];

/// Instruction prefixes that precede a mnemonic on the same line (e.g.
/// `rep movsb`, `lock xadd`). The parser skips a leading prefix so the following
/// mnemonic is classified correctly.
const PREFIXES: &[&str] = &[
    "rep", "repe", "repz", "repne", "repnz", "lock", "wait", "bnd", "notrack", "xacquire",
    "xrelease",
];

/// Size / distance / relocation keywords that may appear inside operands and must
/// not be mistaken for symbol references.
const SIZE_KEYWORDS: &[&str] = &[
    "byte", "word", "dword", "qword", "tword", "oword", "yword", "zword", "ptr", "near", "far",
    "short", "long", "rel", "abs", "strict", "nosplit", "to",
];

/// A representative x86-64 register set. Case-insensitive.
const REGISTERS: &[&str] = &[
    // 64-bit
    "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15", // 32-bit
    "eax", "ebx", "ecx", "edx", "esi", "edi", "ebp", "esp", "r8d", "r9d", "r10d", "r11d", "r12d",
    "r13d", "r14d", "r15d", // 16-bit
    "ax", "bx", "cx", "dx", "si", "di", "bp", "sp", "r8w", "r9w", "r10w", "r11w", "r12w", "r13w",
    "r14w", "r15w", // 8-bit
    "al", "bl", "cl", "dl", "ah", "bh", "ch", "dh", "sil", "dil", "bpl", "spl", "r8b", "r9b",
    "r10b", "r11b", "r12b", "r13b", "r14b", "r15b",
    // segment / instruction pointer / flags
    "cs", "ds", "es", "fs", "gs", "ss", "rip", "eip", "ip",
    // x87 / SIMD (representative)
    "st0", "st1", "st2", "st3", "st4", "st5", "st6", "st7", "mm0", "mm1", "mm2", "mm3", "mm4",
    "mm5", "mm6", "mm7", "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "ymm0",
    "ymm1", "ymm2", "ymm3", "ymm4", "ymm5", "ymm6", "ymm7", "zmm0", "zmm1", "zmm2", "zmm3", "zmm4",
    "zmm5", "zmm6", "zmm7",
];

fn set_for(
    words: &'static [&'static str],
    cell: &'static OnceLock<HashSet<&'static str>>,
) -> &'static HashSet<&'static str> {
    cell.get_or_init(|| words.iter().copied().collect())
}

fn contains(
    words: &'static [&'static str],
    cell: &'static OnceLock<HashSet<&'static str>>,
    name: &str,
) -> bool {
    // NASM keywords are case-insensitive; the sets store lowercase forms.
    set_for(words, cell).contains(name.to_ascii_lowercase().as_str())
}

pub fn is_directive(name: &str) -> bool {
    static CELL: OnceLock<HashSet<&'static str>> = OnceLock::new();
    contains(DIRECTIVES, &CELL, name)
}

pub fn is_pseudo_op(name: &str) -> bool {
    static CELL: OnceLock<HashSet<&'static str>> = OnceLock::new();
    contains(PSEUDO_OPS, &CELL, name)
}

pub fn is_size_keyword(name: &str) -> bool {
    static CELL: OnceLock<HashSet<&'static str>> = OnceLock::new();
    contains(SIZE_KEYWORDS, &CELL, name)
}

pub fn is_register(name: &str) -> bool {
    static CELL: OnceLock<HashSet<&'static str>> = OnceLock::new();
    contains(REGISTERS, &CELL, name)
}

pub fn is_prefix(name: &str) -> bool {
    static CELL: OnceLock<HashSet<&'static str>> = OnceLock::new();
    contains(PREFIXES, &CELL, name)
}

/// Whether an operand identifier should be treated as a symbol reference (i.e. it
/// is neither a register nor a size/distance keyword).
pub fn is_symbol_reference(name: &str) -> bool {
    !is_register(name) && !is_size_keyword(name)
}
