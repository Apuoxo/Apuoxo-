//! Aether Alarm Clock - RTC-based wake alarm with sound.
//!
//! Uses CMOS RTC for time matching. Sound path:
//! 1) HDA PCM dual-tone (primary)
//! 2) PC speaker beep pattern (always available fallback)

use crate::graphics;
use crate::serial;
use crate::time;

const WHITE: u32 = 0x00FFFFFF;
const TEXT: u32 = 0x001A1A1A;
const DIM: u32 = 0x005A5A5A;
const ACCENT: u32 = 0x002B6CB0;
const GREEN: u32 = 0x00228B22;
const RED: u32 = 0x00C42B1C;
const PANEL: u32 = 0x00F0F4F8;
const BTN: u32 = 0x00E8EEF5;
const BTN_BORDER: u32 = 0x007A9BC0;

static mut ENABLED: bool = false;
static mut HOUR: u8 = 7;
static mut MINUTE: u8 = 0;
static mut RINGING: bool = false;
static mut LAST_FIRE_MIN: i32 = -1; // avoid re-fire same minute
static mut RING_TICKS: u32 = 0;
static mut SNOOZE_UNTIL_MIN: i32 = -1; // absolute minute-of-day, or -1

#[inline]
fn minute_of_day(h: u8, m: u8) -> i32 {
    (h as i32) * 60 + (m as i32)
}

pub fn is_enabled() -> bool {
    unsafe { ENABLED }
}
pub fn is_ringing() -> bool {
    unsafe { RINGING }
}
pub fn hour() -> u8 {
    unsafe { HOUR }
}
pub fn minute() -> u8 {
    unsafe { MINUTE }
}

pub fn set_hour(h: u8) {
    unsafe {
        HOUR = h % 24;
        LAST_FIRE_MIN = -1;
    }
}
pub fn set_minute(m: u8) {
    unsafe {
        MINUTE = m % 60;
        LAST_FIRE_MIN = -1;
    }
}
pub fn hour_up() {
    unsafe {
        HOUR = (HOUR + 1) % 24;
        LAST_FIRE_MIN = -1;
    }
}
pub fn hour_down() {
    unsafe {
        HOUR = if HOUR == 0 { 23 } else { HOUR - 1 };
        LAST_FIRE_MIN = -1;
    }
}
pub fn minute_up() {
    unsafe {
        MINUTE = (MINUTE + 1) % 60;
        LAST_FIRE_MIN = -1;
    }
}
pub fn minute_down() {
    unsafe {
        MINUTE = if MINUTE == 0 { 59 } else { MINUTE - 1 };
        LAST_FIRE_MIN = -1;
    }
}
pub fn toggle_enabled() {
    unsafe {
        ENABLED = !ENABLED;
        if !ENABLED {
            stop_ringing();
        }
        LAST_FIRE_MIN = -1;
        SNOOZE_UNTIL_MIN = -1;
    }
}
pub fn enable() {
    unsafe {
        ENABLED = true;
        LAST_FIRE_MIN = -1;
    }
}
pub fn disable() {
    unsafe {
        ENABLED = false;
        stop_ringing();
        SNOOZE_UNTIL_MIN = -1;
    }
}

fn stop_ringing() {
    unsafe {
        RINGING = false;
        RING_TICKS = 0;
    }
    crate::media_player::stop_alarm_sound();
    crate::drivers::audio::playback_stop();
}

pub fn dismiss() {
    stop_ringing();
    // Stay enabled for next day; mark this minute as fired.
    unsafe {
        let (_y, _mo, _d, h, mi, _s) = time::rtc_read();
        LAST_FIRE_MIN = minute_of_day(h, mi);
        SNOOZE_UNTIL_MIN = -1;
    }
}

pub fn snooze_5min() {
    stop_ringing();
    unsafe {
        let (_y, _mo, _d, h, mi, _s) = time::rtc_read();
        let now = minute_of_day(h, mi);
        SNOOZE_UNTIL_MIN = (now + 5) % (24 * 60);
        LAST_FIRE_MIN = now;
        ENABLED = true;
    }
}

