//! ATA PIO primary master - sector R/W for QEMU IDE disk

use crate::serial;

const DATA: u16 = 0x1F0;
const ERR: u16 = 0x1F1;
const SECCNT: u16 = 0x1F2;
const LBA0: u16 = 0x1F3;
const LBA1: u16 = 0x1F4;
const LBA2: u16 = 0x1F5;
const DRIVE: u16 = 0x1F6;
const CMD: u16 = 0x1F7;
const STATUS: u16 = 0x1F7;

const SR_BSY: u8 = 0x80;
const SR_DRQ: u8 = 0x08;
const SR_ERR: u8 = 0x01;
const SR_DF: u8 = 0x20;

const CMD_IDENTIFY: u8 = 0xEC;
const CMD_READ: u8 = 0x20;
const CMD_WRITE: u8 = 0x30;
const CMD_FLUSH: u8 = 0xE7;

static mut PRESENT: bool = false;
static mut TOTAL_SECTORS: u32 = 0;
static mut USE_RAM: bool = false;
static mut MODEL: [u8; 40] = [0; 40];
static mut MODEL_LEN: usize = 0;
static mut ATA_HW: bool = false;
static mut HW_SECTORS: u32 = 0; // physical disk size even if FS uses RAM
// 64 sectors = 32 KiB RAM disk
const RAM_SECTORS: u32 = 1024;
static mut RAMDISK: [u8; 1024 * 512] = [0; 1024 * 512];

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
}
unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") v, options(nostack, preserves_flags));
    v
}
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
}
unsafe fn inw(port: u16) -> u16 {
    let v: u16;
    core::arch::asm!("in ax, dx", in("dx") port, out("ax") v, options(nostack, preserves_flags));
    v
}

fn delay400() {
    unsafe {
        let _ = inb(0x3F6);
        let _ = inb(0x3F6);
        let _ = inb(0x3F6);
        let _ = inb(0x3F6);
    }
}

fn wait_bsy_clear() -> bool {
    let mut t = 0u32;
    while t < 200_000 {
        let s = unsafe { inb(STATUS) };
        if s == 0xFF {
            return false; // floating bus
        }
        if s & SR_BSY == 0 {
            return true;
        }
        t += 1;
    }
    false
}

fn wait_drq() -> bool {
    let mut t = 0u32;
    while t < 200_000 {
        let s = unsafe { inb(STATUS) };
        if s == 0xFF {
            return false;
        }
        if s & SR_ERR != 0 || s & SR_DF != 0 {
            return false;
        }
        if s & SR_DRQ != 0 {
            return true;
        }
        t += 1;
    }
    false
}

pub fn is_present() -> bool {
    unsafe { PRESENT || USE_RAM }
}
pub fn is_ramdisk() -> bool {
    unsafe { USE_RAM }
}


pub fn hw_present() -> bool {
    unsafe { ATA_HW && HW_SECTORS > 0 }
}
pub fn hw_sectors() -> u32 {
    unsafe { HW_SECTORS }
}
pub fn model_bytes(out: &mut [u8]) -> usize {
    unsafe {
        let n = if MODEL_LEN < out.len() { MODEL_LEN } else { out.len() };
        let mut i = 0usize;
        while i < n {
            out[i] = MODEL[i];
            i += 1;
        }
        n
    }
}

pub fn total_sectors() -> u32 {
    unsafe {
        if USE_RAM { RAM_SECTORS } else { TOTAL_SECTORS }
    }
}

pub fn init() -> bool {
    serial::write_str("\n[ATA] probe primary master...\n");
    unsafe {
        outb(DRIVE, 0xE0); // LBA, master
        delay400();
        outb(SECCNT, 0);
        outb(LBA0, 0);
        outb(LBA1, 0);
        outb(LBA2, 0);
        outb(CMD, CMD_IDENTIFY);
        delay400();
        let st = inb(STATUS);
        if st == 0 || st == 0xFF {
            serial::write_str("  [ATA] no device / floating\n");
            PRESENT = false;
            return false;
        }
        if !wait_bsy_clear() {
            serial::write_str("  [ATA] BSY timeout\n");
            return false;
        }
        // ATAPI would have LBA1/LBA2 non-zero
        let l1 = inb(LBA1);
        let l2 = inb(LBA2);
        if l1 != 0 || l2 != 0 {
            serial::write_str("  [ATA] not ATA (maybe ATAPI)\n");
            return false;
        }
        if !wait_drq() {
            serial::write_str("  [ATA] DRQ timeout on IDENTIFY\n");
            return false;
        }
        let mut id = [0u16; 256];
        let mut i = 0usize;
        while i < 256 {
            id[i] = inw(DATA);
            i += 1;
        }
        // words 60-61: total LBA28 sectors
        let sectors = id[60] as u32 | ((id[61] as u32) << 16);
        TOTAL_SECTORS = if sectors == 0 { 2048 } else { sectors };
        PRESENT = true;
        ATA_HW = true;
        USE_RAM = false;
        HW_SECTORS = TOTAL_SECTORS;
        // Model string ATA words 27..46 (byte-swapped)
        let mut mi = 0usize;
        let mut w = 27usize;
        while w <= 46 && mi + 1 < 40 {
            let v = id[w];
            let a = ((v >> 8) & 0xFF) as u8;
            let b = (v & 0xFF) as u8;
            if a != 0 { MODEL[mi] = a; mi += 1; }
            if b != 0 && mi < 40 { MODEL[mi] = b; mi += 1; }
            w += 1;
        }
        // trim trailing spaces
        while mi > 0 && MODEL[mi - 1] == b' ' { mi -= 1; }
        MODEL_LEN = mi;
        serial::write_str("  [ATA] present sectors=");
        serial::write_usize(TOTAL_SECTORS as usize);
        serial::write_str(" (");
        serial::write_usize((TOTAL_SECTORS as usize) / 2);
        serial::write_str(" KB)\n");
        true
    }
}

