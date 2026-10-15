//! FAT12/16/32 READ-ONLY browser
use crate::block;
use crate::part;
use crate::serial;

#[derive(Clone, Copy)]
pub struct FatVol {
    pub part_idx: usize,
    pub disk: u8,
    pub part_lba: u32,
    pub fat_type: u8, // 12, 16, 32
    pub bytes_per_sec: u16,
    pub sec_per_cl: u8,
    pub reserved: u16,
    pub fats: u8,
    pub root_ents: u16,
    pub fat_secs: u32,
    pub total_secs: u32,
    pub root_cl: u32, // FAT32
    pub data_lba: u32,
    pub root_lba: u32, // FAT16 root dir
    pub root_secs: u32,
    pub ok: bool,
}

#[derive(Clone, Copy)]
pub struct FatEntry {
    pub name: [u8; 13], // 8.3 + nul
    pub name_len: usize,
    pub size: u32,
    pub cluster: u32,
    pub is_dir: bool,
    pub attr: u8,
}

static mut VOL: FatVol = FatVol {
    part_idx: 0, disk: 0, part_lba: 0, fat_type: 0,
    bytes_per_sec: 512, sec_per_cl: 0, reserved: 0, fats: 0,
    root_ents: 0, fat_secs: 0, total_secs: 0, root_cl: 0,
    data_lba: 0, root_lba: 0, root_secs: 0, ok: false,
};
static mut CWD_CL: u32 = 0; // 0 = root for FAT32
static mut ENTRIES: [FatEntry; 32] = [FatEntry {
    name: [0; 13], name_len: 0, size: 0, cluster: 0, is_dir: false, attr: 0,
}; 32];
static mut NENT: usize = 0;

fn read_sec(disk: u8, abs_lba: u32, buf: &mut [u8; 512]) -> bool {
    block::read(disk, abs_lba, 1, buf)
}

pub fn mount_partition(pi: usize) -> bool {
    let p = match part::get(pi) {
        Some(p) => p,
        None => return false,
    };
    let mut bpb = [0u8; 512];
    if !block::read(p.disk, p.lba_start, 1, &mut bpb) {
        serial::write_str("[FAT] BPB read fail\n");
        return false;
    }
    let bps = u16::from_le_bytes([bpb[11], bpb[12]]);
    if bps != 512 {
        serial::write_str("[FAT] unsupported sector size\n");
        return false;
    }
    let spc = bpb[13];
    let reserved = u16::from_le_bytes([bpb[14], bpb[15]]);
    let fats = bpb[16];
    let root_ents = u16::from_le_bytes([bpb[17], bpb[18]]);
    let mut total16 = u16::from_le_bytes([bpb[19], bpb[20]]) as u32;
    let mut fat_secs = u16::from_le_bytes([bpb[22], bpb[23]]) as u32;
    if fat_secs == 0 {
        fat_secs = u32::from_le_bytes([bpb[36], bpb[37], bpb[38], bpb[39]]);
    }
    let total32 = u32::from_le_bytes([bpb[32], bpb[33], bpb[34], bpb[35]]);
    if total16 == 0 {
        total16 = total32;
    }
    let root_secs = ((root_ents as u32 * 32) + 511) / 512;
    let data_lba = p.lba_start + reserved as u32 + fats as u32 * fat_secs + root_secs;
    let root_lba = p.lba_start + reserved as u32 + fats as u32 * fat_secs;
    let data_secs = total16.saturating_sub(
        reserved as u32 + fats as u32 * fat_secs + root_secs,
    );
    let clusters = if spc == 0 { 0 } else { data_secs / spc as u32 };
    let fat_type = if clusters < 4085 {
        12
    } else if clusters < 65525 {
        16
    } else {
        32
    };
    let root_cl = if fat_type == 32 {
        u32::from_le_bytes([bpb[44], bpb[45], bpb[46], bpb[47]])
    } else {
        0
    };
    // check fat signature-ish
    if bpb[0] != 0xEB && bpb[0] != 0xE9 {
        serial::write_str("[FAT] bad jump\n");
        // still try
    }
    unsafe {
        VOL = FatVol {
            part_idx: pi,
            disk: p.disk,
            part_lba: p.lba_start,
            fat_type,
            bytes_per_sec: bps,
            sec_per_cl: spc,
            reserved,
            fats,
            root_ents,
            fat_secs,
            total_secs: total16,
            root_cl,
            data_lba,
            root_lba,
            root_secs,
            ok: true,
        };
        CWD_CL = if fat_type == 32 { root_cl } else { 0 };
    }
    serial::write_str("[FAT] mounted type=");
    serial::write_usize(fat_type as usize);
    serial::write_str(" spc=");
    serial::write_usize(spc as usize);
    serial::write_str(" clusters~");
    serial::write_usize(clusters as usize);
    serial::write_str("\n");
    list_cwd()
}

