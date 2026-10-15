//! Aether Desktop v1.1 XP complete UI - Windows XP Luna visual style (homage)

use crate::graphics;
use crate::drivers::ps2;
use crate::fs;
use crate::serial;
use crate::input;

// XP Luna-inspired colors (original Aether theme, not Microsoft assets)
const COL_SKY_TOP: u32 = 0x003A6EA5;
const COL_SKY_MID: u32 = 0x005B9BD5;
const COL_SKY_LOW: u32 = 0x0087CEEB;
const COL_HILL: u32 = 0x003D8B37;
const COL_HILL2: u32 = 0x002E6B28;
const COL_BG: u32 = 0x005B9BD5; // fallback
// Win7 Aero-inspired (software FB approximation of glass)
const COL_TASKBAR: u32 = 0x001C2B3A;
const COL_TASKBAR_TOP: u32 = 0x003A5A78;
const COL_START: u32 = 0x00C48A1A;
const COL_START_HI: u32 = 0x00E8C040;
const COL_PANEL: u32 = 0x005A8FBF;
const COL_TITLE_ACT: u32 = 0x004A8AB8;
const COL_TITLE_INACT: u32 = 0x0098A8B8;
const COL_TITLE_TEXT: u32 = 0x00FFFFFF;
const COL_CLIENT: u32 = 0x00F0F0F0;
const COL_ACCENT: u32 = 0x00339ADC;
const COL_TITLE: u32 = 0x00FFFFFF;
const COL_TEXT: u32 = 0x00000000;
const COL_TEXT_DIM: u32 = 0x00404040;
const COL_TERM_BG: u32 = 0x00000000;
const COL_TERM_FG: u32 = 0x00C0C0C0;
const COL_WIN_BORDER: u32 = 0x003A6A90;
const COL_FOCUS: u32 = 0x004A8AB8;
const COL_INACTIVE: u32 = 0x0098A8B8;
const COL_CURSOR: u32 = 0x00000000;
const COL_CLOSE: u32 = 0x00E81123;
const COL_MAX: u32 = 0x002D7D46;
const COL_MIN: u32 = 0x002D5A27;
const COL_ABOUT_BG: u32 = 0x00F0F0F0;
const COL_DOCK: u32 = 0x001C2B3A;
const COL_BTN_FACE: u32 = 0x00D4D0C8;
const COL_MENU_BG: u32 = 0x00FFFFFF;
const COL_MENU_HDR: u32 = 0x001665CA;

const MAX_WIN: usize = 14;
const TITLE_H: i32 = 26;
const TASKBAR_H: usize = 30;

#[derive(Clone, Copy, PartialEq)]
enum WinKind {
    Terminal,
    About,
    Network,
    Sound,
    Video,
    Files,
    DateTime,
    MyComputer,
    SysProps,
    Settings,
    Keyboard,
    Alarm,
    Movie,
    Browser,
}

struct Window {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    kind: WinKind,
    visible: bool,
    z: i32,
    minimized: bool,
    maximized: bool,
    rx: i32, // restore geometry
    ry: i32,
    rw: i32,
    rh: i32,
}

static mut WINS: [Window; MAX_WIN] = [
    Window { x: 20, y: 36, w: 760, h: 340, kind: WinKind::Terminal, visible: true, z: 5,
        minimized: false, maximized: false, rx: 20, ry: 36, rw: 760, rh: 340 },
    Window { x: 20, y: 36, w: 220, h: 110, kind: WinKind::About, visible: false, z: 1,
        minimized: false, maximized: false, rx: 20, ry: 36, rw: 220, rh: 110 },
    Window { x: 60, y: 40, w: 720, h: 480, kind: WinKind::Network, visible: false, z: 2,
        minimized: false, maximized: false, rx: 60, ry: 40, rw: 720, rh: 480 },
    Window { x: 80, y: 50, w: 480, h: 420, kind: WinKind::Sound, visible: false, z: 3,
        minimized: false, maximized: false, rx: 80, ry: 60, rw: 420, rh: 320 },
    Window { x: 80, y: 40, w: 520, h: 400, kind: WinKind::Video, visible: false, z: 4,
        minimized: false, maximized: false, rx: 160, ry: 190, rw: 300, rh: 160 },
    Window { x: 60, y: 36, w: 780, h: 520, kind: WinKind::Files, visible: false, z: 6,
        minimized: false, maximized: false, rx: 60, ry: 36, rw: 780, rh: 520 },
    Window { x: 280, y: 200, w: 240, h: 140, kind: WinKind::DateTime, visible: false, z: 7,
        minimized: false, maximized: false, rx: 280, ry: 200, rw: 240, rh: 140 },
    Window { x: 60, y: 36, w: 780, h: 520, kind: WinKind::MyComputer, visible: false, z: 8,
        minimized: false, maximized: false, rx: 60, ry: 36, rw: 780, rh: 520 },
    Window { x: 140, y: 50, w: 420, h: 360, kind: WinKind::SysProps, visible: false, z: 9,
        minimized: false, maximized: false, rx: 140, ry: 50, rw: 420, rh: 360 },
    Window { x: 120, y: 60, w: 500, h: 360, kind: WinKind::Settings, visible: false, z: 10,
        minimized: false, maximized: false, rx: 120, ry: 60, rw: 500, rh: 360 },
    Window { x: 100, y: 380, w: 700, h: 240, kind: WinKind::Keyboard, visible: false, z: 11,
        minimized: false, maximized: false, rx: 100, ry: 380, rw: 700, rh: 240 },
    Window { x: 220, y: 80, w: 360, h: 320, kind: WinKind::Alarm, visible: false, z: 12,
        minimized: false, maximized: false, rx: 220, ry: 80, rw: 360, rh: 320 },
    Window { x: 100, y: 48, w: 560, h: 420, kind: WinKind::Movie, visible: false, z: 13,
        minimized: false, maximized: false, rx: 100, ry: 48, rw: 560, rh: 420 },
    Window { x: 80, y: 40, w: 640, h: 480, kind: WinKind::Browser, visible: false, z: 14,
        minimized: false, maximized: false, rx: 80, ry: 40, rw: 640, rh: 480 },
];

// Simple text browser state
static mut BR_URL: [u8; 96] = [0; 96];
static mut BR_URL_LEN: usize = 0;
static mut BR_PAGE: [u8; 8192] = [0; 8192];
static mut BR_PAGE_LEN: usize = 0;
static mut BR_STATUS: [u8; 48] = [0; 48];
static mut BR_STATUS_LEN: usize = 0;
static mut RECYCLE_MODE: bool = false;
static mut BR_FOCUS_URL: bool = true;
static mut BR_INIT: bool = false;
/// Vertical scroll in text lines (0 = top).
static mut BR_SCROLL: usize = 0;
static mut BR_TOTAL_LINES: usize = 0;
static mut BR_TITLE: [u8; 48] = [0; 48];
static mut BR_TITLE_LEN: usize = 0;
/// Simple link list extracted from page (href only, up to 8).
static mut BR_LINK_HREF: [[u8; 96]; 8] = [[0; 96]; 8];
static mut BR_LINK_LEN: [usize; 8] = [0; 8];
static mut BR_LINK_N: usize = 0;
/// One-level back stack
static mut BR_BACK: [u8; 96] = [0; 96];
static mut BR_BACK_LEN: usize = 0;

// On-screen keyboard shift state
static mut OSK_SHIFT: bool = false;
static mut OSK_CAPS: bool = false;
/// Last window that accepts text input (Terminal / Browser).
/// OSK injects keys here even while the Keyboard window holds visual focus.
static mut TEXT_FOCUS: usize = 0;

static mut FOCUS: usize = 0; // terminal
static mut MX: i32 = 400;
static mut MY: i32 = 300;
static mut MB: u8 = 0;
static mut PREV_MB: u8 = 0;
static mut DRAGGING: bool = false;
static mut DRAG_WIN: usize = 0;
static mut DRAG_OX: i32 = 0;
static mut DRAG_OY: i32 = 0;
static mut DRAG_OLD_X: i32 = 0;
static mut DRAG_OLD_Y: i32 = 0;
static mut DRAG_OLD_W: i32 = 0;
static mut DRAG_OLD_H: i32 = 0;
static mut DIRTY_FULL: bool = true;
/// Windows/taskbar/menus only - no wallpaper wipe (stops click flicker).
static mut DIRTY_UI: bool = false;
static mut SELECTED_ICON: usize = usize::MAX;
static mut DIRTY_CURSOR: bool = false;
static mut CURSOR_SAVED: bool = false;
static mut CURSOR_OX: i32 = -1;
static mut CURSOR_OY: i32 = -1;
static mut CURSOR_BG: [u32; 256] = [0; 256]; // 16x16
static mut LAST_SEC: u8 = 255;
static mut RESIZING: bool = false;
static mut RESIZE_WIN: usize = 0;
static mut RESIZE_EDGES: u8 = 0; // bit0=right, bit1=bottom
static mut TITLE_CLICK_FRAME: u32 = 0;
static mut TITLE_CLICK_WIN: usize = usize::MAX;
static mut CLICK_FRAME: u32 = 0;
static mut LAST_CLICK_FRAME: u32 = 0;
static mut LAST_CLICK_WIN: usize = 255;
static mut FRAME_N: u32 = 0;
/// Consecutive left-clicks on the same Explorer row (no time window).
static mut FILES_CLICK_ROW: i32 = -1;
static mut FILES_CLICK_N: u8 = 0;
static mut LOOP_TICK: u32 = 0;
static mut LAST_DESKTOP_ICON_FRAME: u32 = 0;
static mut LAST_DESKTOP_ICON: usize = usize::MAX;
static mut START_MENU: bool = false;
static mut CTX_MENU: bool = false;
static mut CTX_X: i32 = 0;
static mut CTX_Y: i32 = 0;
static mut Z_TOP: i32 = 3;
static mut DISP_SEL: usize = 0;
static mut DISP_TAB: u8 = 0; // 0=Settings 1=Adapter
static mut SETTINGS_VIEW: u8 = 0;
static mut CURSOR_COLOR: u32 = 0x00000000; // black
static mut CURSOR_PENDING: u8 = 1; // index of Black in Settings palette

// Windows 7-style Wi-Fi UI state. The list is deliberately empty until the
// native Intel 2230 scan backend returns real 802.11 results.
static mut WIFI_UI_SCAN_REQUESTED: bool = false;
static mut WIFI_UI_ENABLE_REQ: bool = false;
static mut WIFI_UI_SELECTED: i32 = -1;
static mut WIFI_UI_CONNECT_DIALOG: bool = false;
static mut WIFI_UI_PASSWORD_LEN: usize = 0;
static mut WIFI_UI_PASSWORD: [u8; 64] = [0; 64];
static mut WIFI_UI_STATUS: u8 = 0; // 0=idle, 1=scanning, 2=connected, 3=error

// Media Player menu: 0=closed, 1=File open, 2=Playback open
static mut MEDIA_MENU: u8 = 0;
/// When true, Explorer double-click on audio focuses Media Player (pick mode).
static mut MEDIA_PICK: bool = false;

// Terminal buffer

// ===================== Terminal (professional scrollback + line editor) =====================
const TERM_ROWS: usize = 256;
const TERM_COLS: usize = 120;
const TERM_VIEW_ROWS_MAX: usize = 64;
const TERM_HIST_MAX: usize = 24;
const TERM_INPUT_MAX: usize = 120;

static mut TERM_LINES: [[u8; TERM_COLS]; TERM_ROWS] = [[0; TERM_COLS]; TERM_ROWS];
static mut TERM_LEN: [usize; TERM_ROWS] = [0; TERM_ROWS];
/// Absolute index of the newest line (0..TERM_ROWS-1), grows with wrap.
static mut TERM_HEAD: usize = 0;
/// Number of valid lines in the ring (capped at TERM_ROWS).
static mut TERM_COUNT: usize = 1;
/// Scroll offset from live bottom: 0 = follow output, larger = older history.
static mut TERM_VIEW: usize = 0;
static mut TERM_INIT: bool = false;

static mut INPUT: [u8; TERM_INPUT_MAX] = [0; TERM_INPUT_MAX];
static mut INPUT_LEN: usize = 0;
static mut INPUT_CURSOR: usize = 0;

static mut TERM_HIST: [[u8; TERM_INPUT_MAX]; TERM_HIST_MAX] = [[0; TERM_INPUT_MAX]; TERM_HIST_MAX];
static mut TERM_HIST_LEN: [usize; TERM_HIST_MAX] = [0; TERM_HIST_MAX];
static mut TERM_HIST_N: usize = 0;
/// 0 = editing live line; 1..N = stepping through history
static mut TERM_HIST_POS: usize = 0;

fn term_clear() {
    unsafe {
        let mut r = 0usize;
        while r < TERM_ROWS {
            TERM_LEN[r] = 0;
            let mut c = 0usize;
            while c < TERM_COLS {
                TERM_LINES[r][c] = 0;
                c += 1;
            }
            r += 1;
        }
        TERM_HEAD = 0;
        TERM_COUNT = 1;
        TERM_VIEW = 0;
        TERM_INIT = true;
    }
}

fn term_text_cols_for(ww: i32) -> usize {
    let usable = if ww > 34 { (ww - 34) as usize } else { 1 };
    let cols = usable / 8;
    if cols < 8 {
        8
    } else if cols > TERM_COLS {
        TERM_COLS
    } else {
        cols
    }
}

fn term_text_cols() -> usize {
    unsafe {
        if FOCUS < MAX_WIN && WINS[FOCUS].kind == WinKind::Terminal && WINS[FOCUS].visible {
            term_text_cols_for(WINS[FOCUS].w)
        } else {
            TERM_COLS
        }
    }
}

fn term_visible_rows_for(h: i32) -> usize {
    let hh = h as usize;
    if hh > TITLE_H as usize + 36 {
        let usable = hh.saturating_sub(TITLE_H as usize + 28);
        let rows = usable / 12;
        if rows < 1 {
            1
        } else if rows > TERM_VIEW_ROWS_MAX {
            TERM_VIEW_ROWS_MAX
        } else {
            rows
        }
    } else {
        2
    }
}

fn term_visible_rows() -> usize {
    unsafe {
        if FOCUS < MAX_WIN && WINS[FOCUS].kind == WinKind::Terminal && WINS[FOCUS].visible {
            term_visible_rows_for(WINS[FOCUS].h)
        } else {
            12
        }
    }
}

/// Map logical line index (0 = oldest, TERM_COUNT-1 = newest) to ring slot.
fn term_slot(logical: usize) -> usize {
    unsafe {
        if TERM_COUNT == 0 {
            return 0;
        }
        // head points at newest; oldest is head - (count-1)
        let oldest = if TERM_HEAD + 1 >= TERM_COUNT {
            TERM_HEAD + 1 - TERM_COUNT
        } else {
            TERM_ROWS - (TERM_COUNT - 1 - TERM_HEAD)
        };
        (oldest + logical) % TERM_ROWS
    }
}

fn term_scroll_up() {
    unsafe {
        let rows = term_visible_rows();
        let max_view = if TERM_COUNT > rows { TERM_COUNT - rows } else { 0 };
        if TERM_VIEW < max_view {
            TERM_VIEW += 1;
            DIRTY_UI = true;
        }
    }
}

fn term_scroll_down() {
    unsafe {
        if TERM_VIEW > 0 {
            TERM_VIEW -= 1;
            DIRTY_UI = true;
        }
    }
}

fn term_scroll_home() {
    unsafe {
        let rows = term_visible_rows();
        TERM_VIEW = if TERM_COUNT > rows { TERM_COUNT - rows } else { 0 };
        DIRTY_UI = true;
    }
}

fn term_scroll_end() {
    unsafe {
        TERM_VIEW = 0;
        DIRTY_UI = true;
    }
}

fn term_newline() {
    unsafe {
        // advance head
        if TERM_COUNT < TERM_ROWS {
            TERM_COUNT += 1;
        }
        TERM_HEAD = (TERM_HEAD + 1) % TERM_ROWS;
        TERM_LEN[TERM_HEAD] = 0;
        let mut c = 0usize;
        while c < TERM_COLS {
            TERM_LINES[TERM_HEAD][c] = 0;
            c += 1;
        }
        // stick to bottom while following
        if TERM_VIEW == 0 {
            // stay live
        } else if TERM_VIEW < TERM_COUNT {
            // keep relative view if user scrolled
        }
    }
}

fn term_putc(ch: u8) {
    unsafe {
        if !TERM_INIT {
            term_clear();
        }
        if ch == b'\n' {
            term_newline();
            // new output pulls view to live bottom
            TERM_VIEW = 0;
            return;
        }
        if ch == b'\r' {
            return;
        }
        if ch == 0x08 {
            // backspace on current output line (rare)
            if TERM_LEN[TERM_HEAD] > 0 {
                TERM_LEN[TERM_HEAD] -= 1;
                TERM_LINES[TERM_HEAD][TERM_LEN[TERM_HEAD]] = 0;
            }
            return;
        }
        let cols = term_text_cols();
        if TERM_LEN[TERM_HEAD] >= cols {
            term_newline();
        }
        if TERM_LEN[TERM_HEAD] < TERM_COLS {
            let c = TERM_LEN[TERM_HEAD];
            TERM_LINES[TERM_HEAD][c] = if ch >= 32 && ch < 127 { ch } else { b'?' };
            TERM_LEN[TERM_HEAD] = c + 1;
        }
        TERM_VIEW = 0;
    }
}

fn term_write_hex(mut v: usize) {
    unsafe {
        if v == 0 {
            term_putc(b'0');
            return;
        }
        let mut digits = [0u8; 16];
        let mut n = 0usize;
        while v > 0 && n < 16 {
            let d = v & 0xF;
            digits[n] = if d < 10 { b'0' + d as u8 } else { b'a' + (d - 10) as u8 };
            v >>= 4;
            n += 1;
        }
        while n > 0 {
            n -= 1;
            term_putc(digits[n]);
        }
    }
}

fn term_hist_push() {
    unsafe {
        if INPUT_LEN == 0 {
            return;
        }
        // skip duplicate of last entry
        if TERM_HIST_N > 0 {
            let last = (TERM_HIST_N - 1) % TERM_HIST_MAX;
            if TERM_HIST_LEN[last] == INPUT_LEN {
                let mut same = true;
                let mut i = 0usize;
                while i < INPUT_LEN {
                    if TERM_HIST[last][i] != INPUT[i] {
                        same = false;
                        break;
                    }
                    i += 1;
                }
                if same {
                    TERM_HIST_POS = 0;
                    return;
                }
            }
        }
        let slot = TERM_HIST_N % TERM_HIST_MAX;
        let mut i = 0usize;
        while i < INPUT_LEN && i < TERM_INPUT_MAX {
            TERM_HIST[slot][i] = INPUT[i];
            i += 1;
        }
        TERM_HIST_LEN[slot] = INPUT_LEN;
        TERM_HIST_N += 1;
        TERM_HIST_POS = 0;
    }
}

fn term_hist_load(pos: usize) {
    unsafe {
        if pos == 0 {
            INPUT_LEN = 0;
            INPUT_CURSOR = 0;
            return;
        }
        if TERM_HIST_N == 0 {
            return;
        }
        let available = if TERM_HIST_N < TERM_HIST_MAX { TERM_HIST_N } else { TERM_HIST_MAX };
        if pos > available {
            return;
        }
        // pos=1 is most recent
        let idx = (TERM_HIST_N - pos) % TERM_HIST_MAX;
        let n = TERM_HIST_LEN[idx];
        let mut i = 0usize;
        while i < n {
            INPUT[i] = TERM_HIST[idx][i];
            i += 1;
        }
        INPUT_LEN = n;
        INPUT_CURSOR = n;
    }
}

fn term_hist_up() {
    unsafe {
        let available = if TERM_HIST_N < TERM_HIST_MAX { TERM_HIST_N } else { TERM_HIST_MAX };
        if available == 0 {
            return;
        }
        if TERM_HIST_POS < available {
            TERM_HIST_POS += 1;
            term_hist_load(TERM_HIST_POS);
            DIRTY_UI = true;
        }
    }
}

fn term_hist_down() {
    unsafe {
        if TERM_HIST_POS > 0 {
            TERM_HIST_POS -= 1;
            term_hist_load(TERM_HIST_POS);
            DIRTY_UI = true;
        }
    }
}

pub fn terminal_write(s: &str) {
    for b in s.bytes() {
        term_putc(b);
    }
}

pub fn terminal_write_char(ch: u8) {
    term_putc(ch);
}




fn eq_cmd(cmd: &[u8], clen: usize, expect: &[u8]) -> bool {
    if clen != expect.len() {
        return false;
    }
    let mut i = 0usize;
    while i < clen {
        if cmd[i] != expect[i] {
            return false;
        }
        i += 1;
    }
    true
}



/// Raise window and set focus. Does NOT mark dirty if the window is already
/// focused and already on top - that was the global "click → whole window
/// flashes" bug (DIRTY_UI → render_ui_only redraws every window).
fn bring_to_front(idx: usize) {
    unsafe {
        if idx >= MAX_WIN || !WINS[idx].visible {
            return;
        }
        let mut already_top = true;
        let mut i = 0usize;
        while i < MAX_WIN {
            if i != idx && WINS[i].visible && WINS[i].z >= WINS[idx].z {
                already_top = false;
                break;
            }
            i += 1;
        }
        if FOCUS == idx && already_top {
            return;
        }
        let old_focus = FOCUS;
        Z_TOP = Z_TOP.wrapping_add(1);
        WINS[idx].z = Z_TOP;
        FOCUS = idx;
        // Remember last text-capable window so OSK can type into it.
        match WINS[idx].kind {
            WinKind::Terminal | WinKind::Browser => {
                TEXT_FOCUS = idx;
            }
            _ => {}
        }
        if old_focus != idx {
            // Only the two window frames change (active/inactive title).
            // Immediate paint avoids a full multi-window flash.
            paint_focus_change(old_focus, idx);
        } else {
            // Same focus, but z-order changed (window was buried).
            DIRTY_UI = true;
        }
    }
}

fn hit_test(mx: i32, my: i32) -> Option<usize> {
    unsafe {
        let mut best: Option<usize> = None;
        let mut best_z = (-2147483647-1);
        let mut i = 0usize;
        while i < MAX_WIN {
            if WINS[i].visible {
                let w = &WINS[i];
                if mx >= w.x && mx < w.x + w.w && my >= w.y && my < w.y + w.h {
                    if w.z >= best_z {
                        best_z = w.z;
                        best = Some(i);
                    }
                }
            }
            i += 1;
        }
        best
    }
}

fn in_title(idx: usize, mx: i32, my: i32) -> bool {
    unsafe {
        let w = &WINS[idx];
        mx >= w.x && mx < w.x + w.w && my >= w.y && my < w.y + TITLE_H
    }
}


fn in_max(idx: usize, mx: i32, my: i32) -> bool {
    unsafe {
        let w = &WINS[idx];
        let cx = w.x + w.w - 50;
        let cy = w.y + 4;
        mx >= cx && mx < cx + 20 && my >= cy && my < cy + 16
    }
}
fn in_min(idx: usize, mx: i32, my: i32) -> bool {
    unsafe {
        let w = &WINS[idx];
        let cx = w.x + w.w - 72;
        let cy = w.y + 4;
        mx >= cx && mx < cx + 20 && my >= cy && my < cy + 16
    }
}

fn toggle_maximize(idx: usize) {
    unsafe {
        if idx >= MAX_WIN {
            return;
        }
        DIRTY_FULL = true;
        let sw = graphics::width() as i32;
        let sh = graphics::height() as i32;
        let tb = TASKBAR_H as i32;
        if sw < 320 || sh < 240 {
            return;
        }
        if WINS[idx].maximized {
            // Restore previous geometry with hard bounds
            let mut x = WINS[idx].rx;
            let mut y = WINS[idx].ry;
            let mut w = WINS[idx].rw;
            let mut h = WINS[idx].rh;
            if w < 320 {
                w = 320;
            }
            if h < 200 {
                h = 200;
            }
            if w > sw {
                w = sw;
            }
            if h > sh - tb {
                h = (sh - tb).max(200);
            }
            if x < 0 {
                x = 0;
            }
            if y < 0 {
                y = 0;
            }
            if x + w > sw {
                x = (sw - w).max(0);
            }
            if y + h > sh - tb {
                y = (sh - tb - h).max(0);
            }
            WINS[idx].x = x;
            WINS[idx].y = y;
            WINS[idx].w = w;
            WINS[idx].h = h;
            WINS[idx].maximized = false;
            WINS[idx].visible = true;
            WINS[idx].minimized = false;
        } else {
            // Save current normal geometry before filling the work area
            let mut rw = WINS[idx].w;
            let mut rh = WINS[idx].h;
            if rw < 320 {
                rw = 320;
            }
            if rh < 200 {
                rh = 200;
            }
            WINS[idx].rx = WINS[idx].x;
            WINS[idx].ry = WINS[idx].y;
            WINS[idx].rw = rw;
            WINS[idx].rh = rh;
            WINS[idx].x = 0;
            WINS[idx].y = 0;
            WINS[idx].w = sw;
            WINS[idx].h = (sh - tb).max(200);
            WINS[idx].maximized = true;
            WINS[idx].minimized = false;
            WINS[idx].visible = true;
        }
        DIRTY_FULL = true;
    }
}

fn toggle_minimize(idx: usize) {
    unsafe {
        if WINS[idx].minimized {
            WINS[idx].minimized = false;
            WINS[idx].visible = true;
            // raise restored window
            Z_TOP = Z_TOP.wrapping_add(1);
            WINS[idx].z = Z_TOP;
            FOCUS = idx;
            match WINS[idx].kind {
                WinKind::Terminal | WinKind::Browser => TEXT_FOCUS = idx,
                _ => {}
            }
        } else {
            WINS[idx].minimized = true;
            WINS[idx].visible = false;
            focus_topmost();
        }
        DIRTY_FULL = true;
    }
}

/// Resize hit zones: SE corner (both), right edge (width), bottom edge (height).
fn in_resize(idx: usize, mx: i32, my: i32) -> bool {
    unsafe {
        let w = &WINS[idx];
        if w.maximized || !w.visible {
            return false;
        }
        let on_right = mx >= w.x + w.w - 6 && mx < w.x + w.w && my >= w.y + TITLE_H && my < w.y + w.h;
        let on_bottom = my >= w.y + w.h - 6 && my < w.y + w.h && mx >= w.x && mx < w.x + w.w;
        let on_corner = mx >= w.x + w.w - 16 && mx < w.x + w.w && my >= w.y + w.h - 16 && my < w.y + w.h;
        on_corner || on_right || on_bottom
    }
}

/// Which resize edges are active: bit0=right, bit1=bottom.
fn resize_edges(idx: usize, mx: i32, my: i32) -> u8 {
    unsafe {
        let w = &WINS[idx];
        if w.maximized || !w.visible {
            return 0;
        }
        let mut e = 0u8;
        if mx >= w.x + w.w - 6 && mx < w.x + w.w && my >= w.y + TITLE_H {
            e |= 1;
        }
        if my >= w.y + w.h - 6 && my < w.y + w.h && mx >= w.x {
            e |= 2;
        }
        if mx >= w.x + w.w - 16 && my >= w.y + w.h - 16 {
            e |= 3;
        }
        e
    }
}