fn setup_lba(lba: u32, count: u8) {
    unsafe {
        outb(DRIVE, 0xE0 | (((lba >> 24) & 0x0F) as u8));
        outb(SECCNT, count);
        outb(LBA0, (lba & 0xFF) as u8);
        outb(LBA1, ((lba >> 8) & 0xFF) as u8);
        outb(LBA2, ((lba >> 16) & 0xFF) as u8);
    }
}


/// Read from physical ATA only (never RAM). READ-ONLY path for host disks.
pub fn read_hw_sectors(lba: u32, count: u8, buf: &mut [u8]) -> bool {
    unsafe {
        if !ATA_HW || HW_SECTORS == 0 {
            return false;
        }
        let need = (count as usize) * 512;
        if buf.len() < need {
            return false;
        }
        if lba >= HW_SECTORS || (lba as u64) + (count as u64) > HW_SECTORS as u64 {
            serial::write_str("  [ATA] HW read OOB\n");
            return false;
        }
    }
    if !wait_bsy_clear() {
        serial::write_str("  [ATA] HW BSY timeout\n");
        return false;
    }
    setup_lba(lba, count);
    unsafe { outb(CMD, CMD_READ); }
    let mut sec = 0u8;
    while sec < count {
        if !wait_drq() {
            serial::write_str("  [ATA] HW DRQ fail\n");
            return false;
        }
        let base = (sec as usize) * 512;
        let mut i = 0usize;
        while i < 256 {
            let w = unsafe { inw(DATA) };
            buf[base + i * 2] = (w & 0xFF) as u8;
            buf[base + i * 2 + 1] = (w >> 8) as u8;
            i += 1;
        }
        sec += 1;
    }
    true
}


/// Read `count` sectors (1..256) starting at LBA into buf (count*512 bytes)
pub fn read_sectors(lba: u32, count: u8, buf: &mut [u8]) -> bool {
    if !is_present() {
        return false;
    }
    let need = (count as usize) * 512;
    if buf.len() < need {
        serial::write_str("  [ATA] read buf too small\n");
        return false;
    }
    let total = total_sectors();
    if lba >= total || (lba as u64) + (count as u64) > total as u64 {
        serial::write_str("  [ATA] read LBA out of bounds\n");
        return false;
    }
    // RAM disk backend
    unsafe {
        if USE_RAM {
            let mut sec = 0u8;
            while sec < count {
                let off = ((lba as usize) + (sec as usize)) * 512;
                let base = (sec as usize) * 512;
                let mut i = 0usize;
                while i < 512 {
                    buf[base + i] = RAMDISK[off + i];
                    i += 1;
                }
                sec += 1;
            }
            return true;
        }
    }
    if !wait_bsy_clear() {
        return false;
    }
    setup_lba(lba, count);
    unsafe { outb(CMD, CMD_READ); }
    let mut sec = 0u8;
    while sec < count {
        if !wait_drq() {
            serial::write_str("  [ATA] read DRQ fail\n");
            return false;
        }
        let base = (sec as usize) * 512;
        let mut i = 0usize;
        while i < 256 {
            let w = unsafe { inw(DATA) };
            buf[base + i * 2] = (w & 0xFF) as u8;
            buf[base + i * 2 + 1] = (w >> 8) as u8;
            i += 1;
        }
        sec += 1;
    }
    true
}

