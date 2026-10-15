//! CPU identification (CPUID) for System Properties and diagnostics.

use core::sync::atomic::{AtomicBool, Ordering};

static INIT: AtomicBool = AtomicBool::new(false);
static mut VENDOR: [u8; 13] = [0; 13];
static mut BRAND: [u8; 49] = [0; 49];
static mut FAMILY: u32 = 0;
static mut MODEL: u32 = 0;
static mut STEPPING: u32 = 0;
static mut FEATURES: u32 = 0;
static mut FEATURES_ECX: u32 = 0;

#[inline]
unsafe fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let a: u32;
    let b: u32;
    let c: u32;
    let d: u32;
    // LLVM reserves rbx - save/restore via RSI (same as smp.rs)
    core::arch::asm!(
        "xchg rsi, rbx",
        "cpuid",
        "xchg rsi, rbx",
        inout("eax") leaf => a,
        out("rsi") b,
        lateout("ecx") c,
        lateout("edx") d,
        options(nostack, preserves_flags)
    );
    (a, b, c, d)
}

pub fn init() {
    if INIT.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe {
        let (_max_leaf, b, c, d) = cpuid(0);
        VENDOR[0] = (b & 0xFF) as u8;
        VENDOR[1] = ((b >> 8) & 0xFF) as u8;
        VENDOR[2] = ((b >> 16) & 0xFF) as u8;
        VENDOR[3] = ((b >> 24) & 0xFF) as u8;
        VENDOR[4] = (d & 0xFF) as u8;
        VENDOR[5] = ((d >> 8) & 0xFF) as u8;
        VENDOR[6] = ((d >> 16) & 0xFF) as u8;
        VENDOR[7] = ((d >> 24) & 0xFF) as u8;
        VENDOR[8] = (c & 0xFF) as u8;
        VENDOR[9] = ((c >> 8) & 0xFF) as u8;
        VENDOR[10] = ((c >> 16) & 0xFF) as u8;
        VENDOR[11] = ((c >> 24) & 0xFF) as u8;
        VENDOR[12] = 0;

        let (eax1, _b1, ecx1, edx1) = cpuid(1);
        STEPPING = eax1 & 0xF;
        MODEL = (eax1 >> 4) & 0xF;
        FAMILY = (eax1 >> 8) & 0xF;
        let model_ext = (eax1 >> 16) & 0xF;
        let family_ext = (eax1 >> 20) & 0xFF;
        if FAMILY == 0xF {
            FAMILY = FAMILY + family_ext;
        }
        if FAMILY == 0x6 || FAMILY == 0xF {
            MODEL = MODEL | (model_ext << 4);
        }
        FEATURES = edx1;
        FEATURES_ECX = ecx1;
        let _ = FEATURES_ECX;

        let (max_ext, _, _, _) = cpuid(0x8000_0000);
        if max_ext >= 0x8000_0004 {
            let mut pos = 0usize;
            let mut leaf = 0x8000_0002u32;
            while leaf <= 0x8000_0004 {
                let (a, b2, c2, d2) = cpuid(leaf);
                for word in [a, b2, c2, d2] {
                    let mut sh = 0u32;
                    while sh < 32 && pos < 48 {
                        BRAND[pos] = ((word >> sh) & 0xFF) as u8;
                        pos += 1;
                        sh += 8;
                    }
                }
                leaf += 1;
            }
            BRAND[48] = 0;
            let mut start = 0usize;
            while start < 48 && BRAND[start] == b' ' {
                start += 1;
            }
            if start > 0 {
                let mut j = 0usize;
                while start + j < 48 {
                    BRAND[j] = BRAND[start + j];
                    j += 1;
                }
                while j < 49 {
                    BRAND[j] = 0;
                    j += 1;
                }
            }
        } else {
            let mut i = 0usize;
            while i < 12 {
                BRAND[i] = VENDOR[i];
                i += 1;
            }
        }
    }
}

pub fn vendor() -> &'static [u8] {
    init();
    unsafe {
        let mut len = 0usize;
        while len < 12 && VENDOR[len] != 0 {
            len += 1;
        }
        &VENDOR[..len]
    }
}

pub fn brand() -> &'static [u8] {
    init();
    unsafe {
        let mut len = 0usize;
        while len < 48 && BRAND[len] != 0 {
            len += 1;
        }
        &BRAND[..len]
    }
}

pub fn family() -> u32 {
    init();
    unsafe { FAMILY }
}
pub fn model() -> u32 {
    init();
    unsafe { MODEL }
}
pub fn stepping() -> u32 {
    init();
    unsafe { STEPPING }
}
pub fn has_apic() -> bool {
    init();
    unsafe { (FEATURES & (1 << 9)) != 0 }
}
pub fn has_sse2() -> bool {
    init();
    unsafe { (FEATURES & (1 << 26)) != 0 }
}
pub fn has_pae() -> bool {
    init();
    unsafe { (FEATURES & (1 << 6)) != 0 }
}
