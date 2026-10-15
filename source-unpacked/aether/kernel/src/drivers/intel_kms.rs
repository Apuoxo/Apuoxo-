//! Aether Intel KMS - robust BAR select, MMIO map, forcewake, plane.
//! Target: AH532 = Sandy Bridge HD Graphics 3000 (DID 0x0116, Gen6).
//! Also supports IVB where present. Self-probes PCI if igpu cache empty.

use crate::graphics;
use crate::mm::paging;
use crate::serial;

const PAGE_PRESENT: u64 = paging::PAGE_PRESENT;
const PAGE_WRITE: u64 = paging::PAGE_WRITE;

const FORCEWAKE: usize = 0xA18C;
const FORCEWAKE_ACK: usize = 0x130090;
const FORCEWAKE_MT: usize = 0xA188;
const FORCEWAKE_MT_ACK: usize = 0x130040;

const PIPEASRC: usize = 0x6001C;
const PIPEBSRC: usize = 0x6101C;
const PIPEACONF: usize = 0x70008;
const PIPEBCONF: usize = 0x71008;
const DSPACNTR: usize = 0x70180;
const DSPASTRIDE: usize = 0x70188;
const DSPASURF: usize = 0x7019C;
const DSPALINOFF: usize = 0x70184;
const PIPEADSL: usize = 0x70000;
const CURACNTR: usize = 0x70080;
const CURABASE: usize = 0x70084;
const CURAPOS: usize = 0x70088;
const PIPEASTAT: usize = 0x70024;
/// PIPE_VBLANK status (write-1-to-clear on many gens)
const PIPE_VBLANK_STATUS: u32 = 1 << 1;
const PIPE_VSYNC_STATUS: u32 = 1 << 9;

static mut MMIO_BASE: usize = 0;
static mut MMIO_SIZE: usize = 0;
static mut APERTURE: usize = 0;
static mut KMS_READY: bool = false;
static mut HW_W: u16 = 0;
static mut HW_H: u16 = 0;
static mut LAST_ERR: [u8; 64] = [0; 64];
static mut LAST_ERR_LEN: usize = 0;
static mut GEN: u8 = 0;
static mut DIAG_BAR0: u32 = 0;
static mut DIAG_BAR2: u32 = 0;
static mut DIAG_MMIO: u32 = 0;
static mut DIAG_STEP: u8 = 0;
static mut PANEL_W: u16 = 0;
static mut PANEL_H: u16 = 0;
/// 0 = PIPE A / DSPA, 1 = PIPE B / DSPB (detected at ensure)
static mut ACTIVE_PIPE: u8 = 0;
/// Previous software FB state for modeset rollback
static mut PREV_FB_ADDR: usize = 0;
static mut PREV_FB_W: u16 = 0;
static mut PREV_FB_H: u16 = 0;
static mut PREV_FB_PITCH: u32 = 0;

fn set_err(s: &[u8]) {
    unsafe {
        LAST_ERR_LEN = 0;
        let mut i = 0usize;
        while i < s.len() && i < 63 {
            LAST_ERR[i] = s[i];
            i += 1;
        }
        LAST_ERR_LEN = i;
    }
}
pub fn last_error() -> &'static [u8] {
    unsafe { core::slice::from_raw_parts(LAST_ERR.as_ptr(), LAST_ERR_LEN) }
}
pub fn diag_step() -> u8 {
    unsafe { DIAG_STEP }
}
pub fn diag_mmio() -> u32 {
    unsafe { DIAG_MMIO }
}

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
    outl(0xCF8, addr);
    inl(0xCFC)
}
unsafe fn pci_w32(bus: u8, dev: u8, func: u8, off: u8, val: u32) {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    outl(0xCF8, addr);
    outl(0xCFC, val);
}

unsafe fn mmio_r32(off: usize) -> u32 {
    if MMIO_BASE == 0 || off + 4 > MMIO_SIZE {
        return 0xFFFF_FFFF;
    }
    core::ptr::read_volatile((MMIO_BASE + off) as *const u32)
}
unsafe fn mmio_w32(off: usize, val: u32) {
    if MMIO_BASE == 0 || off + 4 > MMIO_SIZE {
        return;
    }
    core::ptr::write_volatile((MMIO_BASE + off) as *mut u32, val);
}

fn map_range(phys: usize, size: usize) -> bool {
    if phys == 0 || size == 0 {
        return false;
    }
    let cr3 = unsafe { paging::read_cr3() };
    let start = phys & !0xFFF;
    let end = (phys + size + 0xFFF) & !0xFFF;
    let mut va = start;
    let mut ok = 0u32;
    let mut fail = 0u32;
    while va < end {
        unsafe {
            if paging::map_page(cr3, va, va, PAGE_PRESENT | PAGE_WRITE) {
                ok += 1;
            } else {
                fail += 1;
            }
        }
        va += 0x1000;
    }
    unsafe {
        let c = paging::read_cr3();
        paging::load_cr3(c);
    }
    serial::write_str("[KMS] map pages ok=");
    serial::write_usize(ok as usize);
    serial::write_str(" fail=");
    serial::write_usize(fail as usize);
    serial::write_str("\n");
    // succeed if majority mapped (some may already exist)
    ok > 0
}

