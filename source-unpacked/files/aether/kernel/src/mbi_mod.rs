//! Multiboot2 module tags - e.g. WiFi firmware loaded by GRUB
use crate::serial;

fn read_u32(addr: usize) -> u32 {
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

/// Returns (mod_start, mod_end, len)
pub fn find_module_range(mbi: usize, needle: &[u8]) -> Option<(usize, usize, usize)> {
    if mbi == 0 || mbi < 0x1000 {
        return None;
    }
    let total = read_u32(mbi) as usize;
    if total < 16 || total > 0x200000 {
        return None;
    }
    let mut off = 8usize;
    while off + 8 <= total {
        let tag_type = read_u32(mbi + off);
        let tag_size = read_u32(mbi + off + 4) as usize;
        if tag_size < 8 || off + tag_size > total {
            break;
        }
        if tag_type == 0 {
            break;
        }
        if tag_type == 3 && tag_size >= 16 {
            let mod_start = read_u32(mbi + off + 8) as usize;
            let mod_end = read_u32(mbi + off + 12) as usize;
            if mod_end > mod_start && mod_start >= 0x1000 {
                let len = mod_end - mod_start;
                let cmd_off = mbi + off + 16;
                let cmd_max = tag_size.saturating_sub(16);
                if cmdline_has(cmd_off, cmd_max, needle) || needle.is_empty() {
                    serial::write_str("[MBI] module start=");
                    serial::write_hex(mod_start);
                    serial::write_str(" end=");
                    serial::write_hex(mod_end);
                    serial::write_str(" len=");
                    serial::write_usize(len);
                    serial::write_str("\n");
                    return Some((mod_start, mod_end, len));
                }
            }
        }
        off = (off + tag_size + 7) & !7;
    }
    None
}

pub fn find_module(mbi: usize, needle: &[u8]) -> Option<(usize, usize)> {
    find_module_range(mbi, needle).map(|(s, _e, l)| (s, l))
}

fn cmdline_has(addr: usize, max: usize, needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if max == 0 {
        return false;
    }
    let mut i = 0usize;
    while i < max {
        let b = unsafe { core::ptr::read_volatile((addr + i) as *const u8) };
        if b == 0 {
            break;
        }
        i += 1;
    }
    let slen = i;
    if slen < needle.len() {
        return false;
    }
    let mut s = 0usize;
    while s + needle.len() <= slen {
        let mut j = 0usize;
        let mut ok = true;
        while j < needle.len() {
            let c = unsafe { core::ptr::read_volatile((addr + s + j) as *const u8) };
            let n = needle[j];
            let c2 = if c >= b'A' && c <= b'Z' { c + 32 } else { c };
            let n2 = if n >= b'A' && n <= b'Z' { n + 32 } else { n };
            if c2 != n2 {
                ok = false;
                break;
            }
            j += 1;
        }
        if ok {
            return true;
        }
        s += 1;
    }
    false
}
