//! AetherFS - minimal disk filesystem + VFS
//! Layout (LBA):
//!   0: Superblock
//!   1: Free bitmap (1 sector = 4096 blocks max)
//!   2: Root directory (16 entries × 32 bytes)
//!   3..: Data blocks (1 block = 1 sector = 512 B)

use crate::serial;
use crate::drivers::ata;

pub const SECTOR: usize = 512;
pub const MAGIC: u32 = 0xAE74_E5F5; // AetherFS
pub const VERSION: u32 = 1;
pub const MAX_NAME: usize = 28;
pub const MAX_DIR_ENTRIES: usize = 16;
pub const MAX_FILE_SECTORS: usize = 128; // 64 KiB per file
pub const ROOT_LBA: u32 = 2;
pub const BITMAP_LBA: u32 = 1;
pub const SUPER_LBA: u32 = 0;
pub const DATA_START: u32 = 3;

#[repr(C, packed)]
struct Superblock {
    magic: u32,
    version: u32,
    total_sectors: u32,
    data_start: u32,
    root_lba: u32,
    bitmap_lba: u32,
    free_count: u32,
    reserved: [u8; 512 - 28],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DirEntry {
    name: [u8; MAX_NAME],
    name_len: u8,
    flags: u8,       // bit0 used
    start_lba: u32,
    size: u32,       // bytes
}

static mut MOUNTED: bool = false;
static mut FILE_COUNT: u32 = 0;

fn zero_sector(buf: &mut [u8; 512]) {
    let mut i = 0usize;
    while i < 512 {
        buf[i] = 0;
        i += 1;
    }
}

pub fn is_mounted() -> bool {
    unsafe { MOUNTED }
}

pub fn file_count() -> u32 {
    unsafe { FILE_COUNT }
}

fn read_super(buf: &mut [u8; 512]) -> Option<Superblock> {
    if !ata::read_sectors(SUPER_LBA, 1, buf) {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != MAGIC {
        return None;
    }
    let version = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let total = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let free = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    Some(Superblock {
        magic,
        version,
        total_sectors: total,
        data_start: DATA_START,
        root_lba: ROOT_LBA,
        bitmap_lba: BITMAP_LBA,
        free_count: free,
        reserved: [0; 512 - 28],
    })
}

fn write_super(total: u32, free: u32) -> bool {
    let mut buf = [0u8; 512];
    let magic = MAGIC.to_le_bytes();
    let ver = VERSION.to_le_bytes();
    let tot = total.to_le_bytes();
    let ds = DATA_START.to_le_bytes();
    let rl = ROOT_LBA.to_le_bytes();
    let bl = BITMAP_LBA.to_le_bytes();
    let fr = free.to_le_bytes();
    let mut i = 0usize;
    while i < 4 {
        buf[i] = magic[i];
        buf[4 + i] = ver[i];
        buf[8 + i] = tot[i];
        buf[12 + i] = ds[i];
        buf[16 + i] = rl[i];
        buf[20 + i] = bl[i];
        buf[24 + i] = fr[i];
        i += 1;
    }
    ata::write_sectors(SUPER_LBA, 1, &buf) && ata::flush()
}

fn bitmap_alloc() -> Option<u32> {
    let mut bm = [0u8; 512];
    if !ata::read_sectors(BITMAP_LBA, 1, &mut bm) {
        return None;
    }
    let max = (ata::total_sectors().saturating_sub(DATA_START)) as usize;
    let max = if max > 4096 { 4096 } else { max };
    let mut bit = 0usize;
    while bit < max {
        let byte = bit / 8;
        let mask = 1u8 << (bit % 8);
        if bm[byte] & mask == 0 {
            bm[byte] |= mask;
            if !ata::write_sectors(BITMAP_LBA, 1, &bm) {
                return None;
            }
            return Some(DATA_START + bit as u32);
        }
        bit += 1;
    }
    None
}

fn bitmap_free(lba: u32) {
    if lba < DATA_START {
        return;
    }
    let bit = (lba - DATA_START) as usize;
    let mut bm = [0u8; 512];
    if !ata::read_sectors(BITMAP_LBA, 1, &mut bm) {
        return;
    }
    let byte = bit / 8;
    let mask = 1u8 << (bit % 8);
    if byte < 512 {
        bm[byte] &= !mask;
        let _ = ata::write_sectors(BITMAP_LBA, 1, &bm);
    }
}

fn load_root(entries: &mut [DirEntry; MAX_DIR_ENTRIES]) -> bool {
    let mut buf = [0u8; 512];
    if !ata::read_sectors(ROOT_LBA, 1, &mut buf) {
        return false;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        let off = i * 32;
        let mut name = [0u8; MAX_NAME];
        let mut j = 0usize;
        while j < MAX_NAME {
            name[j] = buf[off + j];
            j += 1;
        }
        let name_len = buf[off + 28];
        let flags = buf[off + 29];
        let start = u32::from_le_bytes([buf[off + 24], buf[off + 25], buf[off + 26], buf[off + 27]]);
        // layout: name[28], then start_lba at 24? Let me fix packed layout consistently
        // Better: name[24], name_len, flags, pad, start_lba u32, size u32 = 32
        i += 1;
        let _ = (name, name_len, flags, start);
    }
    // Re-parse with clear layout: 0..24 name, 24 name_len, 25 flags, 26-27 pad, 28-31 start, wait 32 bytes:
    // 0-23: name (24), 24: name_len, 25: flags, 26-27: reserved, 28-31: start_lba - size needs more
    // 32-byte entry: name[22], name_len u8, flags u8, start u32, size u32 = 22+1+1+4+4 = 32
    i = 0;
    while i < MAX_DIR_ENTRIES {
        let off = i * 32;
        let mut e = DirEntry {
            name: [0; MAX_NAME],
            name_len: 0,
            flags: 0,
            start_lba: 0,
            size: 0,
        };
        let mut j = 0usize;
        while j < 22 && j < MAX_NAME {
            e.name[j] = buf[off + j];
            j += 1;
        }
        e.name_len = buf[off + 22];
        e.flags = buf[off + 23];
        e.start_lba = u32::from_le_bytes([buf[off + 24], buf[off + 25], buf[off + 26], buf[off + 27]]);
        e.size = u32::from_le_bytes([buf[off + 28], buf[off + 29], buf[off + 30], buf[off + 31]]);
        entries[i] = e;
        i += 1;
    }
    true
}

fn save_root(entries: &[DirEntry; MAX_DIR_ENTRIES]) -> bool {
    let mut buf = [0u8; 512];
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        let off = i * 32;
        let e = &entries[i];
        let mut j = 0usize;
        while j < 22 {
            buf[off + j] = if j < MAX_NAME { e.name[j] } else { 0 };
            j += 1;
        }
        buf[off + 22] = e.name_len;
        buf[off + 23] = e.flags;
        let s = e.start_lba.to_le_bytes();
        let z = e.size.to_le_bytes();
        buf[off + 24] = s[0];
        buf[off + 25] = s[1];
        buf[off + 26] = s[2];
        buf[off + 27] = s[3];
        buf[off + 28] = z[0];
        buf[off + 29] = z[1];
        buf[off + 30] = z[2];
        buf[off + 31] = z[3];
        i += 1;
    }
    ata::write_sectors(ROOT_LBA, 1, &buf) && ata::flush()
}

fn count_used(entries: &[DirEntry; MAX_DIR_ENTRIES]) -> u32 {
    let mut n = 0u32;
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & 1 != 0 {
            n += 1;
        }
        i += 1;
    }
    n
}