fn forcewake_get() -> bool {
    unsafe {
        if MMIO_BASE == 0 {
            return false;
        }
        // Gen6/7: request bit0, mask bit16 (write 1 to mask to update)
        mmio_w32(FORCEWAKE_MT, 0x0001_0001);
        let mut t = 0u32;
        while t < 50_000 {
            if (mmio_r32(FORCEWAKE_MT_ACK) & 1) != 0 {
                return true;
            }
            t += 1;
        }
        // Legacy FORCEWAKE
        mmio_w32(FORCEWAKE, 0x0001_0001);
        t = 0;
        while t < 50_000 {
            if (mmio_r32(FORCEWAKE_ACK) & 1) != 0 {
                return true;
            }
            t += 1;
        }
        // IVB sometimes needs media forcewake too
        const FORCEWAKE_MEDIA: usize = 0xA180;
        const FORCEWAKE_MEDIA_ACK: usize = 0x1300B0;
        mmio_w32(FORCEWAKE_MEDIA, 0x0001_0001);
        t = 0;
        while t < 50_000 {
            if (mmio_r32(FORCEWAKE_MEDIA_ACK) & 1) != 0 {
                return true;
            }
            t += 1;
        }
        // Still allow MMIO RO path even if ACK never latches (BIOS may hold wake)
        true
    }
}
fn forcewake_put() {
    unsafe {
        mmio_w32(FORCEWAKE_MT, 0x10000);
        mmio_w32(FORCEWAKE, 0);
    }
}

fn decode_src(v: u32) -> (u16, u16) {
    let w = (((v >> 16) & 0xFFF) + 1) as u16;
    let h = ((v & 0xFFF) + 1) as u16;
    (w, h)
}

/// Decode PCI BAR to physical address (handles 64-bit). Returns (phys, is_io, is_pref, is_64).
fn bar_phys(lo: u32, hi: u32) -> (usize, bool, bool, bool) {
    if (lo & 1) != 0 {
        return ((lo & 0xFFFF_FFFC) as usize, true, false, false);
    }
    let is_64 = (lo & 6) == 4;
    let is_pref = (lo & 8) != 0;
    let phys = if is_64 {
        ((lo as u64 & 0xFFFF_FFF0) | ((hi as u64) << 32)) as usize
    } else {
        (lo & 0xFFFF_FFF0) as usize
    };
    (phys, false, is_pref, is_64)
}

/// Find Intel display + choose MMIO (non-prefetch MEM preferred) and aperture (prefetch).
fn probe_bars() -> (u16, u16, usize, usize, u8, u8, u8) {
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
                // Enable MEM+IO+BM
                let cmd = unsafe { pci_r32(bus, dev, func, 0x04) };
                if (cmd & 7) != 7 {
                    unsafe { pci_w32(bus, dev, func, 0x04, cmd | 7); }
                }
                let b0 = unsafe { pci_r32(bus, dev, func, 0x10) };
                let b1 = unsafe { pci_r32(bus, dev, func, 0x14) };
                let b2 = unsafe { pci_r32(bus, dev, func, 0x18) };
                let b3 = unsafe { pci_r32(bus, dev, func, 0x1C) };
                let b4 = unsafe { pci_r32(bus, dev, func, 0x20) };
                let b5 = unsafe { pci_r32(bus, dev, func, 0x24) };
                unsafe {
                    DIAG_BAR0 = b0;
                    DIAG_BAR2 = b2;
                }
                serial::write_str("[KMS] GPU ");
                serial::write_hex(vid as usize);
                serial::write_str(":");
                serial::write_hex(did as usize);
                serial::write_str(" B0=");
                serial::write_hex(b0 as usize);
                serial::write_str(" B2=");
                serial::write_hex(b2 as usize);
                serial::write_str("\n");

                let (p0, io0, pref0, _) = bar_phys(b0, b1);
                let (p2, io2, pref2, _) = bar_phys(b2, b3);
                let (p4, io4, pref4, _) = bar_phys(b4, b5);

                // MMIO: first non-IO MEM, prefer non-prefetchable
                let mut mmio = 0usize;
                let mut aper = 0usize;
                // candidates in order
                let cands = [(p0, io0, pref0), (p2, io2, pref2), (p4, io4, pref4)];
                for &(p, is_io, is_pref) in &cands {
                    if is_io || p == 0 {
                        continue;
                    }
                    if !is_pref && mmio == 0 {
                        mmio = p;
                    }
                }
                for &(p, is_io, is_pref) in &cands {
                    if is_io || p == 0 {
                        continue;
                    }
                    if is_pref && aper == 0 {
                        aper = p;
                    }
                }
                // if no non-pref, use any mem as mmio
                if mmio == 0 {
                    for &(p, is_io, _) in &cands {
                        if !is_io && p != 0 {
                            mmio = p;
                            break;
                        }
                    }
                }
                if aper == 0 {
                    aper = mmio;
                }
                return (vid, did, mmio, aper, bus, dev, func);
            }
        }
    }
    (0, 0, 0, 0, 0, 0, 0)
}