pub fn is_mounted() -> bool {
    unsafe { VOL.ok }
}

fn cluster_to_lba(cl: u32) -> u32 {
    unsafe {
        if cl < 2 {
            return VOL.root_lba;
        }
        VOL.data_lba + (cl - 2) * VOL.sec_per_cl as u32
    }
}

fn fat_get(cl: u32) -> u32 {
    unsafe {
        if !VOL.ok {
            return 0xFFFFFFFF;
        }
        let mut sec = [0u8; 512];
        if VOL.fat_type == 32 {
            let fat_lba = VOL.part_lba + VOL.reserved as u32;
            let off = cl * 4;
            let sec_i = off / 512;
            let so = (off % 512) as usize;
            if !read_sec(VOL.disk, fat_lba + sec_i, &mut sec) {
                return 0xFFFFFFFF;
            }
            u32::from_le_bytes([sec[so], sec[so + 1], sec[so + 2], sec[so + 3]]) & 0x0FFF_FFFF
        } else if VOL.fat_type == 16 {
            let fat_lba = VOL.part_lba + VOL.reserved as u32;
            let off = cl * 2;
            let sec_i = off / 512;
            let so = (off % 512) as usize;
            if !read_sec(VOL.disk, fat_lba + sec_i, &mut sec) {
                return 0xFFFFFFFF;
            }
            u16::from_le_bytes([sec[so], sec[so + 1]]) as u32
        } else {
            0xFFFFFFFF
        }
    }
}


/// CP866 (DOS Cyrillic) / high-bit OEM → ASCII-ish for bitmap font.
fn oem_byte(b: u8) -> u8 {
    if b >= 0x20 && b < 0x7F {
        return b;
    }
    // CP866 А-Я (0x80-0x9F) and а-п (0xA0-0xAF) approximate
    if b >= 0x80 && b <= 0xAF {
        const MAP: &[u8] = b"ABVGDEZHZIYKLMNOPRSTUFHCCss'Y'EYuYaabvgdezhziyklmnop";
        let i = (b - 0x80) as usize;
        if i < MAP.len() {
            return MAP[i];
        }
    }
    // CP866 р-я 0xE0-0xEF
    if b >= 0xE0 && b <= 0xEF {
        const MAP: &[u8] = b"rstufhccss'y'euy";
        return MAP[(b - 0xE0) as usize];
    }
    if b == 0xF0 { return b'r'; } // Ё sometimes
    if b == 0xF1 { return b's'; }
    b'_'
}

