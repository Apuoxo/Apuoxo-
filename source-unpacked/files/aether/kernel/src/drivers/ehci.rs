//! Minimal EHCI host driver for USB HID Boot Mouse (AH532 USB 2.0 path).
//! Async schedule: control transfers. Periodic schedule: interrupt IN reports.
//! Feeds crate::input::process_hid_boot_mouse - same path as xHCI.

use crate::mm;
use crate::mm::paging;
use crate::serial;

static mut DIAG_OK: bool = false;
static mut DIAG_DEV: bool = false;
static mut DIAG_HID: bool = false;
static mut DIAG_REPORTS: u32 = 0;
static mut DIAG_EVENTS: u32 = 0;
static mut LIVE: bool = false;
static mut LAST_VID: u16 = 0;
static mut LAST_PID: u16 = 0;
static mut LAST_CLASS: u8 = 0;
static mut LAST_ERR: u8 = 0; // 0=ok 1=no_ccs 2=reset 3=get_desc 4=set_addr 5=set_cfg 6=not_hid
static mut DEV_EPS: u32 = 0; // QH_EPS_* used
static mut LAST_SPD: u8 = 0; // 0 none 1 FS/LS 2 HS
static mut LAST_QTD0: u32 = 0;
static mut LAST_QTD1: u32 = 0;
static mut LAST_QTD2: u32 = 0;
static mut LAST_USBSTS: u32 = 0;
static mut XFER_QH: usize = 0; // separate from async head

static mut MMIO: usize = 0;
static mut OP: usize = 0;
static mut NPORTS: u8 = 0;
static mut FRAMELIST: usize = 0;
static mut ASYNC_QH: usize = 0;
static mut PERIOD_QH: usize = 0;
static mut QTD_POOL: usize = 0;
static mut BUF: usize = 0;
static mut DEV_ADDR: u8 = 0;
static mut EP_IN: u8 = 1;
static mut MPS: u16 = 8;
static mut PORT: u8 = 0;

pub fn diag_ok() -> bool {
    unsafe { DIAG_OK }
}
pub fn diag_dev() -> bool {
    unsafe { DIAG_DEV }
}
pub fn diag_hid() -> bool {
    unsafe { DIAG_HID }
}
pub fn diag_reports() -> u32 {
    unsafe { DIAG_REPORTS }
}
pub fn diag_events() -> u32 {
    unsafe { DIAG_EVENTS }
}
pub fn diag_live() -> bool {
    unsafe { LIVE }
}
pub fn last_vid() -> u16 { unsafe { LAST_VID } }
pub fn last_pid() -> u16 { unsafe { LAST_PID } }
pub fn last_class() -> u8 { unsafe { LAST_CLASS } }
pub fn last_err() -> u8 { unsafe { LAST_ERR } }
pub fn last_spd() -> u8 { unsafe { LAST_SPD } }
pub fn last_qtd0() -> u32 { unsafe { LAST_QTD0 } }
pub fn last_qtd1() -> u32 { unsafe { LAST_QTD1 } }
pub fn last_qtd2() -> u32 { unsafe { LAST_QTD2 } }
pub fn last_usbsts() -> u32 { unsafe { LAST_USBSTS } }

unsafe fn pci_cfg(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC)
}
unsafe fn pci_r32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let a = pci_cfg(bus, dev, func, off);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let v: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") v, options(nostack, preserves_flags));
    v
}
unsafe fn pci_w16(bus: u8, dev: u8, func: u8, off: u8, val: u16) {
    let a = pci_cfg(bus, dev, func, off & !3);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let mut old: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") old, options(nostack, preserves_flags));
    let shift = (off as u32 & 2) * 8;
    old = (old & !(0xFFFFu32 << shift)) | ((val as u32) << shift);
    core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") old, options(nostack, preserves_flags));
}
unsafe fn pci_w32(bus: u8, dev: u8, func: u8, off: u8, val: u32) {
    let a = pci_cfg(bus, dev, func, off & !3);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") val, options(nostack, preserves_flags));
}

unsafe fn r32(base: usize, off: usize) -> u32 {
    core::ptr::read_volatile((base + off) as *const u32)
}
unsafe fn w32(base: usize, off: usize, val: u32) {
    core::ptr::write_volatile((base + off) as *mut u32, val);
}
unsafe fn r8(base: usize, off: usize) -> u8 {
    core::ptr::read_volatile((base + off) as *const u8)
}

fn delay(iters: u32) {
    let mut i = 0u32;
    while i < iters {
        i = i.wrapping_add(1);
        core::hint::spin_loop();
    }
}

fn map_mmio(phys: usize, size: usize) -> bool {
    if phys == 0 || size == 0 {
        return false;
    }
    let cr3 = unsafe { paging::read_cr3() };
    let start = phys & !0xFFF;
    let end = (phys + size + 0xFFF) & !0xFFF;
    let mut va = start;
    while va < end {
        let _ = unsafe {
            paging::map_page(
                cr3,
                va,
                va,
                paging::PAGE_PRESENT | paging::PAGE_WRITE | paging::PAGE_PCD | paging::PAGE_PWT,
            )
        };
        va += 0x1000;
    }
    unsafe { paging::load_cr3(cr3) };
    true
}

