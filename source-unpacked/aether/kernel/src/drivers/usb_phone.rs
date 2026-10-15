//! USB smartphone support for Aether
//!
//! Target modes when a phone is plugged in:
//! 1. USB Mass Storage (BOT) - "File transfer" / USB storage
//! 2. PTP/MTP (Still Image class 0x06) - detection + status (full MTP later)
//! 3. ADB (vendor-specific) - detection only for now
//!
//! Stack: xHCI host → SET_CONFIGURATION → Bulk IN/OUT → SCSI via CBW/CSW

use crate::serial;

/// USB class codes of interest
pub const CLASS_MASS_STORAGE: u8 = 0x08;
pub const CLASS_STILL_IMAGE: u8 = 0x06; // PTP/MTP
pub const CLASS_VENDOR: u8 = 0xFF; // often ADB

pub const SUBCLASS_SCSI: u8 = 0x06;
pub const PROTO_BOT: u8 = 0x50; // Bulk-Only Transport

/// CBW signature 'USBC'
pub const CBW_SIG: u32 = 0x4342_5355;
/// CSW signature 'USBS'
pub const CSW_SIG: u32 = 0x5342_5355;

#[derive(Clone, Copy, PartialEq)]
pub enum PhoneMode {
    None,
    MassStorage,
    Mtp,
    Adb,
    ChargingOnly,
    Unknown,
}

#[derive(Clone, Copy)]
pub struct PhoneInfo {
    pub mode: PhoneMode,
    pub vid: u16,
    pub pid: u16,
    pub slot: u8,
    pub port: u8,
    pub ep_bulk_in: u8,
    pub ep_bulk_out: u8,
    pub mps_in: u16,
    pub mps_out: u16,
    pub lun: u8,
    pub block_size: u32,
    pub block_count: u32,
    pub mounted: bool,
}

impl PhoneInfo {
    pub const fn empty() -> Self {
        Self {
            mode: PhoneMode::None,
            vid: 0,
            pid: 0,
            slot: 0,
            port: 0,
            ep_bulk_in: 0,
            ep_bulk_out: 0,
            mps_in: 512,
            mps_out: 512,
            lun: 0,
            block_size: 0,
            block_count: 0,
            mounted: false,
        }
    }
}

static mut PHONE: PhoneInfo = PhoneInfo::empty();
static mut STATUS: [u8; 80] = [0; 80];
static mut STATUS_LEN: usize = 0;
static mut MANUFACTURER: [u8; 32] = [0; 32];
static mut PRODUCT: [u8; 32] = [0; 32];
static mut MAN_LEN: usize = 0;
static mut PROD_LEN: usize = 0;

fn set_status(s: &[u8]) {
    unsafe {
        let mut i = 0;
        while i < 79 && i < s.len() {
            STATUS[i] = s[i];
            i += 1;
        }
        STATUS_LEN = i;
    }
}

pub fn is_connected() -> bool {
    unsafe { PHONE.mode != PhoneMode::None }
}

pub fn is_mounted() -> bool {
    unsafe { PHONE.mounted }
}

pub fn info() -> PhoneInfo {
    unsafe { PHONE }
}

pub fn status_bytes() -> &'static [u8] {
    unsafe { &STATUS[..STATUS_LEN] }
}

pub fn mode_name(m: PhoneMode) -> &'static str {
    match m {
        PhoneMode::None => "No device",
        PhoneMode::MassStorage => "Mass Storage (file transfer)",
        PhoneMode::Mtp => "MTP / PTP",
        PhoneMode::Adb => "ADB (debug)",
        PhoneMode::ChargingOnly => "Charging only",
        PhoneMode::Unknown => "Unknown USB device",
    }
}