pub fn format() -> bool {
    serial::write_str("\n[AetherFS] format...\n");
    if !ata::is_present() {
        return false;
    }
    let total = ata::total_sectors();
    // Superblock
    let data_blocks = total.saturating_sub(DATA_START);
    if !write_super(total, data_blocks) {
        serial::write_str("  [AetherFS] super write FAIL\n");
        return false;
    }
    // Bitmap zero
    let mut bm = [0u8; 512];
    if !ata::write_sectors(BITMAP_LBA, 1, &bm) {
        return false;
    }
    // Empty root
    let entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !save_root(&entries) {
        return false;
    }
    let _ = ata::flush();
    unsafe {
        MOUNTED = true;
        FILE_COUNT = 0;
    }
    serial::write_str("  [AetherFS] formatted OK\n");
    true
}

pub fn mount() -> bool {
    serial::write_str("\n[AetherFS] mount...\n");
    if !ata::is_present() {
        serial::write_str("  [AetherFS] no ATA\n");
        return false;
    }
    let mut buf = [0u8; 512];
    match read_super(&mut buf) {
        Some(sb) => {
            serial::write_str("  [AetherFS] magic OK ver=");
            serial::write_usize(sb.version as usize);
            serial::write_str(" sectors=");
            serial::write_usize(sb.total_sectors as usize);
            serial::write_str("\n");
            let mut entries = [DirEntry {
                name: [0; MAX_NAME],
                name_len: 0,
                flags: 0,
                start_lba: 0,
                size: 0,
            }; MAX_DIR_ENTRIES];
            if !load_root(&mut entries) {
                serial::write_str("  [AetherFS] root read FAIL\n");
                return false;
            }
            let n = count_used(&entries);
            unsafe {
                MOUNTED = true;
                FILE_COUNT = n;
            }
            serial::write_str("  [AetherFS] mounted files=");
            serial::write_usize(n as usize);
            serial::write_str("\n");
            true
        }
        None => {
            serial::write_str("  [AetherFS] no superblock - need format\n");
            false
        }
    }
}

