//! SNB (Gen6) GGTT / stolen - safe development path for AH532.
//!
//! Architecture:
//! - Stolen/GSM (phys ~0xDBA00000.., GTT at top): CPU map/read → AH532 reboot.
//!   Hypothesis: reserved/stolen DRAM + PAT/MCE; not a simple missing PTE.
//! - GGTT PTE table lives IN stolen; Gen6 has no full MMIO-only PTE write path.
//! - Graphics APERTURE (prefetchable BAR): CPU access is translated by existing
//!   (BIOS) GTT entries - mapping APERTURE must not touch GSM.
//!
//! Policy:
//! - probe() = PCI only (safe).
//! - expand()/try_map_uc = NEVER map GSM.
//! - aper_smoke() = map 1 page of aperture, R/W pattern, leave LFB alone.

use crate::drivers::intel_kms;
use crate::serial;

const SNB_GMCH_CTRL: u8 = 0x50;
const SNB_GMCH_GMS_SHIFT: u32 = 8;
const SNB_GMCH_GMS_MASK: u32 = 0x1F;
const SNB_GMCH_GGMS_SHIFT: u32 = 6;
const SNB_GMCH_GGMS_MASK: u32 = 0x3;

const INTEL_BSM: u8 = 0x5C;
const GEN6_BDSM: u8 = 0xB0;
const BSM_MASK: u32 = 0xFFF0_0000;

const PGTBL_CTL: usize = 0x2020;

static mut STOLEN_MB: u32 = 0;
static mut APER_MB: u32 = 0;
static mut GTT_MB: u32 = 0;
static mut PGTBL: u32 = 0;
static mut GMCH: u32 = 0;
static mut STOLEN_BASE: u32 = 0;
static mut GTT_PHYS: u32 = 0;
static mut PTE0: u32 = 0;
static mut EXTRA_MAPPED_PAGES: u32 = 0;
static mut EXTRA_MAPPED_MB: u32 = 0;
static mut READY: bool = false;
static mut INIT_OK: bool = false;
/// 0=probe ok  1=no host  2=no kms  3=no write (safe skip)  4=no BSM
static mut LAST_ERR: u8 = 0;

unsafe fn pci_r32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") addr, options(nostack, preserves_flags));
    let val: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") val, options(nostack, preserves_flags));
    val
}

fn gms_to_mb(gss: u32) -> u32 {
    match gss {
        0 => 0,
        1 => 32,
        2 => 64,
        3 => 96,
        4 => 128,
        5 => 160,
        6 => 192,
        7 => 224,
        8 => 256,
        9 => 288,
        10 => 320,
        11 => 352,
        12 => 384,
        13 => 416,
        14 => 448,
        15 => 480,
        16 => 512,
        g if g >= 0x11 && g <= 0x1F => 512 + (g - 0x10) * 32,
        _ => {
            if gss <= 64 {
                gss * 32
            } else {
                0
            }
        }
    }
}

fn ggms_to_mb(gg: u32) -> u32 {
    match gg {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        _ => 0,
    }
}

fn read_stolen_base() -> u32 {
    let bdsm = unsafe { pci_r32(0, 0, 0, GEN6_BDSM) } & BSM_MASK;
    if bdsm != 0 && bdsm != 0xFFF0_0000 {
        return bdsm;
    }
    let bsm = unsafe { pci_r32(0, 0, 0, INTEL_BSM) } & BSM_MASK;
    if bsm != 0 && bsm != 0xFFF0_0000 {
        return bsm;
    }
    0
}

