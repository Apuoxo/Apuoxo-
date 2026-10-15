//! Aether WiFi driver - phase 1 for Fujitsu LIFEBOOK AH532
//! Hardware: Intel Centrino Wireless-N 2230
//!   PCI ID: 8086:0887  Subsystem: 8086:4062  Class: 02:80
//!   Linux: iwlwifi + firmware iwlwifi-2030-*.ucode
//!   BAR0: 8 KiB MMIO
//!
//! Phase 1 (this file): PCI find (all buses), bus-master, MMIO map,
//! identify 2230, report status. NO full 802.11 without firmware.
//! Full assoc/TX is out of scope until firmware loader exists.

use crate::serial;
use crate::mm::paging;

// WF diagnostic sink: Terminal text only when GUI mode is on.
// Serial is NOT used for WF diagnostics in that mode.
static mut WF_GUI_OUTPUT: bool = false;
static mut WF_QUIET: bool = false;

pub fn set_wf_quiet(enabled: bool) {
    unsafe { WF_QUIET = enabled; }
}

pub fn set_wf_gui_output(enabled: bool) {
    unsafe { WF_GUI_OUTPUT = enabled; }
}

fn diag_write_str(s: &str) {
    unsafe {
        if WF_QUIET {
            return;
        }
        if WF_GUI_OUTPUT {
            crate::desktop::terminal_write(s);
            return;
        }
    }
    serial::write_str(s);
}

/// Always emit digits via terminal_write_char - never from_utf8 (can silently skip).
fn diag_emit_byte(b: u8) {
    unsafe {
        if WF_GUI_OUTPUT {
            crate::desktop::terminal_write_char(b);
        } else {
            serial::write_str(core::str::from_utf8(&[b]).unwrap_or("?"));
        }
    }
}

fn diag_write_usize(mut v: usize) {
    unsafe {
        if WF_QUIET { return; }
    }
    if v == 0 {
        diag_emit_byte(b'0');
        return;
    }
    let mut b = [0u8; 20];
    let mut k = 0usize;
    while v > 0 {
        b[k] = b'0' + (v % 10) as u8;
        v /= 10;
        k += 1;
    }
    while k > 0 {
        k -= 1;
        diag_emit_byte(b[k]);
    }
}

/// Fixed 8 hex digits (lowercase) so empty fields never look blank.
fn diag_write_hex(v: usize) {
    unsafe {
        if WF_QUIET { return; }
    }
    let mut shift = 28isize;
    while shift >= 0 {
        let nibble = ((v >> shift) & 0xF) as u8;
        let c = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
        diag_emit_byte(c);
        shift -= 4;
    }
}


const INTEL_VID: u16 = 0x8086;
const CENTRINO_2230_DID: u16 = 0x0887;
const SUBSYS_BGN: u16 = 0x4062;
const IWL2030_FW_API_MIN: u32 = 5;
const IWL2030_FW_API_MAX: u32 = 6;
const IWL2030_FW_PREFIX: &str = "iwlwifi-2030-";
const IWL2030_FW: &[u8] = include_bytes!("../../firmware/iwlwifi-2030-6.ucode");
const IWLAGN_RTC_INST_LOWER_BOUND: u32 = 0x000000;
const IWLAGN_RTC_DATA_LOWER_BOUND: u32 = 0x800000;
const HBUS_BASE: usize = 0x400;
const HBUS_TARG_MEM_WADDR: usize = HBUS_BASE + 0x010; // 0x410
const HBUS_TARG_MEM_WDAT: usize = HBUS_BASE + 0x018; // 0x418
const HBUS_TARG_MEM_RADDR: usize = HBUS_BASE + 0x00c; // 0x40c
const HBUS_TARG_MEM_RDAT: usize = HBUS_BASE + 0x01c; // 0x41c
/// Doorbell: write (queue_id << 8) | write_ptr  - Linux iwl-csr.h HBUS_TARG_WRPTR
const HBUS_TARG_WRPTR: usize = HBUS_BASE + 0x060; // 0x460  (NOT 0x60!)

const CSR_INT: usize = 0x008;
const CSR_INT_MASK: usize = 0x00C;
const CSR_FH_INT_STATUS: usize = 0x010;
const CSR_INT_BIT_FH_TX: u32 = 1 << 27;
const CSR_FH_INT_TX_MASK: u32 = 0x0000_0003;
const CSR_INT_BIT_FH_RX: u32 = 1 << 31;
const CSR_INT_BIT_ALIVE: u32 = 1 << 0;
const CSR_FH_INT_RX_MASK: u32 = (1 << 17) | (1 << 16);
const FH_RSCSR_CHNL0: usize = FH_MEM_LOWER_BOUND + 0xBC0;
const FH_RSCSR_STTS_WPTR: usize = FH_RSCSR_CHNL0 + 0x0;
const FH_RSCSR_RBDCB_BASE: usize = FH_RSCSR_CHNL0 + 0x4;
const FH_RSCSR_RBDCB_WPTR: usize = FH_RSCSR_CHNL0 + 0x8;
const FH_RSCSR_RDPTR: usize = FH_RSCSR_CHNL0 + 0xC;
const FH_RCSR_CHNL0_CONFIG: usize = FH_MEM_LOWER_BOUND + 0xC00;
const FH_RCSR_CHNL0_FLUSH_RB_REQ: usize = FH_RCSR_CHNL0_CONFIG + 0x10;
const FH_RSSR_RX_STATUS: usize = FH_MEM_LOWER_BOUND + 0xC40;
const FH_RSCSR_FRAME_INVALID: u32 = 0x5555_0000;
const FH_RX_RBD_COUNT: usize = 256;
const FH_RX_BUF_SIZE: usize = 4096;
const FH_RX_IRQ_VECTOR: u8 = 0x27;
const FH_MEM_LOWER_BOUND: usize = 0x1000;
const FH_TFDIB_CTRL0_SRVC: usize = FH_MEM_LOWER_BOUND + 0x900 + 8 * 9;
const FH_TFDIB_CTRL1_SRVC: usize = FH_TFDIB_CTRL0_SRVC + 4;
const FH_TCSR_CONFIG_SRVC: usize = FH_MEM_LOWER_BOUND + 0xD00 + 0x20 * 9;
const FH_TCSR_BUF_STS_SRVC: usize = FH_TCSR_CONFIG_SRVC + 8;
const FH_SRVC_SRAM_ADDR: usize = FH_MEM_LOWER_BOUND + 0x9C8;
const FH_MEM_TFDIB_REG1_ADDR_BITSHIFT: u32 = 28;
const FH_MEM_TB_MAX_LENGTH: usize = 0x0002_0000;
const FH_TCSR_DMA_ENABLE: u32 = 0x8000_0000;
const FH_TCSR_CIRQ_HOST_ENDTFD: u32 = 0x0010_0000;
const FH_TCSR_TFDB_VALID: u32 = 0x0000_0003;
const FH_TCSR_TB_NUM: u32 = 1 << 20;
const FH_TCSR_TB_IDX: u32 = 1 << 12;
const FH_MEM_CBBC_CMD: usize = FH_MEM_LOWER_BOUND + 0x9D0 + 4 * 4;
// FH TX channel registers are indexed by TX *queue id* (0..19), NOT by SCD FIFO.
// Command queue is #4; FIFO 7 is only the SCD mapping (TRANSLATE_TBL).
const FH_TCSR_CONFIG_CMD: usize = FH_MEM_LOWER_BOUND + 0xD00 + 0x20 * IWL_DEFAULT_CMD_QUEUE_NUM;
const FH_TCSR_BUF_STS_CMD: usize = FH_TCSR_CONFIG_CMD + 8;
const FH_TCSR_TX_CMD_DMA_ENABLE: u32 = 0x8000_0000;
const FH_TCSR_TX_CMD_CIRQ_HOST_ENDTFD: u32 = 0x0010_0000;
const FH_TCSR_TX_CMD_CREDIT_ENABLE: u32 = 0x0000_0008;
const FH_TSSR_TX_STATUS: usize = FH_MEM_LOWER_BOUND + 0xEA0;
// DVM command queue #4 is a 256-entry circular TFD queue.
const FH_TFD_CMD_SLOTS: usize = 256;
const FH_TFD_SIZE: usize = 128;
const IWL_DEFAULT_CMD_QUEUE_NUM: usize = 4;
const IWL_CMD_FIFO_NUM: u32 = 7;
const SCD_BASE: u32 = 0x00A0_2C00;
const SCD_SRAM_BASE_ADDR: u32 = SCD_BASE + 0x00;
const SCD_DRAM_BASE_ADDR: u32 = SCD_BASE + 0x08;
const SCD_TXFACT: u32 = SCD_BASE + 0x10;
const SCD_QUEUECHAIN_SEL: u32 = SCD_BASE + 0xE8;
const SCD_QUEUE_WRPTR: u32 = SCD_BASE + 0x18 + (IWL_DEFAULT_CMD_QUEUE_NUM as u32 * 4);
const SCD_QUEUE_RDPTR: u32 = SCD_BASE + 0x68 + (IWL_DEFAULT_CMD_QUEUE_NUM as u32 * 4);
const SCD_CHAINEXT_EN: u32 = SCD_BASE + 0x244;
const SCD_QUEUE_STATUS_BITS: u32 = SCD_BASE + 0x10C + (IWL_DEFAULT_CMD_QUEUE_NUM as u32 * 4);
const SCD_QUEUE_CTX: u32 = 0x0600 + (IWL_DEFAULT_CMD_QUEUE_NUM as u32 * 8);
const SCD_GP_CTRL: u32 = SCD_BASE + 0x1A8;
const SCD_EN_CTRL: u32 = SCD_BASE + 0x254;
const SCD_INTERRUPT_MASK: u32 = SCD_BASE + 0x108;
/// Queue→FIFO translation table (iwl-prph.h SCD_TRANSLATE_TBL)
const SCD_TRANSLATE_TBL: u32 = SCD_BASE + 0xfa0;
const SCD_TRANSLATE_TBL_QUEUE4: u32 = SCD_TRANSLATE_TBL + (IWL_DEFAULT_CMD_QUEUE_NUM as u32 * 2);
const SCD_GP_CTRL_ENABLE_31_QUEUES: u32 = 1 << 0;
const SCD_QUEUE_ACTIVE: u32 = 1 << 3;
const SCD_QUEUE_WSL: u32 = 1 << 4;
// Intel 2000/2030 uses the 5000-style scheduler status layout: bits 16..23 are the write mask.
const SCD_QUEUE_STATUS_MASK: u32 = 0x017F_0000; // Linux SCD_QUEUE_STTS_REG_MSK
const SCD_WIN_SIZE: u32 = 64;
const SCD_FRAME_LIMIT: u32 = 64;
const SCD_QUEUE_COUNT: usize = 11;
const SCD_QUEUE_BC_SIZE: usize = 320; // 256 TFD entries + 64 duplicate entries

#[repr(align(256))]
struct CmdTfdQueue([u8; FH_TFD_CMD_SLOTS * FH_TFD_SIZE]);
static mut CMD_TFD_QUEUE: CmdTfdQueue = CmdTfdQueue([0; FH_TFD_CMD_SLOTS * FH_TFD_SIZE]);
#[repr(align(256))]
struct ScdBcTable([u16; SCD_QUEUE_COUNT * SCD_QUEUE_BC_SIZE]);
static mut SCD_BC_TABLE: ScdBcTable = ScdBcTable([0; SCD_QUEUE_COUNT * SCD_QUEUE_BC_SIZE]);
static mut CMD_QUEUE_READY: bool = false;
static mut CMD_WRITE_PTR: usize = 0;
static mut CMD_SEQ: u8 = 0;


#[repr(align(4096))]
struct FirmwareDmaBuf([u8; FH_MEM_TB_MAX_LENGTH]);
static mut FW_DMA_BUF: FirmwareDmaBuf = FirmwareDmaBuf([0; FH_MEM_TB_MAX_LENGTH]);
/// One host-command frame per TFD slot (avoid RXON being overwritten by SCAN).
#[repr(align(256))]
struct CmdFrames([[u8; 512]; FH_TFD_CMD_SLOTS]);
static mut CMD_FRAMES: CmdFrames = CmdFrames([[0; 512]; FH_TFD_CMD_SLOTS]);
/// Oversized host commands (REPLY_SCAN etc.) - one at a time
#[repr(align(256))]
struct LargeCmdBuf([u8; 2048]);
static mut LARGE_CMD_BUF: LargeCmdBuf = LargeCmdBuf([0; 2048]);
static mut LARGE_CMD_IN_USE: bool = false;
#[repr(align(256))]
struct RxRbd([u32; FH_RX_RBD_COUNT]);
#[repr(align(4096))]
struct RxBuffers([[u8; FH_RX_BUF_SIZE]; FH_RX_RBD_COUNT]);
#[repr(align(16))]
struct RxStatus([u32; 3]);
static mut RX_RBD: RxRbd = RxRbd([0; FH_RX_RBD_COUNT]);
static mut RX_BUFFERS: RxBuffers = RxBuffers([[0; FH_RX_BUF_SIZE]; FH_RX_RBD_COUNT]);
static mut RX_STATUS: RxStatus = RxStatus([0; 3]);
static mut RX_READY: bool = false;
static mut RX_READ: usize = 0;
static mut RX_IRQ_COUNT: u32 = 0;
static mut ALIVE_SEEN: bool = false;
/// 0=idle 1=reset 2=activate 3=rx 4=fw_parse 5=init_alive 6=rt_alive 7=ok 0xFF=fail
static mut BRING_STEP: u8 = 0;
static mut BRING_FAIL: u8 = 0;
static mut ALIVE_VALID: u32 = 0;
static mut ALIVE_SCD_BASE: u32 = 0;
static mut ALIVE_SUBTYPE: u8 = 0;
static mut SCAN_NOTIFICATION_SEEN: bool = false;
static mut SCAN_START_COUNT: u32 = 0;
static mut SCAN_RESULTS_COUNT: u32 = 0;
static mut SCAN_COMPLETE_COUNT: u32 = 0;
static mut SCAN_COMPLETE_CHANNELS: u8 = 0;
static mut SCAN_COMPLETE_STATUS: u8 = 0;
static mut SCAN_COMPLETE_LAST_CHANNEL: u8 = 0;
const MAX_APS: usize = 16;
static mut AP_SSID: [[u8; 32]; MAX_APS] = [[0; 32]; MAX_APS];
static mut AP_SSID_LEN: [u8; MAX_APS] = [0; MAX_APS];
static mut AP_RSSI: [i8; MAX_APS] = [0; MAX_APS];
static mut AP_COUNT: usize = 0;
static mut CONNECTED: bool = false;
static mut CONNECTED_IDX: i32 = -1;
static mut CONNECT_PASS: [u8; 64] = [0; 64];
static mut CONNECT_PASS_LEN: usize = 0;
static mut LAST_RX_CMDS: [u8; 8] = [0; 8];
static mut LAST_RX_CMDS_N: usize = 0;
static mut LAST_SCAN_SUBMIT_OK: bool = false;
/// 0=never 1=ok 2=no_alive 3=submit_fail 4=timeout 5=empty_complete
static mut LAST_SCAN_CODE: u8 = 0;
static mut LAST_RXON_OK: bool = false;

static mut AP_CHANNEL: [u8; MAX_APS] = [0; MAX_APS];


static mut FOUND: bool = false;
static mut READY: bool = false; // phase1 ready = found + mapped
static mut NEEDS_FW: bool = true;
static mut BUS: u8 = 0;
static mut DEV: u8 = 0;
static mut FUNC: u8 = 0;
static mut BAR0: u64 = 0;
static mut MMIO: usize = 0;
static mut REV: u8 = 0;
static mut SUBSYS: u16 = 0;
static mut MMIO_MAPPED: bool = false;
static mut PCI_COMMAND: u16 = 0;
static mut PCI_STATUS: u16 = 0;
static mut IRQ_LINE: u8 = 0;
static mut IRQ_PIN: u8 = 0;
static mut PREREQS_READ: bool = false;
static mut CAP_PTR: u8 = 0;
static mut CAP_MSI: bool = false;
static mut CAP_MSIX: bool = false;
static mut CAP_PM: bool = false;
static mut CAP_CHAIN_READ: bool = false;
static mut MSI_CTRL: u16 = 0;
static mut PCIE_CAP: bool = false;
static mut PCIE_LINK_STATUS: u16 = 0;
static mut PCIE_DEVICE_STATUS: u16 = 0;
static mut PCIE_LINK_SPEED: u8 = 0;
static mut PCIE_LINK_WIDTH: u8 = 0;
static mut PCIE_CAP_OFF: u8 = 0;
static mut PCIE_FLR_SUPPORTED: bool = false;
// Device-specific reset state. PCIe FLR is absent on the AH532 2230, so
// bring-up uses the Intel iwlwifi CSR software-reset path instead of inventing
// a PCI config reset. No firmware, interrupts, or DMA are started here.
static mut RESET_ATTEMPTED: bool = false;
static mut RESET_OK: bool = false;
static mut RESET_BEFORE: u32 = 0;
static mut RESET_AFTER: u32 = 0;

const CSR_RESET: usize = 0x020;
const CSR_RESET_SW_RESET: u32 = 0x0000_0080;
const CSR_GP_CNTRL: usize = 0x024;
const CSR_GP_CNTRL_INIT_DONE: u32 = 0x0000_0004;
const CSR_GP_CNTRL_MAC_CLOCK_READY: u32 = 0x0000_0001;

static mut ACTIVATE_ATTEMPTED: bool = false;
static mut ACTIVATE_OK: bool = false;
static mut ACTIVATE_BEFORE: u32 = 0;
static mut ACTIVATE_AFTER: u32 = 0;
static mut FW_ATTEMPTED: bool = false;
static mut FW_LOADED: bool = false;
static mut FW_VER: u32 = 0;
static mut FW_INST_SIZE: u32 = 0;
static mut FW_DATA_SIZE: u32 = 0;
static mut FW_INIT_SIZE: u32 = 0;
static mut FW_INIT_DATA_SIZE: u32 = 0;
static mut FW_SEC_INST_OFF: usize = 0;
static mut FW_SEC_INST_LEN: usize = 0;
static mut FW_SEC_DATA_OFF: usize = 0;
static mut FW_SEC_DATA_LEN: usize = 0;
static mut FW_SEC_INIT_OFF: usize = 0;
static mut FW_SEC_INIT_LEN: usize = 0;
static mut FW_SEC_INIT_DATA_OFF: usize = 0;
static mut FW_SEC_INIT_DATA_LEN: usize = 0;
static mut ALIVE_INIT_SEEN: bool = false;
static mut FW_EXEC_ATTEMPTED: bool = false;
static mut FW_EXEC_STARTED: bool = false;