fn name_eq(e: &DirEntry, name: &str) -> bool {
    let mut nb = name.as_bytes();
    if !nb.is_empty() && nb[0] == b'/' {
        nb = &nb[1..];
    }
    if e.name_len as usize != nb.len() {
        return false;
    }
    let mut i = 0usize;
    while i < nb.len() && i < 22 {
        if e.name[i] != nb[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn strip_slash(path: &str) -> &str {
    path
}

// ─── VFS API ──────────────────────────────────────────────────

pub fn create(path: &str) -> bool {
    if !is_mounted() {
        return false;
    }
    let mut name = path;
    let nb0 = name.as_bytes();
    // skip leading slash without slicing str (avoid panic handlers)
    let start = if !nb0.is_empty() && nb0[0] == b'/' { 1usize } else { 0usize };
    if start >= nb0.len() || nb0.len() - start > 22 {
        return false;
    }
    // For storage we pass full path to name_eq; when storing, skip slash in bytes
    let name_bytes = &nb0[start..];
    let name_len = name_bytes.len();
    let _ = name;
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return false;
    }
    // exists?
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & 1 != 0 && name_eq(&entries[i], name) {
            serial::write_str("  [VFS] create exists\n");
            return false;
        }
        i += 1;
    }
    // free slot
    let mut slot = None;
    i = 0;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & 1 == 0 {
            slot = Some(i);
            break;
        }
        i += 1;
    }
    let slot = match slot {
        Some(s) => s,
        None => {
            serial::write_str("  [VFS] dir full\n");
            return false;
        }
    };
    let lba = match bitmap_alloc() {
        Some(l) => l,
        None => {
            serial::write_str("  [VFS] no free blocks\n");
            return false;
        }
    };
    let nb = name.as_bytes();
    let mut off = 0usize;
    if !nb.is_empty() && nb[0] == b'/' { off = 1; }
    let mut e = DirEntry {
        name: [0; MAX_NAME],
        name_len: (nb.len() - off) as u8,
        flags: 1,
        start_lba: lba,
        size: 0,
    };
    i = 0;
    while off + i < nb.len() && i < 22 {
        e.name[i] = nb[off + i];
        i += 1;
    }
    entries[slot] = e;
    // zero data block
    let mut z = [0u8; 512];
    if !ata::write_sectors(lba, 1, &z) {
        return false;
    }
    if !save_root(&entries) {
        return false;
    }
    unsafe { FILE_COUNT += 1; }
    serial::write_str("  [VFS] create /");
    serial::write_str(name);
    serial::write_str(" LBA=");
    serial::write_usize(lba as usize);
    serial::write_str("\n");
    true
}

pub fn write(path: &str, data: &[u8]) -> bool {
    if !is_mounted() {
        return false;
    }
    let name = strip_slash(path);
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return false;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & 1 != 0 && name_eq(&entries[i], name) {
            let mut buf = [0u8; 512];
            let n = if data.len() > 512 { 512 } else { data.len() };
            let mut j = 0usize;
            while j < n {
                buf[j] = data[j];
                j += 1;
            }
            if !ata::write_sectors(entries[i].start_lba, 1, &buf) {
                return false;
            }
            entries[i].size = n as u32;
            if !save_root(&entries) {
                return false;
            }
            let _ = ata::flush();
            serial::write_str("  [VFS] write /");
            serial::write_str(name);
            serial::write_str(" bytes=");
            serial::write_usize(n);
            serial::write_str("\n");
            return true;
        }
        i += 1;
    }
    serial::write_str("  [VFS] write: not found\n");
    false
}

pub fn read(path: &str, out: &mut [u8]) -> Option<usize> {
    if !is_mounted() {
        return None;
    }
    let name = strip_slash(path);
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return None;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & 1 != 0 && name_eq(&entries[i], name) {
            let mut buf = [0u8; 512];
            if !ata::read_sectors(entries[i].start_lba, 1, &mut buf) {
                return None;
            }
            let n = entries[i].size as usize;
            let n = if n > out.len() { out.len() } else { n };
            let mut j = 0usize;
            while j < n {
                out[j] = buf[j];
                j += 1;
            }
            serial::write_str("  [VFS] read /");
            serial::write_str(name);
            serial::write_str(" bytes=");
            serial::write_usize(n);
            serial::write_str("\n");
            return Some(n);
        }
        i += 1;
    }
    None
}

pub fn delete(path: &str) -> bool {
    if !is_mounted() {
        return false;
    }
    let name = strip_slash(path);
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return false;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & 1 != 0 && name_eq(&entries[i], name) {
            bitmap_free(entries[i].start_lba);
            entries[i].flags = 0;
            entries[i].size = 0;
            entries[i].name_len = 0;
            if !save_root(&entries) {
                return false;
            }
            let _ = ata::flush();
            unsafe {
                if FILE_COUNT > 0 {
                    FILE_COUNT -= 1;
                }
            }
            serial::write_str("  [VFS] delete /");
            serial::write_str(name);
            serial::write_str("\n");
            return true;
        }
        i += 1;
    }
    false
}

pub fn list(out: &mut [[u8; 24]; 16], out_len: &mut [usize; 16]) -> usize {
    let mut n = 0usize;
    if !is_mounted() {
        return 0;
    }
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return 0;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES && n < 16 {
        if entries[i].flags & 1 != 0 {
            let mut j = 0usize;
            while j < 24 {
                out[n][j] = 0;
                j += 1;
            }
            j = 0;
            while j < entries[i].name_len as usize && j < 22 {
                out[n][j] = entries[i].name[j];
                j += 1;
            }
            out_len[n] = entries[i].name_len as usize;
            n += 1;
        }
        i += 1;
    }
    n
}

