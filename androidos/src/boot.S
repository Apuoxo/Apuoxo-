.section .multiboot2
.align 8
header_start:
.long 0xe85250d6
.long 0
.long header_end - header_start
.long -(0xe85250d6 + 0 + (header_end - header_start))
.align 8
.short 0
.short 0
.long 8
header_end:

.section .text
.global _start
.type _start,@function
_start:
    cli
    call kmain
1:
    hlt
    jmp 1b