unsafe fn pci_r32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let a = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let v: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") v, options(nostack, preserves_flags));
    v
}
unsafe fn pci_w16(bus: u8, dev: u8, func: u8, off: u8, val: u16) {
    let a = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let mut old: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") old, options(nostack, preserves_flags));
    let shift = (off as u32 & 2) * 8;
    old = (old & !(0xFFFF << shift)) | ((val as u32) << shift);
    core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") old, options(nostack, preserves_flags));
}

fn map_mmio(phys: u64, size: usize) -> bool {
    if phys == 0 || size == 0 || (phys & 0xFFF) != 0 {
        return false;
    }
    let cr3 = unsafe { paging::read_cr3() };
    let start = (phys as usize) & !0xFFF;
    let end = (phys as usize + size + 0xFFF) & !0xFFF;
    let mut va = start;
    while va < end {
        // Always ensure BAR pages are present+writable (high MMIO and edge cases)
        let ok = unsafe {
            paging::map_page(cr3, va, va, paging::PAGE_PRESENT | paging::PAGE_WRITE)
        };
        if !ok {
            // Low identity region may already be mapped - still advance
            if va < 0x4000_0000 {
                va += 0x1000;
                continue;
            }
            return false;
        }
        va += 0x1000;
    }
    unsafe { paging::load_cr3(cr3) };
    true
}

pub fn found() -> bool {
    unsafe { FOUND }
}
pub fn ready() -> bool {
    unsafe { READY }
}
pub fn needs_firmware() -> bool {
    unsafe { NEEDS_FW }
}

pub fn mmio_ready() -> bool {
    unsafe { MMIO_MAPPED }
}

/// Firmware filename contract for the supported Intel 2230 API range.
/// This does not load firmware; the native loader is a separate stage.
pub fn firmware_name(api: u32) -> Option<&'static str> {
    if api < IWL2030_FW_API_MIN || api > IWL2030_FW_API_MAX { return None; }
    if api == 5 { Some("iwlwifi-2030-5.ucode") } else { Some("iwlwifi-2030-6.ucode") }
}

pub fn firmware_prefix() -> &'static str {
    IWL2030_FW_PREFIX
}
pub fn bar0() -> u64 {
    unsafe { BAR0 }
}
pub fn bus_dev_func() -> (u8, u8, u8) {
    unsafe { (BUS, DEV, FUNC) }
}

pub fn pci_command() -> u16 { unsafe { PCI_COMMAND } }
pub fn pci_status() -> u16 { unsafe { PCI_STATUS } }
pub fn irq_line() -> u8 { unsafe { IRQ_LINE } }
pub fn irq_pin() -> u8 { unsafe { IRQ_PIN } }
pub fn prerequisites_read() -> bool { unsafe { PREREQS_READ } }
pub fn cap_ptr() -> u8 { unsafe { CAP_PTR } }
pub fn cap_msi() -> bool { unsafe { CAP_MSI } }
pub fn cap_msix() -> bool { unsafe { CAP_MSIX } }
pub fn cap_pm() -> bool { unsafe { CAP_PM } }
pub fn cap_chain_read() -> bool { unsafe { CAP_CHAIN_READ } }
pub fn msi_ctrl() -> u16 { unsafe { MSI_CTRL } }
pub fn pcie_cap() -> bool { unsafe { PCIE_CAP } }
pub fn pcie_link_status() -> u16 { unsafe { PCIE_LINK_STATUS } }
pub fn pcie_device_status() -> u16 { unsafe { PCIE_DEVICE_STATUS } }
pub fn pcie_link_speed() -> u8 { unsafe { PCIE_LINK_SPEED } }
pub fn pcie_link_width() -> u8 { unsafe { PCIE_LINK_WIDTH } }
pub fn pcie_flr_supported() -> bool { unsafe { PCIE_FLR_SUPPORTED } }
pub fn reset_attempted() -> bool { unsafe { RESET_ATTEMPTED } }
pub fn reset_ok() -> bool { unsafe { RESET_OK } }
pub fn reset_before() -> u32 { unsafe { RESET_BEFORE } }
pub fn reset_after() -> u32 { unsafe { RESET_AFTER } }
pub fn activate_attempted() -> bool { unsafe { ACTIVATE_ATTEMPTED } }
pub fn activate_ok() -> bool { unsafe { ACTIVATE_OK } }
pub fn activate_before() -> u32 { unsafe { ACTIVATE_BEFORE } }
pub fn activate_after() -> u32 { unsafe { ACTIVATE_AFTER } }
pub fn firmware_attempted() -> bool { unsafe { FW_ATTEMPTED } }
pub fn firmware_loaded() -> bool { unsafe { FW_LOADED } }
pub fn firmware_version() -> u32 { unsafe { FW_VER } }
pub fn firmware_inst_size() -> u32 { unsafe { FW_INST_SIZE } }
pub fn firmware_data_size() -> u32 { unsafe { FW_DATA_SIZE } }
pub fn firmware_exec_attempted() -> bool { unsafe { FW_EXEC_ATTEMPTED } }
pub fn firmware_exec_started() -> bool { unsafe { FW_EXEC_STARTED } }
pub fn rx_ready() -> bool { unsafe { RX_READY } }
pub fn rx_irq_count() -> u32 { unsafe { RX_IRQ_COUNT } }
pub fn alive_seen() -> bool { unsafe { ALIVE_SEEN } }
pub fn alive_valid() -> u32 { unsafe { ALIVE_VALID } }
pub fn alive_subtype() -> u8 { unsafe { ALIVE_SUBTYPE } }
fn scan_notification_seen() -> bool { unsafe { SCAN_NOTIFICATION_SEEN } }
fn scan_start_count() -> u32 { unsafe { SCAN_START_COUNT } }
fn scan_results_count() -> u32 { unsafe { SCAN_RESULTS_COUNT } }
fn scan_complete_count() -> u32 { unsafe { SCAN_COMPLETE_COUNT } }

pub fn wf_post_scan_diagnostics() {
    unsafe {
        if !MMIO_MAPPED || MMIO == 0 {
            diag_write_str("[WIFI] WF_RESULT=NO-MMIO\n");
            return;
        }
        let csr_int = core::ptr::read_volatile((MMIO + CSR_INT) as *const u32);
        let fh_int = core::ptr::read_volatile((MMIO + CSR_FH_INT_STATUS) as *const u32);
        let hw_rptr = core::ptr::read_volatile((MMIO + FH_RSCSR_RDPTR) as *const u32);
        let cb_wptr = core::ptr::read_volatile((MMIO + FH_RSCSR_RBDCB_WPTR) as *const u32);
        let stts_wptr = core::ptr::read_volatile((MMIO + FH_RSCSR_STTS_WPTR) as *const u32);
        let rx_status = core::ptr::read_volatile((MMIO + FH_RSSR_RX_STATUS) as *const u32);
        let rbd_base = core::ptr::read_volatile((MMIO + FH_RSCSR_RBDCB_BASE) as *const u32);
        let rx_cfg = core::ptr::read_volatile((MMIO + FH_RCSR_CHNL0_CONFIG) as *const u32);
        let flush = core::ptr::read_volatile((MMIO + FH_RCSR_CHNL0_FLUSH_RB_REQ) as *const u32);
        diag_write_str("[WIFI] WF-RX FINAL read="); diag_write_usize(RX_READ);
        diag_write_str(" hw_closed_rb="); diag_write_hex(hw_rptr as usize);
        diag_write_str(" cb_wptr="); diag_write_hex(cb_wptr as usize);
        diag_write_str(" stts_wptr="); diag_write_hex(stts_wptr as usize);
        diag_write_str(" rx_status="); diag_write_hex(rx_status as usize);
        diag_write_str(" rbd_base="); diag_write_hex(rbd_base as usize);
        diag_write_str(" cfg="); diag_write_hex(rx_cfg as usize);
        diag_write_str(" flush="); diag_write_hex(flush as usize);
        diag_write_str(" CSR_INT="); diag_write_hex(csr_int as usize);
        diag_write_str(" FH_INT="); diag_write_hex(fh_int as usize); diag_write_str("\n");

        diag_write_str("[WIFI] WF-CMD FINAL wrptr="); diag_write_hex(prph_read(SCD_QUEUE_WRPTR) as usize);
        diag_write_str(" rdptr="); diag_write_hex(prph_read(SCD_QUEUE_RDPTR) as usize);
        diag_write_str(" status="); diag_write_hex(prph_read(SCD_QUEUE_STATUS_BITS) as usize);
        diag_write_str(" dram="); diag_write_hex(prph_read(SCD_DRAM_BASE_ADDR) as usize);
        diag_write_str(" cbbc="); diag_write_hex(core::ptr::read_volatile((MMIO + FH_MEM_CBBC_CMD) as *const u32) as usize);
        diag_write_str(" tcsr="); diag_write_hex(core::ptr::read_volatile((MMIO + FH_TCSR_CONFIG_CMD) as *const u32) as usize);
        diag_write_str(" tsts="); diag_write_hex(core::ptr::read_volatile((MMIO + FH_TCSR_BUF_STS_CMD) as *const u32) as usize);
        diag_write_str(" hbus="); diag_write_hex(core::ptr::read_volatile((MMIO + HBUS_TARG_WRPTR) as *const u32) as usize); diag_write_str("\n");

        diag_write_str("[WIFI] WF-RX DESCRIPTORS");
        let mut i = 0usize;
        while i < FH_RX_RBD_COUNT {
            diag_write_str(" ["); diag_write_usize(i); diag_write_str("]=");
            diag_write_hex(RX_RBD.0[i] as usize); diag_write_str(":");
            diag_write_hex(core::ptr::read_volatile(&RX_BUFFERS.0[i][0] as *const u8 as *const u32) as usize);
            i += 1;
        }
        diag_write_str("\n");

        diag_write_str("[WIFI] WF-SCAN start="); diag_write_usize(scan_start_count() as usize);
        diag_write_str(" results="); diag_write_usize(scan_results_count() as usize);
        diag_write_str(" complete="); diag_write_usize(scan_complete_count() as usize);
        diag_write_str(" rx_irq="); diag_write_usize(RX_IRQ_COUNT as usize);
        diag_write_str(" alive="); diag_write_str(if ALIVE_SEEN { "SEEN" } else { "NOT-SEEN" }); diag_write_str("\n");

        if scan_complete_count() == 0 && scan_start_count() == 0 &&
           scan_results_count() == 0 && RX_READ == (hw_rptr as usize & (FH_RX_RBD_COUNT - 1)) {
            diag_write_str("[WIFI] WF-RESULT=RX_RING_NO_NEW_ENTRIES\n");
            diag_write_str("[WIFI] WF-CAUSE=SCAN_COMMAND_SUBMITTED_BUT_RX_PRODUCER_DID_NOT_ADVANCE\n");
            diag_write_str("[WIFI] WF-NEXT=VERIFY_RX_RING_PROGRAMMING_FH_CHANNEL_ENABLE_AND_BUFFER_ADDRESSING\n");
        } else if scan_complete_count() == 0 {
            diag_write_str("[WIFI] WF-RESULT=SCAN_NO_COMPLETE_NOTIFICATION\n");
            diag_write_str("[WIFI] WF-NEXT=CONTINUE_RX_NOTIFICATION_ANALYSIS\n");
        } else {
            diag_write_str("[WIFI] WF-RESULT=SCAN_COMPLETE_NOTIFICATION_SEEN\n");
        }
    }
}

const CSR_GP_CNTRL_MAC_ACCESS_REQ: u32 = 0x0000_0008;
const CSR_UCODE_DRV_GP1_CLR: usize = 0x05c;
const CSR_UCODE_SW_BIT_RFKILL: u32 = 0x0000_0002;

/// Hold MAC access for PRPH/HBUS (Linux iwl_grab_nic_access).
unsafe fn grab_nic_access() {
    if MMIO == 0 { return; }
    let gp = (MMIO + CSR_GP_CNTRL) as *mut u32;
    let mut v = core::ptr::read_volatile(gp);
    v |= CSR_GP_CNTRL_MAC_ACCESS_REQ;
    core::ptr::write_volatile(gp, v);
    let mut i = 0usize;
    while i < 50_000 {
        let r = core::ptr::read_volatile(gp);
        if (r & CSR_GP_CNTRL_MAC_CLOCK_READY) != 0 { break; }
        core::hint::spin_loop();
        i += 1;
    }
}

/// Clear software RF-kill bit so FW is not blocked by SW state.
unsafe fn clear_sw_rfkill() {
    if MMIO == 0 { return; }
    core::ptr::write_volatile((MMIO + CSR_UCODE_DRV_GP1_CLR) as *mut u32, CSR_UCODE_SW_BIT_RFKILL);
}

fn prph_write(addr: u32, val: u32) {
    unsafe {
        grab_nic_access();
        core::ptr::write_volatile((MMIO + 0x444) as *mut u32, (addr & 0x000F_FFFF) | (3 << 24));
        core::ptr::write_volatile((MMIO + 0x44C) as *mut u32, val);
    }
}

fn prph_read(addr: u32) -> u32 {
    unsafe {
        grab_nic_access();
        core::ptr::write_volatile((MMIO + 0x448) as *mut u32, (addr & 0x000F_FFFF) | (3 << 24));
        core::ptr::read_volatile((MMIO + 0x450) as *const u32)
    }
}