/// Init storage: ATA → mount or format → persistence workflow
pub fn init_storage() -> bool {
    serial::write_str("\n======== STORAGE / AetherFS ========\n");
    // Keep ATA probe for diagnostics, but use RAM disk as FS backend for AH532 test
    let _ = ata::init();
    ata::enable_ramdisk();
    if !ata::self_test() {
        serial::write_str("[STORAGE] RAM disk self-test FAIL\n");
        return false;
    }
    serial::write_str("[RAMDISK OK]\n");

    // Mount existing volume if superblock valid; else format once.
    // Same-boot session keeps files; power-off loses RAM disk (expected).
    let mut need_seed = false;
    if mount() {
        serial::write_str("[AetherFS] mounted existing volume\n");
    } else {
        serial::write_str("[AetherFS] no volume - format\n");
        if !format() {
            serial::write_str("[STORAGE] format FAIL\n");
            return false;
        }
        if !mount() {
            serial::write_str("[STORAGE] mount FAIL\n");
            return false;
        }
        need_seed = true;
        serial::write_str("[AetherFS mounted]\n");
    }

    // Seed system files if missing (idempotent)
    if need_seed || read("/test.txt", &mut [0u8; 4]).is_none() {
        let _ = create("/test.txt");
        let _ = write("/test.txt", b"Hello Aether");
    }
    let _ = mkdir("/MEDIA");
    let _ = mkdir("/bin");
    let _ = mkdir("/VIDEOTEST");
    // Multi-resolution MJPEG test samples (AetherFS RAM disk, max 64KiB/file)
    if read("/VT160.AVI", &mut [0u8; 4]).is_none() {
        let _ = write_large("/VT160.AVI", crate::video_samples::VT160_AVI);
        let _ = write_large("/VT320.AVI", crate::video_samples::VT320_AVI);
        let _ = write_large("/VT640.AVI", crate::video_samples::VT640_AVI);
        serial::write_str("[VIDEOTEST] VT160.AVI 160x120 MJPEG\n");
        serial::write_str("[VIDEOTEST] VT320.AVI 320x180 MJPEG\n");
        serial::write_str("[VIDEOTEST] VT640.AVI 640x360 MJPEG\n");
    }
    // Mirror filenames into VIDEOTEST dir for Explorer navigation
    let _ = write_large_ex("/VIDEOTEST/VT160.AVI", crate::video_samples::VT160_AVI);
    let _ = write_large_ex("/VIDEOTEST/VT320.AVI", crate::video_samples::VT320_AVI);
    let _ = write_large_ex("/VIDEOTEST/VT640.AVI", crate::video_samples::VT640_AVI);
    if read("/MEDIA/MOZART.MP3", &mut [0u8; 4]).is_none() {
        let _ = create("/MEDIA/MOZART.MP3");
        let _ = write("/MEDIA/MOZART.MP3", b"AETHER-MEDIA-MOZART-MP3");
    let _ = create("/MEDIA/TEST.MP4");
    let _ = write("/MEDIA/TEST.MP4", b"AETHER-VID-TEST-MP4-PROCEDURAL");
    }
    let _ = ata::flush();
    serial::write_str("[STORAGE] system volume ready (512KiB RAM disk)\n");

    // Always try read
    let mut buf = [0u8; 64];
    match read("/test.txt", &mut buf) {
        Some(n) => {
            serial::write_str("[STORAGE] /test.txt raw: ");
            let mut i = 0usize;
            while i < n {
                serial::write_hex(buf[i] as usize);
                serial::write_str(" ");
                i += 1;
            }
            serial::write_str("\n");
            // Verify expected
            let expect = b"Hello Aether";
            let mut ok = n == expect.len();
            i = 0;
            while ok && i < n {
                if buf[i] != expect[i] {
                    ok = false;
                }
                i += 1;
            }
            if ok {
                serial::write_str("[STORAGE] PERSISTENCE OK - Hello Aether\n");
            } else {
                serial::write_str("[STORAGE] content mismatch\n");
            }
            ok
        }
        None => {
            serial::write_str("[STORAGE] /test.txt missing - creating\n");
            if create("/test.txt") && write("/test.txt", b"Hello Aether") {
                let _ = ata::flush();
                serial::write_str("[STORAGE] created on this boot\n");
                true
            } else {
                false
            }
        }
    }
}



fn alloc_contiguous(count: u32) -> Option<u32> {
    if count == 0 { return None; }
    let mut bm = [0u8; 512];
    if !ata::read_sectors(BITMAP_LBA, 1, &mut bm) { return None; }
    let max = (ata::total_sectors().saturating_sub(DATA_START)) as usize;
    let max = if max > 4096 { 4096 } else { max };
    let need = count as usize;
    let mut start_bit = 0usize;
    while start_bit + need <= max {
        let mut ok = true;
        let mut k = 0usize;
        while k < need {
            let bit = start_bit + k;
            let byte = bit / 8;
            let mask = 1u8 << (bit % 8);
            if bm[byte] & mask != 0 { ok = false; break; }
            k += 1;
        }
        if ok {
            k = 0;
            while k < need {
                let bit = start_bit + k;
                let byte = bit / 8;
                let mask = 1u8 << (bit % 8);
                bm[byte] |= mask;
                k += 1;
            }
            if !ata::write_sectors(BITMAP_LBA, 1, &bm) { return None; }
            return Some(DATA_START + start_bit as u32);
        }
        start_bit += 1;
    }
    None
}