fn parse_dir_sector(sec: &[u8; 512], start: usize) {
    let mut i = 0usize;
    while i < 512 {
        let off = i;
        if sec[off] == 0 {
            break;
        }
        if sec[off] == 0xE5 {
            i += 32;
            continue;
        }
        let attr = sec[off + 11];
        if attr == 0x0F {
            // LFN skip
            i += 32;
            continue;
        }
        if attr & 0x08 != 0 {
            // volume label
            i += 32;
            continue;
        }
        unsafe {
            if NENT >= 32 {
                break;
            }
            let mut name = [0u8; 13];
            let mut nl = 0usize;
            let mut j = 0usize;
            while j < 8 && sec[off + j] != b' ' {
                name[nl] = oem_byte(sec[off + j]);
                nl += 1;
                j += 1;
            }
            if sec[off + 8] != b' ' {
                name[nl] = b'.';
                nl += 1;
                j = 8;
                while j < 11 && sec[off + j] != b' ' {
                    name[nl] = oem_byte(sec[off + j]);
                    nl += 1;
                    j += 1;
                }
            }
            let cl_hi = u16::from_le_bytes([sec[off + 20], sec[off + 21]]) as u32;
            let cl_lo = u16::from_le_bytes([sec[off + 26], sec[off + 27]]) as u32;
            let cluster = (cl_hi << 16) | cl_lo;
            let size = u32::from_le_bytes([sec[off + 28], sec[off + 29], sec[off + 30], sec[off + 31]]);
            ENTRIES[NENT] = FatEntry {
                name,
                name_len: nl,
                size,
                cluster,
                is_dir: attr & 0x10 != 0,
                attr,
            };
            NENT += 1;
        }
        i += 32;
        let _ = start;
    }
}

pub fn list_cwd() -> bool {
    unsafe {
        if !VOL.ok {
            return false;
        }
        NENT = 0;
        let mut sec = [0u8; 512];
        if VOL.fat_type != 32 {
            // FAT16 root is fixed region
            let mut s = 0u32;
            while s < VOL.root_secs && NENT < 32 {
                if !read_sec(VOL.disk, VOL.root_lba + s, &mut sec) {
                    return false;
                }
                parse_dir_sector(&sec, 0);
                s += 1;
            }
        } else {
            let mut cl = if CWD_CL == 0 { VOL.root_cl } else { CWD_CL };
            let mut guard = 0usize;
            while cl >= 2 && cl < 0x0FFF_FFF8 && NENT < 32 && guard < 64 {
                let lba = cluster_to_lba(cl);
                let mut s = 0u8;
                while s < VOL.sec_per_cl && NENT < 32 {
                    if !read_sec(VOL.disk, lba + s as u32, &mut sec) {
                        return false;
                    }
                    parse_dir_sector(&sec, 0);
                    s += 1;
                }
                cl = fat_get(cl);
                guard += 1;
            }
        }
        serial::write_str("[FAT] entries=");
        serial::write_usize(NENT);
        serial::write_str("\n");
        true
    }
}

pub fn entry_count() -> usize {
    unsafe { NENT }
}

pub fn entry(i: usize) -> Option<FatEntry> {
    unsafe {
        if i < NENT {
            Some(ENTRIES[i])
        } else {
            None
        }
    }
}

pub fn enter_dir(i: usize) -> bool {
    unsafe {
        if i >= NENT || !ENTRIES[i].is_dir {
            return false;
        }
        let cl = ENTRIES[i].cluster;
        CWD_CL = if cl == 0 { VOL.root_cl } else { cl };
        list_cwd()
    }
}

pub fn at_root() -> bool {
    unsafe {
        if !VOL.ok { return true; }
        if VOL.fat_type == 32 {
            CWD_CL == VOL.root_cl || CWD_CL == 0
        } else {
            CWD_CL == 0
        }
    }
}

/// Set CWD to volume root without reading the disk (safe after media/AHCI stress).
pub fn soft_root() {
    unsafe {
        if !VOL.ok { return; }
        CWD_CL = if VOL.fat_type == 32 { VOL.root_cl } else { 0 };
        NENT = 0; // empty until explicit refresh
    }
}

pub fn go_root() -> bool {
    unsafe {
        if !VOL.ok {
            return false;
        }
        CWD_CL = if VOL.fat_type == 32 { VOL.root_cl } else { 0 };
        list_cwd()
    }
}

pub fn vol_info() -> Option<FatVol> {
    unsafe {
        if VOL.ok {
            Some(VOL)
        } else {
            None
        }
    }
}