/// PCI-only probe: never touches GSM physical memory or PGTBL writes.
pub fn probe() -> bool {
    serial::write_str("[GTT] safe probe (no GSM touch)\n");
    let id = unsafe { pci_r32(0, 0, 0, 0) };
    if id == 0xFFFF_FFFF || (id & 0xFFFF) != 0x8086 {
        unsafe {
            LAST_ERR = 1;
            READY = false;
        }
        return false;
    }
    let gmch = unsafe { pci_r32(0, 0, 0, SNB_GMCH_CTRL) };
    let gss = (gmch >> SNB_GMCH_GMS_SHIFT) & SNB_GMCH_GMS_MASK;
    let gg = (gmch >> SNB_GMCH_GGMS_SHIFT) & SNB_GMCH_GGMS_MASK;
    // Gen6 (SNB): Linux i915 gen6_get_stolen_size = gms * 32MB
    let stolen = gms_to_mb(gss);
    let stolen = if stolen == 0 { gss * 32 } else { stolen };
    let mut gtt_sz = ggms_to_mb(gg);
    if gtt_sz == 0 {
        gtt_sz = 2;
    }
    // Aperture size from iGPU prefetchable BAR (not hardcoded 256)
    let mut aper = 0u32;
    let bar2 = unsafe { pci_r32(0, 2, 0, 0x18) }; // typically GMADR
    let bar0 = unsafe { pci_r32(0, 2, 0, 0x10) };
    // Prefer 64-bit prefetchable aperture BAR (bit3=1 prefetch, type 10=64bit)
    let cand = if (bar2 & 0xF) == 0xC || (bar2 & 0x6) == 0x4 { bar2 } else { bar0 };
    if cand != 0 && cand != 0xFFFF_FFFF {
        // Size via size-decode: write all-1s, read back (save/restore)
        let cfg_addr = |off: u8| {
            0x8000_0000u32 | (0 << 16) | (2 << 11) | (0 << 8) | ((off as u32) & 0xFC)
        };
        let off = if (bar2 & 0xF) == 0xC || (bar2 & 0x6) == 0x4 { 0x18u8 } else { 0x10u8 };
        unsafe {
            let a = cfg_addr(off);
            core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
            core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") 0xFFFF_FFFFu32, options(nostack, preserves_flags));
            core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
            let mut sz: u32;
            core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") sz, options(nostack, preserves_flags));
            // restore
            core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
            core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") cand, options(nostack, preserves_flags));
            let mask = sz & 0xFFFF_FFF0;
            if mask != 0 {
                let bytes = (!mask).wrapping_add(1);
                aper = bytes / (1024 * 1024);
            }
        }
    }
    if aper == 0 {
        aper = 256; // fallback if size-decode fails
    }
    // Never touch MMIO here - mmio_peek(PGTBL) hung terminal on AH532
    let pgtbl = 0u32;
    let stolen_base = read_stolen_base();
    let stolen_bytes = (stolen as u64) * 1024 * 1024;
    let gtt_bytes = (gtt_sz as u64) * 1024 * 1024;
    let gtt_phys = if stolen_base != 0 && stolen_bytes > gtt_bytes {
        (stolen_base as u64 + stolen_bytes - gtt_bytes) as u32
    } else {
        0
    };

    unsafe {
        GMCH = gmch;
        STOLEN_MB = stolen;
        GTT_MB = gtt_sz;
        APER_MB = aper;
        PGTBL = pgtbl;
        STOLEN_BASE = stolen_base;
        GTT_PHYS = gtt_phys;
        PTE0 = 0;
        EXTRA_MAPPED_PAGES = 0;
        EXTRA_MAPPED_MB = 0;
        READY = true;
        INIT_OK = false;
        LAST_ERR = if stolen_base == 0 { 4 } else { 3 }; // 3 = skip write (safe)
    }

    serial::write_str("[GTT] GMCH=");
    serial::write_hex(gmch as usize);
    serial::write_str(" stolen=");
    serial::write_usize(stolen as usize);
    serial::write_str("MB base=");
    serial::write_hex(stolen_base as usize);
    serial::write_str(" gtt_phys=");
    serial::write_hex(gtt_phys as usize);
    serial::write_str(" (computed, not mapped)\n");

    crate::drivers::intel_igpu::set_vram_mb(stolen);
    true
}


/// Never map GSM. Runs aperture smoke test instead (BIOS GTT path).
pub fn expand() -> u32 {
    aper_smoke()
}

/// DISABLED permanently on AH532 - GSM phys access reboots.
pub fn try_map_uc() -> u32 {
    serial::write_str("[GTT] try_map_uc BLOCKED (GSM reboot)\n");
    unsafe {
        LAST_ERR = 3;
        INIT_OK = false;
    }
    0
}

const APER_MAP_VA: usize = 0xF1000000;

