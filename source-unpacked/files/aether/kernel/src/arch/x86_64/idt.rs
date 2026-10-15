//! IDT

#[derive(Clone, Copy)]
#[repr(C, packed)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry {
    offset_low: 0, selector: 0, ist: 0, type_attr: 0,
    offset_mid: 0, offset_high: 0, zero: 0,
}; 256];

static mut IDTR: IdtPointer = IdtPointer { limit: 0, base: 0 };

fn set_gate(num: usize, handler: u64, typ: u8) {
    unsafe {
        IDT[num].offset_low = (handler & 0xFFFF) as u16;
        IDT[num].selector = 0x08;
        IDT[num].ist = 0;
        IDT[num].type_attr = typ;
        IDT[num].offset_mid = ((handler >> 16) & 0xFFFF) as u16;
        IDT[num].offset_high = ((handler >> 32) & 0xFFFFFFFF) as u32;
        IDT[num].zero = 0;
    }
}

extern "C" {
    fn isr_page_fault();
    fn isr_syscall();
}

pub fn init() {
    set_gate(14, isr_page_fault as u64, 0x8E);
    set_gate(0x80, isr_syscall as u64, 0xEE);
    unsafe {
        IDTR.limit = (core::mem::size_of_val(&IDT) - 1) as u16;
        IDTR.base = &IDT as *const _ as u64;
        core::arch::asm!("lidt [{}]", in(reg) &IDTR, options(readonly, nostack, preserves_flags));
    }
}