fn write_sec(disk: u8, abs_lba: u32, buf: &[u8; 512]) -> bool {
    block::write(disk, abs_lba, 1, buf)
}

fn fat_set(cl: u32, val: u32) -> bool {
    unsafe {
        if !VOL.ok { return false; }
        let mut sec = [0u8; 512];
        let fat_lba = VOL.part_lba + VOL.reserved as u32;
        if VOL.fat_type == 32 {
            let off = cl * 4;
            let sec_i = off / 512;
            let so = (off % 512) as usize;
            if !read_sec(VOL.disk, fat_lba + sec_i, &mut sec) { return false; }
            let v = (val & 0x0FFF_FFFF) | (u32::from_le_bytes([sec[so], sec[so+1], sec[so+2], sec[so+3]]) & 0xF000_0000);
            sec[so] = (v & 0xFF) as u8;
            sec[so+1] = ((v >> 8) & 0xFF) as u8;
            sec[so+2] = ((v >> 16) & 0xFF) as u8;
            sec[so+3] = ((v >> 24) & 0xFF) as u8;
            if !write_sec(VOL.disk, fat_lba + sec_i, &sec) { return false; }
            // mirror second FAT if present
            if VOL.fats > 1 {
                let _ = write_sec(VOL.disk, fat_lba + VOL.fat_secs + sec_i, &sec);
            }
            true
        } else if VOL.fat_type == 16 {
            let off = cl * 2;
            let sec_i = off / 512;
            let so = (off % 512) as usize;
            if !read_sec(VOL.disk, fat_lba + sec_i, &mut sec) { return false; }
            sec[so] = (val & 0xFF) as u8;
            sec[so+1] = ((val >> 8) & 0xFF) as u8;
            if !write_sec(VOL.disk, fat_lba + sec_i, &sec) { return false; }
            if VOL.fats > 1 {
                let _ = write_sec(VOL.disk, fat_lba + VOL.fat_secs + sec_i, &sec);
            }
            true
        } else {
            false
        }
    }
}

fn alloc_cluster() -> Option<u32> {
    unsafe {
        if !VOL.ok { return None; }
        let max = if VOL.fat_type == 32 { 0x0FFF_FFF0u32 } else { 0xFFF0u32 };
        let mut c = 2u32;
        while c < max && c < 1_000_000 {
            let v = fat_get(c);
            if v == 0 {
                let eoc = if VOL.fat_type == 32 { 0x0FFF_FFFF } else { 0xFFFF };
                if fat_set(c, eoc) {
                    return Some(c);
                }
            }
            c += 1;
        }
        None
    }
}

fn free_chain(start: u32) {
    let mut cl = start;
    let mut guard = 0usize;
    while cl >= 2 && cl < 0x0FFF_FFF8 && guard < 256 {
        let next = fat_get(cl);
        let _ = fat_set(cl, 0);
        cl = next;
        guard += 1;
    }
}

fn to_83(name: &[u8]) -> [u8; 11] {
    let mut out = [b' '; 11];
    let mut i = 0usize;
    let mut di = 0usize;
    while i < name.len() && di < 8 {
        let c = name[i];
        if c == b'.' { break; }
        if c != b' ' {
            out[di] = if c >= b'a' && c <= b'z' { c - 32 } else { c };
            di += 1;
        }
        i += 1;
    }
    while i < name.len() && name[i] != b'.' { i += 1; }
    if i < name.len() && name[i] == b'.' {
        i += 1;
        let mut ei = 8usize;
        while i < name.len() && ei < 11 {
            let c = name[i];
            if c != b' ' {
                out[ei] = if c >= b'a' && c <= b'z' { c - 32 } else { c };
                ei += 1;
            }
            i += 1;
        }
    }
    out
}

