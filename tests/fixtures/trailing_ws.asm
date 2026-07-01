; Fixture for NL053 (trailing whitespace). Two lines below end in whitespace.
section .text
global _start

_start:
    mov eax, 1  
    mov ebx, 0
    int 0x80	
