#![no_std]
#![no_main]

use core::panic::PanicInfo;

core::arch::global_asm!(r#"
    .section .multiboot2
    .align 8
header_start:
    .long 0xe85250d6
    .long 0
    .long header_end - header_start
    .long -(0xe85250d6 + (header_end - header_start))
    .short 0
    .short 0
    .long 8
header_end:

    .section .text.boot
    .code32
    .global _start
    .type _start,@function
_start:
    cli

    # Build identity-mapped 0..1 GiB using 2 MiB pages.
    mov eax, pml4_table
    mov ebx, pdpt_table
    or ebx, 0x3
    mov dword ptr [eax], ebx
    mov dword ptr [eax + 4], 0

    mov eax, pd_table
    mov ebx, eax
    or ebx, 0x3
    mov dword ptr [pdpt_table], ebx
    mov dword ptr [pdpt_table + 4], 0

    xor ecx, ecx
    mov edi, pd_table
    xor edx, edx
.fill_pd:
    mov eax, edx
    or eax, 0x83
    mov dword ptr [edi + ecx * 8], eax
    mov dword ptr [edi + ecx * 8 + 4], 0
    add edx, 0x200000
    inc ecx
    cmp ecx, 512
    jb .fill_pd

    mov eax, pml4_table
    mov cr3, eax

    mov eax, cr4
    or eax, 0x20
    mov cr4, eax

    mov ecx, 0xc0000080
    rdmsr
    or eax, 0x100
    wrmsr

    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax

    lgdt [gdt64_ptr]
    ljmp 0x08, long_mode_start

    .code64
long_mode_start:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor ax, ax
    mov fs, ax
    mov gs, ax

    mov rsp, stack_top
    and rsp, -16
    call kmain

.hang:
    cli
    hlt
    jmp .hang

    .section .rodata
    .align 8
gdt64:
    .quad 0
    .quad 0x00af9a000000ffff
    .quad 0x00cf92000000ffff
gdt64_end:
gdt64_ptr:
    .word gdt64_end - gdt64 - 1
    .long gdt64

    .section .bss
    .align 4096
pml4_table:
    .skip 4096
pdpt_table:
    .skip 4096
pd_table:
    .skip 4096
    .align 16
stack_bottom:
    .skip 16384
stack_top:
"#);

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    let vga = 0xb8000 as *mut u8;
    let text = b"ANDROID-LIKE OS  //  FIRST BOOT";
    unsafe {
        for (i, &b) in text.iter().enumerate() {
            core::ptr::write_volatile(vga.add(i * 2), b);
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
