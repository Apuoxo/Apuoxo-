//! Early serial console (COM1) - non-blocking on real hardware without UART

mod ports {
    use core::arch::asm;

    pub unsafe fn outb(port: u16, val: u8) {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
    pub unsafe fn inb(port: u16) -> u8 {
        let val: u8;
        asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack, preserves_flags));
        val
    }
}

static mut ENABLED: bool = false;

pub fn init() {
    unsafe {
        use ports::*;
        // Probe: write SCRATCH and read back
        outb(0x3F8 + 7, 0xAE);
        let probe = inb(0x3F8 + 7);
        if probe != 0xAE {
            ENABLED = false;
            return;
        }
        outb(0x3F8 + 1, 0x00);
        outb(0x3F8 + 3, 0x80);
        outb(0x3F8 + 0, 0x03);
        outb(0x3F8 + 1, 0x00);
        outb(0x3F8 + 3, 0x03);
        outb(0x3F8 + 2, 0xC7);
        outb(0x3F8 + 4, 0x0B);
        ENABLED = true;
    }
}

pub fn write_byte(b: u8) {
    unsafe {
        if !ENABLED {
            return;
        }
        use ports::*;
        // Timeout - never hang if UART stuck
        let mut t = 0u32;
        while (inb(0x3F8 + 5) & 0x20) == 0 {
            t += 1;
            if t > 100_000 {
                ENABLED = false;
                return;
            }
        }
        outb(0x3F8, b);
    }
}

pub fn write_str(s: &str) {
    for &b in s.as_bytes() {
        if b == b'\n' {
            write_byte(b'\r');
        }
        write_byte(b);
    }
}

pub fn write_usize(n: usize) {
    if n == 0 {
        write_byte(b'0');
        return;
    }
    let mut tmp = n;
    let mut digits = 0;
    while tmp > 0 {
        digits += 1;
        tmp /= 10;
    }
    let mut div = 1usize;
    let mut i = 1;
    while i < digits {
        div *= 10;
        i += 1;
    }
    let mut x = n;
    while div > 0 {
        let d = x / div;
        write_byte(b'0' + d as u8);
        x %= div;
        div /= 10;
    }
}

pub fn write_hex(n: usize) {
    write_str("0x");
    if n == 0 {
        write_byte(b'0');
        return;
    }
    let mut started = false;
    let mut i = 16usize;
    while i > 0 {
        i -= 1;
        let d = ((n >> (i * 4)) & 0xf) as u8;
        if d != 0 || started || i == 0 {
            started = true;
            let c = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
            write_byte(c);
        }
    }
}
