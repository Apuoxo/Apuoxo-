//! Intel GMBUS EDID - Sandy Bridge (AH532 DID 0x0116, Gen6).
//! Manual `dv` only. Tight waits, one forcewake per session.
//! ERR=2 = timeout (no HW_RDY). Tries pins × rates × xfer styles.

use crate::drivers::intel_kms;
use crate::serial;

const GMBUS0: usize = 0xC5100; // PCH SNB (not 0x5100)
const GMBUS1: usize = 0xC5104;
const GMBUS2: usize = 0xC5108;
const GMBUS3: usize = 0xC510C;
const GMBUS4: usize = 0xC5110;
const GMBUS5: usize = 0xC5120;

// GMBUS0 rate field [9:8]
const RATE_100KHZ: u32 = 0 << 8;
const RATE_50KHZ: u32 = 1 << 8;
const RATE_400KHZ: u32 = 2 << 8;

const PIN_SSC: u32 = 1;
const PIN_VGADDC: u32 = 2;
const PIN_PANEL: u32 = 3;
const PIN_DPC: u32 = 4;
const PIN_DPB: u32 = 5;
const PIN_DPD: u32 = 6;

const SW_CLR_INT: u32 = 1 << 31;
const SW_RDY: u32 = 1 << 30;
const CYCLE_NONE: u32 = 0 << 25;
const CYCLE_WAIT: u32 = 1 << 25;
const CYCLE_INDEX: u32 = 2 << 25;
const CYCLE_STOP: u32 = 4 << 25;
const SLAVE_READ: u32 = 1;
const SLAVE_WRITE: u32 = 0;

const HW_RDY: u32 = 1 << 11;
const SATOER: u32 = 1 << 10;
const ACTIVE: u32 = 1 << 9;

const WAIT_ITERS: u32 = 8_000;
const MAX_CHUNKS: u32 = 48;

static mut EDID: [u8; 128] = [0; 128];
static mut EDID_OK: bool = false;
static mut EDID_PIN: u32 = 0;
static mut EDID_RATE: u32 = 0;
static mut EDID_W: u16 = 0;
static mut EDID_H: u16 = 0;
static mut LAST_ERR: u8 = 0;
static mut LAST_PIN: u32 = 0;
static mut LAST_RATE: u32 = 0;
static mut LAST_STYLE: u8 = 0; // 0=index 1=write+read
static mut LAST_STAT: u32 = 0;
static mut ATTEMPTS: u32 = 0;
const MAX_MODES: usize = 8;
static mut EDID_MODES: [(u16, u16, u16); MAX_MODES] = [(0, 0, 0); MAX_MODES]; // w,h,refresh_hz
static mut EDID_MODE_N: usize = 0;

#[inline(always)]
fn rr(off: usize) -> u32 {
    intel_kms::mmio_r32_raw(off)
}
#[inline(always)]
fn ww(off: usize, v: u32) {
    intel_kms::mmio_w32_raw(off, v);
}

fn wait_hw_rdy_hard() -> Result<(), u8> {
    let mut t = 0u32;
    while t < WAIT_ITERS {
        let st = rr(GMBUS2);
        unsafe { LAST_STAT = st; }
        if (st & SATOER) != 0 {
            return Err(3);
        }
        if (st & HW_RDY) != 0 {
            return Ok(());
        }
        t = t.wrapping_add(1);
        core::hint::spin_loop();
    }
    Err(2)
}

fn gmbus_abort_raw() {
    ww(GMBUS1, SW_CLR_INT);
    ww(GMBUS1, 0);
    ww(GMBUS0, 0);
    // small settle
    let mut t = 0u32;
    while t < 500 {
        t += 1;
        core::hint::spin_loop();
    }
}

fn select_pin_rate(pin: u32, rate: u32) {
    // Optional clock gate on GMBUS4 (some SNB need non-zero divider - leave 0 = default)
    ww(GMBUS4, 0);
    ww(GMBUS0, rate | (pin & 7));
}