/// Locate directory entry by 8.3 name in CWD; returns (abs_lba, offset_in_sector).
fn find_entry_slot(name83: &[u8; 11]) -> Option<(u32, usize)> {
    unsafe {
        if !VOL.ok { return None; }
        let mut sec = [0u8; 512];
        if VOL.fat_type != 32 {
            let mut s = 0u32;
            while s < VOL.root_secs {
                if !read_sec(VOL.disk, VOL.root_lba + s, &mut sec) { return None; }
                let mut i = 0usize;
                while i < 512 {
                    if sec[i] == 0 { return None; }
                    if sec[i] != 0xE5 && sec[i + 11] != 0x0F {
                        let mut match_ = true;
                        let mut j = 0usize;
                        while j < 11 {
                            if sec[i + j] != name83[j] { match_ = false; break; }
                            j += 1;
                        }
                        if match_ { return Some((VOL.root_lba + s, i)); }
                    }
                    i += 32;
                }
                s += 1;
            }
        } else {
            let mut cl = if CWD_CL == 0 { VOL.root_cl } else { CWD_CL };
            let mut guard = 0usize;
            while cl >= 2 && cl < 0x0FFF_FFF8 && guard < 64 {
                let lba0 = cluster_to_lba(cl);
                let mut s = 0u8;
                while s < VOL.sec_per_cl {
                    if !read_sec(VOL.disk, lba0 + s as u32, &mut sec) { return None; }
                    let mut i = 0usize;
                    while i < 512 {
                        if sec[i] == 0 { break; }
                        if sec[i] != 0xE5 && sec[i + 11] != 0x0F {
                            let mut match_ = true;
                            let mut j = 0usize;
                            while j < 11 {
                                if sec[i + j] != name83[j] { match_ = false; break; }
                                j += 1;
                            }
                            if match_ { return Some((lba0 + s as u32, i)); }
                        }
                        i += 32;
                    }
                    s += 1;
                }
                cl = fat_get(cl);
                guard += 1;
            }
        }
        None
    }
}

fn find_free_slot() -> Option<(u32, usize)> {
    unsafe {
        if !VOL.ok { return None; }
        let mut sec = [0u8; 512];
        if VOL.fat_type != 32 {
            let mut s = 0u32;
            while s < VOL.root_secs {
                if !read_sec(VOL.disk, VOL.root_lba + s, &mut sec) { return None; }
                let mut i = 0usize;
                while i < 512 {
                    if sec[i] == 0 || sec[i] == 0xE5 {
                        return Some((VOL.root_lba + s, i));
                    }
                    i += 32;
                }
                s += 1;
            }
        } else {
            let mut cl = if CWD_CL == 0 { VOL.root_cl } else { CWD_CL };
            let mut guard = 0usize;
            while cl >= 2 && cl < 0x0FFF_FFF8 && guard < 64 {
                let lba0 = cluster_to_lba(cl);
                let mut s = 0u8;
                while s < VOL.sec_per_cl {
                    if !read_sec(VOL.disk, lba0 + s as u32, &mut sec) { return None; }
                    let mut i = 0usize;
                    while i < 512 {
                        if sec[i] == 0 || sec[i] == 0xE5 {
                            return Some((lba0 + s as u32, i));
                        }
                        i += 32;
                    }
                    s += 1;
                }
                cl = fat_get(cl);
                guard += 1;
            }
        }
        None
    }
}

pub fn read_file(i: usize, out: &mut [u8]) -> Option<usize> {
    unsafe {
        if i >= NENT || ENTRIES[i].is_dir { return None; }
        let mut cl = ENTRIES[i].cluster;
        let size = ENTRIES[i].size as usize;
        let mut written = 0usize;
        let mut sec = [0u8; 512];
        let mut guard = 0usize;
        while written < size && cl >= 2 && cl < 0x0FFF_FFF8 && guard < 512 {
            let lba0 = cluster_to_lba(cl);
            let mut s = 0u8;
            while s < VOL.sec_per_cl && written < size && written < out.len() {
                if !read_sec(VOL.disk, lba0 + s as u32, &mut sec) { return None; }
                let take = (size - written).min(512).min(out.len() - written);
                let mut k = 0usize;
                while k < take {
                    out[written + k] = sec[k];
                    k += 1;
                }
                written += take;
                s += 1;
            }
            cl = fat_get(cl);
            guard += 1;
        }
        Some(written)
    }
}

