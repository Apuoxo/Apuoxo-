bits 64
section .text
global _start
_start:
    mov rax, 1
    mov rdi, 13
    int 0x80
    mov rax, 60
    xor rdi, rdi
    int 0x80
    jmp $