/// qTD token bits
const QTD_ACTIVE: u32 = 1 << 7;
const QTD_IOC: u32 = 1 << 15;
const QTD_PID_OUT: u32 = 0 << 8;
const QTD_PID_IN: u32 = 1 << 8;
const QTD_PID_SETUP: u32 = 2 << 8;
const QTD_DT: u32 = 1 << 31;
const QH_H: u32 = 1 << 15; // head of reclamation list
const QH_EPS_FULL: u32 = 0 << 12;
const QH_EPS_HIGH: u32 = 2 << 12;

unsafe fn qtd_clear(qtd: usize) {
    let p = qtd as *mut u32;
    let mut i = 0usize;
    while i < 8 {
        *p.add(i) = 0;
        i += 1;
    }
}

unsafe fn qh_clear(qh: usize) {
    let p = qh as *mut u32;
    let mut i = 0usize;
    while i < 16 {
        *p.add(i) = 0;
        i += 1;
    }
}

/// Wait until qTD Active bit clears (with timeout). Returns token dword.
unsafe fn wait_qtd_token(qtd: usize, max_spins: u32) -> u32 {
    let mut spins = 0u32;
    let mut token = 0u32;
    while spins < max_spins {
        token = core::ptr::read_volatile((qtd + 8) as *const u32);
        if (token & QTD_ACTIVE) == 0 {
            return token;
        }
        spins = spins.wrapping_add(1);
        core::hint::spin_loop();
    }
    token // still active = timeout
}

unsafe fn async_stop() {
    let mut cmd = r32(OP, 0x00);
    if (cmd & 0x20) != 0 {
        w32(OP, 0x00, cmd & !0x20); // clear ASE
        let mut s = 0u32;
        while s < 1_000_000 {
            // USBSTS bit15 ASS: 1=async on
            if (r32(OP, 0x04) & (1 << 15)) == 0 {
                break;
            }
            s = s.wrapping_add(1);
        }
    }
}

unsafe fn async_start() {
    let mut cmd = r32(OP, 0x00);
    w32(OP, 0x00, cmd | 0x01 | 0x20); // RUN|ASE
    let mut s = 0u32;
    while s < 1_000_000 {
        if (r32(OP, 0x04) & (1 << 15)) != 0 {
            break;
        }
        s = s.wrapping_add(1);
    }
}

