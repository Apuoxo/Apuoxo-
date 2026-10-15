//! Aether Video Player - disk MP4/AVI + software render.
//!
//! Path: Explorer → open_from_ntfs → demux → (MJPEG decode | H.264 metadata)
//! Real H.264 sample decode is Stage-2; container + disk identity work now.

use crate::graphics;
use crate::serial;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    Playing,
    Paused,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Codec {
    None,
    Mjpeg,
    H264,
    Unknown,
}

const MAX_SAMPLES: usize = 1024;
const MAX_PATH: usize = 96;
const DEC_W: usize = 320;
const DEC_H: usize = 180;
const FRAME_RGB_MAX: usize = DEC_W * DEC_H;
/// Scratch for one compressed sample (JPEG or NAL blob).
const SAMPLE_BUF: usize = 512 * 1024;

static mut STATE: State = State::Stopped;
static mut CODEC: Codec = Codec::None;
static mut FRAME: u32 = 0;
static mut TICK: u32 = 0;
static mut FRAME_ACC: u32 = 0;
static mut CLIP_SW: usize = 1; // software DEMO fallback index

static mut PATH: [u8; MAX_PATH] = [0; MAX_PATH];
static mut PATH_LEN: usize = 0;
static mut TITLE: [u8; MAX_PATH] = [0; MAX_PATH];
static mut TITLE_LEN: usize = 0;

static mut SOURCE_NTFS: bool = false;
static mut NTFS_MFT: u32 = 0;
static mut FILE_SIZE: usize = 0;

static mut SAMPLE_OFF: [u32; MAX_SAMPLES] = [0; MAX_SAMPLES];
static mut SAMPLE_SZ: [u32; MAX_SAMPLES] = [0; MAX_SAMPLES];
static mut SAMPLE_N: usize = 0;
static mut SAMPLE_I: usize = 0;

static mut VID_W: u32 = 0;
static mut VID_H: u32 = 0;
static mut FPS_NUM: u32 = 15;
static mut DURATION_MS: u32 = 0;

static mut FRAME_RGB: [u32; FRAME_RGB_MAX] = [0; FRAME_RGB_MAX];
static mut FRAME_OK: bool = false;
static mut DEC_OUT_W: usize = 0;
static mut DEC_OUT_H: usize = 0;
static mut FRAME_SKIP: u32 = 1;
static mut TICK_DIV: u32 = 6;
static mut LAST_ERR: u8 = 0;
static mut FROM_DISK: bool = false;

static mut SBUF: [u8; SAMPLE_BUF] = [0; SAMPLE_BUF];

pub const TEST_W: usize = 320;
pub const TEST_H: usize = 180;
pub const TEST_FPS: u32 = 15;

pub fn init() {
    unsafe {
        STATE = State::Stopped;
        CODEC = Codec::None;
        FRAME = 0;
        SAMPLE_N = 0;
        FROM_DISK = false;
        FRAME_OK = false;
    }
    serial::write_str("[VID] player disk+SW ready\n");
}

pub fn state() -> State {
    unsafe { STATE }
}
pub fn frame() -> u32 {
    unsafe { FRAME }
}
pub fn is_playing() -> bool {
    unsafe { STATE == State::Playing }
}
pub fn from_disk() -> bool {
    unsafe { FROM_DISK }
}

pub fn is_video_path(path: &str) -> bool {
    let b = path.as_bytes();
    if b.len() < 5 {
        return false;
    }
    let n = b.len();
    // .mp4 .MP4 .m4v .avi .mov .mjpg .mjpeg
    let dot = {
        let mut i = n;
        while i > 0 {
            i -= 1;
            if b[i] == b'.' {
                break;
            }
        }
        i
    };
    if dot >= n {
        return false;
    }
    let ext = &b[dot + 1..];
    eq_ext(ext, b"MP4")
        || eq_ext(ext, b"M4V")
        || eq_ext(ext, b"MOV")
        || eq_ext(ext, b"AVI")
        || eq_ext(ext, b"MJPG")
        || eq_ext(ext, b"MJPEG")
}

fn eq_ext(a: &[u8], up: &[u8]) -> bool {
    if a.len() != up.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        let c = if a[i] >= b'a' && a[i] <= b'z' {
            a[i] - 32
        } else {
            a[i]
        };
        if c != up[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn copy_title(name: &[u8]) {
    unsafe {
        let n = name.len().min(MAX_PATH);
        let mut i = 0;
        while i < n {
            TITLE[i] = name[i];
            PATH[i] = name[i];
            i += 1;
        }
        TITLE_LEN = n;
        PATH_LEN = n;
    }
}

pub fn clip_name() -> &'static str {
    unsafe {
        if FROM_DISK && TITLE_LEN > 0 {
            core::str::from_utf8_unchecked(&TITLE[..TITLE_LEN])
        } else if CLIP_SW == 0 {
            "TEST.MP4"
        } else {
            "DEMO.MP4"
        }
    }
}

pub fn clip_desc() -> &'static str {
    unsafe {
        if !FROM_DISK {
            return if CLIP_SW == 0 {
                "Simple bars + bounce"
            } else {
                "Multi-scene complex demo"
            };
        }
        match CODEC {
            Codec::Mjpeg => "Disk MJPEG",
            Codec::H264 => "Disk H.264 (demux)",
            Codec::Unknown => "Disk video",
            Codec::None => "Disk",
        }
    }
}

pub fn open_test() {
    // Prefer real MJPEG decode self-test; fallback to SW demo
    if open_embedded_mjpeg() {
        play();
        return;
    }
    unsafe {
        FROM_DISK = false;
        SOURCE_NTFS = false;
        CODEC = Codec::None;
        SAMPLE_N = 0;
        CLIP_SW = 1;
        FRAME = 0;
        FRAME_OK = false;
        LAST_ERR = 0;
    }
    play();
}

pub fn next_clip() {
    unsafe {
        if FROM_DISK {
            // cycle info only
            return;
        }
        CLIP_SW = if CLIP_SW == 0 { 1 } else { 0 };
        FRAME = 0;
        FRAME_ACC = 1;
    }
    play();
}

/// Open video file from NTFS by MFT reference (same path as audio open).

/// Built-in MJPEG AVI - verifies real JPEG decode path without disk.
pub fn open_embedded_mjpeg() -> bool {
    let data = crate::video_embed::EMBED_AVI;
    unsafe {
        FROM_DISK = true; // treat as "real file" path for UI
        SOURCE_NTFS = false;
        NTFS_MFT = 0;
        FILE_SIZE = data.len();
        SAMPLE_N = 0;
        SAMPLE_I = 0;
        FRAME = 0;
        FRAME_OK = false;
        CODEC = Codec::None;
        LAST_ERR = 0;
    }
    copy_title(b"TEST_MJPG.AVI");
    if demux_avi_memory(data) {
        unsafe { CODEC = Codec::Mjpeg; }
        let _ = decode_sample(0);
        serial::write_str("[VID] embedded MJPEG open samples=");
        serial::write_usize(unsafe { SAMPLE_N });
        serial::write_str("\n");
        return true;
    }
    false
}

