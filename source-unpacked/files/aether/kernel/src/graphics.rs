//! Framebuffer drawing primitives + 8x8 font
//!
//! Single source of truth: graphics::FB (LFB physical mapping).
//! Drawing targets an optional RAM backbuffer; present_* copies to LFB.
//! Cursor / get_pixel read from backbuffer when active (never from slow VRAM).

pub struct Framebuffer {
    pub addr: usize,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bpp: u8,
    pub software: bool,
}

static mut FB: Framebuffer = Framebuffer {
    addr: 0,
    width: 0,
    height: 0,
    pitch: 0,
    bpp: 0,
    software: false,
};

// RAM backbuffer (32bpp, pitch = width * 4, contiguous)
static mut BB_PTR: usize = 0;
static mut BB_BYTES: usize = 0;
static mut BB_W: usize = 0;
static mut BB_H: usize = 0;
static mut BB_PITCH: usize = 0; // bytes per row in BB (= BB_W * 4)
static mut BB_ACTIVE: bool = false;

/// Allocate / resize backbuffer for current FB size. Falls back to direct LFB if OOM.
fn ensure_backbuffer() {
    unsafe {
        if FB.addr == 0 || FB.width == 0 || FB.height == 0 || FB.bpp != 32 {
            BB_ACTIVE = false;
            return;
        }
        let w = FB.width;
        let h = FB.height;
        let pitch = w * 4;
        let need = pitch.saturating_mul(h);
        if need == 0 {
            BB_ACTIVE = false;
            return;
        }
        // Reuse if same size
        if BB_ACTIVE && BB_PTR != 0 && BB_W == w && BB_H == h && BB_BYTES >= need {
            return;
        }
        // Free old
        if BB_PTR != 0 && BB_BYTES != 0 {
            let pages = (BB_BYTES + 4095) / 4096;
            let mut i = 0usize;
            while i < pages {
                crate::mm::free_page(BB_PTR + i * 4096);
                i += 1;
            }
            BB_PTR = 0;
            BB_BYTES = 0;
            BB_ACTIVE = false;
        }
        let pages = (need + 4095) / 4096;
        match crate::mm::alloc_pages(pages) {
            Some(phys) => {
                crate::mm::zero_pages(phys, pages);
                BB_PTR = phys;
                BB_BYTES = pages * 4096;
                BB_W = w;
                BB_H = h;
                BB_PITCH = pitch;
                BB_ACTIVE = true;
                crate::serial::write_str("[FB] backbuffer ");
                crate::serial::write_usize(w);
                crate::serial::write_str("x");
                crate::serial::write_usize(h);
                crate::serial::write_str(" pages=");
                crate::serial::write_usize(pages);
                crate::serial::write_str("\n");
            }
            None => {
                BB_ACTIVE = false;
                crate::serial::write_str("[FB] backbuffer OOM - direct LFB\n");
            }
        }
    }
}

pub fn init(addr: usize, width: usize, height: usize, pitch: usize, bpp: u8, software: bool) {
    unsafe {
        FB.addr = addr;
        FB.width = width;
        FB.height = height;
        FB.pitch = pitch;
        FB.bpp = bpp;
        FB.software = software;
    }
    crate::drivers::video::sync_from_graphics();
    ensure_backbuffer();
}

pub fn snapshot() -> Framebuffer {
    unsafe {
        Framebuffer {
            addr: FB.addr,
            width: FB.width,
            height: FB.height,
            pitch: FB.pitch,
            bpp: FB.bpp,
            software: FB.software,
        }
    }
}

pub fn ready() -> bool {
    unsafe { FB.addr != 0 && (FB.bpp == 32 || FB.bpp == 24) }
}
pub fn width() -> usize { unsafe { FB.width } }
pub fn fb_addr() -> usize { unsafe { FB.addr } }
pub fn pitch() -> usize { unsafe { FB.pitch } }
pub fn bpp() -> u8 { unsafe { FB.bpp } }
pub fn software() -> bool { unsafe { FB.software } }
pub fn height() -> usize { unsafe { FB.height } }
pub fn backbuffer_active() -> bool { unsafe { BB_ACTIVE } }