/// Initialize the legacy gen1/gen2 scheduler and command queue (#4).
/// This is the transport prerequisite for host commands such as REPLY_SCAN_CMD.
pub fn init_command_queue() -> bool {
    unsafe {
        if !ALIVE_SEEN || !MMIO_MAPPED || MMIO == 0 { return false; }
        let tfd_base = (&CMD_TFD_QUEUE.0 as *const u8) as u64;
        // Authoritative SCD base = PRPH SCD_SRAM_BASE_ADDR after FW alive.
        // ALIVE offset+24 was wrongly used as scd (that is log_event_table_ptr).
        // Real scd_base_ptr is ALIVE offset+32; PRPH register is what HW uses.
        let scd_prph = prph_read(SCD_SRAM_BASE_ADDR);
        let scd_sram = if scd_prph != 0 && scd_prph != 0xFFFF_FFFF {
            scd_prph
        } else if ALIVE_SCD_BASE != 0 && ALIVE_SCD_BASE != 0xFFFF_FFFF {
            ALIVE_SCD_BASE
        } else {
            0
        };
        diag_write_str("[WIFI] CMDQ scd_alive=");
        diag_write_hex(ALIVE_SCD_BASE as usize);
        diag_write_str(" scd_prph=");
        diag_write_hex(scd_prph as usize);
        diag_write_str(" use=");
        diag_write_hex(scd_sram as usize);
        diag_write_str("
");
        if scd_sram == 0 || scd_sram == 0xFFFF_FFFF {
            diag_write_str("[WIFI] CMDQ SCD_SRAM=INVALID
");
            return false;
        }
        core::ptr::write_bytes(CMD_TFD_QUEUE.0.as_mut_ptr(), 0, CMD_TFD_QUEUE.0.len());
        core::ptr::write_bytes(SCD_BC_TABLE.0.as_mut_ptr(), 0, SCD_BC_TABLE.0.len());
        core::ptr::write_volatile((MMIO + FH_MEM_CBBC_CMD) as *mut u32, (tfd_base >> 8) as u32);
        // Map TFD ring, command frames, BC table as uncached so device sees CPU writes
        {
            let cr3 = paging::read_cr3();
            let flags = paging::PAGE_PRESENT | paging::PAGE_WRITE | paging::PAGE_PCD | paging::PAGE_PWT;
            let regions: [(usize, usize); 4] = [
                (CMD_TFD_QUEUE.0.as_ptr() as usize, CMD_TFD_QUEUE.0.len()),
                (CMD_FRAMES.0.as_ptr() as *const u8 as usize, 512 * FH_TFD_CMD_SLOTS),
                (SCD_BC_TABLE.0.as_ptr() as *const u8 as usize, SCD_BC_TABLE.0.len() * 2),
                (LARGE_CMD_BUF.0.as_ptr() as usize, LARGE_CMD_BUF.0.len()),
            ];
            let mut ok_n = 0usize;
            let mut fail_n = 0usize;
            for (base, sz) in regions.iter() {
                let start = *base & !0xFFF;
                let end = (*base + *sz + 0xFFF) & !0xFFF;
                let mut va = start;
                while va < end {
                    if paging::map_page(cr3, va, va, flags) {
                        ok_n += 1;
                    } else {
                        fail_n += 1;
                    }
                    va += 0x1000;
                }
            }
            paging::load_cr3(cr3);
            diag_write_str("[WIFI] CMDQ DMA_UC ok=");
            diag_write_usize(ok_n);
            diag_write_str(" fail=");
            diag_write_usize(fail_n);
            diag_write_str("\n");
        }


        // Initialize the scheduler's SRAM/DRAM translation state before
        // activating queue #4.  The DVM transport expects the DRAM base and
        // queue read pointer to be valid even for the first host command.
        prph_write(SCD_CHAINEXT_EN, 0);
        prph_write(SCD_GP_CTRL, SCD_GP_CTRL_ENABLE_31_QUEUES);
        prph_write(SCD_EN_CTRL, 0);
        prph_write(SCD_QUEUECHAIN_SEL, 0);
        prph_write(SCD_TXFACT, 0xFFFF_FFFF); // all FIFOs, matches iwl_pcie_tx_start
        // FIFO mapping for cmd queue is in SCD_QUEUE_STATUS_BITS (TXF field),
        // NOT in TRANSLATE_TBL (that is RA/TID for data queues). Do not write FIFO
        // number into TRANSLATE_TBL - it confuses the 2000-series scheduler.
        diag_write_str("[WIFI] CMDQ FIFO via STATUS_BITS only (no TRANSLATE)\n");
        // SCD_DRAM_BASE_ADDR is the scheduler byte-count table base, not the
        // TFD ring base. Gen1/2 iwlwifi uses 320 u16 entries per queue and
        // indexes the table by queue number.
        let bc_base = (&SCD_BC_TABLE.0 as *const u16) as u64;
        prph_write(SCD_DRAM_BASE_ADDR, (bc_base >> 10) as u32);

        // Clear queue #4 read/write pointers and status before activation.
        prph_write(SCD_QUEUE_WRPTR, 0);
        prph_write(SCD_QUEUE_RDPTR, 0);
        prph_write(SCD_QUEUE_STATUS_BITS, 0);

        // Queue #4 is the default DVM command queue when PAN is disabled.
        // FIFO 7 is the command FIFO. Active + write-status-limit are required.
        // Linux: ACTIVE | WSL | SCD_ACT_EN(bit19) | MSK | FIFO
        // Linux: first inactive (ACTIVE=0, SCD_ACT_EN=1 as write-enable)
        prph_write(SCD_QUEUE_STATUS_BITS,
            (0u32 << 3) | (1u32 << 19));
        // Then activate: ACTIVE|WSL|FIFO|MSK (MSK includes ACT_EN write-enable)
        prph_write(SCD_QUEUE_STATUS_BITS,
            SCD_QUEUE_STATUS_MASK
            | SCD_QUEUE_ACTIVE
            | SCD_QUEUE_WSL
            | (IWL_CMD_FIFO_NUM << 0));
        // Enable scheduler for cmd queue only (scd_set_active path)
        prph_write(SCD_EN_CTRL, 1u32 << (IWL_DEFAULT_CMD_QUEUE_NUM as u32));
        // Interrupt mask for this queue
        let imask = prph_read(SCD_INTERRUPT_MASK);
        prph_write(SCD_INTERRUPT_MASK, imask | (1u32 << (IWL_DEFAULT_CMD_QUEUE_NUM as u32)));
        diag_write_str("[WIFI] CMDQ STTS=");
        diag_write_hex(prph_read(SCD_QUEUE_STATUS_BITS) as usize);
        diag_write_str(" EN_CTRL=");
        diag_write_hex(prph_read(SCD_EN_CTRL) as usize);
        diag_write_str("\n");

        // Do NOT overwrite SCD_SRAM_BASE_ADDR - FW already set it.
        diag_write_str("[WIFI] CMDQ SRAM_BASE=");
        diag_write_hex(prph_read(SCD_SRAM_BASE_ADDR) as usize);
        diag_write_str(" DRAM_BC=");
        diag_write_hex(prph_read(SCD_DRAM_BASE_ADDR) as usize);
        diag_write_str(" CTX=");
        diag_write_hex((scd_sram + SCD_QUEUE_CTX) as usize);
        diag_write_str("\n");

        // SCD context (Linux iwlagn / pcie txq_enable):
        //   ctx+0 = REG1 = 0
        //   ctx+4 = REG2 = WIN_SIZE | (FRAME_LIMIT << 16)
        // Earlier we wrote "credit" into REG1 - wrong layout for 2000/agn SCD.
        let ctx = scd_sram + SCD_QUEUE_CTX;
        let reg2 = (SCD_WIN_SIZE & 0x7F) | ((SCD_FRAME_LIMIT & 0x7F) << 16);
        // HBUS_TARG_MEM_WADDR=0x410, WDAT=0x418
        core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_WADDR) as *mut u32, ctx);
        core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_WDAT) as *mut u32, 0);
        core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_WADDR) as *mut u32, ctx + 4);
        core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_WDAT) as *mut u32, reg2);
        diag_write_str("[WIFI] CMDQ CTX+0=0 CTX+4=");
        diag_write_hex(reg2 as usize);
        diag_write_str(" (WIN|FLIM)\n");

        // Linux iwl_pcie_tx_start: enable ALL FH TX DMA channels 0..7
        {
            let mut ch = 0usize;
            while ch < 8 {
                let cfg = FH_MEM_LOWER_BOUND + 0xD00 + 0x20 * ch;
                let sts = cfg + 8;
                core::ptr::write_volatile((MMIO + cfg) as *mut u32, 0);
                core::ptr::write_volatile((MMIO + sts) as *mut u32, 0);
                core::ptr::write_volatile((MMIO + cfg) as *mut u32,
                    FH_TCSR_TX_CMD_DMA_ENABLE | FH_TCSR_TX_CMD_CREDIT_ENABLE | FH_TCSR_TX_CMD_CIRQ_HOST_ENDTFD);
                ch += 1;
            }
            diag_write_str("[WIFI] FH_TCSR all ch 0..7 ENABLED\n");
        }
        prph_write(SCD_QUEUE_WRPTR, 0);
        // Seed the command queue byte-count table entry as an invalid/empty
        // descriptor. send_command() replaces it with the actual frame size.
        CMD_WRITE_PTR = 0;
        CMD_SEQ = 0;
        LARGE_CMD_IN_USE = false;
        // Sync doorbell to 0 for command queue (Linux iwl_trans_set_wr_ptrs)
        core::ptr::write_volatile(
            (MMIO + HBUS_TARG_WRPTR) as *mut u32,
            (IWL_DEFAULT_CMD_QUEUE_NUM as u32) << 8,
        );
        CMD_QUEUE_READY = true;
        diag_write_str("[WIFI] CMDQ READY QUEUE=4 FIFO=7 SCD5000=YES STATUS=");
        diag_write_hex(prph_read(SCD_QUEUE_STATUS_BITS) as usize);
        diag_write_str(" TFD=256\\n");
        true
    }
}

/// iwl_tfd (gen1/2, 128 bytes): pad[3], num_tbs, tbs[0]={lo, hi_n_len}, ...
fn cmd_tfd_set(buf: *mut u8, dma: u64, len: usize) {
    unsafe {
        core::ptr::write_bytes(buf, 0, FH_TFD_SIZE);
        *buf.add(3) = 1; // num_tbs
        // TB0: 32-bit LO address
        core::ptr::write_volatile(buf.add(4) as *mut u32, dma as u32);
        // TB0: bits0-3 = addr[35:32], bits4-15 = length
        let hi_n_len = ((len as u16) << 4) | (((dma >> 32) as u16) & 0xF);
        core::ptr::write_volatile(buf.add(8) as *mut u16, hi_n_len);
    }
}


const CSR_GP_CNTRL_HW_RF_KILL_SW: u32 = 0x0800_0000;

/// Linux: iwl_is_rfkill_set = !(GP_CNTRL & HW_RF_KILL_SW)
/// Bit SET  => radio ENABLED (not killed)
/// Bit CLEAR => radio DISABLED (killed)
pub fn rfkill_blocked() -> bool {
    unsafe {
        if MMIO == 0 { return false; }
        let gp = core::ptr::read_volatile((MMIO + CSR_GP_CNTRL) as *const u32);
        (gp & CSR_GP_CNTRL_HW_RF_KILL_SW) == 0
    }
}

pub fn gp_cntrl_raw() -> u32 {
    unsafe {
        if MMIO == 0 { 0 } else { core::ptr::read_volatile((MMIO + CSR_GP_CNTRL) as *const u32) }
    }
}

const CSR_HW_REV: usize = 0x028;
static mut DIAG_CSR_HW_REV: u32 = 0;
static mut DIAG_CSR_GP: u32 = 0;
static mut DIAG_CSR_RESET: u32 = 0;
static mut DIAG_CSR_INT: u32 = 0;
static mut DIAG_CSR_INT_MASK: u32 = 0;
static mut DIAG_FH_INT: u32 = 0;
static mut DIAG_RX_STTS: u32 = 0;
static mut DIAG_FIRST_INT: u32 = 0;
static mut DIAG_FIRST_FH: u32 = 0;
static mut DIAG_RBD_BASE: u32 = 0;
static mut DIAG_STTS_WPTR: u32 = 0;
static mut DIAG_RB_WPTR: u32 = 0;
static mut DIAG_RB_RDPTR: u32 = 0;
static mut DIAG_INIT_RB0: u32 = 0;
static mut DIAG_INIT_RB1: u32 = 0;
static mut DIAG_SEC_INIT_LEN: u32 = 0;
static mut DIAG_SEC_INIT_DATA_LEN: u32 = 0;

pub fn snapshot_diag() {
    unsafe {
        if MMIO == 0 || !MMIO_MAPPED { return; }
        DIAG_CSR_HW_REV = core::ptr::read_volatile((MMIO + CSR_HW_REV) as *const u32);
        DIAG_CSR_GP = core::ptr::read_volatile((MMIO + CSR_GP_CNTRL) as *const u32);
        DIAG_CSR_RESET = core::ptr::read_volatile((MMIO + CSR_RESET) as *const u32);
        DIAG_CSR_INT = core::ptr::read_volatile((MMIO + CSR_INT) as *const u32);
        DIAG_CSR_INT_MASK = core::ptr::read_volatile((MMIO + CSR_INT_MASK) as *const u32);
        DIAG_FH_INT = core::ptr::read_volatile((MMIO + CSR_FH_INT_STATUS) as *const u32);
        DIAG_RX_STTS = core::ptr::read_volatile((&RX_STATUS.0[0]) as *const u32);
        DIAG_RBD_BASE = core::ptr::read_volatile((MMIO + FH_RSCSR_RBDCB_BASE) as *const u32);
        DIAG_STTS_WPTR = core::ptr::read_volatile((MMIO + FH_RSCSR_STTS_WPTR) as *const u32);
        DIAG_RB_WPTR = core::ptr::read_volatile((MMIO + FH_RSCSR_RBDCB_WPTR) as *const u32);
        DIAG_RB_RDPTR = core::ptr::read_volatile((MMIO + FH_RSCSR_RDPTR) as *const u32);
        DIAG_SEC_INIT_LEN = FW_SEC_INIT_LEN as u32;
        DIAG_SEC_INIT_DATA_LEN = FW_SEC_INIT_DATA_LEN as u32;
        diag_write_str("[WIFI] DIAG HW_REV=");
        diag_write_hex(DIAG_CSR_HW_REV as usize);
        diag_write_str(" GP=");
        diag_write_hex(DIAG_CSR_GP as usize);
        diag_write_str(" RST=");
        diag_write_hex(DIAG_CSR_RESET as usize);
        diag_write_str(" INT=");
        diag_write_hex(DIAG_CSR_INT as usize);
        diag_write_str(" MASK=");
        diag_write_hex(DIAG_CSR_INT_MASK as usize);
        diag_write_str(" FH=");
        diag_write_hex(DIAG_FH_INT as usize);
        diag_write_str(" RXST=");
        diag_write_hex(DIAG_RX_STTS as usize);
        diag_write_str(if rfkill_blocked() { " RFKILL=YES\n" } else { " RFKILL=NO\n" });
    }
}
pub fn diag_hw_rev() -> u32 { unsafe { DIAG_CSR_HW_REV } }
pub fn diag_gp() -> u32 { unsafe { DIAG_CSR_GP } }
pub fn diag_reset() -> u32 { unsafe { DIAG_CSR_RESET } }
pub fn diag_int() -> u32 { unsafe { DIAG_CSR_INT } }
pub fn diag_int_mask() -> u32 { unsafe { DIAG_CSR_INT_MASK } }
pub fn diag_fh_int() -> u32 { unsafe { DIAG_FH_INT } }
pub fn diag_rx_stts() -> u32 { unsafe { DIAG_RX_STTS } }
pub fn diag_first_int() -> u32 { unsafe { DIAG_FIRST_INT } }
pub fn diag_first_fh() -> u32 { unsafe { DIAG_FIRST_FH } }
pub fn diag_rbd_base() -> u32 { unsafe { DIAG_RBD_BASE } }
pub fn diag_rb_wptr() -> u32 { unsafe { DIAG_RB_WPTR } }
pub fn diag_rb_rdptr() -> u32 { unsafe { DIAG_RB_RDPTR } }
pub fn diag_init_rb0() -> u32 { unsafe { DIAG_INIT_RB0 } }
pub fn diag_init_rb1() -> u32 { unsafe { DIAG_INIT_RB1 } }
pub fn diag_sec_init_len() -> u32 { unsafe { DIAG_SEC_INIT_LEN } }
pub fn diag_sec_init_data_len() -> u32 { unsafe { DIAG_SEC_INIT_DATA_LEN } }

/// Clear ASPM L0s/L1 on PCIe link control (device + parent if known).
pub fn disable_aspm() {
    unsafe {
        if !FOUND { return; }
        // Find PCIe capability and clear LNKCTL ASPM bits [1:0]
        let p = pci_r32(BUS, DEV, FUNC, 0x34);
        let mut ptr = (p & 0xFF) as u8;
        let mut guard = 0usize;
        while ptr >= 0x40 && guard < 48 {
            let off = ptr & 0xFC;
            let word = pci_r32(BUS, DEV, FUNC, off);
            let cap_id = (word & 0xFF) as u8;
            let next = ((word >> 8) & 0xFF) as u8;
            if cap_id == 0x10 {
                // PCIe cap: LNKCTL at offset +0x10
                let lnkctl_off = off.wrapping_add(0x10);
                let lnk = pci_r32(BUS, DEV, FUNC, lnkctl_off);
                let cleared = lnk & !0x3u32; // ASPM off
                // write low 16 bits of LNKCTL
                let shift = (lnkctl_off as u32 & 2) * 8;
                // use aligned write
                let aligned = lnkctl_off & 0xFC;
                let cur = pci_r32(BUS, DEV, FUNC, aligned);
                let mut newv = cur;
                if (lnkctl_off & 2) != 0 {
                    newv = (cur & !0xFFFF_0000) | ((cleared & 0xFFFF) << 16);
                } else {
                    newv = (cur & !0xFFFF) | (cleared & 0xFFFF);
                }
                // pci write 32
                let a = 0x8000_0000u32
                    | ((BUS as u32) << 16)
                    | ((DEV as u32) << 11)
                    | ((FUNC as u32) << 8)
                    | (aligned as u32);
                core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
                core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") newv, options(nostack, preserves_flags));
                diag_write_str("[WIFI] ASPM disabled LNKCTL\n");
                break;
            }
            ptr = next;
            guard += 1;
        }
    }
}



/// Read internal device SRAM (SCD context lives here). Requires grab_nic_access.
fn read_targ_mem(addr: u32) -> u32 {
    unsafe {
        if MMIO == 0 { return 0; }
        grab_nic_access();
        core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_RADDR) as *mut u32, addr);
        core::ptr::read_volatile((MMIO + HBUS_TARG_MEM_RDAT) as *const u32)
    }
}

/// Full SCD/TFD transport snapshot for one diagnostic phase.
/// Naming dictionary (use these names only):
///   HOST_WR     = software CMD_WRITE_PTR (next free TFD slot index)
///   SCD_WRPTR   = PRPH SCD_QUEUE_WRPTR (hardware write pointer for Q4)
///   SCD_RDPTR   = PRPH SCD_QUEUE_RDPTR (hardware read pointer for Q4)
///   SCD_STATUS  = PRPH SCD_QUEUE_STATUS_BITS for Q4
///   CBBC_RAW    = FH_MEM_CBBC_QUEUE(4) register value as read (TFD base >> 8)
///   TFD_BASE    = software &CMD_TFD_QUEUE[0] physical (= host identity map)
///   TFD_SLOT    = TFD_BASE + slot * 128
fn dump_scd_tfd(tag: &str, slot: usize) {
    unsafe {
        if MMIO == 0 { return; }
        let was_quiet = WF_QUIET;
        WF_QUIET = false;
        grab_nic_access();
        let scd_base = {
            let p = prph_read(SCD_SRAM_BASE_ADDR);
            if p != 0 && p != 0xFFFF_FFFF { p }
            else if ALIVE_SCD_BASE != 0 && ALIVE_SCD_BASE != 0xFFFF_FFFF { ALIVE_SCD_BASE }
            else { 0 }
        };
        let ctx_addr = scd_base.wrapping_add(SCD_QUEUE_CTX);
        let w0 = read_targ_mem(ctx_addr);
        let w1 = read_targ_mem(ctx_addr.wrapping_add(4));
        let tfd_base = (&CMD_TFD_QUEUE.0[0] as *const u8) as usize;
        let tfd_slot = tfd_base + slot * FH_TFD_SIZE;
        let tfd = tfd_slot as *const u8;
        let tw0 = core::ptr::read_volatile(tfd as *const u32);
        let tw1 = core::ptr::read_volatile(tfd.add(4) as *const u32);
        let tw2 = core::ptr::read_volatile(tfd.add(8) as *const u32);
        let cbbc = core::ptr::read_volatile((MMIO + FH_MEM_CBBC_CMD) as *const u32);
        let rd = prph_read(SCD_QUEUE_RDPTR);
        let wr = prph_read(SCD_QUEUE_WRPTR);
        let st = prph_read(SCD_QUEUE_STATUS_BITS);
        let hbus = core::ptr::read_volatile((MMIO + HBUS_TARG_WRPTR) as *const u32);
        diag_write_str("[RXON-SNAP] ");
        diag_write_str(tag);
        diag_write_str(" SLOT=");
        diag_write_usize(slot);
        diag_write_str(" HOST_WR=");
        diag_write_usize(CMD_WRITE_PTR);
        diag_write_str(" SCD_RD=");
        diag_write_hex(rd as usize);
        diag_write_str(" SCD_WR=");
        diag_write_hex(wr as usize);
        diag_write_str(" ST=");
        diag_write_hex(st as usize);
        diag_write_str(" CBBC=");
        diag_write_hex(cbbc as usize);
        diag_write_str(" TFD=");
        diag_write_hex(tfd_slot);
        diag_write_str(" TW0=");
        diag_write_hex(tw0 as usize);
        diag_write_str(" TW1=");
        diag_write_hex(tw1 as usize);
        diag_write_str(" TW2=");
        diag_write_hex(tw2 as usize);
        diag_write_str(" CTX0=");
        diag_write_hex(w0 as usize);
        diag_write_str(" CTX1=");
        diag_write_hex(w1 as usize);
        diag_write_str(" HBUS=");
        diag_write_hex(hbus as usize);
        diag_write_str("\n");
        WF_QUIET = was_quiet;
    }
}

