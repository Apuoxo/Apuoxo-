//! CMOS RTC (BIOS clock) - ports 0x70/0x71

static mut TICKS: u64 = 0;

pub fn tick() {
    unsafe {
        TICKS += 1;
    }
}
pub fn uptime() -> u64 {
    unsafe { TICKS }
}

unsafe fn cmos_read(reg: u8) -> u8 {
    core::arch::asm!(
        "out dx, al",
        in("dx") 0x70u16,
        in("al") (reg | 0x80),
        options(nostack, preserves_flags)
    );
    let v: u8;
    core::arch::asm!(
        "in al, dx",
        in("dx") 0x71u16,
        out("al") v,
        options(nostack, preserves_flags)
    );
    v
}

fn bcd_to_bin(v: u8) -> u8 {
    ((v >> 4) * 10) + (v & 0x0F)
}

/// (year_full, month, day, hour, min, sec)
pub fn rtc_read() -> (u16, u8, u8, u8, u8, u8) {
    unsafe {
        let mut t = 0u32;
        while t < 10000 {
            if cmos_read(0x0A) & 0x80 == 0 {
                break;
            }
            t += 1;
        }
        let mut sec = cmos_read(0x00);
        let mut min = cmos_read(0x02);
        let mut hour = cmos_read(0x04);
        let mut day = cmos_read(0x07);
        let mut month = cmos_read(0x08);
        let mut year = cmos_read(0x09);
        let status_b = cmos_read(0x0B);
        if status_b & 0x04 == 0 {
            sec = bcd_to_bin(sec);
            min = bcd_to_bin(min);
            hour = bcd_to_bin(hour & 0x7F);
            day = bcd_to_bin(day);
            month = bcd_to_bin(month);
            year = bcd_to_bin(year);
        }
        hour = hour % 24;
        let y = 2000u16 + (year as u16);
        (y, month, day, hour, min, sec)
    }
}

/// "YYYY-MM-DD HH:MM:SS" → 19 chars into buf
pub fn format_datetime(buf: &mut [u8; 20]) -> usize {
    let (y, mo, d, h, mi, s) = rtc_read();
    buf[0] = b'0' + ((y / 1000) % 10) as u8;
    buf[1] = b'0' + ((y / 100) % 10) as u8;
    buf[2] = b'0' + ((y / 10) % 10) as u8;
    buf[3] = b'0' + (y % 10) as u8;
    buf[4] = b'-';
    buf[5] = b'0' + (mo / 10);
    buf[6] = b'0' + (mo % 10);
    buf[7] = b'-';
    buf[8] = b'0' + (d / 10);
    buf[9] = b'0' + (d % 10);
    buf[10] = b' ';
    buf[11] = b'0' + (h / 10);
    buf[12] = b'0' + (h % 10);
    buf[13] = b':';
    buf[14] = b'0' + (mi / 10);
    buf[15] = b'0' + (mi % 10);
    buf[16] = b':';
    buf[17] = b'0' + (s / 10);
    buf[18] = b'0' + (s % 10);
    buf[19] = 0;
    19
}