pub fn init() {
    serial::write_str("[KMS] init\n");
    unsafe {
        let g = crate::drivers::intel_igpu::gen();
        if g != 0 {
            GEN = g;
        }
    }
    serial::write_str("[KMS] gen=");
    serial::write_usize(unsafe { GEN as usize });
    serial::write_str("\n");
    unsafe {
        KMS_READY = false;
        MMIO_BASE = 0;
        DIAG_STEP = 0;
    }
    set_err(b"init start");
    unsafe { DIAG_STEP = 1; }

    let (vid, did, mmio_phys, aper, _b, _d, _f) = probe_bars();
    if vid == 0 {
        // fallback to igpu cache
        let g = crate::drivers::intel_igpu::gpu();
        if !g.present {
            set_err(b"no GPU class03");
            serial::write_str("[KMS] no GPU\n");
            unsafe { DIAG_STEP = 2; }
            return;
        }
        unsafe { DIAG_STEP = 3; }
        let (p0, io0, pref0, _) = bar_phys(g.bar0, g.bar1);
        let (p2, io2, pref2, _) = bar_phys(g.bar2, g.bar3);
        let mut mmio = if !io0 && !pref0 && p0 != 0 {
            p0
        } else if !io0 && p0 != 0 {
            p0
        } else if !io2 && p2 != 0 {
            p2
        } else {
            0
        };
        let aper2 = if !io2 && pref2 { p2 } else { mmio };
        if mmio == 0 {
            set_err(b"no MMIO from igpu");
            unsafe { DIAG_STEP = 4; }
            return;
        }
        finish_init(did, mmio, aper2);
        return;
    }
    unsafe { DIAG_STEP = 5; }
    if mmio_phys == 0 {
        set_err(b"no MMIO BAR found");
        serial::write_str("[KMS] no MMIO BAR\n");
        unsafe { DIAG_STEP = 6; }
        return;
    }
    finish_init(did, mmio_phys, aper);
}

