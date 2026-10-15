//! Font manager - wraps system bitmap font for unified text
use crate::graphics;

pub fn draw(x: usize, y: usize, s: &str, color: u32) {
    graphics::draw_str(x, y, s, color);
}

pub fn draw_shadowed(x: usize, y: usize, s: &str, color: u32, shadow: u32) {
    graphics::draw_str(x + 1, y + 1, s, shadow);
    graphics::draw_str(x, y, s, color);
}

pub fn char_w() -> usize { 8 }
pub fn char_h() -> usize { 8 }