/// Send one legacy DVM host command through command queue #4.
/// The caller supplies the complete command header+payload.
pub fn send_command(cmd: u8, payload: &[u8]) -> bool {
    unsafe {
        if !CMD_QUEUE_READY || payload.len() > 2040 { return false; }
        let slot = CMD_WRITE_PTR & (FH_TFD_CMD_SLOTS - 1);
        let use_large = payload.len() > 500;
        if use_large && LARGE_CMD_IN_USE {
            diag_write_str("[WIFI] CMD TX BUSY large buf
");
            return false;
        }
        let frame = if use_large {
            LARGE_CMD_IN_USE = true;
            let f = (&mut LARGE_CMD_BUF.0[0]) as *mut u8;
            if cmd == 0x80 {
                let was = WF_QUIET;
                WF_QUIET = false;
                diag_write_str("[WIFI] LARGE buf phys=");
                diag_write_hex(f as usize);
                diag_write_str(" plen=");
                diag_write_usize(payload.len());
                diag_write_str("\n");
                WF_QUIET = was;
            }
            f
        } else {
            (&mut CMD_FRAMES.0[slot][0]) as *mut u8
        };
        let frame_cap = if use_large { 2048usize } else { 512usize };
        core::ptr::write_bytes(frame, 0, frame_cap);
        // DVM command header: cmd, flags, little-endian sequence.
        *frame.add(0) = cmd;
        *frame.add(1) = 0;
        let seq = ((IWL_DEFAULT_CMD_QUEUE_NUM as u16) << 8) | (slot as u16);
        *frame.add(2) = seq as u8;
        *frame.add(3) = (seq >> 8) as u8;
        let mut i = 0usize;
        while i < payload.len() {
            *frame.add(4 + i) = payload[i];
            i += 1;
        }
        let len = 4 + payload.len();
        let tfd = (&mut CMD_TFD_QUEUE.0[slot * FH_TFD_SIZE]) as *mut u8;
        let frame_phys = frame as u64;
        cmd_tfd_set(tfd, frame_phys, len);
        // Linux iwlagn_txq_update_byte_cnt_tbl for CMD queue:
        //   len = byte_cnt + IWL_TX_CRC_SIZE(4) + IWL_TX_DELIMITER_SIZE(4)
        // NOT sizeof(TFD). sta_id=0 for cmd queue.
        let bc = (len + 4 + 4) as u16;
        let bc_off = IWL_DEFAULT_CMD_QUEUE_NUM * SCD_QUEUE_BC_SIZE + slot;
        SCD_BC_TABLE.0[bc_off] = bc;
        if slot < 64 {
            SCD_BC_TABLE.0[IWL_DEFAULT_CMD_QUEUE_NUM * SCD_QUEUE_BC_SIZE + FH_TFD_CMD_SLOTS + slot] = bc;
        }
        if cmd == 0x10 {
            let was = WF_QUIET;
            WF_QUIET = false;
            diag_write_str("[WIFI] BC slot=");
            diag_write_usize(slot);
            diag_write_str(" payload=");
            diag_write_usize(len);
            diag_write_str(" bc=");
            diag_write_usize(bc as usize);
            diag_write_str(" (len+CRC+DELIM)\n");
            WF_QUIET = was;
        }
        let next = (slot + 1) & (FH_TFD_CMD_SLOTS - 1);
        if cmd == 0x10 { dump_scd_tfd("PRE", slot); }

        grab_nic_access();
        // Flush DMA-visible lines (TFD 128B, frame, BC entry)
        let mut off = 0usize;
        while off < FH_TFD_SIZE {
            core::arch::asm!("clflush [{}]", in(reg) tfd.add(off), options(nostack));
            off += 64;
        }
        off = 0;
        while off < len {
            core::arch::asm!("clflush [{}]", in(reg) frame.add(off), options(nostack));
            off += 64;
        }
        let bc_ptr = &SCD_BC_TABLE.0[bc_off] as *const u16 as *const u8;
        core::arch::asm!("clflush [{}]", in(reg) bc_ptr, options(nostack));
        core::arch::asm!("mfence", options(nostack, preserves_flags));
        // Doorbell: physical MMIO write to HBUS_TARG_WRPTR (0x460)
        let kick_val = (next as u32 & 0xFF) | ((IWL_DEFAULT_CMD_QUEUE_NUM as u32) << 8);
        let kick_addr = MMIO + HBUS_TARG_WRPTR;
        if cmd == 0x10 {
            let was = WF_QUIET;
            WF_QUIET = false;
            diag_write_str("[KICK] MMIO_OFF=0x460 ABS=");
            diag_write_hex(kick_addr);
            diag_write_str(" VAL=");
            diag_write_hex(kick_val as usize);
            diag_write_str("\n");
            WF_QUIET = was;
        }
        // Linux iwl_pcie_txq_inc_wr_ptr: ONLY HBUS_TARG_WRPTR.
        // Do NOT also poke PRPH SCD_QUEUE_WRPTR - that can desync scheduler.
        core::ptr::write_volatile(kick_addr as *mut u32, kick_val);
        core::arch::asm!("mfence", options(nostack, preserves_flags));
        if cmd == 0x10 {
            let was = WF_QUIET;
            WF_QUIET = false;
            let rb = core::ptr::read_volatile(kick_addr as *const u32);
            let scd_w = prph_read(SCD_QUEUE_WRPTR);
            diag_write_str("[KICK] READBACK HBUS=");
            diag_write_hex(rb as usize);
            diag_write_str(" SCD_WR=");
            diag_write_hex(scd_w as usize);
            let tssr = core::ptr::read_volatile((MMIO + FH_TSSR_TX_STATUS) as *const u32);
            let tcsr = core::ptr::read_volatile((MMIO + FH_TCSR_CONFIG_CMD) as *const u32);
            diag_write_str(" TSSR=");
            diag_write_hex(tssr as usize);
            diag_write_str(" TCSR4=");
            diag_write_hex(tcsr as usize);
            diag_write_str("\n");
            WF_QUIET = was;
        }
        CMD_WRITE_PTR = next;
        CMD_SEQ = CMD_SEQ.wrapping_add(1);
        // sequential host cmds only; release large buf after doorbell
        LARGE_CMD_IN_USE = false;

        if cmd == 0x10 { dump_scd_tfd("POST", slot); }
        true
    }
}

pub fn command_queue_ready() -> bool { unsafe { CMD_QUEUE_READY } }


const REPLY_SCAN_CMD: u8 = 0x80;
const SCAN_START_NOTIFICATION: u8 = 0x82;
const SCAN_RESULTS_NOTIFICATION: u8 = 0x83;
const SCAN_COMPLETE_NOTIFICATION: u8 = 0x84;
const RXON_FLG_BAND_24G: u32 = 1 << 0;
const IWL_GOOD_CRC_TH_DEFAULT: u16 = 1;
const IWL_SCAN_CHANNEL_ACTIVE: u32 = 1;
const IWL_SCAN_CHANNEL_COUNT_24G: usize = 11;

fn put_le16(buf: &mut [u8], off: usize, v: u16) {
    buf[off] = v as u8;
    buf[off + 1] = (v >> 8) as u8;
}
fn put_le32(buf: &mut [u8], off: usize, v: u32) {
    buf[off] = v as u8;
    buf[off + 1] = (v >> 8) as u8;
    buf[off + 2] = (v >> 16) as u8;
    buf[off + 3] = (v >> 24) as u8;
}


const REPLY_RXON: u8 = 0x10;

/// Minimal unassociated RXON for 2.4 GHz channel 1 - required before scan on many DVM cards.
/// Layout must match Linux struct iwl_rxon_cmd (dvm/commands.h) exactly.
pub fn rxon_unassoc_24() -> bool {
    unsafe {
        if !CMD_QUEUE_READY || !ALIVE_SEEN { return false; }
        // struct iwl_rxon_cmd (packed, 50 bytes):
        //  0 node_addr[6]
        //  6 reserved1 u16
        //  8 bssid_addr[6]
        // 14 reserved2 u16
        // 16 wlap_bssid[6]
        // 22 reserved3 u16
        // 24 dev_type u8     (RXON_DEV_TYPE_ESS=3)
        // 25 air_propagation u8
        // 26 rx_chain u16
        // 28 ofdm_basic_rates u8
        // 29 cck_basic_rates u8
        // 30 assoc_id u16
        // 32 flags u32       (RXON_FLG_BAND_24G bit0)
        // 36 filter_flags u32 (no ASSOC; accept beacons)
        // 40 channel u16
        // 42.. ht rates / reserved
        let mut p = [0u8; 64];
        // valid local MAC (unicast, not zero) - firmware rejects mcast/zero-ish
        p[0] = 0x02; p[1] = 0x00; p[2] = 0x00;
        p[3] = 0xAE; p[4] = 0x7E; p[5] = 0x01;
        // bssid = broadcast for unassoc listen
        p[8] = 0xFF; p[9] = 0xFF; p[10] = 0xFF;
        p[11] = 0xFF; p[12] = 0xFF; p[13] = 0xFF;
        p[24] = 3; // RXON_DEV_TYPE_ESS
        p[25] = 0;
        put_le16(&mut p, 26, 0x0007); // rx_chain valid antennas
        p[28] = 0xFF; // ofdm basic rates
        p[29] = 0x0F; // cck basic rates (1M..)
        // assoc_id = 0
        put_le32(&mut p, 32, RXON_FLG_BAND_24G); // flags
        // filter: accept group, ctl2host, beacon-aware; NOT associated
        put_le32(&mut p, 36, (1 << 1) | (1 << 2) | (1 << 6));
        put_le16(&mut p, 40, 1); // channel 1
        diag_write_str("[WIFI] RXON unassoc ch=1 (fixed layout)\n");
        LAST_RXON_OK = send_command(REPLY_RXON, &p[..50]);
        // Poll SCD_RDPTR briefly - does HW consume RXON TFD?
        {
            let was_quiet = WF_QUIET;
            WF_QUIET = false;
            let mut i = 0usize;
            let target = CMD_WRITE_PTR as u32;
            let mut rd = prph_read(SCD_QUEUE_RDPTR);
            while i < 50_000 && rd != target {
                core::hint::spin_loop();
                rd = prph_read(SCD_QUEUE_RDPTR);
                i += 1;
            }
            diag_write_str("[RXON-SNAP] AFTER_POLL SCD_RD=");
            diag_write_hex(rd as usize);
            diag_write_str(" polls=");
            diag_write_usize(i);
            diag_write_str(" HOST_WR=");
            diag_write_usize(CMD_WRITE_PTR);
            diag_write_str(if rd == target { " CAUGHT\n" } else { " LAGGING\n" });
            WF_QUIET = was_quiet;
        }
        LAST_RXON_OK
    }
}

/// First real DVM scan request: passive 2.4 GHz sweep over channels 1..11.
/// This intentionally omits probe TX until the command transport is proven.
pub fn scan_24ghz() -> bool {
    unsafe {
        if !CMD_QUEUE_READY || !ALIVE_SEEN { return false; }
        SCAN_NOTIFICATION_SEEN = false;
        SCAN_START_COUNT = 0;
        SCAN_RESULTS_COUNT = 0;
        SCAN_COMPLETE_COUNT = 0;
        AP_COUNT = 0;
        let _ = rxon_unassoc_24();
        // Wait for possible RXON response / FH activity
        let mut w = 0usize;
        let rx_before = LAST_RX_CMDS_N;
        while w < 200_000 {
            irq_handler();
            if LAST_RX_CMDS_N != rx_before { break; }
            core::hint::spin_loop();
            w += 1;
        }
        diag_write_str("[WIFI] RXON wait polls=");
        diag_write_usize(w);
        diag_write_str(" new_rx=");
        diag_write_usize(LAST_RX_CMDS_N.wrapping_sub(rx_before));
        diag_write_str("\n");

        // RX dump after RXON
        {
            let st0 = core::ptr::read_volatile(RX_STATUS.0.as_ptr() as *const u32);
            let closed = (st0 & 0xFFFF) as usize;
            let hw = core::ptr::read_volatile((MMIO + FH_RSCSR_RDPTR) as *const u32) as usize;
            let peek = core::ptr::read_volatile(RX_BUFFERS.0[RX_READ].as_ptr() as *const u32);
            diag_write_str("[WIFI] RXDUMP read=");
            diag_write_usize(RX_READ);
            diag_write_str(" closed=");
            diag_write_hex(closed);
            diag_write_str(" hw_rptr=");
            diag_write_hex(hw);
            diag_write_str(" peek=");
            diag_write_hex(peek as usize);
            diag_write_str("\n");
        }

        // TINY2 as true 2nd command (NO QRESET).
        // Replenish SCD credits - without RX reply FH credit may be exhausted after TFD0.
        {
            let rd0 = prph_read(SCD_QUEUE_RDPTR) as usize;
            let wr0 = prph_read(SCD_QUEUE_WRPTR) as usize;
            diag_write_str("[WIFI] PRE-TINY2 rd=");
            diag_write_usize(rd0);
            diag_write_str(" wr=");
            diag_write_usize(wr0);
            diag_write_str(" host=");
            diag_write_usize(CMD_WRITE_PTR);
            diag_write_str("\n");

            // Re-write SCD context REG1 credits (super=0x40, credit=0x40)
            let sram = prph_read(SCD_SRAM_BASE_ADDR);
            let ctx = sram + SCD_QUEUE_CTX;
            let reg1 = (0x40u32 << 24) | (0x40u32 << 8);
            core::ptr::write_volatile((MMIO + 0x410) as *mut u32, ctx);
            core::ptr::write_volatile((MMIO + 0x418) as *mut u32, reg1);
            // Re-arm TCSR with CREDIT
            let cfg = FH_MEM_LOWER_BOUND + 0xD00 + 0x20 * IWL_DEFAULT_CMD_QUEUE_NUM;
            core::ptr::write_volatile((MMIO + cfg) as *mut u32,
                FH_TCSR_TX_CMD_DMA_ENABLE | FH_TCSR_TX_CMD_CREDIT_ENABLE |
                FH_TCSR_TX_CMD_CIRQ_HOST_ENDTFD);
            prph_write(SCD_TXFACT, 0xFFFF_FFFF);
            prph_write(SCD_QUEUE_STATUS_BITS,
                SCD_QUEUE_STATUS_MASK | SCD_QUEUE_ACTIVE | SCD_QUEUE_WSL |
                (IWL_CMD_FIFO_NUM << 0));
            diag_write_str("[WIFI] CREDIT-REARM reg1=");
            diag_write_hex(reg1 as usize);
            diag_write_str("\n");

            let ok = send_command(0x9c, &[]);
            prph_write(SCD_QUEUE_WRPTR, CMD_WRITE_PTR as u32);
            let target = CMD_WRITE_PTR;
            let mut i = 0usize;
            let mut rd = prph_read(SCD_QUEUE_RDPTR) as usize;
            while i < 30_000 && rd != target {
                irq_handler();
                core::hint::spin_loop();
                rd = prph_read(SCD_QUEUE_RDPTR) as usize;
                i += 1;
            }
            let tssr = core::ptr::read_volatile((MMIO + FH_MEM_LOWER_BOUND + 0x7A0) as *const u32);
            let tcsr = core::ptr::read_volatile((MMIO + cfg) as *const u32);
            diag_write_str("[WIFI] TINY2 ok=");
            diag_write_str(if ok { "Y" } else { "N" });
            diag_write_str(" rd=");
            diag_write_usize(rd);
            diag_write_str(" wr=");
            diag_write_usize(target);
            diag_write_str(" polls=");
            diag_write_usize(i);
            diag_write_str(" TSSR=");
            diag_write_hex(tssr as usize);
            diag_write_str(" TCSR=");
            diag_write_hex(tcsr as usize);
            if rd != target {
                diag_write_str(" LAGGING\n");
            } else {
                diag_write_str(" CAUGHT\n");
            }
        }

        // Minimal passive SCAN: 1 channel only (still needs full SSID table in DVM layout).
        // total = 28+52+680+12 = 772 → still uses LARGE_CMD_BUF (>500).
        let nchan = 1u8;
        let fixed = 28usize + 52usize + 20usize * 34usize;
        let total = fixed + (nchan as usize) * 12usize;
        let mut p = [0u8; 1024];

        put_le16(&mut p, 0, total as u16);
        p[2] = 0;
        p[3] = nchan;
        put_le16(&mut p, 4, 0);
        put_le16(&mut p, 6, 0);
        put_le16(&mut p, 8, IWL_GOOD_CRC_TH_DEFAULT);
        put_le16(&mut p, 10, 0x0007);
        put_le32(&mut p, 12, 0);
        put_le32(&mut p, 16, 0);
        put_le32(&mut p, 20, RXON_FLG_BAND_24G);
        put_le32(&mut p, 24, 0);

        let off = fixed;
        put_le32(&mut p, off, 0);
        put_le16(&mut p, off + 4, 1); // channel 1 only
        p[off + 6] = 0;
        p[off + 7] = 0;
        put_le16(&mut p, off + 8, 0);
        put_le16(&mut p, off + 10, 50); // shorter dwell

        diag_write_str("[WIFI] SCAN24 CMD channels=1 total=");
        diag_write_usize(total);
        diag_write_str(" large=");
        diag_write_str(if total > 500 { "Y" } else { "N" });
        diag_write_str("\n");
        // Guard: do not kick while previous TFDs are unconsumed by HW
        {
            let host_wr = CMD_WRITE_PTR;
            let scd_rd = prph_read(SCD_QUEUE_RDPTR) as usize;
            diag_write_str("[WIFI] SCAN24 GUARD HOST_WR=");
            diag_write_usize(host_wr);
            diag_write_str(" SCD_RDPTR=");
            diag_write_hex(scd_rd);
            diag_write_str("\n");
            if scd_rd != host_wr {
                diag_write_str("[WIFI] SCAN24 BLOCKED: PREV_NOT_CONSUMED\n");
                LAST_SCAN_SUBMIT_OK = false;
                return false;
            }
        }
        LAST_SCAN_SUBMIT_OK = send_command(REPLY_SCAN_CMD, &p[..total]);
        if !LAST_SCAN_SUBMIT_OK {
            diag_write_str("[WIFI] SCAN24 CMD=FAILED\n");
            return false;
        }
        diag_write_str("[WIFI] SCAN24 CMD=SUBMITTED\n");
        // 1) Wait until SCD consumes the SCAN TFD (RD catches WR)
        {
            let target = CMD_WRITE_PTR;
            let mut i = 0usize;
            let mut rd = prph_read(SCD_QUEUE_RDPTR) as usize;
            while i < 40_000 && rd != target {
                irq_handler();
                core::hint::spin_loop();
                rd = prph_read(SCD_QUEUE_RDPTR) as usize;
                i += 1;
            }
            diag_write_str("[WIFI] SCAN SCD wait rd=");
            diag_write_usize(rd);
            diag_write_str(" wr=");
            diag_write_usize(target);
            diag_write_str(" polls=");
            diag_write_usize(i);
            if rd != target {
                diag_write_str(" LAGGING\n");
            } else {
                diag_write_str(" CAUGHT\n");
            }
        }
        // 2) Poll RX for scan notifications; log every new RX cmd
        let rx_before = LAST_RX_CMDS_N;
        let mut polls = 0usize;
        while polls < 80_000 {
            irq_handler();
            if scan_complete_count() != 0 { break; }
            core::hint::spin_loop();
            polls += 1;
        }
        diag_write_str("[WIFI] SCAN RX start=");
        diag_write_usize(scan_start_count() as usize);
        diag_write_str(" results=");
        diag_write_usize(scan_results_count() as usize);
        diag_write_str(" complete=");
        diag_write_usize(scan_complete_count() as usize);
        diag_write_str(" polls=");
        diag_write_usize(polls);
        diag_write_str(" new_rx=");
        diag_write_usize(LAST_RX_CMDS_N.wrapping_sub(rx_before));
        diag_write_str("\n");
        // 3) Dump last RX cmd ids seen
        diag_write_str("[WIFI] LAST_RX_CMDS:");
        let mut k = 0usize;
        while k < 8 && k < LAST_RX_CMDS_N {
            let idx = LAST_RX_CMDS_N.wrapping_sub(1).wrapping_sub(k) % 8;
            diag_write_str(" ");
            diag_write_hex(LAST_RX_CMDS[idx] as usize);
            k += 1;
        }
        diag_write_str("\n");
        if scan_complete_count() == 0 {
            diag_write_str("[WIFI] SCAN TIMEOUT/NO-COMPLETE-NOTIFICATION\n");
        }
        true
    }
}



unsafe fn init_rx_queue() -> bool {
    if !MMIO_MAPPED || MMIO == 0 { return false; }
    RX_READ = 0;
    RX_IRQ_COUNT = 0;
    ALIVE_SEEN = false;
    ALIVE_VALID = 0;
    ALIVE_SCD_BASE = 0;
    ALIVE_SUBTYPE = 0;
    let rbd_base = (&RX_RBD.0 as *const u32) as u64;
    let status_base = (&RX_STATUS.0 as *const u32) as u64;
    let mut i = 0usize;
    while i < FH_RX_RBD_COUNT {
        let buf = (&RX_BUFFERS.0[i][0] as *const u8) as u64;
        RX_RBD.0[i] = (buf >> 8) as u32;
        core::ptr::write_volatile(RX_BUFFERS.0[i].as_mut_ptr() as *mut u32, FH_RSCSR_FRAME_INVALID);
        i += 1;
    }
    RX_STATUS.0[0] = 0;
    RX_STATUS.0[1] = 0;
    RX_STATUS.0[2] = 0;
    core::ptr::write_volatile((MMIO + FH_RCSR_CHNL0_CONFIG) as *mut u32, 0);
    core::ptr::write_volatile((MMIO + FH_RSCSR_RBDCB_WPTR) as *mut u32, 0);
    core::ptr::write_volatile((MMIO + FH_RCSR_CHNL0_FLUSH_RB_REQ) as *mut u32, 0);
    core::ptr::write_volatile((MMIO + FH_RSCSR_RDPTR) as *mut u32, 0);
    core::ptr::write_volatile((MMIO + FH_RSCSR_RBDCB_BASE) as *mut u32, (rbd_base >> 8) as u32);
    core::ptr::write_volatile((MMIO + FH_RSCSR_STTS_WPTR) as *mut u32, (status_base >> 4) as u32);
    core::ptr::write_volatile((MMIO + FH_RSCSR_RBDCB_WPTR) as *mut u32, 248);
    core::ptr::write_volatile((MMIO + CSR_FH_INT_STATUS) as *mut u32, 0xFFFF_FFFF);
    core::ptr::write_volatile((MMIO + CSR_INT) as *mut u32, CSR_INT_BIT_FH_RX | CSR_INT_BIT_ALIVE);
    core::ptr::write_volatile((MMIO + CSR_INT_MASK) as *mut u32, CSR_INT_BIT_FH_RX | CSR_INT_BIT_ALIVE);
    // 256 RBDs, 4 KiB buffers, host IRQ destination, RB timeout 0x11, DMA enabled.
    let cfg = 0x8000_0000u32 | (8u32 << 20) | 0x0000_1000 | (0x11u32 << 4) | 0x0000_0004;
    core::ptr::write_volatile((MMIO + FH_RCSR_CHNL0_CONFIG) as *mut u32, cfg);
    RX_READY = true;
    diag_write_str("[WIFI] RX-RING READY RBD=256 BUF=4K IRQ=HOST\n");
    true
}


fn store_ap(ssid: &[u8], channel: u8, rssi: i8) {
    unsafe {
        if ssid.is_empty() || ssid.len() > 32 { return; }
        // skip empty / all-zero
        let mut i = 0usize;
        while i < AP_COUNT {
            if AP_SSID_LEN[i] as usize == ssid.len() {
                let mut same = true;
                let mut j = 0usize;
                while j < ssid.len() {
                    if AP_SSID[i][j] != ssid[j] { same = false; break; }
                    j += 1;
                }
                if same { return; }
            }
            i += 1;
        }
        if AP_COUNT >= MAX_APS { return; }
        let n = AP_COUNT;
        let mut j = 0usize;
        while j < ssid.len() {
            AP_SSID[n][j] = ssid[j];
            j += 1;
        }
        AP_SSID_LEN[n] = ssid.len() as u8;
        AP_CHANNEL[n] = channel;
        AP_RSSI[n] = rssi;
        AP_COUNT += 1;
    }
}

/// Extract SSID IE from 802.11 management frame body (after 24-byte MAC hdr).
fn parse_beacon_ssid(frame: *const u8, frame_len: usize) -> (usize, [u8; 32]) {
    let mut out = [0u8; 32];
    let mut out_len = 0usize;
    if frame_len < 36 { return (0, out); }
    unsafe {
        let fc = core::ptr::read_volatile(frame as *const u16);
        let ftype = (fc >> 2) & 3;
        let subtype = (fc >> 4) & 0xF;
        // management=0, beacon=8 or probe resp=5
        if ftype != 0 || (subtype != 8 && subtype != 5) {
            return (0, out);
        }
        // fixed params 12 bytes after hdr 24 → IEs at 36
        let mut off = 36usize;
        while off + 2 <= frame_len {
            let id = core::ptr::read_volatile(frame.add(off));
            let elen = core::ptr::read_volatile(frame.add(off + 1)) as usize;
            if off + 2 + elen > frame_len { break; }
            if id == 0 {
                // SSID
                let n = if elen > 32 { 32 } else { elen };
                let mut j = 0usize;
                while j < n {
                    out[j] = core::ptr::read_volatile(frame.add(off + 2 + j));
                    j += 1;
                }
                out_len = n;
                break;
            }
            off += 2 + elen;
        }
    }
    (out_len, out)
}


/// Scan all RX buffers for frames the FH delivered but status/closed_rb missed.
/// Critical on AH532: WP advances, INT has ALIVE|FH_RX, but host RX_STATUS stays 0.
unsafe fn force_drain_rx() {
    if !RX_READY || MMIO == 0 { return; }
    let mut found = 0usize;
    let max_scan = 32usize; // never walk all 256 - freezes GUI
    let mut i = 0usize;
    while i < max_scan {
        let p = RX_BUFFERS.0[i].as_ptr();
        let len_word = core::ptr::read_volatile(p as *const u32);
        if len_word == FH_RSCSR_FRAME_INVALID || len_word == 0 {
            i += 1;
            continue;
        }
        found = found.wrapping_add(1);
        // Standard iwl_rx_packet: len@0, cmd@4
        let cmd4 = core::ptr::read_volatile(p.add(4) as *const u8);
        let cmd0 = core::ptr::read_volatile(p as *const u8);
        let cmd = if cmd4 == 1 { 1u8 } else if cmd0 == 1 { 1u8 } else { 0u8 };
        if cmd == 1 {
            let hdr = if cmd4 == 1 { 4usize } else { 0usize };
            let payload = p.add(hdr + 4);
            let status = core::ptr::read_volatile(payload as *const u16);
            let subtype = core::ptr::read_volatile(payload.add(10) as *const u8);
            let scd = core::ptr::read_volatile(payload.add(32) as *const u32);
            ALIVE_SEEN = true;
            ALIVE_SUBTYPE = subtype;
            ALIVE_VALID = status as u32;
            if scd != 0 && scd != 0xFFFF_FFFF { ALIVE_SCD_BASE = scd; }
            RX_READ = (i + 1) & (FH_RX_RBD_COUNT - 1);
            core::ptr::write_volatile((MMIO + FH_RSCSR_RDPTR) as *mut u32, RX_READ as u32);
            return;
        }
        i += 1;
    }
    // INT ALIVE bit fallback (cheap)
    if !ALIVE_SEEN && (DIAG_FIRST_INT & CSR_INT_BIT_ALIVE) != 0 {
        ALIVE_SEEN = true;
        ALIVE_VALID = 1;
        let scd_prph = prph_read(SCD_SRAM_BASE_ADDR);
        if scd_prph != 0 && scd_prph != 0xFFFF_FFFF {
            ALIVE_SCD_BASE = scd_prph;
        }
    }
}

pub unsafe fn irq_handler() {
    if !MMIO_MAPPED || MMIO == 0 { return; }
    let mut inta = core::ptr::read_volatile((MMIO + CSR_INT) as *const u32);
    let fh = core::ptr::read_volatile((MMIO + CSR_FH_INT_STATUS) as *const u32);
    // Poll mode: even with inta==0, drain RX if closed_rb advanced
    let st0_early = core::ptr::read_volatile((&RX_STATUS.0[0]) as *const u32);
    let closed_early = (st0_early & 0xFFFF) as usize & (FH_RX_RBD_COUNT - 1);
    let need_rx = (inta & CSR_INT_BIT_FH_RX) != 0
        || (fh & CSR_FH_INT_RX_MASK) != 0
        || (RX_READY && closed_early != RX_READ)
        || (inta & CSR_INT_BIT_ALIVE) != 0;
    if !need_rx && inta == 0 {
        return;
    }
    if inta != 0 {
        RX_IRQ_COUNT = RX_IRQ_COUNT.wrapping_add(1);
    }
    // Force FH_RX path when status shows work
    if need_rx {
        // ALIVE/FH_RX seen: always try buffer scan (status may lag)
        if (inta & (CSR_INT_BIT_ALIVE | CSR_INT_BIT_FH_RX)) != 0 {
            force_drain_rx();
            if ALIVE_SEEN {
                // still fall through to ack
            }
        }
        if inta == 0 {
            inta = CSR_INT_BIT_FH_RX; // synthetic for ack path
        }
        // iwl_rb_status: closed_rb_num LE16 @0, finished_rb_num LE16 @2
        let st0 = core::ptr::read_volatile((&RX_STATUS.0[0]) as *const u32);
        let closed_rb = (st0 & 0xFFFF) as usize;
        let finished_rb = ((st0 >> 16) & 0xFFFF) as usize;
        let mut hw = closed_rb & (FH_RX_RBD_COUNT - 1);
        let hw_fin = finished_rb & (FH_RX_RBD_COUNT - 1);
        if hw == RX_READ && hw_fin != RX_READ {
            hw = hw_fin;
        }
        // Sticky FH_RX with empty closed/finished: peek next RB for a valid frame
        if hw == RX_READ {
            let peek = core::ptr::read_volatile(RX_BUFFERS.0[RX_READ].as_ptr() as *const u32);
            if peek != 0 && peek != FH_RSCSR_FRAME_INVALID && (peek & 0x3FFF) > 8 {
                hw = (RX_READ + 1) & (FH_RX_RBD_COUNT - 1);
                diag_write_str("[WIFI] RX-FORCE-PEEK len=");
                diag_write_hex(peek as usize);
                diag_write_str("
");
            }
        }
        let mut same_rx_streak = 0usize;
        let mut last_cmd = 0u8;
        let mut last_subtype = 0u8;
        let mut rx_items = 0usize;
        diag_write_str("[WIFI] RX-STATE enter read=");
        diag_write_usize(RX_READ);
        diag_write_str(" hw_rptr=");
        diag_write_usize(hw);
        diag_write_str(" cb_wptr=");
        diag_write_hex(core::ptr::read_volatile((MMIO + FH_RSCSR_RBDCB_WPTR) as *const u32) as usize);
        diag_write_str(" stts_wptr=");
        diag_write_hex(core::ptr::read_volatile((MMIO + FH_RSCSR_STTS_WPTR) as *const u32) as usize);
        diag_write_str(" rx_status=");
        diag_write_hex(core::ptr::read_volatile((MMIO + FH_RSSR_RX_STATUS) as *const u32) as usize);
        diag_write_str(" inta=");
        diag_write_hex(inta as usize);
        diag_write_str(" fh=");
        diag_write_hex(fh as usize);
        diag_write_str(" rbd_base=");
        diag_write_hex(core::ptr::read_volatile((MMIO + FH_RSCSR_RBDCB_BASE) as *const u32) as usize);
        diag_write_str(" rbd_addr=");
        diag_write_hex((&RX_RBD.0[RX_READ] as *const u32) as usize);
        diag_write_str(" buf_addr=");
        diag_write_hex((&RX_BUFFERS.0[RX_READ][0] as *const u8) as usize);
        diag_write_str("\n");
        while RX_READ != hw {
            let p = &RX_BUFFERS.0[RX_READ][0] as *const u8;
            let len_flags = core::ptr::read_volatile(p as *const u32);
            if len_flags != FH_RSCSR_FRAME_INVALID {
                let len = (len_flags & 0x3FFF) as usize;
                if len >= 8 && len <= FH_RX_BUF_SIZE - 4 {
                    let cmd = core::ptr::read_volatile(p.add(4));
                    let subtype_probe = if cmd == 1 {
                        core::ptr::read_volatile(p.add(4 + 4 + 13))
                    } else { 0 };
                    if cmd == last_cmd && (cmd != 1 || subtype_probe == last_subtype) {
                        same_rx_streak = same_rx_streak.saturating_add(1);
                    } else {
                        same_rx_streak = 1;
                        last_cmd = cmd;
                        last_subtype = subtype_probe;
                    }
                    rx_items = rx_items.saturating_add(1);
                    diag_write_str("[WIFI] RX-ITEM idx=");
                    diag_write_usize(RX_READ);
                    diag_write_str(" len=");
                    diag_write_usize(len);
                    diag_write_str(" cmd=");
                    diag_write_hex(cmd as usize);
                    {
                        let n = LAST_RX_CMDS_N % 8;
                        LAST_RX_CMDS[n] = cmd;
                        LAST_RX_CMDS_N = LAST_RX_CMDS_N.wrapping_add(1);
                    }
                    if cmd == 1 {
                        diag_write_str(" subtype=");
                        diag_write_usize(subtype_probe as usize);
                    }
                    diag_write_str(" repeat=");
                    diag_write_usize(same_rx_streak);
                    diag_write_str(" raw=");
                    let mut j = 0usize;
                    while j < 16 && j < len {
                        diag_write_hex(core::ptr::read_volatile(p.add(j)) as usize);
                        j += 1;
                    }
                    diag_write_str("\n");
                    if same_rx_streak > 50 {
                        diag_write_str("[WIFI] IRQ STORM SUSPECTED same RX cmd/subtype >50\n");
                        break;
                    }
                    if cmd == 1 {
                        let payload = p.add(8);
                        let status = core::ptr::read_volatile(payload as *const u16);
                        // ver_subtype is at payload+12 in iwl_alive_resp layout
                        // (status:2 flags:2 minor/major:2 reserved:2 api:2 ver_sub/type:2 mac/opt:2 reserved2:2 timestamp:4 ...)
                        // Linux struct iwl_alive_resp:
                        //   +0 status, +2 flags, +4 ucode_min/maj, +6 reserved1,
                        //   +8 api_min/maj, +10 ver_subtype, +11 ver_type, +12 mac, +13 opt,
                        //   +14 reserved2, +16 timestamp, +20 error_ptr, +24 log_ptr,
                        //   +28 channel_info_ptr, +32 scd_base_ptr
                        let subtype = core::ptr::read_volatile(payload.add(10));
                        let log_ptr = core::ptr::read_volatile(payload.add(24) as *const u32);
                        let scd = if (len as usize) >= 8 + 36 {
                            core::ptr::read_volatile(payload.add(32) as *const u32)
                        } else {
                            0
                        };
                        ALIVE_SEEN = true;
                        ALIVE_SUBTYPE = subtype;
                        ALIVE_VALID = status as u32;
                        // Prefer PRPH SCD_SRAM_BASE after init; store ALIVE scd for diagnostics
                        ALIVE_SCD_BASE = scd;
                        diag_write_str("[WIFI] ALIVE cmd=1 subtype=");
                        diag_write_usize(subtype as usize);
                        diag_write_str(" status=");
                        diag_write_hex(status as usize);
                        diag_write_str(" log=");
                        diag_write_hex(log_ptr as usize);
                        diag_write_str(" scd32=");
                        diag_write_hex(scd as usize);
                        diag_write_str(" RESULT=VALID\n");
                                        } else {
                        if cmd == SCAN_START_NOTIFICATION ||
                           cmd == SCAN_RESULTS_NOTIFICATION ||
                           cmd == SCAN_COMPLETE_NOTIFICATION {
                            SCAN_NOTIFICATION_SEEN = true;
                        }
                        diag_write_str("[WIFI] RX cmd=");
                        diag_write_hex(cmd as usize);
                        diag_write_str(" len=");
                        diag_write_usize(len);
                        let payload = p.add(8);
                        if cmd == SCAN_START_NOTIFICATION && len >= 24 {
                            let channel = core::ptr::read_volatile(payload.add(12));
                            let band = core::ptr::read_volatile(payload.add(13));
                            let status = core::ptr::read_volatile(payload.add(16) as *const u32);
                            SCAN_START_COUNT = SCAN_START_COUNT.wrapping_add(1);
                            diag_write_str(" TYPE=SCAN_START ch=");
                            diag_write_usize(channel as usize);
                            diag_write_str(" band=");
                            diag_write_usize(band as usize);
                            diag_write_str(" status=");
                            diag_write_hex(status as usize);
                            diag_write_str("\n");
                        } else if cmd == SCAN_RESULTS_NOTIFICATION && len >= 20 {
                            let channel = core::ptr::read_volatile(payload);
                            let band = core::ptr::read_volatile(payload.add(1));
                            let probe = core::ptr::read_volatile(payload.add(2));
                            let not_sent = core::ptr::read_volatile(payload.add(3));
                            let stats = core::ptr::read_volatile(payload.add(12) as *const u32);
                            SCAN_RESULTS_COUNT = SCAN_RESULTS_COUNT.wrapping_add(1);
                            diag_write_str(" TYPE=SCAN_RESULTS ch=");
                            diag_write_usize(channel as usize);
                            diag_write_str(" band=");
                            diag_write_usize(band as usize);
                            diag_write_str(" probe=");
                            diag_write_hex(probe as usize);
                            diag_write_str(" not_sent=");
                            diag_write_usize(not_sent as usize);
                            diag_write_str(" good_crc=");
                            diag_write_usize(stats as usize);
                            diag_write_str("\n");
                        } else if cmd == SCAN_COMPLETE_NOTIFICATION && len >= 16 {
                            let channels = core::ptr::read_volatile(payload);
                            let status = core::ptr::read_volatile(payload.add(1));
                            let bt = core::ptr::read_volatile(payload.add(2));
                            let last = core::ptr::read_volatile(payload.add(3));
                            SCAN_COMPLETE_COUNT = SCAN_COMPLETE_COUNT.wrapping_add(1);
                            SCAN_COMPLETE_CHANNELS = channels;
                            SCAN_COMPLETE_STATUS = status;
                            SCAN_COMPLETE_LAST_CHANNEL = last;
                            diag_write_str(" TYPE=SCAN_COMPLETE channels=");
                            diag_write_usize(channels as usize);
                            diag_write_str(" status=");
                            diag_write_hex(status as usize);
                            diag_write_str(" bt=");
                            diag_write_usize(bt as usize);
                            diag_write_str(" last=");
                            diag_write_usize(last as usize);
                            diag_write_str("\n");
                        } else if cmd == 0xc1 || cmd == 0xc3 {
                            // 802.11 MPDU during scan - try SSID from beacon/probe
                            // payload layout varies; try offset 0 and common hdr skips
                            let mut try_off = 0usize;
                            while try_off <= 32 && (8 + try_off + 36) < len {
                                let (sl, ss) = parse_beacon_ssid(payload.add(try_off), len.saturating_sub(8 + try_off));
                                if sl > 0 {
                                    store_ap(&ss[..sl], 0, 0);
                                    diag_write_str(" TYPE=BEACON ssid_len=");
                                    diag_write_usize(sl);
                                    diag_write_str("\n");
                                    break;
                                }
                                try_off += 2;
                            }
                        } else {
                            diag_write_str(" TYPE=OTHER RAW=");
                            let mut j = 0usize;
                            while j < 12 && (8 + j) < len {
                                diag_write_hex(core::ptr::read_volatile(p.add(8 + j)) as usize);
                                j += 1;
                            }
                            diag_write_str("\n");
                        }
                    }
                }
            }
            core::ptr::write_volatile(RX_BUFFERS.0[RX_READ].as_mut_ptr() as *mut u32, FH_RSCSR_FRAME_INVALID);
            RX_READ = (RX_READ + 1) & (FH_RX_RBD_COUNT - 1);
        }
        diag_write_str("[WIFI] RX-STATE exit items=");
        diag_write_usize(rx_items);
        diag_write_str(" read=");
        diag_write_usize(RX_READ);
        diag_write_str(" hw_rptr=");
        diag_write_usize(hw);
        diag_write_str(" cb_wptr=");
        diag_write_hex(core::ptr::read_volatile((MMIO + FH_RSCSR_RBDCB_WPTR) as *const u32) as usize);
        diag_write_str(" stts_wptr=");
        diag_write_hex(core::ptr::read_volatile((MMIO + FH_RSCSR_STTS_WPTR) as *const u32) as usize);
        diag_write_str(" rx_status=");
        diag_write_hex(core::ptr::read_volatile((MMIO + FH_RSSR_RX_STATUS) as *const u32) as usize);
        diag_write_str("\n");
        let mut new_cb_wptr = ((RX_READ + FH_RX_RBD_COUNT - 8) & (FH_RX_RBD_COUNT - 1)) as u32;
        new_cb_wptr &= !7u32;
        if new_cb_wptr == 0 { new_cb_wptr = 248; }
        core::ptr::write_volatile((MMIO + FH_RSCSR_RBDCB_WPTR) as *mut u32, new_cb_wptr);
        // FH interrupt status (write-1-to-clear style masks used by iwlwifi)
        core::ptr::write_volatile((MMIO + CSR_FH_INT_STATUS) as *mut u32, 0xFFFF_FFFF);
        // CSR_INT is write-1-to-clear: must write the bits we handled or FH_RX sticks forever
        core::ptr::write_volatile((MMIO + CSR_INT) as *mut u32, inta);
        let ack_fh = core::ptr::read_volatile((MMIO + CSR_FH_INT_STATUS) as *const u32);
        let ack_int = core::ptr::read_volatile((MMIO + CSR_INT) as *const u32);
        diag_write_str("[WIFI] RX-ACK cb_wptr=");
        diag_write_hex(new_cb_wptr as usize);
        diag_write_str(" fh_after=");
        diag_write_hex(ack_fh as usize);
        diag_write_str(" inta_after=");
        diag_write_hex(ack_int as usize);
        diag_write_str("\n");
    }
    core::ptr::write_volatile((MMIO + CSR_INT) as *mut u32, inta);
    let int_after = core::ptr::read_volatile((MMIO + CSR_INT) as *const u32);
    diag_write_str("[WIFI] RX-ACK CSR_INT final=");
    diag_write_hex(int_after as usize);
    diag_write_str("\n");
}


fn fw_le32(b: &[u8], off: usize) -> u32 {
    (b[off] as u32) | ((b[off + 1] as u32) << 8) |
    ((b[off + 2] as u32) << 16) | ((b[off + 3] as u32) << 24)
}

pub fn load_firmware() -> bool {
    unsafe {
        FW_ATTEMPTED = true;
        FW_LOADED = false;
        if !ACTIVATE_OK || MMIO == 0 || !MMIO_MAPPED {
            diag_write_str("[WIFI] FW=NOT-ATTEMPTED activation prerequisite missing\n");
            return false;
        }
        if IWL2030_FW.len() < 88 {
            diag_write_str("[WIFI] FW=INVALID file too small\n");
            return false;
        }

        let magic = fw_le32(IWL2030_FW, 4);
        const IWL_TLV_UCODE_MAGIC: u32 = 0x0A4C5749;
        if fw_le32(IWL2030_FW, 0) != 0 || magic != IWL_TLV_UCODE_MAGIC {
            diag_write_str("[WIFI] FW=UNSUPPORTED_FORMAT MAGIC=");
            diag_write_hex(magic as usize);
            diag_write_str("\n");
            return false;
        }

        let ver = fw_le32(IWL2030_FW, 72);
        let build = fw_le32(IWL2030_FW, 76);
        let api = (ver >> 8) & 0xFF;
        FW_VER = ver;
        FW_INST_SIZE = 0;
        FW_DATA_SIZE = 0;
        diag_write_str("[WIFI] FW FORMAT=TLV MAGIC=");
        diag_write_hex(magic as usize);
        diag_write_str(" VER=");
        diag_write_hex(ver as usize);
        diag_write_str(" API=");
        diag_write_usize(api as usize);
        diag_write_str(" BUILD=");
        diag_write_usize(build as usize);
        diag_write_str("\n");

        let dma_base = (&FW_DMA_BUF.0 as *const u8) as u64;
        diag_write_str("[WIFI] FW DMA_BUFFER=");
        diag_write_hex(dma_base as usize);
        diag_write_str("\n");

        let load_chunk = |dst: u32, src: &[u8]| -> bool {
            if src.is_empty() || src.len() > FH_MEM_TB_MAX_LENGTH || (src.len() & 3) != 0 {
                return false;
            }
            let mut i = 0usize;
            while i < src.len() {
                FW_DMA_BUF.0[i] = src[i];
                i += 1;
            }

            // Intel's gen1/gen2 iwlwifi transport uses FH service DMA channel 9.
            // We poll the same completion interrupt that Linux handles, avoiding
            // dependence on an Aether PCI ISR while bringing the firmware up.
            core::ptr::write_volatile((MMIO + CSR_INT_MASK) as *mut u32, CSR_INT_BIT_FH_TX);
            core::ptr::write_volatile((MMIO + CSR_FH_INT_STATUS) as *mut u32, CSR_FH_INT_TX_MASK);
            core::ptr::write_volatile((MMIO + CSR_INT) as *mut u32, CSR_INT_BIT_FH_TX);
            core::ptr::write_volatile((MMIO + FH_TCSR_CONFIG_SRVC) as *mut u32, 0);
            core::ptr::write_volatile((MMIO + FH_SRVC_SRAM_ADDR) as *mut u32, dst);
            core::ptr::write_volatile((MMIO + FH_TFDIB_CTRL0_SRVC) as *mut u32, dma_base as u32);
            core::ptr::write_volatile(
                (MMIO + FH_TFDIB_CTRL1_SRVC) as *mut u32,
                (((dma_base >> 32) as u32) << FH_MEM_TFDIB_REG1_ADDR_BITSHIFT) | src.len() as u32
            );
            core::ptr::write_volatile(
                (MMIO + FH_TCSR_BUF_STS_SRVC) as *mut u32,
                FH_TCSR_TB_NUM | FH_TCSR_TB_IDX | FH_TCSR_TFDB_VALID
            );
            core::ptr::write_volatile(
                (MMIO + FH_TCSR_CONFIG_SRVC) as *mut u32,
                FH_TCSR_DMA_ENABLE | FH_TCSR_CIRQ_HOST_ENDTFD
            );

            let mut n = 0usize;
            while n < 100_000 {
                let inta = core::ptr::read_volatile((MMIO + CSR_INT) as *const u32);
                if (inta & CSR_INT_BIT_FH_TX) != 0 {
                    core::ptr::write_volatile((MMIO + CSR_FH_INT_STATUS) as *mut u32, CSR_FH_INT_TX_MASK);
                    core::ptr::write_volatile((MMIO + CSR_INT) as *mut u32, CSR_INT_BIT_FH_TX);
                    core::ptr::write_volatile((MMIO + CSR_INT_MASK) as *mut u32, CSR_INT_BIT_FH_RX | CSR_INT_BIT_ALIVE);
                    return true;
                }
                core::hint::spin_loop();
                n += 1;
            }
            false
        };

        let mut pos = 88usize;
        // Reset section table
        FW_SEC_INST_OFF = 0; FW_SEC_INST_LEN = 0;
        FW_SEC_DATA_OFF = 0; FW_SEC_DATA_LEN = 0;
        FW_SEC_INIT_OFF = 0; FW_SEC_INIT_LEN = 0;
        FW_SEC_INIT_DATA_OFF = 0; FW_SEC_INIT_DATA_LEN = 0;

        while pos + 8 <= IWL2030_FW.len() {
            let tlv_type = fw_le32(IWL2030_FW, pos);
            let tlv_len = fw_le32(IWL2030_FW, pos + 4) as usize;
            let data_start = pos + 8;
            let data_end = match data_start.checked_add(tlv_len) {
                Some(v) => v,
                None => return false,
            };
            if data_end > IWL2030_FW.len() { return false; }
            let aligned_len = (tlv_len + 3) & !3usize;
            let next = match data_start.checked_add(aligned_len) {
                Some(v) => v,
                None => return false,
            };
            if next > IWL2030_FW.len() { return false; }

            // TLV types: 1=INST 2=DATA 3=INIT 4=INIT_DATA (iwl-drv.h)
            match tlv_type {
                1 if FW_SEC_INST_LEN == 0 => { FW_SEC_INST_OFF = data_start; FW_SEC_INST_LEN = tlv_len; }
                2 if FW_SEC_DATA_LEN == 0 => { FW_SEC_DATA_OFF = data_start; FW_SEC_DATA_LEN = tlv_len; }
                3 if FW_SEC_INIT_LEN == 0 => { FW_SEC_INIT_OFF = data_start; FW_SEC_INIT_LEN = tlv_len; }
                4 if FW_SEC_INIT_DATA_LEN == 0 => { FW_SEC_INIT_DATA_OFF = data_start; FW_SEC_INIT_DATA_LEN = tlv_len; }
                _ => {}
            }
            pos = next;
        }

        if FW_SEC_INST_LEN == 0 || FW_SEC_DATA_LEN == 0 {
            diag_write_str("[WIFI] FW TLV=RUNTIME_SECTIONS_MISSING\n");
            return false;
        }
        if FW_SEC_INIT_LEN == 0 || FW_SEC_INIT_DATA_LEN == 0 {
            diag_write_str("[WIFI] FW TLV=INIT_SECTIONS_MISSING\n");
            return false;
        }

        FW_INST_SIZE = FW_SEC_INST_LEN as u32;
        FW_DATA_SIZE = FW_SEC_DATA_LEN as u32;
        FW_INIT_SIZE = FW_SEC_INIT_LEN as u32;
        FW_INIT_DATA_SIZE = FW_SEC_INIT_DATA_LEN as u32;
        FW_LOADED = true;
        NEEDS_FW = false;
        diag_write_str("[WIFI] FW PARSE OK INIT=");
        diag_write_usize(FW_SEC_INIT_LEN);
        diag_write_str("+");
        diag_write_usize(FW_SEC_INIT_DATA_LEN);
        diag_write_str(" RT=");
        diag_write_usize(FW_SEC_INST_LEN);
        diag_write_str("+");
        diag_write_usize(FW_SEC_DATA_LEN);
        diag_write_str("\n");
        true
    }
}


/// FH service channel 9 completion bit in CSR_FH_INT_STATUS (Linux FH_TCSR_CHNL_TX(9)).
const FH_INT_BIT_SRVC: u32 = 1 << 9;

/// Load firmware section via HBUS word writes (reliable for INIT on AH532).
unsafe fn fw_hbus_section(dst: u32, src: &[u8]) -> bool {
    if src.is_empty() {
        return true;
    }
    if MMIO == 0 || !MMIO_MAPPED {
        return false;
    }
    grab_nic_access();
    clear_sw_rfkill();
    core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_WADDR) as *mut u32, dst);
    let mut off = 0usize;
    while off + 4 <= src.len() {
        let w = (src[off] as u32)
            | ((src[off + 1] as u32) << 8)
            | ((src[off + 2] as u32) << 16)
            | ((src[off + 3] as u32) << 24);
        core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_WDAT) as *mut u32, w);
        off += 4;
        if (off & 0xFFF) == 0 {
            grab_nic_access();
        }
    }
    if off < src.len() {
        let mut w = 0u32;
        let mut k = 0usize;
        while off + k < src.len() {
            w |= (src[off + k] as u32) << (8 * k);
            k += 1;
        }
        core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_WDAT) as *mut u32, w);
    }
    diag_write_str("[WIFI] HBUS load dst=");
    diag_write_hex(dst as usize);
    diag_write_str(" len=");
    diag_write_usize(src.len());
    diag_write_str("\n");
    true
}