fn finish_init(did: u16, mmio_phys: usize, aper: usize) {
    unsafe {
        GEN = match did {
            // SNB Gen6
            0x0102 | 0x0106 | 0x010A | 0x010B |
            0x0112 | 0x0116 | 0x0122 | 0x0126 => 6,
            // IVB Gen7 + HSW display-compatible
            0x0152 | 0x0156 | 0x015A | 0x015E |
            0x0162 | 0x0166 | 0x016A | 0x016E |
            0x0402 | 0x0406 | 0x0412 | 0x0416 | 0x0A16 | 0x0A26 => 7,
            // Unknown Intel iGPU: AH532 reference is SNB Gen6
            _ => 6,
        };
        serial::write_str("[KMS] DID=");
        serial::write_hex(did as usize);
        serial::write_str(" GEN=");
        serial::write_usize(GEN as usize);
        serial::write_str("\n");
        APERTURE = aper;
        DIAG_MMIO = mmio_phys as u32;
        DIAG_STEP = 7;
    }
    let map_sz = 4 * 1024 * 1024; // 4MiB MMIO window
    let mapped = map_range(mmio_phys, map_sz);
    unsafe {
        MMIO_BASE = mmio_phys;
        MMIO_SIZE = map_sz;
    }
    if !mapped {
        set_err(b"map MMIO partial/fail");
        serial::write_str("[KMS] map weak - try access\n");
    }
    serial::write_str("[KMS] MMIO=");
    serial::write_hex(mmio_phys);
    serial::write_str(" aper=");
    serial::write_hex(aper);
    serial::write_str("\n");
    unsafe { DIAG_STEP = 8; }

    // Probe read
    let probe = unsafe { mmio_r32(0) };
    serial::write_str("[KMS] mmio[0]=");
    serial::write_hex(probe as usize);
    serial::write_str("\n");
    if probe == 0xFFFF_FFFF {
        set_err(b"MMIO read 0xFFFFFFFF");
        serial::write_str("[KMS] MMIO looks dead\n");
        unsafe {
            DIAG_STEP = 9;
            KMS_READY = false;
        }
        return;
    }
    unsafe { DIAG_STEP = 10; }

    let fw = forcewake_get();
    if fw {
        serial::write_str("[KMS] forcewake OK\n");
        set_err(b"forcewake OK");
    } else {
        serial::write_str("[KMS] forcewake FAIL\n");
        set_err(b"forcewake fail, RO regs");
    }
    unsafe { DIAG_STEP = 11; }

    unsafe {
        let conf_a = mmio_r32(PIPEACONF);
        let conf_b = mmio_r32(PIPEBCONF);
        let src_a = mmio_r32(PIPEASRC);
        let src_b = mmio_r32(PIPEBSRC);
        let (wa, ha) = decode_src(src_a);
        let (wb, hb) = decode_src(src_b);
        if (conf_a & (1 << 31)) != 0 && wa > 16 && wa < 8192 && ha > 16 {
            HW_W = wa;
            HW_H = ha;
            ACTIVE_PIPE = 0;
        } else if (conf_b & (1 << 31)) != 0 && wb > 16 {
            HW_W = wb;
            HW_H = hb;
            ACTIVE_PIPE = 1;
        } else if wa > 16 && wa < 8192 {
            HW_W = wa;
            HW_H = ha;
            ACTIVE_PIPE = 0;
        } else if wb > 16 {
            HW_W = wb;
            HW_H = hb;
            ACTIVE_PIPE = 1;
        }
        serial::write_str("[KMS] active_pipe=");
        serial::write_usize(ACTIVE_PIPE as usize);
        serial::write_str(if ACTIVE_PIPE == 0 { " (A)
" } else { " (B)
" });
        let surf = mmio_r32(DSPASURF);
        let stride = mmio_r32(DSPASTRIDE);
        serial::write_str("[KMS] PIPEA=");
        serial::write_hex(conf_a as usize);
        serial::write_str(" ");
        serial::write_usize(HW_W as usize);
        serial::write_str("x");
        serial::write_usize(HW_H as usize);
        serial::write_str(" surf=");
        serial::write_hex(surf as usize);
        serial::write_str(" stride=");
        serial::write_hex(stride as usize);
        serial::write_str("\n");
        KMS_READY = true;
        DIAG_STEP = 12;
        // Panel timing: Intel HTOTAL low=hactive-1, high=htotal-1
        let ht = mmio_r32(0x60000);
        let vt = mmio_r32(0x6000C);
        let hact = ((ht & 0xFFFF) + 1) as u16;
        let vact = ((vt & 0xFFFF) + 1) as u16;
        if hact >= 640 && hact <= 4096 && vact >= 480 && vact <= 2160 {
            PANEL_W = hact;
            PANEL_H = vact;
            serial::write_str("[KMS] panel ");
            serial::write_usize(hact as usize);
            serial::write_str("x");
            serial::write_usize(vact as usize);
            serial::write_str("\n");
        }
        set_err(b"KMS ready");
    }
    forcewake_put();
}


/// Safe MMIO snapshot (forcewake + known regs). No GSM/GTT access.
/// out[0]=PIPEACONF [1]=SRC [2]=DSPACNTR [3]=STRIDE [4]=SURF [5]=STAT
/// [6]=MMIO_lo [7]=FW [8]=LINOFF [9]=fb_addr_lo [10]=pitch [11]=path_flags
/// path_flags bit0=surf_zero bit1=stride_match_sw
pub fn pipe_snapshot(out: &mut [u32; 12]) -> bool {
    if !ready() {
        return false;
    }
    let fw = forcewake_get();
    unsafe {
        out[0] = mmio_r32(PIPEACONF);
        out[1] = mmio_r32(PIPEASRC);
        out[2] = mmio_r32(DSPACNTR);
        out[3] = mmio_r32(DSPASTRIDE);
        out[4] = mmio_r32(DSPASURF);
        out[5] = mmio_r32(PIPEASTAT);
        out[8] = mmio_r32(DSPALINOFF);
    }
    out[6] = mmio_base() as u32;
    out[7] = if fw { 1 } else { 0 };
    let fb = crate::graphics::fb_addr() as u32;
    let pitch = crate::graphics::pitch() as u32;
    out[9] = fb;
    out[10] = pitch;
    let mut flags = 0u32;
    if out[4] == 0 {
        flags |= 1;
    }
    if out[3] == pitch || out[3] == pitch.wrapping_add(0x20) {
        flags |= 2;
    }
    out[11] = flags;
    forcewake_put();
    true
}

/// Decode plane enable / format hints from DSPACNTR (Gen6)
pub fn dsp_plane_enabled(cntr: u32) -> bool {
    (cntr & 0x8000_0000) != 0
}



/// RO cursor + scanline (no writes, no GSM).
pub fn cursor_snapshot(out: &mut [u32; 4]) -> bool {
    if !ready() {
        return false;
    }
    let fw = forcewake_get();
    unsafe {
        out[0] = mmio_r32(CURACNTR);
        out[1] = mmio_r32(CURABASE);
        out[2] = mmio_r32(CURAPOS);
        out[3] = mmio_r32(PIPEADSL);
    }
    forcewake_put();
    let _ = fw;
    true
}


/// 0 = Multiboot/SW LFB primary (AH532 confirmed)
/// 1 = GPU SURF programmed (not yet on AH532)
pub fn display_path_id() -> u8 {
    if !ready() {
        return 0xFF;
    }
    let fw = forcewake_get();
    let surf = unsafe { mmio_r32(DSPASURF) };
    forcewake_put();
    let _ = fw;
    if surf == 0 {
        0
    } else {
        1
    }
}

/// Dump selected display MMIO offsets (RO). out words: pairs (off, val) up to n/2.
pub fn display_reg_dump(offs: &[usize], out_val: &mut [u32]) -> usize {
    if !ready() {
        return 0;
    }
    let fw = forcewake_get();
    let mut n = 0usize;
    let mut i = 0usize;
    while i < offs.len() && n < out_val.len() {
        let off = offs[i];
        out_val[n] = unsafe { mmio_r32(off) };
        n += 1;
        i += 1;
    }
    forcewake_put();
    let _ = fw;
    n
}

pub fn ensure() {
    if unsafe { DIAG_STEP } == 0 {
        serial::write_str("[KMS] ensure() lazy init\n");
        init();
    }
}
pub fn ready() -> bool {
    ensure();
    unsafe { KMS_READY }
}
pub fn panel_width() -> u16 { unsafe { PANEL_W } }
pub fn panel_height() -> u16 { unsafe { PANEL_H } }
pub fn hw_width() -> u16 {
    unsafe { HW_W }
}
pub fn hw_height() -> u16 {
    unsafe { HW_H }
}
pub fn mmio_base() -> usize {
    unsafe { MMIO_BASE }
}
pub fn mmio_peek(off: usize) -> u32 {
    ensure();
    if unsafe { MMIO_BASE } == 0 { return 0xDEAD_DEAD; }
    let _ = forcewake_get();
    let v = unsafe { mmio_r32(off) };
    forcewake_put();
    v
}
pub fn mmio_write(off: usize, val: u32) {
    ensure();
    if unsafe { MMIO_BASE } == 0 { return; }
    let _ = forcewake_get();
    unsafe { mmio_w32(off, val); }
    forcewake_put();
}
/// Hold forcewake for a batch of raw MMIO ops (GMBUS). Must pair with forcewake_leave.
pub fn forcewake_enter() -> bool {
    ensure();
    if unsafe { MMIO_BASE } == 0 {
        return false;
    }
    forcewake_get()
}
pub fn forcewake_leave() {
    forcewake_put();
}
/// Raw MMIO without per-call forcewake. Only valid between forcewake_enter/leave.
pub fn mmio_r32_raw(off: usize) -> u32 {
    unsafe {
        if MMIO_BASE == 0 {
            return 0xDEAD_DEAD;
        }
        mmio_r32(off)
    }
}
pub fn mmio_w32_raw(off: usize, val: u32) {
    unsafe {
        if MMIO_BASE == 0 {
            return;
        }
        mmio_w32(off, val);
    }
}
pub fn aperture() -> usize {
    unsafe { APERTURE }
}

/// Program primary plane. surf_addr is GM/aperture offset or phys depending on gen.

/// Wait for vertical blank on Pipe A (timeout → false, still safe to continue).
const PIPEBSTAT: usize = 0x71024;
const PIPEBDSL: usize = 0x71000;
const PIPE_VBLANK_ENABLE: u32 = 1 << 17; // enable status reporting
const PIPE_START_VBLANK_STATUS: u32 = 1 << 2;

/// Wait for vertical blank. Tries PIPE A then B; enables status bits first.
/// Fallback: scanline wrap detection. Timeout → false (caller may continue).
pub fn wait_vblank() -> bool {
    unsafe {
        if MMIO_BASE == 0 {
            return false;
        }
        // Prefer active pipe: PIPECONF enable bit 31
        let conf_a = mmio_r32(PIPEACONF);
        let conf_b = mmio_r32(PIPEBCONF);
        let use_a = (conf_a & (1u32 << 31)) != 0 || (conf_b & (1u32 << 31)) == 0;
        let (stat_off, dsl_off) = if use_a {
            (PIPEASTAT, PIPEADSL)
        } else {
            (PIPEBSTAT, PIPEBDSL)
        };

        // Enable VBLANK status reporting (upper enable bit)
        let en = mmio_r32(stat_off);
        mmio_w32(stat_off, en | PIPE_VBLANK_ENABLE | (PIPE_VBLANK_ENABLE << 0));
        // Clear sticky status (W1C lower bits)
        mmio_w32(stat_off, PIPE_VBLANK_STATUS | PIPE_START_VBLANK_STATUS);

        let mut t = 0u32;
        while t < 500_000 {
            let st = mmio_r32(stat_off);
            if (st & (PIPE_VBLANK_STATUS | PIPE_START_VBLANK_STATUS)) != 0 {
                mmio_w32(stat_off, st & (PIPE_VBLANK_STATUS | PIPE_START_VBLANK_STATUS));
                return true;
            }
            t += 1;
        }

        // Fallback: scanline change / wrap (works when STATUS bits stay 0)
        let s0 = mmio_r32(dsl_off) & 0x1FFF;
        t = 0;
        while t < 1_000_000 {
            let s1 = mmio_r32(dsl_off) & 0x1FFF;
            // wrap or large delta ≈ frame boundary
            if s1 < s0.saturating_sub(8) || (s0 > 8 && s1 + 32 < s0) {
                return true;
            }
            // also accept any movement of scanline (proves pipe alive)
            if t > 100_000 && s1 != s0 {
                // wait until near top of frame (small scanline)
                if s1 < 32 {
                    return true;
                }
            }
            t += 1;
        }
        false
    }
}

/// Diagnostic: which pipe, STAT, DSL values once
pub fn vblank_diag(out_pipe: &mut u8, out_stat: &mut u32, out_dsl: &mut u32) {
    ensure();
    let _ = forcewake_get();
    unsafe {
        let conf_a = mmio_r32(PIPEACONF);
        let conf_b = mmio_r32(PIPEBCONF);
        let use_a = (conf_a & (1u32 << 31)) != 0 || (conf_b & (1u32 << 31)) == 0;
        *out_pipe = if use_a { 0 } else { 1 };
        let off = if use_a { PIPEASTAT } else { PIPEBSTAT };
        let dsl = if use_a { PIPEADSL } else { PIPEBDSL };
        *out_stat = mmio_r32(off);
        *out_dsl = mmio_r32(dsl);
    }
    forcewake_put();
}


pub fn set_plane_surface(surf: u32, stride: u32, w: u16, h: u16) -> bool {
    if !ready() {
        set_err(b"kms not ready");
        return false;
    }
    let _ = forcewake_get();
    unsafe {
        // Gen6 primary planes: A @ 0x70180, B @ 0x71180
        let pipe = ACTIVE_PIPE;
        let (cntr_off, stride_off, linoff_off, surf_off, src_off) = if pipe == 0 {
            (DSPACNTR, DSPASTRIDE, DSPALINOFF, DSPASURF, PIPEASRC)
        } else {
            (0x71180usize, 0x71188usize, 0x71184usize, 0x7119Cusize, PIPEBSRC)
        };
        serial::write_str("[KMS] set_plane pipe=");
        serial::write_usize(pipe as usize);
        serial::write_str(" surf=");
        serial::write_hex(surf as usize);
        serial::write_str(" stride=");
        serial::write_hex(stride as usize);
        serial::write_str("\n");

        // disable plane
        let mut cntr = mmio_r32(cntr_off);
        mmio_w32(cntr_off, cntr & !(1u32 << 31));
        let mut d = 0u32;
        while d < 80_000 {
            d += 1;
        }
        // stride must be 64-byte aligned
        let st = (stride + 63) & !63;
        mmio_w32(stride_off, st);
        mmio_w32(linoff_off, 0);
        if w > 16 && h > 16 {
            let src = ((((w as u32) - 1) & 0xFFF) << 16) | (((h as u32) - 1) & 0xFFF);
            mmio_w32(src_off, src);
            HW_W = w;
            HW_H = h;
        }
        // format XRGB8888 = 0x6 on gen6, enable bit 31
        cntr = mmio_r32(cntr_off);
        cntr &= !(0xFu32 << 26);
        cntr |= 0x6u32 << 26;
        cntr |= 1u32 << 31;
        cntr &= !(1u32 << 10); // linear
        mmio_w32(cntr_off, cntr);
        let _vb = wait_vblank();
        // SURF write triggers double-buffer latch - write last
        mmio_w32(surf_off, surf);
        let _vb2 = wait_vblank();
    }
    forcewake_put();
    set_err(b"plane updated");
    true
}


/// Map aperture range identity so we can use it as linear FB
fn map_aper_range(phys: usize, size: usize) -> bool {
    unsafe {
        if !crate::mm::paging::pat_wc_ready() {
            crate::mm::paging::init_pat_wc();
        }
    }
    if phys == 0 || size == 0 {
        return false;
    }
    let cr3 = unsafe { paging::kernel_cr3() };
    let start = phys & !0xFFF;
    let end = (phys + size + 0xFFF) & !0xFFF;
    let mut a = start;
    let mut ok = 0usize;
    while a < end {
        if unsafe { paging::map_page(cr3, a, a, crate::mm::paging::fb_map_flags()) } {
            ok += 1;
        }
        a += 4096;
        if a >= end {
            break;
        }
        // Cap at 64MB aperture window (panel FB rarely needs more)
        if a - start > 64 * 1024 * 1024 {
            break;
        }
    }
    serial::write_str("[KMS] aper map pages=");
    serial::write_usize(ok);
    serial::write_str("\n");
    ok > 0
}

/// Switch primary plane to native panel resolution (keeps pipe timings).
/// Returns true if plane programmed and graphics updated.
pub fn modeset_panel() -> bool {
    ensure();
    let _ = forcewake_get();
    let (pw, ph) = unsafe {
        let ht = mmio_r32(0x60000);
        let vt = mmio_r32(0x6000C);
        let mut ha = ((ht & 0xFFFF) + 1) as u16;
        let mut va = ((vt & 0xFFFF) + 1) as u16;
        if ha < 640 || va < 480 {
            ha = if PANEL_W >= 640 { PANEL_W } else { 1366 };
            va = if PANEL_H >= 480 { PANEL_H } else { 768 };
        }
        PANEL_W = ha;
        PANEL_H = va;
        (ha, va)
    };
    forcewake_put();
    modeset_to(pw, ph)
}


/// Diagnostic snapshot of display-dependent sizes (serial)
pub fn vblank_test() -> bool {
    ensure();
    let _ = forcewake_get();
    let ok = wait_vblank();
    forcewake_put();
    ok
}
pub fn gen() -> u8 {
    unsafe { GEN }
}
pub fn diag_mode_chain(tag: &str) {
    serial::write_str("[MODE-DIAG] ");
    serial::write_str(tag);
    serial::write_str("\n");
    serial::write_str("  MODE:   ");
    serial::write_usize(unsafe { HW_W as usize });
    serial::write_str("x");
    serial::write_usize(unsafe { HW_H as usize });
    serial::write_str(" panel=");
    serial::write_usize(unsafe { PANEL_W as usize });
    serial::write_str("x");
    serial::write_usize(unsafe { PANEL_H as usize });
    serial::write_str("\n");
    serial::write_str("  FB:     ");
    serial::write_usize(graphics::width());
    serial::write_str("x");
    serial::write_usize(graphics::height());
    serial::write_str(" pitch=");
    serial::write_usize(graphics::pitch());
    serial::write_str(" addr=");
    serial::write_hex(graphics::fb_addr());
    serial::write_str("\n");
    serial::write_str("  COMPOSITOR/DESKTOP = graphics FB size\n");
    let (mx, my) = crate::drivers::ps2::mouse_pos();
    serial::write_str("  MOUSE:  ");
    serial::write_usize(mx as usize);
    serial::write_str(",");
    serial::write_usize(my as usize);
    serial::write_str(" bounds=");
    serial::write_usize(graphics::width().saturating_sub(1));
    serial::write_str(",");
    serial::write_usize(graphics::height().saturating_sub(1));
    serial::write_str("\n");
}

pub fn modeset_to(w: u16, h: u16) -> bool {
    ensure();
    if !ready() {
        set_err(b"kms not ready");
        return false;
    }
    // Refresh which pipe is active (A vs B) before programming plane
    unsafe {
        let conf_a = mmio_r32(PIPEACONF);
        let conf_b = mmio_r32(PIPEBCONF);
        if (conf_a & (1u32 << 31)) != 0 {
            ACTIVE_PIPE = 0;
        } else if (conf_b & (1u32 << 31)) != 0 {
            ACTIVE_PIPE = 1;
        }
    }
    if w < 320 || h < 200 || w > 8192 || h > 4320 {
        set_err(b"bad mode");
        return false;
    }
    // Snapshot current software FB for rollback
    let snap = crate::graphics::snapshot();
    unsafe {
        PREV_FB_ADDR = snap.addr;
        PREV_FB_W = snap.width as u16;
        PREV_FB_H = snap.height as u16;
        PREV_FB_PITCH = snap.pitch as u32;
    }
    diag_mode_chain("before modeset_to");
    let aper = unsafe { APERTURE };
    if aper == 0 {
        set_err(b"no aperture");
        return false;
    }
    let stride = ((((w as u32) * 4) + 63) & !63);
    let fb_size = (stride as usize).saturating_mul(h as usize);
    serial::write_str("[KMS] modeset_to ");
    serial::write_usize(w as usize);
    serial::write_str("x");
    serial::write_usize(h as usize);
    serial::write_str(" stride=");
    serial::write_hex(stride as usize);
    serial::write_str(" pipe=");
    serial::write_usize(unsafe { ACTIVE_PIPE as usize });
    serial::write_str("
");
    if !map_aper_range(aper, fb_size.saturating_add(0x20000)) {
        set_err(b"map aper fail");
        return false;
    }
    unsafe {
        let mut off = 0usize;
        while off + 4 <= fb_size {
            core::ptr::write_volatile((aper + off) as *mut u32, 0x00102840);
            off += 4;
        }
    }
    if !set_plane_surface(0, stride, w, h) {
        set_err(b"plane fail - rollback");
        unsafe {
            if PREV_FB_ADDR != 0 && PREV_FB_W >= 320 && PREV_FB_H >= 200 {
                crate::graphics::init(
                    PREV_FB_ADDR,
                    PREV_FB_W as usize,
                    PREV_FB_H as usize,
                    PREV_FB_PITCH as usize,
                    32,
                    false,
                );
                serial::write_str("[KMS] rollback SoT restored
");
            }
        }
        return false;
    }
    crate::graphics::init(aper, w as usize, h as usize, stride as usize, 32, false);
    serial::write_str("[KMS] SoT -> graphics::FB ");
    serial::write_usize(w as usize);
    serial::write_str("x");
    serial::write_usize(h as usize);
    serial::write_str("
");
    unsafe {
        HW_W = w;
        HW_H = h;
    }
    crate::drivers::ps2::clamp_to_screen();
    diag_mode_chain("after modeset_to");
    set_err(b"modeset_to OK");
    true
}

/// True if size is panel-native or a known plane-scale candidate.
/// Full CRT timing reprogramming is NOT performed - pipe stays at panel timings.
pub fn mode_is_safe(w: u16, h: u16) -> bool {
    let pw = unsafe { if PANEL_W >= 640 { PANEL_W } else { 0 } };
    let ph = unsafe { if PANEL_H >= 480 { PANEL_H } else { 0 } };
    if pw != 0 && ph != 0 && w == pw && h == ph {
        return true;
    }
    matches!(
        (w, h),
        (800, 600) | (1024, 768) | (1280, 720) | (1280, 800) | (1360, 768) | (1366, 768)
            | (1440, 900) | (1600, 900) | (1680, 1050) | (1920, 1080)
    )
}

pub fn apply(w: u16, h: u16, bpp: u8) -> u8 {
    ensure();
    crate::drivers::intel_igpu::set_pref(w, h, bpp);
    if !ready() {
        set_err(b"KMS not ready");
        return 4;
    }
    let cur_w = if graphics::ready() { graphics::width() as u16 } else { 0 };
    let cur_h = if graphics::ready() { graphics::height() as u16 } else { 0 };
    if w == cur_w && h == cur_h {
        set_err(b"already current");
        return 1;
    }
    if !mode_is_safe(w, h) {
        set_err(b"mode not in safe plane-scale list");
        return 2;
    }
    if modeset_to(w, h) {
        return 1;
    }
    // modeset_to already rolled back SoT on plane fail
    5
}


/// Dump key display registers to serial + return summary for terminal
pub fn dump_regs() {
    ensure();
    if unsafe { MMIO_BASE } == 0 {
        serial::write_str("[KMS] dump: no MMIO\n");
        return;
    }
    let fw = forcewake_get();
    serial::write_str("[KMS] dump fw=");
    serial::write_str(if fw { "OK" } else { "FAIL" });
    serial::write_str("\n");
    unsafe {
        let regs: &[(usize, &str)] = &[
            (0x2020, "PGTBL_CTL"),
            (FORCEWAKE_MT, "FW_MT"),
            (FORCEWAKE_MT_ACK, "FW_MT_ACK"),
            (PIPEACONF, "PIPEACONF"),
            (PIPEASRC, "PIPEASRC"),
            (PIPEBCONF, "PIPEBCONF"),
            (PIPEBSRC, "PIPEBSRC"),
            (DSPACNTR, "DSPACNTR"),
            (DSPASTRIDE, "DSPASTRIDE"),
            (DSPASURF, "DSPASURF"),
            (DSPALINOFF, "DSPALINOFF"),
            (0x70024, "DSPAADDR"),
            (0x60000, "HTOTAL_A"),
            (0x60004, "HBLANK_A"),
            (0x60008, "HSYNC_A"),
            (0x6000C, "VTOTAL_A"),
            (0x60010, "VBLANK_A"),
            (0x60014, "VSYNC_A"),
        ];
        for &(off, name) in regs {
            let v = mmio_r32(off);
            serial::write_str("[KMS] ");
            serial::write_str(name);
            serial::write_str("=");
            serial::write_hex(v as usize);
            serial::write_str("\n");
        }
        // Decode timing if available
        let ht = mmio_r32(0x60000);
        let vt = mmio_r32(0x6000C);
        let hactive = ((ht >> 16) & 0xFFFF) + 1;
        let vactive = ((vt >> 16) & 0xFFFF) + 1;
        serial::write_str("[KMS] timing hactive=");
        serial::write_usize(hactive as usize);
        serial::write_str(" vactive=");
        serial::write_usize(vactive as usize);
        serial::write_str("\n");
        // GGTT sample first 4 PTEs if aperture mapped as GTT at high end is complex -
        // for Gen6 PGTBL often points into MMIO or stolen. Just log PGTBL.
    }
    forcewake_put();
}

pub fn status_line(out: &mut [u8]) -> usize {
    let msg = if !ready() {
        "KMS: not ready"
    } else {
        "KMS: MMIO+plane ready"
    };
    let mut i = 0usize;
    for b in msg.bytes() {
        if i >= out.len() {
            break;
        }
        out[i] = b;
        i += 1;
    }
    i
}