fn is_avi_video_chunk(id: [u8; 4]) -> bool {
    id[0] >= b'0'
        && id[0] <= b'9'
        && id[1] >= b'0'
        && id[1] <= b'9'
        && id[2] == b'd'
        && (id[3] == b'c' || id[3] == b'b')
}

fn demux_avi_memory(data: &[u8]) -> bool {
    if data.len() < 12 || &data[0..4] != b"RIFF" {
        return false;
    }
    if &data[8..12] != b"AVI " && &data[8..12] != b"AVIX" {
        return false;
    }
    let mut n = 0usize;
    let mut width = 160u32;
    let mut height = 120u32;
    let mut fps = 10u32;
    let mut is_mjpeg = false;

    // iterative stack walk
    let mut stack_s = [0usize; 12];
    let mut stack_e = [0usize; 12];
    let mut sp = 1usize;
    stack_s[0] = 12;
    stack_e[0] = data.len();
    let mut guard = 0usize;
    while sp > 0 && n < MAX_SAMPLES && guard < 30000 {
        guard += 1;
        sp -= 1;
        let mut off = stack_s[sp];
        let end = stack_e[sp];
        let mut local = 0usize;
        while off + 8 <= end && n < MAX_SAMPLES && local < 10000 {
            local += 1;
            let id = [data[off], data[off + 1], data[off + 2], data[off + 3]];
            let mut sz = (data[off + 4] as u32)
                | ((data[off + 5] as u32) << 8)
                | ((data[off + 6] as u32) << 16)
                | ((data[off + 7] as u32) << 24);
            let mut sz = sz as usize;
            if off + 8 + sz > data.len() {
                sz = end.saturating_sub(off + 8);
            }
            let d0 = off + 8;
            let d1 = (d0 + sz).min(data.len());

            if id == *b"LIST" && sz >= 4 {
                let next = off + 8 + sz + (sz & 1);
                if next < end && sp < 11 {
                    stack_s[sp] = next;
                    stack_e[sp] = end;
                    sp += 1;
                }
                if sp < 12 {
                    stack_s[sp] = d0 + 4;
                    stack_e[sp] = d0 + sz;
                    sp += 1;
                }
                break;
            } else if id == *b"strh" && sz >= 36 && d1 >= d0 + 36 {
                if &data[d0..d0 + 4] == b"vids" {
                    let h = &data[d0 + 4..d0 + 8];
                    if h == b"MJPG" || h == b"mjpg" || h == b"JPEG" || h == b"jpeg" {
                        is_mjpeg = true;
                    }
                    let scale = u32::from_le_bytes([
                        data[d0 + 20], data[d0 + 21], data[d0 + 22], data[d0 + 23],
                    ]);
                    let rate = u32::from_le_bytes([
                        data[d0 + 24], data[d0 + 25], data[d0 + 26], data[d0 + 27],
                    ]);
                    if scale > 0 && rate > 0 {
                        fps = (rate / scale).max(1).min(60);
                    }
                }
            } else if id == *b"strf" && sz >= 20 && d1 >= d0 + 20 {
                let w = i32::from_le_bytes([data[d0 + 4], data[d0 + 5], data[d0 + 6], data[d0 + 7]]);
                let h = i32::from_le_bytes([data[d0 + 8], data[d0 + 9], data[d0 + 10], data[d0 + 11]]);
                if w > 0 { width = w as u32; }
                if h != 0 { height = h.unsigned_abs(); }
                if &data[d0 + 16..d0 + 20] == b"MJPG" || &data[d0 + 16..d0 + 20] == b"JPEG" {
                    is_mjpeg = true;
                }
            } else if is_avi_video_chunk(id) && sz >= 2 {
                let soi = d1 > d0 + 1 && data[d0] == 0xFF && data[d0 + 1] == 0xD8;
                if soi || sz > 128 {
                    unsafe {
                        SAMPLE_OFF[n] = d0 as u32;
                        SAMPLE_SZ[n] = sz as u32;
                    }
                    n += 1;
                    if soi { is_mjpeg = true; }
                }
            }
            let step = 8 + sz + (sz & 1);
            if step < 8 { break; }
            off = off.saturating_add(step);
        }
    }

    // SOI fallback
    if n == 0 {
        let mut i = 0usize;
        while i + 3 < data.len() && n < MAX_SAMPLES {
            if data[i] == 0xFF && data[i + 1] == 0xD8 {
                let mut j = i + 2;
                while j + 1 < data.len() {
                    if data[j] == 0xFF && data[j + 1] == 0xD9 {
                        j += 2;
                        break;
                    }
                    if j > i + 2 && data[j] == 0xFF && data[j + 1] == 0xD8 {
                        break;
                    }
                    j += 1;
                }
                let sz = j - i;
                if sz > 100 {
                    unsafe {
                        SAMPLE_OFF[n] = i as u32;
                        SAMPLE_SZ[n] = sz as u32;
                    }
                    n += 1;
                    is_mjpeg = true;
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
    }

    if n == 0 {
        return false;
    }
    let _ = is_mjpeg;
    unsafe {
        SAMPLE_N = n;
        CODEC = Codec::Mjpeg;
        VID_W = width;
        VID_H = height;
        FPS_NUM = fps;
        FROM_DISK = true;
        SOURCE_NTFS = false;
        FILE_SIZE = data.len();
        NTFS_MFT = 0xFFFF_FFFE;
    }
    true
}


static mut AFS_FILE: [u8; 65536] = [0; 65536];
static mut AFS_LEN: usize = 0;

/// Open MJPEG/AVI from AetherFS RAM disk path (e.g. /VIDEOTEST/VT160.AVI).
pub fn open_from_aetherfs(path: &str) -> bool {
    let mut tmp = [0u8; 65536];
    let n = match crate::fs::read_large_ex(path, &mut tmp) {
        Some(x) if x > 32 => x,
        _ => {
            // try read_range full via file_size
            let sz = match crate::fs::file_size(path) {
                Some(s) if s > 32 && s <= 65536 => s,
                _ => return false,
            };
            match crate::fs::read_range(path, 0, &mut tmp[..sz]) {
                Some(x) if x > 32 => x,
                _ => return false,
            }
        }
    };
    unsafe {
        let mut i = 0usize;
        while i < n {
            AFS_FILE[i] = tmp[i];
            i += 1;
        }
        AFS_LEN = n;
        FROM_DISK = true;
        SOURCE_NTFS = false;
        NTFS_MFT = 0xFFFF_FFFD;
        FILE_SIZE = n;
        SAMPLE_N = 0;
        SAMPLE_I = 0;
        FRAME = 0;
        FRAME_OK = false;
        CODEC = Codec::None;
        LAST_ERR = 0;
    }
    // title from path
    let b = path.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'/' {
            start = i + 1;
        }
        i += 1;
    }
    copy_title(&b[start..]);
    // demux from AFS_FILE
    let ok = unsafe { demux_avi_memory_buf(&AFS_FILE[..AFS_LEN]) };
    if !ok {
        // try SOI scan in buffer
        let ok2 = unsafe { demux_soi_memory(&AFS_FILE[..AFS_LEN]) };
        if !ok2 {
            unsafe {
                CODEC = Codec::Unknown;
                LAST_ERR = 2;
            }
            return true; // still open UI with name
        }
    }
    unsafe {
        NTFS_MFT = 0xFFFF_FFFD;
        CODEC = Codec::Mjpeg;
    }
    tune_perf_from_file();
    let _ = decode_sample(0);
    serial::write_str("[VID] AetherFS open ");
    serial::write_str(path);
    serial::write_str(" samples=");
    serial::write_usize(unsafe { SAMPLE_N });
    serial::write_str("\n");
    true
}

fn demux_avi_memory_buf(data: &[u8]) -> bool {
    // reuse demux_avi_memory logic but keep NTFS_MFT caller-set
    let saved = unsafe { NTFS_MFT };
    let ok = demux_avi_memory(data);
    unsafe {
        NTFS_MFT = saved;
        if ok {
            SOURCE_NTFS = false;
            FILE_SIZE = data.len();
        }
    }
    ok
}

fn demux_soi_memory(data: &[u8]) -> bool {
    let mut n = 0usize;
    let mut i = 0usize;
    while i + 3 < data.len() && n < MAX_SAMPLES {
        if data[i] == 0xFF && data[i + 1] == 0xD8 {
            let mut j = i + 2;
            while j + 1 < data.len() {
                if data[j] == 0xFF && data[j + 1] == 0xD9 {
                    j += 2;
                    break;
                }
                if j > i + 2 && data[j] == 0xFF && data[j + 1] == 0xD8 {
                    break;
                }
                j += 1;
            }
            let sz = j - i;
            if sz > 100 {
                unsafe {
                    SAMPLE_OFF[n] = i as u32;
                    SAMPLE_SZ[n] = sz as u32;
                }
                n += 1;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    if n == 0 {
        return false;
    }
    unsafe {
        SAMPLE_N = n;
        CODEC = Codec::Mjpeg;
        VID_W = 320;
        VID_H = 240;
        FPS_NUM = 6;
        FROM_DISK = true;
    }
    true
}

pub fn open_from_ntfs(mft_ref: u32, name: &[u8]) -> bool {
    if mft_ref == 0 || name.is_empty() {
        return false;
    }
    let nlen = name.len().min(90);
    let mut pb = [0u8; 96];
    let mut i = 0;
    while i < nlen {
        pb[i] = name[i];
        i += 1;
    }
    let path = unsafe { core::str::from_utf8_unchecked(&pb[..nlen]) };
    if !is_video_path(path) {
        return false;
    }
    let size = match crate::fs_ntfs::file_data_size(mft_ref) {
        Some(s) if s > 32 => s,
        _ => {
            unsafe { LAST_ERR = 1 };
            return false;
        }
    };
    copy_title(name);
    unsafe {
        SOURCE_NTFS = true;
        NTFS_MFT = mft_ref;
        FILE_SIZE = size;
        FROM_DISK = true;
        SAMPLE_N = 0;
        SAMPLE_I = 0;
        FRAME = 0;
        FRAME_OK = false;
        CODEC = Codec::None;
        VID_W = 0;
        VID_H = 0;
        STATE = State::Stopped;
        LAST_ERR = 0;
    }

    // Read header for demux (first 512KB is enough for most moov-first or we scan)
    let hdr_len = size.min(512 * 1024);
    // Use heap-less: parse by reading ranges on demand
    let lower_avi = {
        let b = path.as_bytes();
        let n = b.len();
        n >= 4
            && b[n - 4] == b'.'
            && (b[n - 3] | 0x20) == b'a'
            && (b[n - 2] | 0x20) == b'v'
            && (b[n - 1] | 0x20) == b'i'
    };
    let mut ok = false;
    if lower_avi {
        ok = demux_avi_mjpeg(mft_ref, size);
        if !ok {
            ok = demux_mp4(mft_ref, size);
        }
    } else {
        ok = demux_mp4(mft_ref, size);
        if !ok {
            ok = demux_avi_mjpeg(mft_ref, size);
        }
    }
    if !ok {
        // Last resort: scan file for JPEG SOI/EOI pairs (raw MJPEG / odd AVI)
        if scan_jpeg_soi_file(mft_ref, size) {
            serial::write_str("[VID] SOI-scan OK file=");
            serial::write_str(path);
            serial::write_str("\n");
            ok = true;
        } else {
            unsafe {
                LAST_ERR = 2;
                STATE = State::Error;
                CODEC = Codec::Unknown;
            }
            serial::write_str("[VID] demux fail file=");
            serial::write_str(path);
            serial::write_str("\n");
            unsafe {
                FROM_DISK = true;
                CODEC = Codec::Unknown;
            }
            return true;
        }
    }

    serial::write_str("[VID] open disk ");
    serial::write_str(path);
    serial::write_str(" samples=");
    serial::write_usize(unsafe { SAMPLE_N });
    serial::write_str(" codec=");
    serial::write_str(match unsafe { CODEC } {
        Codec::Mjpeg => "MJPEG",
        Codec::H264 => "H264",
        _ => "?",
    });
    serial::write_str("\n");

    tune_perf_from_file();
    // Decode first frame if MJPEG
    if unsafe { CODEC == Codec::Mjpeg && SAMPLE_N > 0 } {
        let _ = decode_sample(0);
    } else if unsafe { CODEC == Codec::H264 } {
        // Prove we read THIS file: fingerprint first sample into SW seed
        let _ = load_sample_to_sbuf(0);
        unsafe {
            FRAME_OK = false;
        }
    }
    true
}

fn read_at(mft: u32, off: usize, buf: &mut [u8]) -> Option<usize> {
    crate::fs_ntfs::read_file_range(mft, off, buf)
}

fn be32(b: &[u8]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as u32)
}
fn le32(b: &[u8]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16) | ((b[3] as u32) << 24)
}
fn fourcc(b: &[u8]) -> [u8; 4] {
    [b[0], b[1], b[2], b[3]]
}

fn demux_mp4(mft: u32, size: usize) -> bool {
    // Scan top-level boxes; follow moov. Support moov at start or after mdat (scan).
    let mut off = 0usize;
    let mut found_moov = false;
    let mut scan_guard = 0usize;
    while off + 8 <= size && scan_guard < 64 {
        scan_guard += 1;
        let mut hdr = [0u8; 16];
        let n = match read_at(mft, off, &mut hdr) {
            Some(x) if x >= 8 => x,
            _ => break,
        };
        let mut box_size = be32(&hdr[0..4]) as usize;
        let typ = fourcc(&hdr[4..8]);
        if box_size == 1 && n >= 16 {
            // largesize
            let hi = be32(&hdr[8..12]) as u64;
            let lo = be32(&hdr[12..16]) as u64;
            box_size = ((hi << 32) | lo) as usize;
        } else if box_size == 0 {
            box_size = size - off;
        }
        if box_size < 8 || off + box_size > size {
            break;
        }
        if typ == *b"moov" {
            found_moov = parse_moov(mft, off + 8, box_size - 8);
            break;
        }
        off += box_size;
    }
    if !found_moov {
        // Linear scan for moov type in first 1MB chunks
        let mut probe = 0usize;
        let mut tmp = [0u8; 4096];
        while probe + 8 < size.min(2 * 1024 * 1024) {
            let chunk = read_at(mft, probe, &mut tmp).unwrap_or(0);
            if chunk < 8 {
                break;
            }
            let mut i = 0usize;
            while i + 8 <= chunk {
                if tmp[i + 4] == b'm'
                    && tmp[i + 5] == b'o'
                    && tmp[i + 6] == b'o'
                    && tmp[i + 7] == b'v'
                {
                    let bs = be32(&tmp[i..i + 4]) as usize;
                    if bs >= 8 {
                        found_moov = parse_moov(mft, probe + i + 8, bs - 8);
                        break;
                    }
                }
                i += 1;
            }
            if found_moov {
                break;
            }
            probe += chunk.saturating_sub(7);
        }
    }
    found_moov && unsafe { SAMPLE_N > 0 }
}

fn parse_moov(mft: u32, start: usize, len: usize) -> bool {
    let end = start + len;
    let mut off = start;
    let mut ok = false;
    while off + 8 <= end {
        let mut hdr = [0u8; 8];
        if read_at(mft, off, &mut hdr).unwrap_or(0) < 8 {
            break;
        }
        let mut bs = be32(&hdr[0..4]) as usize;
        let typ = fourcc(&hdr[4..8]);
        if bs < 8 {
            break;
        }
        if off + bs > end {
            bs = end - off;
        }
        if typ == *b"trak" {
            if parse_trak(mft, off + 8, bs - 8) {
                ok = true;
            }
        }
        off += bs;
    }
    ok
}

fn parse_trak(mft: u32, start: usize, len: usize) -> bool {
    // Need stsd (codec) + stsz/stco or co64 + optional stsc
    let end = start + len;
    let mut off = start;
    let mut is_video = false;
    let mut codec = Codec::Unknown;
    let mut width = 0u32;
    let mut height = 0u32;
    // collect sample sizes and chunk offsets
    let mut stsz_count = 0u32;
    let mut stsz_default = 0u32;
    let mut stco_count = 0u32;
    // limits
    const MAX_STSZ: usize = MAX_SAMPLES;
    const MAX_STCO: usize = 256;
    let mut stsz: [u32; MAX_STSZ] = [0; MAX_STSZ];
    let mut stco: [u32; MAX_STCO] = [0; MAX_STCO];
    let mut stsc_n = 0usize;
    let mut stsc_first: [u32; 64] = [0; 64];
    let mut stsc_spp: [u32; 64] = [0; 64]; // samples per chunk

    while off + 8 <= end {
        let mut hdr = [0u8; 8];
        if read_at(mft, off, &mut hdr).unwrap_or(0) < 8 {
            break;
        }
        let mut bs = be32(&hdr[0..4]) as usize;
        let typ = fourcc(&hdr[4..8]);
        if bs < 8 {
            break;
        }
        if off + bs > end {
            bs = end - off;
        }
        let content = off + 8;
        let clen = bs - 8;

        if typ == *b"mdia" || typ == *b"minf" || typ == *b"stbl" || typ == *b"edts" {
            // recurse one level by scanning inside
            let inner_ok = parse_trak_boxes(
                mft,
                content,
                clen,
                &mut is_video,
                &mut codec,
                &mut width,
                &mut height,
                &mut stsz_count,
                &mut stsz_default,
                &mut stsz,
                &mut stco_count,
                &mut stco,
                &mut stsc_n,
                &mut stsc_first,
                &mut stsc_spp,
            );
            if inner_ok {
                // continue scanning siblings too
            }
            off += bs;
            continue;
        }
        // leaf handling at this level rare
        off += bs;
    }
    // Second pass: explicit walk via helper already filled if nested
    // Re-walk with dedicated function
    let _ = parse_trak_boxes(
        mft,
        start,
        len,
        &mut is_video,
        &mut codec,
        &mut width,
        &mut height,
        &mut stsz_count,
        &mut stsz_default,
        &mut stsz,
        &mut stco_count,
        &mut stco,
        &mut stsc_n,
        &mut stsc_first,
        &mut stsc_spp,
    );

    if !is_video || stsz_count == 0 || stco_count == 0 {
        return false;
    }
    // Build sample table from stsc + stco + stsz
    let n = (stsz_count as usize).min(MAX_SAMPLES);
    let mut si = 0usize;
    let mut chunk_idx = 0u32;
    while si < n && (chunk_idx as usize) < stco_count as usize {
        let spp = stsc_samples_per_chunk(chunk_idx + 1, stsc_n, &stsc_first, &stsc_spp);
        let mut base = stco[chunk_idx as usize];
        let mut s = 0u32;
        while s < spp && si < n {
            let sz = if stsz_default != 0 {
                stsz_default
            } else {
                stsz[si]
            };
            unsafe {
                SAMPLE_OFF[si] = base;
                SAMPLE_SZ[si] = sz;
            }
            base = base.wrapping_add(sz);
            si += 1;
            s += 1;
        }
        chunk_idx += 1;
    }
    unsafe {
        SAMPLE_N = si;
        CODEC = codec;
        VID_W = if width > 0 { width } else { 320 };
        VID_H = if height > 0 { height } else { 240 };
        FPS_NUM = 15;
        DURATION_MS = (si as u32).saturating_mul(1000 / FPS_NUM.max(1));
    }
    si > 0
}

fn stsc_samples_per_chunk(
    chunk_1based: u32,
    stsc_n: usize,
    first: &[u32; 64],
    spp: &[u32; 64],
) -> u32 {
    if stsc_n == 0 {
        return 1;
    }
    let mut i = 0usize;
    let mut cur = spp[0];
    while i < stsc_n {
        if first[i] <= chunk_1based {
            cur = spp[i];
        } else {
            break;
        }
        i += 1;
    }
    if cur == 0 {
        1
    } else {
        cur
    }
}

fn parse_trak_boxes(
    mft: u32,
    start: usize,
    len: usize,
    is_video: &mut bool,
    codec: &mut Codec,
    width: &mut u32,
    height: &mut u32,
    stsz_count: &mut u32,
    stsz_default: &mut u32,
    stsz: &mut [u32; MAX_SAMPLES],
    stco_count: &mut u32,
    stco: &mut [u32; 256],
    stsc_n: &mut usize,
    stsc_first: &mut [u32; 64],
    stsc_spp: &mut [u32; 64],
) -> bool {
    let end = start + len;
    let mut off = start;
    while off + 8 <= end {
        let mut hdr = [0u8; 8];
        if read_at(mft, off, &mut hdr).unwrap_or(0) < 8 {
            break;
        }
        let mut bs = be32(&hdr[0..4]) as usize;
        let typ = fourcc(&hdr[4..8]);
        if bs < 8 {
            break;
        }
        if off + bs > end {
            bs = end - off;
        }
        let c0 = off + 8;
        let cl = bs - 8;

        if typ == *b"mdia"
            || typ == *b"minf"
            || typ == *b"stbl"
            || typ == *b"edts"
            || typ == *b"trak"
        {
            let _ = parse_trak_boxes(
                mft,
                c0,
                cl,
                is_video,
                codec,
                width,
                height,
                stsz_count,
                stsz_default,
                stsz,
                stco_count,
                stco,
                stsc_n,
                stsc_first,
                stsc_spp,
            );
        } else if typ == *b"hdlr" && cl >= 12 {
            let mut b = [0u8; 16];
            if read_at(mft, c0, &mut b).unwrap_or(0) >= 12 {
                // version/flags + pre_defined + handler
                let h = fourcc(&b[8..12]);
                if h == *b"vide" {
                    *is_video = true;
                }
            }
        } else if typ == *b"stsd" && cl >= 16 {
            let mut b = [0u8; 128];
            let n = read_at(mft, c0, &mut b).unwrap_or(0);
            if n >= 24 {
                // ver/flags(4) entry_count(4) then sample entry
                let entry = &b[8..];
                if entry.len() >= 24 {
                    let format = fourcc(&entry[4..8]);
                    // VisualSampleEntry: after 6+2 reserved, width height at +24 from entry start? 
                    // SampleEntry: size(4)+format(4)+reserved(6)+data_ref(2) = 16, then width(2) height(2)
                    if entry.len() >= 20 {
                        *width = ((entry[16] as u32) << 8) | (entry[17] as u32);
                        *height = ((entry[18] as u32) << 8) | (entry[19] as u32);
                    }
                    if format == *b"avc1" || format == *b"h264" || format == *b"AVC1" {
                        *codec = Codec::H264;
                        *is_video = true;
                    } else if format == *b"mp4v"
                        || format == *b"jpeg"
                        || format == *b"mjpa"
                        || format == *b"MJPG"
                    {
                        *codec = Codec::Mjpeg;
                        *is_video = true;
                    }
                }
            }
        } else if typ == *b"stsz" && cl >= 12 {
            let mut b = [0u8; 16];
            if read_at(mft, c0, &mut b).unwrap_or(0) >= 12 {
                *stsz_default = be32(&b[4..8]);
                *stsz_count = be32(&b[8..12]);
                if *stsz_default == 0 {
                    let cnt = (*stsz_count as usize).min(MAX_SAMPLES);
                    let mut i = 0usize;
                    while i < cnt {
                        let mut s = [0u8; 4];
                        if read_at(mft, c0 + 12 + i * 4, &mut s).unwrap_or(0) >= 4 {
                            stsz[i] = be32(&s);
                        }
                        i += 1;
                    }
                }
            }
        } else if typ == *b"stco" && cl >= 8 {
            let mut b = [0u8; 8];
            if read_at(mft, c0, &mut b).unwrap_or(0) >= 8 {
                *stco_count = be32(&b[4..8]).min(256);
                let mut i = 0usize;
                while i < *stco_count as usize {
                    let mut s = [0u8; 4];
                    if read_at(mft, c0 + 8 + i * 4, &mut s).unwrap_or(0) >= 4 {
                        stco[i] = be32(&s);
                    }
                    i += 1;
                }
            }
        } else if typ == *b"co64" && cl >= 8 {
            // only take low 32 bits if address fits
            let mut b = [0u8; 8];
            if read_at(mft, c0, &mut b).unwrap_or(0) >= 8 {
                *stco_count = be32(&b[4..8]).min(256);
                let mut i = 0usize;
                while i < *stco_count as usize {
                    let mut s = [0u8; 8];
                    if read_at(mft, c0 + 8 + i * 8, &mut s).unwrap_or(0) >= 8 {
                        stco[i] = be32(&s[4..8]); // low part of 64-bit
                    }
                    i += 1;
                }
            }
        } else if typ == *b"stsc" && cl >= 8 {
            let mut b = [0u8; 8];
            if read_at(mft, c0, &mut b).unwrap_or(0) >= 8 {
                let n = be32(&b[4..8]).min(64) as usize;
                *stsc_n = n;
                let mut i = 0usize;
                while i < n {
                    let mut e = [0u8; 12];
                    if read_at(mft, c0 + 8 + i * 12, &mut e).unwrap_or(0) >= 12 {
                        stsc_first[i] = be32(&e[0..4]);
                        stsc_spp[i] = be32(&e[4..8]);
                    }
                    i += 1;
                }
            }
        }
        off += bs;
    }
    *is_video
}


fn scan_jpeg_soi_file(mft: u32, size: usize) -> bool {
    let mut n = 0usize;
    let mut off = 0usize;
    let mut buf = [0u8; 8192];
    let mut in_jpg = false;
    let mut jpg_start = 0usize;
    let mut prev = 0u8;
    while off < size && n < MAX_SAMPLES {
        let chunk = (size - off).min(8192);
        let got = match read_at(mft, off, &mut buf[..chunk]) {
            Some(x) if x > 0 => x,
            _ => break,
        };
        let mut i = 0usize;
        while i < got {
            let b = buf[i];
            if prev == 0xFF && b == 0xD8 {
                in_jpg = true;
                jpg_start = off + i - 1;
            } else if in_jpg && prev == 0xFF && b == 0xD9 {
                let endp = off + i + 1;
                let sz = endp - jpg_start;
                if sz > 200 {
                    unsafe {
                        SAMPLE_OFF[n] = jpg_start as u32;
                        SAMPLE_SZ[n] = sz as u32;
                    }
                    n += 1;
                }
                in_jpg = false;
            }
            prev = b;
            i += 1;
        }
        off += got;
    }
    if n == 0 {
        return false;
    }
    unsafe {
        SAMPLE_N = n;
        CODEC = Codec::Mjpeg;
        VID_W = 320;
        VID_H = 240;
        FPS_NUM = 10;
        FROM_DISK = true;
    }
    true
}

fn demux_avi_mjpeg(mft: u32, size: usize) -> bool {
    let mut hdr = [0u8; 12];
    if read_at(mft, 0, &mut hdr).unwrap_or(0) < 12 {
        return false;
    }
    if &hdr[0..4] != b"RIFF" {
        return false;
    }
    if &hdr[8..12] != b"AVI " && &hdr[8..12] != b"AVIX" {
        return false;
    }

    let mut n = 0usize;
    let mut width = 320u32;
    let mut height = 240u32;
    let mut fps = 10u32;
    let mut is_mjpeg = false;

    // Stack-based iterative walk of chunks (enter LIST)
    // frames: store list of (parent ranges) using simple recursive-style via explicit stack
    let mut stack_start = [0usize; 12];
    let mut stack_end = [0usize; 12];
    let mut sp = 0usize;
    stack_start[0] = 12;
    stack_end[0] = size;
    sp = 1;

    let mut guard = 0usize;
    while sp > 0 && n < MAX_SAMPLES && guard < 30000 {
        guard += 1;
        sp -= 1;
        let mut off = stack_start[sp];
        let end = stack_end[sp];
        let mut local_g = 0usize;
        while off + 8 <= end && n < MAX_SAMPLES && local_g < 10000 {
            local_g += 1;
            let mut h = [0u8; 8];
            if read_at(mft, off, &mut h).unwrap_or(0) < 8 {
                break;
            }
            let id = [h[0], h[1], h[2], h[3]];
            let mut sz = le32(&h[4..8]) as usize;
            if off + 8 + sz > size {
                if off + 8 >= end {
                    break;
                }
                sz = end.saturating_sub(off + 8);
            }
            let data_off = off + 8;

            if id == *b"LIST" && sz >= 4 {
                // push remainder after this LIST, then enter LIST body
                let next = off + 8 + sz + (sz & 1);
                if next < end && sp < 11 {
                    stack_start[sp] = next;
                    stack_end[sp] = end;
                    sp += 1;
                }
                // enter list after 4-byte type
                if sp < 12 {
                    stack_start[sp] = data_off + 4;
                    stack_end[sp] = data_off + sz;
                    sp += 1;
                }
                break; // continue from stack
            } else if id == *b"strh" && sz >= 36 {
                let mut b = [0u8; 36];
                if read_at(mft, data_off, &mut b).unwrap_or(0) >= 36 {
                    if &b[0..4] == b"vids" {
                        if &b[4..8] == b"MJPG"
                            || &b[4..8] == b"mjpg"
                            || &b[4..8] == b"JPEG"
                            || &b[4..8] == b"jpeg"
                        {
                            is_mjpeg = true;
                        }
                        let scale = le32(&b[20..24]);
                        let rate = le32(&b[24..28]);
                        if scale > 0 && rate > 0 {
                            fps = (rate / scale).max(1).min(60);
                        }
                    }
                }
            } else if id == *b"strf" && sz >= 20 {
                let mut b = [0u8; 20];
                if read_at(mft, data_off, &mut b).unwrap_or(0) >= 20 {
                    let w = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
                    let hh = i32::from_le_bytes([b[8], b[9], b[10], b[11]]);
                    if w > 0 {
                        width = w as u32;
                    }
                    if hh != 0 {
                        height = hh.unsigned_abs();
                    }
                    if &b[16..20] == b"MJPG" || &b[16..20] == b"JPEG" {
                        is_mjpeg = true;
                    }
                }
            } else if is_avi_video_chunk(id) && sz >= 2 {
                let mut soi = [0u8; 2];
                let has_soi = read_at(mft, data_off, &mut soi).unwrap_or(0) >= 2
                    && soi[0] == 0xFF
                    && soi[1] == 0xD8;
                if has_soi || sz > 128 {
                    unsafe {
                        SAMPLE_OFF[n] = data_off as u32;
                        SAMPLE_SZ[n] = sz as u32;
                    }
                    n += 1;
                    if has_soi {
                        is_mjpeg = true;
                    }
                }
            }

            let step = 8 + sz + (sz & 1);
            if step < 8 {
                break;
            }
            off = off.saturating_add(step);
        }
    }

    // Fallback: scan file for JPEG SOI/EOI pairs (handles odd muxers)
    if n == 0 {
        let mut off = 0usize;
        let mut buf = [0u8; 4096];
        let mut carry = 0u8;
        let mut in_jpg = false;
        let mut jpg_start = 0usize;
        while off < size && n < MAX_SAMPLES {
            let chunk = (size - off).min(4096);
            let got = match read_at(mft, off, &mut buf[..chunk]) {
                Some(x) => x,
                None => break,
            };
            if got == 0 {
                break;
            }
            let mut i = 0usize;
            while i < got {
                let b0 = if i == 0 && off > 0 {
                    carry
                } else if i == 0 {
                    buf[0]
                } else {
                    buf[i - 1]
                };
                // simpler: only look inside buf
                if i + 1 < got && buf[i] == 0xFF && buf[i + 1] == 0xD8 {
                    in_jpg = true;
                    jpg_start = off + i;
                } else if in_jpg && i + 1 < got && buf[i] == 0xFF && buf[i + 1] == 0xD9 {
                    let endp = off + i + 2;
                    let sz = endp - jpg_start;
                    if sz > 100 {
                        unsafe {
                            SAMPLE_OFF[n] = jpg_start as u32;
                            SAMPLE_SZ[n] = sz as u32;
                        }
                        n += 1;
                        is_mjpeg = true;
                    }
                    in_jpg = false;
                }
                i += 1;
            }
            if got > 0 {
                carry = buf[got - 1];
            }
            off += got;
        }
    }

    if n == 0 {
        return false;
    }
    unsafe {
        SAMPLE_N = n;
        CODEC = Codec::Mjpeg;
        VID_W = width;
        VID_H = height;
        FPS_NUM = fps;
        FROM_DISK = true;
    }
    let _ = is_mjpeg;
    true
}

fn load_sample_to_sbuf(idx: usize) -> Option<usize> {
    unsafe {
        if idx >= SAMPLE_N {
            return None;
        }
        let off = SAMPLE_OFF[idx] as usize;
        let sz = (SAMPLE_SZ[idx] as usize).min(SAMPLE_BUF);
        if sz == 0 {
            return None;
        }
        // Embedded AVI in kernel image
        if NTFS_MFT == 0xFFFF_FFFE {
            let data = crate::video_embed::EMBED_AVI;
            if off + sz > data.len() {
                return None;
            }
            let mut i = 0usize;
            while i < sz {
                SBUF[i] = data[off + i];
                i += 1;
            }
            return Some(sz);
        }
        // AetherFS-loaded file buffer
        if NTFS_MFT == 0xFFFF_FFFD {
            if off + sz > AFS_LEN {
                return None;
            }
            let mut i = 0usize;
            while i < sz {
                SBUF[i] = AFS_FILE[off + i];
                i += 1;
            }
            return Some(sz);
        }
        let mft = NTFS_MFT;
        match read_at(mft, off, &mut SBUF[..sz]) {
            Some(n) if n > 0 => Some(n),
            _ => None,
        }
    }
}


fn tune_perf_from_file() {
    unsafe {
        let px = VID_W.saturating_mul(VID_H);
        // Huffman is fast now - skip less, keep mouse alive on high res
        if px >= 1280 * 720 {
            FRAME_SKIP = 2;
            TICK_DIV = 3;
        } else if px >= 960 * 540 {
            FRAME_SKIP = 1;
            TICK_DIV = 2;
        } else {
            FRAME_SKIP = 1;
            TICK_DIV = 2;
        }
        if SAMPLE_N > 0 && SAMPLE_SZ[0] > 200000 {
            FRAME_SKIP = FRAME_SKIP.saturating_add(1);
        }
    }
}

fn decode_sample(idx: usize) -> bool {
    let n = match load_sample_to_sbuf(idx) {
        Some(x) => x,
        None => return false,
    };
    unsafe {
        SAMPLE_I = idx;
        // Find JPEG SOI inside sample (some muxers add padding)
        let mut start = 0usize;
        while start + 2 < n {
            if SBUF[start] == 0xFF && SBUF[start + 1] == 0xD8 {
                break;
            }
            start += 1;
        }
        let slice = if start + 100 < n { &SBUF[start..n] } else { &SBUF[..n] };

        if CODEC == Codec::Mjpeg || CODEC == Codec::Unknown {
            match crate::jpeg_dec::decode_rgb(slice, &mut FRAME_RGB, DEC_W, DEC_H) {
                Some((w, h)) => {
                    VID_W = w as u32;
                    VID_H = h as u32;
                    DEC_OUT_W = w;
                    DEC_OUT_H = h;
                    FRAME_OK = true;
                    LAST_ERR = 0;
                    return true;
                }
                None => {
                    // Do NOT show TV-static noise - solid status frame
                    FRAME_OK = false;
                    LAST_ERR = 3;
                    return false;
                }
            }
        }
        if CODEC == Codec::H264 {
            FRAME_OK = false;
            LAST_ERR = 4;
            return true;
        }
        false
    }
}

fn jpeg_fallback_preview(data: &[u8]) -> bool {
    // Map sample bytes → luminance so each file/frame differs.
    let mut y = 0usize;
    while y < DEC_H {
        let mut x = 0usize;
        while x < DEC_W {
            let si = 16 + ((y * data.len().saturating_sub(32)) / DEC_H.max(1))
                + (x * 17) % data.len().saturating_sub(16).max(1);
            let v = data[si % data.len()] as u32;
            let c = (v << 16) | (v << 8) | v;
            unsafe {
                FRAME_RGB[y * DEC_W + x] = c;
            }
            x += 1;
        }
        y += 1;
    }
    // Color bars overlay top strip using first bytes signature
    let mut x = 0usize;
    while x < DEC_W {
        let v = data[x % data.len().min(64).max(1)] as u32;
        let c = ((v.wrapping_mul(3)) << 16) | ((v.wrapping_mul(5)) << 8) | v.wrapping_mul(7);
        unsafe {
            FRAME_RGB[x] = c & 0xFFFFFF;
        }
        x += 1;
    }
    true
}

pub fn play() {
    unsafe {
        STATE = State::Playing;
    }
    serial::write_str("[VID] PLAY ");
    serial::write_str(clip_name());
    serial::write_str("\n");
}

pub fn pause() {
    unsafe {
        if STATE == State::Playing {
            STATE = State::Paused;
        } else if STATE == State::Paused {
            STATE = State::Playing;
        }
    }
}

pub fn stop() {
    unsafe {
        STATE = State::Stopped;
        FRAME = 0;
        SAMPLE_I = 0;
        FRAME_ACC = 0;
    }
    serial::write_str("[VID] STOP\n");
}

pub fn tick() {
    unsafe {
        if STATE != State::Playing {
            return;
        }
        TICK = TICK.wrapping_add(1);
        // Adaptive: large frames → slower UI tick so mouse stays usable
        let div = if TICK_DIV < 2 { 2 } else { TICK_DIV };
        if TICK % div != 0 {
            return;
        }
        FRAME = FRAME.wrapping_add(1);
        FRAME_ACC = FRAME_ACC.wrapping_add(1);

        if FROM_DISK && SAMPLE_N > 0 {
            let mut idx = (SAMPLE_I + 1) % SAMPLE_N;
            // Skip frames on heavy samples (720p MJPEG) to keep system responsive
            let skip = if FRAME_SKIP < 1 { 1 } else { FRAME_SKIP as usize };
            if skip > 1 {
                idx = (SAMPLE_I + skip) % SAMPLE_N;
            }
            if CODEC == Codec::Mjpeg {
                let _ = decode_sample(idx);
            } else {
                SAMPLE_I = idx;
                FRAME_OK = false;
            }
        } else {
            let lim = if CLIP_SW == 0 { 15 * 12 } else { 15 * 20 };
            if FRAME >= lim {
                FRAME = 0;
            }
        }
    }
}

pub fn needs_redraw() -> bool {
    unsafe { STATE == State::Playing && FRAME_ACC > 0 }
}
pub fn clear_redraw() {
    unsafe {
        FRAME_ACC = 0;
    }
}

fn rgb(r: u32, g: u32, b: u32) -> u32 {
    ((r & 255) << 16) | ((g & 255) << 8) | (b & 255)
}
fn fill(ox: usize, oy: usize, fw: usize, fh: usize, x: usize, y: usize, w: usize, h: usize, c: u32) {
    if x >= fw || y >= fh || w == 0 || h == 0 {
        return;
    }
    let ww = if x + w > fw { fw - x } else { w };
    let hh = if y + h > fh { fh - y } else { h };
    graphics::fill_rect(ox + x, oy + y, ww, hh, c);
}

fn blit_decoded(ox: usize, oy: usize, fw: usize, fh: usize) {
    let (sw, sh) = unsafe {
        let w = if DEC_OUT_W > 0 { DEC_OUT_W } else { DEC_W };
        let h = if DEC_OUT_H > 0 { DEC_OUT_H } else { DEC_H };
        (w.max(1), h.max(1))
    };
    // Fast path: 2x2 blocks (4x fewer ops than put_pixel per pixel)
    let step = 1usize;
    let mut y = 0usize;
    while y < fh {
        let sy = y * sh / fh;
        let mut x = 0usize;
        while x < fw {
            let sx = x * sw / fw;
            let c = unsafe { FRAME_RGB[sy * sw + sx] };
            let bw = if x + step > fw { fw - x } else { step };
            let bh = if y + step > fh { fh - y } else { step };
            graphics::fill_rect(ox + x, oy + y, bw, bh, c);
            x += step;
        }
        y += step;
    }
}

// --- Software multi-scene (when no disk / demux fail) ---
fn scene_bars(ox: usize, oy: usize, fw: usize, fh: usize, f: u32) {
    let colors: [u32; 8] = [
        0x00C0C0C0, 0x00C0C000, 0x0000C0C0, 0x0000C000, 0x00C000C0, 0x00C00000, 0x000000C0, 0x00000000,
    ];
    let mut i = 0usize;
    while i < 8 {
        fill(ox, oy, fw, fh, i * fw / 8, 0, fw / 8 + 1, fh * 2 / 3, colors[i]);
        i += 1;
    }
    fill(ox, oy, fw, fh, 0, fh * 3 / 4, fw, fh / 4, 0x00101010);
    let mx = ((f * 4) as usize) % fw.saturating_sub(8).max(1);
    fill(ox, oy, fw, fh, mx, fh * 3 / 4 + 4, 8, fh / 4 - 8, 0x00FFFFFF);
    graphics::draw_str(ox + 8, oy + 8, "SCENE1 BARS", 0x00FFFFFF);
}
fn scene_plasma(ox: usize, oy: usize, fw: usize, fh: usize, f: u32) {
    let step = 2usize;
    let mut y = 0usize;
    while y < fh {
        let mut x = 0usize;
        while x < fw {
            let t = f as i32;
            let v1 = (x as i32 * 3 + t * 2) & 63;
            let v2 = (y as i32 * 2 + t) & 63;
            let r = (v1 * 3) as u32 & 255;
            let g = (v2 * 4) as u32 & 255;
            let b = ((x as i32 + y as i32 + t) as u32 * 5) & 255;
            fill(ox, oy, fw, fh, x, y, step, step, rgb(r, g, b));
            x += step;
        }
        y += step;
    }
    graphics::draw_str(ox + 8, oy + 8, "SCENE2 PLASMA", 0x00FFFFFF);
}
fn scene_check(ox: usize, oy: usize, fw: usize, fh: usize, f: u32) {
    let cell = 8 + ((f / 8) % 12) as usize;
    let mut y = 0usize;
    while y < fh {
        let mut x = 0usize;
        while x < fw {
            let c = if ((x / cell) + (y / cell) + (f as usize / 4)) & 1 == 0 {
                0x00204060
            } else {
                0x00E8F0F8
            };
            fill(ox, oy, fw, fh, x, y, cell, cell, c);
            x += cell;
        }
        y += cell;
    }
    graphics::draw_str(ox + 8, oy + 8, "SCENE3 CHECKER", 0x00000000);
}
fn scene_tunnel(ox: usize, oy: usize, fw: usize, fh: usize, f: u32) {
    fill(ox, oy, fw, fh, 0, 0, fw, fh, 0x00080810);
    let cx = fw / 2;
    let cy = fh / 2;
    let mut i = 0u32;
    while i < 16 {
        let phase = (f + i * 4) % 32;
        let w = 4 + phase as usize * fw / 40;
        let h = 4 + phase as usize * fh / 40;
        let c = rgb(40 + phase * 4, 20, 80 + phase * 3);
        if w < fw && h < fh {
            graphics::border_rect(
                ox + cx.saturating_sub(w / 2),
                oy + cy.saturating_sub(h / 2),
                w,
                h,
                c,
            );
        }
        i += 1;
    }
    graphics::draw_str(ox + 8, oy + 8, "SCENE4 TUNNEL", 0x00FFFFFF);
}
fn scene_stars(ox: usize, oy: usize, fw: usize, fh: usize, f: u32) {
    fill(ox, oy, fw, fh, 0, 0, fw, fh, 0x00000018);
    let mut i = 0u32;
    while i < 40 {
        let sx = ((i * 97 + f * 3) as usize) % fw.max(1);
        let sy = ((i * 57 + f) as usize) % fh.max(1);
        let bright = 120 + ((i * 13) % 135);
        fill(ox, oy, fw, fh, sx, sy, 2, 2, rgb(bright, bright, bright + 20));
        i += 1;
    }
    graphics::draw_str(ox + 8, oy + 8, "SCENE5 STARS", 0x00A0E0FF);
}

fn draw_clip_test(ox: usize, oy: usize, fw: usize, fh: usize, f: u32) {
    let mut y = 0usize;
    while y < fh {
        let t = (y as u32 * 180 / fh.max(1) as u32) as u32;
        let col = (0x20u32 + t / 2) << 16 | (0x40 + t / 3) << 8 | (0x80 + t / 2);
        fill(ox, oy, fw, fh, 0, y, fw, 1, col);
        y += 1;
    }
    let period = 90u32;
    let t = f % period;
    let phase = if t < period / 2 { t } else { period - t };
    let max_x = fw.saturating_sub(24) as u32;
    let sx = ((phase as u32 * max_x) / (period / 2).max(1)) as usize;
    fill(ox, oy, fw, fh, sx, 20, 24, 24, 0x00FFCC00);
    graphics::draw_str(ox + 8, oy + 8, "TEST.MP4 SIMPLE", 0x00FFFFFF);
}

fn draw_clip_demo(ox: usize, oy: usize, fw: usize, fh: usize, f: u32) {
    let scene = (f / (15 * 4)) % 5;
    let local = f % (15 * 4);
    match scene {
        0 => scene_bars(ox, oy, fw, fh, local),
        1 => scene_plasma(ox, oy, fw, fh, local),
        2 => scene_check(ox, oy, fw, fh, local),
        3 => scene_tunnel(ox, oy, fw, fh, local),
        _ => scene_stars(ox, oy, fw, fh, local),
    }
}

pub fn draw_frame(vx: usize, vy: usize, vw: usize, vh: usize) {
    if vw < 8 || vh < 8 {
        return;
    }
    graphics::fill_rect(vx, vy, vw, vh, 0x00000000);
    let fw = TEST_W.min(vw.saturating_sub(4));
    let fh = TEST_H.min(vh.saturating_sub(4));
    let ox = vx + (vw.saturating_sub(fw)) / 2;
    let oy = vy + (vh.saturating_sub(fh)) / 2;
    let f = unsafe { FRAME };

    unsafe {
        if FROM_DISK {
            if FRAME_OK {
                blit_decoded(ox, oy, fw, fh);
            } else {
                // Disk file identity strip + status
                fill(ox, oy, fw, fh, 0, 0, fw, fh, 0x00101828);
                graphics::draw_str(ox + 8, oy + 8, clip_name(), 0x00FFFFFF);
                let msg = match CODEC {
                    Codec::H264 => "H.264 demux OK - pixel decode pending",
                    Codec::Mjpeg => "MJPEG frame decode failed",
                    Codec::Unknown => "Container/codec not fully parsed",
                    Codec::None => "Loading...",
                };
                graphics::draw_str(ox + 8, oy + 28, msg, 0x00FFE080);
                // progress by sample index
                if SAMPLE_N > 0 {
                    let bar_w = fw.saturating_sub(16);
                    let filled = bar_w * SAMPLE_I / SAMPLE_N.max(1);
                    fill(ox, oy, fw, fh, 8, fh - 24, bar_w, 10, 0x00304050);
                    fill(ox, oy, fw, fh, 8, fh - 24, filled, 10, 0x0030A060);
                }
            }
            // HUD
            graphics::draw_str(ox + 8, oy + fh - 12, "FROM DISK", 0x00A0E0FF);
        } else if CLIP_SW == 0 {
            draw_clip_test(ox, oy, fw, fh, f);
        } else {
            draw_clip_demo(ox, oy, fw, fh, f);
        }
    }
}

pub fn status_line(out: &mut [u8]) -> usize {
    let s: &[u8] = match unsafe { STATE } {
        State::Playing => b"PLAYING",
        State::Paused => b"PAUSED",
        State::Stopped => b"STOPPED",
        State::Error => b"ERROR",
    };
    let mut n = 0usize;
    while n < s.len() && n < out.len() {
        out[n] = s[n];
        n += 1;
    }
    n
}

pub fn disk_info_line(out: &mut [u8]) -> usize {
    // "S=12/100 H264" style
    unsafe {
        let mut n = 0usize;
        let push = |out: &mut [u8], n: &mut usize, b: u8| {
            if *n < out.len() {
                out[*n] = b;
                *n += 1;
            }
        };
        push(out, &mut n, b'S');
        push(out, &mut n, b'=');
        // sample index decimal
        let mut v = SAMPLE_I;
        let mut tmp = [0u8; 8];
        let mut ti = 0usize;
        if v == 0 {
            tmp[0] = b'0';
            ti = 1;
        } else {
            while v > 0 && ti < 8 {
                tmp[ti] = b'0' + (v % 10) as u8;
                v /= 10;
                ti += 1;
            }
        }
        while ti > 0 {
            ti -= 1;
            push(out, &mut n, tmp[ti]);
        }
        push(out, &mut n, b'/');
        v = SAMPLE_N;
        ti = 0;
        if v == 0 {
            tmp[0] = b'0';
            ti = 1;
        } else {
            while v > 0 && ti < 8 {
                tmp[ti] = b'0' + (v % 10) as u8;
                v /= 10;
                ti += 1;
            }
        }
        while ti > 0 {
            ti -= 1;
            push(out, &mut n, tmp[ti]);
        }
        n
    }
}