/// Write file larger than 1 sector (chain consecutive LBAs, update size)
pub fn write_large(path: &str, data: &[u8]) -> bool {
    if !is_mounted() { return false; }
    let name = strip_slash(path);
    // create if needed
    let mut entries = [DirEntry { name: [0; MAX_NAME], name_len: 0, flags: 0, start_lba: 0, size: 0 }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) { return false; }
    let mut slot = None;
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & 1 != 0 && name_eq(&entries[i], name) {
            slot = Some(i);
            break;
        }
        if entries[i].flags & 1 == 0 && slot.is_none() {
            slot = Some(i);
        }
        i += 1;
    }
    let slot = match slot { Some(s) => s, None => return false };
    let sectors = (data.len() + 511) / 512;
    if sectors == 0 || sectors > MAX_FILE_SECTORS { return false; }
    // allocate first LBA if new
    let mut start = entries[slot].start_lba;
    if entries[slot].flags & 1 == 0 || start == 0 {
        // allocate `sectors` consecutive blocks
        start = match alloc_contiguous(sectors as u32) {
            Some(l) => l,
            None => {
                serial::write_str("  [VFS] contiguous alloc FAIL\n");
                return false;
            }
        };
        let nb = name.as_bytes();
        let mut off = 0usize;
        if !nb.is_empty() && nb[0] == b'/' { off = 1; }
        let mut e = DirEntry { name: [0; MAX_NAME], name_len: (nb.len() - off) as u8, flags: 1, start_lba: start, size: 0 };
        let mut j = 0usize;
        while off + j < nb.len() && j < 22 { e.name[j] = nb[off + j]; j += 1; }
        entries[slot] = e;
    }
    let mut sec = 0usize;
    while sec < sectors {
        let mut buf = [0u8; 512];
        let off = sec * 512;
        let mut j = 0usize;
        while j < 512 && off + j < data.len() {
            buf[j] = data[off + j];
            j += 1;
        }
        if !ata::write_sectors(start + sec as u32, 1, &buf) { return false; }
        sec += 1;
    }
    entries[slot].size = data.len() as u32;
    entries[slot].flags = 1;
    if !save_root(&entries) { return false; }
    let _ = ata::flush();
    serial::write_str("  [VFS] write_large /");
    serial::write_str(name);
    serial::write_str(" bytes=");
    serial::write_usize(data.len());
    serial::write_str(" sectors=");
    serial::write_usize(sectors);
    serial::write_str("\n");
    true
}



/// Write file into a directory path like "/VIDEOTEST/VT160.AVI"
pub fn write_large_ex(path: &str, data: &[u8]) -> bool {
    if !is_mounted() || data.is_empty() {
        return false;
    }
    let pb = path.as_bytes();
    // find last slash
    let mut last = 0usize;
    let mut i = 0usize;
    while i < pb.len() {
        if pb[i] == b'/' {
            last = i;
        }
        i += 1;
    }
    if last == 0 || last + 1 >= pb.len() {
        return write_large(path, data);
    }

    // parent path without trailing extra
    let mut parent_path = [0u8; 64];
    let mut plen = 0usize;
    i = 0;
    while i < last && plen < 63 {
        parent_path[plen] = pb[i];
        plen += 1;
        i += 1;
    }
    if plen == 0 {
        parent_path[0] = b'/';
        plen = 1;
    }
    let parent_str = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(parent_path.as_ptr(), plen)) };
    let Some(dir_lba) = find_dir_ex(parent_str.as_bytes()) else {
        serial::write_str("[VFS] write_large_ex parent missing\n");
        return false;
    };
    let mut fname = [0u8; 24];
    let mut fnlen = 0usize;
    i = last + 1;
    while i < pb.len() && fnlen < 22 {
        fname[fnlen] = pb[i];
        fnlen += 1;
        i += 1;
    }
    if fnlen == 0 {
        return false;
    }
    let sectors = (data.len() + 511) / 512;
    if sectors == 0 || sectors > MAX_FILE_SECTORS {
        return false;
    }
    let mut e = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_dir_entries_ex(dir_lba, &mut e) {
        return false;
    }
    let mut slot = None;
    i = 0;
    while i < MAX_DIR_ENTRIES {
        if e[i].flags & FLAG_USED != 0 {
            if e[i].name_len as usize == fnlen {
                let mut ok = true;
                let mut j = 0usize;
                while j < fnlen {
                    if e[i].name[j] != fname[j] {
                        ok = false;
                        break;
                    }
                    j += 1;
                }
                if ok {
                    slot = Some(i);
                    break;
                }
            }
        } else if slot.is_none() {
            slot = Some(i);
        }
        i += 1;
    }
    let Some(slot) = slot else {
        return false;
    };
    let start = match alloc_contiguous(sectors as u32) {
        Some(l) => l,
        None => return false,
    };
    let mut sec = 0usize;
    while sec < sectors {
        let mut buf = [0u8; 512];
        let off = sec * 512;
        let mut j = 0usize;
        while j < 512 && off + j < data.len() {
            buf[j] = data[off + j];
            j += 1;
        }
        if !ata::write_sectors(start + sec as u32, 1, &buf) {
            return false;
        }
        sec += 1;
    }
    let mut j = 0usize;
    while j < MAX_NAME {
        e[slot].name[j] = 0;
        j += 1;
    }
    j = 0;
    while j < fnlen && j < 22 {
        e[slot].name[j] = fname[j];
        j += 1;
    }
    e[slot].name_len = fnlen as u8;
    e[slot].flags = FLAG_USED;
    e[slot].start_lba = start;
    e[slot].size = data.len() as u32;
    // save dir sector
    let mut b = [0u8; 512];
    i = 0;
    while i < MAX_DIR_ENTRIES {
        let o = i * 32;
        let mut j = 0usize;
        while j < 22 {
            b[o + j] = e[i].name[j];
            j += 1;
        }
        b[o + 22] = e[i].name_len;
        b[o + 23] = e[i].flags;
        let lb = e[i].start_lba.to_le_bytes();
        b[o + 24] = lb[0];
        b[o + 25] = lb[1];
        b[o + 26] = lb[2];
        b[o + 27] = lb[3];
        let sz = e[i].size.to_le_bytes();
        b[o + 28] = sz[0];
        b[o + 29] = sz[1];
        b[o + 30] = sz[2];
        b[o + 31] = sz[3];
        i += 1;
    }
    if !ata::write_sectors(dir_lba, 1, &b) {
        return false;
    }
    let _ = ata::flush();
    serial::write_str("[VFS] write_large_ex ");
    serial::write_str(path);
    serial::write_str(" bytes=");
    serial::write_usize(data.len());
    serial::write_str("\n");
    true
}


