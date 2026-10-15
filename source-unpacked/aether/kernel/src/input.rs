//! Input layer - PS/2 + USB HID Boot Keyboard

#[derive(Clone, Copy)]
pub struct KeyEvent {
    pub key: u8,       // ASCII or special
    pub pressed: bool,
    pub hid_code: u8,
    pub modifiers: u8,
}

const QUEUE_SIZE: usize = 32;
static mut QUEUE: [KeyEvent; QUEUE_SIZE] = [KeyEvent { key: 0, pressed: false, hid_code: 0, modifiers: 0 }; QUEUE_SIZE];
static mut Q_HEAD: usize = 0;
static mut Q_TAIL: usize = 0;
static mut LAST_MOD: u8 = 0;
static mut LAST_KEYS: [u8; 6] = [0; 6];

pub fn push_event(ev: KeyEvent) {
    unsafe {
        let next = (Q_TAIL + 1) % QUEUE_SIZE;
        if next != Q_HEAD {
            QUEUE[Q_TAIL] = ev;
            Q_TAIL = next;
        }
    }
}

pub fn poll() -> Option<KeyEvent> {
    unsafe {
        if Q_HEAD == Q_TAIL { return None; }
        let ev = QUEUE[Q_HEAD];
        Q_HEAD = (Q_HEAD + 1) % QUEUE_SIZE;
        Some(ev)
    }
}

/// HID Boot Keyboard usage ID → ASCII (US layout, rough)
pub fn hid_to_ascii(code: u8, shift: bool) -> Option<u8> {
    let c = match code {
        0x04 => b'a', 0x05 => b'b', 0x06 => b'c', 0x07 => b'd', 0x08 => b'e',
        0x09 => b'f', 0x0A => b'g', 0x0B => b'h', 0x0C => b'i', 0x0D => b'j',
        0x0E => b'k', 0x0F => b'l', 0x10 => b'm', 0x11 => b'n', 0x12 => b'o',
        0x13 => b'p', 0x14 => b'q', 0x15 => b'r', 0x16 => b's', 0x17 => b't',
        0x18 => b'u', 0x19 => b'v', 0x1A => b'w', 0x1B => b'x', 0x1C => b'y',
        0x1D => b'z',
        0x1E => b'1', 0x1F => b'2', 0x20 => b'3', 0x21 => b'4', 0x22 => b'5',
        0x23 => b'6', 0x24 => b'7', 0x25 => b'8', 0x26 => b'9', 0x27 => b'0',
        0x28 => b'\n', // Enter
        0x2A => 0x08,  // Backspace
        0x2C => b' ',
        0x2D => b'-', 0x2E => b'=', 0x2F => b'[', 0x30 => b']',
        0x33 => b';', 0x34 => b'\'', 0x36 => b',', 0x37 => b'.', 0x38 => b'/',
        _ => 0,
    };
    if c == 0 { return None; }
    let mut ch = c;
    if shift && ch >= b'a' && ch <= b'z' { ch -= 32; }
    Some(ch)
}

/// Process 8-byte Boot Keyboard report; emit press/release events
pub fn process_hid_boot_report(report: &[u8; 8]) {
    let modifiers = report[0];
    let shift = (modifiers & 0x22) != 0; // L/R shift
    let mut new_keys = [0u8; 6];
    let mut i = 0usize;
    while i < 6 {
        new_keys[i] = report[2 + i];
        i += 1;
    }

    unsafe {
        // Releases: in LAST_KEYS but not in new_keys
        i = 0;
        while i < 6 {
            let old = LAST_KEYS[i];
            if old != 0 {
                let mut found = false;
                let mut j = 0usize;
                while j < 6 {
                    if new_keys[j] == old { found = true; break; }
                    j += 1;
                }
                if !found {
                    let ascii = hid_to_ascii(old, (LAST_MOD & 0x22) != 0).unwrap_or(0);
                    push_event(KeyEvent { key: ascii, pressed: false, hid_code: old, modifiers: LAST_MOD });
                }
            }
            i += 1;
        }
        // Presses: in new_keys but not in LAST_KEYS
        i = 0;
        while i < 6 {
            let nk = new_keys[i];
            if nk != 0 {
                let mut found = false;
                let mut j = 0usize;
                while j < 6 {
                    if LAST_KEYS[j] == nk { found = true; break; }
                    j += 1;
                }
                if !found {
                    let ascii = hid_to_ascii(nk, shift).unwrap_or(0);
                    push_event(KeyEvent { key: ascii, pressed: true, hid_code: nk, modifiers });
                }
            }
            i += 1;
        }
        LAST_MOD = modifiers;
        LAST_KEYS = new_keys;
    }
}

pub fn set_last(_sc: u8) {}
pub fn last() -> u8 { 0 }

#[derive(Clone, Copy)]
pub struct MouseEvent {
    pub dx: i8,
    pub dy: i8,
    pub buttons: u8, // bit0 left, bit1 right
}

static mut MOUSE_Q: [MouseEvent; 32] = [MouseEvent { dx: 0, dy: 0, buttons: 0 }; 32];
static mut MQ_H: usize = 0;
static mut MQ_T: usize = 0;
static mut LAST_BTN: u8 = 0;

pub fn push_mouse(dx: i8, dy: i8, buttons: u8) {
    unsafe {
        let next = (MQ_T + 1) % 32;
        if next != MQ_H {
            MOUSE_Q[MQ_T] = MouseEvent { dx, dy, buttons };
            MQ_T = next;
        }
        LAST_BTN = buttons;
    }
}

pub fn poll_mouse() -> Option<MouseEvent> {
    unsafe {
        if MQ_H == MQ_T {
            return None;
        }
        let e = MOUSE_Q[MQ_H];
        MQ_H = (MQ_H + 1) % 32;
        Some(e)
    }
}

pub fn mouse_buttons() -> u8 {
    unsafe { LAST_BTN }
}

/// USB HID Boot Mouse report: buttons, dx, dy [, wheel]
pub fn process_hid_boot_mouse(report: &[u8]) {
    if report.len() < 3 {
        return;
    }
    let buttons = report[0] & 0x07;
    let dx = report[1] as i8;
    let dy = report[2] as i8;
    // USB HID Y often inverted relative to screen
    push_mouse(dx, -dy, buttons);
}