fn start_ringing() {
    unsafe {
        if RINGING {
            return;
        }
        RINGING = true;
        RING_TICKS = 0;
    }
    serial::write_str("[ALARM] RINGING\n");
    // Prefer HDA dual-tone; always also arm speaker pattern via tick.
    let _ = crate::media_player::start_alarm_sound();
}

/// Call from desktop main loop every iteration.
pub fn poll() {
    let (_y, _mo, _d, h, mi, _s) = time::rtc_read();
    let now = minute_of_day(h, mi);

    unsafe {
        if RINGING {
            RING_TICKS = RING_TICKS.wrapping_add(1);
            // Speaker pattern every ~0.5s wall-ish (loop is busy; use tick count)
            // Short non-blocking-ish beeps (duration_units small). Rare enough to not freeze UI.
            if RING_TICKS % 200_000 == 1 {
                crate::drivers::audio::beep_hz(880, 3);
            } else if RING_TICKS % 200_000 == 100_000 {
                crate::drivers::audio::beep_hz(1320, 3);
            }
            // Keep HDA buffer fed
            crate::drivers::audio::playback_poll();
            // Auto-stop after long ring (~few minutes of ticks) - user can dismiss sooner
            if RING_TICKS > 8_000_000 {
                dismiss();
            }
            return;
        }

        if !ENABLED {
            return;
        }

        // Snooze path
        if SNOOZE_UNTIL_MIN >= 0 {
            if now == SNOOZE_UNTIL_MIN && LAST_FIRE_MIN != now {
                SNOOZE_UNTIL_MIN = -1;
                LAST_FIRE_MIN = now;
                start_ringing();
            }
            return;
        }

        let target = minute_of_day(HOUR, MINUTE);
        if now == target && LAST_FIRE_MIN != now {
            LAST_FIRE_MIN = now;
            start_ringing();
        }
    }
}

fn draw_2(x: usize, y: usize, v: u8, col: u32) {
    graphics::draw_char(x, y, b'0' + (v / 10) % 10, col);
    graphics::draw_char(x + 8, y, b'0' + (v % 10), col);
}

fn btn(x: usize, y: usize, w: usize, h: usize, label: &str, hot: bool) {
    let bg = if hot { 0x00D0E8F8 } else { BTN };
    graphics::fill_rect(x, y, w, h, bg);
    graphics::border_rect(x, y, w, h, BTN_BORDER);
    graphics::draw_str(x + 8, y + (h / 2).saturating_sub(4), label, TEXT);
}