/// Parse config descriptor for phone-like interfaces.
/// Returns (mode, bulk_in_addr, bulk_out_addr, mps_in, mps_out)
pub fn find_phone_interface(cfg: &[u8]) -> Option<(PhoneMode, u8, u8, u16, u16)> {
    let mut mode = PhoneMode::None;
    let mut bulk_in = 0u8;
    let mut bulk_out = 0u8;
    let mut mps_in = 512u16;
    let mut mps_out = 512u16;
    let mut in_msc = false;
    let mut in_mtp = false;
    let mut in_adb = false;

    let mut off = 0usize;
    while off + 2 <= cfg.len() {
        let blen = cfg[off] as usize;
        let bty = cfg[off + 1];
        if blen < 2 || off + blen > cfg.len() {
            break;
        }
        if bty == 4 && blen >= 9 {
            // Interface
            let ic = cfg[off + 5];
            let is = cfg[off + 6];
            let ip = cfg[off + 7];
            in_msc = false;
            in_mtp = false;
            in_adb = false;
            if ic == CLASS_MASS_STORAGE && (is == SUBCLASS_SCSI || is == 0x05 || is == 0x02) && ip == PROTO_BOT
            {
                in_msc = true;
                mode = PhoneMode::MassStorage;
            } else if ic == CLASS_STILL_IMAGE {
                in_mtp = true;
                mode = PhoneMode::Mtp;
            } else if ic == CLASS_VENDOR && is == 0x42 {
                // Google ADB often 0xFF / 0x42
                in_adb = true;
                mode = PhoneMode::Adb;
            } else if ic == CLASS_VENDOR {
                in_adb = true;
                if mode == PhoneMode::None {
                    mode = PhoneMode::Unknown;
                }
            }
        } else if bty == 5 && blen >= 7 {
            // Endpoint
            let addr = cfg[off + 2];
            let attr = cfg[off + 3];
            let mps = cfg[off + 4] as u16 | ((cfg[off + 5] as u16) << 8);
            let is_bulk = (attr & 0x03) == 2;
            if is_bulk && (in_msc || in_mtp || in_adb) {
                if (addr & 0x80) != 0 {
                    bulk_in = addr;
                    mps_in = if mps == 0 { 512 } else { mps };
                } else {
                    bulk_out = addr;
                    mps_out = if mps == 0 { 512 } else { mps };
                }
            }
        }
        off += blen;
    }

    if mode != PhoneMode::None && (bulk_in != 0 || mode == PhoneMode::ChargingOnly) {
        Some((mode, bulk_in, bulk_out, mps_in, mps_out))
    } else if mode != PhoneMode::None {
        Some((mode, bulk_in, bulk_out, mps_in, mps_out))
    } else {
        None
    }
}

/// Build a BOT Command Block Wrapper (31 bytes)
pub fn build_cbw(
    tag: u32,
    data_len: u32,
    flags_in: bool,
    lun: u8,
    cb_len: u8,
    cb: &[u8],
    out: &mut [u8; 31],
) {
    // dCBWSignature
    out[0] = 0x55;
    out[1] = 0x53;
    out[2] = 0x42;
    out[3] = 0x43;
    // dCBWTag
    out[4] = tag as u8;
    out[5] = (tag >> 8) as u8;
    out[6] = (tag >> 16) as u8;
    out[7] = (tag >> 24) as u8;
    // dCBWDataTransferLength
    out[8] = data_len as u8;
    out[9] = (data_len >> 8) as u8;
    out[10] = (data_len >> 16) as u8;
    out[11] = (data_len >> 24) as u8;
    // bmCBWFlags: 0x80 = IN
    out[12] = if flags_in { 0x80 } else { 0x00 };
    out[13] = lun;
    out[14] = cb_len;
    let mut i = 0usize;
    while i < 16 {
        out[15 + i] = if i < cb.len() { cb[i] } else { 0 };
        i += 1;
    }
}

/// SCSI INQUIRY (6-byte CDB)
pub fn scsi_inquiry(cb: &mut [u8; 16]) {
    *cb = [0; 16];
    cb[0] = 0x12; // INQUIRY
    cb[4] = 36; // allocation length
}

/// SCSI TEST UNIT READY
pub fn scsi_test_unit_ready(cb: &mut [u8; 16]) {
    *cb = [0; 16];
    cb[0] = 0x00;
}

/// SCSI READ CAPACITY (10)
pub fn scsi_read_capacity10(cb: &mut [u8; 16]) {
    *cb = [0; 16];
    cb[0] = 0x25;
}

/// SCSI READ (10)
pub fn scsi_read10(cb: &mut [u8; 16], lba: u32, blocks: u16) {
    *cb = [0; 16];
    cb[0] = 0x28;
    cb[2] = (lba >> 24) as u8;
    cb[3] = (lba >> 16) as u8;
    cb[4] = (lba >> 8) as u8;
    cb[5] = lba as u8;
    cb[7] = (blocks >> 8) as u8;
    cb[8] = blocks as u8;
}