/// Safe GGTT-related test without GSM:
/// Map 1 page of graphics APERTURE (not stolen), write/read pattern.
/// Requires aperture phys != MMIO base. Leaves Multiboot LFB untouched.
pub fn aper_smoke() -> u32 {
    if !unsafe { READY } {
        let _ = probe();
    }
    crate::drivers::intel_kms::ensure();
    let aper = crate::drivers::intel_kms::aperture();
    let mmio = crate::drivers::intel_kms::mmio_base();
    serial::write_str("[GTT] aper_smoke aper=");
    serial::write_hex(aper);
    serial::write_str(" mmio=");
    serial::write_hex(mmio);
    serial::write_str("\n");

    if aper == 0 {
        serial::write_str("[GTT] no aperture BAR\n");
        unsafe {
            LAST_ERR = 6;
            INIT_OK = false;
        }
        return 0;
    }
    if aper == mmio {
        serial::write_str("[GTT] aper==mmio refuse write\n");
        unsafe {
            LAST_ERR = 7;
            INIT_OK = false;
        }
        return 0;
    }

    let phys = aper & !0xFFF;
    let virt = APER_MAP_VA;
    let cr3 = unsafe { crate::mm::paging::read_cr3() };
    let flags = crate::mm::paging::PAGE_PRESENT
        | crate::mm::paging::PAGE_WRITE
        | crate::mm::paging::PAGE_PCD
        | crate::mm::paging::PAGE_PWT;

    serial::write_str("[GTT] map aper page va=");
    serial::write_hex(virt);
    serial::write_str(" phys=");
    serial::write_hex(phys);
    serial::write_str("\n");

    let ok = unsafe { crate::mm::paging::map_page(cr3, virt, phys, flags) };
    if !ok {
        serial::write_str("[GTT] aperture map_page FAIL\n");
        unsafe {
            LAST_ERR = 5;
            INIT_OK = false;
        }
        return 0;
    }
    unsafe {
        core::arch::asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags));
    }

    let pattern: u32 = 0xAE770001;
    let ptr = virt as *mut u32;
    unsafe {
        core::ptr::write_volatile(ptr, pattern);
        let got = core::ptr::read_volatile(ptr);
        PTE0 = got;
        if got == pattern {
            EXTRA_MAPPED_PAGES = 1;
            EXTRA_MAPPED_MB = 0;
            INIT_OK = true;
            LAST_ERR = 0;
            serial::write_str("[GTT] aper R/W OK (BIOS GTT path)\n");
            // grow to 64KB GPU-visible window (16 pages), still no GSM/SURF
            let n = aper_buf_init(64);
            if n > 0 {
                serial::write_str("[GTT] aper_buf ready pages=");
                serial::write_usize(n as usize);
                serial::write_str("\n");
                let _ = aper_fill_test();
            }
            1
        } else {
            EXTRA_MAPPED_PAGES = 0;
            INIT_OK = false;
            LAST_ERR = 8;
            serial::write_str("[GTT] aper R/W mismatch got=");
            serial::write_hex(got as usize);
            serial::write_str("\n");
            0
        }
    }
}


/// Pages successfully mapped from aperture (after aper_buf_init).
static mut APER_PAGES: u32 = 0;
static mut APER_VA: usize = 0;
static mut APER_PHYS_BASE: usize = 0;