#[inline]
fn draw_base() -> (usize, usize) {
    // (base, pitch) - BB if active else LFB
    unsafe {
        if BB_ACTIVE && BB_PTR != 0 {
            (BB_PTR, BB_PITCH)
        } else {
            (FB.addr, FB.pitch)
        }
    }
}

pub fn put_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        if FB.addr == 0 || x >= FB.width || y >= FB.height {
            return;
        }
        let (base, pitch) = draw_base();
        if FB.bpp == 32 {
            let ptr = (base + y * pitch + x * 4) as *mut u32;
            // BB is normal RAM - non-volatile write is fine; LFB still volatile
            if BB_ACTIVE {
                *ptr = color;
            } else {
                core::ptr::write_volatile(ptr, color);
            }
        } else if FB.bpp == 24 {
            let p = (base + y * pitch + x * 3) as *mut u8;
            *p = (color & 0xFF) as u8;
            *p.add(1) = ((color >> 8) & 0xFF) as u8;
            *p.add(2) = ((color >> 16) & 0xFF) as u8;
        }
    }
}

/// Read pixel from backbuffer (preferred) or LFB.
pub fn get_pixel(x: usize, y: usize) -> u32 {
    unsafe {
        if FB.addr == 0 || x >= FB.width || y >= FB.height || FB.bpp != 32 {
            return 0;
        }
        let (base, pitch) = draw_base();
        let ptr = (base + y * pitch + x * 4) as *const u32;
        if BB_ACTIVE {
            *ptr
        } else {
            core::ptr::read_volatile(ptr)
        }
    }
}

pub fn fill(color: u32) {
    unsafe {
        if FB.addr == 0 || FB.width == 0 || FB.height == 0 {
            return;
        }
        let (base, pitch) = draw_base();
        if FB.bpp == 32 {
            let mut y = 0usize;
            while y < FB.height {
                let row = (base + y * pitch) as *mut u32;
                let mut x = 0usize;
                while x < FB.width {
                    if BB_ACTIVE {
                        *row.add(x) = color;
                    } else {
                        core::ptr::write_volatile(row.add(x), color);
                    }
                    x += 1;
                }
                y += 1;
            }
        } else {
            let mut y = 0usize;
            while y < FB.height {
                let mut x = 0usize;
                while x < FB.width {
                    put_pixel(x, y, color);
                    x += 1;
                }
                y += 1;
            }
        }
    }
}

pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    unsafe {
        if FB.addr == 0 || w == 0 || h == 0 {
            return;
        }
        let x1 = if x >= FB.width { return } else { x };
        let y1 = if y >= FB.height { return } else { y };
        let x2 = core::cmp::min(x1.saturating_add(w), FB.width);
        let y2 = core::cmp::min(y1.saturating_add(h), FB.height);
        if x2 <= x1 || y2 <= y1 {
            return;
        }
        let (base, pitch) = draw_base();
        if FB.bpp == 32 {
            let mut row_y = y1;
            while row_y < y2 {
                let row = (base + row_y * pitch + x1 * 4) as *mut u32;
                let mut col = 0usize;
                let n = x2 - x1;
                while col < n {
                    if BB_ACTIVE {
                        *row.add(col) = color;
                    } else {
                        core::ptr::write_volatile(row.add(col), color);
                    }
                    col += 1;
                }
                row_y += 1;
            }
        } else {
            let mut yy = y1;
            while yy < y2 {
                let mut xx = x1;
                while xx < x2 {
                    put_pixel(xx, yy, color);
                    xx += 1;
                }
                yy += 1;
            }
        }
    }
}

pub fn border_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    if w == 0 || h == 0 {
        return;
    }
    fill_rect(x, y, w, 1, color);
    fill_rect(x, y + h.saturating_sub(1), w, 1, color);
    fill_rect(x, y, 1, h, color);
    fill_rect(x + w.saturating_sub(1), y, 1, h, color);
}