/// Style 0: INDEX+WAIT read (index=0 in GMBUS5).
fn xfer_index_read(pin: u32, rate: u32, addr7: u32, buf: &mut [u8]) -> Result<(), u8> {
    let len = buf.len() as u32;
    gmbus_abort_raw();
    select_pin_rate(pin, rate);
    ww(GMBUS5, 0);
    let cmd = SW_RDY
        | CYCLE_INDEX
        | CYCLE_WAIT
        | ((len & 0x1FF) << 16)
        | ((addr7 & 0x7F) << 1)
        | SLAVE_READ;
    ww(GMBUS1, cmd);
    let mut got = 0usize;
    let mut chunks = 0u32;
    while got < buf.len() {
        if chunks >= MAX_CHUNKS {
            gmbus_abort_raw();
            return Err(2);
        }
        chunks += 1;
        wait_hw_rdy_hard()?;
        let data = rr(GMBUS3);
        let remain = buf.len() - got;
        let n = if remain > 4 { 4 } else { remain };
        let mut i = 0usize;
        while i < n {
            buf[got + i] = ((data >> (8 * i)) & 0xFF) as u8;
            i += 1;
        }
        got += n;
    }
    ww(GMBUS1, SW_RDY | CYCLE_STOP);
    let _ = wait_hw_rdy_hard();
    gmbus_abort_raw();
    Ok(())
}

/// Style 1: write index byte 0, then NOINDEX read (two transactions).
fn xfer_write_then_read(pin: u32, rate: u32, addr7: u32, buf: &mut [u8]) -> Result<(), u8> {
    let len = buf.len() as u32;
    gmbus_abort_raw();
    select_pin_rate(pin, rate);

    // WRITE 1 byte (offset 0)
    // Put data in GMBUS3 first, then command
    ww(GMBUS3, 0x00);
    let cmd_w = SW_RDY
        | CYCLE_WAIT
        | (1u32 << 16)
        | ((addr7 & 0x7F) << 1)
        | SLAVE_WRITE;
    ww(GMBUS1, cmd_w);
    wait_hw_rdy_hard()?;
    // After write, HW_RDY may need STOP before read
    ww(GMBUS1, SW_RDY | CYCLE_STOP);
    let _ = wait_hw_rdy_hard();

    // READ len bytes, NO index (CYCLE_WAIT only)
    let cmd_r = SW_RDY
        | CYCLE_WAIT
        | ((len & 0x1FF) << 16)
        | ((addr7 & 0x7F) << 1)
        | SLAVE_READ;
    ww(GMBUS1, cmd_r);
    let mut got = 0usize;
    let mut chunks = 0u32;
    while got < buf.len() {
        if chunks >= MAX_CHUNKS {
            gmbus_abort_raw();
            return Err(2);
        }
        chunks += 1;
        wait_hw_rdy_hard()?;
        let data = rr(GMBUS3);
        let remain = buf.len() - got;
        let n = if remain > 4 { 4 } else { remain };
        let mut i = 0usize;
        while i < n {
            buf[got + i] = ((data >> (8 * i)) & 0xFF) as u8;
            i += 1;
        }
        got += n;
    }
    ww(GMBUS1, SW_RDY | CYCLE_STOP);
    let _ = wait_hw_rdy_hard();
    gmbus_abort_raw();
    Ok(())
}

fn push_mode(w: u16, h: u16, hz: u16) {
    if w < 640 || h < 480 || w > 4096 || h > 2160 {
        return;
    }
    unsafe {
        // dedup
        let mut i = 0usize;
        while i < EDID_MODE_N {
            if EDID_MODES[i].0 == w && EDID_MODES[i].1 == h {
                if hz > EDID_MODES[i].2 {
                    EDID_MODES[i].2 = hz;
                }
                return;
            }
            i += 1;
        }
        if EDID_MODE_N < MAX_MODES {
            EDID_MODES[EDID_MODE_N] = (w, h, hz);
            EDID_MODE_N += 1;
        }
    }
}

fn parse_detailed_at(edid: &[u8; 128], b: usize) -> Option<(u16, u16, u16)> {
    if b + 17 >= 128 {
        return None;
    }
    let clock_10khz = edid[b] as u32 | ((edid[b + 1] as u32) << 8);
    if clock_10khz == 0 {
        return None;
    }
    let ha = edid[b + 2] as u16 | (((edid[b + 4] as u16) & 0xF0) << 4);
    let hbl = edid[b + 3] as u16 | (((edid[b + 4] as u16) & 0x0F) << 8);
    let va = edid[b + 5] as u16 | (((edid[b + 7] as u16) & 0xF0) << 4);
    let vbl = edid[b + 6] as u16 | (((edid[b + 7] as u16) & 0x0F) << 8);
    if ha < 640 || va < 480 {
        return None;
    }
    let ht = (ha as u32).saturating_add(hbl as u32);
    let vt = (va as u32).saturating_add(vbl as u32);
    let hz = if ht > 0 && vt > 0 {
        let pix = clock_10khz.saturating_mul(10_000);
        let div = ht.saturating_mul(vt);
        if div == 0 { 60 } else { ((pix / div) as u16).max(1) }
    } else {
        60
    };
    Some((ha, va, hz))
}