pub fn delete_entry(i: usize) -> bool {
    unsafe {
        if i >= NENT { return false; }
        let e = ENTRIES[i];
        let name83 = to_83(&e.name[..e.name_len]);
        let slot = match find_entry_slot(&name83) {
            Some(s) => s,
            None => return false,
        };
        let mut sec = [0u8; 512];
        if !read_sec(VOL.disk, slot.0, &mut sec) { return false; }
        sec[slot.1] = 0xE5;
        if !write_sec(VOL.disk, slot.0, &sec) { return false; }
        if e.cluster >= 2 {
            free_chain(e.cluster);
        }
        list_cwd()
    }
}

pub fn rename_entry(i: usize, new_name: &[u8]) -> bool {
    unsafe {
        if i >= NENT || new_name.is_empty() { return false; }
        let e = ENTRIES[i];
        let old83 = to_83(&e.name[..e.name_len]);
        let new83 = to_83(new_name);
        let slot = match find_entry_slot(&old83) {
            Some(s) => s,
            None => return false,
        };
        let mut sec = [0u8; 512];
        if !read_sec(VOL.disk, slot.0, &mut sec) { return false; }
        let mut j = 0usize;
        while j < 11 {
            sec[slot.1 + j] = new83[j];
            j += 1;
        }
        if !write_sec(VOL.disk, slot.0, &sec) { return false; }
        list_cwd()
    }
}

pub fn write_file(name: &[u8], data: &[u8]) -> bool {
    unsafe {
        if !VOL.ok || name.is_empty() { return false; }
        let name83 = to_83(name);
        // overwrite existing
        if let Some((lba, off)) = find_entry_slot(&name83) {
            let mut sec = [0u8; 512];
            if read_sec(VOL.disk, lba, &mut sec) {
                let cl_hi = u16::from_le_bytes([sec[off + 20], sec[off + 21]]) as u32;
                let cl_lo = u16::from_le_bytes([sec[off + 26], sec[off + 27]]) as u32;
                let old_cl = (cl_hi << 16) | cl_lo;
                if old_cl >= 2 { free_chain(old_cl); }
                sec[off] = 0xE5;
                let _ = write_sec(VOL.disk, lba, &sec);
            }
        }
        let slot = match find_free_slot() {
            Some(s) => s,
            None => return false,
        };
        let mut first_cl = 0u32;
        let mut prev = 0u32;
        let mut offset = 0usize;
        let spc = VOL.sec_per_cl as usize;
        let cl_bytes = spc * 512;
        while offset < data.len() || (data.is_empty() && first_cl == 0) {
            let cl = match alloc_cluster() {
                Some(c) => c,
                None => return false,
            };
            if first_cl == 0 {
                first_cl = cl;
            } else {
                let _ = fat_set(prev, cl);
            }
            prev = cl;
            let lba0 = cluster_to_lba(cl);
            let mut s = 0usize;
            while s < spc {
                let mut sec = [0u8; 512];
                let base = offset + s * 512;
                if base < data.len() {
                    let take = (data.len() - base).min(512);
                    let mut k = 0usize;
                    while k < take {
                        sec[k] = data[base + k];
                        k += 1;
                    }
                }
                if !write_sec(VOL.disk, lba0 + s as u32, &sec) { return false; }
                s += 1;
            }
            offset += cl_bytes;
            if data.is_empty() { break; }
        }
        let eoc = if VOL.fat_type == 32 { 0x0FFF_FFFF } else { 0xFFFF };
        if prev >= 2 {
            let _ = fat_set(prev, eoc);
        }
        let mut sec = [0u8; 512];
        if !read_sec(VOL.disk, slot.0, &mut sec) { return false; }
        let mut j = 0usize;
        while j < 11 {
            sec[slot.1 + j] = name83[j];
            j += 1;
        }
        sec[slot.1 + 11] = 0x20; // archive
        sec[slot.1 + 20] = ((first_cl >> 16) & 0xFF) as u8;
        sec[slot.1 + 21] = ((first_cl >> 24) & 0xFF) as u8;
        sec[slot.1 + 26] = (first_cl & 0xFF) as u8;
        sec[slot.1 + 27] = ((first_cl >> 8) & 0xFF) as u8;
        let sz = data.len() as u32;
        sec[slot.1 + 28] = (sz & 0xFF) as u8;
        sec[slot.1 + 29] = ((sz >> 8) & 0xFF) as u8;
        sec[slot.1 + 30] = ((sz >> 16) & 0xFF) as u8;
        sec[slot.1 + 31] = ((sz >> 24) & 0xFF) as u8;
        if !write_sec(VOL.disk, slot.0, &sec) { return false; }
        list_cwd()
    }
}


