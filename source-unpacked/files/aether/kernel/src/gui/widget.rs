//! Reusable GUI widgets (foundation)
use crate::graphics;
use crate::gui::theme;

pub fn draw_button(x: usize, y: usize, w: usize, h: usize, label: &str, pressed: bool) {
    let face = if pressed { 0x00D4D0C8 } else { theme::COL_BTN_FACE };
    graphics::fill_rect(x, y, w, h, face);
    if pressed {
        graphics::border_rect(x, y, w, h, theme::COL_BTN_SH);
        graphics::draw_str(x + 8 + 1, y + h / 2 - 4 + 1, label, theme::COL_TEXT);
    } else {
        // raised 3D
        graphics::fill_rect(x, y, w, 1, theme::COL_BTN_HI);
        graphics::fill_rect(x, y, 1, h, theme::COL_BTN_HI);
        graphics::fill_rect(x, y + h - 1, w, 1, theme::COL_BTN_SH);
        graphics::fill_rect(x + w - 1, y, 1, h, theme::COL_BTN_SH);
        graphics::draw_str(x + 8, y + h / 2 - 4, label, theme::COL_TEXT);
    }
}

pub fn draw_panel(x: usize, y: usize, w: usize, h: usize) {
    graphics::fill_rect(x, y, w, h, theme::COL_CLIENT);
    graphics::border_rect(x, y, w, h, 0x00808080);
}