fn parse_edid_modes(edid: &[u8; 128]) {
    unsafe { EDID_MODE_N = 0; }
    if edid[0] != 0x00 || edid[1] != 0xFF || edid[7] != 0x00 {
        return;
    }
    let bases = [54usize, 72, 90, 108];
    for &b in &bases {
        if let Some((w, h, hz)) = parse_detailed_at(edid, b) {
            push_mode(w, h, hz);
        }
    }
    // Established timings (bytes 35-37) - common set
    let et0 = edid[35];
    let et1 = edid[36];
    // bit flags: 720x400@70, 640x480@60, 640x480@67, 640x480@72, 640x480@75,
    // 800x600@56, 800x600@60, 800x600@72, 800x600@75, 832x624@75,
    // 1024x768@60, 1024x768@70, 1024x768@75, 1280x1024@75, ...
    if (et1 & 0x20) != 0 { push_mode(640, 480, 60); }
    if (et1 & 0x01) != 0 { push_mode(800, 600, 60); }
    if (et0 & 0x08) != 0 { push_mode(1024, 768, 60); }
    // Standard timings 38..53 (8 pairs)
    let mut i = 38usize;
    while i + 1 < 54 {
        let b0 = edid[i];
        let b1 = edid[i + 1];
        if b0 != 0x01 || b1 != 0x01 {
            let w = ((b0 as u16) + 31) * 8;
            let ar = (b1 >> 6) & 3;
            let h = match ar {
                0 => w * 10 / 16, // 16:10
                1 => w * 3 / 4,   // 4:3
                2 => w * 4 / 5,   // 5:4
                _ => w * 9 / 16,  // 16:9
            };
            let hz = 60 + (b1 & 0x3F) as u16;
            push_mode(w, h, hz);
        }
        i += 2;
    }
    // Always ensure preferred from first DTD is present (already first push)
}

fn parse_size(edid: &[u8; 128]) -> (u16, u16) {
    parse_edid_modes(edid);
    unsafe {
        if EDID_MODE_N > 0 {
            (EDID_MODES[0].0, EDID_MODES[0].1)
        } else {
            (0, 0)
        }
    }
}

fn try_one(
    pin: u32,
    rate: u32,
    style: u8,
    buf: &mut [u8; 128],
) -> Result<(), u8> {
    unsafe {
        LAST_PIN = pin;
        LAST_RATE = rate;
        LAST_STYLE = style;
        ATTEMPTS = ATTEMPTS.wrapping_add(1);
    }
    serial::write_str("[GMBUS] pin=");
    serial::write_usize(pin as usize);
    serial::write_str(" rate=");
    serial::write_usize((rate >> 8) as usize);
    serial::write_str(" sty=");
    serial::write_usize(style as usize);
    serial::write_str("\n");
    let r = if style == 0 {
        xfer_index_read(pin, rate, 0x50, buf)
    } else {
        xfer_write_then_read(pin, rate, 0x50, buf)
    };
    match &r {
        Ok(()) => serial::write_str("[GMBUS] xfer ok\n"),
        Err(e) => {
            serial::write_str("[GMBUS] err=");
            serial::write_usize(*e as usize);
            serial::write_str(" stat=");
            serial::write_hex(unsafe { LAST_STAT as usize });
            serial::write_str("\n");
        }
    }
    r
}