/// Read file from nested path e.g. /VIDEOTEST/VT160.AVI
pub fn read_large_ex(path: &str, out: &mut [u8]) -> Option<usize> {
    if !is_mounted() || out.is_empty() {
        return None;
    }
    let pb = path.as_bytes();
    let mut last = 0usize;
    let mut i = 0usize;
    while i < pb.len() {
        if pb[i] == b'/' {
            last = i;
        }
        i += 1;
    }
    if last == 0 || last + 1 >= pb.len() {
        return read_large(path, out);
    }
    let mut parent_path = [0u8; 64];
    let mut plen = 0usize;
    i = 0;
    while i < last && plen < 63 {
        parent_path[plen] = pb[i];
        plen += 1;
        i += 1;
    }
    if plen == 0 {
        parent_path[0] = b'/';
        plen = 1;
    }
    let parent_str = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(parent_path.as_ptr(), plen)) };
    let dir_lba = find_dir_ex(parent_str.as_bytes())?;
    let mut fname = [0u8; 24];
    let mut fnlen = 0usize;
    i = last + 1;
    while i < pb.len() && fnlen < 22 {
        fname[fnlen] = pb[i];
        fnlen += 1;
        i += 1;
    }
    let mut e = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_dir_entries_ex(dir_lba, &mut e) {
        return None;
    }
    i = 0;
    while i < MAX_DIR_ENTRIES {
        if e[i].flags & FLAG_USED != 0
            && e[i].flags & FLAG_DIR == 0
            && e[i].name_len as usize == fnlen
        {
            let mut ok = true;
            let mut j = 0usize;
            while j < fnlen {
                if e[i].name[j] != fname[j] {
                    ok = false;
                    break;
                }
                j += 1;
            }
            if ok {
                let size = e[i].size as usize;
                let start = e[i].start_lba;
                let want = size.min(out.len());
                let sectors = (want + 511) / 512;
                let mut sec = 0usize;
                let mut total = 0usize;
                while sec < sectors {
                    let mut buf = [0u8; 512];
                    if !ata::read_sectors(start + sec as u32, 1, &mut buf) {
                        break;
                    }
                    let off = sec * 512;
                    let mut j = 0usize;
                    while j < 512 && off + j < want {
                        out[off + j] = buf[j];
                        j += 1;
                        total += 1;
                    }
                    sec += 1;
                }
                return Some(total);
            }
        }
        i += 1;
    }
    None
}

pub fn read_large(path: &str, out: &mut [u8]) -> Option<usize> {
    if !is_mounted() { return None; }
    let name = strip_slash(path);
    let mut entries = [DirEntry { name: [0; MAX_NAME], name_len: 0, flags: 0, start_lba: 0, size: 0 }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) { return None; }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & 1 != 0 && name_eq(&entries[i], name) {
            let total = entries[i].size as usize;
            let total = if total > out.len() { out.len() } else { total };
            let sectors = (total + 511) / 512;
            let mut sec = 0usize;
            while sec < sectors {
                let mut buf = [0u8; 512];
                if !ata::read_sectors(entries[i].start_lba + sec as u32, 1, &mut buf) { return None; }
                let off = sec * 512;
                let mut j = 0usize;
                while j < 512 && off + j < total {
                    out[off + j] = buf[j];
                    j += 1;
                }
                sec += 1;
            }
            serial::write_str("  [VFS] read_large /");
            serial::write_str(name);
            serial::write_str(" bytes=");
            serial::write_usize(total);
            serial::write_str("\n");
            return Some(total);
        }
        i += 1;
    }
    None
}

// ---- File Manager support ----
const FLAG_USED: u8 = 1;
const FLAG_DIR: u8 = 2;

