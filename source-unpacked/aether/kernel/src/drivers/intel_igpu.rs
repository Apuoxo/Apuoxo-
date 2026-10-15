//! Native Intel integrated GPU probe + display state for Aether.
//! Runtime path remains Multiboot LFB; this module owns hardware identity,
//! BAR map, VRAM estimates, and preferred mode settings for the UI.

use crate::graphics;
use crate::serial;

const PCI_CFG_ADDR: u16 = 0xCF8;
const PCI_CFG_DATA: u16 = 0xCFC;

#[derive(Clone, Copy)]
pub struct GpuInfo {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vid: u16,
    pub did: u16,
    pub rev: u8,
    pub sub: u8,
    pub progif: u8,
    pub bar0: u32, // often MMIO
    pub bar1: u32,
    pub bar2: u32, // often GMADR 64-bit low
    pub bar3: u32, // GMADR high
    pub bar4: u32, // IO
    pub bar5: u32,
    pub irq_line: u8,
    pub irq_pin: u8,
    pub cmd: u16,
    pub present: bool,
}

#[derive(Clone, Copy)]
pub struct ModePref {
    pub w: u16,
    pub h: u16,
    pub bpp: u8,
}

static mut GPU: GpuInfo = GpuInfo {
    bus: 0, dev: 0, func: 0, vid: 0, did: 0, rev: 0, sub: 0, progif: 0,
    bar0: 0, bar1: 0, bar2: 0, bar3: 0, bar4: 0, bar5: 0,
    irq_line: 0, irq_pin: 0, cmd: 0, present: false,
};
static mut PREF: ModePref = ModePref { w: 800, h: 600, bpp: 32 };
static mut VRAM_MB: u32 = 0;
static mut DRIVER_NAME: [u8; 32] = *b"Aether iGPU LFB v1.0\0\0\0\0\0\0\0\0\0\0\0\0";
static mut READY: bool = false;
static mut GEN: u8 = 0;

unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nostack, preserves_flags));
}
unsafe fn inl(port: u16) -> u32 {
    let v: u32;
    core::arch::asm!("in eax, dx", in("dx") port, out("eax") v, options(nostack, preserves_flags));
    v
}
unsafe fn pci_r32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    outl(PCI_CFG_ADDR, addr);
    inl(PCI_CFG_DATA)
}


fn gen_from_did(did: u16) -> u8 {
    match did {
        // Sandy Bridge Gen6
        0x0102 | 0x0106 | 0x010A | 0x010B |
        0x0112 | 0x0116 | 0x0122 | 0x0126 => 6,
        // Ivy Bridge Gen7 (desktop + mobile + server)
        0x0152 | 0x0156 | 0x015A | 0x015E |
        0x0162 | 0x0166 | 0x016A | 0x016E => 7,
        // Haswell Gen7.5 treated as 7 for display offsets
        0x0402 | 0x0406 | 0x040A | 0x0412 | 0x0416 | 0x041A |
        0x041E | 0x0A06 | 0x0A16 | 0x0A26 | 0x0A2E | 0x0D16 | 0x0D26 => 7,
        _ => 0, // unknown - caller decides
    }
}

fn did_name(did: u16) -> &'static str {
    match did {
        0x0102 | 0x0106 | 0x010A => "SNB HD Graphics 2000",
        0x0112 | 0x0116 => "SNB HD Graphics 3000",
        0x0122 | 0x0126 => "SNB HD Graphics 3000",
        0x0152 | 0x0156 | 0x015A => "IVB HD Graphics 2500",
        0x0162 | 0x0166 | 0x016A => "IVB HD Graphics 4000",
        0x0046 => "Ironlake HD Graphics",
        _ => "Intel HD Graphics",
    }
}


