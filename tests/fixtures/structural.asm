global _start
extern printf

    mov eax, 1        ; NL020: real code before any section

%macro prologue 0     ; NL010: macro never closed
    push rbp          ; inside macro body — NOT flagged by NL020

section .text
_start:
    jmp missing_label ; NL001: undefined label
    call printf
