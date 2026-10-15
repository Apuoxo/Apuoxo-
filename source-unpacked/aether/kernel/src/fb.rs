//! Multiboot2 framebuffer discovery only.
//! After init, ALL drawing and size queries go through `graphics::FB`
//! (single source of truth). Local FB_* are bootstrap mirrors only.

use crate::serial;
use crate::mm;
use crate::mm::paging;

const MB2_TAG_FB: u32 = 8;
const MB2_TAG_END: u32 = 0;

static mut FB_ADDR: usize = 0;
static mut FB_W: usize = 0;
static mut FB_H: usize = 0;
static mut FB_PITCH: usize = 0;
static mut FB_BPP: u8 = 0;
static mut FB_OK: bool = false;

pub fn is_ready() -> bool {
    // Single source of truth after init: graphics::FB
    if crate::graphics::ready() {
        return true;
    }
    unsafe { FB_OK }
}

pub fn width() -> usize {
    if crate::graphics::ready() {
        return crate::graphics::width();
    }
    unsafe { FB_W }
}
pub fn height() -> usize {
    if crate::graphics::ready() {
        return crate::graphics::height();
    }
    unsafe { FB_H }
}

pub fn pitch() -> usize {
    if crate::graphics::ready() {
        return crate::graphics::pitch();
    }
    unsafe { FB_PITCH }
}
pub fn addr() -> usize {
    if crate::graphics::ready() {
        return crate::graphics::fb_addr();
    }
    unsafe { FB_ADDR }
}

fn vga_fb_mark(col: usize, a: u8, b: u8) {
    // write two chars at row 1
    unsafe {
        let p = (0xB8000 + 80 * 2 + col * 2) as *mut u16;
        *p = 0x0E00u16 | (a as u16);
        let p2 = (0xB8000 + 80 * 2 + (col + 1) * 2) as *mut u16;
        *p2 = 0x0E00u16 | (b as u16);
    }
}

fn read_u32(p: usize) -> u32 {
    unsafe { core::ptr::read_unaligned(p as *const u32) }
}
fn read_u64(p: usize) -> u64 {
    unsafe { core::ptr::read_unaligned(p as *const u64) }
}
fn read_u8(p: usize) -> u8 {
    unsafe { *(p as *const u8) }
}

/// Map physical FB range into current page tables (identity, USER not needed)
fn map_fb_range(phys: usize, size: usize) -> bool {
    if phys == 0 || size == 0 {
        return false;
    }
    unsafe {
        if !paging::pat_wc_ready() {
            paging::init_pat_wc();
        }
    }
    let cr3 = unsafe { paging::read_cr3() };
    let start = phys & !0xFFF;
    let end = (phys + size + 0xFFF) & !0xFFF;
    let mut va = start;
    while va < end {
        // already identity-mapped below 1GiB?
        if va < 0x4000_0000 {
            va += 0x1000;
            continue;
        }
        // map 4K page at identity
        if !unsafe {
            paging::map_page(
                cr3,
                va,
                va,
                paging::fb_map_flags(),
            )
        } {
            // may already be mapped - try continue
        }
        va += 0x1000;
    }
    unsafe { paging::load_cr3(cr3) };
    true
}


