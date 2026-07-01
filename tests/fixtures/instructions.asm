; Fixture for NL030 (unknown mnemonic) and NL031 (operand count).
section .text
global _start
_start:
    mov eax, 1        ; valid
    mxv ebx, 2        ; NL030: typo'd mnemonic
    ret eax, ebx      ; NL031: ret takes 0 or 1 operands
    je _start         ; valid (cc expansion)
    add rax, rbx      ; valid (group macro)