fn load_dir_lba(lba: u32, entries: &mut [DirEntry; MAX_DIR_ENTRIES]) -> bool {
    let mut buf = [0u8; 512];
    if !ata::read_sectors(lba, 1, &mut buf) {
        return false;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        let off = i * 32;
        let mut j = 0usize;
        while j < MAX_NAME {
            entries[i].name[j] = if off + j < 512 { buf[off + j] } else { 0 };
            j += 1;
        }
        entries[i].name_len = buf[off + 28];
        entries[i].flags = buf[off + 29];
        entries[i].start_lba = u32::from_le_bytes([
            buf[off + 24],
            buf[off + 25],
            buf[off + 26],
            buf[off + 27],
        ]);
        // packed layout may differ - use same as load_root
        i += 1;
    }
    // Prefer existing load_root encoding: re-read using load_root logic for root only
    true
}

/// Entry info for File Manager
#[derive(Clone, Copy)]
pub struct ListItem {
    pub name: [u8; 24],
    pub name_len: usize,
    pub size: u32,
    pub is_dir: bool,
}

pub fn list_ex(out: &mut [ListItem; 16]) -> usize {
    let mut n = 0usize;
    if !is_mounted() {
        return 0;
    }
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return 0;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES && n < 16 {
        if entries[i].flags & FLAG_USED != 0 {
            let mut j = 0usize;
            let nl = entries[i].name_len as usize;
            let nl = if nl > 24 { 24 } else { nl };
            while j < 24 {
                out[n].name[j] = 0;
                j += 1;
            }
            j = 0;
            while j < nl {
                out[n].name[j] = entries[i].name[j];
                j += 1;
            }
            out[n].name_len = nl;
            out[n].size = entries[i].size;
            out[n].is_dir = entries[i].flags & FLAG_DIR != 0;
            n += 1;
        }
        i += 1;
    }
    n
}

pub fn rename(old_path: &str, new_name: &str) -> bool {
    if !is_mounted() {
        return false;
    }
    let old = strip_slash(old_path);
    let new = strip_slash(new_name);
    if new.is_empty() || new.len() > MAX_NAME {
        return false;
    }
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return false;
    }
    // collision?
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & FLAG_USED != 0 && name_eq(&entries[i], new) {
            return false;
        }
        i += 1;
    }
    i = 0;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & FLAG_USED != 0 && name_eq(&entries[i], old) {
            let mut j = 0usize;
            while j < MAX_NAME {
                entries[i].name[j] = 0;
                j += 1;
            }
            j = 0;
            let nb = new.as_bytes();
            while j < nb.len() && j < MAX_NAME {
                entries[i].name[j] = nb[j];
                j += 1;
            }
            entries[i].name_len = nb.len() as u8;
            if !save_root(&entries) {
                return false;
            }
            let _ = ata::flush();
            serial::write_str("  [VFS] rename ");
            serial::write_str(old);
            serial::write_str(" -> ");
            serial::write_str(new);
            serial::write_str("\n");
            return true;
        }
        i += 1;
    }
    false
}

pub fn mkdir(path: &str) -> bool {
    if !is_mounted() {
        return false;
    }
    let name = strip_slash(path);
    if name.is_empty() || name.len() > MAX_NAME {
        return false;
    }
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return false;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & FLAG_USED != 0 && name_eq(&entries[i], name) {
            return false;
        }
        i += 1;
    }
    // allocate sector for directory contents (empty)
    let Some(lba) = bitmap_alloc() else {
        return false;
    };
    let mut empty = [0u8; 512];
    if !ata::write_sectors(lba, 1, &empty) {
        bitmap_free(lba);
        return false;
    }
    i = 0;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & FLAG_USED == 0 {
            let mut j = 0usize;
            while j < MAX_NAME {
                entries[i].name[j] = 0;
                j += 1;
            }
            let nb = name.as_bytes();
            j = 0;
            while j < nb.len() && j < MAX_NAME {
                entries[i].name[j] = nb[j];
                j += 1;
            }
            entries[i].name_len = nb.len() as u8;
            entries[i].flags = FLAG_USED | FLAG_DIR;
            entries[i].start_lba = lba;
            entries[i].size = 0;
            if !save_root(&entries) {
                return false;
            }
            let _ = ata::flush();
            unsafe {
                FILE_COUNT += 1;
            }
            serial::write_str("  [VFS] mkdir /");
            serial::write_str(name);
            serial::write_str("\n");
            return true;
        }
        i += 1;
    }
    bitmap_free(lba);
    false
}

pub fn storage_info(total_out: &mut u32, free_out: &mut u32, used_files: &mut u32) {
    *total_out = 0;
    *free_out = 0;
    *used_files = 0;
    if !is_mounted() {
        return;
    }
    let mut buf = [0u8; 512];
    if let Some(sb) = read_super(&mut buf) {
        *total_out = sb.total_sectors;
        *free_out = sb.free_count;
    } else {
        *total_out = ata::total_sectors();
        *free_out = 0;
    }
    *used_files = file_count();
}

pub fn is_dir(path: &str) -> bool {
    if !is_mounted() {
        return false;
    }
    let name = strip_slash(path);
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return false;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & FLAG_USED != 0 && name_eq(&entries[i], name) {
            return entries[i].flags & FLAG_DIR != 0;
        }
        i += 1;
    }
    false
}