/// Create a subdirectory in the current FAT directory (ATTR_DIRECTORY = 0x10).
pub fn mkdir(name: &[u8]) -> bool {
    unsafe {
        if !VOL.ok || name.is_empty() {
            return false;
        }
        let name83 = to_83(name);
        if find_entry_slot(&name83).is_some() {
            return false;
        }
        let cl = match alloc_cluster() {
            Some(c) => c,
            None => return false,
        };
        let eoc = if VOL.fat_type == 32 { 0x0FFF_FFFF } else { 0xFFFF };
        let _ = fat_set(cl, eoc);
        let lba0 = cluster_to_lba(cl);
        let spc = VOL.sec_per_cl as u32;
        let mut s = 0u32;
        while s < spc {
            let z = [0u8; 512];
            if !write_sec(VOL.disk, lba0 + s, &z) {
                return false;
            }
            s += 1;
        }
        let mut sec = [0u8; 512];
        sec[0] = b'.';
        let mut j = 1usize;
        while j < 11 {
            sec[j] = b' ';
            j += 1;
        }
        sec[11] = 0x10;
        sec[26] = (cl & 0xFF) as u8;
        sec[27] = ((cl >> 8) & 0xFF) as u8;
        sec[20] = ((cl >> 16) & 0xFF) as u8;
        sec[21] = ((cl >> 24) & 0xFF) as u8;
        sec[32] = b'.';
        sec[33] = b'.';
        j = 34;
        while j < 43 {
            sec[j] = b' ';
            j += 1;
        }
        sec[43] = 0x10;
        let parent = if CWD_CL == 0 { VOL.root_cl } else { CWD_CL };
        sec[32 + 26] = (parent & 0xFF) as u8;
        sec[32 + 27] = ((parent >> 8) & 0xFF) as u8;
        sec[32 + 20] = ((parent >> 16) & 0xFF) as u8;
        sec[32 + 21] = ((parent >> 24) & 0xFF) as u8;
        if !write_sec(VOL.disk, lba0, &sec) {
            return false;
        }
        let slot = match find_free_slot() {
            Some(s) => s,
            None => return false,
        };
        let mut psec = [0u8; 512];
        if !read_sec(VOL.disk, slot.0, &mut psec) {
            return false;
        }
        j = 0;
        while j < 11 {
            psec[slot.1 + j] = name83[j];
            j += 1;
        }
        psec[slot.1 + 11] = 0x10;
        psec[slot.1 + 26] = (cl & 0xFF) as u8;
        psec[slot.1 + 27] = ((cl >> 8) & 0xFF) as u8;
        psec[slot.1 + 20] = ((cl >> 16) & 0xFF) as u8;
        psec[slot.1 + 21] = ((cl >> 24) & 0xFF) as u8;
        if !write_sec(VOL.disk, slot.0, &psec) {
            return false;
        }
        list_cwd()
    }
}