/// Copy one rectangle from backbuffer → LFB. No-op if BB inactive.
pub fn present_rect(x: usize, y: usize, w: usize, h: usize) {
    unsafe {
        if !BB_ACTIVE || BB_PTR == 0 || FB.addr == 0 || FB.bpp != 32 {
            return;
        }
        if w == 0 || h == 0 {
            return;
        }
        let x1 = if x >= FB.width { return } else { x };
        let y1 = if y >= FB.height { return } else { y };
        let x2 = core::cmp::min(x1.saturating_add(w), FB.width);
        let y2 = core::cmp::min(y1.saturating_add(h), FB.height);
        if x2 <= x1 || y2 <= y1 {
            return;
        }
        let row_bytes = (x2 - x1) * 4;
        let mut yy = y1;
        while yy < y2 {
            let src = (BB_PTR + yy * BB_PITCH + x1 * 4) as *const u8;
            let dst = (FB.addr + yy * FB.pitch + x1 * 4) as *mut u8;
            core::ptr::copy_nonoverlapping(src, dst, row_bytes);
            yy += 1;
        }
    }
}

/// Full-screen present (BB → LFB), optionally synced to VBlank.
/// Present full frame. `vsync=true` waits for VBlank (tear-free, ~1 frame latency).
pub fn present_full_ex(vsync: bool) {
    unsafe {
        if !BB_ACTIVE || BB_PTR == 0 || FB.addr == 0 {
            return;
        }
        if vsync && crate::drivers::intel_kms::ready() {
            let _ = crate::drivers::intel_kms::wait_vblank();
        }
        present_rect(0, 0, FB.width, FB.height);
    }
}

pub fn present_full() {
    present_full_ex(true);
}

/// Fast present without waiting for VBlank (UI/cursor path - lower latency).
pub fn present_full_fast() {
    present_full_ex(false);
}

pub fn blit_rect(sx: usize, sy: usize, dx: usize, dy: usize, w: usize, h: usize) {
    // Same-buffer blit within BB (or LFB)
    unsafe {
        if FB.addr == 0 || FB.bpp != 32 || w == 0 || h == 0 {
            return;
        }
        let (base, pitch) = draw_base();
        let mut row = 0usize;
        while row < h {
            let ysrc = sy + row;
            let ydst = dy + row;
            if ysrc >= FB.height || ydst >= FB.height {
                break;
            }
            let mut col = 0usize;
            while col < w {
                let xs = sx + col;
                let xd = dx + col;
                if xs < FB.width && xd < FB.width {
                    let s = *((base + ysrc * pitch + xs * 4) as *const u32);
                    let d = (base + ydst * pitch + xd * 4) as *mut u32;
                    if BB_ACTIVE {
                        *d = s;
                    } else {
                        core::ptr::write_volatile(d, s);
                    }
                }
                col += 1;
            }
            row += 1;
        }
    }
}