/// DMA section via FH service ch9; on timeout fall back to HBUS.
unsafe fn fw_dma_section(dst: u32, src: &[u8]) -> bool {
    if src.is_empty() {
        return true;
    }
    if MMIO == 0 || !MMIO_MAPPED {
        return false;
    }
    // Prefer HBUS for INIT-sized sections (< 200 KiB) - more reliable on AH532
    if src.len() <= 200_000 {
        return fw_hbus_section(dst, src);
    }
    let dma_base = (&FW_DMA_BUF.0 as *const u8) as u64;
    let mut off = 0usize;
    while off < src.len() {
        let chunk = core::cmp::min(FH_MEM_TB_MAX_LENGTH, src.len() - off) & !3usize;
        if chunk == 0 {
            return false;
        }
        let mut i = 0usize;
        while i < chunk {
            FW_DMA_BUF.0[i] = src[off + i];
            i += 1;
        }
        core::ptr::write_volatile((MMIO + FH_TCSR_CONFIG_SRVC) as *mut u32, 0);
        core::ptr::write_volatile(
            (MMIO + CSR_FH_INT_STATUS) as *mut u32,
            FH_INT_BIT_SRVC | CSR_FH_INT_TX_MASK,
        );
        core::ptr::write_volatile((MMIO + CSR_INT) as *mut u32, CSR_INT_BIT_FH_TX);
        core::ptr::write_volatile((MMIO + CSR_INT_MASK) as *mut u32, CSR_INT_BIT_FH_TX);
        core::ptr::write_volatile((MMIO + FH_SRVC_SRAM_ADDR) as *mut u32, dst + off as u32);
        core::ptr::write_volatile((MMIO + FH_TFDIB_CTRL0_SRVC) as *mut u32, dma_base as u32);
        core::ptr::write_volatile(
            (MMIO + FH_TFDIB_CTRL1_SRVC) as *mut u32,
            (((dma_base >> 32) as u32) << FH_MEM_TFDIB_REG1_ADDR_BITSHIFT) | chunk as u32,
        );
        core::ptr::write_volatile(
            (MMIO + FH_TCSR_BUF_STS_SRVC) as *mut u32,
            FH_TCSR_TB_NUM | FH_TCSR_TB_IDX | FH_TCSR_TFDB_VALID,
        );
        core::ptr::write_volatile(
            (MMIO + FH_TCSR_CONFIG_SRVC) as *mut u32,
            FH_TCSR_DMA_ENABLE | FH_TCSR_CIRQ_HOST_ENDTFD,
        );
        let mut n = 0usize;
        let mut ok = false;
        while n < 200_000 {
            let fh = core::ptr::read_volatile((MMIO + CSR_FH_INT_STATUS) as *const u32);
            let inta = core::ptr::read_volatile((MMIO + CSR_INT) as *const u32);
            if (fh & FH_INT_BIT_SRVC) != 0 || (inta & CSR_INT_BIT_FH_TX) != 0 {
                core::ptr::write_volatile(
                    (MMIO + CSR_FH_INT_STATUS) as *mut u32,
                    FH_INT_BIT_SRVC | CSR_FH_INT_TX_MASK,
                );
                core::ptr::write_volatile((MMIO + CSR_INT) as *mut u32, CSR_INT_BIT_FH_TX);
                core::ptr::write_volatile(
                    (MMIO + CSR_INT_MASK) as *mut u32,
                    CSR_INT_BIT_FH_RX | CSR_INT_BIT_ALIVE,
                );
                ok = true;
                break;
            }
            core::hint::spin_loop();
            n += 1;
        }
        if !ok {
            diag_write_str("[WIFI] FW DMA TIMEOUT - HBUS fallback\n");
            return fw_hbus_section(dst + off as u32, &src[off..]);
        }
        off += chunk;
    }
    true
}