/// Focus the visible window with highest z-order (true topmost).
fn focus_topmost() {
    unsafe {
        let mut best: Option<usize> = None;
        let mut best_z = i32::MIN;
        let mut i = 0usize;
        while i < MAX_WIN {
            if WINS[i].visible && !WINS[i].minimized && WINS[i].z >= best_z {
                best_z = WINS[i].z;
                best = Some(i);
            }
            i += 1;
        }
        if let Some(idx) = best {
            FOCUS = idx;
            match WINS[idx].kind {
                WinKind::Terminal | WinKind::Browser => TEXT_FOCUS = idx,
                _ => {}
            }
        }
    }
}

fn win_title(kind: WinKind) -> &'static str {
    match kind {
        WinKind::Terminal => "Terminal",
        WinKind::About => "About",
        WinKind::Network => "Network",
        WinKind::Sound => "Media Player",
        WinKind::Video => "Display Properties",
        WinKind::Files => "Windows Explorer",
        WinKind::DateTime => "Date/Time",
        WinKind::MyComputer => "Windows Explorer",
        WinKind::SysProps => "System Properties",
        WinKind::Settings => "Settings",
        WinKind::Keyboard => "On-Screen Keyboard",
        WinKind::Alarm => "Alarm Clock",
        WinKind::Movie => "Video Player",
        WinKind::Browser => "Browser",
    }
}

pub fn open_browser() {
    open_win(13);
}
fn open_win(slot: usize) {
    unsafe {
        if slot == 99 {
            // Recycle Bin -> About window in recycle mode
            RECYCLE_MODE = true;
            let rslot = 1usize; // About
            WINS[rslot].visible = true;
            WINS[rslot].minimized = false;
            WINS[rslot].w = 360;
            WINS[rslot].h = 220;
            bring_to_front(rslot);
            return;
        }
        if slot >= MAX_WIN {
            return;
        }
        RECYCLE_MODE = false;
        if WINS[slot].kind == WinKind::Settings {
            SETTINGS_VIEW = 0;
            CURSOR_PENDING = 0;
        }
        let was = WINS[slot].visible && !WINS[slot].minimized;
        WINS[slot].minimized = false;
        WINS[slot].visible = true;
        if slot == 5 && !was {
            crate::files_mgr::reset();
        }
        bring_to_front(slot);
        DIRTY_FULL = true;
    }
}

/// Public entry for Files / shell to raise the Media Player window (slot 3).


fn draw_osk(wx: usize, wy: usize, ww: usize, wh: usize) {
    graphics::fill_rect(wx + 3, wy + TITLE_H as usize, ww - 6, wh - TITLE_H as usize - 3, 0x00D4D0C8);
    let base_y = wy + TITLE_H as usize + 8;
    let base_x = wx + 8;
    let kw = 42usize;
    let kh = 36usize;
    let gap = 4usize;
    let shift = unsafe { OSK_SHIFT || OSK_CAPS };
    let r0: &[u8] = if shift { b"~!@#$%^&*()_+" } else { b"`1234567890-=" };
    let r1: &[u8] = if shift { b"QWERTYUIOP{}|" } else { b"qwertyuiop[]\\" };
    let r2: &[u8] = if shift { b"ASDFGHJKL;\x22" } else { b"asdfghjkl;\x27" };
    let r3: &[u8] = if shift { b"ZXCVBNM<>?" } else { b"zxcvbnm,./" };
    let all = [r0, r1, r2, r3];
    let xoffs = [0usize, 20, 30, 40];
    let mut r = 0usize;
    while r < 4 {
        let row = all[r];
        let y = base_y + r * (kh + gap);
        let mut c = 0usize;
        while c < row.len() {
            let x = base_x + xoffs[r] + c * (kw + gap);
            graphics::fill_rect(x, y, kw, kh, 0x00ECE9D8);
            graphics::border_rect(x, y, kw, kh, 0x00808080);
            graphics::draw_char(x + 14, y + 10, row[c], COL_TEXT);
            c += 1;
        }
        if r == 0 {
            let x = base_x + xoffs[r] + row.len() * (kw + gap);
            graphics::fill_rect(x, y, kw + 20, kh, 0x00ECE9D8);
            graphics::border_rect(x, y, kw + 20, kh, 0x00808080);
            graphics::draw_str(x + 6, y + 10, "Bksp", COL_TEXT);
        }
        if r == 2 {
            let x = base_x + xoffs[r] + row.len() * (kw + gap);
            graphics::fill_rect(x, y, kw + 28, kh, 0x00ECE9D8);
            graphics::border_rect(x, y, kw + 28, kh, 0x00808080);
            graphics::draw_str(x + 8, y + 10, "Enter", COL_TEXT);
        }
        r += 1;
    }
    let y = base_y + 4 * (kh + gap);
    let shift_bg = if unsafe { OSK_SHIFT } { 0x00316AC5 } else { 0x00ECE9D8 };
    let caps_bg = if unsafe { OSK_CAPS } { 0x00316AC5 } else { 0x00ECE9D8 };
    let st = if unsafe { OSK_SHIFT } { 0x00FFFFFF } else { COL_TEXT };
    let ct = if unsafe { OSK_CAPS } { 0x00FFFFFF } else { COL_TEXT };
    graphics::fill_rect(base_x, y, 70, kh, shift_bg);
    graphics::border_rect(base_x, y, 70, kh, 0x00808080);
    graphics::draw_str(base_x + 12, y + 10, "Shift", st);
    graphics::fill_rect(base_x + 74, y, 56, kh, caps_bg);
    graphics::border_rect(base_x + 74, y, 56, kh, 0x00808080);
    graphics::draw_str(base_x + 82, y + 10, "Caps", ct);
    graphics::fill_rect(base_x + 140, y, 280, kh, 0x00ECE9D8);
    graphics::border_rect(base_x + 140, y, 280, kh, 0x00808080);
    graphics::draw_str(base_x + 250, y + 10, "Space", COL_TEXT);
    graphics::fill_rect(base_x + 428, y, 70, kh, shift_bg);
    graphics::border_rect(base_x + 428, y, 70, kh, 0x00808080);
    graphics::draw_str(base_x + 440, y + 10, "Shift", st);
    graphics::draw_str(base_x, y + kh + 6, "Click keys - text goes to focused window", COL_TEXT_DIM);
    let _ = ww;
}

fn handle_osk_click(mx: i32, my: i32, wx: i32, wy: i32) -> bool {
    let base_y = wy + TITLE_H + 8;
    let base_x = wx + 8;
    let kw = 42i32;
    let kh = 36i32;
    let gap = 4i32;
    let shift = unsafe { OSK_SHIFT || OSK_CAPS };
    let r0: &[u8] = if shift { b"~!@#$%^&*()_+" } else { b"`1234567890-=" };
    let r1: &[u8] = if shift { b"QWERTYUIOP{}|" } else { b"qwertyuiop[]\\" };
    let r2: &[u8] = if shift { b"ASDFGHJKL;\x22" } else { b"asdfghjkl;\x27" };
    let r3: &[u8] = if shift { b"ZXCVBNM<>?" } else { b"zxcvbnm,./" };
    let all = [r0, r1, r2, r3];
    let xoffs = [0i32, 20, 30, 40];
    let mut r = 0usize;
    while r < 4 {
        let row = all[r];
        let y = base_y + r as i32 * (kh + gap);
        let mut c = 0usize;
        while c < row.len() {
            let x = base_x + xoffs[r] + c as i32 * (kw + gap);
            if mx >= x && mx < x + kw && my >= y && my < y + kh {
                handle_key(row[c]);
                unsafe {
                    if OSK_SHIFT && !OSK_CAPS {
                        OSK_SHIFT = false;
                    }
                }
                return true;
            }
            c += 1;
        }
        if r == 0 {
            let x = base_x + xoffs[r] + row.len() as i32 * (kw + gap);
            if mx >= x && mx < x + kw + 20 && my >= y && my < y + kh {
                handle_key(0x08);
                return true;
            }
        }
        if r == 2 {
            let x = base_x + xoffs[r] + row.len() as i32 * (kw + gap);
            if mx >= x && mx < x + kw + 28 && my >= y && my < y + kh {
                handle_key(b'\n');
                return true;
            }
        }
        r += 1;
    }
    let y = base_y + 4 * (kh + gap);
    if mx >= base_x && mx < base_x + 70 && my >= y && my < y + kh {
        unsafe { OSK_SHIFT = !OSK_SHIFT; }
        return true;
    }
    if mx >= base_x + 74 && mx < base_x + 130 && my >= y && my < y + kh {
        unsafe { OSK_CAPS = !OSK_CAPS; }
        return true;
    }
    if mx >= base_x + 140 && mx < base_x + 420 && my >= y && my < y + kh {
        handle_key(b' ');
        return true;
    }
    if mx >= base_x + 428 && mx < base_x + 498 && my >= y && my < y + kh {
        unsafe { OSK_SHIFT = !OSK_SHIFT; }
        return true;
    }
    false
}

pub fn open_media_player() {
    open_win(3);
}

pub fn open_video_player() {
    open_win(12);
}

pub fn clear_media_pick() {
    unsafe {
        MEDIA_PICK = false;
    }
}

pub fn media_pick_active() -> bool {
    unsafe { MEDIA_PICK }
}


fn draw_xp_wallpaper(w: usize, h: usize) {
    // Real wallpaper asset (RGB565), scaled to current FB (e.g. 1366x768)
    let sw = crate::wallpaper_data::WALL_W;
    let sh = crate::wallpaper_data::WALL_H;
    let src = &crate::wallpaper_data::WALL_RGB565;
    if sw == 0 || sh == 0 || w == 0 || h == 0 {
        graphics::fill(0x00102840);
        return;
    }
    let mut y = 0usize;
    while y < h {
        let sy = y * sh / h;
        let mut x = 0usize;
        while x < w {
            let sx = x * sw / w;
            let idx = sy * sw + sx;
            let c = if idx < src.len() { src[idx] } else { 0 };
            // RGB565 -> 0x00RRGGBB
            let r = ((c >> 11) & 0x1F) as u32;
            let g = ((c >> 5) & 0x3F) as u32;
            let b = (c & 0x1F) as u32;
            let rgb = ((r * 255 / 31) << 16) | ((g * 255 / 63) << 8) | (b * 255 / 31);
            graphics::put_pixel(x, y, rgb);
            x += 1;
        }
        y += 1;
    }
}


fn draw_desktop_icons() {
    use crate::gui::icon::{self, IconId};
    use crate::gui::font;
    use crate::gui::theme;
    // Classic desktop layout: column of icons with labels
    // EXACT length: loop bound must match array len (OOB here aborted paint
    // before taskbar/cursor - desktop looked incomplete).
    let items: [(IconId, usize); 9] = [
        (IconId::MyComputer, 7),
        (IconId::MyDocuments, 5),
        (IconId::Terminal, 0),
        (IconId::Network, 2),
        (IconId::Settings, 3), // Media Player
        (IconId::Terminal, 10), // On-Screen Keyboard
        (IconId::Network, 13), // Browser
        (IconId::Settings, 9), // Settings
        (IconId::RecycleBin, 99),
    ];
    let n = items.len();
    let mut i = 0usize;
    while i < n {
        let (id, _slot) = items[i];
        let x = 24usize;
        let y = 36 + i * 72;
        let sel = unsafe { SELECTED_ICON == i };
        icon::blit(id, x, y, sel);
        let lab = if i == 4 {
            "Media Player"
        } else if i == 5 {
            "Keyboard"
        } else if i == 6 {
            "Browser"
        } else {
            icon::label(id)
        };
        let tw = lab.len() * 8;
        let lx = if tw < 48 { x + (48 - tw) / 2 } else { x };
        font::draw_shadowed(lx, y + theme::ICON_SIZE + 4, lab, 0x00FFFFFF, 0x00404040);
        i += 1;
    }
}