/// Sandy Bridge / Ivy Bridge: host bridge 0:0.0 config 0x50 (GMCH_CTRL).
/// GMS bits [12:8] (mask 0x1F) → stolen in 32MB units (Linux i915 gen6).
fn read_gmch_stolen_mb() -> (u32, u32, u32) {
    unsafe {
        let addr = 0x8000_0000u32; // bus0 dev0 func0
        core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") addr, options(nostack, preserves_flags));
        let id: u32;
        core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") id, options(nostack, preserves_flags));
        if (id & 0xFFFF) != 0x8086 {
            return (0, 0, 0);
        }
        let addr50 = 0x8000_0000u32 | 0x50;
        core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") addr50, options(nostack, preserves_flags));
        let gmch: u32;
        core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") gmch, options(nostack, preserves_flags));
        let gms = (gmch >> 8) & 0x1F;
        let ggms = (gmch >> 6) & 0x3;
        let stolen = gms * 32; // gen6: each unit 32MB
        // Aperture typical: SNB mobile 256MB graphics aperture (not GGMS which is GTT size)
        let aper = if stolen > 0 { 256 } else { 0 };
        let _ = ggms;
        (stolen, aper, gmch)
    }
}

pub fn init() {
    serial::write_str("[iGPU] probe\n");
    unsafe {
        READY = false;
        GPU.present = false;
    }
    for bus in 0u8..=255 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = unsafe { pci_r32(bus, dev, func, 0) };
                if id == 0xFFFF_FFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let vid = (id & 0xFFFF) as u16;
                let did = ((id >> 16) & 0xFFFF) as u16;
                let cr = unsafe { pci_r32(bus, dev, func, 0x08) };
                let class = ((cr >> 24) & 0xFF) as u8;
                if class != 0x03 {
                    if func == 0 {
                        let ht = unsafe { pci_r32(bus, dev, func, 0x0C) };
                        if ((ht >> 16) & 0x80) == 0 {
                            break;
                        }
                    }
                    continue;
                }
                // First display controller
                let sub = ((cr >> 16) & 0xFF) as u8;
                let progif = ((cr >> 8) & 0xFF) as u8;
                let rev = (cr & 0xFF) as u8;
                let bar0 = unsafe { pci_r32(bus, dev, func, 0x10) };
                let bar1 = unsafe { pci_r32(bus, dev, func, 0x14) };
                let bar2 = unsafe { pci_r32(bus, dev, func, 0x18) };
                let bar3 = unsafe { pci_r32(bus, dev, func, 0x1C) };
                let bar4 = unsafe { pci_r32(bus, dev, func, 0x20) };
                let bar5 = unsafe { pci_r32(bus, dev, func, 0x24) };
                let cmd = (unsafe { pci_r32(bus, dev, func, 0x04) } & 0xFFFF) as u16;
                let irq = unsafe { pci_r32(bus, dev, func, 0x3C) };
                unsafe {
                    GPU = GpuInfo {
                        bus, dev, func, vid, did, rev, sub, progif,
                        bar0, bar1, bar2, bar3, bar4, bar5,
                        irq_line: (irq & 0xFF) as u8,
                        irq_pin: ((irq >> 8) & 0xFF) as u8,
                        cmd,
                        present: true,
                    };
                    // Estimate aperture from 64-bit prefetchable BAR2
                    // Size not from config without sizing write; use typical Ivy ~256MB aperture
                    // Prefer: if LFB size known, report that as active; VRAM est. from BAR
                    let (st, ap, gmch) = read_gmch_stolen_mb();
                    GMCH_RAW = gmch;
                    APER_EST_MB = ap;
                    if st > 0 {
                        VRAM_MB = st;
                    } else {
                        VRAM_MB = 64;
                    }
                    // Active FB from graphics
                    if graphics::ready() {
                        PREF.w = graphics::width() as u16;
                        PREF.h = graphics::height() as u16;
                        PREF.bpp = graphics::bpp();
                    }
                    let g = gen_from_did(did);
                    GEN = if g != 0 { g } else { 6 }; // unknown: prefer Gen6 (AH532 = SNB HD 3000 class)
                    READY = true;
                }
                serial::write_str("[iGPU] ");
                serial::write_hex(vid as usize);
                serial::write_str(":");
                serial::write_hex(did as usize);
                serial::write_str(" gen=");
                serial::write_usize(unsafe { GEN as usize });
                serial::write_str(" ");
                serial::write_str(did_name(did));
                serial::write_str("\n");
                return;
            }
        }
    }
    serial::write_str("[iGPU] no display PCI\n");
    // Still usable via LFB only
    unsafe {
        if graphics::ready() {
            PREF.w = graphics::width() as u16;
            PREF.h = graphics::height() as u16;
            PREF.bpp = graphics::bpp();
            READY = true;
        }
    }
}

