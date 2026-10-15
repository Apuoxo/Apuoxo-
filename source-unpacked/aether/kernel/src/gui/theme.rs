//! Unified XP-inspired visual theme for Aether GUI

pub const COL_DESKTOP: u32 = 0x003A6EA5; // classic bliss-ish blue-green base
pub const COL_TASKBAR: u32 = 0x00245EDC;
pub const COL_TASKBAR_TOP: u32 = 0x003C81F3;
pub const COL_START: u32 = 0x003C8A2E;
pub const COL_START_HI: u32 = 0x0050A838;
pub const COL_TITLE_ACT: u32 = 0x000A246A;
pub const COL_TITLE_ACT2: u32 = 0x00166ACB;
pub const COL_TITLE_INACT: u32 = 0x007A96DF;
pub const COL_CLIENT: u32 = 0x00ECE9D8;
pub const COL_BTN_FACE: u32 = 0x00ECE9D8;
pub const COL_BTN_HI: u32 = 0x00FFFFFF;
pub const COL_BTN_SH: u32 = 0x00404040;
pub const COL_TEXT: u32 = 0x00000000;
pub const COL_TEXT_INV: u32 = 0x00FFFFFF;
pub const COL_SEL: u32 = 0x00316AC5;
pub const COL_MENU: u32 = 0x00FFFFFF;
pub const COL_MENU_BORDER: u32 = 0x00666666;
pub const COL_CLOSE: u32 = 0x00E81123;
pub const TITLE_H: i32 = 22;
pub const TASKBAR_H: usize = 30;
pub const ICON_SIZE: usize = 32;
pub const ICON_GAP: usize = 12;
pub const ICON_LABEL_H: usize = 28;

pub struct Metrics {
    pub title_h: i32,
    pub taskbar_h: usize,
    pub icon_size: usize,
    pub border: i32,
}
pub const METRICS: Metrics = Metrics {
    title_h: 22,
    taskbar_h: 30,
    icon_size: 32,
    border: 3,
};