/// Single control transfer - Linux-style: stop async, build QH+qTDs, start, wait.
/// Uses separate XFER_QH linked from ASYNC_QH head (H-bit dummy).
unsafe fn control_xfer(
    addr: u8,
    setup: &[u8; 8],
    data: *mut u8,
    data_len: usize,
    data_in: bool,
) -> bool {
    if ASYNC_QH == 0 || QTD_POOL == 0 || BUF == 0 {
        return false;
    }
    // Prefer dedicated xfer QH; fall back to offset in pool
    let head = ASYNC_QH;
    let qh = if XFER_QH != 0 { XFER_QH } else { ASYNC_QH + 64 };
    let qtd0 = QTD_POOL;
    let qtd1 = QTD_POOL + 32;
    let qtd2 = QTD_POOL + 64;
    let setup_buf = BUF;
    let data_buf = BUF + 64;

    // Copy SETUP / data out
    let mut i = 0usize;
    while i < 8 {
        *((setup_buf + i) as *mut u8) = setup[i];
        i += 1;
    }
    if data_len > 0 && !data.is_null() && !data_in {
        i = 0;
        while i < data_len && i < 512 {
            *((data_buf + i) as *mut u8) = *data.add(i);
            i += 1;
        }
    }
    if data_len > 0 && data_in {
        i = 0;
        while i < data_len && i < 512 {
            *((data_buf + i) as *mut u8) = 0;
            i += 1;
        }
    }

    async_stop();

    qtd_clear(qtd0);
    qtd_clear(qtd1);
    qtd_clear(qtd2);
    qh_clear(qh);

    // --- SETUP qTD (DATA0, 8 bytes) ---
    let setup_token = QTD_ACTIVE | QTD_PID_SETUP | ((8u32) << 16) | (3 << 10);
    *((qtd0 + 8) as *mut u32) = setup_token;
    *((qtd0 + 12) as *mut u32) = setup_buf as u32;
    *((qtd0 + 4) as *mut u32) = 1; // alt next terminate

    let mut last = qtd0;
    if data_len > 0 {
        *(qtd0 as *mut u32) = qtd1 as u32; // T=0
        let pid = if data_in { QTD_PID_IN } else { QTD_PID_OUT };
        // DATA1 toggle
        let tok = QTD_ACTIVE | pid | QTD_DT | ((data_len as u32) << 16) | (3 << 10);
        *((qtd1 + 8) as *mut u32) = tok;
        *((qtd1 + 12) as *mut u32) = data_buf as u32;
        *((qtd1 + 4) as *mut u32) = 1;
        *(qtd1 as *mut u32) = 1; // term for now
        last = qtd1;
    } else {
        *(qtd0 as *mut u32) = 1;
    }

    // STATUS stage: opposite PID, DATA1, 0 length
    let status_pid = if data_len > 0 && data_in {
        QTD_PID_OUT
    } else {
        QTD_PID_IN
    };
    let status_tok = QTD_ACTIVE | status_pid | QTD_DT | QTD_IOC | (3 << 10); // 0 bytes
    *((qtd2 + 8) as *mut u32) = status_tok;
    *((qtd2 + 12) as *mut u32) = 0;
    *(qtd2 as *mut u32) = 1;
    *((qtd2 + 4) as *mut u32) = 1;
    *(last as *mut u32) = qtd2 as u32;

    // --- XFER QH ---
    let eps = DEV_EPS;
    // EP0 default MPS: HS=64, FS/LS=8 (USB2.0 §5.5.3) before reading bMaxPacketSize0
    let mps0: u32 = if eps == QH_EPS_HIGH { 64 } else { 8 };
    *(qh as *mut u32) = (head as u32) | (1 << 1);
    let epc = (addr as u32)
        | (0u32 << 8)
        | eps
        | (mps0 << 16)
        | (1 << 14); // DTC
    *((qh + 4) as *mut u32) = epc;
    *((qh + 8) as *mut u32) = 1 << 30; // Mult=1
    *((qh + 12) as *mut u32) = 0; // current qTD
    *((qh + 16) as *mut u32) = qtd0 as u32; // next qTD overlay
    *((qh + 20) as *mut u32) = 1; // alt
    // zero rest of overlay (token etc.) so Active is clear until HC loads qTD
    let mut o = 6usize;
    while o < 12 {
        *((qh + o * 4) as *mut u32) = 0;
        o += 1;
    }

    // --- HEAD QH (dummy, H-bit) ---
    qh_clear(head);
    *(head as *mut u32) = (qh as u32) | (1 << 1); // link to xfer QH
    *((head + 4) as *mut u32) = QH_H | (64 << 16) | (1 << 14);
    *((head + 8) as *mut u32) = 1 << 30;
    *((head + 16) as *mut u32) = 1; // no qTD on head
    *((head + 20) as *mut u32) = 1;

    w32(OP, 0x18, head as u32); // ASYNCLISTADDR
    async_start();

    // Doorbell: Interrupt on Async Advance Doorbell
    w32(OP, 0x00, r32(OP, 0x00) | (1 << 6));
    // Wait IAA in USBSTS bit5
    let mut s = 0u32;
    while s < 500_000 {
        if (r32(OP, 0x04) & (1 << 5)) != 0 {
            w32(OP, 0x04, 1 << 5); // W1C clear IAA
            break;
        }
        s = s.wrapping_add(1);
    }

    // Wait STATUS qTD (last)
    let tok2 = wait_qtd_token(qtd2, 8_000_000);
    let tok0 = core::ptr::read_volatile((qtd0 + 8) as *const u32);
    let tok1 = core::ptr::read_volatile((qtd1 + 8) as *const u32);
    LAST_QTD0 = tok0;
    LAST_QTD1 = tok1;
    LAST_QTD2 = tok2;
    LAST_USBSTS = r32(OP, 0x04);

    async_stop();

    // Restore idle head
    qh_clear(head);
    *(head as *mut u32) = (head as u32) | (1 << 1);
    *((head + 4) as *mut u32) = QH_H | (64 << 16) | (1 << 14);
    *((head + 8) as *mut u32) = 1 << 30;
    *((head + 16) as *mut u32) = 1;
    w32(OP, 0x18, head as u32);
    async_start();

    // Success: status not active, not halted
    if (tok2 & QTD_ACTIVE) != 0 {
        return false;
    }
    if (tok2 & (1 << 6)) != 0 {
        return false; // halted
    }
    // SETUP should also have completed
    if (tok0 & QTD_ACTIVE) != 0 || (tok0 & (1 << 6)) != 0 {
        return false;
    }

    if data_len > 0 && data_in && !data.is_null() {
        i = 0;
        while i < data_len && i < 512 {
            *data.add(i) = *((data_buf + i) as *const u8);
            i += 1;
        }
    }
    true
}