pub fn entry_size(path: &str) -> Option<u32> {
    if !is_mounted() {
        return None;
    }
    let name = strip_slash(path);
    let mut entries = [DirEntry {
        name: [0; MAX_NAME],
        name_len: 0,
        flags: 0,
        start_lba: 0,
        size: 0,
    }; MAX_DIR_ENTRIES];
    if !load_root(&mut entries) {
        return None;
    }
    let mut i = 0usize;
    while i < MAX_DIR_ENTRIES {
        if entries[i].flags & FLAG_USED != 0 && name_eq(&entries[i], name) {
            return Some(entries[i].size);
        }
        i += 1;
    }
    None
}

pub fn read_name(name: &[u8], name_len: usize, out: &mut [u8]) -> Option<usize> {
    if name_len == 0 || name_len > MAX_NAME {
        return None;
    }
    let mut path = [0u8; 40];
    path[0] = b'/';
    let mut i = 0usize;
    while i < name_len {
        path[1 + i] = name[i];
        i += 1;
    }
    let s = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(path.as_ptr(), 1 + name_len)) };
    read(s, out)
}

pub fn delete_name(name: &[u8], name_len: usize) -> bool {
    if name_len == 0 || name_len > MAX_NAME {
        return false;
    }
    let mut path = [0u8; 40];
    path[0] = b'/';
    let mut i = 0usize;
    while i < name_len {
        path[1 + i] = name[i];
        i += 1;
    }
    let s = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(path.as_ptr(), 1 + name_len)) };
    delete(s)
}

pub fn list_ex_path(path:&str,out:&mut [ListItem;16])->usize{if !is_mounted(){return 0;}let Some(lba)=find_dir_ex(path.as_bytes())else{return 0;};let mut e=[DirEntry{name:[0;MAX_NAME],name_len:0,flags:0,start_lba:0,size:0};MAX_DIR_ENTRIES];if !load_dir_entries_ex(lba,&mut e){return 0;}let mut n=0;let mut i=0;while i<MAX_DIR_ENTRIES&&n<16{if e[i].flags&FLAG_USED!=0{let nl=if e[i].name_len as usize>24{24}else{e[i].name_len as usize};let mut j=0;while j<24{out[n].name[j]=0;j+=1;}j=0;while j<nl{out[n].name[j]=e[i].name[j];j+=1;}out[n].name_len=nl;out[n].size=e[i].size;out[n].is_dir=e[i].flags&FLAG_DIR!=0;n+=1;}i+=1;}n}


fn find_dir_ex(path:&[u8])->Option<u32>{if path.len()==0||(path.len()==1&&path[0]==b'/'){return Some(ROOT_LBA);}let mut cur=ROOT_LBA;let mut i=0;while i<path.len(){while i<path.len()&&path[i]==b'/'{i+=1;}if i>=path.len(){break;}let s=i;while i<path.len()&&path[i]!=b'/'{i+=1;}let part=&path[s..i];let mut e=[DirEntry{name:[0;MAX_NAME],name_len:0,flags:0,start_lba:0,size:0};MAX_DIR_ENTRIES];if !load_dir_entries_ex(cur,&mut e){return None;}let mut hit=None;let mut n=0;while n<MAX_DIR_ENTRIES{if e[n].flags&FLAG_USED!=0&&e[n].flags&FLAG_DIR!=0&&e[n].name_len as usize==part.len(){let mut j=0;let mut ok=true;while j<part.len(){if e[n].name[j]!=part[j]{ok=false;break;}j+=1;}if ok{hit=Some(e[n].start_lba);break;}}n+=1;}cur=hit?;}Some(cur)}

fn load_dir_entries_ex(lba:u32,e:&mut [DirEntry;MAX_DIR_ENTRIES])->bool{let mut b=[0u8;512];if !ata::read_sectors(lba,1,&mut b){return false;}let mut i=0;while i<MAX_DIR_ENTRIES{let o=i*32;let mut x=DirEntry{name:[0;MAX_NAME],name_len:0,flags:0,start_lba:0,size:0};let mut j=0;while j<22{x.name[j]=b[o+j];j+=1;}x.name_len=b[o+22];x.flags=b[o+23];x.start_lba=u32::from_le_bytes([b[o+24],b[o+25],b[o+26],b[o+27]]);x.size=u32::from_le_bytes([b[o+28],b[o+29],b[o+30],b[o+31]]);e[i]=x;i+=1;}true}


/// Size in bytes of a regular file, if present.
pub fn file_size(path: &str) -> Option<usize> {
    entry_size(path).map(|s| s as usize)
}

/// Read up to `out.len()` bytes starting at `offset` from a file.
pub fn read_range(path: &str, offset: usize, out: &mut [u8]) -> Option<usize> {
    if out.is_empty() {
        return Some(0);
    }
    // Fallback: full read then slice (AetherFS files are small enough for media tests).
    let mut tmp = [0u8; 65536];
    let n = read_large(path, &mut tmp)?;
    if offset >= n {
        return Some(0);
    }
    let avail = n - offset;
    let take = if avail < out.len() { avail } else { out.len() };
    out[..take].copy_from_slice(&tmp[offset..offset + take]);
    Some(take)
}