/// Paint Alarm window client area (below title bar).
pub fn draw(wx: usize, wy: usize, ww: usize, wh: usize, title_h: usize) {
    let by = wy + title_h;
    let bh = wh.saturating_sub(title_h + 3);
    graphics::fill_rect(wx + 3, by, ww.saturating_sub(6), bh, PANEL);

    // Header
    graphics::draw_str(wx + 16, by + 12, "Alarm Clock", ACCENT);
    graphics::draw_str(wx + 16, by + 30, "Wake up with sound at the set time", DIM);

    // Current time from RTC
    let (_y, _mo, _d, ch, cm, cs) = time::rtc_read();
    graphics::draw_str(wx + 16, by + 56, "Now:", DIM);
    draw_2(wx + 56, by + 56, ch, TEXT);
    graphics::draw_char(wx + 72, by + 56, b':', TEXT);
    draw_2(wx + 80, by + 56, cm, TEXT);
    graphics::draw_char(wx + 96, by + 56, b':', TEXT);
    draw_2(wx + 104, by + 56, cs, TEXT);

    // Alarm time big display
    let ah = unsafe { HOUR };
    let am = unsafe { MINUTE };
    graphics::fill_rect(wx + 16, by + 84, 200, 56, WHITE);
    graphics::border_rect(wx + 16, by + 84, 200, 56, BTN_BORDER);
    // Large HH:MM using double-width spacing
    draw_2(wx + 40, by + 104, ah, ACCENT);
    graphics::draw_char(wx + 60, by + 104, b':', ACCENT);
    draw_2(wx + 72, by + 104, am, ACCENT);

    // Hour / minute adjust
    btn(wx + 230, by + 84, 36, 24, "H+", false);
    btn(wx + 270, by + 84, 36, 24, "H-", false);
    btn(wx + 230, by + 116, 36, 24, "M+", false);
    btn(wx + 270, by + 116, 36, 24, "M-", false);

    // Enable toggle
    let en = unsafe { ENABLED };
    let en_label = if en { "Alarm: ON " } else { "Alarm: OFF" };
    let en_col = if en { GREEN } else { RED };
    graphics::fill_rect(wx + 16, by + 156, 120, 28, if en { 0x00E8F8E8 } else { 0x00F8E8E8 });
    graphics::border_rect(wx + 16, by + 156, 120, 28, en_col);
    graphics::draw_str(wx + 28, by + 164, en_label, en_col);

    btn(wx + 148, by + 156, 80, 28, "Toggle", false);

    // Ringing banner
    if unsafe { RINGING } {
        graphics::fill_rect(wx + 16, by + 198, ww.saturating_sub(40), 36, 0x00FFE8E8);
        graphics::border_rect(wx + 16, by + 198, ww.saturating_sub(40), 36, RED);
        graphics::draw_str(wx + 28, by + 210, "ALARM!  [Stop]   [Snooze 5 min]", RED);
    } else {
        graphics::draw_str(wx + 16, by + 198, "Status: armed when ON; uses RTC + HDA/speaker", DIM);
        let snooze = unsafe { SNOOZE_UNTIL_MIN };
        if snooze >= 0 {
            let sh = (snooze / 60) as u8;
            let sm = (snooze % 60) as u8;
            graphics::draw_str(wx + 16, by + 216, "Snooze until:", DIM);
            draw_2(wx + 120, by + 216, sh, TEXT);
            graphics::draw_char(wx + 136, by + 216, b':', TEXT);
            draw_2(wx + 144, by + 216, sm, TEXT);
        }
    }

    // Test sound
    btn(wx + 16, by + 250, 100, 26, "Test sound", false);
    graphics::draw_str(wx + 128, by + 256, "HDA tone + speaker beep", DIM);
}

/// Click handler. Coordinates relative to window client... actually absolute.
/// Returns true if handled.
pub fn on_click(wx: i32, wy: i32, _ww: i32, _wh: i32, title_h: i32, mx: i32, my: i32) -> bool {
    let by = wy + title_h;
    let relx = mx - wx;
    let rely = my - by;
    if rely < 0 {
        return false;
    }

    // H+ H- M+ M-
    if rely >= 84 && rely < 108 {
        if relx >= 230 && relx < 266 {
            hour_up();
            return true;
        }
        if relx >= 270 && relx < 306 {
            hour_down();
            return true;
        }
    }
    if rely >= 116 && rely < 140 {
        if relx >= 230 && relx < 266 {
            minute_up();
            return true;
        }
        if relx >= 270 && relx < 306 {
            minute_down();
            return true;
        }
    }
    // Toggle
    if rely >= 156 && rely < 184 {
        if relx >= 16 && relx < 236 {
            toggle_enabled();
            return true;
        }
    }
    // Ringing controls
    if unsafe { RINGING } && rely >= 198 && rely < 234 {
        // Stop roughly left half, Snooze right
        if relx < 160 {
            dismiss();
            return true;
        }
        snooze_5min();
        return true;
    }
    // Test sound
    if rely >= 250 && rely < 276 && relx >= 16 && relx < 116 {
        let _ = crate::media_player::start_alarm_sound();
        crate::drivers::audio::beep_hz(880, 12);
        crate::drivers::audio::beep_hz(1320, 12);
        return true;
    }
    false
}