/// Returns: 0=fail, 1=full/low (no PE), 2=high-speed (PE+HSP)
unsafe fn port_reset_speed(port: u8) -> u8 {
    let off = 0x44 + (port as usize) * 4;
    let mut ps = r32(OP, off);
    // ensure power
    if (ps & (1 << 12)) == 0 {
        w32(OP, off, ps | (1 << 12));
        delay(100_000);
        ps = r32(OP, off);
    }
    // clear Port Owner so we own it
    if (ps & (1 << 13)) != 0 {
        w32(OP, off, ps & !(1 << 13));
        delay(50_000);
        ps = r32(OP, off);
    }
    if (ps & 1) == 0 {
        return 0;
    }
    // Port Reset, clear PE
    w32(OP, off, (ps | (1 << 8) | (1 << 12)) & !(1 << 2));
    delay(1_000_000);
    ps = r32(OP, off);
    w32(OP, off, (ps & !(1 << 8)) | (1 << 12));
    delay(300_000);
    // Wait for PE (HS) or settle FS
    let mut spins = 0u32;
    while spins < 2_000_000 {
        ps = r32(OP, off);
        if (ps & (1 << 2)) != 0 {
            // PE set - prefer HSP for true HS
            if (ps & (1 << 9)) != 0 {
                DEV_EPS = QH_EPS_HIGH;
                LAST_SPD = 2;
                return 2;
            }
            // PE without HSP: still try as HS (some silicon)
            DEV_EPS = QH_EPS_HIGH;
            LAST_SPD = 2;
            return 2;
        }
        spins = spins.wrapping_add(1);
    }
    ps = r32(OP, off);
    // Still connected? Full/Low speed - EHCI root cannot talk without companion/TT
    if (ps & 1) != 0 {
        DEV_EPS = QH_EPS_FULL;
        LAST_SPD = 1;
        let ls = (ps >> 10) & 3;
        serial::write_str("[EHCI] FS/LS PORTSC=");
        serial::write_hex(ps as usize);
        serial::write_str(" LS=");
        serial::write_usize(ls as usize);
        serial::write_str(" - no companion, skip EHCI control\n");
        // Do NOT force PE: EHCI root cannot do FS without TT/companion
        return 1;
    }
    0
}

unsafe fn port_reset(port: u8) -> bool {
    port_reset_speed(port) != 0
}

unsafe fn setup_periodic_mouse() {
    // QH for interrupt IN EP1, addr DEV_ADDR
    let qh = PERIOD_QH;
    let qtd = QTD_POOL + 96;
    let report = BUF + 256;
    qh_clear(qh);
    qtd_clear(qtd);
    let mut i = 0usize;
    while i < 8 {
        *((report + i) as *mut u8) = 0;
        i += 1;
    }

    // qTD: IN, Active, length MPS
    let tok = QTD_ACTIVE | QTD_PID_IN | QTD_IOC | ((MPS as u32) << 16) | (3 << 10);
    *((qtd + 8) as *mut u32) = tok;
    *((qtd + 12) as *mut u32) = report as u32;
    *(qtd as *mut u32) = 1;
    *((qtd + 4) as *mut u32) = 1;

    // QH: S-mask every frame bit0, EPS full, max pkt MPS
    *(qh as *mut u32) = 1; // terminate horiz (only one)
    let epc = (DEV_ADDR as u32)
        | ((EP_IN as u32) << 8)
        | QH_EPS_FULL
        | ((MPS as u32) << 16)
        | (1 << 14);
    *((qh + 4) as *mut u32) = epc;
    // S-mask = 0x01 (microframe 0), C-mask 0 for full-speed
    *((qh + 8) as *mut u32) = 0x0001 | (1 << 30);
    *((qh + 16) as *mut u32) = qtd as u32;
    *((qh + 20) as *mut u32) = 1;

    // Frame list: link QH type=1 every 8 frames
    let fl = FRAMELIST as *mut u32;
    i = 0;
    while i < 1024 {
        if (i % 8) == 0 {
            *fl.add(i) = (qh as u32) | (1 << 1); // QH type
        } else {
            *fl.add(i) = 1; // terminate
        }
        i += 1;
    }

    w32(OP, 0x14, FRAMELIST as u32); // PERIODICLISTBASE
    let cmd = r32(OP, 0x00);
    w32(OP, 0x00, cmd | 0x11); // RUN + PSE
}

unsafe fn requeue_interrupt() {
    let qtd = QTD_POOL + 96;
    let report = BUF + 256;
    let mut i = 0usize;
    while i < 8 {
        *((report + i) as *mut u8) = 0;
        i += 1;
    }
    qtd_clear(qtd);
    let tok = QTD_ACTIVE | QTD_PID_IN | QTD_IOC | ((MPS as u32) << 16) | (3 << 10);
    *((qtd + 8) as *mut u32) = tok;
    *((qtd + 12) as *mut u32) = report as u32;
    *(qtd as *mut u32) = 1;
    *((qtd + 4) as *mut u32) = 1;
    // overlay next on period QH
    *((PERIOD_QH + 16) as *mut u32) = qtd as u32;
    *((PERIOD_QH + 20) as *mut u32) = 1;
    // clear active overlay token so HC fetches next
    *((PERIOD_QH + 24) as *mut u32) = 0;
}