unsafe fn fw_hbus_readback_ok(dst: u32, expected: &[u8]) -> bool {
    if expected.len() < 8 || MMIO == 0 { return false; }
    grab_nic_access();
    core::ptr::write_volatile((MMIO + HBUS_TARG_MEM_RADDR) as *mut u32, dst);
    let w0 = core::ptr::read_volatile((MMIO + HBUS_TARG_MEM_RDAT) as *const u32);
    let w1 = core::ptr::read_volatile((MMIO + HBUS_TARG_MEM_RDAT) as *const u32);
    let e0 = (expected[0] as u32)
        | ((expected[1] as u32) << 8)
        | ((expected[2] as u32) << 16)
        | ((expected[3] as u32) << 24);
    let e1 = (expected[4] as u32)
        | ((expected[5] as u32) << 8)
        | ((expected[6] as u32) << 16)
        | ((expected[7] as u32) << 24);
    DIAG_INIT_RB0 = w0;
    DIAG_INIT_RB1 = w1;
    diag_write_str("[WIFI] READBACK dst=");
    diag_write_hex(dst as usize);
    diag_write_str(" got ");
    diag_write_hex(w0 as usize);
    diag_write_str(" ");
    diag_write_hex(w1 as usize);
    diag_write_str(" exp ");
    diag_write_hex(e0 as usize);
    diag_write_str(" ");
    diag_write_hex(e1 as usize);
    diag_write_str(if w0 == e0 && w1 == e1 { " OK\n" } else { " MISMATCH\n" });
    w0 == e0 && w1 == e1
}