fn hit_desktop_icon(mx: i32, my: i32) -> Option<usize> {
    // Must match draw_desktop_icons: 9 icons, y = 36 + i*72
    let mut i = 0usize;
    while i < 9 {
        let y = 36 + (i as i32) * 72;
        if mx >= 16 && mx < 80 && my >= y && my < y + 64 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn draw_taskbar_buttons(w: usize, h: usize) {
    let mut x = 78usize; // after Start
    let y = h.saturating_sub(26);
    unsafe {
        let mut i = 0usize;
        while i < MAX_WIN {
            if WINS[i].visible || WINS[i].minimized {
                let title = win_title(WINS[i].kind);
                // short label for taskbar
                let short = match WINS[i].kind {
                    WinKind::Terminal => "Term",
                    WinKind::MyComputer => "Comp",
                    WinKind::Files => "Files",
                    WinKind::Network => "Net",
                    WinKind::Sound => "Media",
                    WinKind::Video => "Vid",
                    WinKind::SysProps => "Sys",
                    WinKind::DateTime => "Time",
                    WinKind::About => "About",
                    WinKind::Settings => "Settings",
                    WinKind::Keyboard => "Keys",
                    WinKind::Alarm => "Alarm",
                    WinKind::Movie => "Video",
                    WinKind::Browser => "Web",
                };
                let bw = short.len() * 8 + 20;
                if x + bw > w.saturating_sub(100) {
                    break;
                }
                let pressed = i == FOCUS && WINS[i].visible && !WINS[i].minimized;
                let col = if pressed { 0x001C4F9C } else { 0x003C7FB1 };
                graphics::fill_rect(x, y, bw, 22, col);
                graphics::border_rect(x, y, bw, 22, if pressed { 0x000A246A } else { 0x006B9ACD });
                if pressed {
                    graphics::fill_rect(x + 1, y + 1, bw - 2, 1, 0x000A246A);
                } else {
                    graphics::fill_rect(x + 1, y + 1, bw - 2, 1, 0x006BB0E0);
                }
                graphics::draw_str(x + 10, y + 7, short, COL_TITLE_TEXT);
                x += bw + 3;
            }
            i += 1;
        }
    }
}


fn hit_taskbar(mx: i32, my: i32) -> Option<usize> {
    let h = graphics::height() as i32;
    if my < h - (TASKBAR_H as i32) {
        return None;
    }
    let mut x = 78i32;
    unsafe {
        let mut i = 0usize;
        while i < MAX_WIN {
            if WINS[i].visible || WINS[i].minimized {
                let title = win_title(WINS[i].kind);
                let bw = (title.len() as i32) * 8 + 16;
                if mx >= x && mx < x + bw {
                    return Some(i);
                }
                x += bw + 6;
            }
            i += 1;
        }
    }
    None
}

fn in_close(idx: usize, mx: i32, my: i32) -> bool {
    unsafe {
        let w = &WINS[idx];
        let cx = w.x + w.w - 28;
        let cy = w.y + 4;
        mx >= cx && mx < cx + 22 && my >= cy && my < cy + 16
    }
}

fn clamp_win(idx: usize) {
    unsafe {
        if idx >= MAX_WIN {
            return;
        }
        let sw = graphics::width() as i32;
        let sh = graphics::height() as i32;
        let tb = TASKBAR_H as i32;
        let work_h = (sh - tb).max(200);
        // Maximized: lock to work area
        if WINS[idx].maximized {
            WINS[idx].x = 0;
            WINS[idx].y = 0;
            WINS[idx].w = sw.max(320);
            WINS[idx].h = work_h;
            return;
        }
        // Minimum size (usable title + client)
        if WINS[idx].w < 200 {
            WINS[idx].w = 200;
        }
        if WINS[idx].h < 150 {
            WINS[idx].h = 150;
        }
        if WINS[idx].w > sw {
            WINS[idx].w = sw;
        }
        if WINS[idx].h > work_h {
            WINS[idx].h = work_h;
        }
        // Keep at least 48px of title bar on-screen (can drag back)
        if WINS[idx].x + WINS[idx].w < 48 {
            WINS[idx].x = 48 - WINS[idx].w;
        }
        if WINS[idx].x > sw - 48 {
            WINS[idx].x = (sw - 48).max(0);
        }
        if WINS[idx].y < 0 {
            WINS[idx].y = 0;
        }
        if WINS[idx].y > work_h - TITLE_H {
            WINS[idx].y = (work_h - TITLE_H).max(0);
        }
    }
}

fn apply_mouse_delta(dx: i32, dy: i32) {
    unsafe {
        let sw = graphics::width() as i32;
        let sh = graphics::height() as i32;
        MX += dx;
        MY += dy;
        if MX < 0 {
            MX = 0;
        }
        if MY < 0 {
            MY = 0;
        }
        if MX >= sw {
            MX = sw - 1;
        }
        if MY >= sh {
            MY = sh - 1;
        }
        // Cursor only - never force window repaint on every mouse move
        DIRTY_CURSOR = true;
    }
}

fn handle_terminal_scroll_click(mx: i32, my: i32) -> bool {
    unsafe {
        if FOCUS >= MAX_WIN || !WINS[FOCUS].visible || WINS[FOCUS].kind != WinKind::Terminal {
            return false;
        }
        let w = &WINS[FOCUS];
        let bar_x = w.x + w.w - 18;
        let bar_top = w.y + TITLE_H as i32 + 4;
        let bar_bottom = w.y + w.h - 6;
        if mx < bar_x || mx >= bar_x + 14 || my < bar_top || my >= bar_bottom {
            return false;
        }
        let view_rows = term_visible_rows_for(w.h);
        let max_view = if TERM_COUNT > view_rows { TERM_COUNT - view_rows } else { 0 };
        if my < bar_top + 14 {
            term_scroll_up(); // ^ = older
        } else if my >= bar_bottom - 14 {
            term_scroll_down(); // v = newer
        } else if max_view > 0 {
            let track_top = bar_top + 14;
            let track_h = (bar_bottom - 14 - track_top).max(1);
            let rel = (my - track_top).max(0) as usize;
            // top of track = max_view (oldest), bottom = 0 (live)
            TERM_VIEW = max_view.saturating_sub(rel * max_view / track_h as usize);
        }
        DIRTY_UI = true;
        true
    }
}

fn redraw_input_window(idx: usize) {
    unsafe {
        cursor_restore();
        draw_window(idx);
        CURSOR_SAVED = false;
        cursor_save_and_draw(MX, MY);
        crate::graphics::present_full_fast();
    }
}

/// Redraw ONLY this explorer window (never DIRTY_UI / all windows).
fn redraw_files_after_click(idx: usize) {
    // Always full window redraw. Partial selection paint rebooted AH532.
    let _ = crate::files_mgr::take_paint_hint();
    redraw_input_window(idx);
}

/// Repaint only old+new focused window frames (no wallpaper wipe, less flash).
fn paint_focus_change(old_idx: usize, new_idx: usize) {
    unsafe {
        cursor_restore();
        if old_idx < MAX_WIN && old_idx != new_idx && WINS[old_idx].visible {
            draw_window(old_idx);
        }
        if new_idx < MAX_WIN && WINS[new_idx].visible {
            draw_window(new_idx);
        }
        // taskbar button pressed state
        let w = graphics::width();
        let h = graphics::height();
        let tb = TASKBAR_H;
        graphics::fill_rect(0, h.saturating_sub(tb), w, tb, COL_TASKBAR);
        graphics::fill_rect(0, h.saturating_sub(tb), w, 2, COL_TASKBAR_TOP);
        draw_taskbar_buttons(w, h);
        draw_status_icons_and_clock(w, h);
        draw_start_button(w, h);
        CURSOR_SAVED = false;
        cursor_save_and_draw(MX, MY);
        crate::graphics::present_full_fast();
    }
}

fn handle_mouse_buttons(buttons: u8) {
    unsafe {
        let left = buttons & 1;
        let prev_left = PREV_MB & 1;
        let mx = MX;
        let my = MY;
        FRAME_N = FRAME_N.wrapping_add(1);

                // Desktop context menu (right-click empty area)
        let right = buttons & 2;
        let prev_right = PREV_MB & 2;
        if right != 0 && prev_right == 0 {
            // if click on desktop (not on taskbar, not on window)
            let sh = graphics::height() as i32;
            let on_taskbar = my >= sh - (TASKBAR_H as i32);
            let on_win = hit_window(mx, my).is_some();
            if !on_taskbar && !on_win {
                if let Some(act) = hit_ctx_menu(mx, my) {
                    // already open - handle below
                } else if CTX_MENU {
                    if let Some(act) = hit_ctx_menu(mx, my) {
                        let _ = act;
                    } else {
                        CTX_MENU = false;
                        DIRTY_UI = true;
                    }
                } else {
                    CTX_X = mx;
                    CTX_Y = my;
                    CTX_MENU = true;
                    START_MENU = false;
                    DIRTY_UI = true;
                }
            }
        }
        if CTX_MENU && (buttons & 1) != 0 && (PREV_MB & 1) == 0 {
            if let Some(act) = hit_ctx_menu(mx, my) {
                CTX_MENU = false;
                DIRTY_UI = true;
                match act {
                    0 => { DIRTY_FULL = true; } // Refresh
                    1 => {
                        // New text file on AetherFS
                        if crate::fs::is_mounted() {
                            let _ = crate::fs::create("NEW.TXT");
                            let _ = crate::fs::write("NEW.TXT", b"New file\n");
                        }
                    }
                    2 => open_win(0),
                    3 => open_win(7),
                    4 => open_win(8),
                    _ => {}
                }
            } else {
                CTX_MENU = false;
                DIRTY_UI = true;
            }
        }
// Right-click / secondary-click → Explorer context menu
        // bit1 = right, bit2 = middle (some touchpads). Also: Shift held + left edge.
        let right = buttons & 2;
        let prev_right = PREV_MB & 2;
        let mid = buttons & 4;
        let prev_mid = PREV_MB & 4;
        let shift_left = (buttons & 1) != 0
            && (PREV_MB & 1) == 0
            && crate::drivers::ps2::shift_held();
        let secondary = (right != 0 && prev_right == 0)
            || (mid != 0 && prev_mid == 0)
            || shift_left;
        if secondary {
            if let Some(idx) = hit_test(mx, my) {
                let is_explorer = matches!(
                    WINS[idx].kind,
                    WinKind::Files | WinKind::MyComputer
                );
                if is_explorer && my >= WINS[idx].y + TITLE_H {
                    bring_to_front(idx);
                    serial::write_str("[MOUSE] explorer context\n");
                    let changed = crate::files_mgr::on_click(
                        WINS[idx].x, WINS[idx].y, WINS[idx].w, WINS[idx].h,
                        TITLE_H, mx, my, true, false,
                    );
                    if changed {
                        redraw_files_after_click(idx);
                    }
                    PREV_MB = buttons;
                    MB = buttons;
                    return;
                }
            }
        }
        if left != 0 && prev_left == 0 {
            // Terminal scrollbar is a real clickable control, independent of keyboard input.
            if handle_browser_scroll_click(mx, my) {
                DIRTY_UI = true;
            }
            if handle_terminal_scroll_click(mx, my) {
                PREV_MB = buttons;
                MB = buttons;
                return;
            }
            // XP Start button (bottom-left)
            let sh = graphics::height() as i32;
            if my >= sh - (TASKBAR_H as i32) && mx < 74 {
                START_MENU = !START_MENU;
                DIRTY_UI = true;
            }
            // Quick Launch T/F/N
            if my >= sh - (TASKBAR_H as i32) {
                if mx >= 92 && mx < 108 {
                    open_win(0);
                } else if mx >= 112 && mx < 128 {
                    open_win(5);
                } else if mx >= 132 && mx < 148 {
                    open_win(2);
                }
            }
            // Start menu items
            if START_MENU {
                if let Some(act) = hit_start_menu(mx, my) {
                    START_MENU = false;
                    {
                        let slot = start_menu_slot(act);
                        if slot == 12 {
                            open_win(12);
                            crate::video_player::open_test();
                        } else if slot < MAX_WIN {
                            open_win(slot);
                        }
                    }
                } else if my >= 28 {
                    // click outside menu closes it (unless handled below)
                    // don't close if clicking dock/icons/windows - only empty
                }
            }
            // System tray
            if let Some(ti) = hit_tray(mx, my) {
                match ti {
                    1 => open_win(2), // Network
                    2 => open_win(3), // Sound
                    3 => open_win(4), // Video
                    4 => open_win(6), // DateTime
                    _ => {}
                }
            }
            // Taskbar window buttons
            if let Some(idx) = hit_taskbar(mx, my) {
                open_win(idx);
            }
            // Dock launcher left
            let sh = graphics::height() as i32;
            if my >= sh - (TASKBAR_H as i32) && mx >= 90 && mx < 320 {
                let slot = if mx < 48 { 0 }
                    else if mx < 96 { 2 }
                    else if mx < 144 { 3 }
                    else if mx < 192 { 4 }
                    else if mx < 248 { 5 }
                    else if mx < 320 { 1 }
                    else { 255 };
                if slot < MAX_WIN {
                    open_win(slot);
                }
            }
            // Desktop icons: first click selects, second click opens.
            if let Some(idx) = hit_desktop_icon(mx, my) {
                let dbl = LAST_DESKTOP_ICON == idx
                    && FRAME_N.wrapping_sub(LAST_DESKTOP_ICON_FRAME) < 25;
                SELECTED_ICON = idx;
                LAST_DESKTOP_ICON = idx;
                LAST_DESKTOP_ICON_FRAME = FRAME_N;
                DIRTY_UI = true;
                if dbl {
                    match idx {
                        0 => open_win(7),
                        1 => open_win(5),
                        2 => open_win(0),
                        3 => open_win(2),
                        4 => open_win(3),  // Media Player
                        5 => open_win(10), // On-Screen Keyboard
                        6 => open_win(13), // Browser
                        7 => open_win(9),  // Settings
                        _ => {}
                    }
                    LAST_DESKTOP_ICON = usize::MAX;
                }
            }
            // Windows
            if let Some(idx) = hit_test(mx, my) {
                let old_focus = FOCUS;
                bring_to_front(idx);
                if in_close(idx, mx, my) {
                    if WINS[idx].kind == WinKind::Movie {
                        crate::video_player::stop();
                    }
                    if WINS[idx].kind == WinKind::Sound {
                        crate::media_player::stop();
                    }
                    WINS[idx].visible = false;
                    WINS[idx].minimized = false;
                    WINS[idx].maximized = false;
                    focus_topmost();
                    // Must wipe wallpaper under closed window
                    DIRTY_FULL = true;
                } else if in_max(idx, mx, my) {
                    toggle_maximize(idx);
                    DIRTY_FULL = true;
                } else if in_min(idx, mx, my) {
                    toggle_minimize(idx);
                    // toggle_minimize already sets DIRTY_FULL
                } else if in_resize(idx, mx, my)
                    // Files: right-edge scrollbar lives above the corner - do not
                    // start resize when the pointer is still in the list body.
                    && !(WINS[idx].kind == WinKind::Files
                        && my < WINS[idx].y + WINS[idx].h - 18)
                {
                    RESIZING = true;
                    RESIZE_WIN = idx;
                    RESIZE_EDGES = resize_edges(idx, mx, my);
                    if RESIZE_EDGES == 0 {
                        RESIZE_EDGES = 3; // default SE corner
                    }
                    DRAG_OLD_W = WINS[idx].w;
                    DRAG_OLD_H = WINS[idx].h;
                } else if WINS[idx].kind == WinKind::Movie
                    && my >= WINS[idx].y + TITLE_H
                {
                    let wx = WINS[idx].x;
                    let wy = WINS[idx].y;
                    let wh = WINS[idx].h;
                    let cy = wy + wh - 50;
                    if my >= cy + 10 && my < cy + 34 {
                        if mx >= wx + 16 && mx < wx + 72 {
                            crate::video_player::play();
                            DIRTY_UI = true;
                        } else if mx >= wx + 80 && mx < wx + 144 {
                            crate::video_player::pause();
                            DIRTY_UI = true;
                        } else if mx >= wx + 152 && mx < wx + 208 {
                            crate::video_player::stop();
                            DIRTY_UI = true;
                        } else if mx >= wx + 220 && mx < wx + 276 {
                            crate::video_player::next_clip();
                            DIRTY_UI = true;
                        }
                    }

                } else if WINS[idx].kind == WinKind::Alarm
                    && my >= WINS[idx].y + TITLE_H
                {
                    let _ = crate::alarm::on_click(
                        WINS[idx].x, WINS[idx].y, WINS[idx].w, WINS[idx].h,
                        TITLE_H, mx, my,
                    );
                    DIRTY_UI = true;
                } else if WINS[idx].kind == WinKind::Network
                    && my >= WINS[idx].y + TITLE_H
                {
                    let nx = WINS[idx].x;
                    let ny = WINS[idx].y;
                    let nw = WINS[idx].w;
                    let nh = WINS[idx].h;
                    // Password dialog modal hits
                    if WIFI_UI_CONNECT_DIALOG {
                        let dx = nx + nw / 2 - 160;
                        let dy = ny + nh / 2 - 70;
                        // Connect in dialog
                        if mx >= dx + 40 && mx < dx + 140 && my >= dy + 100 && my < dy + 126 {
                            if WIFI_UI_SELECTED >= 0 {
                                let ok = crate::drivers::wifi::connect(
                                    WIFI_UI_SELECTED as usize,
                                    &WIFI_UI_PASSWORD[..WIFI_UI_PASSWORD_LEN],
                                );
                                WIFI_UI_STATUS = if ok { 2 } else { 3 };
                                WIFI_UI_CONNECT_DIALOG = false;
                            }
                            DIRTY_UI = true;
                        } else if mx >= dx + 160 && mx < dx + 260 && my >= dy + 100 && my < dy + 126 {
                            WIFI_UI_CONNECT_DIALOG = false;
                            DIRTY_UI = true;
                        }
                    } else {
                        let lx = nx + 220;
                        let ly = ny + TITLE_H + 48;
                        let lw = nw - 232;
                        let lh = nh - TITLE_H - 100;
                        let by = ly + lh - 36;
                        let list_top = ly + 52;
                        // AP row select
                        let n = crate::drivers::wifi::ap_count();
                        let max_rows = if n > 12 { 12 } else { n };
                        let mut row = 0usize;
                        while row < max_rows {
                            let y = list_top + (row as i32) * 22;
                            if mx >= lx && mx < lx + lw && my >= y && my < y + 22 {
                                WIFI_UI_SELECTED = row as i32;
                                DIRTY_UI = true;
                            }
                            row += 1;
                        }
                        // Refresh / Enable WiFi
                        if mx >= lx + 8 && mx < lx + 96 && my >= by && my < by + 26 {
                            // First Enable only: bring_up once. Later clicks = safe scan.
                            if !crate::drivers::wifi::alive_ok()
                                && crate::drivers::wifi::bring_step() == 0
                            {
                                WIFI_UI_STATUS = 1;
                                DIRTY_UI = true;
                                // Run bring_up outside input path via flag
                                WIFI_UI_SCAN_REQUESTED = true; // also triggers enable below
                                WIFI_UI_ENABLE_REQ = true;
                            } else {
                                WIFI_UI_SCAN_REQUESTED = true;
                                WIFI_UI_STATUS = 1;
                                DIRTY_UI = true;
                            }
                        }
                        // Connect
                        if mx >= lx + 104 && mx < lx + 200 && my >= by && my < by + 26 {
                            if WIFI_UI_SELECTED >= 0 {
                                WIFI_UI_CONNECT_DIALOG = true;
                                WIFI_UI_PASSWORD_LEN = 0;
                                DIRTY_UI = true;
                            }
                        }
                        // Disconnect
                        if mx >= lx + 208 && mx < lx + 308 && my >= by && my < by + 26 {
                            crate::drivers::wifi::disconnect();
                            WIFI_UI_STATUS = 0;
                            DIRTY_UI = true;
                        }
                        // Ethernet Get IP
                        let px = nx + 12;
                        let py = ny + TITLE_H + 48;
                        if mx >= px + 10 && mx < px + 110 && my >= py + 170 && my < py + 194 {
                            crate::drivers::net::init();
                            let _ = crate::drivers::net::dhcp_run();
                            DIRTY_UI = true;
                        }
                    }
                } else if WINS[idx].kind == WinKind::MyComputer
                    && my >= WINS[idx].y + TITLE_H
                {
                    // My Computer and the Files icon must share one Explorer
                    // implementation. This removes the legacy click path.
                    let right = (buttons & 2) != 0;
                    // Double-click: same row, within ~0.4s of button events, but not
                    // the same physical press. Requires a prior click that was NOT open.
                    // Double-click = 2 consecutive presses on the SAME row (no timer).
                    let mut double_click = false;
                    if !right {
                        let row = (my - (WINS[idx].y + TITLE_H + 56 + 30)) / 32;
                        if row == FILES_CLICK_ROW && row >= 0 {
                            FILES_CLICK_N = FILES_CLICK_N.saturating_add(1);
                        } else {
                            FILES_CLICK_ROW = row;
                            FILES_CLICK_N = 1;
                        }
                        double_click = FILES_CLICK_N >= 2;
                        if double_click {
                            FILES_CLICK_N = 0;
                            FILES_CLICK_ROW = -1;
                        }
                    }
                    let changed = crate::files_mgr::on_click(
                        WINS[idx].x, WINS[idx].y, WINS[idx].w, WINS[idx].h,
                        TITLE_H, mx, my, right, double_click,
                    );
                    if changed { redraw_files_after_click(idx); }
                } else if WINS[idx].kind == WinKind::Files
                    && my >= WINS[idx].y + TITLE_H
                {
                    let right = (buttons & 2) != 0;
                    let mut double_click = false;
                    if !right {
                        let row = (my - (WINS[idx].y + TITLE_H + 56 + 30)) / 32;
                        if row == FILES_CLICK_ROW && row >= 0 {
                            FILES_CLICK_N = FILES_CLICK_N.saturating_add(1);
                        } else {
                            FILES_CLICK_ROW = row;
                            FILES_CLICK_N = 1;
                        }
                        double_click = FILES_CLICK_N >= 2;
                        if double_click {
                            FILES_CLICK_N = 0;
                            FILES_CLICK_ROW = -1;
                        }
                    }
                    let changed = crate::files_mgr::on_click(
                        WINS[idx].x, WINS[idx].y, WINS[idx].w, WINS[idx].h,
                        TITLE_H, mx, my, right, double_click,
                    );
                    if changed { redraw_files_after_click(idx); }
                } else if WINS[idx].kind == WinKind::Settings
                    && my >= WINS[idx].y + TITLE_H
                {
                    let sx = WINS[idx].x;
                    let sy = WINS[idx].y;
                    let sw = WINS[idx].w;
                    let sh = WINS[idx].h;
                    if SETTINGS_VIEW == 0 {
                        // Ten native Settings categories. Mouse is functional;
                        // the rest open explicit future-subsystem placeholders.
                        let mut i = 0usize;
                        while i < 10 {
                            let row_y = sy + 66 + (i as i32) * 25;
                            if mx >= sx + 14 && mx < sx + 177 && my >= row_y - 2 && my < row_y + 21 {
                                SETTINGS_VIEW = (i + 1) as u8;
                                if i == 0 {
                                    CURSOR_PENDING = match CURSOR_COLOR {
                                        0x00000000 => 1,
                                        0x00E81123 => 2,
                                        0x0000A000 => 3,
                                        0x000000CC => 4,
                                        _ => 0,
                                    };
                                }
                                DIRTY_UI = true;
                                break;
                            }
                            i += 1;
                        }
                    } else if SETTINGS_VIEW == 1 {
                        let colors: [u32; 5] = [0x00FFFFFF, 0x00000000, 0x00E81123, 0x0000A000, 0x000000CC];
                        let mut i = 0usize;
                        while i < 5 {
                            let bx = sx + 28 + (i as i32) * 88;
                            if mx >= bx && mx < bx + 68 && my >= sy + 112 && my < sy + 194 {
                                CURSOR_PENDING = i as u8;
                                DIRTY_UI = true;
                            }
                            i += 1;
                        }
                        let okx = sx + sw - 184;
                        let cancelx = sx + sw - 94;
                        let by = sy + sh - 42;
                        if my >= by && my < by + 24 {
                            if mx >= okx && mx < okx + 78 {
                                CURSOR_COLOR = colors[CURSOR_PENDING as usize];
                                SETTINGS_VIEW = 0;
                                DIRTY_UI = true;
                            } else if mx >= cancelx && mx < cancelx + 78 {
                                SETTINGS_VIEW = 0;
                                CURSOR_PENDING = 0;
                                DIRTY_UI = true;
                            }
                        }
                    } else {
                        // Future-category placeholder: Back returns to Settings home.
                        let bx = sx + sw - 94;
                        let by = sy + sh - 42;
                        if mx >= bx && mx < bx + 78 && my >= by && my < by + 24 {
                            SETTINGS_VIEW = 0;
                            DIRTY_UI = true;
                        }
                    }
                } else if WINS[idx].kind == WinKind::Keyboard
                    && my >= WINS[idx].y + TITLE_H
                {
                    if handle_osk_click(mx, my, WINS[idx].x, WINS[idx].y) {
                        DIRTY_UI = true;
                    }
                } else if WINS[idx].kind == WinKind::Browser
                    && my >= WINS[idx].y + TITLE_H
                {
                    let wx = WINS[idx].x;
                    let wy = WINS[idx].y;
                    let ww = WINS[idx].w;
                    let by = wy + TITLE_H;
                    // Go button
                    let gx = wx + ww - 188;
                    if my >= by + 8 && my < by + 30 {
                        if mx >= gx && mx < gx + 32 {
                            browser_go();
                            DIRTY_UI = true;
                        } else if mx >= gx + 36 && mx < gx + 68 {
                            browser_back();
                            DIRTY_UI = true;
                        } else if mx >= gx + 72 && mx < gx + 108 {
                            BR_URL = [0; 96];
                            let d = b"example.com";
                            let mut i = 0usize;
                            while i < d.len() {
                                BR_URL[i] = d[i];
                                i += 1;
                            }
                            BR_URL_LEN = d.len();
                            BR_INIT = true;
                            browser_go();
                            DIRTY_UI = true;
                        } else if mx >= gx + 112 && mx < gx + 152 {
                            BR_URL = [0; 96];
                            let d = b"example.org";
                            let mut i = 0usize;
                            while i < d.len() {
                                BR_URL[i] = d[i];
                                i += 1;
                            }
                            BR_URL_LEN = d.len();
                            BR_INIT = true;
                            browser_go();
                            DIRTY_UI = true;
                        } else if mx >= wx + 44 && mx < gx {
                            // click URL bar - focus for typing
                            TEXT_FOCUS = idx;
                            BR_FOCUS_URL = true;
                            DIRTY_UI = true;
                        }
                    }
                } else if WINS[idx].kind == WinKind::Video
                    && my >= WINS[idx].y + TITLE_H
                {
                    let wx = WINS[idx].x;
                    let wy = WINS[idx].y;
                    let ww = WINS[idx].w;
                    let wh = WINS[idx].h;
                    let tab_y = wy + TITLE_H + 6;
                    // Tabs
                    if my >= tab_y && my < tab_y + 22 {
                        if mx >= wx + 12 && mx < wx + 92 {
                            DISP_TAB = 0; DIRTY_UI = true;
                        } else if mx >= wx + 100 && mx < wx + 180 {
                            DISP_TAB = 1; DIRTY_UI = true;
                        }
                    } else if DISP_TAB == 0 {
                        // Mode list
                        let lx = wx + 16;
                        let ly = wy + 104;
                        let lw = 280;
                        let lh = 160;
                        if mx >= lx && mx < lx + lw && my >= ly && my < ly + lh {
                            let row = ((my - ly - 6) / 18) as isize;
                            if row >= 0 {
                                let n = crate::drivers::intel_igpu::mode_count();
                                if (row as usize) < n {
                                    DISP_SEL = row as usize;
                                    DIRTY_UI = true;
                                }
                            }
                        }
                        // Apply / OK
                        let bx = wx + ww - 180;
                        let by = wy + wh - 40;
                        if my >= by && my < by + 26 {
                            if mx >= bx && mx < bx + 70 {
                                // Apply selected mode
                                if let Some((w, h, bpp)) = crate::drivers::intel_igpu::mode_at(DISP_SEL) {
                                    let st = crate::drivers::intel_igpu::apply_mode(w, h, bpp);
                                    let _ = st;
                                    DIRTY_FULL = true;
                                }
                            } else if mx >= bx + 80 && mx < bx + 150 {
                                if let Some((w, h, bpp)) = crate::drivers::intel_igpu::mode_at(DISP_SEL) {
                                    let _ = crate::drivers::intel_igpu::apply_mode(w, h, bpp);
                                }
                                // close window
                                WINS[idx].visible = false;
                                DIRTY_FULL = true;
                            }
                        }
                    }
                } else if WINS[idx].kind == WinKind::Sound
                    && my >= WINS[idx].y + TITLE_H
                {
                    let wx = WINS[idx].x;
                    let wy = WINS[idx].y;
                    let ww = WINS[idx].w;
                    let wh = WINS[idx].h;
                    let menu_y = wy + TITLE_H;
                    let mut handled = false;

                    // File / Playback menu bar
                    if my >= menu_y && my < menu_y + 22 {
                        if mx >= wx + 8 && mx < wx + 48 {
                            MEDIA_MENU = if MEDIA_MENU == 1 { 0 } else { 1 };
                            handled = true;
                            DIRTY_UI = true;
                        } else if mx >= wx + 52 && mx < wx + 116 {
                            MEDIA_MENU = if MEDIA_MENU == 2 { 0 } else { 2 };
                            handled = true;
                            DIRTY_UI = true;
                        }
                    }

                    // File dropdown items
                    if !handled && MEDIA_MENU == 1 {
                        let dx = wx + 8;
                        let dy = menu_y + 22;
                        if mx >= dx && mx < dx + 160 && my >= dy && my < dy + 72 {
                            if my >= dy + 4 && my < dy + 24 {
                                // Open File... → Explorer pick mode
                                MEDIA_PICK = true;
                                MEDIA_MENU = 0;
                                open_win(5); // Files
                                crate::files_mgr::open_media_folder();
                                handled = true;
                                DIRTY_UI = true;
                            } else if my >= dy + 24 && my < dy + 44 {
                                MEDIA_PICK = true;
                                MEDIA_MENU = 0;
                                open_win(5);
                                handled = true;
                                DIRTY_UI = true;
                            } else if my >= dy + 44 && my < dy + 64 {
                                MEDIA_MENU = 0;
                                handled = true;
                                DIRTY_UI = true;
                            }
                        } else {
                            MEDIA_MENU = 0;
                            DIRTY_UI = true;
                        }
                    }

                    // Playback dropdown
                    if !handled && MEDIA_MENU == 2 {
                        let dx = wx + 52;
                        let dy = menu_y + 22;
                        if mx >= dx && mx < dx + 120 && my >= dy && my < dy + 92 {
                            if my >= dy + 4 && my < dy + 24 {
                                let _ = crate::drivers::audio::ensure_ready();
                                if crate::media_player::data_bytes() == 0 {
                                    let _ = crate::media_player::open_builtin(
                                        crate::media_player::selected_builtin());
                                }
                                crate::media_player::play();
                            } else if my >= dy + 24 && my < dy + 44 {
                                crate::media_player::pause();
                            } else if my >= dy + 44 && my < dy + 64 {
                                crate::media_player::stop();
                            } else if my >= dy + 64 && my < dy + 84 {
                                crate::media_player::toggle_mute();
                            }
                            MEDIA_MENU = 0;
                            handled = true;
                            DIRTY_UI = true;
                        } else {
                            MEDIA_MENU = 0;
                            DIRTY_UI = true;
                        }
                    }

                    // Track list
                    let n = crate::media_player::builtin_count();
                    if !handled {
                        let mut i = 0usize;
                        while i < n {
                            let y = menu_y + 66 + i as i32 * 22;
                            if my >= y && my < y + 20 && mx >= wx + 10 && mx < wx + ww as i32 - 14 {
                                let _ = crate::media_player::select_builtin(i);
                                let _ = crate::media_player::open_builtin(i);
                                handled = true;
                                DIRTY_UI = true;
                                break;
                            }
                            i += 1;
                        }
                    }

                    // Open File / Browse buttons under list
                    if !handled {
                        let oby = menu_y + 66 + n as i32 * 22 + 8;
                        if my >= oby && my < oby + 24 {
                            if mx >= wx + 12 && mx < wx + 132 {
                                MEDIA_PICK = true;
                                open_win(5);
                                crate::files_mgr::open_media_folder();
                                handled = true;
                                DIRTY_UI = true;
                            } else if mx >= wx + 140 && mx < wx + 240 {
                                MEDIA_PICK = true;
                                open_win(5);
                                crate::files_mgr::open_media_folder();
                                handled = true;
                                DIRTY_UI = true;
                            }
                        }
                    }

                    // Transport buttons
                    let by = wy + wh as i32 - 88;
                    if !handled && my >= by && my < by + 24 {
                        if mx >= wx + 12 && mx < wx + 68 {
                            let _ = crate::drivers::audio::ensure_ready();
                            if crate::media_player::data_bytes() == 0 {
                                let _ = crate::media_player::open_builtin(
                                    crate::media_player::selected_builtin());
                            }
                            crate::media_player::play();
                            handled = true;
                        } else if mx >= wx + 76 && mx < wx + 132 {
                            crate::media_player::pause();
                            handled = true;
                        } else if mx >= wx + 140 && mx < wx + 196 {
                            crate::media_player::stop();
                            handled = true;
                        } else if mx >= wx + 210 && mx < wx + 250 {
                            crate::media_player::volume_down();
                            handled = true;
                        } else if mx >= wx + 256 && mx < wx + 296 {
                            crate::media_player::volume_up();
                            handled = true;
                        } else if mx >= wx + 304 && mx < wx + 352 {
                            crate::media_player::toggle_mute();
                            handled = true;
                        }
                        DIRTY_UI = true;
                    }
                    if !handled && MEDIA_MENU == 0 {
                        // no beep on empty client area
                    }
                } else if in_title(idx, mx, my) {
                    // Double-click title → maximize/restore
                    if LAST_CLICK_WIN == idx && FRAME_N.wrapping_sub(LAST_CLICK_FRAME) < 25 {
                        toggle_maximize(idx);
                        LAST_CLICK_WIN = 255;
                        DIRTY_FULL = true;
                    } else {
                        LAST_CLICK_WIN = idx;
                        LAST_CLICK_FRAME = FRAME_N;
                        // Drag from maximized: restore to normal size under cursor (OS norm)
                        if WINS[idx].maximized {
                            let sw = graphics::width() as i32;
                            let rw = if WINS[idx].rw >= 200 { WINS[idx].rw } else { 480 };
                            let rh = if WINS[idx].rh >= 150 { WINS[idx].rh } else { 320 };
                            WINS[idx].maximized = false;
                            WINS[idx].w = rw.min(sw);
                            WINS[idx].h = rh;
                            WINS[idx].x = (mx - rw / 2).max(0);
                            WINS[idx].y = 0;
                            clamp_win(idx);
                            DIRTY_FULL = true;
                        }
                        DRAGGING = true;
                        DRAG_WIN = idx;
                        DRAG_OX = mx - WINS[idx].x;
                        DRAG_OY = my - WINS[idx].y;
                        DRAG_OLD_X = WINS[idx].x;
                        DRAG_OLD_Y = WINS[idx].y;
                        DRAG_OLD_W = WINS[idx].w;
                        DRAG_OLD_H = WINS[idx].h;
                        if old_focus != idx {
                            paint_focus_change(old_focus, idx);
                        }
                    }
                } else {
                    // Client click: focus only, no global flash
                    if old_focus != idx {
                        paint_focus_change(old_focus, idx);
                    }
                }
            }
        }
        if left == 0 && prev_left != 0 {
            if DRAGGING || RESIZING {
                // Restore desktop under old footprint
                DIRTY_FULL = true;
            }
            DRAGGING = false;
            RESIZING = false;
        }
        if DRAGGING && left != 0 {
            WINS[DRAG_WIN].x = mx - DRAG_OX;
            WINS[DRAG_WIN].y = my - DRAG_OY;
            clamp_win(DRAG_WIN);
            render_drag_step();
        }
        if RESIZING && left != 0 {
            let idx = RESIZE_WIN;
            let edges = RESIZE_EDGES;
            let sw = graphics::width() as i32;
            let sh = graphics::height() as i32;
            let tb = TASKBAR_H as i32;
            let mut nw = WINS[idx].w;
            let mut nh = WINS[idx].h;
            if edges & 1 != 0 {
                nw = mx - WINS[idx].x;
            }
            if edges & 2 != 0 {
                nh = my - WINS[idx].y;
            }
            if nw < 200 { nw = 200; }
            if nh < 150 { nh = 150; }
            if WINS[idx].x + nw > sw { nw = (sw - WINS[idx].x).max(200); }
            if WINS[idx].y + nh > sh - tb { nh = (sh - tb - WINS[idx].y).max(150); }
            WINS[idx].w = nw;
            WINS[idx].h = nh;
            WINS[idx].rw = nw;
            WINS[idx].rh = nh;
            DIRTY_UI = true;
        }
        PREV_MB = buttons;
        MB = buttons;
    }
}

fn draw_cursor(x: i32, y: i32) {
    let x = if x < 0 { 0usize } else { x as usize };
    let y = if y < 0 { 0usize } else { y as usize };
    let col = unsafe { CURSOR_COLOR };
    graphics::fill_rect(x, y, 2, 16, col);
    graphics::fill_rect(x, y, 12, 2, col);
    graphics::fill_rect(x + 2, y + 4, 8, 2, col);
    graphics::fill_rect(x + 2, y + 8, 6, 2, col);
    graphics::put_pixel(x + 3, y + 12, col);
    graphics::put_pixel(x + 4, y + 13, col);
}

fn draw_window(idx: usize) {
    unsafe {
        if !WINS[idx].visible {
            return;
        }
        let w = &WINS[idx];
        let focused = FOCUS == idx;
        let title_bg = if focused { COL_TITLE_ACT } else { COL_TITLE_INACT };
        let border = if focused { COL_FOCUS } else { COL_INACTIVE };
        let wx = w.x as usize;
        let wy = w.y as usize;
        let ww = w.w as usize;
        let wh = w.h as usize;

        let is_explorer = matches!(w.kind, WinKind::Files | WinKind::MyComputer);
        // outer frame + soft drop shadow
        graphics::fill_rect(wx + 4, wy + 4, ww, wh, 0x00505050);
        graphics::fill_rect(wx, wy, ww, wh, if is_explorer { 0x00F0F4F8 } else { COL_CLIENT });
        if is_explorer {
            graphics::border_rect(wx, wy, ww, wh, if focused { 0x003A6EA5 } else { 0x008090A0 });
            graphics::border_rect(wx + 1, wy + 1, ww - 2, wh - 2, 0x00E8F0F8);
        } else {
            graphics::border_rect(wx, wy, ww, wh, border);
            graphics::border_rect(wx + 1, wy + 1, ww - 2, wh - 2, 0x00FFFFFF);
        }
        // Title bar: Aero glass for Explorer, classic for other windows
        if is_explorer {
            let th = TITLE_H as usize;
            let mut i = 0usize;
            while i < th.saturating_sub(2) {
                // light glass gradient (Win7 Aero approx)
                let t = if focused {
                    let r = 160u32 + (i as u32 * 20 / th.max(1) as u32);
                    let g = 190u32 + (i as u32 * 10 / th.max(1) as u32);
                    let b = 230u32.saturating_sub(i as u32 * 30 / th.max(1) as u32);
                    (r << 16) | (g << 8) | b
                } else {
                    let g = 180u32 + (i as u32 * 20 / th.max(1) as u32);
                    (g << 16) | (g << 8) | g
                };
                graphics::fill_rect(wx + 2, wy + 2 + i, ww - 4, 1, t);
                i += 1;
            }
            graphics::fill_rect(wx + 2, wy + th - 1, ww - 4, 1, 0x00A0B8D0);
        } else {
            graphics::fill_rect(wx + 2, wy + 2, ww - 4, TITLE_H as usize - 2, title_bg);
            if focused {
                graphics::fill_rect(wx + 2, wy + 2, ww - 4, 3, 0x00166ACB);
                graphics::fill_rect(wx + 2, wy + TITLE_H as usize - 5, ww - 4, 2, 0x00082A5A);
            }
        }
        // 16x16 window icon in title bar
        {
            use crate::gui::icon::{self, IconId};
            let iid = match w.kind {
                WinKind::Terminal => IconId::Terminal,
                WinKind::MyComputer | WinKind::SysProps => IconId::MyComputer,
                WinKind::Settings => IconId::Settings,
                WinKind::Files => IconId::Folder,
                WinKind::Network => IconId::Network,
                WinKind::Sound | WinKind::Video | WinKind::DateTime | WinKind::Alarm | WinKind::Movie | WinKind::Browser => IconId::Settings,
                _ => IconId::File,
            };
            let img = icon::generate(iid);
            let mut yy = 0usize;
            while yy < 16 {
                let mut xx = 0usize;
                while xx < 16 {
                    let c = img[(yy * 2) * 32 + xx * 2];
                    if (c >> 24) > 0 {
                        graphics::put_pixel(wx + 6 + xx, wy + 5 + yy, c & 0x00FFFFFF);
                    }
                    xx += 1;
                }
                yy += 1;
            }
        }
        // title text after icon
        let title_col = if is_explorer { 0x00102848 } else { COL_TITLE_TEXT };
        graphics::draw_str(wx + 26, wy + 9, win_title(w.kind), title_col);
        // caption buttons
        let cy = wy + 5;
        let close_x = wx + ww - 28;
        let max_x = wx + ww - 50;
        let min_x = wx + ww - 72;
        if is_explorer {
            // glass min / max
            graphics::fill_rect(min_x, cy, 20, 16, 0x00D8E8F8);
            graphics::border_rect(min_x, cy, 20, 16, 0x007090B0);
            graphics::fill_rect(min_x + 5, cy + 11, 10, 2, 0x00204060);
            graphics::fill_rect(max_x, cy, 20, 16, 0x00D8E8F8);
            graphics::border_rect(max_x, cy, 20, 16, 0x007090B0);
            graphics::border_rect(max_x + 5, cy + 3, 10, 9, 0x00204060);
            // red Aero close
            graphics::fill_rect(close_x, cy, 22, 16, 0x00E04343);
            graphics::fill_rect(close_x + 1, cy + 1, 20, 4, 0x00F07070);
            graphics::border_rect(close_x, cy, 22, 16, 0x00902020);
            graphics::draw_str(close_x + 7, cy + 4, "X", 0x00FFFFFF);
        } else {
            graphics::fill_rect(min_x, cy, 16, 14, COL_BTN_FACE);
            graphics::border_rect(min_x, cy, 16, 14, 0x00404040);
            graphics::fill_rect(min_x + 3, cy + 10, 10, 2, 0x00000000);
            graphics::fill_rect(max_x, cy, 16, 14, COL_BTN_FACE);
            graphics::border_rect(max_x, cy, 16, 14, 0x00404040);
            graphics::border_rect(max_x + 3, cy + 3, 10, 8, 0x00000000);
            graphics::fill_rect(close_x, cy, 16, 14, COL_CLOSE);
            graphics::border_rect(close_x, cy, 16, 14, 0x00800000);
            graphics::draw_str(close_x + 4, cy + 3, "X", COL_TITLE_TEXT);
        }
        if !w.maximized {
            graphics::fill_rect(wx + ww - 12, wy + wh - 12, 8, 2, 0x00808080);
            graphics::fill_rect(wx + ww - 12, wy + wh - 8, 8, 2, 0x00808080);
        }

        match w.kind {
            WinKind::Terminal => {
                if !TERM_INIT {
                    term_clear();
                    terminal_write("Aether Terminal  v2
");
                    terminal_write("Type help | hh | ww | clear
");
                }
                let client_x = wx + 3;
                let client_y = wy + TITLE_H as usize;
                let client_w = ww.saturating_sub(6);
                let client_h = wh.saturating_sub(TITLE_H as usize + 3);
                graphics::fill_rect(client_x, client_y, client_w, client_h, 0x000C0C0C);
                let focused = FOCUS == idx;
                let view_rows = term_visible_rows_for(wh as i32);
                let cols = term_text_cols_for(ww as i32);
                let prompt_h = 20usize;
                let text_bottom = wy + wh.saturating_sub(prompt_h + 2);
                let text_top = client_y + 4;
                let line_h = 12usize;
                // clamp view
                let max_view = if TERM_COUNT > view_rows { TERM_COUNT - view_rows } else { 0 };
                if TERM_VIEW > max_view {
                    TERM_VIEW = max_view;
                }
                // first logical line to show
                let start_log = if TERM_COUNT > view_rows {
                    TERM_COUNT - view_rows - TERM_VIEW
                } else {
                    0
                };
                let mut row = 0usize;
                while row < view_rows {
                    let logical = start_log + row;
                    if logical >= TERM_COUNT {
                        break;
                    }
                    let slot = term_slot(logical);
                    let y = text_top + row * line_h;
                    if y + line_h > text_bottom {
                        break;
                    }
                    // dim older lines slightly when scrolled
                    let color = if TERM_VIEW > 0 && logical + view_rows < TERM_COUNT {
                        0x00888888
                    } else {
                        COL_TERM_FG
                    };
                    let n = TERM_LEN[slot].min(cols);
                    let mut k = 0usize;
                    while k < n {
                        let ch = TERM_LINES[slot][k];
                        if ch != 0 {
                            graphics::draw_char(client_x + 6 + k * 8, y, ch, color);
                        }
                        k += 1;
                    }
                    row += 1;
                }
                // scrollbar
                let bar_x = wx + ww.saturating_sub(16);
                let bar_top = text_top;
                let bar_h = text_bottom.saturating_sub(text_top).max(20);
                graphics::fill_rect(bar_x, bar_top, 12, bar_h, 0x00181818);
                graphics::fill_rect(bar_x, bar_top, 12, 14, 0x00282828);
                graphics::draw_str(bar_x + 2, bar_top + 3, "^", 0x00AAAAAA);
                graphics::fill_rect(bar_x, bar_top + bar_h.saturating_sub(14), 12, 14, 0x00282828);
                graphics::draw_str(bar_x + 2, bar_top + bar_h.saturating_sub(11), "v", 0x00AAAAAA);
                if max_view > 0 {
                    let track = bar_h.saturating_sub(28).max(1);
                    let thumb_h = (track * view_rows / TERM_COUNT.max(1)).max(8).min(track);
                    let thumb_y = if max_view == 0 {
                        0
                    } else {
                        track * (max_view - TERM_VIEW) / max_view
                    };
                    graphics::fill_rect(bar_x + 1, bar_top + 14 + thumb_y, 10, thumb_h, 0x00316AC5);
                }
                // prompt bar
                let prompt_y = text_bottom + 2;
                graphics::fill_rect(client_x, text_bottom, client_w.saturating_sub(14), prompt_h, 0x00101010);
                graphics::fill_rect(client_x, text_bottom, client_w.saturating_sub(14), 1, 0x00304030);
                let prompt = b"Aether> ";
                let mut pi = 0usize;
                while pi < prompt.len() {
                    graphics::draw_char(client_x + 6 + pi * 8, prompt_y, prompt[pi], 0x0000E070);
                    pi += 1;
                }
                let px0 = client_x + 6 + prompt.len() * 8;
                // input with cursor
                let mut k = 0usize;
                while k < INPUT_LEN && k < cols.saturating_sub(prompt.len()) {
                    graphics::draw_char(px0 + k * 8, prompt_y, INPUT[k], 0x00F0F0F0);
                    k += 1;
                }
                // caret: blink when focused
                if focused {
                    let blink = (FRAME_N / 20) % 2 == 0;
                    if blink {
                        let cx = px0 + INPUT_CURSOR.min(INPUT_LEN) * 8;
                        graphics::fill_rect(cx, prompt_y, 7, 10, 0x0000E070);
                        if INPUT_CURSOR < INPUT_LEN {
                            graphics::draw_char(cx, prompt_y, INPUT[INPUT_CURSOR], 0x00000000);
                        }
                    }
                }
                // status: scroll indicator
                if TERM_VIEW > 0 {
                    graphics::draw_str(wx + ww.saturating_sub(90), prompt_y, "[scroll]", 0x00666666);
                }
            }
                        WinKind::About => {
                if RECYCLE_MODE {
                    graphics::draw_str(wx + 16, wy + 40, "Recycle Bin", COL_TEXT);
                    graphics::draw_str(wx + 16, wy + 64, "The Recycle Bin is empty.", COL_TEXT_DIM);
                    graphics::draw_str(wx + 16, wy + 88, "Deleted AetherFS items are removed", COL_TEXT_DIM);
                    graphics::draw_str(wx + 16, wy + 104, "immediately (no soft-delete yet).", COL_TEXT_DIM);
                    graphics::draw_str(wx + 16, wy + 136, "NTFS/FAT volumes stay read-mostly.", COL_TEXT_DIM);
                    graphics::draw_str(wx + 16, wy + 160, "Close this window to continue.", COL_TEXT_DIM);
                } else {
                    graphics::draw_str(wx + 16, wy + 40, "Aether OS", COL_TEXT);
                    graphics::draw_str(wx + 16, wy + 60, "Polymorphic kernel (Rust no_std)", COL_TEXT_DIM);
                    graphics::draw_str(wx + 16, wy + 84, "Type sys in Terminal for status.", COL_TEXT_DIM);
                }
            }

            WinKind::Network => {
                // Full Network Center: Ethernet + Wi-Fi
                graphics::fill_rect(wx + 3, wy + TITLE_H as usize, ww - 6, wh - TITLE_H as usize - 3, 0x00F0F4F8);
                // Header band
                graphics::fill_rect(wx + 3, wy + TITLE_H as usize, ww - 6, 36, 0x00E3EBF5);
                graphics::draw_str(wx + 16, wy + TITLE_H as usize + 12, "Network and Sharing Center", 0x001F4E79);

                // --- Left: Ethernet ---
                let px = wx + 12;
                let py = wy + TITLE_H as usize + 48;
                graphics::fill_rect(px, py, 200, 210, 0x00FFFFFF);
                graphics::border_rect(px, py, 200, 210, 0x00B0B8C0);
                graphics::fill_rect(px, py, 200, 28, 0x00DCE6F0);
                graphics::draw_str(px + 10, py + 8, "Ethernet", 0x001F4E79);
                if crate::drivers::net::eth_found() {
                    graphics::draw_str(px + 10, py + 40, "Realtek RTL8168", 0x00222222);
                    if crate::drivers::net::link_up() {
                        graphics::draw_str(px + 10, py + 58, "Status: Link up", 0x00008000);
                    } else {
                        graphics::draw_str(px + 10, py + 58, "Status: No link", 0x00A04000);
                    }
                    if crate::drivers::net::dhcp_ok() {
                        let mut ip = [0u8; 4];
                        crate::drivers::net::dhcp_ip(&mut ip);
                        graphics::draw_str(px + 10, py + 80, "DHCP: OK", 0x00008000);
                        // IP as text
                        let mut buf = [0u8; 20];
                        let mut n = 0usize;
                        let put = |b: &mut [u8], n: &mut usize, v: u8| {
                            if v >= 100 { b[*n] = b'0' + v / 100; *n += 1; }
                            if v >= 10 { b[*n] = b'0' + (v / 10) % 10; *n += 1; }
                            b[*n] = b'0' + v % 10; *n += 1;
                        };
                        put(&mut buf, &mut n, ip[0]); buf[n] = b'.'; n += 1;
                        put(&mut buf, &mut n, ip[1]); buf[n] = b'.'; n += 1;
                        put(&mut buf, &mut n, ip[2]); buf[n] = b'.'; n += 1;
                        put(&mut buf, &mut n, ip[3]);
                        let s = unsafe { core::str::from_utf8_unchecked(&buf[..n]) };
                        graphics::draw_str(px + 10, py + 98, s, 0x001F4E79);
                    } else {
                        graphics::draw_str(px + 10, py + 80, "DHCP: not leased", 0x00606A73);
                    }
                    {
                        let mut mac = [0u8; 6];
                        crate::drivers::net::eth_mac(&mut mac);
                        // "MAC aa:bb:cc:dd:ee:ff"
                        let mut line = [0u8; 24];
                        let pref = b"MAC ";
                        let mut n = 0usize;
                        while n < 4 { line[n] = pref[n]; n += 1; }
                        let hex = b"0123456789abcdef";
                        for i in 0..6 {
                            if i > 0 { line[n] = b':'; n += 1; }
                            line[n] = hex[(mac[i] >> 4) as usize]; n += 1;
                            line[n] = hex[(mac[i] & 0xf) as usize]; n += 1;
                        }
                        let s = unsafe { core::str::from_utf8_unchecked(&line[..n]) };
                        graphics::draw_str(px + 10, py + 120, s, 0x001F4E79);
                    }
                } else {
                    graphics::draw_str(px + 10, py + 40, "No Ethernet NIC", 0x00A04040);
                }
                // DHCP button
                graphics::fill_rect(px + 10, py + 170, 100, 24, 0x00F3F3F3);
                graphics::border_rect(px + 10, py + 170, 100, 24, 0x008A8A8A);
                graphics::draw_str(px + 28, py + 176, "Get IP", 0x00222222);

                // --- Right: Wi-Fi list ---
                let lx = wx + 220;
                let ly = wy + TITLE_H as usize + 48;
                let lw = ww.saturating_sub(232);
                let lh = wh.saturating_sub(TITLE_H as usize + 100);
                graphics::fill_rect(lx, ly, lw, lh, 0x00FFFFFF);
                graphics::border_rect(lx, ly, lw, lh, 0x00B0B8C0);
                graphics::fill_rect(lx, ly, lw, 28, 0x00DCE6F0);
                graphics::draw_str(lx + 10, ly + 8, "Wireless networks", 0x001F4E79);

                if crate::drivers::wifi::found() {
                    graphics::draw_str(lx + 160, ly + 8, "Intel N-2230", 0x00606A73);
                } else {
                    graphics::draw_str(lx + 160, ly + 8, "No Wi-Fi adapter", 0x00A04040);
                }
                // Wi-Fi MAC under header
                {
                    let mut mac = [0u8; 6];
                    crate::drivers::wifi::wifi_mac(&mut mac);
                    let mut line = [0u8; 28];
                    let pref = b"WiFi MAC ";
                    let mut n = 0usize;
                    while n < 9 { line[n] = pref[n]; n += 1; }
                    let hex = b"0123456789abcdef";
                    if crate::drivers::wifi::wifi_mac_ok() {
                        for i in 0..6 {
                            if i > 0 { line[n] = b':'; n += 1; }
                            line[n] = hex[(mac[i] >> 4) as usize]; n += 1;
                            line[n] = hex[(mac[i] & 0xf) as usize]; n += 1;
                        }
                    } else {
                        let unk = b"(after ALIVE)";
                        let mut i = 0usize;
                        while i < unk.len() { line[n] = unk[i]; n += 1; i += 1; }
                    }
                    let s = unsafe { core::str::from_utf8_unchecked(&line[..n]) };
                    graphics::draw_str(lx + 10, ly + 32, s, 0x00444F5C);
                }

                let list_top = ly + 52;
                if unsafe { WIFI_UI_STATUS == 1 } {
                    graphics::draw_str(lx + 16, list_top + 12, "Searching for wireless networks...", 0x001E5AA8);
                } else {
                    let n = crate::drivers::wifi::ap_count();
                    if n == 0 {
                        if !crate::drivers::wifi::found() {
                            graphics::draw_str(lx + 16, list_top + 12, "Wi-Fi hardware not found", 0x00A04040);
                        } else if !crate::drivers::wifi::alive_ok() {
                            graphics::draw_str(lx + 16, list_top + 8, "INIT ALIVE FAIL - scan blocked", 0x00C00000);
                            let step = crate::drivers::wifi::bring_step();
                            let fail = crate::drivers::wifi::bring_fail();
                            let msg = match (step, fail) {
                                (_, 1) => "Fail: software reset",
                                (_, 2) => "Fail: activate MAC clock",
                                (_, 3) => "Fail: RX queue",
                                (_, 4) => "Fail: parse/load ucode",
                                (_, 5) => "Fail: INIT ucode ALIVE",
                                (_, 6) => "Fail: RUNTIME ucode ALIVE",
                                (1, _) => "Step: reset...",
                                (2, _) => "Step: activate...",
                                (3, _) => "Step: RX queue...",
                                (4, _) => "Step: FW parse...",
                                (5, _) => "Step: INIT ALIVE wait...",
                                (7, _) => "Bring-up done but ALIVE clear?",
                                _ => "Press Scan/Up once to bring-up",
                            };
                            graphics::draw_str(lx + 16, list_top + 28, msg, 0x00A04000);
                            // RF-kill highly visible
                            if crate::drivers::wifi::rfkill_blocked() {
                                graphics::draw_str(lx + 16, list_top + 48, "RF-KILL ACTIVE - enable WiFi switch/Fn", 0x00C00000);
                            } else {
                                graphics::draw_str(lx + 16, list_top + 48, "RF-KILL: clear (radio enabled)", 0x00008000);
                            }
                            if crate::drivers::wifi::fw_loaded_ok() {
                                graphics::draw_str(lx + 16, list_top + 68, "ucode parsed OK | MAC clock OK", 0x00008000);
                            }
                            // Register log: final + first-seen INT/FH (not wiped by ack)
                            {
                                fn put_hex(v: u32, buf: &mut [u8], n: &mut usize) {
                                    let hex = b"0123456789abcdef";
                                    let mut sh = 28i32;
                                    while sh >= 0 {
                                        let nib = ((v >> sh) & 0xF) as usize;
                                        buf[*n] = hex[nib];
                                        *n += 1;
                                        sh -= 4;
                                    }
                                }
                                let mut line = [0u8; 72];
                                let mut n = 0usize;
                                for &c in b"1stINT=" { line[n] = c; n += 1; }
                                put_hex(crate::drivers::wifi::diag_first_int(), &mut line, &mut n);
                                for &c in b" 1stFH=" { line[n] = c; n += 1; }
                                put_hex(crate::drivers::wifi::diag_first_fh(), &mut line, &mut n);
                                let s = unsafe { core::str::from_utf8_unchecked(&line[..n]) };
                                graphics::draw_str(lx + 16, list_top + 88, s, 0x00A00000);
                                n = 0;
                                for &c in b"RBD=" { line[n] = c; n += 1; }
                                put_hex(crate::drivers::wifi::diag_rbd_base(), &mut line, &mut n);
                                for &c in b" WP=" { line[n] = c; n += 1; }
                                put_hex(crate::drivers::wifi::diag_rb_wptr(), &mut line, &mut n);
                                for &c in b" RD=" { line[n] = c; n += 1; }
                                put_hex(crate::drivers::wifi::diag_rb_rdptr(), &mut line, &mut n);
                                let s2 = unsafe { core::str::from_utf8_unchecked(&line[..n]) };
                                graphics::draw_str(lx + 16, list_top + 108, s2, 0x00444F5C);
                                n = 0;
                                for &c in b"INITrb " { line[n] = c; n += 1; }
                                put_hex(crate::drivers::wifi::diag_init_rb0(), &mut line, &mut n);
                                line[n] = b'/'; n += 1;
                                put_hex(crate::drivers::wifi::diag_init_rb1(), &mut line, &mut n);
                                for &c in b" len=" { line[n] = c; n += 1; }
                                put_hex(crate::drivers::wifi::diag_sec_init_len(), &mut line, &mut n);
                                let s3 = unsafe { core::str::from_utf8_unchecked(&line[..n]) };
                                graphics::draw_str(lx + 16, list_top + 128, s3, 0x00444F5C);
                            }
                            {
                                let fi = crate::drivers::wifi::diag_first_int();
                                let msg = if crate::drivers::wifi::alive_ok() {
                                    "Log: ALIVE OK - ready to scan"
                                } else if fi == 0 {
                                    "Log: 1stINT=0 => FW never raised IRQ"
                                } else if (fi & 1) != 0 {
                                    "Log: ALIVE bit set - parsing RX buffers"
                                } else {
                                    "Log: IRQ seen but ALIVE cmd not parsed"
                                };
                                graphics::draw_str(lx + 16, list_top + 148, msg, 0x00606A73);
                            }
                        } else {
                            // After search: explicit empty / fail + logs
                            let sc = crate::drivers::wifi::last_scan_code();
                            if sc == 0 {
                                graphics::draw_str(lx + 16, list_top + 12, "No scan yet. Press Scan/Up.", 0x00606A73);
                            } else {
                                graphics::draw_str(lx + 16, list_top + 12, "Networks not found", 0x00A04040);
                                graphics::draw_str(lx + 16, list_top + 34, "Scan did not pass", 0x00A06000);
                                let reason = match sc {
                                    2 => "Log: no ALIVE - scan skipped",
                                    3 => "Log: SCAN cmd submit failed",
                                    4 => "Log: no SCAN_COMPLETE from FW",
                                    5 => "Log: SCAN_COMPLETE but 0 AP",
                                    1 => "Log: OK but list empty?",
                                    _ => "Log: unknown scan result",
                                };
                                graphics::draw_str(lx + 16, list_top + 56, reason, 0x00444F5C);
                                // Extra counters
                                let mut line = [0u8; 48];
                                let pfx = b"start/res/done=";
                                let mut n = 0usize;
                                for &c in pfx { line[n] = c; n += 1; }
                                let put_u = |v: u32, buf: &mut [u8], n: &mut usize| {
                                    let mut x = v;
                                    let mut tmp = [0u8; 10];
                                    let mut k = 0usize;
                                    if x == 0 { tmp[0] = b'0'; k = 1; }
                                    else {
                                        while x > 0 {
                                            tmp[k] = b'0' + (x % 10) as u8;
                                            x /= 10;
                                            k += 1;
                                        }
                                    }
                                    while k > 0 {
                                        k -= 1;
                                        buf[*n] = tmp[k];
                                        *n += 1;
                                    }
                                };
                                put_u(crate::drivers::wifi::scan_notif_start(), &mut line, &mut n);
                                line[n] = b'/'; n += 1;
                                put_u(crate::drivers::wifi::scan_notif_results(), &mut line, &mut n);
                                line[n] = b'/'; n += 1;
                                put_u(crate::drivers::wifi::scan_notif_complete(), &mut line, &mut n);
                                let s = unsafe { core::str::from_utf8_unchecked(&line[..n]) };
                                graphics::draw_str(lx + 16, list_top + 78, s, 0x00606A73);
                            }
                        }
                    } else {
                        let max_rows = ((lh.saturating_sub(40)) / 22).min(n).min(12);
                        let mut row = 0usize;
                        while row < max_rows {
                            let y = list_top + row * 22;
                            let sel = unsafe { WIFI_UI_SELECTED == row as i32 };
                            if sel {
                                graphics::fill_rect(lx + 2, y, lw.saturating_sub(4), 22, 0x00CDE4F7);
                            }
                            // signal bars from RSSI
                            let rssi = crate::drivers::wifi::ap_rssi(row);
                            let bars = if rssi > -50 { 4 } else if rssi > -60 { 3 } else if rssi > -70 { 2 } else if rssi > -80 { 1 } else { 0 };
                            let mut b = 0usize;
                            while b < 4 {
                                let h = 4 + b * 3;
                                let col = if b < bars { 0x00228B22 } else { 0x00C0C0C0 };
                                graphics::fill_rect(lx + 8 + b * 5, y + 18 - h, 4, h, col);
                                b += 1;
                            }
                            // lock icon if secured
                            if crate::drivers::wifi::ap_secured(row) {
                                graphics::draw_str(lx + 32, y + 5, "L", 0x00808000);
                            }
                            let mut ssid = [0u8; 32];
                            let sl = crate::drivers::wifi::ap_ssid_str(row, &mut ssid);
                            if sl > 0 {
                                let s = unsafe { core::str::from_utf8_unchecked(&ssid[..sl]) };
                                graphics::draw_str(lx + 48, y + 5, s, 0x001F4E79);
                            } else {
                                graphics::draw_str(lx + 48, y + 5, "(hidden)", 0x00606A73);
                            }
                            // channel
                            let ch = crate::drivers::wifi::ap_channel(row);
                            if ch > 0 {
                                let mut cb = [b'c', b'h', 0, 0];
                                cb[2] = b'0' + (ch / 10);
                                cb[3] = b'0' + (ch % 10);
                                let cs = unsafe { core::str::from_utf8_unchecked(&cb) };
                                graphics::draw_str(lx + lw.saturating_sub(50), y + 5, cs, 0x00808080);
                            }
                            row += 1;
                        }
                    }
                }

                // Buttons
                let by = ly + lh.saturating_sub(36);
                graphics::fill_rect(lx + 8, by, 88, 26, 0x00F3F3F3);
                graphics::border_rect(lx + 8, by, 88, 26, 0x008A8A8A);
                graphics::draw_str(lx + 18, by + 8, "Scan/Up", 0x00222222);
                graphics::fill_rect(lx + 104, by, 96, 26, 0x00F3F3F3);
                graphics::border_rect(lx + 104, by, 96, 26, 0x008A8A8A);
                graphics::draw_str(lx + 124, by + 8, "Connect", 0x00222222);
                graphics::fill_rect(lx + 208, by, 100, 26, 0x00F3F3F3);
                graphics::border_rect(lx + 208, by, 100, 26, 0x008A8A8A);
                graphics::draw_str(lx + 222, by + 8, "Disconnect", 0x00222222);

                // Status bar
                let sy = wy + wh - 34;
                graphics::fill_rect(wx + 3, sy, ww - 6, 30, 0x00E8EDF3);
                graphics::fill_rect(wx + 3, sy, ww - 6, 1, 0x00C0CBD7);
                if crate::drivers::wifi::is_connected() {
                    graphics::draw_str(wx + 16, sy + 10, "Wi-Fi: Connected", 0x00008000);
                } else if unsafe { WIFI_UI_STATUS == 1 } {
                    graphics::draw_str(wx + 16, sy + 10, "Scanning for networks...", 0x001E5AA8);
                } else if crate::drivers::wifi::ap_count() == 0 && crate::drivers::wifi::last_scan_code() != 0 {
                    graphics::draw_str(wx + 16, sy + 10, "Scan finished: no networks", 0x00A06000);
                } else if crate::drivers::wifi::alive_ok() {
                    graphics::draw_str(wx + 16, sy + 10, "Wi-Fi adapter ready", 0x00222222);
                } else if crate::drivers::wifi::found() {
                    graphics::draw_str(wx + 16, sy + 10, "Wi-Fi found - firmware/ALIVE pending", 0x00A06000);
                } else {
                    graphics::draw_str(wx + 16, sy + 10, "No wireless adapter", 0x00A04040);
                }
                if crate::drivers::net::dhcp_ok() {
                    graphics::draw_str(wx + ww.saturating_sub(200), sy + 10, "Ethernet: IP OK", 0x00008000);
                }

                // Password / Connect dialog overlay
                if unsafe { WIFI_UI_CONNECT_DIALOG } {
                    let dx = wx + ww / 2 - 160;
                    let dy = wy + wh / 2 - 70;
                    graphics::fill_rect(dx + 3, dy + 3, 320, 140, 0x00404040);
                    graphics::fill_rect(dx, dy, 320, 140, 0x00FFFFFF);
                    graphics::border_rect(dx, dy, 320, 140, 0x001F4E79);
                    graphics::draw_str(dx + 16, dy + 14, "Connect to network", 0x001F4E79);
                    graphics::draw_str(dx + 16, dy + 40, "Security key (WPA):", 0x00444F5C);
                    graphics::fill_rect(dx + 16, dy + 58, 288, 24, 0x00FFFFFF);
                    graphics::border_rect(dx + 16, dy + 58, 288, 24, 0x008A8A8A);
                    // password bullets
                    let plen = unsafe { WIFI_UI_PASSWORD_LEN };
                    let mut k = 0usize;
                    while k < plen && k < 32 {
                        graphics::draw_str(dx + 22 + k * 8, dy + 64, "*", 0x00222222);
                        k += 1;
                    }
                    graphics::fill_rect(dx + 40, dy + 100, 100, 26, 0x00E8F0FF);
                    graphics::border_rect(dx + 40, dy + 100, 100, 26, 0x001F4E79);
                    graphics::draw_str(dx + 62, dy + 108, "Connect", 0x001F4E79);
                    graphics::fill_rect(dx + 160, dy + 100, 100, 26, 0x00F3F3F3);
                    graphics::border_rect(dx + 160, dy + 100, 100, 26, 0x008A8A8A);
                    graphics::draw_str(dx + 186, dy + 108, "Cancel", 0x00222222);
                }
            }
            WinKind::Sound => {
                graphics::fill_rect(wx + 3, wy + TITLE_H as usize, ww - 6, wh - TITLE_H as usize - 3, COL_CLIENT);
                // Menu bar
                let menu_y = wy + TITLE_H as usize;
                graphics::fill_rect(wx + 3, menu_y, ww - 6, 22, 0x00ECE9D8);
                graphics::fill_rect(wx + 3, menu_y + 21, ww - 6, 1, 0x00A0A0A0);
                let file_hi = unsafe { MEDIA_MENU == 1 };
                let play_hi = unsafe { MEDIA_MENU == 2 };
                graphics::fill_rect(wx + 8, menu_y + 2, 40, 18, if file_hi { 0x00316AC5 } else { 0x00ECE9D8 });
                graphics::draw_str(wx + 12, menu_y + 5, "File", if file_hi { 0x00FFFFFF } else { COL_TEXT });
                graphics::fill_rect(wx + 52, menu_y + 2, 64, 18, if play_hi { 0x00316AC5 } else { 0x00ECE9D8 });
                graphics::draw_str(wx + 56, menu_y + 5, "Playback", if play_hi { 0x00FFFFFF } else { COL_TEXT });

                // Current file / title
                graphics::draw_str(wx + 12, menu_y + 28, "Now:", COL_TEXT_DIM);
                let mut tbuf = [0u8; 96];
                let tn = crate::media_player::title(&mut tbuf);
                if tn > 0 {
                    let mut i = 0usize;
                    while i < tn && i < 48 {
                        graphics::draw_char(wx + 48 + i * 8, menu_y + 28, tbuf[i], COL_TEXT);
                        i += 1;
                    }
                } else {
                    graphics::draw_str(wx + 48, menu_y + 28, "(no file)", COL_TEXT_DIM);
                }

                graphics::draw_str(wx + 12, menu_y + 46, "Library / open from Explorer:", COL_TEXT_DIM);
                // Playlist
                let n = crate::media_player::builtin_count();
                let sel = crate::media_player::selected_builtin();
                let mut i = 0usize;
                while i < n {
                    let y = menu_y + 66 + i * 22;
                    let row_bg = if i == sel { 0x00316AC5 } else { COL_CLIENT };
                    let fg = if i == sel { 0x00FFFFFF } else { COL_TEXT };
                    graphics::fill_rect(wx + 10, y, ww - 24, 20, row_bg);
                    graphics::draw_str(wx + 16, y + 4, crate::media_player::builtin_name(i), fg);
                    graphics::draw_str(wx + ww - 90, y + 4, crate::media_player::builtin_kind(i), if i == sel { 0x00D0E8FF } else { COL_TEXT_DIM });
                    i += 1;
                }
                // Open File button (always visible)
                let oby = menu_y + 66 + n * 22 + 8;
                graphics::fill_rect(wx + 12, oby, 120, 24, 0x00D4E8FF);
                graphics::draw_str(wx + 20, oby + 6, "Open File...", 0x00003366);
                graphics::fill_rect(wx + 140, oby, 100, 24, COL_BTN_FACE);
                graphics::draw_str(wx + 148, oby + 6, "Browse...", COL_TEXT);

                // Transport buttons
                let by = wy + wh - 88;
                graphics::fill_rect(wx + 12, by, 56, 24, COL_BTN_FACE);
                graphics::draw_str(wx + 22, by + 6, "Play", COL_TEXT);
                graphics::fill_rect(wx + 76, by, 56, 24, COL_BTN_FACE);
                graphics::draw_str(wx + 82, by + 6, "Pause", COL_TEXT);
                graphics::fill_rect(wx + 140, by, 56, 24, COL_BTN_FACE);
                graphics::draw_str(wx + 150, by + 6, "Stop", COL_TEXT);
                graphics::fill_rect(wx + 210, by, 40, 24, COL_BTN_FACE);
                graphics::draw_str(wx + 220, by + 6, "Vol-", COL_TEXT);
                graphics::fill_rect(wx + 256, by, 40, 24, COL_BTN_FACE);
                graphics::draw_str(wx + 266, by + 6, "Vol+", COL_TEXT);
                graphics::fill_rect(wx + 304, by, 48, 24, COL_BTN_FACE);
                graphics::draw_str(wx + 312, by + 6, "Mute", COL_TEXT);

                // Status line
                let sy = wy + wh - 52;
                let st = match crate::media_player::state() {
                    crate::media_player::State::Playing => "Playing",
                    crate::media_player::State::Paused => "Paused",
                    crate::media_player::State::Stopped => "Stopped",
                    crate::media_player::State::Error => "Error",
                    crate::media_player::State::Empty => "Empty",
                };
                graphics::draw_str(wx + 12, sy, st, 0x00008000);
                graphics::draw_str(wx + 90, sy, "Vol:", COL_TEXT_DIM);
                draw_u32(wx + 120, sy, crate::media_player::volume() as u32, COL_TEXT);
                if crate::media_player::muted() {
                    graphics::draw_str(wx + 160, sy, "MUTE", 0x00800000);
                }
                if unsafe { MEDIA_PICK } {
                    graphics::draw_str(wx + 210, sy, "Pick file in Explorer", 0x00003399);
                }
                let hy = wy + wh - 34;
                if crate::drivers::audio::hda_stream_running() {
                    graphics::draw_str(wx + 12, hy, "HDA DMA running", 0x00008000);
                    graphics::draw_str(wx + 140, hy, "LPIB:", COL_TEXT_DIM);
                    draw_u32(wx + 180, hy, crate::drivers::audio::hda_lpib(), COL_TEXT);
                } else if crate::drivers::audio::hda_found() {
                    graphics::draw_str(wx + 12, hy, "HDA ready - Play or Open File", COL_TEXT_DIM);
                } else {
                    graphics::draw_str(wx + 12, hy, "HDA not found", 0x00800000);
                }

                // File dropdown
                if unsafe { MEDIA_MENU == 1 } {
                    let dx = wx + 8;
                    let dy = menu_y + 22;
                    graphics::fill_rect(dx, dy, 160, 72, 0x00FFFFFF);
                    graphics::fill_rect(dx, dy, 160, 1, 0x00000000);
                    graphics::fill_rect(dx, dy + 71, 160, 1, 0x00000000);
                    graphics::fill_rect(dx, dy, 1, 72, 0x00000000);
                    graphics::fill_rect(dx + 159, dy, 1, 72, 0x00000000);
                    graphics::draw_str(dx + 10, dy + 8, "Open File...", COL_TEXT);
                    graphics::draw_str(dx + 10, dy + 28, "Open Explorer", COL_TEXT);
                    graphics::draw_str(dx + 10, dy + 48, "Close menu", COL_TEXT_DIM);
                }
                if unsafe { MEDIA_MENU == 2 } {
                    let dx = wx + 52;
                    let dy = menu_y + 22;
                    graphics::fill_rect(dx, dy, 120, 92, 0x00FFFFFF);
                    graphics::fill_rect(dx, dy, 120, 1, 0x00000000);
                    graphics::fill_rect(dx, dy + 91, 120, 1, 0x00000000);
                    graphics::fill_rect(dx, dy, 1, 92, 0x00000000);
                    graphics::fill_rect(dx + 119, dy, 1, 92, 0x00000000);
                    graphics::draw_str(dx + 10, dy + 8, "Play", COL_TEXT);
                    graphics::draw_str(dx + 10, dy + 28, "Pause", COL_TEXT);
                    graphics::draw_str(dx + 10, dy + 48, "Stop", COL_TEXT);
                    graphics::draw_str(dx + 10, dy + 68, "Mute", COL_TEXT);
                }
            }
            WinKind::Video => {
                draw_display_props(wx, wy, ww, wh);
            }
            WinKind::Files => {
                                crate::files_mgr::draw(wx, wy, ww, wh, TITLE_H as usize);
            }
            WinKind::DateTime => {
                                graphics::fill_rect(wx + 3, wy + TITLE_H as usize, ww - 6, wh - TITLE_H as usize - 3, COL_CLIENT);
                let mut tbuf = [0u8; 20];
                let n = crate::time::format_datetime(&mut tbuf);
                graphics::draw_str(wx + 16, wy + 48, "System clock (RTC)", COL_TEXT_DIM);
                let mut i = 0usize;
                while i < n {
                    graphics::draw_char(wx + 16 + i * 8, wy + 68, tbuf[i], COL_TEXT);
                    i += 1;
                }
                graphics::draw_str(wx + 16, wy + 92, "Source: CMOS 0x70", COL_TEXT_DIM);
            }
            WinKind::Alarm => {
                crate::alarm::draw(wx, wy, ww, wh, TITLE_H as usize);
            }
            WinKind::Movie => {
                let body_y = wy + TITLE_H as usize;
                let body_h = wh.saturating_sub(TITLE_H as usize + 3);
                graphics::fill_rect(wx + 3, body_y, ww - 6, body_h, 0x00101820);
                graphics::fill_rect(wx + 3, body_y, ww - 6, 28, 0x00ECE9D8);
                graphics::draw_str(wx + 12, body_y + 8, crate::video_player::clip_name(), COL_TEXT);
                graphics::draw_str(wx + 120, body_y + 8, crate::video_player::clip_desc(), COL_TEXT_DIM);
                let stage_y = body_y + 30;
                let stage_h = body_h.saturating_sub(30 + 56);
                crate::video_player::draw_frame(wx + 8, stage_y, ww.saturating_sub(16), stage_h);
                let cy = wy + (wh as usize).saturating_sub(50);
                graphics::fill_rect(wx + 3, cy, ww - 6, 44, 0x00D4D0C8);
                graphics::fill_rect(wx + 16, cy + 10, 56, 24, 0x00316AC5);
                graphics::draw_str(wx + 24, cy + 16, "Play", 0x00FFFFFF);
                graphics::fill_rect(wx + 80, cy + 10, 64, 24, 0x005A6673);
                graphics::draw_str(wx + 88, cy + 16, "Pause", 0x00FFFFFF);
                graphics::fill_rect(wx + 152, cy + 10, 56, 24, 0x008B0000);
                graphics::draw_str(wx + 160, cy + 16, "Stop", 0x00FFFFFF);
                graphics::fill_rect(wx + 220, cy + 10, 56, 24, 0x00208040);
                graphics::draw_str(wx + 228, cy + 16, "Next", 0x00FFFFFF);
                let mut st = [0u8; 16];
                let sn = crate::video_player::status_line(&mut st);
                let mut i = 0usize;
                while i < sn {
                    graphics::draw_char(wx + 290 + i * 8, cy + 16, st[i], COL_TEXT);
                    i += 1;
                }
                if crate::video_player::from_disk() {
                    let mut di = [0u8; 24];
                    let dn = crate::video_player::disk_info_line(&mut di);
                    let mut i = 0usize;
                    while i < dn {
                        graphics::draw_char(wx + 370 + i * 8, cy + 16, di[i], COL_TEXT_DIM);
                        i += 1;
                    }
                } else {
                    graphics::draw_str(wx + 370, cy + 16, "SW 320x180", COL_TEXT_DIM);
                }
            }
            WinKind::Browser => {
                draw_browser(wx, wy, ww, wh);
            }
            WinKind::MyComputer => {
                // My Computer is the Explorer entry point. Do not render the
                // legacy storage window here: route this window through the
                // single native Windows-7-style Explorer implementation.
                crate::files_mgr::draw(wx, wy, ww, wh, TITLE_H as usize);
            }
            WinKind::Keyboard => {
                draw_osk(wx, wy, ww, wh);
            }
            WinKind::Settings => {
                draw_settings(wx, wy, ww, wh);
            }
            WinKind::SysProps => {
                                graphics::fill_rect(wx + 3, wy + TITLE_H as usize, ww - 6, wh - TITLE_H as usize - 3, COL_CLIENT);
                // Tab strip
                graphics::fill_rect(wx + 10, wy + 34, 70, 18, 0x00FFFFFF);
                graphics::border_rect(wx + 10, wy + 34, 70, 18, 0x00808080);
                graphics::draw_str(wx + 18, wy + 38, "General", COL_TEXT);

                graphics::border_rect(wx + 10, wy + 52, ww - 24, 130, 0x00808080);
                graphics::draw_str(wx + 18, wy + 60, "System:", COL_TEXT_DIM);
                graphics::draw_str(wx + 18, wy + 76, "Aether Operating System", COL_TEXT);
                graphics::draw_str(wx + 18, wy + 92, "Native kernel (not Windows)", 0x00800000);
                graphics::draw_str(wx + 18, wy + 108, "Desktop shell: XP-style UI", COL_TEXT_DIM);
                graphics::draw_str(wx + 18, wy + 124, "Ring3 userspace: active", COL_TEXT_DIM);
                graphics::draw_str(wx + 18, wy + 148, "Registered to: Aether User", COL_TEXT_DIM);

                graphics::border_rect(wx + 10, wy + 190, ww - 24, 150, 0x00808080);
                graphics::draw_str(wx + 18, wy + 198, "Computer:", COL_TEXT_DIM);
                // Real CPU brand from CPUID
                crate::cpu::init();
                let brand = crate::cpu::brand();
                let mut bi = 0usize;
                let mut bx = wx + 18;
                while bi < brand.len() && bi < 40 {
                    graphics::draw_char(bx, wy + 214, brand[bi], COL_TEXT);
                    bx += 8;
                    bi += 1;
                }
                if brand.is_empty() {
                    graphics::draw_str(wx + 18, wy + 214, "x86-64 processor", COL_TEXT);
                }
                // SMP cores
                graphics::draw_str(wx + 18, wy + 230, "CPUs online:", COL_TEXT);
                draw_u32(wx + 120, wy + 230, crate::smp::online_count() as u32, COL_TEXT);
                graphics::draw_str(wx + 150, wy + 230, "/", COL_TEXT);
                draw_u32(wx + 160, wy + 230, crate::smp::cpu_count() as u32, COL_TEXT);
                // RAM: installed (Multiboot total) preferred
                let inst = crate::mm::installed_mb() as u32;
                let managed = crate::mm::total_mb() as u32;
                let free_mib = crate::mm::free_mb() as u32;
                let show = if inst > 0 { inst } else { managed };
                graphics::draw_str(wx + 18, wy + 246, "Total RAM:", COL_TEXT);
                draw_u32(wx + 110, wy + 246, show, COL_TEXT);
                graphics::draw_str(wx + 160, wy + 246, "MB", COL_TEXT);
                graphics::draw_str(wx + 18, wy + 262, "Available:", COL_TEXT);
                draw_u32(wx + 110, wy + 262, free_mib, COL_TEXT);
                graphics::draw_str(wx + 160, wy + 262, "MB", COL_TEXT);
                // Video
                let gw = crate::graphics::width() as u32;
                let gh = crate::graphics::height() as u32;
                graphics::draw_str(wx + 18, wy + 278, "Display:", COL_TEXT);
                draw_u32(wx + 90, wy + 278, gw, COL_TEXT);
                graphics::draw_str(wx + 130, wy + 278, "x", COL_TEXT);
                draw_u32(wx + 140, wy + 278, gh, COL_TEXT);
                // Network + storage
                graphics::draw_str(wx + 12, wy + 310, "Net:", COL_TEXT_DIM);
                if crate::drivers::net::dhcp_ok() {
                    graphics::draw_str(wx + 50, wy + 310, "DHCP OK (Ethernet)", COL_TEXT);
                } else {
                    graphics::draw_str(wx + 50, wy + 310, "run HH in Terminal", COL_TEXT_DIM);
                }
                graphics::draw_str(wx + 12, wy + 326, "Disk:", COL_TEXT_DIM);
                if crate::drivers::ata::is_hardware() {
                    graphics::draw_str(wx + 50, wy + 326, "ATA HDD", COL_TEXT);
                } else if crate::drivers::ata::is_ramdisk() {
                    graphics::draw_str(wx + 50, wy + 326, "RAM disk", COL_TEXT);
                } else {
                    graphics::draw_str(wx + 50, wy + 326, "none", COL_TEXT);
                }
                graphics::draw_str(wx + 12, wy + 342, "USB:", COL_TEXT_DIM);
                if crate::drivers::xhci::diag_xhci_ok() {
                    graphics::draw_str(wx + 50, wy + 342, "xHCI OK", COL_TEXT);
                } else {
                    graphics::draw_str(wx + 50, wy + 342, "xHCI -", COL_TEXT_DIM);
                }
                if crate::drivers::ehci::diag_ok() {
                    graphics::draw_str(wx + 120, wy + 342, "EHCI OK", COL_TEXT);
                } else {
                    graphics::draw_str(wx + 120, wy + 342, "EHCI -", COL_TEXT_DIM);
                }
            }
        }
    }
}


fn draw_display_props(wx: usize, wy: usize, ww: usize, wh: usize) {
    unsafe {
        graphics::fill_rect(wx + 3, wy + TITLE_H as usize, ww - 6, wh - TITLE_H as usize - 3, COL_CLIENT);

        // Tabs
        let tab_y = wy + TITLE_H as usize + 6;
        let t0x = wx + 12;
        let t1x = wx + 100;
        if DISP_TAB == 0 {
            graphics::fill_rect(t0x, tab_y, 80, 22, 0x00FFFFFF);
            graphics::border_rect(t0x, tab_y, 80, 22, 0x00316AC5);
            graphics::draw_str(t0x + 12, tab_y + 6, "Settings", COL_TEXT);
            graphics::fill_rect(t1x, tab_y, 80, 22, 0x00E8E8E8);
            graphics::border_rect(t1x, tab_y, 80, 22, 0x00808080);
            graphics::draw_str(t1x + 14, tab_y + 6, "Adapter", COL_TEXT_DIM);
        } else {
            graphics::fill_rect(t0x, tab_y, 80, 22, 0x00E8E8E8);
            graphics::border_rect(t0x, tab_y, 80, 22, 0x00808080);
            graphics::draw_str(t0x + 12, tab_y + 6, "Settings", COL_TEXT_DIM);
            graphics::fill_rect(t1x, tab_y, 80, 22, 0x00FFFFFF);
            graphics::border_rect(t1x, tab_y, 80, 22, 0x00316AC5);
            graphics::draw_str(t1x + 14, tab_y + 6, "Adapter", COL_TEXT);
        }

        if DISP_TAB == 0 {
            graphics::draw_str(wx + 16, wy + 60, "Display:", COL_TEXT_DIM);
            graphics::draw_str(wx + 90, wy + 60, "Plug and Play Monitor", COL_TEXT);

            graphics::draw_str(wx + 16, wy + 84, "Screen resolution", COL_TEXT);
            // Mode list box
            let lx = wx + 16;
            let ly = wy + 104;
            let lw = 280usize;
            let lh = 160usize;
            graphics::fill_rect(lx, ly, lw, lh, 0x00FFFFFF);
            graphics::border_rect(lx, ly, lw, lh, 0x00808080);

            let n = crate::drivers::intel_igpu::mode_count();
            let mut i = 0usize;
            while i < n && i < 8 {
                let row_y = ly + 6 + i * 18;
                if let Some((w, h, bpp)) = crate::drivers::intel_igpu::mode_at(i) {
                    if i == DISP_SEL {
                        graphics::fill_rect(lx + 2, row_y - 2, lw - 4, 18, 0x00316AC5);
                        let col = 0x00FFFFFF;
                        draw_u32(lx + 10, row_y, w as u32, col);
                        graphics::draw_str(lx + 50, row_y, "x", col);
                        draw_u32(lx + 62, row_y, h as u32, col);
                        graphics::draw_str(lx + 110, row_y, "32-bit", col);
                        let _ = bpp;
                    } else {
                        draw_u32(lx + 10, row_y, w as u32, COL_TEXT);
                        graphics::draw_str(lx + 50, row_y, "x", COL_TEXT);
                        draw_u32(lx + 62, row_y, h as u32, COL_TEXT);
                        graphics::draw_str(lx + 110, row_y, "32-bit", COL_TEXT_DIM);
                    }
                }
                i += 1;
            }
            if n == 0 {
                graphics::draw_str(lx + 10, ly + 20, "No modes listed", COL_TEXT_DIM);
            }

            // Current
            graphics::draw_str(wx + 16, wy + 280, "Current:", COL_TEXT_DIM);
            draw_u32(wx + 90, wy + 280, graphics::width() as u32, COL_TEXT);
            graphics::draw_str(wx + 130, wy + 280, "x", COL_TEXT);
            draw_u32(wx + 142, wy + 280, graphics::height() as u32, COL_TEXT);
            graphics::draw_str(wx + 190, wy + 280, "32 bpp", COL_TEXT_DIM);

            let pw = crate::drivers::intel_kms::panel_width();
            let ph = crate::drivers::intel_kms::panel_height();
            if pw >= 640 && ph >= 480 {
                graphics::draw_str(wx + 16, wy + 300, "Panel native:", COL_TEXT_DIM);
                draw_u32(wx + 130, wy + 300, pw as u32, COL_TEXT);
                graphics::draw_str(wx + 170, wy + 300, "x", COL_TEXT);
                draw_u32(wx + 182, wy + 300, ph as u32, COL_TEXT);
            }

            // Apply / OK buttons
            let bx = wx + ww - 180;
            let by = wy + wh - 40;
            graphics::fill_rect(bx, by, 70, 26, COL_BTN_FACE);
            graphics::border_rect(bx, by, 70, 26, 0x00404040);
            graphics::draw_str(bx + 16, by + 8, "Apply", COL_TEXT);
            graphics::fill_rect(bx + 80, by, 70, 26, COL_BTN_FACE);
            graphics::border_rect(bx + 80, by, 70, 26, 0x00404040);
            graphics::draw_str(bx + 100, by + 8, "OK", COL_TEXT);

            graphics::draw_str(wx + 16, wy + 330, "Note: modeset needs Intel KMS path", COL_TEXT_DIM);
        } else {
            // Adapter tab
            graphics::draw_str(wx + 16, wy + 60, "Adapter", COL_TEXT);
            crate::drivers::intel_igpu::refresh_vram();
            let _ = crate::drivers::intel_gtt::probe();
            if crate::drivers::intel_igpu::ready() {
                graphics::draw_str(wx + 16, wy + 84, crate::drivers::intel_igpu::chip_name(), COL_TEXT);
                graphics::draw_str(wx + 16, wy + 104, crate::drivers::intel_igpu::driver_name(), COL_TEXT_DIM);
                let g = crate::drivers::intel_igpu::gpu();
                graphics::draw_str(wx + 16, wy + 128, "PCI:", COL_TEXT_DIM);
                draw_u32(wx + 60, wy + 128, g.bus as u32, COL_TEXT);
                graphics::draw_str(wx + 90, wy + 128, ":", COL_TEXT);
                draw_u32(wx + 100, wy + 128, g.dev as u32, COL_TEXT);
                graphics::draw_str(wx + 130, wy + 128, ".", COL_TEXT);
                draw_u32(wx + 140, wy + 128, g.func as u32, COL_TEXT);
                graphics::draw_str(wx + 16, wy + 148, "VID:DID 8086:", COL_TEXT_DIM);
                let mut hx = [0u8; 4];
                let mut v = g.did as u32;
                let mut i = 4usize;
                while i > 0 {
                    i -= 1;
                    let n = (v & 0xF) as u8;
                    hx[i] = if n < 10 { b'0' + n } else { b'A' + (n - 10) };
                    v >>= 4;
                }
                let mut i = 0usize;
                while i < 4 {
                    graphics::draw_char(wx + 130 + i * 8, wy + 148, hx[i], COL_TEXT);
                    i += 1;
                }

                // Stolen = BIOS DVMT pre-allocated (from GMCH_CTRL) - real hardware value
                // HD 3000 has NO discrete VRAM - shared system RAM.
                // Stolen = BIOS pre-alloc (GMCH GMS). Shared max per Intel/Fujitsu AH532 ~1695 MB.
                let mut stolen = crate::drivers::intel_igpu::vram_mb();
                if crate::drivers::intel_gtt::ready() {
                    let s = crate::drivers::intel_gtt::stolen_mb();
                    if s > 0 { stolen = s; }
                }
                let mut aper = crate::drivers::intel_igpu::aper_est_mb();
                if crate::drivers::intel_gtt::ready() {
                    let a = crate::drivers::intel_gtt::aper_mb();
                    if a > 0 { aper = a; }
                }
                if aper == 0 { aper = 256; }

                // Shared max depends on system RAM (Intel SNB table / Fujitsu datasheet)
                let ram_mb = crate::mm::total_mb() as u32;
                let shared_max: u32 = if ram_mb >= 2048 {
                    1695 // Fujitsu AH532 / Intel >2GB: up to ~1695 MB
                } else if ram_mb >= 1024 {
                    783
                } else {
                    256
                };
                let total_avail = if shared_max > stolen { shared_max } else { stolen };

                graphics::draw_str(wx + 16, wy + 176, "Stolen (BIOS):", COL_TEXT_DIM);
                draw_u32(wx + 140, wy + 176, stolen, COL_TEXT);
                graphics::draw_str(wx + 180, wy + 176, "MB fixed", COL_TEXT_DIM);

                graphics::draw_str(wx + 16, wy + 196, "Shared max:", COL_TEXT_DIM);
                draw_u32(wx + 140, wy + 196, shared_max, COL_TEXT);
                graphics::draw_str(wx + 180, wy + 196, "MB (UMA)", COL_TEXT_DIM);

                graphics::draw_str(wx + 16, wy + 216, "Total avail:", COL_TEXT_DIM);
                draw_u32(wx + 140, wy + 216, total_avail, COL_TEXT);
                graphics::draw_str(wx + 180, wy + 216, "MB", COL_TEXT);

                graphics::draw_str(wx + 16, wy + 236, "Aperture:", COL_TEXT_DIM);
                draw_u32(wx + 140, wy + 236, aper, COL_TEXT);
                graphics::draw_str(wx + 180, wy + 236, "MB map", COL_TEXT_DIM);

                // Active FB + pitch already drawn above; shift note
                graphics::draw_str(wx + 16, wy + 300, "NOTE: HD3000=shared RAM, no discrete VRAM", COL_TEXT_DIM);
                graphics::draw_str(wx + 16, wy + 320, "Fujitsu AH532: up to 1695 MB shared", COL_TEXT_DIM);

                graphics::draw_str(wx + 16, wy + 340, "GMCH:", COL_TEXT_DIM);
                let gmch = crate::drivers::intel_igpu::gmch_raw();
                let mut hx = [0u8; 8];
                let mut v = gmch;
                let mut i = 8usize;
                while i > 0 {
                    i -= 1;
                    let n = (v & 0xF) as u8;
                    hx[i] = if n < 10 { b'0' + n } else { b'A' + (n - 10) };
                    v >>= 4;
                }
                let mut i = 0usize;
                while i < 8 {
                    graphics::draw_char(wx + 70 + i * 8, wy + 340, hx[i], COL_TEXT);
                    i += 1;
                }

                graphics::draw_str(wx + 16, wy + 216, "Active framebuffer:", COL_TEXT_DIM);
                draw_u32(wx + 180, wy + 216, crate::drivers::intel_igpu::active_fb_mb(), COL_TEXT);
                graphics::draw_str(wx + 210, wy + 216, "MB", COL_TEXT);

                graphics::draw_str(wx + 16, wy + 240, "Pitch:", COL_TEXT_DIM);
                draw_u32(wx + 70, wy + 240, graphics::pitch() as u32, COL_TEXT);
                graphics::draw_str(wx + 16, wy + 260, "BPP: 32", COL_TEXT_DIM);

                if crate::drivers::intel_kms::ready() {
                    graphics::draw_str(wx + 16, wy + 288, "KMS: READY", 0x00008000);
                } else {
                    graphics::draw_str(wx + 16, wy + 288, "KMS: not ready (Multiboot LFB)", 0x00808000);
                }
                graphics::draw_str(wx + 16, wy + 310, "Note: Stolen=BIOS pre-alloc; Windows may show aperture", COL_TEXT_DIM);
            } else {
                graphics::draw_str(wx + 16, wy + 84, "No Intel iGPU detected", 0x00800000);
                graphics::draw_str(wx + 16, wy + 104, "Using Multiboot framebuffer only", COL_TEXT_DIM);
                graphics::draw_str(wx + 16, wy + 128, "Resolution:", COL_TEXT_DIM);
                draw_u32(wx + 110, wy + 128, graphics::width() as u32, COL_TEXT);
                graphics::draw_str(wx + 150, wy + 128, "x", COL_TEXT);
                draw_u32(wx + 162, wy + 128, graphics::height() as u32, COL_TEXT);
            }
        }
    }
}

fn draw_settings(wx: usize, wy: usize, ww: usize, wh: usize) {
    unsafe {
        graphics::fill_rect(wx + 3, wy + TITLE_H as usize, ww - 6, wh - TITLE_H as usize - 3, COL_CLIENT);

        // Settings categories are intentionally present as native placeholders.
        // Only Mouse is functional today; the other pages document the future
        // driver/subsystem surfaces without pretending they are implemented.
        let cats: [&str; 10] = [
            "Mouse",
            "Display",
            "Sound",
            "Network",
            "Date and Time",
            "Keyboard",
            "Storage",
            "Power",
            "Devices",
            "System",
        ];

        if SETTINGS_VIEW == 1 {
            graphics::draw_str(wx + 18, wy + 44, "Mouse", COL_TEXT);
            graphics::draw_str(wx + 18, wy + 62, "Mouse settings", COL_TEXT_DIM);
            graphics::border_rect(wx + 14, wy + 76, ww - 28, 150, 0x00808080);
            graphics::draw_str(wx + 26, wy + 88, "Choose cursor", COL_TEXT);
            let colors: [u32; 5] = [0x00FFFFFF, 0x00000000, 0x00E81123, 0x0000A000, 0x000000CC];
            let names: [&str; 5] = ["White", "Black", "Red", "Green", "Blue"];
            let mut i = 0usize;
            while i < 5 {
                let bx = wx + 28 + i * 88;
                let by = wy + 112;
                let selected = CURSOR_PENDING == i as u8;
                graphics::fill_rect(bx, by, 68, 82, if selected { 0x00DCEBFA } else { 0x00FFFFFF });
                graphics::border_rect(bx, by, 68, 82, if selected { 0x00316AC5 } else { 0x00808080 });
                let cx = bx + 24;
                let cy = by + 10;
                let col = colors[i];
                graphics::fill_rect(cx, cy, 2, 18, col);
                graphics::fill_rect(cx, cy, 13, 2, col);
                graphics::fill_rect(cx + 2, cy + 5, 9, 2, col);
                graphics::fill_rect(cx + 2, cy + 10, 7, 2, col);
                graphics::put_pixel(cx + 3, cy + 14, col);
                graphics::put_pixel(cx + 4, cy + 15, col);
                graphics::draw_str(bx + 8, by + 58, names[i], COL_TEXT);
                i += 1;
            }
            graphics::draw_str(wx + 18, wy + 244, "Current cursor:", COL_TEXT_DIM);
            let current = match CURSOR_PENDING { 0 => "White", 1 => "Black", 2 => "Red", 3 => "Green", _ => "Blue" };
            graphics::draw_str(wx + 126, wy + 244, current, COL_TEXT);
            let okx = wx + ww - 184;
            let cancelx = wx + ww - 94;
            let by = wy + wh - 42;
            graphics::fill_rect(okx, by, 78, 24, COL_BTN_FACE);
            graphics::border_rect(okx, by, 78, 24, 0x00404040);
            graphics::draw_str(okx + 22, by + 8, "Choose", COL_TEXT);
            graphics::fill_rect(cancelx, by, 78, 24, COL_BTN_FACE);
            graphics::border_rect(cancelx, by, 78, 24, 0x00404040);
            graphics::draw_str(cancelx + 22, by + 8, "Cancel", COL_TEXT);
            return;
        }

        if SETTINGS_VIEW == 0 {
            graphics::fill_rect(wx + 8, wy + 34, 175, wh - 50, 0x00FFFFFF);
            graphics::border_rect(wx + 8, wy + 34, 175, wh - 50, 0x00808080);
            graphics::draw_str(wx + 20, wy + 44, "Settings", COL_TEXT);

            let mut i = 0usize;
            while i < cats.len() {
                let row_y = wy + 66 + i * 25;
                let selected = i == 0;
                if selected {
                    graphics::fill_rect(wx + 14, row_y - 2, 163, 23, 0x00DCEBFA);
                    graphics::border_rect(wx + 14, row_y - 2, 163, 23, 0x00316AC5);
                }
                graphics::draw_str(wx + 24, row_y + 4, cats[i], COL_TEXT);
                i += 1;
            }

            graphics::border_rect(wx + 196, wy + 34, ww - 212, wh - 50, 0x00808080);
            graphics::draw_str(wx + 212, wy + 48, "Settings", COL_TEXT);
            graphics::draw_str(wx + 212, wy + 76, "Configure Aether OS.", COL_TEXT_DIM);
            graphics::draw_str(wx + 212, wy + 98, "Mouse", COL_TEXT);
            graphics::draw_str(wx + 212, wy + 116, "Pointer and cursor settings are available.", COL_TEXT_DIM);
            graphics::draw_str(wx + 212, wy + 150, "Other categories are prepared", COL_TEXT_DIM);
            graphics::draw_str(wx + 212, wy + 166, "for future native drivers and services.", COL_TEXT_DIM);
            return;
        }

        // Placeholder page for a future subsystem.
        let idx = (SETTINGS_VIEW - 2) as usize;
        if idx < cats.len() {
            graphics::draw_str(wx + 18, wy + 44, cats[idx], COL_TEXT);
            graphics::draw_str(wx + 18, wy + 62, "Settings", COL_TEXT_DIM);
            graphics::border_rect(wx + 14, wy + 76, ww - 28, wh - 132, 0x00808080);
            graphics::draw_str(wx + 30, wy + 104, "This section is prepared for future", COL_TEXT);
            graphics::draw_str(wx + 30, wy + 122, "native Aether OS implementation.", COL_TEXT);
            graphics::draw_str(wx + 30, wy + 150, "Status: planned (honest UI)", 0x00808000);
            graphics::draw_str(wx + 30, wy + 178, "Core: USB scanusb, Explorer, SysProps.", COL_TEXT_DIM);
            let bx = wx + ww - 94;
            let by = wy + wh - 42;
            graphics::fill_rect(bx, by, 78, 24, COL_BTN_FACE);
            graphics::border_rect(bx, by, 78, 24, 0x00404040);
            graphics::draw_str(bx + 24, by + 8, "Back", COL_TEXT);
        }
    }
}
fn draw_u32(x: usize, y: usize, n: u32, color: u32) {
    if n == 0 {
        graphics::draw_char(x, y, b'0', color);
        return;
    }
    let mut digs = [0u8; 10];
    let mut nd = 0usize;
    let mut v = n;
    while v > 0 && nd < 10 {
        digs[nd] = b'0' + (v % 10) as u8;
        v /= 10;
        nd += 1;
    }
    let mut i = nd;
    let mut cx = x;
    while i > 0 {
        i -= 1;
        graphics::draw_char(cx, y, digs[i], color);
        cx += 8;
    }
}

fn draw_mouse_diag(y: usize) {
    use crate::drivers::ps2;
    use crate::drivers::xhci;
    let ok = 0x0040C060u32;
    let fail = 0x00C04040u32;
    let txt = 0x00E0E8F0u32;
    graphics::draw_str(8, y, "[PS2] ctrl=", txt);
    if ps2::diag_controller_ok() {
        graphics::draw_str(8 + 11 * 8, y, "OK", ok);
    } else {
        graphics::draw_str(8 + 11 * 8, y, "FAIL", fail);
    }
    graphics::draw_str(8 + 16 * 8, y, " ack=", txt);
    if ps2::diag_ack_ok() {
        graphics::draw_str(8 + 21 * 8, y, "OK", ok);
    } else {
        graphics::draw_str(8 + 21 * 8, y, "FAIL", fail);
    }
    graphics::draw_str(8 + 26 * 8, y, " stream=", txt);
    if ps2::diag_stream_ok() {
        graphics::draw_str(8 + 34 * 8, y, "OK", ok);
    } else {
        graphics::draw_str(8 + 34 * 8, y, "FAIL", fail);
    }
    graphics::draw_str(8 + 39 * 8, y, " pkts=", txt);
    draw_u32(8 + 45 * 8, y, ps2::diag_packets(), txt);

    let y2 = y + 12;
    graphics::draw_str(8, y2, "[USB] xHCI=", txt);
    if xhci::diag_xhci_ok() {
        graphics::draw_str(8 + 12 * 8, y2, "OK", ok);
    } else {
        graphics::draw_str(8 + 12 * 8, y2, "FAIL", fail);
    }
    graphics::draw_str(8 + 17 * 8, y2, " mouse=", txt);
    if xhci::diag_dev_found() || xhci::diag_mouse_if() {
        graphics::draw_str(8 + 24 * 8, y2, "FOUND", ok);
    } else {
        graphics::draw_str(8 + 24 * 8, y2, "NONE", fail);
    }
    graphics::draw_str(8 + 30 * 8, y2, " HID=", txt);
    if xhci::diag_hid_ok() {
        graphics::draw_str(8 + 35 * 8, y2, "OK", ok);
    } else {
        graphics::draw_str(8 + 35 * 8, y2, "FAIL", fail);
    }
    graphics::draw_str(8 + 40 * 8, y2, " R=", txt);
    draw_u32(8 + 43 * 8, y2, xhci::diag_reports(), txt);
    graphics::draw_str(8 + 50 * 8, y2, "C=", txt);
    draw_u32(8 + 52 * 8, y2, xhci::diag_completions(), txt);
    graphics::draw_str(8 + 58 * 8, y2, "E=", txt);
    draw_u32(8 + 60 * 8, y2, xhci::diag_mouse_events(), txt);
    // EHCI status line
    let y3 = y2 + 12;
    graphics::draw_str(8, y3, "[EHCI]", txt);
    if crate::drivers::ehci::diag_ok() {
        graphics::draw_str(8 + 7 * 8, y3, "OK", ok);
    } else {
        graphics::draw_str(8 + 7 * 8, y3, "FAIL", fail);
    }
    graphics::draw_str(8 + 12 * 8, y3, " LIVE=", txt);
    if crate::drivers::ehci::diag_live() {
        graphics::draw_str(8 + 18 * 8, y3, "Y", ok);
    } else {
        graphics::draw_str(8 + 18 * 8, y3, "N", fail);
    }
    graphics::draw_str(8 + 21 * 8, y3, " R=", txt);
    draw_u32(8 + 24 * 8, y3, crate::drivers::ehci::diag_reports(), txt);
    graphics::draw_str(8 + 32 * 8, y3, " E=", txt);
    draw_u32(8 + 35 * 8, y3, crate::drivers::ehci::diag_events(), txt);
}



const CURSOR_W: usize = 12;
const CURSOR_H: usize = 16;

fn cursor_restore() {
    unsafe {
        if !CURSOR_SAVED || CURSOR_OX < 0 {
            return;
        }
        let ox = CURSOR_OX as usize;
        let oy = CURSOR_OY as usize;
        let mut row = 0usize;
        while row < CURSOR_H {
            let mut col = 0usize;
            while col < CURSOR_W {
                graphics::put_pixel(ox + col, oy + row, CURSOR_BG[row * CURSOR_W + col]);
                col += 1;
            }
            row += 1;
        }
        CURSOR_SAVED = false;
    }
}

fn cursor_save_and_draw(x: i32, y: i32) {
    unsafe {
        cursor_restore();
        let sw = graphics::width() as i32;
        let sh = graphics::height() as i32;
        let x = if x < 0 { 0 } else if x + CURSOR_W as i32 > sw { sw - CURSOR_W as i32 } else { x };
        let y = if y < 0 { 0 } else if y + CURSOR_H as i32 > sh { sh - CURSOR_H as i32 } else { y };
        let ux = x as usize;
        let uy = y as usize;
        let mut row = 0usize;
        while row < CURSOR_H {
            let mut col = 0usize;
            while col < CURSOR_W {
                CURSOR_BG[row * CURSOR_W + col] = graphics::get_pixel(ux + col, uy + row);
                col += 1;
            }
            row += 1;
        }
        CURSOR_OX = x;
        CURSOR_OY = y;
        CURSOR_SAVED = true;
        draw_cursor(x, y);
    }
}

fn redraw_status_strip() {
    let w = graphics::width();
    let h = graphics::height();
    if w == 0 || h == 0 {
        return;
    }
    let tb = TASKBAR_H;
    let y0 = h.saturating_sub(tb);
    graphics::fill_rect(0, y0, w, tb, COL_TASKBAR);
    graphics::fill_rect(0, y0, w, 2, COL_TASKBAR_TOP);
    draw_taskbar_buttons(w, h);
    draw_status_icons_and_clock(w, h);
    draw_start_button(w, h);
}



fn draw_start_button(_w: usize, h: usize) {
    let tb = TASKBAR_H;
    let ty = h.saturating_sub(tb);
    let sb_w = 72usize;
    graphics::fill_rect(0, ty, sb_w, tb, COL_START);
    graphics::fill_rect(0, ty, sb_w, 3, COL_START_HI);
    graphics::fill_rect(0, ty, 3, tb, COL_START_HI);
    graphics::border_rect(0, ty, sb_w, tb, 0x00FFFFFF);
    graphics::fill_rect(6, ty + 6, 7, 7, 0x00F0F0F0);
    graphics::fill_rect(15, ty + 6, 7, 7, 0x00F0F0F0);
    graphics::fill_rect(6, ty + 15, 7, 7, 0x00F0F0F0);
    graphics::fill_rect(15, ty + 15, 7, 7, 0x00F0F0F0);
    graphics::draw_str(26, ty + 11, "start", 0x00FFFFFF);
}


fn draw_status_icons_and_clock(w: usize, h: usize) {
    let cy = h.saturating_sub(24);
    // Compact tray from right: clock (5 chars HH:MM) + 3 icons 16px
    // layout right-to-left so clock always fits
    let clock_w = 5 * 8 + 8; // "12:34" + pad
    let icon_slot = 20usize;
    let tray_w = clock_w + icon_slot * 3 + 8;
    let tray_x = if w > tray_w + 4 { w - tray_w - 4 } else { 4 };
    // background strip
    graphics::fill_rect(tray_x, cy - 1, tray_w, 18, 0x001C4F9C);
    // Network icon: green if WiFi firmware alive, else red
    let nx = tray_x + 2;
    // Green ONLY when networks seen or connected - not merely ALIVE.
    let net_col = if unsafe { WIFI_UI_STATUS == 2 } || crate::drivers::wifi::ap_count() > 0 {
        0x0020C020u32 // green: usable scan results / connected
    } else if crate::drivers::wifi::alive_ok() {
        0x00E0A000u32 // orange: firmware up, no networks yet
    } else if crate::drivers::wifi::found() {
        0x00C02020u32 // red: device present, firmware not alive
    } else {
        0x00606060u32 // gray: no adapter
    };
    graphics::fill_rect(nx + 2, cy + 3, 8, 6, 0x00C0C0C0);
    graphics::fill_rect(nx + 3, cy + 4, 6, 4, net_col);
    graphics::fill_rect(nx + 10, cy + 5, 4, 2, net_col);
    // Volume speaker
    let vx = tray_x + icon_slot + 2;
    graphics::fill_rect(vx + 2, cy + 5, 4, 6, 0x00E0E0E0);
    graphics::fill_rect(vx + 6, cy + 3, 2, 10, 0x00E0E0E0);
    graphics::fill_rect(vx + 9, cy + 4, 2, 2, 0x00A0A0A0);
    graphics::fill_rect(vx + 9, cy + 8, 2, 2, 0x00A0A0A0);
    graphics::fill_rect(vx + 11, cy + 2, 2, 2, 0x00A0A0A0);
    graphics::fill_rect(vx + 11, cy + 10, 2, 2, 0x00A0A0A0);
    // Display monitor
    let dx = tray_x + icon_slot * 2 + 2;
    graphics::fill_rect(dx + 1, cy + 3, 12, 8, 0x00A0A0A0);
    graphics::fill_rect(dx + 2, cy + 4, 10, 6, 0x000A246A);
    graphics::fill_rect(dx + 5, cy + 11, 4, 2, 0x00808080);
    // Clock HH:MM only
    let mut tbuf = [0u8; 20];
    let n = crate::time::format_datetime(&mut tbuf);
    // extract time portion if "YYYY-MM-DD HH:MM" or use last 5 chars
    let (tx, tlen) = if n >= 16 {
        // assume "YYYY-MM-DD HH:MM.."
        (11usize, 5usize)
    } else if n >= 5 {
        (n - 5, 5)
    } else {
        (0usize, n)
    };
    let cx = tray_x + icon_slot * 3 + 4;
    let mut i = 0usize;
    while i < tlen && tx + i < n {
        graphics::draw_char(cx + i * 8, cy + 3, tbuf[tx + i], 0x00FFFFFF);
        i += 1;
    }
}


fn hit_tray(mx: i32, my: i32) -> Option<u8> {
    // 1=net 2=vol 3=display 4=clock - match draw_status_icons_and_clock
    let w = graphics::width() as i32;
    let h = graphics::height() as i32;
    if my < h - (TASKBAR_H as i32) {
        return None;
    }
    let clock_w = 5 * 8 + 8;
    let icon_slot = 20i32;
    let tray_w = clock_w as i32 + icon_slot * 3 + 8;
    let tray_x = if w > tray_w + 4 { w - tray_w - 4 } else { 4 };
    if mx >= tray_x && mx < tray_x + icon_slot {
        return Some(1);
    }
    if mx >= tray_x + icon_slot && mx < tray_x + icon_slot * 2 {
        return Some(2);
    }
    if mx >= tray_x + icon_slot * 2 && mx < tray_x + icon_slot * 3 {
        return Some(3);
    }
    if mx >= tray_x + icon_slot * 3 {
        return Some(4);
    }
    None
}


/// Drag frame: erase old window rect + redraw windows/panel - NO full framebuffer fill
fn render_drag_step() {
    unsafe {
        if !DRAGGING || DRAG_WIN >= MAX_WIN {
            return;
        }
        let w = graphics::width();
        let h = graphics::height();
        // 1) Erase UNION of old + new footprint (reduces trail flicker)
        let nx = WINS[DRAG_WIN].x;
        let ny = WINS[DRAG_WIN].y;
        let nw = WINS[DRAG_WIN].w;
        let nh = WINS[DRAG_WIN].h;
        let left = if DRAG_OLD_X < nx { DRAG_OLD_X } else { nx };
        let top = if DRAG_OLD_Y < ny { DRAG_OLD_Y } else { ny };
        let right = if DRAG_OLD_X + DRAG_OLD_W > nx + nw {
            DRAG_OLD_X + DRAG_OLD_W
        } else {
            nx + nw
        };
        let bottom = if DRAG_OLD_Y + DRAG_OLD_H > ny + nh {
            DRAG_OLD_Y + DRAG_OLD_H
        } else {
            ny + nh
        };
        let ox = if left < 0 { 0usize } else { left as usize };
        let oy = if top < 0 { 0usize } else { top as usize };
        let ow = if right - left < 0 { 0 } else { (right - left) as usize + 6 };
        let oh = if bottom - top < 0 { 0 } else { (bottom - top) as usize + 6 };
        graphics::fill_rect(ox, oy, ow, oh, COL_BG);
        // 2) If old rect hit panel/dock, restore strips
        if oy < 30 {
            graphics::fill_rect(0, 0, w, 28, COL_PANEL);
            graphics::fill_rect(0, 28, w, 2, COL_ACCENT);
            graphics::draw_str(12, 10, "Aether Desktop v1.1 XP", COL_TITLE);
        }
        let dock_y = h.saturating_sub(40);
        if oy + oh > dock_y {
            redraw_status_strip();
        }
        // 3) Redraw all windows in z-order (no full clear)
        let mut order = [0usize; MAX_WIN];
        let mut n = 0usize;
        let mut i = 0usize;
        while i < MAX_WIN {
            if WINS[i].visible {
                order[n] = i;
                n += 1;
            }
            i += 1;
        }
        let mut a = 0usize;
        while a < n {
            let mut b = 0usize;
            while b + 1 < n {
                if WINS[order[b]].z > WINS[order[b + 1]].z {
                    let tmp = order[b];
                    order[b] = order[b + 1];
                    order[b + 1] = tmp;
                }
                b += 1;
            }
            a += 1;
        }
        i = 0;
        while i < n {
            draw_window(order[i]);
            i += 1;
        }
        // Update old rect to current for next step
        DRAG_OLD_X = WINS[DRAG_WIN].x;
        DRAG_OLD_Y = WINS[DRAG_WIN].y;
        DRAG_OLD_W = WINS[DRAG_WIN].w;
        DRAG_OLD_H = WINS[DRAG_WIN].h;
        CURSOR_SAVED = false;
        cursor_save_and_draw(MX, MY);
        crate::graphics::present_full_fast();
    }
}




fn hit_window(mx: i32, my: i32) -> Option<usize> {
    unsafe {
        let mut best: Option<usize> = None;
        let mut best_z = -1i32;
        let mut i = 0usize;
        while i < MAX_WIN {
            if WINS[i].visible && !WINS[i].minimized {
                let w = &WINS[i];
                if mx >= w.x && mx < w.x + w.w as i32 && my >= w.y && my < w.y + w.h as i32 {
                    if w.z as i32 > best_z {
                        best_z = w.z as i32;
                        best = Some(i);
                    }
                }
            }
            i += 1;
        }
        best
    }
}

fn draw_ctx_menu() {
    unsafe {
        if !CTX_MENU { return; }
        let mx = CTX_X as usize;
        let my = CTX_Y as usize;
        let w = 160usize;
        let h = 120usize;
        let sw = graphics::width();
        let sh = graphics::height();
        let x = if mx + w > sw { sw.saturating_sub(w) } else { mx };
        let y = if my + h > sh.saturating_sub(30) { sh.saturating_sub(30 + h) } else { my };
        graphics::fill_rect(x + 2, y + 2, w, h, 0x00404040);
        graphics::fill_rect(x, y, w, h, 0x00FFFFFF);
        graphics::border_rect(x, y, w, h, 0x00808080);
        let items = ["Refresh", "New Text File", "Open Terminal", "Open Computer", "Properties"];
        let mut i = 0usize;
        while i < 5 {
            graphics::draw_str(x + 12, y + 10 + i * 20, items[i], 0x00000000);
            i += 1;
        }
    }
}

fn hit_ctx_menu(mx: i32, my: i32) -> Option<usize> {
    unsafe {
        if !CTX_MENU { return None; }
        let x0 = CTX_X;
        let y0 = CTX_Y;
        let w = 160i32;
        let h = 120i32;
        if mx < x0 || mx >= x0 + w || my < y0 || my >= y0 + h {
            return None;
        }
        let idx = ((my - y0 - 6) / 20) as usize;
        if idx < 5 { Some(idx) } else { None }
    }
}

/// Single source of truth: Start menu labels AND window slots.
/// Index returned by hit_start_menu() maps 1:1 to this table.
const START_MENU_COUNT: usize = 10;
const START_MENU_H: usize = 28 + START_MENU_COUNT * 28 + 28; // header + rows + shutdown
const START_MENU_W: usize = 220;
const START_ITEM_H: i32 = 28;
const START_HDR_H: i32 = 36;

fn start_menu_slot(idx: usize) -> usize {
    // Must match draw_start_menu order exactly
    match idx {
        0 => 0,  // Terminal
        1 => 5,  // Files (Explorer)
        2 => 7,  // My Computer
        3 => 3,  // Media Player (audio)
        4 => 12, // Video Player
        5 => 2,  // Network
        6 => 9,  // Settings
        7 => 11, // Alarm Clock
        8 => 10, // On-Screen Keyboard
        9 => 13, // Browser
        _ => 255,
    }
}

fn start_menu_label(idx: usize) -> &'static str {
    match idx {
        0 => "Terminal",
        1 => "Files",
        2 => "My Computer",
        3 => "Media Player",
        4 => "Video Player",
        5 => "Network",
        6 => "Settings",
        7 => "Alarm Clock",
        8 => "Keyboard",
        9 => "Browser",
        _ => "",
    }
}

fn draw_start_menu() {
    use crate::gui::icon::{self, IconId};
    let h = graphics::height();
    let tb = TASKBAR_H;
    let menu_h = START_MENU_H;
    let menu_w = START_MENU_W;
    let mx = 2usize;
    let my = h.saturating_sub(tb + menu_h);
    graphics::fill_rect(mx + 2, my + 2, menu_w, menu_h, 0x00404040);
    graphics::fill_rect(mx, my, menu_w, menu_h, 0x00FFFFFF);
    graphics::fill_rect(mx, my, 4, menu_h, COL_START);
    graphics::fill_rect(mx, my, menu_w, 28, 0x00245EDC);
    graphics::draw_str(mx + 12, my + 10, "Aether User", 0x00FFFFFF);
    let icons: [IconId; START_MENU_COUNT] = [
        IconId::Terminal,
        IconId::Folder,
        IconId::MyComputer,
        IconId::Settings,
        IconId::Settings,
        IconId::Network,
        IconId::Settings,
        IconId::Settings,
        IconId::Terminal,
        IconId::Network,
    ];
    let mut i = 0usize;
    while i < START_MENU_COUNT {
        let iy = my + 36 + i * 28;
        icon::blit(icons[i], mx + 10, iy, false);
        graphics::draw_str(mx + 48, iy + 10, start_menu_label(i), 0x00000000);
        i += 1;
    }
    graphics::fill_rect(mx, my + menu_h - 28, menu_w, 28, 0x00D4D0C8);
    graphics::draw_str(mx + 16, my + menu_h - 18, "Turn Off Computer", 0x00000000);
}

fn hit_start_menu(mx: i32, my: i32) -> Option<usize> {
    let h = graphics::height() as i32;
    let tb = TASKBAR_H as i32;
    let menu_h = START_MENU_H as i32;
    let menu_w = START_MENU_W as i32;
    let x0 = 2i32;
    let y0 = h - tb - menu_h;
    if mx < x0 || mx >= x0 + menu_w || my < y0 || my >= y0 + menu_h {
        return None;
    }
    // Shutdown strip - not an app index
    if my >= y0 + menu_h - 28 {
        return None;
    }
    let rel = my - (y0 + START_HDR_H);
    if rel < 0 {
        return None;
    }
    let idx = (rel / START_ITEM_H) as usize;
    if idx < START_MENU_COUNT {
        Some(idx)
    } else {
        None
    }
}


/// Redraw chrome only: taskbar, windows, menus, cursor.
/// Does NOT wipe wallpaper - avoids global flicker on clicks/typing.
fn render_ui_only() {
    let w = graphics::width();
    let h = graphics::height();
    if w == 0 || h == 0 {
        return;
    }
    let tb = TASKBAR_H;
    graphics::fill_rect(0, h.saturating_sub(tb), w, tb, COL_TASKBAR);
    graphics::fill_rect(0, h.saturating_sub(tb), w, 2, COL_TASKBAR_TOP);
    draw_taskbar_buttons(w, h);
    draw_status_icons_and_clock(w, h);
    draw_start_button(w, h);
    unsafe {
        let mut order = [0usize; MAX_WIN];
        let mut n = 0usize;
        let mut i = 0usize;
        while i < MAX_WIN {
            if WINS[i].visible {
                order[n] = i;
                n += 1;
            }
            i += 1;
        }
        let mut a = 0usize;
        while a < n {
            let mut b = 0usize;
            while b + 1 < n {
                if WINS[order[b]].z > WINS[order[b + 1]].z {
                    let tmp = order[b];
                    order[b] = order[b + 1];
                    order[b + 1] = tmp;
                }
                b += 1;
            }
            a += 1;
        }
        i = 0;
        while i < n {
            draw_window(order[i]);
            i += 1;
        }
        if START_MENU {
            draw_start_menu();
        }
        if CTX_MENU {
            draw_ctx_menu();
        }
        // Taskbar always on top of windows (panel + Start + clock + cursor)
        {
            let w = graphics::width();
            let h = graphics::height();
            let tb = TASKBAR_H;
            graphics::fill_rect(0, h.saturating_sub(tb), w, tb, COL_TASKBAR);
            graphics::fill_rect(0, h.saturating_sub(tb), w, 2, COL_TASKBAR_TOP);
            draw_taskbar_buttons(w, h);
            draw_status_icons_and_clock(w, h);
            draw_start_button(w, h);
        }
        CURSOR_SAVED = false;
        cursor_save_and_draw(MX, MY);
    }
}

fn render() {
    let w = graphics::width();
    let h = graphics::height();
    if w == 0 || h == 0 {
        return;
    }
    // XP-style bliss-like sky/hills (procedural, not MS wallpaper)
    draw_xp_wallpaper(w, h);
    draw_desktop_icons();
    // XP blue taskbar bottom
    let tb = TASKBAR_H;
    graphics::fill_rect(0, h.saturating_sub(tb), w, tb, COL_TASKBAR);
    graphics::fill_rect(0, h.saturating_sub(tb), w, 2, COL_TASKBAR_TOP);
    let ty = h.saturating_sub(tb);
    draw_taskbar_buttons(w, h);
    draw_status_icons_and_clock(w, h);
    // Start button LAST so nothing covers it (critical for visibility)
    draw_start_button(w, h);
    // windows then start menu on top
    unsafe {
        let mut order = [0usize; MAX_WIN];
        let mut n = 0usize;
        let mut i = 0usize;
        while i < MAX_WIN {
            if WINS[i].visible {
                order[n] = i;
                n += 1;
            }
            i += 1;
        }
        let mut a = 0usize;
        while a < n {
            let mut b = 0usize;
            while b + 1 < n {
                if WINS[order[b]].z > WINS[order[b + 1]].z {
                    let tmp = order[b];
                    order[b] = order[b + 1];
                    order[b + 1] = tmp;
                }
                b += 1;
            }
            a += 1;
        }
        i = 0;
        while i < n {
            draw_window(order[i]);
            i += 1;
        }
        if START_MENU {
            draw_start_menu();
        }
        if CTX_MENU {
            draw_ctx_menu();
        }
        // Taskbar always on top of windows (panel + Start + clock + cursor)
        {
            let w = graphics::width();
            let h = graphics::height();
            let tb = TASKBAR_H;
            graphics::fill_rect(0, h.saturating_sub(tb), w, tb, COL_TASKBAR);
            graphics::fill_rect(0, h.saturating_sub(tb), w, 2, COL_TASKBAR_TOP);
            draw_taskbar_buttons(w, h);
            draw_status_icons_and_clock(w, h);
            draw_start_button(w, h);
        }
        CURSOR_SAVED = false;
        cursor_save_and_draw(MX, MY);
    }
}


fn handle_special_key(hid_code: u8) -> bool {
    unsafe {
        if FOCUS >= MAX_WIN || !WINS[FOCUS].visible {
            return false;
        }
        if WINS[FOCUS].kind == WinKind::Files || WINS[FOCUS].kind == WinKind::MyComputer {
            // USB HID usage IDs (Boot Keyboard)
            // Up=0x52 Down=0x51 Left=0x50 Right=0x4F Enter=0x28 Backspace=0x2A
            crate::files_mgr::note_scancode(hid_code); // show raw HID id in SC=
            let sc = match hid_code {
                0x52 => 0x48u8, // Up
                0x51 => 0x50u8, // Down
                0x50 => 0x4Bu8, // Left
                0x4F => 0x4Du8, // Right
                0x28 => 0x1Cu8, // Enter
                0x2A => 0x0Eu8, // Backspace
                0x4B => 0x49u8, // PageUp
                0x4E => 0x51u8, // PageDown
                0x57 => 0x4Eu8, // Keypad + → Up (AH532)
                0x56 => 0x4Au8, // Keypad - → Down
                _ => 0u8,
            };
            if sc != 0 && crate::files_mgr::on_nav_key(sc) {
                DIRTY_UI = true;
                let f = FOCUS;
                if f < MAX_WIN {
                    redraw_files_after_click(f);
                }
                true
            } else if hid_code != 0 {
                // unknown key: still refresh SC= display
                let f = FOCUS;
                if f < MAX_WIN {
                    redraw_files_after_click(f);
                }
                true
            } else {
                false
            }
        } else if WINS[FOCUS].kind == WinKind::Terminal {
            match hid_code {
                0x52 => {
                    // Up = history
                    term_hist_up();
                    true
                }
                0x51 => {
                    // Down = history
                    term_hist_down();
                    true
                }
                0x4B => {
                    // PageUp = scroll history view
                    term_scroll_up();
                    true
                }
                0x4E => {
                    term_scroll_down();
                    true
                }
                0x50 => {
                    // Left
                    if INPUT_CURSOR > 0 {
                        INPUT_CURSOR -= 1;
                        DIRTY_UI = true;
                    }
                    true
                }
                0x4F => {
                    // Right
                    if INPUT_CURSOR < INPUT_LEN {
                        INPUT_CURSOR += 1;
                        DIRTY_UI = true;
                    }
                    true
                }
                0x4A => {
                    // Home
                    INPUT_CURSOR = 0;
                    DIRTY_UI = true;
                    true
                }
                0x4D => {
                    // End
                    INPUT_CURSOR = INPUT_LEN;
                    DIRTY_UI = true;
                    true
                }
                _ => false,
            }
        } else if WINS[FOCUS].kind == WinKind::Browser {
            match hid_code {
                0x52 => {
                    // Up
                    if BR_SCROLL > 0 { BR_SCROLL -= 1; DIRTY_UI = true; }
                    true
                }
                0x51 => {
                    // Down
                    if BR_SCROLL + 1 < BR_TOTAL_LINES.max(1) { BR_SCROLL += 1; DIRTY_UI = true; }
                    true
                }
                0x4B => {
                    // PageUp
                    BR_SCROLL = BR_SCROLL.saturating_sub(8);
                    DIRTY_UI = true;
                    true
                }
                0x4E => {
                    // PageDown
                    BR_SCROLL = (BR_SCROLL + 8).min(BR_TOTAL_LINES.saturating_sub(1));
                    DIRTY_UI = true;
                    true
                }
                0x4A => {
                    // Home
                    BR_SCROLL = 0;
                    DIRTY_UI = true;
                    true
                }
                0x4D => {
                    // End
                    BR_SCROLL = BR_TOTAL_LINES.saturating_sub(1);
                    DIRTY_UI = true;
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }
}

fn browser_set_status(s: &[u8]) {
    unsafe {
        let mut i = 0usize;
        while i < 48 {
            BR_STATUS[i] = 0;
            i += 1;
        }
        i = 0;
        while i < s.len() && i < 47 {
            BR_STATUS[i] = s[i];
            i += 1;
        }
        BR_STATUS_LEN = i;
    }
}

fn browser_ensure_url() {
    unsafe {
        if BR_INIT {
            return;
        }
        // example.com answers plain HTTP 200
        let d = b"example.com";
        let mut i = 0usize;
        while i < d.len() {
            BR_URL[i] = d[i];
            i += 1;
        }
        BR_URL_LEN = d.len();
        BR_INIT = true;
        browser_set_status(b"Ready - Go (example.com)");
    }
}

fn html_tag_name_eq(src: &[u8], mut i: usize, name: &[u8]) -> bool {
    // src[i] should be just after '<'
    while i < src.len() && (src[i] == b'/' || src[i] == b' ' || src[i] == b'\t') {
        i += 1;
    }
    let mut k = 0usize;
    while k < name.len() && i + k < src.len() {
        let a = src[i + k];
        let b = name[k];
        let al = if a >= b'A' && a <= b'Z' { a + 32 } else { a };
        let bl = if b >= b'A' && b <= b'Z' { b + 32 } else { b };
        if al != bl {
            return false;
        }
        k += 1;
    }
    if k != name.len() {
        return false;
    }
    // next char should be space, >, /, or end
    if i + k >= src.len() {
        return true;
    }
    let n = src[i + k];
    n == b'>' || n == b'/' || n == b' ' || n == b'\t' || n == b'\n' || n == b'\r'
}

fn html_to_text(src: &[u8], dst: &mut [u8]) -> usize {
    let mut i = 0usize;
    let mut o = 0usize;
    let mut in_tag = false;
    let mut skip_depth = 0i32; // style/script/head skip
    while i < src.len() && o < dst.len() {
        let c = src[i];
        if c == b'<' {
            // detect skip blocks: style, script, head, noscript
            if skip_depth == 0 {
                if html_tag_name_eq(src, i + 1, b"style")
                    || html_tag_name_eq(src, i + 1, b"script")
                    || html_tag_name_eq(src, i + 1, b"head")
                    || html_tag_name_eq(src, i + 1, b"noscript")
                {
                    skip_depth = 1;
                }
            } else {
                // closing tags reduce depth
                if i + 1 < src.len() && src[i + 1] == b'/' {
                    if html_tag_name_eq(src, i + 2, b"style")
                        || html_tag_name_eq(src, i + 2, b"script")
                        || html_tag_name_eq(src, i + 2, b"head")
                        || html_tag_name_eq(src, i + 2, b"noscript")
                    {
                        skip_depth = 0;
                    }
                }
            }
            // <br> / <p> / <div> / <li> / <h1-6> → newline-ish space
            let brk = html_tag_name_eq(src, i + 1, b"br")
                || html_tag_name_eq(src, i + 1, b"p")
                || html_tag_name_eq(src, i + 1, b"div")
                || html_tag_name_eq(src, i + 1, b"li")
                || html_tag_name_eq(src, i + 1, b"tr")
                || html_tag_name_eq(src, i + 1, b"h1")
                || html_tag_name_eq(src, i + 1, b"h2")
                || html_tag_name_eq(src, i + 1, b"h3");
            in_tag = true;
            i += 1;
            if brk && skip_depth == 0 && o < dst.len() {
                if o == 0 || dst[o - 1] != b'\n' {
                    dst[o] = b'\n';
                    o += 1;
                }
            }
            continue;
        }
        if c == b'>' {
            in_tag = false;
            i += 1;
            continue;
        }
        if !in_tag && skip_depth == 0 {
            if c == b'&' {
                let rest = src.len().saturating_sub(i);
                let mut ent_ch = 0u8;
                let mut skip_n = 0usize;
                if rest >= 5 && &src[i..i + 5] == b"&amp;" {
                    ent_ch = b'&'; skip_n = 5;
                } else if rest >= 4 && &src[i..i + 4] == b"&lt;" {
                    ent_ch = b'<'; skip_n = 4;
                } else if rest >= 4 && &src[i..i + 4] == b"&gt;" {
                    ent_ch = b'>'; skip_n = 4;
                } else if rest >= 6 && &src[i..i + 6] == b"&nbsp;" {
                    ent_ch = b' '; skip_n = 6;
                } else if rest >= 5 && &src[i..i + 5] == b"&quot;" {
                    ent_ch = b'"'; skip_n = 5;
                }
                if skip_n > 0 {
                    if !(ent_ch == b' ' && (o == 0 || dst[o - 1] == b' ' || dst[o - 1] == b'\n')) {
                        dst[o] = ent_ch;
                        o += 1;
                    }
                    i += skip_n;
                    continue;
                }
            }
            let ch = if c == b'\r' || c == b'\t' {
                b' '
            } else if c == b'\n' {
                b'\n'
            } else if c >= 32 && c < 127 {
                c
            } else {
                b' '
            };
            if ch == b' ' && (o == 0 || dst[o - 1] == b' ' || dst[o - 1] == b'\n') {
                i += 1;
                continue;
            }
            if ch == b'\n' && (o == 0 || dst[o - 1] == b'\n') {
                i += 1;
                continue;
            }
            dst[o] = ch;
            o += 1;
        }
        i += 1;
    }
    // trim trailing spaces
    while o > 0 && dst[o - 1] == b' ' {
        o -= 1;
    }
    o
}


/// Split URL into (host, path). Accepts "example.com", "http://example.com/x".
fn browser_parse_url(url: &[u8]) -> (usize, usize, usize, usize) {
    // returns host_start, host_len, path_start, path_len (path_start points into url)
    let mut s = 0usize;
    // skip scheme
    if url.len() >= 7 {
        let mut http = true;
        let sch = b"http://";
        let mut i = 0;
        while i < 7 {
            let a = url[i];
            let b = sch[i];
            let al = if a >= b'A' && a <= b'Z' { a + 32 } else { a };
            if al != b {
                http = false;
                break;
            }
            i += 1;
        }
        if http {
            s = 7;
        }
    }
    if url.len() >= 8 {
        let mut https = true;
        let sch = b"https://";
        let mut i = 0;
        while i < 8 {
            let a = url[i];
            let b = sch[i];
            let al = if a >= b'A' && a <= b'Z' { a + 32 } else { a };
            if al != b {
                https = false;
                break;
            }
            i += 1;
        }
        if https {
            s = 8;
        }
    }
    // host until / or end
    let mut h = s;
    while h < url.len() && url[h] != b'/' && url[h] != b'?' {
        h += 1;
    }
    let host_len = h.saturating_sub(s);
    if h < url.len() && url[h] == b'/' {
        let path_start = h;
        let path_len = url.len() - h;
        (s, host_len, path_start, path_len)
    } else {
        (s, host_len, 0, 0) // path default /
    }
}


fn html_extract_title(src: &[u8]) {
    unsafe {
        BR_TITLE_LEN = 0;
        let mut i = 0usize;
        while i + 7 < src.len() {
            // find <title
            if src[i] == b'<' && html_tag_name_eq(src, i + 1, b"title") {
                // skip to >
                while i < src.len() && src[i] != b'>' {
                    i += 1;
                }
                if i < src.len() {
                    i += 1;
                }
                let start = i;
                while i < src.len() {
                    if src[i] == b'<' {
                        break;
                    }
                    i += 1;
                }
                let mut o = 0usize;
                let mut j = start;
                while j < i && o < 47 {
                    let c = src[j];
                    if c >= 32 && c < 127 {
                        BR_TITLE[o] = c;
                        o += 1;
                    } else if c == b'\n' || c == b'\r' || c == b'\t' {
                        if o > 0 && BR_TITLE[o - 1] != b' ' {
                            BR_TITLE[o] = b' ';
                            o += 1;
                        }
                    }
                    j += 1;
                }
                BR_TITLE_LEN = o;
                return;
            }
            i += 1;
        }
    }
}

fn html_extract_links(src: &[u8]) {
    unsafe {
        BR_LINK_N = 0;
        let mut i = 0usize;
        while i + 10 < src.len() && BR_LINK_N < 8 {
            if src[i] == b'<' && html_tag_name_eq(src, i + 1, b"a") {
                // find href=
                let mut j = i + 2;
                let mut href_at = None;
                while j + 6 < src.len() && src[j] != b'>' {
                    let a = src[j];
                    let al = if a >= b'A' && a <= b'Z' { a + 32 } else { a };
                    if al == b'h'
                        && j + 5 < src.len()
                    {
                        let mut ok = true;
                        let tag = b"href=";
                        let mut k = 0usize;
                        while k < 5 {
                            let x = src[j + k];
                            let xl = if x >= b'A' && x <= b'Z' { x + 32 } else { x };
                            if xl != tag[k] {
                                ok = false;
                                break;
                            }
                            k += 1;
                        }
                        if ok {
                            href_at = Some(j + 5);
                            break;
                        }
                    }
                    j += 1;
                }
                if let Some(hs) = href_at {
                    let mut p = hs;
                    let quote = if p < src.len() && (src[p] == b'"' || src[p] == b'\'') {
                        let q = src[p];
                        p += 1;
                        q
                    } else {
                        0u8
                    };
                    let start = p;
                    while p < src.len() {
                        if quote != 0 {
                            if src[p] == quote {
                                break;
                            }
                        } else if src[p] == b' ' || src[p] == b'>' || src[p] == b'\n' {
                            break;
                        }
                        p += 1;
                    }
                    let mut len = p.saturating_sub(start);
                    if len > 95 {
                        len = 95;
                    }
                    // skip javascript: and #
                    if len > 0 {
                        let mut skip = false;
                        if len >= 11 {
                            let mut js = true;
                            let pref = b"javascript:";
                            let mut k = 0;
                            while k < 11 {
                                let x = src[start + k];
                                let xl = if x >= b'A' && x <= b'Z' { x + 32 } else { x };
                                if xl != pref[k] {
                                    js = false;
                                    break;
                                }
                                k += 1;
                            }
                            if js {
                                skip = true;
                            }
                        }
                        if src[start] == b'#' {
                            skip = true;
                        }
                        if !skip {
                            let n = BR_LINK_N;
                            let mut k = 0usize;
                            while k < len {
                                BR_LINK_HREF[n][k] = src[start + k];
                                k += 1;
                            }
                            while k < 96 {
                                BR_LINK_HREF[n][k] = 0;
                                k += 1;
                            }
                            BR_LINK_LEN[n] = len;
                            BR_LINK_N += 1;
                        }
                    }
                }
            }
            i += 1;
        }
    }
}

fn browser_open_link(idx: usize) {
    unsafe {
        if idx >= BR_LINK_N {
            return;
        }
        let len = BR_LINK_LEN[idx];
        if len == 0 {
            return;
        }
        // save back
        let mut i = 0usize;
        while i < BR_URL_LEN && i < 96 {
            BR_BACK[i] = BR_URL[i];
            i += 1;
        }
        BR_BACK_LEN = BR_URL_LEN;
        // resolve relative vs absolute
        let href = &BR_LINK_HREF[idx][..len];
        if len >= 7 {
            // absolute http
            let mut abs = true;
            let sch = b"http://";
            let mut k = 0;
            while k < 7 {
                let a = href[k];
                let al = if a >= b'A' && a <= b'Z' { a + 32 } else { a };
                if al != sch[k] {
                    abs = false;
                    break;
                }
                k += 1;
            }
            if abs {
                i = 0;
                while i < len && i < 95 {
                    BR_URL[i] = href[i];
                    i += 1;
                }
                BR_URL_LEN = if len > 95 { 95 } else { len };
                browser_go();
                return;
            }
        }
        // protocol-relative //host
        if len >= 2 && href[0] == b'/' && href[1] == b'/' {
            BR_URL[0] = b'h';
            BR_URL[1] = b't';
            BR_URL[2] = b't';
            BR_URL[3] = b'p';
            BR_URL[4] = b':';
            let mut i = 0usize;
            while i < len && i + 5 < 95 {
                BR_URL[5 + i] = href[i];
                i += 1;
            }
            BR_URL_LEN = 5 + (if len > 90 { 90 } else { len });
            browser_go();
            return;
        }
        // path-absolute /foo
        if len >= 1 && href[0] == b'/' {
            let (hs, hl, _, _) = browser_parse_url(&BR_URL[..BR_URL_LEN]);
            if hl == 0 {
                return;
            }
            let mut o = 0usize;
            // copy host only into new URL
            let mut k = 0usize;
            while k < hl && o < 95 {
                BR_URL[o] = BR_URL[hs + k];
                o += 1;
                k += 1;
            }
            k = 0;
            while k < len && o < 95 {
                BR_URL[o] = href[k];
                o += 1;
                k += 1;
            }
            BR_URL_LEN = o;
            browser_go();
            return;
        }
        // relative path: append to current directory
        let (hs, hl, ps, pl) = browser_parse_url(&BR_URL[..BR_URL_LEN]);
        let mut base = [0u8; 96];
        let mut bl = 0usize;
        // host
        let mut k = 0usize;
        while k < hl && bl < 90 {
            base[bl] = BR_URL[hs + k];
            bl += 1;
            k += 1;
        }
        // directory of current path
        if pl > 0 {
            let path = &BR_URL[ps..ps + pl];
            let mut last_slash = 0usize;
            let mut m = 0usize;
            while m < pl {
                if path[m] == b'/' {
                    last_slash = m;
                }
                m += 1;
            }
            m = 0;
            while m <= last_slash && m < pl && bl < 90 {
                base[bl] = path[m];
                bl += 1;
                m += 1;
            }
        } else {
            base[bl] = b'/';
            bl += 1;
        }
        k = 0;
        while k < len && bl < 95 {
            base[bl] = href[k];
            bl += 1;
            k += 1;
        }
        let mut i = 0usize;
        while i < bl {
            BR_URL[i] = base[i];
            i += 1;
        }
        BR_URL_LEN = bl;
        browser_go();
    }
}

fn browser_back() {
    unsafe {
        if BR_BACK_LEN == 0 {
            browser_set_status(b"No history");
            return;
        }
        let mut i = 0usize;
        while i < BR_BACK_LEN {
            BR_URL[i] = BR_BACK[i];
            i += 1;
        }
        BR_URL_LEN = BR_BACK_LEN;
        BR_BACK_LEN = 0;
        browser_go();
    }
}

fn browser_go() {
    unsafe {
        browser_ensure_url();
        // push previous URL to back (if different and non-empty)
        if BR_URL_LEN > 0 && BR_PAGE_LEN > 0 {
            let mut i = 0usize;
            while i < BR_URL_LEN && i < 96 {
                BR_BACK[i] = BR_URL[i];
                i += 1;
            }
            BR_BACK_LEN = BR_URL_LEN;
        }
        browser_set_status(b"Resolving...");
        DIRTY_UI = true;
        if !crate::drivers::net::dhcp_ok() {
            crate::drivers::net::init();
            let _ = crate::drivers::net::dhcp_run();
        }
        if !crate::drivers::net::dhcp_ok() {
            browser_set_status(b"No network (run HH first)");
            return;
        }
        let (hs, hl, ps, pl) = browser_parse_url(&BR_URL[..BR_URL_LEN]);
        if hl == 0 {
            browser_set_status(b"Empty host");
            return;
        }
        let host = &BR_URL[hs..hs + hl];
        let mut path_buf = [0u8; 128];
        let path: &[u8] = if pl > 0 && ps + pl <= BR_URL_LEN {
            let n = if pl > 127 { 127 } else { pl };
            let mut i = 0usize;
            while i < n {
                path_buf[i] = BR_URL[ps + i];
                i += 1;
            }
            &path_buf[..n]
        } else {
            b"/"
        };
        browser_set_status(b"DNS lookup...");
        DIRTY_UI = true;
        let ip = match crate::drivers::net::dns_resolve(host) {
            Some(a) => a,
            None => {
                browser_set_status(b"DNS failed (try HH, then example.com)");
                return;
            }
        };
        // show resolved IP in status briefly
        {
            let mut st = [0u8; 48];
            let mut p = 0usize;
            let msg = b"IP ";
            while p < msg.len() {
                st[p] = msg[p];
                p += 1;
            }
            let mut o = 0usize;
            while o < 4 {
                let mut v = ip[o] as usize;
                let mut dig = [0u8; 3];
                let mut d = 0usize;
                if v == 0 {
                    dig[0] = b'0';
                    d = 1;
                }
                while v > 0 && d < 3 {
                    dig[d] = b'0' + (v % 10) as u8;
                    v /= 10;
                    d += 1;
                }
                while d > 0 {
                    d -= 1;
                    st[p] = dig[d];
                    p += 1;
                }
                if o < 3 {
                    st[p] = b'.';
                    p += 1;
                }
                o += 1;
            }
            browser_set_status(&st[..p]);
            DIRTY_UI = true;
        }
        browser_set_status(b"HTTP GET...");
        DIRTY_UI = true;
        let mut raw = [0u8; 8192];
        // path always "/" for now; host string may still contain scheme - http_get uses it for Host: header
        let n = crate::drivers::net::http_get(ip, host, path, &mut raw);
        if n == 0 {
            let e = crate::drivers::net::last_err();
            let msg: &[u8] = match e {
                20 => b"HTTP: no SYN-ACK (silence)",
                24 => b"HTTP: TCP seen, no SYN-ACK",
                21 => b"HTTP: RST from server",
                22 => b"HTTP: GET send failed",
                23 => b"HTTP: empty body",
                12 => b"HTTP: SYN send failed",
                10 | 11 => b"HTTP: no network/ARP",
                _ => b"HTTP failed / empty",
            };
            browser_set_status(msg);
            BR_PAGE_LEN = 0;
            return;
        }
        // skip headers
        let mut body = 0usize;
        while body + 3 < n {
            if raw[body] == b'\r'
                && raw[body + 1] == b'\n'
                && raw[body + 2] == b'\r'
                && raw[body + 3] == b'\n'
            {
                body += 4;
                break;
            }
            body += 1;
        }

        // One-hop redirect: if 301/302 and Location host is same-style absolute http
        if n > 12 && raw[9] == b'3' && (raw[10] == b'0' || raw[10] == b'1' || raw[10] == b'2') {
            let mut loc = 0usize;
            while loc + 10 < body {
                // case-insensitive "Location:"
                let mut ok = true;
                let loc_tag = b"location:";
                let mut k = 0usize;
                while k < 9 && loc + k < n {
                    let a = raw[loc + k];
                    let al = if a >= b'A' && a <= b'Z' { a + 32 } else { a };
                    if al != loc_tag[k] {
                        ok = false;
                        break;
                    }
                    k += 1;
                }
                if ok && k == 9 {
                    // skip spaces
                    let mut v = loc + 9;
                    while v < body && (raw[v] == b' ' || raw[v] == b'\t') {
                        v += 1;
                    }
                    let vs = v;
                    while v < body && raw[v] != b'\r' && raw[v] != b'\n' {
                        v += 1;
                    }
                    if v > vs && v - vs < 95 {
                        let mut i = 0usize;
                        while i < v - vs {
                            BR_URL[i] = raw[vs + i];
                            i += 1;
                        }
                        BR_URL_LEN = v - vs;
                        browser_set_status(b"Redirect - press Go again");
                        DIRTY_UI = true;
                        return;
                    }
                    break;
                }
                loc += 1;
            }
        }

        BR_SCROLL = 0;
        html_extract_title(&raw[body..n]);
        html_extract_links(&raw[body..n]);
        BR_PAGE_LEN = html_to_text(&raw[body..n], &mut BR_PAGE);
        BR_FOCUS_URL = false;
        let mut st = [0u8; 48];
        let mut p = 0usize;
        let msg = b"OK bytes=";
        while p < msg.len() {
            st[p] = msg[p];
            p += 1;
        }
        let mut v = BR_PAGE_LEN;
        let mut dig = [0u8; 8];
        let mut d = 0usize;
        if v == 0 {
            dig[0] = b'0';
            d = 1;
        }
        while v > 0 && d < 8 {
            dig[d] = b'0' + (v % 10) as u8;
            v /= 10;
            d += 1;
        }
        while d > 0 {
            d -= 1;
            st[p] = dig[d];
            p += 1;
        }
        browser_set_status(&st[..p]);
    }
}

fn browser_count_lines(max_cols: usize) -> usize {
    unsafe {
        if max_cols == 0 {
            return 1;
        }
        let mut lines = 1usize;
        let mut col = 0usize;
        let mut i = 0usize;
        while i < BR_PAGE_LEN {
            let c = BR_PAGE[i];
            if c == b'\n' {
                lines += 1;
                col = 0;
            } else {
                col += 1;
                if col >= max_cols {
                    lines += 1;
                    col = 0;
                }
            }
            i += 1;
        }
        if lines == 0 {
            1
        } else {
            lines
        }
    }
}


fn browser_scroll_up() {
    unsafe {
        if BR_SCROLL > 0 {
            BR_SCROLL -= 1;
            DIRTY_UI = true;
        }
    }
}
fn browser_scroll_down() {
    unsafe {
        if BR_SCROLL + 1 < BR_TOTAL_LINES.max(1) {
            BR_SCROLL += 1;
            DIRTY_UI = true;
        }
    }
}
fn handle_browser_scroll_click(mx: i32, my: i32) -> bool {
    unsafe {
        if FOCUS >= MAX_WIN || !WINS[FOCUS].visible || WINS[FOCUS].kind != WinKind::Browser {
            return false;
        }
        let w = &WINS[FOCUS];
        let wx = w.x;
        let wy = w.y;
        let ww = w.w as usize;
        let wh = w.h as usize;
        let by = wy + TITLE_H;
        let ty = by + 40;
        let status_h = 22;
        let text_h = (wh as i32 - TITLE_H - 40 - status_h - 4).max(12);
        let sb_x = wx + ww as i32 - 18;
        let sb_y = ty;
        let sb_h = text_h;
        if mx < sb_x || mx >= sb_x + 12 || my < sb_y || my >= sb_y + sb_h {
            return false;
        }
        if my < sb_y + 14 {
            browser_scroll_up();
            return true;
        }
        if my >= sb_y + sb_h - 14 {
            browser_scroll_down();
            return true;
        }
        // click track: jump proportionally
        let track = (sb_h - 28).max(1);
        let rel = (my - sb_y - 14).max(0) as usize;
        let total = BR_TOTAL_LINES.max(1);
        let max_rows = ((text_h as usize) / 12).max(1);
        let max_off = total.saturating_sub(max_rows);
        if max_off > 0 {
            BR_SCROLL = rel * max_off / track as usize;
            if BR_SCROLL > max_off {
                BR_SCROLL = max_off;
            }
            DIRTY_UI = true;
        }
        true
    }
}

fn draw_browser(wx: usize, wy: usize, ww: usize, wh: usize) {
    unsafe {
        browser_ensure_url();
        let by = wy + TITLE_H as usize;
        let body_h = wh.saturating_sub(TITLE_H as usize + 3);
        graphics::fill_rect(wx + 3, by, ww - 6, body_h, 0x00FFFFFF);
        // toolbar
        graphics::fill_rect(wx + 3, by, ww - 6, 36, 0x00ECE9D8);
        graphics::draw_str(wx + 10, by + 12, "URL:", COL_TEXT);
        let url_w = ww.saturating_sub(160);
        graphics::fill_rect(wx + 44, by + 8, url_w, 22, 0x00FFFFFF);
        graphics::border_rect(wx + 44, by + 8, url_w, 22, 0x00808080);
        let mut i = 0usize;
        while i < BR_URL_LEN && i < (url_w.saturating_sub(8)) / 8 {
            graphics::draw_char(wx + 48 + i * 8, by + 12, BR_URL[i], COL_TEXT);
            i += 1;
        }
        let gx = wx + ww.saturating_sub(188);
        graphics::fill_rect(gx, by + 8, 32, 22, 0x00316AC5);
        graphics::draw_str(gx + 6, by + 12, "Go", 0x00FFFFFF);
        graphics::fill_rect(gx + 36, by + 8, 32, 22, 0x00D4D0C8);
        graphics::draw_str(gx + 40, by + 12, "Bk", COL_TEXT);
        graphics::fill_rect(gx + 72, by + 8, 36, 22, 0x00D4D0C8);
        graphics::draw_str(gx + 78, by + 12, "Ex.", COL_TEXT);
        graphics::fill_rect(gx + 112, by + 8, 40, 22, 0x00D4D0C8);
        graphics::draw_str(gx + 118, by + 12, "Org", COL_TEXT);

        // title line
        let title_y = by + 38;
        if BR_TITLE_LEN > 0 {
            graphics::draw_str(wx + 12, title_y, "Title:", 0x00666666);
            let mut ti = 0usize;
            while ti < BR_TITLE_LEN && ti < 50 {
                graphics::draw_char(wx + 60 + ti * 8, title_y, BR_TITLE[ti], 0x00000000);
                ti += 1;
            }
        }
        // text area geometry
        let ty = by + 54;
        let status_h = 22usize;
        let links_h = if BR_LINK_N > 0 { 16usize + BR_LINK_N.min(4) * 12 } else { 0usize };
        let text_h = wh.saturating_sub(TITLE_H as usize + 54 + status_h + links_h + 4);
        let max_cols = (ww.saturating_sub(36)) / 8;
        let max_rows = if text_h / 12 > 0 { text_h / 12 } else { 1 };
        let total = browser_count_lines(max_cols.max(1));
        BR_TOTAL_LINES = total;
        if BR_SCROLL >= total {
            BR_SCROLL = total.saturating_sub(1);
        }
        let skip = BR_SCROLL;

        // render wrapped text starting after `skip` lines
        let mut line = 0usize;
        let mut row = 0usize;
        let mut col = 0usize;
        i = 0;
        while i < BR_PAGE_LEN && row < max_rows {
            let c = BR_PAGE[i];
            if c == b'\n' {
                if line >= skip {
                    row += 1;
                    col = 0;
                }
                line += 1;
                i += 1;
                continue;
            }
            if line < skip {
                // still consume wrap for skipped lines
                col += 1;
                if col >= max_cols.max(1) {
                    line += 1;
                    col = 0;
                }
                i += 1;
                continue;
            }
            if col >= max_cols.max(1) {
                row += 1;
                col = 0;
                line += 1;
                if row >= max_rows {
                    break;
                }
            }
            graphics::draw_char(wx + 12 + col * 8, ty + row * 12, c, 0x00000000);
            col += 1;
            i += 1;
        }

        // scrollbar (right edge of page body)
        let sb_x = wx + ww.saturating_sub(18);
        let sb_y = ty;
        let sb_h = text_h.max(12);
        graphics::fill_rect(sb_x, sb_y, 12, sb_h, 0x00E8E8E8);
        graphics::border_rect(sb_x, sb_y, 12, sb_h, 0x00A0A0A0);
        // up / down buttons
        graphics::fill_rect(sb_x, sb_y, 12, 14, 0x00D0D0D0);
        graphics::draw_str(sb_x + 2, sb_y + 3, "^", COL_TEXT);
        graphics::fill_rect(sb_x, sb_y + sb_h.saturating_sub(14), 12, 14, 0x00D0D0D0);
        graphics::draw_str(sb_x + 2, sb_y + sb_h.saturating_sub(11), "v", COL_TEXT);
        // thumb
        if total > max_rows {
            let track = sb_h.saturating_sub(28);
            let thumb_h = (track * max_rows / total).max(8).min(track);
            let max_off = total.saturating_sub(max_rows);
            let thumb_y = if max_off == 0 {
                0
            } else {
                track * BR_SCROLL / max_off
            };
            graphics::fill_rect(sb_x + 1, sb_y + 14 + thumb_y, 10, thumb_h, 0x00316AC5);
        }

        // links strip (keys 1-8)
        if BR_LINK_N > 0 {
            let ly = wy + wh - status_h - links_h;
            graphics::fill_rect(wx + 3, ly, ww - 6, links_h, 0x00F5F5F0);
            graphics::draw_str(wx + 10, ly + 2, "Links (1-8):", 0x00316AC5);
            let mut li = 0usize;
            while li < BR_LINK_N && li < 4 {
                let mut lab = [0u8; 64];
                lab[0] = b'1' + li as u8;
                lab[1] = b')';
                lab[2] = b' ';
                let nlen = BR_LINK_LEN[li].min(50);
                let mut k = 0usize;
                while k < nlen {
                    lab[3 + k] = BR_LINK_HREF[li][k];
                    k += 1;
                }
                let mut c = 0usize;
                while c < 3 + nlen {
                    graphics::draw_char(wx + 10 + c * 8, ly + 14 + li * 12, lab[c], 0x00000000);
                    c += 1;
                }
                li += 1;
            }
        }
        // status bar with scroll info
        graphics::fill_rect(wx + 3, wy + wh - status_h, ww - 6, status_h - 4, 0x00D4D0C8);
        i = 0;
        while i < BR_STATUS_LEN {
            graphics::draw_char(wx + 10 + i * 8, wy + wh - 16, BR_STATUS[i], COL_TEXT);
            i += 1;
        }
        // "line X/Y" on the right of status
        let mut info = [0u8; 24];
        let mut p = 0usize;
        let put = |v: usize, buf: &mut [u8], p: &mut usize| {
            let mut x = v;
            let mut d = [0u8; 8];
            let mut n = 0usize;
            if x == 0 {
                d[0] = b'0';
                n = 1;
            } else {
                while x > 0 && n < 8 {
                    d[n] = b'0' + (x % 10) as u8;
                    x /= 10;
                    n += 1;
                }
            }
            while n > 0 {
                n -= 1;
                if *p < buf.len() {
                    buf[*p] = d[n];
                    *p += 1;
                }
            }
        };
        put(BR_SCROLL + 1, &mut info, &mut p);
        if p < 23 {
            info[p] = b'/';
            p += 1;
        }
        put(total, &mut info, &mut p);
        let ix = wx + ww.saturating_sub(10 + p * 8);
        let mut k = 0usize;
        while k < p {
            graphics::draw_char(ix + k * 8, wy + wh - 16, info[k], COL_TEXT);
            k += 1;
        }
    }
}


fn handle_key(ch: u8) {
    unsafe {
        if ch == 0x1B {
            if WIFI_UI_CONNECT_DIALOG {
                WIFI_UI_CONNECT_DIALOG = false;
                DIRTY_UI = true;
                return;
            }
            // Esc: let Explorer close its modal/context state before closing desktop menus.
            if FOCUS < MAX_WIN && WINS[FOCUS].visible && WINS[FOCUS].kind == WinKind::Files {
                if crate::files_mgr::on_escape() {
                    DIRTY_UI = true;
                    return;
                }
            }
            START_MENU = false;
            CTX_MENU = false;
            DIRTY_UI = true;
            return;
        }
        // Wi-Fi password dialog
        if WIFI_UI_CONNECT_DIALOG {
            if ch == b'\n' {
                if WIFI_UI_SELECTED >= 0 {
                    let ok = crate::drivers::wifi::connect(
                        WIFI_UI_SELECTED as usize,
                        &WIFI_UI_PASSWORD[..WIFI_UI_PASSWORD_LEN],
                    );
                    WIFI_UI_STATUS = if ok { 2 } else { 3 };
                }
                WIFI_UI_CONNECT_DIALOG = false;
                DIRTY_UI = true;
                return;
            }
            if ch == 0x08 {
                if WIFI_UI_PASSWORD_LEN > 0 {
                    WIFI_UI_PASSWORD_LEN -= 1;
                }
                DIRTY_UI = true;
                return;
            }
            if ch >= 32 && ch < 127 && WIFI_UI_PASSWORD_LEN < 63 {
                WIFI_UI_PASSWORD[WIFI_UI_PASSWORD_LEN] = ch;
                WIFI_UI_PASSWORD_LEN += 1;
                DIRTY_UI = true;
            }
            return;
        }
        // Route keys to the text-capable target:
        // when OSK (Keyboard window) has visual focus, use TEXT_FOCUS so typing
        // still goes into Terminal / Browser.
        let target = if FOCUS < MAX_WIN
            && WINS[FOCUS].visible
            && WINS[FOCUS].kind == WinKind::Keyboard
        {
            TEXT_FOCUS
        } else {
            FOCUS
        };
        // Browser URL field receives OSK / keyboard
        if target < MAX_WIN && WINS[target].visible && WINS[target].kind == WinKind::Browser {
            if ch == b'\n' {
                browser_go();
                DIRTY_UI = true;
                return;
            }
            if ch == 0x08 {
                if BR_FOCUS_URL && BR_URL_LEN > 0 {
                    BR_URL_LEN -= 1;
                    BR_URL[BR_URL_LEN] = 0;
                }
                DIRTY_UI = true;
                return;
            }
            if !BR_FOCUS_URL && ch >= b'1' && ch <= b'8' {
                browser_open_link((ch - b'1') as usize);
                DIRTY_UI = true;
                return;
            }
            if ch >= 32 && ch < 127 && BR_URL_LEN < 95 {
                BR_FOCUS_URL = true;
                BR_URL[BR_URL_LEN] = ch;
                BR_URL_LEN += 1;
                DIRTY_UI = true;
            }
            return;
        }
        if target >= MAX_WIN || !WINS[target].visible || WINS[target].kind != WinKind::Terminal {
            return;
        }
        if ch == b'\n' {
            terminal_write("Aether> ");
            let mut k = 0usize;
            while k < INPUT_LEN {
                term_putc(INPUT[k]);
                k += 1;
            }
            terminal_write("
");
            term_hist_push();
            let cmd_len = INPUT_LEN;
            let mut cmd_buf = [0u8; TERM_INPUT_MAX];
            k = 0;
            while k < cmd_len {
                cmd_buf[k] = INPUT[k];
                k += 1;
            }
            INPUT_LEN = 0;
            INPUT_CURSOR = 0;
            TERM_HIST_POS = 0;
            TERM_VIEW = 0;
            if cmd_len == 5
                && cmd_buf[0] == b'c'
                && cmd_buf[1] == b'l'
                && cmd_buf[2] == b'e'
                && cmd_buf[3] == b'a'
                && cmd_buf[4] == b'r'
            {
                term_clear();
                terminal_write("Aether Terminal ready.\n");
            } else if cmd_len == 3
                && cmd_buf[0] == b'c'
                && cmd_buf[1] == b'l'
                && cmd_buf[2] == b's'
            {
                term_clear();
                terminal_write("Aether Terminal ready.\n");
            } else if cmd_len > 0 {
                crate::shell::run_command_from_gui(&cmd_buf, cmd_len);
            }
            DIRTY_UI = true;
        } else if ch == 0x08 {
            if INPUT_CURSOR > 0 && INPUT_LEN > 0 {
                let mut i = INPUT_CURSOR - 1;
                while i + 1 < INPUT_LEN {
                    INPUT[i] = INPUT[i + 1];
                    i += 1;
                }
                if INPUT_LEN > 0 {
                    INPUT_LEN -= 1;
                }
                INPUT_CURSOR -= 1;
                DIRTY_UI = true;
            }
        } else if ch >= 32 && ch < 127 && INPUT_LEN < TERM_INPUT_MAX - 1 {
            if INPUT_CURSOR < INPUT_LEN {
                let mut i = INPUT_LEN;
                while i > INPUT_CURSOR {
                    INPUT[i] = INPUT[i - 1];
                    i -= 1;
                }
            }
            INPUT[INPUT_CURSOR] = ch;
            INPUT_LEN += 1;
            INPUT_CURSOR += 1;
            DIRTY_UI = true;
        }
    }
}

pub fn run() -> ! {
    serial::write_str("\n======== Aether Desktop v1.1 XP ========\n");
    crate::drivers::audio::init();
    crate::drivers::audio::play_startup_chime();
    serial::write_str("--- MOUSE DIAG ---\n");
    serial::write_str(if crate::drivers::ps2::diag_controller_ok() { "[PS2] controller OK\n" } else { "[PS2] controller FAIL\n" });
    serial::write_str(if crate::drivers::ps2::diag_ack_ok() { "[PS2] mouse ACK OK\n" } else { "[PS2] mouse ACK FAIL\n" });
    serial::write_str(if crate::drivers::ps2::diag_stream_ok() { "[PS2] streaming OK\n" } else { "[PS2] streaming FAIL\n" });
    serial::write_str("[PS2] packets=");
    serial::write_usize(crate::drivers::ps2::diag_packets() as usize);
    serial::write_str("\n");
    serial::write_str(if crate::drivers::xhci::diag_xhci_ok() { "[USB] xHCI OK\n" } else { "[USB] xHCI FAIL\n" });
    serial::write_str(if crate::drivers::xhci::diag_dev_found() || crate::drivers::xhci::diag_mouse_if() {
        "[USB] mouse FOUND\n"
    } else {
        "[USB] mouse NOT FOUND\n"
    });
    serial::write_str(if crate::drivers::xhci::diag_hid_ok() { "[USB] HID OK\n" } else { "[USB] HID FAIL\n" });
    serial::write_str("[USB] reports=");
    serial::write_usize(crate::drivers::xhci::diag_reports() as usize);
    serial::write_str("
");
    serial::write_str(if crate::drivers::ehci::diag_ok() { "[EHCI] OK
" } else { "[EHCI] FAIL
" });
    serial::write_str(if crate::drivers::ehci::diag_live() { "[EHCI] mouse LIVE
" } else { "[EHCI] mouse idle
" });
    serial::write_str("[EHCI] reports=");
    serial::write_usize(crate::drivers::ehci::diag_reports() as usize);
    serial::write_str("
");
    serial::write_str(if crate::drivers::uhci::diag_ok() { "[UHCI] OK
" } else { "[UHCI] FAIL
" });
    serial::write_str(if crate::drivers::uhci::diag_live() { "[UHCI] mouse LIVE
" } else { "[UHCI] mouse idle
" });
    serial::write_str("[UHCI] reports=");
    serial::write_usize(crate::drivers::uhci::diag_reports() as usize);
    serial::write_str("
");
    term_clear();
    terminal_write("Aether Desktop v1.1 XP\n");
    terminal_write("AUTOSTART: PCI/USB diag on serial\n");
    terminal_write("Re-run: type 1  then plug mouse\n");
    terminal_write("Aether Terminal ready. Type help.\n");
    unsafe {
        DIRTY_FULL = true;
        DIRTY_UI = false;
        DIRTY_CURSOR = false;
        MX = (graphics::width() / 2) as i32;
        MY = (graphics::height() / 2) as i32;
    }
    // Full first paint: wallpaper + icons + taskbar + windows + cursor
    render();
    unsafe {
        DIRTY_FULL = false;
        DIRTY_UI = false;
        DIRTY_CURSOR = false;
    }

    loop {
        unsafe { LOOP_TICK = LOOP_TICK.wrapping_add(1); }
        crate::time::tick();
        ps2::poll();
        // Keep HDA DMA buffer fed while Media Player is running
        crate::drivers::audio::playback_poll();
        // Alarm: poll only; never open windows mid-poll (avoids re-entrancy)
        crate::alarm::poll();
        // Wi-Fi UI: optional one-shot enable, then scan
        unsafe {
            if WIFI_UI_ENABLE_REQ {
                WIFI_UI_ENABLE_REQ = false;
                let _ = crate::drivers::wifi::bring_up();
                DIRTY_UI = true;
            }
            if WIFI_UI_SCAN_REQUESTED {
                WIFI_UI_SCAN_REQUESTED = false;
                let n = crate::drivers::wifi::scan();
                WIFI_UI_STATUS = if n > 0 { 0 } else { 3 }; // 3=error/empty after attempt
                DIRTY_UI = true;
            }
        }
        unsafe {
            if crate::alarm::is_ringing() && MAX_WIN > 11 && !WINS[11].visible {
                open_win(11);
                DIRTY_FULL = true; // full paint so panel/cursor stay consistent
            }
        }
        let (pmx, pmy) = ps2::mouse_pos();
        let pbtn = ps2::mouse_buttons();
        unsafe {
            if pmx != MX || pmy != MY {
                MX = pmx;
                MY = pmy;
                let sw = graphics::width() as i32;
                let sh = graphics::height() as i32;
                if MX < 0 { MX = 0; }
                if MY < 0 { MY = 0; }
                if MX >= sw { MX = sw - 1; }
                if MY >= sh { MY = sh - 1; }
                if DRAGGING && (MB & 1) != 0 {
                    WINS[DRAG_WIN].x = MX - DRAG_OX;
                    WINS[DRAG_WIN].y = MY - DRAG_OY;
                    clamp_win(DRAG_WIN);
                    render_drag_step(); // dirty region, not full fill
                } else {
                    DIRTY_CURSOR = true;
                }
            }
            if pbtn != MB {
                handle_mouse_buttons(pbtn);
            }
        }

        while let Some(ev) = input::poll_mouse() {
            apply_mouse_delta(ev.dx as i32, ev.dy as i32);
            unsafe {
                if DRAGGING && (ev.buttons & 1) != 0 {
                    WINS[DRAG_WIN].x = MX - DRAG_OX;
                    WINS[DRAG_WIN].y = MY - DRAG_OY;
                    clamp_win(DRAG_WIN);
                }
            }
            handle_mouse_buttons(ev.buttons);
        }

        // Drain up to 16 scancodes (bound - never hang the UI loop).
        let mut sc_n = 0u32;
        while sc_n < 16 {
            let sc = ps2::last_scancode();
            if sc == 0 {
                break;
            }
            sc_n += 1;
            let files_focused = unsafe {
                FOCUS < MAX_WIN
                    && WINS[FOCUS].visible
                    && !WINS[FOCUS].minimized
                    && (WINS[FOCUS].kind == WinKind::Files
                        || WINS[FOCUS].kind == WinKind::MyComputer)
            };
            let code = sc & 0x7F;
            let is_break = (sc & 0x80) != 0;
            if !is_break && files_focused {
                // Always store raw scancode → status shows SC=xx after redraw
                crate::files_mgr::note_scancode(code);
                let is_nav = matches!(
                    code,
                    0x48 | 0x50 | 0x4B | 0x4D | 0x49 | 0x51 | 0x47 | 0x4F
                    | 0x4E | 0x4A | 0x1C | 0x0E
                );
                if is_nav {
                    let _ = crate::files_mgr::on_nav_key(code);
                }
                unsafe {
                    let f = FOCUS;
                    if f < MAX_WIN {
                        redraw_files_after_click(f);
                    }
                }
            } else if !is_break && matches!(code, 0x48 | 0x50 | 0x4B | 0x4D | 0x49 | 0x51 | 0x47 | 0x4F) {
                match code {
                    0x48 | 0x49 | 0x47 => term_scroll_up(),
                    0x50 | 0x51 | 0x4F => term_scroll_down(),
                    _ => {}
                }
            } else if !is_break {
                if let Some(ch) = ps2::scancode_to_ascii(sc) {
                    handle_key(ch);
                }
            }
        }
        while let Some(ev) = input::poll() {
            if ev.pressed {
                if handle_special_key(ev.hid_code) {
                    continue;
                }
                if ev.key != 0 {
                    handle_key(ev.key);
                }
            }
        }

        crate::drivers::xhci::poll_mouse_live();
        crate::drivers::ehci::poll_mouse_live();
        crate::drivers::uhci::poll_mouse_live();

        static mut CLOCK_TICK: u32 = 0;
        unsafe {
            CLOCK_TICK += 1;
            // Clock strip only when second changes - no full-screen fill
            if CLOCK_TICK >= 800 {
                CLOCK_TICK = 0;
                let (_y, _mo, _d, _h, _mi, s) = crate::time::rtc_read();
                if s != LAST_SEC {
                    LAST_SEC = s;
                    cursor_restore();
                    redraw_status_strip();
                    cursor_save_and_draw(MX, MY);
                    crate::graphics::present_full_fast();
                }
            }
            // Video player software frames
            crate::video_player::tick();
            if crate::video_player::needs_redraw() {
                crate::video_player::clear_redraw();
                // Repaint movie window only when visible
                let mut mi = 0usize;
                while mi < MAX_WIN {
                    if WINS[mi].visible && WINS[mi].kind == WinKind::Movie {
                        cursor_restore();
                        draw_window(mi);
                        cursor_save_and_draw(MX, MY);
                        crate::graphics::present_full_fast();
                        break;
                    }
                    mi += 1;
                }
            }
            if DIRTY_FULL {
                CURSOR_SAVED = false;
                render();
                // present BB → LFB once per full frame
                crate::graphics::present_full();
                DIRTY_FULL = false;
                DIRTY_UI = false;
                DIRTY_CURSOR = false;
            } else if DIRTY_UI {
                CURSOR_SAVED = false;
                render_ui_only();
                crate::graphics::present_full_fast();
                DIRTY_UI = false;
                DIRTY_CURSOR = false;
            } else if DIRTY_CURSOR {
                let ox = CURSOR_OX;
                let oy = CURSOR_OY;
                cursor_save_and_draw(MX, MY);
                // present old + new cursor rects only
                if ox >= 0 && oy >= 0 {
                    crate::graphics::present_rect(
                        ox as usize, oy as usize, CURSOR_W + 2, CURSOR_H + 2,
                    );
                }
                crate::graphics::present_rect(
                    MX.max(0) as usize, MY.max(0) as usize, CURSOR_W + 2, CURSOR_H + 2,
                );
                DIRTY_CURSOR = false;
            }
        }
        let mut d = 0u32;
        while d < 200 {
            d += 1;
        }
    }
}