fn init_one(bus: u8, dev: u8, func: u8) -> bool {
    unsafe {
        let bar = pci_r32(bus, dev, func, 0x10);
        if (bar & 1) != 0 {
            return false; // IO BAR unsupported
        }
        let phys = (bar & 0xFFFF_F000) as usize;
        if phys == 0 {
            return false;
        }
        // Enable BM + MMIO
        let cmd = pci_r32(bus, dev, func, 0x04);
        pci_w16(bus, dev, func, 0x04, (cmd as u16) | 0x06);

        if !map_mmio(phys, 0x1000) {
            serial::write_str("[EHCI] MMIO map fail\n");
            return false;
        }
        MMIO = phys;
        let cap = r8(phys, 0) as usize;
        OP = phys + cap;
        // BIOS handoff via EECP (USBLEGSUP) - required on many laptops
        {
            let hccp = r32(phys, 0x08);
            let mut eecp = ((hccp >> 8) & 0xFF) as u8;
            let mut hops = 0u8;
            while eecp != 0 && eecp != 0xFF && hops < 8 {
                let cap_id = pci_r32(bus, dev, func, eecp) & 0xFF;
                if cap_id == 0x01 {
                    // USBLEGSUP
                    let mut leg = pci_r32(bus, dev, func, eecp);
                    serial::write_str("[EHCI] USBLEGSUP=");
                    serial::write_hex(leg as usize);
                    serial::write_str("\n");
                    // Set OS Owned (bit 24)
                    leg |= 1 << 24;
                    pci_w32(bus, dev, func, eecp, leg);
                    let mut w = 0u32;
                    while w < 2_000_000 {
                        leg = pci_r32(bus, dev, func, eecp);
                        // BIOS Owned bit 16 cleared?
                        if (leg & (1 << 16)) == 0 {
                            break;
                        }
                        w = w.wrapping_add(1);
                    }
                    serial::write_str("[EHCI] handoff LEG=");
                    serial::write_hex(leg as usize);
                    serial::write_str("\n");
                    // Disable SMI in USBLEGCTLSTS (EECP+4)
                    let mut legctl = pci_r32(bus, dev, func, eecp.wrapping_add(4));
                    legctl = 0; // clear SMI enables
                    pci_w32(bus, dev, func, eecp.wrapping_add(4), legctl);
                    break;
                }
                eecp = ((pci_r32(bus, dev, func, eecp) >> 8) & 0xFF) as u8;
                hops = hops.wrapping_add(1);
            }
        }
        let hcs = r32(phys, 0x04);
        NPORTS = (hcs & 0xF) as u8;
        if NPORTS == 0 {
            NPORTS = 8;
        }

        serial::write_str("[EHCI] BAR=");
        serial::write_hex(phys);
        serial::write_str(" OP=");
        serial::write_hex(OP);
        serial::write_str(" ports=");
        serial::write_usize(NPORTS as usize);
        serial::write_str("\n");

        // Halt if running
        let mut cmd = r32(OP, 0x00);
        if (cmd & 1) != 0 {
            w32(OP, 0x00, cmd & !1);
            let mut s = 0u32;
            while s < 1_000_000 {
                if (r32(OP, 0x04) & 1) != 0 {
                    break;
                }
                s = s.wrapping_add(1);
            }
        }
        // Reset HC
        w32(OP, 0x00, 2);
        let mut s = 0u32;
        while s < 2_000_000 {
            if (r32(OP, 0x00) & 2) == 0 {
                break;
            }
            s = s.wrapping_add(1);
        }

        // CTRLDSSEGMENT = 0 (32-bit only)
        w32(OP, 0x10, 0);
        // Take port ownership
        w32(OP, 0x40, 1); // CONFIGFLAG
        delay(50_000);
        // Power all ports (PP bit12) - without PP, CCS stays 0
        {
            let mut pi = 0u8;
            while pi < NPORTS {
                let off = 0x44 + (pi as usize) * 4;
                let mut ps = r32(OP, off);
                if (ps & (1 << 12)) == 0 {
                    w32(OP, off, ps | (1 << 12));
                }
                // Clear Port Owner so EHCI keeps the port (bit13)
                ps = r32(OP, off);
                if (ps & (1 << 13)) != 0 {
                    w32(OP, off, ps & !(1 << 13));
                }
                pi = pi.wrapping_add(1);
            }
            delay(200_000);
        }


        // Allocate DMA structures (below 4G, identity-mapped)
        let fl = match mm::alloc_pages(1) {
            Some(p) => p,
            None => return false,
        };
        let pool = match mm::alloc_pages(2) {
            Some(p) => p,
            None => return false,
        };
        mm::zero_pages(fl, 1);
        mm::zero_pages(pool, 2);
        FRAMELIST = fl;
        ASYNC_QH = pool; // head QH (64B)
        XFER_QH = pool + 64; // transfer QH
        PERIOD_QH = pool + 128;
        QTD_POOL = pool + 256;
        BUF = pool + 512;

        // Async list head (dummy H-bit, self-linked)
        qh_clear(ASYNC_QH);
        qh_clear(XFER_QH);
        *(ASYNC_QH as *mut u32) = (ASYNC_QH as u32) | (1 << 1);
        *((ASYNC_QH + 4) as *mut u32) = QH_H | (64 << 16) | (1 << 14);
        *((ASYNC_QH + 8) as *mut u32) = 1 << 30;
        *((ASYNC_QH + 16) as *mut u32) = 1;
        w32(OP, 0x18, ASYNC_QH as u32);
        w32(OP, 0x08, 0); // USBINTR off
        w32(OP, 0x00, 0x21); // RUN | ASE
        delay(80_000);

        DIAG_OK = true;
        // Power ports again + settle (CCS can lag)
        {
            let mut pi = 0u8;
            while pi < NPORTS {
                let off = 0x44 + (pi as usize) * 4;
                let ps = r32(OP, off);
                w32(OP, off, ps | (1 << 12));
                pi = pi.wrapping_add(1);
            }
            delay(500_000);
        }

        // Find connected port
        let mut p = 0u8;
        let mut found_port = 0xFFu8;
        while p < NPORTS {
            let ps = r32(OP, 0x44 + (p as usize) * 4);
            if (ps & 1) != 0 {
                found_port = p;
                serial::write_str("[EHCI] port ");
                serial::write_usize(p as usize);
                serial::write_str(" CCS PORTSC=");
                serial::write_hex(ps as usize);
                serial::write_str("\n");
                break;
            }
            p = p.wrapping_add(1);
        }
        if found_port == 0xFF {
            serial::write_str("[EHCI] no device on any port\n");
            return true; // HC ok, no mouse yet
        }
        PORT = found_port;
        let spd = port_reset_speed(found_port);
        if spd == 0 {
            serial::write_str("[EHCI] port reset fail\n");
            LAST_ERR = 2;
            return true;
        }
        DIAG_DEV = true;
        LAST_ERR = 0;
        if spd == 1 {
            LAST_ERR = 6;
            serial::write_str("[EHCI] FS device on EHCI root - use USB3/xHCI port\n");
            return true;
        }

        // GET_DESCRIPTOR device
        let mut setup = [0u8; 8];
        setup[0] = 0x80;
        setup[1] = 0x06;
        setup[2] = 0x00;
        setup[3] = 0x01;
        setup[6] = 18;
        let mut desc = [0u8; 18];
        if !control_xfer(0, &setup, desc.as_mut_ptr(), 18, true) {
            setup[6] = 8;
            if !control_xfer(0, &setup, desc.as_mut_ptr(), 8, true) {
                serial::write_str("[EHCI] GET_DESC fail spd=");
                serial::write_usize(spd as usize);
                serial::write_str("\n");
                LAST_ERR = 3;
                return true;
            }
        }
        LAST_VID = u16::from_le_bytes([desc[8], desc[9]]);
        LAST_PID = u16::from_le_bytes([desc[10], desc[11]]);
        LAST_CLASS = if desc[0] >= 5 { desc[4] } else { 0 };
        serial::write_str("[EHCI] VID=");
        serial::write_hex(LAST_VID as usize);
        serial::write_str(" PID=");
        serial::write_hex(LAST_PID as usize);
        serial::write_str(" CLASS=");
        serial::write_hex(LAST_CLASS as usize);
        serial::write_str(" SPD=");
        serial::write_usize(spd as usize);
        serial::write_str("\n");
        if spd == 1 {
            // FS/LS on EHCI root without companion - mark error but continue try
            LAST_ERR = 6;
        }
        // SET_ADDRESS 1
        setup = [0u8; 8];
        setup[0] = 0x00;
        setup[1] = 0x05;
        setup[2] = 1; // address 1
        if !control_xfer(0, &setup, core::ptr::null_mut(), 0, false) {
            serial::write_str("[EHCI] SET_ADDR fail\n");
            return true;
        }
        DEV_ADDR = 1;
        delay(50_000);

        // GET config descriptor (partial)
        setup = [0u8; 8];
        setup[0] = 0x80;
        setup[1] = 0x06;
        setup[3] = 0x02; // CONFIGURATION
        setup[6] = 32;
        let mut cfg = [0u8; 64];
        let _ = control_xfer(1, &setup, cfg.as_mut_ptr(), 32, true);

        // SET_CONFIGURATION 1
        setup = [0u8; 8];
        setup[1] = 0x09;
        setup[2] = 1;
        if !control_xfer(1, &setup, core::ptr::null_mut(), 0, false) {
            serial::write_str("[EHCI] SET_CONFIG fail\n");
            return true;
        }

        // --- Smartphone / MSC / MTP on USB2 (EHCI) ---
        // Fetch larger config if possible
        setup = [0u8; 8];
        setup[0] = 0x80;
        setup[1] = 0x06;
        setup[3] = 0x02;
        setup[6] = 64;
        let _ = control_xfer(1, &setup, cfg.as_mut_ptr(), 64, true);
        let cfg_slice = &cfg[..];
        if let Some((mode, if_num, _proto, ep_in_a, ep_out_a)) =
            crate::drivers::usb_phone::find_phone_interface(cfg_slice)
        {
            serial::write_str("[EHCI] PHONE class detected mode=");
            serial::write_str(crate::drivers::usb_phone::mode_name(mode));
            serial::write_str("\n");
            crate::drivers::usb_phone::register_device(
                mode,
                LAST_VID,
                LAST_PID,
                0, // slot n/a on EHCI path
                found_port,
                (ep_in_a & 0x0F) as u8,
                (ep_out_a & 0x0F) as u8,
                512,
                512,
            );
            // MSC/MTP bulk transfers need more EHCI QH work; detection is step 1
            serial::write_str("[EHCI] phone REGISTERED (USB2 path) - use PH\n");
            return true;
        }

        // SET_PROTOCOL Boot (HID class) interface 0
        setup = [0u8; 8];
        setup[0] = 0x21; // class, interface
        setup[1] = 0x0B; // SET_PROTOCOL
        setup[2] = 0; // boot
        setup[4] = 0; // interface
        let _ = control_xfer(1, &setup, core::ptr::null_mut(), 0, false);

        // SET_IDLE
        setup = [0u8; 8];
        setup[0] = 0x21;
        setup[1] = 0x0A;
        let _ = control_xfer(1, &setup, core::ptr::null_mut(), 0, false);

        EP_IN = 1;
        MPS = 4;
        // Parse config for interrupt IN if possible
        if cfg[0] >= 9 {
            let mut off = 0usize;
            while off + 2 < 32 {
                let len = cfg[off] as usize;
                let ty = cfg[off + 1];
                if len < 2 {
                    break;
                }
                if ty == 0x05 && len >= 7 {
                    // endpoint
                    let addr = cfg[off + 2];
                    let attr = cfg[off + 3];
                    if (addr & 0x80) != 0 && (attr & 3) == 3 {
                        EP_IN = addr & 0x0F;
                        MPS = u16::from_le_bytes([cfg[off + 4], cfg[off + 5]]);
                        if MPS == 0 {
                            MPS = 4;
                        }
                        if MPS > 8 {
                            MPS = 8;
                        }
                        break;
                    }
                }
                off += len;
            }
        }

        DIAG_HID = true;
        setup_periodic_mouse();
        LIVE = true;
        serial::write_str("[EHCI] HID mouse LIVE addr=");
        serial::write_usize(DEV_ADDR as usize);
        serial::write_str(" ep=");
        serial::write_usize(EP_IN as usize);
        serial::write_str(" mps=");
        serial::write_usize(MPS as usize);
        serial::write_str("\n");
        true
    }
}

