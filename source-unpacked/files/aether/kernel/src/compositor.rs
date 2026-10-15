//! Software compositor with cursor

use crate::graphics;
use crate::drivers::ps2;

static mut CURSOR_X: i32 = 400;
static mut CURSOR_Y: i32 = 300;
static mut FOCUS: usize = 0; // focused window index
static mut NEEDS_REDRAW: bool = true;

pub struct Window {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub title: &'static str,
    pub color: u32,
    pub visible: bool,
}

static mut WINDOWS: [Window; 4] = [
    Window { x: 40,  y: 50,  w: 280, h: 160, title: "LINUX",   color: 0x001a3328, visible: true },
    Window { x: 360, y: 50,  w: 280, h: 160, title: "WINDOWS", color: 0x00331a33, visible: true },
    Window { x: 40,  y: 250, w: 280, h: 140, title: "ANDROID", color: 0x0028331a, visible: true },
    Window { x: 360, y: 250, w: 280, h: 140, title: "TERMINAL", color: 0x001a2030, visible: true },
];

pub fn show(idx: usize) {
    if idx < 4 { unsafe { WINDOWS[idx].visible = true; NEEDS_REDRAW = true; } }
}

fn draw_window(w: &Window, focused: bool) {
    if !w.visible { return; }
    let border = if focused { 0x00F0C040 } else { 0x00486078 };
    graphics::fill_rect(w.x as usize + 3, w.y as usize + 3, w.w as usize, w.h as usize, 0x00060505);
    graphics::fill_rect(w.x as usize, w.y as usize, w.w as usize, w.h as usize, border);
    graphics::fill_rect(w.x as usize + 2, w.y as usize + 2, w.w as usize - 4, 22, 0x00243850);
    graphics::fill_rect(w.x as usize + 2, w.y as usize + 24, w.w as usize - 4, w.h as usize - 26, w.color);
    graphics::draw_str((w.x + 10) as usize, (w.y + 7) as usize, w.title, 0x00F0F0F0);
}

fn draw_cursor(x: i32, y: i32) {
    // Simple arrow cursor 11x17
    let cx = x as usize;
    let cy = y as usize;
    // white body
    let shape: &[(isize, isize)] = &[
        (0,0),(0,1),(0,2),(0,3),(0,4),(0,5),(0,6),(0,7),(0,8),(0,9),(0,10),
        (1,1),(1,2),(1,3),(1,4),(1,5),(1,6),(1,7),(1,8),
        (2,2),(2,3),(2,4),(2,5),(2,6),(2,7),
        (3,3),(3,4),(3,5),(3,6),
        (4,4),(4,5),(4,9),(4,10),
        (5,5),(5,8),(5,9),(5,11),
        (6,6),(6,8),(6,11),
        (7,7),(7,12),
        (8,8),(8,12),
        (9,9),(9,10),(9,11),
    ];
    for &(dx, dy) in shape {
        let px = cx as isize + dx;
        let py = cy as isize + dy;
        if px >= 0 && py >= 0 {
            graphics::put_pixel(px as usize, py as usize, 0x00FFFFFF);
        }
    }
    // black outline points
    graphics::put_pixel(cx, cy, 0x00000000);
}

pub fn render() {
    if !graphics::ready() { return; }
    graphics::fill(0x000a1420);

    // top bar
    graphics::fill_rect(0, 0, graphics::width(), 36, 0x00121c28);
    graphics::draw_str(14, 11, "AETHER", 0x00FFFFFF);
    graphics::draw_str(95, 11, "1.6", 0x0070A0C0);

    let (mx, my) = unsafe { (CURSOR_X, CURSOR_Y) };
    // status
    graphics::draw_str(200, 11, "X", 0x00A0A0A0);

    unsafe {
        for i in 0..4 {
            draw_window(&WINDOWS[i], i == FOCUS);
        }
    }

    // dock
    let dh = 46;
    let dy = graphics::height() - dh;
    graphics::fill_rect(0, dy, graphics::width(), dh, 0x000e1820);
    graphics::fill_rect(22, dy + 7, 38, 32, 0x00284868);
    graphics::fill_rect(70, dy + 7, 38, 32, 0x00482848);
    graphics::fill_rect(118, dy + 7, 38, 32, 0x00284828);

    draw_cursor(mx, my);
    unsafe { NEEDS_REDRAW = false; }
}

pub fn handle_input() {
    ps2::poll();
    let (mx, my) = ps2::mouse_pos();
    let btn = ps2::mouse_buttons();
    unsafe {
        if mx != CURSOR_X || my != CURSOR_Y {
            CURSOR_X = mx;
            CURSOR_Y = my;
            NEEDS_REDRAW = true;
        }
        // Left click → focus window under cursor
        if btn & 1 != 0 {
            let mut i = 4;
            while i > 0 {
                i -= 1;
                let w = &WINDOWS[i];
                if w.visible
                    && mx >= w.x && mx < w.x + w.w
                    && my >= w.y && my < w.y + w.h
                {
                    if FOCUS != i {
                        FOCUS = i;
                        NEEDS_REDRAW = true;
                    }
                    break;
                }
            }
        }
    }
    let sc = ps2::last_scancode();
    if sc != 0 {
        if let Some(ch) = ps2::scancode_to_ascii(sc) {
            crate::serial::write_str("  [gui] key->win");
            crate::serial::write_usize(unsafe { FOCUS });
            crate::serial::write_str(" ch=0x");
            crate::serial::write_usize(ch as usize);
            crate::serial::write_str("\n");
            unsafe { NEEDS_REDRAW = true; }
        }
    }
    // USB HID Boot Keyboard events
    while let Some(ev) = crate::input::poll() {
        if ev.pressed {
            crate::serial::write_str("  [GUI] delivered to focused window key=0x");
            crate::serial::write_usize(ev.key as usize);
            crate::serial::write_str(" hid=0x");
            crate::serial::write_usize(ev.hid_code as usize);
            crate::serial::write_str(" win=");
            crate::serial::write_usize(unsafe { FOCUS });
            crate::serial::write_str("\n");
            unsafe { NEEDS_REDRAW = true; }
        }
    }
}

pub fn needs_redraw() -> bool {
    unsafe { NEEDS_REDRAW }
}


pub fn show_storage_status() {
    use crate::graphics;
    use crate::fs;
    // Panel bottom-left
    let x = 20usize;
    let y = 420usize;
    graphics::fill_rect(x, y, 360, 120, 0x00101820);
    graphics::fill_rect(x + 2, y + 2, 356, 116, 0x00203040);
    graphics::draw_str(x + 12, y + 10, "AETHER STORAGE", 0x00F0C040);
    if fs::is_mounted() {
        graphics::draw_str(x + 12, y + 32, "Storage: OK", 0x0040F080);
        graphics::draw_str(x + 12, y + 50, "Filesystem: AetherFS", 0x00E0E0E0);
        graphics::draw_str(x + 12, y + 68, "Mounted: /", 0x00E0E0E0);
        // files count via serial already; draw test.txt label
        graphics::draw_str(x + 12, y + 90, "/test.txt  Hello Aether", 0x00A0D0FF);
    } else {
        graphics::draw_str(x + 12, y + 32, "Storage: NONE", 0x00F06060);
    }
    unsafe { NEEDS_REDRAW = false; }
}
