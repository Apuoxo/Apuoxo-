//! Thin video driver façade - always reflects `graphics::FB` (single source of truth).

use crate::graphics;
use crate::serial;

static mut READY: bool = false;

/// Called from `graphics::init` so WIDTH/HEIGHT never drift after modeset.
pub fn sync_from_graphics() {
    unsafe {
        READY = graphics::ready();
    }
}

pub fn init() {
    sync_from_graphics();
    if !ready() {
        serial::write_str("[VIDEO] FAIL - no framebuffer\n");
        return;
    }
    serial::write_str("[VIDEO] OK FB ");
    serial::write_usize(graphics::width());
    serial::write_str("x");
    serial::write_usize(graphics::height());
    serial::write_str(" pitch=");
    serial::write_usize(graphics::pitch());
    serial::write_str(" bpp=");
    serial::write_usize(graphics::bpp() as usize);
    serial::write_str("\n");
}

pub fn ready() -> bool {
    (unsafe { READY }) && graphics::ready()
}

pub fn width() -> u32 {
    graphics::width() as u32
}
pub fn height() -> u32 {
    graphics::height() as u32
}
pub fn pitch() -> u32 {
    graphics::pitch() as u32
}
pub fn bpp() -> u8 {
    graphics::bpp()
}
pub fn addr() -> usize {
    graphics::fb_addr()
}