pub fn probe() {
    serial::write_str("\n[EHCI] scan...\n");
    unsafe {
        DIAG_OK = false;
        DIAG_DEV = false;
        DIAG_HID = false;
        DIAG_REPORTS = 0;
        DIAG_EVENTS = 0;
        LIVE = false;
        MMIO = 0;
        OP = 0;
    }
    let mut bus = 0u8;
    while bus < 16 {
        let mut dev = 0u8;
        while dev < 32 {
            let mut func = 0u8;
            while func < 8 {
                let id = unsafe { pci_r32(bus, dev, func, 0) };
                if id != 0xFFFF_FFFF && id != 0 {
                    let cl = unsafe { pci_r32(bus, dev, func, 0x08) };
                    let class = ((cl >> 24) & 0xFF) as u8;
                    let sub = ((cl >> 16) & 0xFF) as u8;
                    let prog = ((cl >> 8) & 0xFF) as u8;
                    // USB EHCI: 0C 03 20
                    if class == 0x0C && sub == 0x03 && prog == 0x20 {
                        serial::write_str("[EHCI] found ");
                        serial::write_hex(bus as usize);
                        serial::write_str(":");
                        serial::write_hex(dev as usize);
                        serial::write_str(".");
                        serial::write_hex(func as usize);
                        serial::write_str(" VID:DID=");
                        serial::write_hex((id & 0xFFFF) as usize);
                        serial::write_str(":");
                        serial::write_hex(((id >> 16) & 0xFFFF) as usize);
                        serial::write_str("\n");
                        if init_one(bus, dev, func) {
                            return;
                        }
                    }
                }
                // multi-function check
                if func == 0 {
                    let ht = unsafe { pci_r32(bus, dev, 0, 0x0C) };
                    if ((ht >> 16) & 0x80) == 0 {
                        break;
                    }
                }
                func = func.wrapping_add(1);
            }
            dev = dev.wrapping_add(1);
        }
        bus = bus.wrapping_add(1);
    }
    serial::write_str("[EHCI] no controller\n");
}