/// Map up to `want_pages` of aperture (4K each) for a small GPU-visible buffer.
/// Does not touch GSM, GTT PTEs, DSPASURF, or Multiboot LFB.
pub fn aper_buf_init(want_pages: u32) -> u32 {
    if want_pages == 0 {
        return 0;
    }
    let want = if want_pages > 512 { 512 } else { want_pages }; // max 1MB
    crate::drivers::intel_kms::ensure();
    let aper = crate::drivers::intel_kms::aperture();
    let mmio = crate::drivers::intel_kms::mmio_base();
    if aper == 0 || aper == mmio {
        unsafe { LAST_ERR = if aper == 0 { 6 } else { 7 }; }
        return 0;
    }
    let cr3 = unsafe { crate::mm::paging::read_cr3() };
    let flags = crate::mm::paging::PAGE_PRESENT
        | crate::mm::paging::PAGE_WRITE
        | crate::mm::paging::PAGE_PCD
        | crate::mm::paging::PAGE_PWT;
    let va0 = APER_MAP_VA;
    let mut mapped = 0u32;
    let mut i = 0u32;
    while i < want {
        let virt = va0 + (i as usize) * 4096;
        let phys = (aper & !0xFFF) + (i as usize) * 4096;
        let ok = unsafe { crate::mm::paging::map_page(cr3, virt, phys, flags) };
        if !ok {
            break;
        }
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags));
        }
        mapped += 1;
        i += 1;
    }
    if mapped == 0 {
        unsafe {
            LAST_ERR = 5;
            APER_PAGES = 0;
        }
        serial::write_str("[GTT] aper_buf map 0 pages\n");
        return 0;
    }
    // Touch first and last dword
    unsafe {
        let p0 = va0 as *mut u32;
        core::ptr::write_volatile(p0, 0xAE7E0001u32);
        let last = (va0 + (mapped as usize - 1) * 4096) as *mut u32;
        core::ptr::write_volatile(last, 0xAE7E0002u32);
        let g0 = core::ptr::read_volatile(p0);
        let g1 = core::ptr::read_volatile(last);
        if g0 != 0xAE7E0001 || g1 != 0xAE7E0002 {
            LAST_ERR = 8;
            APER_PAGES = 0;
            serial::write_str("[GTT] aper_buf R/W fail\n");
            return 0;
        }
        APER_PAGES = mapped;
        APER_VA = va0;
        APER_PHYS_BASE = aper & !0xFFF;
        INIT_OK = true;
        LAST_ERR = 0;
        EXTRA_MAPPED_PAGES = mapped;
        EXTRA_MAPPED_MB = mapped / 256; // rough
    }
    serial::write_str("[GTT] aper_buf pages=");
    serial::write_usize(mapped as usize);
    serial::write_str(" va=");
    serial::write_hex(va0);
    serial::write_str("\n");
    mapped
}

pub fn aper_va() -> usize {
    unsafe { APER_VA }
}
pub fn aper_phys() -> usize {
    unsafe { APER_PHYS_BASE }
}
pub fn aper_pages() -> u32 {
    unsafe { APER_PAGES }
}

/// Bytes available in mapped aperture buffer.
pub fn aper_bytes() -> usize {
    unsafe { (APER_PAGES as usize) * 4096 }
}

/// Fill mapped aperture with a visible test pattern (CPU only; not on screen until SURF).
pub fn aper_fill_test() -> bool {
    let va = unsafe { APER_VA };
    let pages = unsafe { APER_PAGES };
    if va == 0 || pages == 0 {
        return false;
    }
    let n = (pages as usize) * 4096;
    let mut i = 0usize;
    while i + 4 <= n {
        // alternating blue/green bars every 64 dwords
        let bar = (i / 256) & 1;
        let v = if bar == 0 { 0xFF0000FFu32 } else { 0xFF00FF00u32 };
        unsafe { core::ptr::write_volatile((va + i) as *mut u32, v); }
        i += 4;
    }
    // verify corners
    unsafe {
        let a = core::ptr::read_volatile(va as *const u32);
        let b = core::ptr::read_volatile((va + n - 4) as *const u32);
        if a == 0 && b == 0 {
            return false;
        }
    }
    serial::write_str("[GTT] aper_fill_test OK bytes=");
    serial::write_usize(n);
    serial::write_str("\n");
    true
}



pub fn ready() -> bool {
    unsafe { READY }
}
pub fn init_ok() -> bool {
    unsafe { INIT_OK }
}
pub fn stolen_mb() -> u32 {
    unsafe { STOLEN_MB }
}
pub fn aper_mb() -> u32 {
    unsafe { APER_MB }
}
pub fn gtt_mb() -> u32 {
    unsafe { GTT_MB }
}
pub fn pgtbl() -> u32 {
    unsafe { PGTBL }
}
pub fn gmch() -> u32 {
    unsafe { GMCH }
}
pub fn stolen_base() -> u32 {
    unsafe { STOLEN_BASE }
}
pub fn gtt_phys() -> u32 {
    unsafe { GTT_PHYS }
}
pub fn pte0() -> u32 {
    unsafe { PTE0 }
}
pub fn extra_pages() -> u32 {
    unsafe { EXTRA_MAPPED_PAGES }
}
pub fn extra_mb() -> u32 {
    unsafe { EXTRA_MAPPED_MB }
}
pub fn last_err() -> u8 {
    unsafe { LAST_ERR }
}
pub fn total_mb() -> u32 {
    unsafe { STOLEN_MB }
}
