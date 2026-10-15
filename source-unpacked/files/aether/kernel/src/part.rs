//! MBR + GPT partition table parser (READ-ONLY)
use crate::block;
use crate::serial;

#[derive(Clone, Copy)]
pub struct Partition {
    pub disk: u8,
    pub index: u8,
    pub ptype: u8,       // MBR type or 0xEE for GPT member
    pub lba_start: u32,
    pub sectors: u32,
    pub active: bool,
    pub is_gpt: bool,
    pub gpt_type: [u8; 16], // GPT type GUID first 4 bytes used for classify
    pub name: [u8; 16],
    pub name_len: usize,
}

static mut PARTS: [Partition; 16] = [Partition {
    disk: 0, index: 0, ptype: 0, lba_start: 0, sectors: 0, active: false,
    is_gpt: false, gpt_type: [0; 16], name: [0; 16], name_len: 0,
}; 16];
static mut NPART: usize = 0;

pub fn count() -> usize {
    unsafe { NPART }
}

pub fn get(i: usize) -> Option<Partition> {
    unsafe {
        if i < NPART {
            Some(PARTS[i])
        } else {
            None
        }
    }
}

fn push(p: Partition) {
    unsafe {
        if NPART < 16 {
            PARTS[NPART] = p;
            NPART += 1;
        }
    }
}

fn scan_mbr(disk: u8, mbr: &[u8; 512]) {
    let mut pi = 0usize;
    while pi < 4 {
        let off = 446 + pi * 16;
        let ptype = mbr[off + 4];
        let lba = u32::from_le_bytes([mbr[off + 8], mbr[off + 9], mbr[off + 10], mbr[off + 11]]);
        let secs = u32::from_le_bytes([mbr[off + 12], mbr[off + 13], mbr[off + 14], mbr[off + 15]]);
        if ptype == 0xEE {
            // Protective GPT - parse GPT
            scan_gpt(disk);
            return;
        }
        if ptype != 0 && secs > 0 {
            push(Partition {
                disk,
                index: pi as u8,
                ptype,
                lba_start: lba,
                sectors: secs,
                active: mbr[off] == 0x80,
                is_gpt: false,
                gpt_type: [0; 16],
                name: [0; 16],
                name_len: 0,
            });
            serial::write_str("  MBR part type=0x");
            serial::write_hex(ptype as usize);
            serial::write_str(" LBA=");
            serial::write_usize(lba as usize);
            serial::write_str("\n");
        }
        pi += 1;
    }
}

fn scan_gpt(disk: u8) {
    let mut hdr = [0u8; 512];
    if !block::read(disk, 1, 1, &mut hdr) {
        serial::write_str("[PART] GPT header read fail\n");
        return;
    }
    if hdr[0]!=b'E' || hdr[1]!=b'F' || hdr[2]!=b'I' {
        serial::write_str("[PART] bad GPT sig\n");
        return;
    }
    let part_lba = u64::from_le_bytes([
        hdr[72], hdr[73], hdr[74], hdr[75], hdr[76], hdr[77], hdr[78], hdr[79],
    ]);
    let part_count = u32::from_le_bytes([hdr[80], hdr[81], hdr[82], hdr[83]]);
    let part_entsz = u32::from_le_bytes([hdr[84], hdr[85], hdr[86], hdr[87]]);
    if part_entsz < 128 || part_count == 0 || part_count > 128 {
        return;
    }
    serial::write_str("[PART] GPT OK entries=");
    serial::write_usize(part_count as usize);
    serial::write_str("\n");
    let ents_per_sec = 512 / part_entsz as usize;
    if ents_per_sec == 0 {
        return;
    }
    let mut idx = 0u32;
    while idx < part_count && idx < 32 {
        let sec = (part_lba as u32) + (idx / ents_per_sec as u32);
        let mut buf = [0u8; 512];
        if !block::read(disk, sec, 1, &mut buf) {
            break;
        }
        let off = ((idx as usize) % ents_per_sec) * part_entsz as usize;
        // type GUID all zero = empty
        let mut empty = true;
        let mut t = 0usize;
        while t < 16 {
            if buf[off + t] != 0 {
                empty = false;
                break;
            }
            t += 1;
        }
        if !empty {
            let start = u64::from_le_bytes([
                buf[off + 32], buf[off + 33], buf[off + 34], buf[off + 35],
                buf[off + 36], buf[off + 37], buf[off + 38], buf[off + 39],
            ]);
            let end = u64::from_le_bytes([
                buf[off + 40], buf[off + 41], buf[off + 42], buf[off + 43],
                buf[off + 44], buf[off + 45], buf[off + 46], buf[off + 47],
            ]);
            let secs = if end >= start { (end - start + 1) as u32 } else { 0 };
            let mut gpt_type = [0u8; 16];
            let mut i = 0usize;
            while i < 16 {
                gpt_type[i] = buf[off + i];
                i += 1;
            }
            // Classify by first bytes of common GUIDs
            let ptype = classify_gpt(&gpt_type);
            push(Partition {
                disk,
                index: idx as u8,
                ptype,
                lba_start: start as u32,
                sectors: secs,
                active: false,
                is_gpt: true,
                gpt_type,
                name: [0; 16],
                name_len: 0,
            });
            serial::write_str("  GPT part type=0x");
            serial::write_hex(ptype as usize);
            serial::write_str(" LBA=");
            serial::write_usize(start as usize);
            serial::write_str(" secs=");
            serial::write_usize(secs as usize);
            serial::write_str("\n");
        }
        idx += 1;
    }
}

