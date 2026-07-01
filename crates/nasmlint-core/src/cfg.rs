//! Control-flow graph over a program's instructions.
//!
//! This is the M4 foundation for flow analysis. Nodes are instructions (each a
//! trivial basic block); edges follow control: a `jmp` goes only to its target, a
//! conditional jump (`jcc`/`loop`) both branches and falls through, `ret` (and
//! `hlt`/`ud2`) terminate, `call` continues to the next instruction *and* makes
//! its callee reachable, and everything else falls through.
//!
//! ## The `has_unresolved_transfer` guard
//!
//! Reachability is only trustworthy when every jump target is known. A computed
//! jump (`jmp rax`, `jmp [table]`) or a jump to a label we can't resolve means
//! some edges are missing, which would make live code look dead. When any
//! unconditional/conditional jump target does not resolve to a known instruction,
//! [`Cfg::has_unresolved_transfer`] is set and reachability-based rules must bow out
//! rather than risk a false positive. (Unresolved `call` targets — e.g. `extern`
//! functions — are fine: a call always also falls through, so no edge is lost.)

use std::collections::HashMap;

use crate::ast::{LineBody, Program};
use crate::diagnostics::Span;

/// How an instruction transfers control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminator {
    /// `ret`/`iret`/`hlt`/`ud2`/... — control does not continue.
    Return,
    /// `jmp` — control goes only to the target.
    Jump,
    /// `jcc`/`loop` — control branches to the target or falls through.
    Branch,
    /// `call` — control continues after returning, and the callee is entered.
    Call,
    /// Any other instruction — control falls through to the next.
    Fallthrough,
}

/// Classify a mnemonic's control-flow effect.
pub fn classify(mnemonic: &str) -> Terminator {
    let m = mnemonic.to_ascii_lowercase();
    match m.as_str() {
        "jmp" => Terminator::Jump,
        "call" => Terminator::Call,
        "ret" | "retn" | "retf" | "retw" | "retd" | "retq" | "iret" | "iretw" | "iretd"
        | "iretq" | "sysret" | "sysexit" | "hlt" | "ud0" | "ud1" | "ud2" => Terminator::Return,
        // All remaining `j*` mnemonics are conditional jumps; `loop*` too.
        _ if m.starts_with('j') || m.starts_with("loop") => Terminator::Branch,
        _ => Terminator::Fallthrough,
    }
}

/// One instruction node in the graph.
pub struct Node {
    pub line_no: usize,
    pub span: Span,
    pub terminator: Terminator,
    /// True if this instruction carries (or is preceded by) a label — a possible
    /// jump target and thus a block leader.
    pub labeled: bool,
}

/// The control-flow graph for a program's instructions, plus reachability.
pub struct Cfg {
    pub nodes: Vec<Node>,
    reachable: Vec<bool>,
    pub has_unresolved_transfer: bool,
}

impl Cfg {
    /// Whether node `i` is reachable from an entry (first instruction or a
    /// `global`-labelled instruction).
    pub fn is_reachable(&self, i: usize) -> bool {
        self.reachable[i]
    }