/// Parse Multiboot2 info; return true if usable 32bpp RGB FB found
pub fn init_from_mbi(mbi: usize) -> bool {
    serial::write_str("\n[FB] Multiboot2 scan...\n");
    if mbi == 0 || mbi < 0x1000 {
        serial::write_str("[FBX] no MBI pointer\n");
        vga_fb_mark(0, b'F', b'X');
        return false;
    }

    // total_size at mbi+0
    let total = read_u32(mbi) as usize;
    if total < 16 || total > 0x100000 {
        serial::write_str("[FBX] bad MBI size\n");
        vga_fb_mark(0, b'F', b'X');
        return false;
    }

    let mut off = 8usize;
    while off + 8 <= total {
        let tag_type = read_u32(mbi + off);
        let tag_size = read_u32(mbi + off + 4) as usize;
        if tag_size < 8 || off + tag_size > total {
            break;
        }
        if tag_type == MB2_TAG_END {
            break;
        }
        if tag_type == MB2_TAG_FB && tag_size >= 28 {
            let addr = read_u64(mbi + off + 8) as usize;
            let pitch = read_u32(mbi + off + 16) as usize;
            let width = read_u32(mbi + off + 20) as usize;
            let height = read_u32(mbi + off + 24) as usize;
            let bpp = read_u8(mbi + off + 28);
            // Multiboot2: type 0=indexed 1=RGB 2=EGA text
            let fb_type = if tag_size >= 30 { read_u8(mbi + off + 29) } else { 1u8 };
            let (rpos, rsz, gpos, gsz, bpos, bsz) = if fb_type == 1 && tag_size >= 38 {
                (
                    read_u8(mbi + off + 32),
                    read_u8(mbi + off + 33),
                    read_u8(mbi + off + 34),
                    read_u8(mbi + off + 35),
                    read_u8(mbi + off + 36),
                    read_u8(mbi + off + 37),
                )
            } else {
                (16u8, 8u8, 8u8, 8u8, 0u8, 8u8) // typical XRGB8888
            };
            serial::write_str("[FB0] detected addr=");
            serial::write_hex(addr);
            serial::write_str(" ");
            serial::write_usize(width);
            serial::write_str("x");
            serial::write_usize(height);
            serial::write_str(" pitch=");
            serial::write_usize(pitch);
            serial::write_str(" bpp=");
            serial::write_usize(bpp as usize);
            serial::write_str(" type=");
            serial::write_usize(fb_type as usize);
            serial::write_str(" RGB ");
            serial::write_usize(rpos as usize);
            serial::write_str("/");
            serial::write_usize(rsz as usize);
            serial::write_str(" ");
            serial::write_usize(gpos as usize);
            serial::write_str("/");
            serial::write_usize(gsz as usize);
            serial::write_str(" ");
            serial::write_usize(bpos as usize);
            serial::write_str("/");
            serial::write_usize(bsz as usize);
            serial::write_str("\n");
            vga_fb_mark(0, b'F', b'0');

            // Validate: pitch (bytes) >= width * (bpp/8)
            let bpp_bytes = if bpp == 32 { 4usize } else if bpp == 24 { 3usize } else { 0usize };
            let min_pitch = width.saturating_mul(bpp_bytes);
            if addr == 0
                || width < 320
                || width > 8192
                || height < 200
                || height > 4320
                || bpp_bytes == 0
                || pitch < min_pitch
                || (fb_type != 1 && fb_type != 0)
                || (bpp != 32 && bpp != 24)
            {
                serial::write_str("[FBX] parameters invalid pitch/type/bpp\n");
                serial::write_str("[FBX] min_pitch=");
                serial::write_usize(min_pitch);
                serial::write_str(" got_pitch=");
                serial::write_usize(pitch);
                serial::write_str("\n");
                vga_fb_mark(0, b'F', b'X');
                return false;
            }
            if pitch > min_pitch.saturating_mul(2) {
                serial::write_str("[FB] note: large pitch padding\n");
            }
            vga_fb_mark(0, b'F', b'1');
            serial::write_str("[FB1] parameters valid\n");

            let fb_size = pitch.saturating_mul(height);
            if fb_size == 0 || fb_size > 32 * 1024 * 1024 {
                serial::write_str("[FBX] size too large\n");
                vga_fb_mark(0, b'F', b'X');
                return false;
            }

            if !map_fb_range(addr, fb_size) {
                serial::write_str("[FBX] map failed\n");
                vga_fb_mark(0, b'F', b'X');
                return false;
            }
            vga_fb_mark(0, b'F', b'2');
            serial::write_str("[FB2] mapped\n");

            // Bootstrap mirror (legacy); drawing uses graphics::FB only.
            unsafe {
                FB_ADDR = addr;
                FB_W = width;
                FB_H = height;
                FB_PITCH = pitch;
                FB_BPP = bpp;
                FB_OK = true;
            }
            // SINGLE SOURCE OF TRUTH:
            crate::graphics::init(addr, width, height, pitch, bpp, false);
            serial::write_str("[FB] SoT -> graphics::FB\n");
            serial::write_str("[FB] Multiboot mode ");
            serial::write_usize(width);
            serial::write_str("x");
            serial::write_usize(height);
            serial::write_str("\n");
            // CLEAN_INIT_DONE
            return true;
        }
        // next tag 8-byte aligned
        off = (off + tag_size + 7) & !7;
    }
    serial::write_str("[FBX] no framebuffer tag - text mode\n");
    vga_fb_mark(0, b'F', b'X');
    false
}

// Drawing always goes through graphics::FB (single source of truth).
fn put_pixel(x: usize, y: usize, color: u32) {
    crate::graphics::put_pixel(x, y, color);
}
fn fill(color: u32) {
    crate::graphics::fill(color);
}
fn fill_rect(x0: usize, y0: usize, w: usize, h: usize, color: u32) {
    crate::graphics::fill_rect(x0, y0, w, h, color);
}

/// Test pattern: dark blue fill, green bar, red square, white border
pub fn draw_test_pattern() -> bool {
    if !is_ready() {
        return false;
    }
    serial::write_str("[FB] drawing test pattern...\n");
    // dark blue background
    fill(0x0010_2840);
    // green horizontal bar
    fill_rect(40, 40, width().saturating_sub(80), 20, 0x0020_C060);
    // red square
    fill_rect(80, 100, 120, 120, 0x00C0_3040);
    // cyan rectangle
    fill_rect(240, 100, 200, 80, 0x0020_A0C0);
    // white corner markers
    fill_rect(0, 0, 16, 16, 0x00FF_FFFF);
    fill_rect(width().saturating_sub(16), 0, 16, 16, 0x00FF_FFFF);
    fill_rect(0, height().saturating_sub(16), 16, 16, 0x00FF_FFFF);
    fill_rect(
        width().saturating_sub(16),
        height().saturating_sub(16),
        16,
        16,
        0x00FF_FFFF,
    );
    serial::write_str("[FB3] test pattern rendered\n");
    vga_fb_mark(0, b'F', b'3');
    true
}

/// Entry from kernel: try MBI, pattern, never hang
pub fn try_init(mbi: usize) {
    if init_from_mbi(mbi) {
        let _ = draw_test_pattern();
    } else {
        serial::write_str("[FB] safe fallback - keep text shell\n");
    }
}