/// Non-blocking: if interrupt qTD completed, feed HID report and requeue.
pub fn poll_mouse_live() {
    unsafe {
        if !LIVE || PERIOD_QH == 0 {
            return;
        }
        let qtd = QTD_POOL + 96;
        let token = core::ptr::read_volatile((qtd + 8) as *const u32);
        if (token & QTD_ACTIVE) != 0 {
            return;
        }
        // completed
        if (token & (1 << 6)) != 0 {
            // halted - requeue
            requeue_interrupt();
            return;
        }
        let report_addr = BUF + 256;
        let mut report = [0u8; 8];
        let mut i = 0usize;
        while i < 8 {
            report[i] = *((report_addr + i) as *const u8);
            i += 1;
        }
        DIAG_REPORTS = DIAG_REPORTS.wrapping_add(1);
        if report[0] != 0 || report[1] != 0 || report[2] != 0 {
            crate::input::process_hid_boot_mouse(&report);
            DIAG_EVENTS = DIAG_EVENTS.wrapping_add(1);
        }
        requeue_interrupt();
    }
}

pub fn rescan() {
    unsafe {
        if DIAG_OK && OP != 0 {
            // Soft re-enum: keep HC, re-scan ports
            LIVE = false;
            DIAG_HID = false;
            DIAG_DEV = false;
            LAST_ERR = 0;
            LAST_VID = 0;
            LAST_PID = 0;
            LAST_CLASS = 0;
            power_and_rescan_ports();
            delay(400_000);
            let mut p = 0u8;
            let mut found = 0xFFu8;
            while p < NPORTS {
                let ps = r32(OP, 0x44 + (p as usize) * 4);
                if (ps & 1) != 0 {
                    found = p;
                    break;
                }
                p = p.wrapping_add(1);
            }
            if found == 0xFF {
                LAST_ERR = 1;
                serial::write_str("[EHCI] soft rescan: no CCS\n");
                return;
            }
            PORT = found;
            // Fall through by calling internal path via full probe for reliability
        }
    }
    probe();
}