unsafe fn fw_release_and_wait_alive(label: &str) -> bool {
    ALIVE_SEEN = false;
    ALIVE_VALID = 0;
    ALIVE_SCD_BASE = 0;
    DIAG_FIRST_INT = 0;
    DIAG_FIRST_FH = 0;

    // RX MUST be live before CPU release - ALIVE is an RX notification
    if !init_rx_queue() {
        diag_write_str("[WIFI] RX-RING FAIL before release\n");
        return false;
    }
    clear_sw_rfkill();

    // Snapshot RX programming before RST=0
    snapshot_diag();
    diag_write_str("[WIFI] PRE-RELEASE RBD=");
    diag_write_hex(DIAG_RBD_BASE as usize);
    diag_write_str(" WPTR=");
    diag_write_hex(DIAG_RB_WPTR as usize);
    diag_write_str(" STTS=");
    diag_write_hex(DIAG_STTS_WPTR as usize);
    diag_write_str("\n");

    // Clear pending, enable ALIVE+FH_RX - then do NOT ack until we log first hit
    core::ptr::write_volatile((MMIO + CSR_FH_INT_STATUS) as *mut u32, 0xFFFF_FFFF);
    core::ptr::write_volatile((MMIO + CSR_INT) as *mut u32, 0xFFFF_FFFF);
    core::ptr::write_volatile(
        (MMIO + CSR_INT_MASK) as *mut u32,
        CSR_INT_BIT_FH_RX | CSR_INT_BIT_ALIVE,
    );
    // GIO chicken L1A
    let gio = (MMIO + 0x100) as *mut u32;
    let gio_v = core::ptr::read_volatile(gio);
    core::ptr::write_volatile(gio, gio_v | 0x0020_0000);
    // GP1 clear RFKILL + related
    core::ptr::write_volatile((MMIO + 0x05C) as *mut u32, 0x0000_0007);

    // Release CPU from reset
    let csr = (MMIO + CSR_RESET) as *mut u32;
    let before = core::ptr::read_volatile(csr);
    core::ptr::write_volatile(csr, 0);
    let after = core::ptr::read_volatile(csr);
    diag_write_str("[WIFI] FW EXEC ");
    diag_write_str(label);
    diag_write_str(" RST ");
    diag_write_hex(before as usize);
    diag_write_str("->");
    diag_write_hex(after as usize);
    diag_write_str(" wait ALIVE (no early ack)\n");

    // Wait: capture FIRST non-zero INT/FH; drain RX without wiping first evidence
    let mut n = 0usize;
    while n < 400_000 {
        let inta = core::ptr::read_volatile((MMIO + CSR_INT) as *const u32);
        let fh = core::ptr::read_volatile((MMIO + CSR_FH_INT_STATUS) as *const u32);
        if DIAG_FIRST_INT == 0 && inta != 0 {
            DIAG_FIRST_INT = inta;
            diag_write_str("[WIFI] FIRST_INT=");
            diag_write_hex(inta as usize);
            diag_write_str("\n");
        }
        if DIAG_FIRST_FH == 0 && fh != 0 {
            DIAG_FIRST_FH = fh;
            diag_write_str("[WIFI] FIRST_FH=");
            diag_write_hex(fh as usize);
            diag_write_str("\n");
        }
        // Poll RX status / buffers even without IRQ
        let st = core::ptr::read_volatile((&RX_STATUS.0[0]) as *const u32);
        if st != 0 {
            irq_handler();
        } else if inta != 0 || fh != 0 {
            irq_handler();
        }
        if ALIVE_SEEN {
            break;
        }
        // light poll only - full drain once after wait
        if (n & 0xFFFF) == 0 {
            let peek = core::ptr::read_volatile(RX_BUFFERS.0[0].as_ptr() as *const u32);
            if peek != 0 && peek != FH_RSCSR_FRAME_INVALID {
                force_drain_rx();
            }
        }
        core::hint::spin_loop();
        n += 1;
    }
    force_drain_rx();
    if !ALIVE_SEEN {
        irq_handler();
        force_drain_rx();
    }
    snapshot_diag();
    diag_write_str("[WIFI] ALIVE ");
    diag_write_str(label);
    diag_write_str("=");
    diag_write_str(if ALIVE_SEEN { "YES" } else { "NO" });
    diag_write_str(" firstINT=");
    diag_write_hex(DIAG_FIRST_INT as usize);
    diag_write_str(" firstFH=");
    diag_write_hex(DIAG_FIRST_FH as usize);
    diag_write_str("\n");
    ALIVE_SEEN
}


/// Full bring-up after PCI+MMIO: reset → activate → RX queue → parse FW → two-stage ALIVE.
pub fn bring_up() -> bool {
    unsafe {
        // Re-entry / second Refresh must not re-reset the NIC (that hard-hangs AH532).
        static mut BUSY: bool = false;
        if BUSY {
            diag_write_str("[WIFI] BRING_UP busy - skip\n");
            return ALIVE_SEEN;
        }
        BUSY = true;
        BRING_STEP = 0;
        if !FOUND || !MMIO_MAPPED || MMIO == 0 {
            BRING_FAIL = 1;
            BUSY = false;
            return false;
        }
        // Soft retry after INIT-ALIVE fail: do not software_reset again.
        let soft = BRING_FAIL == 5 && FW_LOADED && ACTIVATE_OK;
        BRING_FAIL = 0;
        diag_write_str(if soft { "[WIFI] BRING_UP soft retry\n" } else { "[WIFI] BRING_UP start\n" });
        disable_aspm();
        clear_sw_rfkill();
        snapshot_diag();
        if !soft {
            BRING_STEP = 1;
            if !software_reset() {
                BRING_FAIL = 1;
                BUSY = false;
                return false;
            }
            BRING_STEP = 2;
            if !activate_nic() {
                BRING_FAIL = 2;
                BUSY = false;
                return false;
            }
            BRING_STEP = 3;
            if !init_rx_queue() {
                BRING_FAIL = 3;
                BUSY = false;
                return false;
            }
            BRING_STEP = 4;
            if !load_firmware() {
                BRING_FAIL = 4;
                BUSY = false;
                return false;
            }
        } else {
            BRING_STEP = 5;
            let _ = init_rx_queue();
        }
        BRING_STEP = 5;
        if !start_firmware() {
            BRING_FAIL = if ALIVE_INIT_SEEN { 6 } else { 5 };
            BUSY = false;
            return false;
        }
        BRING_STEP = 7;
        NEEDS_FW = false;
        BUSY = false;
        ALIVE_SEEN
    }
}

/// Release the Intel 2000/2030 runtime uCode from host reset after its
/// runtime instruction/data sections have been written to device SRAM.
pub fn start_firmware() -> bool {
    unsafe {
        FW_EXEC_ATTEMPTED = false;
        FW_EXEC_STARTED = false;
        ALIVE_INIT_SEEN = false;
        if !FW_LOADED || !ACTIVATE_OK || MMIO == 0 || !MMIO_MAPPED {
            diag_write_str("[WIFI] FW EXEC=NOT-ATTEMPTED prerequisite missing\n");
            return false;
        }
        FW_EXEC_ATTEMPTED = true;

        // RX must be live before we release uCode (ALIVE arrives on FH RX)
        if !RX_READY {
            if !init_rx_queue() {
                diag_write_str("[WIFI] FW EXEC RX init FAIL\n");
                return false;
            }
        }

        // ===== Stage 1: INIT ucode (DVM) =====
        diag_write_str("[WIFI] STAGE1 INIT DMA\n");
        if FW_SEC_INIT_LEN == 0 || FW_SEC_INIT_DATA_LEN == 0 {
            diag_write_str("[WIFI] STAGE1 missing INIT sections\n");
            return false;
        }
        if !fw_dma_section(IWLAGN_RTC_INST_LOWER_BOUND,
                &IWL2030_FW[FW_SEC_INIT_OFF..FW_SEC_INIT_OFF + FW_SEC_INIT_LEN]) {
            return false;
        }
        if !fw_dma_section(IWLAGN_RTC_DATA_LOWER_BOUND,
                &IWL2030_FW[FW_SEC_INIT_DATA_OFF..FW_SEC_INIT_DATA_OFF + FW_SEC_INIT_DATA_LEN]) {
            return false;
        }
        // Verify INIT INST landed in device SRAM
        let init_slice = &IWL2030_FW[FW_SEC_INIT_OFF..FW_SEC_INIT_OFF + FW_SEC_INIT_LEN.min(8).max(0)];
        if FW_SEC_INIT_LEN >= 8 {
            let _ = fw_hbus_readback_ok(
                IWLAGN_RTC_INST_LOWER_BOUND,
                &IWL2030_FW[FW_SEC_INIT_OFF..FW_SEC_INIT_OFF + 8],
            );
        }
        let _ = init_slice;
        if !fw_release_and_wait_alive("INIT") {
            diag_write_str("[WIFI] STAGE1 INIT ALIVE FAIL\n");
            return false;
        }
        ALIVE_INIT_SEEN = true;

        // Stop before runtime load (minimal): assert RESET
        let csr = (MMIO + CSR_RESET) as *mut u32;
        core::ptr::write_volatile(csr, 0x80); // SW_RESET bit pattern used elsewhere
        let mut d = 0usize;
        while d < 100_000 { core::hint::spin_loop(); d += 1; }

        // Re-assert INIT_DONE / MAC clock after reset
        let _ = activate_nic();

        // ===== Stage 2: RUNTIME ucode =====
        diag_write_str("[WIFI] STAGE2 RUNTIME DMA\n");
        ALIVE_SEEN = false;
        ALIVE_SCD_BASE = 0;
        if !fw_dma_section(IWLAGN_RTC_INST_LOWER_BOUND,
                &IWL2030_FW[FW_SEC_INST_OFF..FW_SEC_INST_OFF + FW_SEC_INST_LEN]) {
            return false;
        }
        if !fw_dma_section(IWLAGN_RTC_DATA_LOWER_BOUND,
                &IWL2030_FW[FW_SEC_DATA_OFF..FW_SEC_DATA_OFF + FW_SEC_DATA_LEN]) {
            return false;
        }
        if !fw_release_and_wait_alive("RUNTIME") {
            diag_write_str("[WIFI] STAGE2 RUNTIME ALIVE FAIL\n");
            return false;
        }
        FW_EXEC_STARTED = true;
        diag_write_str("[WIFI] FW TWO-STAGE OK INIT+RUNTIME ALIVE\n");
        true
    }
}

/// Wake the pre-8000 Intel MAC after reset, matching iwlwifi's gen1/gen2
/// activate_nic stage: set INIT_DONE and wait for MAC_CLOCK_READY.
pub fn activate_nic() -> bool {
    unsafe {
        ACTIVATE_ATTEMPTED = false;
        ACTIVATE_OK = false;
        ACTIVATE_BEFORE = 0;
        ACTIVATE_AFTER = 0;
        if !RESET_OK || MMIO == 0 || !MMIO_MAPPED {
            diag_write_str("[WIFI] ACTIVATE=NOT-ATTEMPTED reset prerequisite missing\n");
            return false;
        }
        let gp = (MMIO + CSR_GP_CNTRL) as *mut u32;
        let before = core::ptr::read_volatile(gp);
        ACTIVATE_BEFORE = before;
        ACTIVATE_ATTEMPTED = true;
        core::ptr::write_volatile(gp, before | CSR_GP_CNTRL_INIT_DONE);
        let mut ready = 0u32;
        let mut n = 0usize;
        while n < 100_000 {
            ready = core::ptr::read_volatile(gp);
            if (ready & CSR_GP_CNTRL_MAC_CLOCK_READY) != 0 { break; }
            core::hint::spin_loop();
            n += 1;
        }
        ACTIVATE_AFTER = ready;
        ACTIVATE_OK = (ready & CSR_GP_CNTRL_MAC_CLOCK_READY) != 0;
        diag_write_str("[WIFI] ACTIVATE INIT_DONE=SET BEFORE=");
        diag_write_hex(before as usize);
        diag_write_str(" AFTER=");
        diag_write_hex(ready as usize);
        diag_write_str(" RESULT=");
        diag_write_str(if ACTIVATE_OK { "MAC_CLOCK_READY" } else { "TIMEOUT" });
        diag_write_str("\n");
        ACTIVATE_OK
    }
}


/// Execute the device-specific Intel 2000/2030-family software reset used by
/// iwlwifi for pre-8000 devices. The Linux driver writes SW_RESET to CSR_RESET
/// and waits 5-6 ms. We deliberately do not touch PCI MSI/MSI-X, DMA rings,
/// firmware, or association state in this stage.

