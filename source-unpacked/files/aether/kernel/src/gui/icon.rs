//! Aether icon resource system - original XP-inspired pixel art (not MS assets)
use crate::graphics;

pub const ICON_W: usize = 32;
pub const ICON_H: usize = 32;

#[derive(Clone, Copy, PartialEq)]
pub enum IconId {
    MyComputer = 0,
    MyDocuments = 1,
    RecycleBin = 2,
    Network = 3,
    Folder = 4,
    File = 5,
    Terminal = 6,
    Settings = 7,
    Start = 8,
    Count = 9,
}

/// RGBA8888 packed as 0xAARRGGBB - AA=0 transparent
fn px(buf: &mut [u32; ICON_W * ICON_H], x: usize, y: usize, c: u32) {
    if x < ICON_W && y < ICON_H {
        buf[y * ICON_W + x] = c;
    }
}
fn fill(buf: &mut [u32; ICON_W * ICON_H], x0: usize, y0: usize, w: usize, h: usize, c: u32) {
    let mut y = y0;
    while y < y0 + h && y < ICON_H {
        let mut x = x0;
        while x < x0 + w && x < ICON_W {
            px(buf, x, y, c);
            x += 1;
        }
        y += 1;
    }
}
fn rect(buf: &mut [u32; ICON_W * ICON_H], x0: usize, y0: usize, w: usize, h: usize, c: u32) {
    let mut x = x0;
    while x < x0 + w {
        px(buf, x, y0, c);
        if h > 0 {
            px(buf, x, y0 + h - 1, c);
        }
        x += 1;
    }
    let mut y = y0;
    while y < y0 + h {
        px(buf, x0, y, c);
        if w > 0 {
            px(buf, x0 + w - 1, y, c);
        }
        y += 1;
    }
}

fn gen_computer(buf: &mut [u32; ICON_W * ICON_H]) {
    // CRT monitor + tower - classic XP computer silhouette
    fill(buf, 4, 4, 20, 16, 0xFFC0C0C0);
    fill(buf, 6, 6, 16, 12, 0xFF204080);
    fill(buf, 8, 8, 12, 8, 0xFF60A0E0);
    fill(buf, 10, 20, 8, 2, 0xFF808080);
    fill(buf, 8, 22, 12, 3, 0xFFA0A0A0);
    // tower
    fill(buf, 24, 6, 6, 18, 0xFFE8E8E8);
    rect(buf, 24, 6, 6, 18, 0xFF606060);
    fill(buf, 25, 8, 4, 2, 0xFF202020);
    fill(buf, 25, 12, 4, 1, 0xFF00A000);
    fill(buf, 25, 14, 4, 1, 0xFF808080);
    rect(buf, 4, 4, 20, 16, 0xFF404040);
}

fn gen_documents(buf: &mut [u32; ICON_W * ICON_H]) {
    // folder with document
    fill(buf, 4, 10, 24, 16, 0xFFE8B84A);
    fill(buf, 4, 8, 10, 4, 0xFFE8B84A);
    fill(buf, 5, 11, 22, 14, 0xFFF0D078);
    rect(buf, 4, 8, 24, 18, 0xFFA07820);
    // paper
    fill(buf, 12, 4, 12, 14, 0xFFFFFFF0);
    rect(buf, 12, 4, 12, 14, 0xFF808080);
    fill(buf, 14, 7, 8, 1, 0xFFC0C0C0);
    fill(buf, 14, 10, 8, 1, 0xFFC0C0C0);
    fill(buf, 14, 13, 6, 1, 0xFFC0C0C0);
}

fn gen_recycle(buf: &mut [u32; ICON_W * ICON_H]) {
    // recycle bin can
    fill(buf, 8, 8, 16, 20, 0xFFC8D0D8);
    rect(buf, 8, 8, 16, 20, 0xFF608090);
    fill(buf, 6, 6, 20, 4, 0xFFA0B0C0);
    rect(buf, 6, 6, 20, 4, 0xFF406070);
    // recycle arrows simplified
    fill(buf, 12, 12, 3, 8, 0xFF208020);
    fill(buf, 17, 14, 3, 8, 0xFF208020);
    fill(buf, 14, 18, 6, 2, 0xFF208020);
}

fn gen_network(buf: &mut [u32; ICON_W * ICON_H]) {
    // two PCs + cable
    fill(buf, 2, 6, 12, 10, 0xFFC0C0C0);
    fill(buf, 4, 8, 8, 6, 0xFF2060A0);
    fill(buf, 18, 14, 12, 10, 0xFFC0C0C0);
    fill(buf, 20, 16, 8, 6, 0xFF2060A0);
    // cable
    fill(buf, 14, 12, 4, 2, 0xFF404040);
    fill(buf, 16, 12, 2, 6, 0xFF404040);
}