pub fn read_edid() -> bool {
    if !intel_kms::ready() {
        unsafe {
            LAST_ERR = 1;
            EDID_OK = false;
        }
        serial::write_str("[GMBUS] no MMIO\n");
        return false;
    }
    if !intel_kms::forcewake_enter() {
        unsafe {
            LAST_ERR = 1;
            EDID_OK = false;
        }
        return false;
    }
    unsafe { ATTEMPTS = 0; }
    serial::write_str("[GMBUS] PCH base 0xC5100 (SNB LVDS)\n");
    // Sanity: write pin select and read back - if still 0, MMIO not hitting GMBUS
    ww(GMBUS0, RATE_100KHZ | PIN_PANEL);
    let poke = rr(GMBUS0);
    serial::write_str("[GMBUS] poke GMBUS0=");
    serial::write_hex(poke as usize);
    serial::write_str("\n");
    unsafe { LAST_STAT = poke; }
    if poke == 0 || poke == 0xFFFF_FFFF {
        serial::write_str("[GMBUS] poke FAIL - wrong MMIO/power\n");
        unsafe { LAST_ERR = 5; EDID_OK = false; }
        gmbus_abort_raw();
        intel_kms::forcewake_leave();
        return false;
    }

    serial::write_str("[GMBUS] pin×rate×style scan\n");

    let pins: [u32; 6] = [
        PIN_PANEL, PIN_DPB, PIN_DPC, PIN_DPD, PIN_VGADDC, PIN_SSC,
    ];
    // Prefer slower first (SNB panel DDC often flaky at 100+)
    let rates: [u32; 2] = [RATE_50KHZ, RATE_100KHZ];
    let mut buf = [0u8; 128];
    let mut ok = false;

    'outer: for &rate in &rates {
        for &pin in &pins {
            for style in 0u8..2u8 {
                match try_one(pin, rate, style, &mut buf) {
                    Ok(()) => {
                        if buf[0] == 0x00 && buf[1] == 0xFF && buf[7] == 0x00 {
                            let (w0, h0) = parse_size(&buf);
                            unsafe {
                                EDID = buf;
                                EDID_OK = true;
                                EDID_PIN = pin;
                                EDID_RATE = rate;
                                EDID_W = w0;
                                EDID_H = h0;
                                LAST_ERR = 0;
                            }
                            serial::write_str("[GMBUS] EDID OK modes=");
                            serial::write_usize(unsafe { EDID_MODE_N });
                            serial::write_str("\n");
                            let mut mi = 0usize;
                            while mi < unsafe { EDID_MODE_N } {
                                let (mw, mh, mz) = unsafe { EDID_MODES[mi] };
                                serial::write_str("  ");
                                serial::write_usize(mw as usize);
                                serial::write_str("x");
                                serial::write_usize(mh as usize);
                                serial::write_str("@");
                                serial::write_usize(mz as usize);
                                serial::write_str("\n");
                                mi += 1;
                            }
                            ok = true;
                            break 'outer;
                        }
                        unsafe { LAST_ERR = 4; }
                    }
                    Err(e) => unsafe { LAST_ERR = e },
                }
            }
        }
    }

    if !ok {
        unsafe { EDID_OK = false; }
        serial::write_str("[GMBUS] all attempts failed\n");
    }
    gmbus_abort_raw();
    intel_kms::forcewake_leave();
    ok
}

pub fn ok() -> bool {
    unsafe { EDID_OK }
}
pub fn pin() -> u32 {
    unsafe { EDID_PIN }
}
pub fn rate_code() -> u32 {
    unsafe { EDID_RATE >> 8 }
}
pub fn width() -> u16 {
    unsafe { EDID_W }
}
pub fn height() -> u16 {
    unsafe { EDID_H }
}
pub fn last_err() -> u8 {
    unsafe { LAST_ERR }
}
pub fn last_pin() -> u32 {
    unsafe { LAST_PIN }
}
pub fn last_rate() -> u32 {
    unsafe { LAST_RATE >> 8 }
}
pub fn last_style() -> u8 {
    unsafe { LAST_STYLE }
}
pub fn last_stat() -> u32 {
    unsafe { LAST_STAT }
}
pub fn attempts() -> u32 {
    unsafe { ATTEMPTS }
}
pub fn mode_count() -> usize {
    unsafe { EDID_MODE_N }
}
pub fn mode_at(i: usize) -> Option<(u16, u16, u16)> {
    unsafe {
        if i < EDID_MODE_N {
            Some(EDID_MODES[i])
        } else {
            None
        }
    }
}
pub fn preferred() -> Option<(u16, u16, u16)> {
    mode_at(0)
}
pub fn header_byte(i: usize) -> u8 {
    unsafe {
        if i < 16 {
            EDID[i]
        } else {
            0
        }
    }
}
