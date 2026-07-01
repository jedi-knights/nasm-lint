; Fixture for NL040 (unreachable code).
section .text
global _start
_start:
    cmp eax, 0
    je .zero
    jmp .done
    mov eax, 99      ; NL040: unreachable — follows an unconditional jmp
.zero:
    mov eax, 1
.done:
    ret
    nop              ; NL040: unreachable — follows ret