fn classify_gpt(guid: &[u8; 16]) -> u8 {
    // Microsoft basic data: EBD0A0A2-B9E5-4433-87C0-68B6B72699C7
    if guid[0] == 0xA2 && guid[1] == 0xA0 && guid[2] == 0xD0 && guid[3] == 0xEB {
        return 0x07; // NTFS/exFAT
    }
    // EFI system: C12A7328-F81F-11D2-BA4B-00A0C93EC93B
    if guid[0] == 0x28 && guid[1] == 0x73 && guid[2] == 0x2A && guid[3] == 0xC1 {
        return 0xEF; // EFI FAT
    }
    // Linux filesystem: 0FC63DAF-8483-4772-8E79-3D69D8477DE4
    if guid[0] == 0xAF && guid[1] == 0x3D && guid[2] == 0xC6 && guid[3] == 0x0F {
        return 0x83;
    }
    // Linux swap
    if guid[0] == 0x6D && guid[1] == 0xFD && guid[2] == 0xA0 && guid[3] == 0x6B {
        return 0x82;
    }
    0x00
}

pub fn scan() {
    unsafe {
        NPART = 0;
    }
    let nd = block::count();
    let mut di = 0usize;
    while di < nd {
        let mut mbr = [0u8; 512];
        if !block::read(di as u8, 0, 1, &mut mbr) {
            serial::write_str("[PART] MBR read fail disk=");
            serial::write_usize(di);
            serial::write_str("\n");
            di += 1;
            continue;
        }
        if mbr[510] != 0x55 || mbr[511] != 0xAA {
            // try GPT anyway at LBA1
            let mut hdr = [0u8; 512];
            if block::read(di as u8, 1, 1, &mut hdr) && &hdr[0..8] == b"EFI PART" {
                scan_gpt(di as u8);
            } else {
                serial::write_str("[PART] no MBR/GPT disk=");
                serial::write_usize(di);
                serial::write_str("\n");
            }
            di += 1;
            continue;
        }
        serial::write_str("[PART] MBR OK disk=");
        serial::write_usize(di);
        serial::write_str("\n");
        scan_mbr(di as u8, &mbr);
        di += 1;
    }
}

pub fn type_name(t: u8) -> &'static str {
    match t {
        0x01 | 0x04 | 0x06 | 0x0E => "FAT12/16",
        0x0B | 0x0C => "FAT32",
        0x07 => "NTFS/exFAT",
        0x83 => "Linux",
        0x82 => "Linux swap",
        0x05 | 0x0F => "Extended",
        0xEF => "EFI System",
        0xEE => "GPT protective",
        _ => "Unknown",
    }
}