    /// Build the graph. `globals` are exported symbol names, treated as entry roots
    /// since they may be called from outside this translation unit.
    pub fn build(program: &Program, globals: &[String]) -> Cfg {
        // Collect instruction nodes, attaching any labels that lead up to them.
        struct Raw {
            line_no: usize,
            span: Span,
            terminator: Terminator,
            target: Option<String>,
            labels: Vec<String>,
        }
        let mut raws: Vec<Raw> = Vec::new();
        let mut pending: Vec<String> = Vec::new();

        for line in &program.lines {
            if let Some(label) = &line.label {
                pending.push(label.name.clone());
            }
            match &line.body {
                LineBody::Instruction(instr) => {
                    // The first operand identifier is the branch target, if any.
                    let target = instr
                        .operands
                        .first()
                        .and_then(|op| op.idents.first())
                        .map(|id| id.name.clone());
                    raws.push(Raw {
                        line_no: line.line_no,
                        span: instr.mnemonic_span,
                        terminator: classify(&instr.mnemonic),
                        target,
                        labels: std::mem::take(&mut pending),
                    });
                }
                // A label-only line keeps its label pending for the next instruction.
                LineBody::Empty => {}
                // Data/directive/preprocessor lines end any pending code label so a
                // data label is not attached to a later instruction.
                _ => pending.clear(),
            }
        }

        // Map each label to the instruction it leads. First definition wins.
        let mut label_index: HashMap<&str, usize> = HashMap::new();
        for (i, raw) in raws.iter().enumerate() {
            for name in &raw.labels {
                label_index.entry(name.as_str()).or_insert(i);
            }
        }

        // Build successor edges.
        let n = raws.len();
        let mut successors: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut has_unresolved_transfer = false;

        for (i, raw) in raws.iter().enumerate() {
            let next = (i + 1 < n).then_some(i + 1);
            let resolved = raw
                .target
                .as_deref()
                .and_then(|t| label_index.get(t).copied());
            match raw.terminator {
                Terminator::Return => {}
                Terminator::Fallthrough => successors[i].extend(next),
                Terminator::Call => {
                    successors[i].extend(resolved); // callee entered
                    successors[i].extend(next); // and control returns
                }
                Terminator::Jump => match resolved {
                    Some(t) => successors[i].push(t),
                    None => has_unresolved_transfer = true,
                },
                Terminator::Branch => {
                    match resolved {
                        Some(t) => successors[i].push(t),
                        None => has_unresolved_transfer = true,
                    }
                    successors[i].extend(next); // conditional: also falls through
                }
            }
        }

        // Reachability from entry roots: the first instruction and every
        // global-labelled instruction.
        let global_set: std::collections::HashSet<&str> =
            globals.iter().map(String::as_str).collect();
        let mut reachable = vec![false; n];
        let mut stack: Vec<usize> = Vec::new();
        if n > 0 {
            stack.push(0);
        }
        for (i, raw) in raws.iter().enumerate() {
            if raw.labels.iter().any(|l| global_set.contains(l.as_str())) {
                stack.push(i);
            }
        }
        while let Some(i) = stack.pop() {
            if reachable[i] {
                continue;
            }
            reachable[i] = true;
            for &s in &successors[i] {
                if !reachable[s] {
                    stack.push(s);
                }
            }
        }

        let nodes = raws
            .into_iter()
            .map(|r| Node {
                line_no: r.line_no,
                span: r.span,
                terminator: r.terminator,
                labeled: !r.labels.is_empty(),
            })
            .collect();

        Cfg {
            nodes,
            reachable,
            has_unresolved_transfer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn cfg(src: &str, globals: &[&str]) -> Cfg {
        let owned: Vec<String> = globals.iter().map(|s| s.to_string()).collect();
        Cfg::build(&parse(src), &owned)
    }

    #[test]
    fn classifies_terminators() {
        assert_eq!(classify("jmp"), Terminator::Jump);
        assert_eq!(classify("je"), Terminator::Branch);
        assert_eq!(classify("ret"), Terminator::Return);
        assert_eq!(classify("call"), Terminator::Call);
        assert_eq!(classify("mov"), Terminator::Fallthrough);
        assert_eq!(classify("loop"), Terminator::Branch);
    }

    #[test]
    fn code_after_jmp_is_unreachable() {
        // insn0 jmp done, insn1 mov (dead), insn2 ret (labelled done)
        let c = cfg("    jmp done\n    mov eax, 1\ndone:\n    ret\n", &[]);
        assert!(c.is_reachable(0));
        assert!(!c.is_reachable(1)); // the mov is unreachable
        assert!(c.is_reachable(2)); // done: is a jump target
        assert!(!c.has_unresolved_transfer);
    }

    #[test]
    fn computed_jump_sets_guard() {
        let c = cfg("    jmp rax\n    mov eax, 1\n", &[]);
        assert!(c.has_unresolved_transfer);
    }

    #[test]
    fn extern_call_does_not_set_guard() {
        // An unresolved call target is fine: call always also falls through.
        let c = cfg("section .text\n    call printf\n    ret\n", &[]);
        assert!(!c.has_unresolved_transfer);
        assert!(c.is_reachable(0) && c.is_reachable(1));
    }

    #[test]
    fn uncalled_non_global_routine_is_unreachable() {
        let src = "_start:\n    ret\nhelper:\n    nop\n    ret\n";
        let c = cfg(src, &["_start"]);
        assert!(c.is_reachable(0)); // _start
        assert!(!c.is_reachable(1)); // helper: nop — never called, not global
    }
}