/// Record a detected phone from enumeration
pub fn register_device(
    mode: PhoneMode,
    vid: u16,
    pid: u16,
    slot: u8,
    port: u8,
    ep_in: u8,
    ep_out: u8,
    mps_in: u16,
    mps_out: u16,
) {
    unsafe {
        PHONE = PhoneInfo {
            mode,
            vid,
            pid,
            slot,
            port,
            ep_bulk_in: ep_in,
            ep_bulk_out: ep_out,
            mps_in,
            mps_out,
            lun: 0,
            block_size: 0,
            block_count: 0,
            mounted: false,
        };
    }
    match mode {
        PhoneMode::MassStorage => set_status(b"Phone: Mass Storage - configure USB mode on phone"),
        PhoneMode::Mtp => set_status(b"Phone: MTP detected (protocol WIP)"),
        PhoneMode::Adb => set_status(b"Phone: ADB interface detected"),
        _ => set_status(b"Phone: USB device registered"),
    }
    serial::write_str("[PHONE] registered mode=");
    serial::write_str(mode_name(mode));
    serial::write_str(" VID=");
    serial::write_hex(vid as usize);
    serial::write_str(" PID=");
    serial::write_hex(pid as usize);
    serial::write_str(" IN=0x");
    serial::write_hex(ep_in as usize);
    serial::write_str(" OUT=0x");
    serial::write_hex(ep_out as usize);
    serial::write_str("\n");
}

pub fn set_capacity(block_size: u32, block_count: u32) {
    unsafe {
        PHONE.block_size = block_size;
        PHONE.block_count = block_count;
        PHONE.mounted = block_size > 0 && block_count > 0;
    }
    if block_size > 0 {
        set_status(b"Phone storage ready (BOT)");
    }
}

pub fn set_status_phone_bulk_ready() {
    set_status(b"Phone MSC: bulk EPs ready - unlock phone");
}

pub fn clear() {
    unsafe {
        PHONE = PhoneInfo::empty();
        STATUS_LEN = 0;
        MAN_LEN = 0;
        PROD_LEN = 0;
    }
}

/// Draw status panel for desktop Phone window
pub fn draw(wx: usize, wy: usize, ww: usize, wh: usize, title_h: usize) {
    use crate::graphics;
    let cy = wy + title_h;
    let ch = if wh > title_h { wh - title_h } else { 0 };
    graphics::fill_rect(wx + 2, cy, ww.saturating_sub(4), ch, 0x00F0F0F0);
    graphics::draw_str(wx + 16, cy + 16, "USB Smartphone", 0x00000000);
    graphics::draw_str(wx + 16, cy + 40, "Connect phone and set File transfer mode", 0x00404040);

    let p = info();
    graphics::draw_str(wx + 16, cy + 72, "Status:", 0x00000000);
    let name = mode_name(p.mode);
    graphics::draw_str(wx + 80, cy + 72, name, 0x00008000);

    if p.mode != PhoneMode::None {
        graphics::draw_str(wx + 16, cy + 96, "VID:PID", 0x00404040);
        // hex will be drawn simply
        graphics::draw_str(wx + 16, cy + 120, "Bulk endpoints configured when MSC", 0x00404040);
        if p.mounted {
            graphics::draw_str(wx + 16, cy + 144, "Storage: mounted (BOT)", 0x00008000);
            graphics::draw_str(wx + 16, cy + 168, "Use Explorer after mount path is bound", 0x00404040);
        } else if p.mode == PhoneMode::MassStorage {
            graphics::draw_str(wx + 16, cy + 144, "Storage: not mounted yet", 0x00800000);
            graphics::draw_str(wx + 16, cy + 168, "Unlock phone + File transfer / MTP", 0x00404040);
        }
        if p.ep_bulk_in != 0 {
            graphics::draw_str(wx + 16, cy + 200, "Bulk IN/OUT endpoints present", 0x00000080);
        }
    } else {
        graphics::draw_str(wx + 16, cy + 96, "No smartphone on USB", 0x00800000);
        graphics::draw_str(wx + 16, cy + 120, "1. Plug USB cable", 0x00404040);
        graphics::draw_str(wx + 16, cy + 140, "2. Unlock phone", 0x00404040);
        graphics::draw_str(wx + 16, cy + 160, "3. Choose File transfer / USB storage", 0x00404040);
        graphics::draw_str(wx + 16, cy + 180, "4. Type PH in Terminal to rescan", 0x00404040);
    }

    // status line
    unsafe {
        if STATUS_LEN > 0 {
            graphics::draw_bytes(wx + 16, cy + ch.saturating_sub(28), &STATUS[..STATUS_LEN], 0x00000000);
        }
    }
}

pub fn on_terminal_cmd(cmd: &[u8]) -> bool {
    // PH = phone rescan/status
    if cmd.len() >= 2 && (cmd[0] == b'P' || cmd[0] == b'p') && (cmd[1] == b'H' || cmd[1] == b'h') {
        serial::write_str("[PHONE] status\n");
        let p = info();
        serial::write_str("  mode=");
        serial::write_str(mode_name(p.mode));
        serial::write_str("\n");
        crate::drivers::xhci::rescan_phone();
        true
    } else {
        false
    }
}