/// Write short PORTSC dump into caller buffer via callbacks is hard; print via serial + expose CCS mask.
pub fn power_and_rescan_ports() {
    unsafe {
        if OP == 0 || !DIAG_OK {
            return;
        }
        let mut p = 0u8;
        while p < NPORTS {
            let off = 0x44 + (p as usize) * 4;
            let mut ps = r32(OP, off);
            w32(OP, off, ps | (1 << 12)); // PP
            ps = r32(OP, off);
            if (ps & (1 << 13)) != 0 {
                w32(OP, off, ps & !(1 << 13)); // clear PO
            }
            p = p.wrapping_add(1);
        }
        // short settle
        let mut s = 0u32;
        while s < 300_000 {
            s = s.wrapping_add(1);
            core::hint::spin_loop();
        }
    }
}

pub fn port_ccs_mask() -> u32 {
    unsafe {
        if OP == 0 {
            return 0;
        }
        let mut mask = 0u32;
        let mut p = 0u8;
        while p < NPORTS {
            let ps = r32(OP, 0x44 + (p as usize) * 4);
            if (ps & 1) != 0 {
                mask |= 1 << p;
            }
            p = p.wrapping_add(1);
        }
        mask
    }
}

pub fn port_count() -> u8 {
    unsafe { NPORTS }
}

pub fn port_status(p: u8) -> u32 {
    unsafe {
        if OP == 0 || p >= NPORTS {
            return 0;
        }
        r32(OP, 0x44 + (p as usize) * 4)
    }
}


static mut EHCI_SCAN: [u8; 2048] = [0; 2048];
static mut EHCI_SCAN_LEN: usize = 0;
fn eapp(s: &[u8]) {
    unsafe {
        let mut i = 0usize;
        while i < s.len() && EHCI_SCAN_LEN < 2047 {
            EHCI_SCAN[EHCI_SCAN_LEN] = s[i];
            EHCI_SCAN_LEN += 1;
            i += 1;
        }
    }
}
pub fn scan_usb_detail() -> &'static [u8] {
    unsafe { EHCI_SCAN_LEN = 0; }
    eapp(b"======== SCANUSB EHCI (USB2) ========\n");
    if !diag_ok() {
        eapp(b"EHCI: controller not ready\n");
        eapp(b"======== END EHCI ========\n");
        return unsafe { &EHCI_SCAN[..EHCI_SCAN_LEN] };
    }
    power_and_rescan_ports();
    let n = port_count();
    eapp(b"EHCI ports=");
    eapp(&[b'0' + (n % 10)]);
    eapp(b"\n");
    let mut p = 0u8;
    while p < n && p < 16 {
        let ps = port_status(p);
        eapp(b"--- EHCI Port ");
        eapp(&[b'0' + (p % 10)]);
        eapp(b" ---\n  PORTSC=");
        // hex
        let mut v = ps;
        eapp(b"0x");
        let mut i = 0u32;
        while i < 8 {
            let nib = ((v >> (28 - i * 4)) & 0xF) as u8;
            eapp(&[if nib < 10 { b'0' + nib } else { b'A' + nib - 10 }]);
            i += 1;
        }
        eapp(b"\n  CCS=");
        eapp(if (ps & 1) != 0 { b"YES" } else { b"no" });
        eapp(b" PED=");
        eapp(if (ps & 2) != 0 { b"YES" } else { b"no" });
        eapp(b" PP=");
        eapp(if (ps & (1 << 12)) != 0 { b"YES" } else { b"no" });
        eapp(b" OWNER=");
        eapp(if (ps & (1 << 13)) != 0 { b"companion" } else { b"EHCI" });
        eapp(b"\n");
        p = p.wrapping_add(1);
    }
    eapp(b"last VID=");
    // skip verbose
    eapp(b" class path: phone detect after enum\n");
    eapp(b"======== END EHCI ========\n");
    serial::write_str(unsafe { core::str::from_utf8_unchecked(&EHCI_SCAN[..EHCI_SCAN_LEN]) });
    unsafe { &EHCI_SCAN[..EHCI_SCAN_LEN] }
}