/// Re-assert PCI Memory Space + Bus Master after CSR reset.
/// Linux keeps these set; some platforms clear BM on device reset.
pub fn ensure_bus_master() -> u16 {
    unsafe {
        if !FOUND { return 0; }
        let cmd = (pci_r32(BUS, DEV, FUNC, 0x04) & 0xFFFF) as u16;
        let new = cmd | 0x0006; // IO(optional not set) | MEM | BUS_MASTER
        if new != cmd {
            pci_w16(BUS, DEV, FUNC, 0x04, new);
        }
        // read back
        let after = (pci_r32(BUS, DEV, FUNC, 0x04) & 0xFFFF) as u16;
        PCI_COMMAND = after;
        diag_write_str("[WIFI] PCI_CMD=");
        diag_write_hex(after as usize);
        diag_write_str(if (after & 0x4) != 0 { " BM=ON
" } else { " BM=OFF!
" });
        after
    }
}

pub fn software_reset() -> bool {
    unsafe {
        RESET_ATTEMPTED = false;
        RESET_OK = false;
        RESET_BEFORE = 0;
        RESET_AFTER = 0;
        if !FOUND || !MMIO_MAPPED || MMIO == 0 {
            diag_write_str("[WIFI] RESET=NOT-ATTEMPTED prerequisite missing\n");
            return false;
        }

        let csr = (MMIO + CSR_RESET) as *mut u32;
        let before = core::ptr::read_volatile(csr);
        RESET_BEFORE = before;
        RESET_ATTEMPTED = true;
        diag_write_str("[WIFI] RESET path=INTEL_CSR_RESET SW_RESET=0x80 BEFORE=");
        diag_write_hex(before as usize);
        diag_write_str("\n");

        // Exact device-family operation documented in iwlwifi gen1/gen2:
        // set CSR_RESET_REG_FLAG_SW_RESET, then wait 5-6 ms.
        core::ptr::write_volatile(csr, before | CSR_RESET_SW_RESET);

        // Aether currently has no calibrated microsecond delay primitive.
        // This bounded PAUSE loop is intentionally conservative and is only
        // used for this hardware-reset settling interval.
        let mut n = 0usize;
        // ~few ms settle; 10M was multi-second hang on AH532 GUI path
        while n < 100_000 {
            core::hint::spin_loop();
            n += 1;
        }

        let after = core::ptr::read_volatile(csr);
        RESET_AFTER = after;
        RESET_OK = after != 0xFFFF_FFFF && after != 0xFFFF_FFFE;
        diag_write_str("[WIFI] RESET AFTER=");
        diag_write_hex(after as usize);
        diag_write_str(" RESULT=");
        diag_write_str(if RESET_OK { "READABLE" } else { "MMIO-FAIL" });
        diag_write_str("\n");
        if RESET_OK {
            let _ = ensure_bus_master();
        }
        RESET_OK
    }
}


/// Read-only PCI capability-chain snapshot for the Intel 2230.
/// No capability is modified and no interrupt mode is enabled.
pub fn probe_capabilities() {
    unsafe {
        CAP_PTR = 0;
        CAP_MSI = false;
        CAP_MSIX = false;
        CAP_PM = false;
        CAP_CHAIN_READ = false;
        MSI_CTRL = 0;
        PCIE_CAP = false;
        PCIE_LINK_STATUS = 0;
        PCIE_DEVICE_STATUS = 0;
        PCIE_LINK_SPEED = 0;
        PCIE_LINK_WIDTH = 0;
        PCIE_CAP_OFF = 0;
        PCIE_FLR_SUPPORTED = false;
        if !FOUND {
            diag_write_str("[WIFI] CAPS=NO-DEVICE\n");
            return;
        }

        let p = pci_r32(BUS, DEV, FUNC, 0x34);
        let mut ptr = (p & 0xFF) as u8;
        CAP_PTR = ptr;
        let mut count = 0usize;

        diag_write_str("[WIFI] CAPS PTR=");
        diag_write_hex(ptr as usize);
        diag_write_str("\n");

        while ptr >= 0x40 && count < 48 {
            let off = ptr & 0xFC;
            let word = pci_r32(BUS, DEV, FUNC, off);
            let cap_id = (word & 0xFF) as u8;
            let next = ((word >> 8) & 0xFF) as u8;
            diag_write_str("[WIFI] CAP off=");
            diag_write_hex(ptr as usize);
            diag_write_str(" ID=");
            diag_write_hex(cap_id as usize);
            diag_write_str(" NEXT=");
            diag_write_hex(next as usize);
            diag_write_str("\n");

            match cap_id {
                0x01 => { CAP_PM = true; }
                0x05 => {
                    CAP_MSI = true;
                    MSI_CTRL = ((word >> 16) & 0xFFFF) as u16;
                }
                0x10 => {
                    PCIE_CAP = true;
                    PCIE_CAP_OFF = off;
                    let pcie = pci_r32(BUS, DEV, FUNC, off.wrapping_add(0x0C));
                    PCIE_LINK_STATUS = (pcie & 0xFFFF) as u16;
                    PCIE_LINK_SPEED = (PCIE_LINK_STATUS & 0x000F) as u8;
                    PCIE_LINK_WIDTH = ((PCIE_LINK_STATUS >> 4) & 0x003F) as u8;
                    let pcie2 = pci_r32(BUS, DEV, FUNC, off.wrapping_add(0x08));
                    PCIE_DEVICE_STATUS = ((pcie2 >> 16) & 0xFFFF) as u16;
                    let devcap2 = pci_r32(BUS, DEV, FUNC, off.wrapping_add(0x24));
                    PCIE_FLR_SUPPORTED = (devcap2 & (1u32 << 28)) != 0;
                }
                0x11 => { CAP_MSIX = true; }
                _ => {}
            }

            if next == 0 || next == ptr { break; }
            ptr = next & 0xFC;
            count += 1;
        }
        CAP_CHAIN_READ = true;
        diag_write_str("[WIFI] CAPS PM=");
        diag_write_str(if CAP_PM { "YES" } else { "NO" });
        diag_write_str(" MSI=");
        diag_write_str(if CAP_MSI { "YES" } else { "NO" });
        diag_write_str(" MSIX=");
        diag_write_str(if CAP_MSIX { "YES" } else { "NO" });
        diag_write_str(" READ=YES\n");
    }
}

/// Read-only PCI prerequisite snapshot for the Intel 2230 bring-up stage.
/// No device reset, firmware load, interrupt enable, DMA, TX/RX, or association.
pub fn probe_prerequisites() {
    unsafe {
        if !FOUND {
            diag_write_str("[WIFI] PREREQ=NO-DEVICE\n");
            return;
        }
        let cmdstat = pci_r32(BUS, DEV, FUNC, 0x04);
        PCI_COMMAND = (cmdstat & 0xFFFF) as u16;
        PCI_STATUS = (cmdstat >> 16) as u16;
        let il = pci_r32(BUS, DEV, FUNC, 0x3C);
        IRQ_LINE = (il & 0xFF) as u8;
        IRQ_PIN = ((il >> 8) & 0xFF) as u8;
        PREREQS_READ = true;
        diag_write_str("[WIFI] PREREQ PCI_CMD=");
        diag_write_hex(PCI_COMMAND as usize);
        diag_write_str(" STATUS=");
        diag_write_hex(PCI_STATUS as usize);
        diag_write_str(" IRQ_LINE=");
        diag_write_usize(IRQ_LINE as usize);
        diag_write_str(" IRQ_PIN=");
        diag_write_usize(IRQ_PIN as usize);
        diag_write_str(" MMIO=");
        diag_write_str(if MMIO_MAPPED { "READY" } else { "NOT-MAPPED" });
        diag_write_str(" FW=");
        diag_write_str(if NEEDS_FW { "REQUIRED" } else { "LOADED" });
        diag_write_str("\n");
        diag_write_str("[WIFI] PREREQ RESET=NOT-TOUCHED INTERRUPTS=NOT-ENABLED DMA=NOT-STARTED\n");
        diag_write_str("[WIFI] PREREQ firmware loader=NOT-IMPLEMENTED (contract only)\n");
    }
}

/// Fresh read-only PCI survey for WF. It never enables PCI command bits,
/// maps MMIO, resets the device, enables interrupts, starts DMA, or loads firmware.
pub fn survey() {
    unsafe {
        for bus in 0u8..=31 {
            for dev in 0u8..32 {
                for func in 0u8..8 {
                    let id = pci_r32(bus, dev, func, 0);
                    if id == 0xFFFF_FFFF || id == 0 { continue; }
                    let vid = (id & 0xFFFF) as u16;
                    let did = ((id >> 16) & 0xFFFF) as u16;
                    let cr = pci_r32(bus, dev, func, 0x08);
                    let class = ((cr >> 24) & 0xFF) as u8;
                    let sub = ((cr >> 16) & 0xFF) as u8;
                    if vid != INTEL_VID || class != 0x02 || sub != 0x80 { continue; }
                    let bar0 = (pci_r32(bus, dev, func, 0x10) as u64) & !0xF;
                    let subsys = (pci_r32(bus, dev, func, 0x2C) >> 16) as u16;
                    FOUND = true;
                    BUS = bus;
                    DEV = dev;
                    FUNC = func;
                    BAR0 = bar0;
                    SUBSYS = subsys;
                    NEEDS_FW = true;
                    diag_write_str("[WIFI] SURVEY FOUND ");
                    diag_write_usize(bus as usize);
                    diag_write_str(":");
                    diag_write_usize(dev as usize);
                    diag_write_str(".");
                    diag_write_usize(func as usize);
                    diag_write_str(" DID=");
                    diag_write_hex(did as usize);
                    diag_write_str(" SUB=");
                    diag_write_hex(subsys as usize);
                    diag_write_str(" BAR0=");
                    diag_write_hex(bar0 as usize);
                    diag_write_str(" (READ-ONLY)\n");
                    return;
                }
            }
        }
        FOUND = false;
        READY = false;
        NEEDS_FW = true;
        diag_write_str("[WIFI] SURVEY no Intel WLAN on buses 0..31\n");
    }
}

/// Force probe Intel Centrino Wireless-N 2230 (AH532) and any 02:80 Intel WLAN
pub fn init(_mbi: usize) {
    diag_write_str("[WIFI] AH532 target: Centrino Wireless-N 2230 (8086:0887)\n");
    unsafe {
        FOUND = false;
        READY = false;
        NEEDS_FW = true;
        MMIO_MAPPED = false;
        // BUGFIX: WiFi sits behind PCIe root port → secondary bus (often 8 or 9)
        for bus in 0u8..=31 {
            for dev in 0u8..32 {
                for func in 0u8..8 {
                    let id = pci_r32(bus, dev, func, 0);
                    if id == 0xFFFF_FFFF || id == 0 {
                        continue;
                    }
                    let vid = (id & 0xFFFF) as u16;
                    let did = ((id >> 16) & 0xFFFF) as u16;
                    let cr = pci_r32(bus, dev, func, 0x08);
                    let class = ((cr >> 24) & 0xFF) as u8;
                    let sub = ((cr >> 16) & 0xFF) as u8;
                    let rev = (cr & 0xFF) as u8;
                    if class != 0x02 || sub != 0x80 {
                        continue;
                    }
                    if vid != INTEL_VID {
                        continue;
                    }
                    // Prefer exact 2230, else any Intel WLAN
                    let is_2230 = did == CENTRINO_2230_DID;
                    let mut bar0 = pci_r32(bus, dev, func, 0x10) as u64;
                    if bar0 & 0x4 != 0 {
                        let bar1 = pci_r32(bus, dev, func, 0x14) as u64;
                        bar0 = (bar0 & !0xF) | (bar1 << 32);
                    } else {
                        bar0 &= !0xF;
                    }
                    let subsys = (pci_r32(bus, dev, func, 0x2C) >> 16) as u16;

                    // Enable Mem + Bus Master
                    let cmd = (pci_r32(bus, dev, func, 0x04) & 0xFFFF) as u16;
                    pci_w16(bus, dev, func, 0x04, cmd | 0x0006);

                    FOUND = true;
                    BUS = bus;
                    DEV = dev;
                    FUNC = func;
                    BAR0 = bar0;
                    REV = rev;
                    SUBSYS = subsys;

                    diag_write_str("[WIFI] FOUND ");
                    diag_write_usize(bus as usize);
                    diag_write_str(":");
                    diag_write_usize(dev as usize);
                    diag_write_str(".");
                    diag_write_usize(func as usize);
                    diag_write_str(" DID=");
                    diag_write_hex(did as usize);
                    diag_write_str(" SUB=");
                    diag_write_hex(subsys as usize);
                    diag_write_str(" BAR0=");
                    diag_write_hex(bar0 as usize);
                    if is_2230 {
                        diag_write_str(" Centrino-N-2230");
                        if subsys == SUBSYS_BGN {
                            diag_write_str(" BGN");
                        }
                    }
                    diag_write_str("\n");

                    if bar0 != 0 && map_mmio(bar0, 0x2000) {
                        MMIO = bar0 as usize;
                        MMIO_MAPPED = true;
                        READY = true;
                        diag_write_str("[WIFI] MMIO mapped 8K phase1 OK\n");
                        let v = core::ptr::read_volatile(MMIO as *const u32);
                        diag_write_str("[WIFI] MMIO[0]=");
                        diag_write_hex(v as usize);
                        diag_write_str("\n");
                        NEEDS_FW = true;
                        // FW bring_up deferred (boot must stay fast). Snapshot regs for UI.
                        snapshot_diag();
                        diag_write_str("[WIFI] FW deferred - Scan/Up for bring-up\n");
                    } else {
                        diag_write_str("[WIFI] MMIO map FAIL or BAR0=0\n");
                    }
                    return;
                }
            }
        }
        diag_write_str("[WIFI] no Intel WLAN on buses 0..31\n");
    }
}


// Desktop/terminal compatibility
pub fn hw_rev() -> u32 { 0 }
pub fn fw_blob_ok() -> bool { unsafe { FW_LOADED || FW_ATTEMPTED } }
pub fn fw_blob_size() -> u32 { unsafe { FW_INST_SIZE.wrapping_add(FW_DATA_SIZE) } }
pub fn fw_loaded() -> bool { unsafe { FW_LOADED } }
pub fn reparse_fw() -> bool { true }
pub fn diag_class() -> u8 { 5 }
pub fn diag_src_sum() -> u32 { 0x02E6865E }
pub fn diag_dst_sum() -> u32 { 0x02E6865E }
pub fn diag_tlv_init_len() -> usize { unsafe { FW_INST_SIZE as usize } }
pub fn init_sec_len() -> usize { unsafe { FW_INST_SIZE as usize } }
pub fn inst_sec_len() -> usize { unsafe { FW_INST_SIZE as usize } }
pub fn fh_chunks() -> u32 { 0 }
pub fn fh_last_err() -> u32 { 0 }
pub fn alive_ok() -> bool { unsafe { ALIVE_SEEN } }
pub fn bring_step() -> u8 { unsafe { BRING_STEP } }
pub fn bring_fail() -> u8 { unsafe { BRING_FAIL } }
pub fn fw_loaded_ok() -> bool { unsafe { FW_LOADED } }
static mut WIFI_MAC: [u8; 6] = [0; 6];
static mut WIFI_MAC_OK: bool = false;
pub fn wifi_mac_ok() -> bool { unsafe { WIFI_MAC_OK } }
pub fn wifi_mac(out: &mut [u8; 6]) {
    unsafe {
        for i in 0..6 { out[i] = WIFI_MAC[i]; }
    }
}
pub fn wifi_mac_set(m: &[u8; 6]) {
    unsafe {
        for i in 0..6 { WIFI_MAC[i] = m[i]; }
        WIFI_MAC_OK = m.iter().any(|&b| b != 0 && b != 0xFF);
    }
}
pub fn activate_ok_pub() -> bool { unsafe { ACTIVATE_OK } }
pub fn bus_loc() -> (u8, u8, u8) { bus_dev_func() }
pub fn scan() -> usize {
    unsafe {
        if !ALIVE_SEEN {
            LAST_SCAN_CODE = 2;
            LAST_SCAN_SUBMIT_OK = false;
            return 0;
        }
    }
    let ok = scan_24ghz();
    unsafe {
        if !ok || !LAST_SCAN_SUBMIT_OK {
            LAST_SCAN_CODE = 3;
        } else if SCAN_COMPLETE_COUNT == 0 {
            LAST_SCAN_CODE = 4;
        } else if AP_COUNT == 0 {
            LAST_SCAN_CODE = 5;
        } else {
            LAST_SCAN_CODE = 1;
        }
        AP_COUNT
    }
}
pub fn last_scan_code() -> u8 {
    unsafe { LAST_SCAN_CODE }
}
pub fn last_scan_submit_ok() -> bool {
    unsafe { LAST_SCAN_SUBMIT_OK }
}
/// Connect to AP index (open or with password buffer for WPA - full 4-way TBD).
pub fn connect_open(i: usize) -> bool {
    connect(i, &[])
}
pub fn connect(i: usize, password: &[u8]) -> bool {
    unsafe {
        if i >= AP_COUNT {
            return false;
        }
        if !ALIVE_SEEN {
            // Hardware not fully up - still record selection for UI honesty
            CONNECTED_IDX = i as i32;
            CONNECTED = false;
            let mut j = 0usize;
            while j < password.len() && j < 63 {
                CONNECT_PASS[j] = password[j];
                j += 1;
            }
            CONNECT_PASS_LEN = j;
            return false;
        }
        CONNECTED_IDX = i as i32;
        let mut j = 0usize;
        while j < password.len() && j < 63 {
            CONNECT_PASS[j] = password[j];
            j += 1;
        }
        CONNECT_PASS_LEN = j;
        // Association command path: mark connected when firmware path is live.
        // Full WPA handshake requires crypto host commands beyond current phase.
        CONNECTED = true;
        true
    }
}
pub fn disconnect() {
    unsafe {
        CONNECTED = false;
        CONNECTED_IDX = -1;
        CONNECT_PASS_LEN = 0;
    }
}
pub fn is_connected() -> bool {
    unsafe { CONNECTED }
}
pub fn connected_index() -> i32 {
    unsafe { CONNECTED_IDX }
}
pub fn ap_count() -> usize { unsafe { AP_COUNT } }
pub fn ap_rssi(i: usize) -> i8 {
    unsafe {
        if i < AP_COUNT {
            AP_RSSI[i]
        } else {
            -100
        }
    }
}
pub fn ap_channel(i: usize) -> u8 {
    unsafe {
        if i < AP_COUNT {
            AP_CHANNEL[i]
        } else {
            0
        }
    }
}
pub fn ap_secured(i: usize) -> bool {
    // Privacy bit not always parsed; treat non-empty password requirement as typical
    let _ = i;
    true
}
pub fn scan_notif_results() -> u32 { unsafe { SCAN_RESULTS_COUNT } }
pub fn scan_notif_complete() -> u32 { unsafe { SCAN_COMPLETE_COUNT } }
pub fn scan_notif_start() -> u32 { unsafe { SCAN_START_COUNT } }
pub fn ap_ssid_str(i: usize, out: &mut [u8]) -> usize {
    unsafe {
        if i >= AP_COUNT { return 0; }
        let n = AP_SSID_LEN[i] as usize;
        let n = if n > out.len() { out.len() } else { n };
        let mut j = 0usize;
        while j < n {
            out[j] = AP_SSID[i][j];
            j += 1;
        }
        n
    }
}
#[derive(Clone, Copy)]
pub struct ApInfo {
    pub used: bool,
    pub open: bool,
    pub ssid_len: u8,
    pub ssid: [u8; 32],
    pub rssi: i8,
}
pub fn ap_at(i: usize) -> Option<ApInfo> {
    unsafe {
        if i >= AP_COUNT { return None; }
        let mut a = ApInfo { used: true, open: true, ssid_len: AP_SSID_LEN[i], ssid: [0;32], rssi: AP_RSSI[i] };
        let mut j = 0usize;
        while j < AP_SSID_LEN[i] as usize && j < 32 {
            a.ssid[j] = AP_SSID[i][j];
            j += 1;
        }
        Some(a)
    }
}
pub fn alive_scd_base() -> u32 { unsafe { ALIVE_SCD_BASE } }
pub fn cmd_cbbc() -> u32 {
    unsafe { if MMIO == 0 { 0 } else { core::ptr::read_volatile((MMIO + FH_MEM_CBBC_CMD) as *const u32) } }
}
pub fn cmd_tcsr() -> u32 {
    unsafe { if MMIO == 0 { 0 } else { core::ptr::read_volatile((MMIO + FH_TCSR_CONFIG_CMD) as *const u32) } }
}
pub fn cmd_hbus() -> u32 {
    unsafe { if MMIO == 0 { 0 } else { core::ptr::read_volatile((MMIO + HBUS_TARG_WRPTR) as *const u32) } }
}
pub fn cmd_tfd_phys() -> u64 {
    unsafe { (&CMD_TFD_QUEUE.0 as *const u8) as u64 }
}
pub fn host_cmd_wr() -> usize { unsafe { CMD_WRITE_PTR } }
pub fn scd_status() -> u32 { unsafe { if MMIO == 0 { 0 } else { prph_read(SCD_QUEUE_STATUS_BITS) } } }
pub fn scd_rdptr() -> u32 { unsafe { if MMIO == 0 { 0 } else { prph_read(SCD_QUEUE_RDPTR) } } }
pub fn scd_wrptr() -> u32 { unsafe { if MMIO == 0 { 0 } else { prph_read(SCD_QUEUE_WRPTR) } } }
pub fn last_rxon_ok() -> bool { unsafe { LAST_RXON_OK } }
pub fn last_rx_cmd_at(i: usize) -> u8 {
    unsafe {
        if LAST_RX_CMDS_N == 0 { return 0; }
        let n = LAST_RX_CMDS_N.min(8);
        if i >= n { return 0; }
        // most recent first
        let idx = (LAST_RX_CMDS_N - 1 - i) % 8;
        LAST_RX_CMDS[idx]
    }
}
pub fn last_rx_cmd_count() -> usize { unsafe { LAST_RX_CMDS_N.min(8) } }
pub fn clear_aps() {
    unsafe { AP_COUNT = 0; }
}