pub fn ready() -> bool {
    unsafe { READY }
}
pub fn gen() -> u8 {
    unsafe { GEN }
}
pub fn did() -> u16 {
    unsafe { GPU.did }
}
pub fn vid() -> u16 {
    unsafe { GPU.vid }
}
pub fn gpu() -> GpuInfo {
    unsafe { GPU }
}
pub fn vram_mb() -> u32 {
    unsafe { VRAM_MB }
}
static mut GMCH_RAW: u32 = 0;
static mut APER_EST_MB: u32 = 0;
pub fn gmch_raw() -> u32 { unsafe { GMCH_RAW } }
pub fn aper_est_mb() -> u32 { unsafe { APER_EST_MB } }
pub fn refresh_vram() {
    let (st, ap, gmch) = read_gmch_stolen_mb();
    unsafe {
        GMCH_RAW = gmch;
        APER_EST_MB = ap;
        if st > 0 { VRAM_MB = st; }
    }
}
pub fn set_vram_mb(mb: u32) {
    unsafe { VRAM_MB = mb; }
}
pub fn pref() -> ModePref {
    unsafe { PREF }
}
pub fn set_pref(w: u16, h: u16, bpp: u8) {
    unsafe {
        PREF.w = w;
        PREF.h = h;
        PREF.bpp = bpp;
    }
}
pub fn chip_name() -> &'static str {
    unsafe {
        if !GPU.present {
            return "Generic LFB";
        }
        did_name(GPU.did)
    }
}
pub fn driver_name() -> &'static str {
    "Aether iGPU/LFB 1.0"
}

/// Supported mode list for UI (index)
pub fn mode_count() -> usize {
    let n = crate::drivers::intel_gmbus::mode_count();
    if n > 0 {
        n
    } else {
        // Built-in list + panel native if known
        let pw = crate::drivers::intel_kms::panel_width();
        let ph = crate::drivers::intel_kms::panel_height();
        if pw >= 640 && ph >= 480 { 5 } else { 4 }
    }
}
pub fn mode_at(i: usize) -> Option<(u16, u16, u8)> {
    let n = crate::drivers::intel_gmbus::mode_count();
    if n > 0 {
        return crate::drivers::intel_gmbus::mode_at(i).map(|(w, h, _hz)| (w, h, 32));
    }
    let pw = crate::drivers::intel_kms::panel_width();
    let ph = crate::drivers::intel_kms::panel_height();
    let has_panel = pw >= 640 && ph >= 480;
    if has_panel {
        match i {
            0 => Some((pw, ph, 32)), // native first
            1 => Some((800, 600, 32)),
            2 => Some((1024, 768, 32)),
            3 => Some((1280, 720, 32)),
            4 => Some((1366, 768, 32)),
            _ => None,
        }
    } else {
        match i {
            0 => Some((800, 600, 32)),
            1 => Some((1024, 768, 32)),
            2 => Some((1366, 768, 32)),
            3 => Some((1280, 720, 32)),
            _ => None,
        }
    }
}

/// Apply preference: records mode; hardware modeset is not available without full KMS.
/// Returns status code: 0=saved LFB-only, 1=already current, 2=need reboot/GRUB
pub fn apply_mode(w: u16, h: u16, bpp: u8) -> u8 {
    set_pref(w, h, bpp);
    // Delegate to KMS plane/path
    crate::drivers::intel_kms::apply(w, h, bpp)
}

pub fn active_fb_mb() -> u32 {
    if !graphics::ready() {
        return 0;
    }
    let sz = graphics::pitch().saturating_mul(graphics::height());
    (sz as u32) / (1024 * 1024)
}