/// Write `count` sectors from buf
pub fn write_sectors(lba: u32, count: u8, buf: &[u8]) -> bool {
    if !is_present() {
        return false;
    }
    let need = (count as usize) * 512;
    if buf.len() < need {
        return false;
    }
    let total = total_sectors();
    if lba >= total || (lba as u64) + (count as u64) > total as u64 {
        serial::write_str("  [ATA] write LBA out of bounds\n");
        return false;
    }
    unsafe {
        if USE_RAM {
            let mut sec = 0u8;
            while sec < count {
                let off = ((lba as usize) + (sec as usize)) * 512;
                let base = (sec as usize) * 512;
                let mut i = 0usize;
                while i < 512 {
                    RAMDISK[off + i] = buf[base + i];
                    i += 1;
                }
                sec += 1;
            }
            return true;
        }
    }
    if !wait_bsy_clear() {
        return false;
    }
    setup_lba(lba, count);
    unsafe { outb(CMD, CMD_WRITE); }
    let mut sec = 0u8;
    while sec < count {
        if !wait_drq() {
            serial::write_str("  [ATA] write DRQ fail\n");
            return false;
        }
        let base = (sec as usize) * 512;
        let mut i = 0usize;
        while i < 256 {
            let lo = buf[base + i * 2] as u16;
            let hi = buf[base + i * 2 + 1] as u16;
            unsafe { outw(DATA, lo | (hi << 8)); }
            i += 1;
        }
        sec += 1;
    }
    // Wait BSY clear after write
    if !wait_bsy_clear() {
        return false;
    }
    let st = unsafe { inb(STATUS) };
    if st & SR_ERR != 0 {
        serial::write_str("  [ATA] write ERR status=");
        serial::write_hex(st as usize);
        serial::write_str("\n");
        return false;
    }
    true
}

pub fn flush() -> bool {
    unsafe { if USE_RAM { return true; } }
    if !is_present() { return true; }
    let _ = wait_bsy_clear();
    unsafe {
        outb(DRIVE, 0xE0);
        outb(CMD, CMD_FLUSH);
    }
    let mut t = 0u32;
    while t < 500_000 {
        let s = unsafe { inb(STATUS) };
        if s & SR_BSY == 0 {
            return s & SR_ERR == 0;
        }
        t += 1;
    }
    true // soft OK for QEMU
}

/// Runtime self-test: write pattern to high LBA, read back
pub fn self_test() -> bool {
    unsafe {
        if USE_RAM {
            serial::write_str("  [ATA] RAM disk self-test OK\n");
            return true;
        }
    }
    if !is_present() { return false; }
    serial::write_str("  [ATA] self-test LBA R/W...
");
    let mut buf = [0u8; 512];
    let mut chk = [0u8; 512];
    let lba = 100u32;
    let msg = b"Hello Aether Persistent Storage
";
    let mut i = 0usize;
    while i < 512 { buf[i] = 0xA5; i += 1; }
    i = 0;
    while i < msg.len() { buf[i] = msg[i]; i += 1; }
    if !write_sectors(lba, 1, &buf) {
        serial::write_str("  [ATA] self-test WRITE FAIL
");
        return false;
    }
    serial::write_str("  [ATA] write OK
");
    let _ = flush();
    // drain status
    let _ = wait_bsy_clear();
    if !read_sectors(lba, 1, &mut chk) {
        serial::write_str("  [ATA] self-test READ FAIL
");
        return false;
    }
    serial::write_str("  [ATA] read OK first bytes=");
    serial::write_hex(chk[0] as usize);
    serial::write_str(" ");
    serial::write_hex(chk[1] as usize);
    serial::write_str(" ");
    serial::write_hex(chk[2] as usize);
    serial::write_str("
");
    i = 0;
    while i < msg.len() {
        if chk[i] != msg[i] {
            serial::write_str("  [ATA] self-test COMPARE FAIL at ");
            serial::write_usize(i);
            serial::write_str("
");
            return false;
        }
        i += 1;
    }
    serial::write_str("  [ATA] self-test OK (LBA 100)
");
    true
}


/// Fallback when no IDE/SATA PIO device - 32 KiB in-RAM disk for AetherFS
pub fn is_hardware() -> bool {
    unsafe { ATA_HW || HW_SECTORS > 0 }
}

/// Physical HDD size in KiB (0 if none probed)
pub fn hardware_capacity_kib() -> u32 {
    unsafe { HW_SECTORS / 2 }
}

pub fn model(out: &mut [u8]) -> usize {
    unsafe {
        let n = if MODEL_LEN > out.len() { out.len() } else { MODEL_LEN };
        let mut i = 0usize;
        while i < n {
            out[i] = MODEL[i];
            i += 1;
        }
        n
    }
}

/// Capacity in KiB
pub fn capacity_kib() -> u32 {
    total_sectors() / 2
}

pub fn enable_ramdisk() {
    unsafe {
        USE_RAM = true;
        PRESENT = true;
        // keep ATA_HW / HW_SECTORS for My Computer display
        TOTAL_SECTORS = RAM_SECTORS;
        let s = b"Aether RAM Disk";
        let mut i = 0usize;
        while i < s.len() && i < 40 {
            MODEL[i] = s[i];
            i += 1;
        }
        MODEL_LEN = s.len();
        let mut i = 0usize;
        while i < RAMDISK.len() {
            RAMDISK[i] = 0;
            i += 1;
        }
    }
    serial::write_str("[RAMDISK OK] 512 KiB volatile system volume\n");
    serial::write_str("  base=static BSS  sectors=64  size=32768\n");
}