pub fn draw_line(mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: u32) {
    let dx = if x1 > x0 { x1 - x0 } else { x0 - x1 };
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let dy = if y1 > y0 { y0 - y1 } else { y1 - y0 };
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x0 >= 0 && y0 >= 0 {
            put_pixel(x0 as usize, y0 as usize, color);
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

// Minimal 8x8 font for printable ASCII 0x20-0x7E (partial but usable set)
// Generated compact: index = ch - 0x20
fn glyph(ch: u8) -> [u8; 8] {
    match ch {
        b' ' => [0, 0, 0, 0, 0, 0, 0, 0],
        b'!' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
        b'"' => [0x66, 0x66, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'#' => [0x66, 0xFF, 0x66, 0x66, 0xFF, 0x66, 0x66, 0x00],
        b'$' => [0x18, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x18, 0x00],
        b'%' => [0x62, 0x66, 0x0C, 0x18, 0x30, 0x66, 0x46, 0x00],
        b'&' => [0x3C, 0x66, 0x3C, 0x38, 0x67, 0x66, 0x3F, 0x00],
        b'\'' => [0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        b')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        b'*' => [0x00, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0x00, 0x00],
        b'+' => [0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
        b',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30],
        b'-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        b'.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        b'/' => [0x02, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x40, 0x00],
        b'0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        b'1' => [0x18, 0x18, 0x38, 0x18, 0x18, 0x18, 0x7E, 0x00],
        b'2' => [0x3C, 0x66, 0x06, 0x0C, 0x30, 0x60, 0x7E, 0x00],
        b'3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        b'4' => [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
        b'5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        b'6' => [0x3C, 0x60, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
        b'7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        b'8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        b'9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
        b':' => [0x00, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x00],
        b';' => [0x00, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x30],
        b'<' => [0x0C, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0C, 0x00],
        b'=' => [0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00],
        b'>' => [0x30, 0x18, 0x0C, 0x06, 0x0C, 0x18, 0x30, 0x00],
        b'?' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x00, 0x18, 0x00],
        b'@' => [0x3C, 0x66, 0x6E, 0x6A, 0x6E, 0x60, 0x3C, 0x00],
        b'A' | b'a' => [0x18, 0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x00],
        b'B' | b'b' => [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
        b'C' | b'c' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        b'D' | b'd' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        b'E' | b'e' => [0x7E, 0x60, 0x60, 0x78, 0x60, 0x60, 0x7E, 0x00],
        b'F' | b'f' => [0x7E, 0x60, 0x60, 0x78, 0x60, 0x60, 0x60, 0x00],
        b'G' | b'g' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3C, 0x00],
        b'H' | b'h' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        b'I' | b'i' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        b'J' | b'j' => [0x06, 0x06, 0x06, 0x06, 0x66, 0x66, 0x3C, 0x00],
        b'K' | b'k' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        b'L' | b'l' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        b'M' | b'm' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        b'N' | b'n' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        b'O' | b'o' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        b'P' | b'p' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        b'Q' | b'q' => [0x3C, 0x66, 0x66, 0x66, 0x6A, 0x6C, 0x36, 0x00],
        b'R' | b'r' => [0x7C, 0x66, 0x66, 0x7C, 0x78, 0x6C, 0x66, 0x00],
        b'S' | b's' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        b'T' | b't' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        b'U' | b'u' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        b'V' | b'v' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        b'W' | b'w' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        b'X' | b'x' => [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
        b'Y' | b'y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        b'Z' | b'z' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
        b'[' => [0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00],
        b'\\' => [0x40, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x02, 0x00],
        b']' => [0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00],
        b'^' => [0x18, 0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7E, 0x00],
        b'`' => [0x30, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'{' => [0x0E, 0x18, 0x18, 0x70, 0x18, 0x18, 0x0E, 0x00],
        b'|' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        b'}' => [0x70, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x70, 0x00],
        b'~' => [0x00, 0x00, 0x3A, 0x6C, 0x00, 0x00, 0x00, 0x00],
        _ => [0x7E, 0x42, 0x42, 0x42, 0x42, 0x42, 0x7E, 0x00],
    }
}

pub fn draw_char(x: usize, y: usize, c: u8, color: u32) {
    let g = glyph(c);
    let mut row = 0usize;
    while row < 8 {
        let bits = g[row];
        let mut col = 0usize;
        while col < 8 {
            if bits & (0x80 >> col) != 0 {
                put_pixel(x + col, y + row, color);
            }
            col += 1;
        }
        row += 1;
    }
}

pub fn draw_str(x: usize, y: usize, s: &str, color: u32) {
    let mut cx = x;
    for b in s.bytes() {
        if b == b'\n' {
            continue;
        }
        draw_char(cx, y, b, color);
        cx += 8;
    }
}

pub fn draw_bytes(x: usize, y: usize, s: &[u8], color: u32) {
    let mut cx = x;
    let mut i = 0usize;
    while i < s.len() {
        draw_char(cx, y, s[i], color);
        cx += 8;
        i += 1;
    }
}
