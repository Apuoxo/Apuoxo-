#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

global_asm!(r#"
.section .multiboot2
.align 8
mb2_header:
    .long 0xE85250D6
    .long 0
    .long mb2_header_end - mb2_header
    .long -(0xE85250D6 + (mb2_header_end - mb2_header))
    .word 0
    .word 0
    .long 8
mb2_header_end:

.section .text.boot
.global _start
.type _start, @function
_start:
    cli
    mov $boot_stack_top, %esp

    # Build an identity map for the first 1 GiB with 2 MiB pages.
    mov $pdpt, %eax
    or $0x3, %eax
    mov %eax, pml4
    mov $pd, %eax
    or $0x3, %eax
    mov %eax, pdpt

    xor %ecx, %ecx
    mov $pd, %edi
1:
    mov %ecx, %eax
    shl $21, %eax
    or $0x83, %eax
    mov %eax, (%edi)
    movl $0, 4(%edi)
    add $8, %edi
    inc %ecx
    cmp $512, %ecx
    jne 1b

    # Enable PAE.
    mov %cr4, %eax
    or $0x20, %eax
    mov %eax, %cr4

    mov $pml4, %eax
    mov %eax, %cr3

    # Enable EFER.LME.
    mov $0xC0000080, %ecx
    rdmsr
    or $0x100, %eax
    wrmsr

    # Enable paging.
    mov %cr0, %eax
    or $0x80000000, %eax
    mov %eax, %cr0

    lgdt gdt64_ptr
    ljmp $0x08, $long_mode

.code64
long_mode:
    mov $0x10, %eax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss

    mov $boot_stack_top, %rsp
    and $-16, %rsp
    call kmain

2:
    cli
    hlt
    jmp 2b

.section .rodata
.align 8
gdt64:
    .quad 0
    .quad 0x00AF9A000000FFFF
    .quad 0x00AF92000000FFFF
gdt64_end:

.align 8
gdt64_ptr:
    .word gdt64_end - gdt64 - 1
    .quad gdt64

.section .bss
.align 4096
pml4:
    .skip 4096
pdpt:
    .skip 4096
pd:
    .skip 4096

.align 16
boot_stack:
    .skip 16384
boot_stack_top:
"#);

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    let vga = 0xb8000 as *mut u8;
    let text = b"ANDROID-LIKE OS  //  CLEAN BOOT";
    unsafe {
        for (i, &byte) in text.iter().enumerate() {
            core::ptr::write_volatile(vga.add(i * 2), byte);
            core::ptr::write_volatile(vga.add(i * 2 + 1), 0x0f);
        }
    }

    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