fn gen_folder(buf: &mut [u32; ICON_W * ICON_H]) {
    fill(buf, 3, 10, 26, 16, 0xFFE8C040);
    fill(buf, 3, 8, 12, 4, 0xFFE8C040);
    fill(buf, 4, 11, 24, 14, 0xFFF0D060);
    rect(buf, 3, 8, 26, 18, 0xFFA08020);
}

fn gen_file(buf: &mut [u32; ICON_W * ICON_H]) {
    fill(buf, 8, 2, 16, 26, 0xFFFFFFF8);
    rect(buf, 8, 2, 16, 26, 0xFF606060);
    // folded corner
    fill(buf, 18, 2, 6, 6, 0xFFE0E0E0);
    fill(buf, 10, 10, 12, 1, 0xFFA0A0A0);
    fill(buf, 10, 14, 12, 1, 0xFFA0A0A0);
    fill(buf, 10, 18, 10, 1, 0xFFA0A0A0);
}

fn gen_terminal(buf: &mut [u32; ICON_W * ICON_H]) {
    fill(buf, 2, 4, 28, 22, 0xFF202020);
    rect(buf, 2, 4, 28, 22, 0xFF808080);
    fill(buf, 4, 6, 24, 16, 0xFF001800);
    // prompt
    fill(buf, 6, 10, 2, 2, 0xFF00FF00);
    fill(buf, 10, 10, 8, 2, 0xFF00CC00);
    fill(buf, 6, 14, 2, 2, 0xFF00FF00);
}

fn gen_settings(buf: &mut [u32; ICON_W * ICON_H]) {
    // gear-ish
    fill(buf, 10, 4, 12, 4, 0xFF808890);
    fill(buf, 10, 24, 12, 4, 0xFF808890);
    fill(buf, 4, 10, 4, 12, 0xFF808890);
    fill(buf, 24, 10, 4, 12, 0xFF808890);
    fill(buf, 8, 8, 16, 16, 0xFFA0A8B0);
    fill(buf, 12, 12, 8, 8, 0xFF4060A0);
    rect(buf, 12, 12, 8, 8, 0xFF203060);
}

fn gen_start(buf: &mut [u32; ICON_W * ICON_H]) {
    // green orb-ish start
    fill(buf, 6, 6, 20, 20, 0xFF3C9A30);
    fill(buf, 8, 8, 16, 16, 0xFF50B848);
    fill(buf, 10, 10, 12, 12, 0xFF70D060);
    // flag-like white
    fill(buf, 12, 12, 8, 3, 0xFFFFFFF0);
    fill(buf, 12, 16, 5, 3, 0xFFFFFFF0);
}

pub fn generate(id: IconId) -> [u32; ICON_W * ICON_H] {
    let mut buf = [0u32; ICON_W * ICON_H];
    match id {
        IconId::MyComputer => gen_computer(&mut buf),
        IconId::MyDocuments => gen_documents(&mut buf),
        IconId::RecycleBin => gen_recycle(&mut buf),
        IconId::Network => gen_network(&mut buf),
        IconId::Folder => gen_folder(&mut buf),
        IconId::File => gen_file(&mut buf),
        IconId::Terminal => gen_terminal(&mut buf),
        IconId::Settings => gen_settings(&mut buf),
        IconId::Start => gen_start(&mut buf),
        IconId::Count => {}
    }
    buf
}

/// Blit icon with alpha (AA in high byte). selected draws blue selection behind.
pub fn blit(id: IconId, dx: usize, dy: usize, selected: bool) {
    if selected {
        graphics::fill_rect(dx.saturating_sub(2), dy.saturating_sub(2), ICON_W + 4, ICON_H + 4, 0x00316AC5);
    }
    let img = generate(id);
    let mut y = 0usize;
    while y < ICON_H {
        let mut x = 0usize;
        while x < ICON_W {
            let c = img[y * ICON_W + x];
            let a = (c >> 24) & 0xFF;
            if a > 0 {
                // strip alpha for put_pixel (opaque)
                graphics::put_pixel(dx + x, dy + y, c & 0x00FFFFFF);
            }
            x += 1;
        }
        y += 1;
    }
}

pub fn label(id: IconId) -> &'static str {
    match id {
        IconId::MyComputer => "My Computer",
        IconId::MyDocuments => "My Documents",
        IconId::RecycleBin => "Recycle Bin",
        IconId::Network => "Network",
        IconId::Folder => "Folder",
        IconId::File => "File",
        IconId::Terminal => "Terminal",
        IconId::Settings => "Settings",
        IconId::Start => "start",
        IconId::Count => "",
    }
}
