//! Minimal UHCI host for USB Full-Speed HID Boot Mouse (Intel companion to EHCI).
//! Typical path on AH532: mouse is FS → not owned by EHCI → UHCI/OHCI companion.

use crate::mm;
use crate::serial;

static mut DIAG_OK: bool = false;
static mut DIAG_DEV: bool = false;
static mut DIAG_HID: bool = false;
static mut DIAG_REPORTS: u32 = 0;
static mut DIAG_EVENTS: u32 = 0;
static mut LIVE: bool = false;

static mut IO_BASE: u16 = 0;
static mut FRAMELIST: usize = 0;
static mut TD_POOL: usize = 0;
static mut BUF: usize = 0;
static mut DEV_ADDR: u8 = 0;
static mut EP_IN: u8 = 1;
static mut MPS: u16 = 4;
static mut INTR_TD: usize = 0;
static mut REPORT: usize = 0;

pub fn diag_ok() -> bool { unsafe { DIAG_OK } }
pub fn diag_dev() -> bool { unsafe { DIAG_DEV } }
pub fn diag_hid() -> bool { unsafe { DIAG_HID } }
pub fn diag_reports() -> u32 { unsafe { DIAG_REPORTS } }
pub fn diag_events() -> u32 { unsafe { DIAG_EVENTS } }
pub fn diag_live() -> bool { unsafe { LIVE } }

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
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
}
unsafe fn inw(port: u16) -> u16 {
    let v: u16;
    core::arch::asm!("in ax, dx", in("dx") port, out("ax") v, options(nostack, preserves_flags));
    v
}
unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nostack, preserves_flags));
}

fn delay(n: u32) {
    let mut i = 0u32;
    while i < n {
        i = i.wrapping_add(1);
        core::hint::spin_loop();
    }
}

// UHCI TD status bits
const TD_ACTIVE: u32 = 1 << 23;
const TD_IOC: u32 = 1 << 24;
const TD_IOS: u32 = 1 << 25; // isochronous - not used
const TD_LS: u32 = 1 << 26;
const TD_SPD: u32 = 1 << 29;

fn td_write(td: usize, link: u32, cs: u32, token: u32, buffer: u32) {
    unsafe {
        core::ptr::write_volatile(td as *mut u32, link);
        core::ptr::write_volatile((td + 4) as *mut u32, cs);
        core::ptr::write_volatile((td + 8) as *mut u32, token);
        core::ptr::write_volatile((td + 12) as *mut u32, buffer);
    }
}

fn td_active(td: usize) -> bool {
    unsafe {
        let cs = core::ptr::read_volatile((td + 4) as *const u32);
        (cs & TD_ACTIVE) != 0
    }
}

fn wait_td(td: usize, max: u32) -> bool {
    let mut s = 0u32;
    while s < max {
        if !td_active(td) {
            let cs = unsafe { core::ptr::read_volatile((td + 4) as *const u32) };
            // stalled?
            if (cs & (1 << 22)) != 0 {
                return false;
            }
            return true;
        }
        s = s.wrapping_add(1);
        core::hint::spin_loop();
    }
    false
}

/// Token: maxLen | toggle | endpoint | device | pid
fn make_token(pid: u8, addr: u8, ep: u8, toggle: u8, max_len: u16) -> u32 {
    let len = if max_len == 0 {
        0x7FF
    } else {
        (max_len as u32 - 1) & 0x7FF
    };
    ((len) << 21) | ((toggle as u32) << 19) | ((ep as u32) << 15) | ((addr as u32) << 8) | (pid as u32)
}

const PID_SETUP: u8 = 0x2D;
const PID_IN: u8 = 0x69;
const PID_OUT: u8 = 0xE1;

unsafe fn control_xfer(addr: u8, setup: &[u8; 8], data: *mut u8, data_len: usize, data_in: bool) -> bool {
    let td0 = TD_POOL;
    let td1 = TD_POOL + 16;
    let td2 = TD_POOL + 32;
    let setup_buf = BUF;
    let data_buf = BUF + 64;

    let mut i = 0usize;
    while i < 8 {
        *((setup_buf + i) as *mut u8) = setup[i];
        i += 1;
    }
    if data_len > 0 && !data.is_null() && !data_in {
        i = 0;
        while i < data_len.min(64) {
            *((data_buf + i) as *mut u8) = *data.add(i);
            i += 1;
        }
    }

    // SETUP
    let t0 = make_token(PID_SETUP, addr, 0, 0, 8);
    td_write(td0, (td1 as u32) | 4, TD_ACTIVE | (3 << 27), t0, setup_buf as u32); // depth first bit2=0 means breadth? bit2=1 VF depth
    // UHCI link: bit0 T, bit1 Q, bit2 Depth/Breadth. Use depth-first (bit2=1) for control sequence.
    // Actually: bit 2 = 1 means depth first (process this TD's vertical before horizontal).

    if data_len > 0 {
        let pid = if data_in { PID_IN } else { PID_OUT };
        let t1 = make_token(pid, addr, 0, 1, data_len as u16);
        let next = (td2 as u32) | 4;
        td_write(td1, next, TD_ACTIVE | (3 << 27), t1, data_buf as u32);
    } else {
        // no data: td0 -> td2
        td_write(td0, (td2 as u32) | 4, TD_ACTIVE | (3 << 27), t0, setup_buf as u32);
    }

    // STATUS: opposite of data
    let st_pid = if data_len > 0 && data_in { PID_OUT } else { PID_IN };
    let t2 = make_token(st_pid, addr, 0, 1, 0); // 0 length
    td_write(td2, 1, TD_ACTIVE | TD_IOC | (3 << 27), t2, 0);

    // Insert into frame list queue head (low latency): pointer to first TD
    let fl = FRAMELIST as *mut u32;
    let first = if data_len > 0 { td0 } else { td0 };
    // Put in several frames so HC sees it soon
    i = 0;
    while i < 16 {
        *fl.add(i) = first as u32; // T=0, Q=0 = TD
        i += 1;
    }

    let last = td2;
    if !wait_td(last, 3_000_000) {
        return false;
    }
    if data_len > 0 && data_in && !data.is_null() {
        i = 0;
        while i < data_len.min(64) {
            *data.add(i) = *((data_buf + i) as *const u8);
            i += 1;
        }
    }
    // clear frame list entries
    i = 0;
    while i < 16 {
        *fl.add(i) = 1; // terminate
        i += 1;
    }
    true
}

unsafe fn port_reset(port_off: u16) -> bool {
    // PORTSC: bit0 CCS, bit1 CSC, bit2 PE, bit8 reset, bit9 suspend, bit12 LS
    let mut ps = inw(IO_BASE + port_off);
    if (ps & 1) == 0 {
        return false;
    }
    // Reset
    outw(IO_BASE + port_off, (ps | (1 << 9)) & !4); // suspend clear enable?
    outw(IO_BASE + port_off, ps | (1 << 9)); // Port Reset bit is 9 on UHCI
    delay(1_000_000);
    ps = inw(IO_BASE + port_off);
    outw(IO_BASE + port_off, ps & !(1 << 9));
    delay(200_000);
    // Enable
    ps = inw(IO_BASE + port_off);
    outw(IO_BASE + port_off, ps | 4);
    delay(100_000);
    (inw(IO_BASE + port_off) & 4) != 0
}

fn init_one(bus: u8, dev: u8, func: u8) -> bool {
    unsafe {
        // UHCI: BAR4 usually IO
        let bar4 = pci_r32(bus, dev, func, 0x20);
        let mut io = 0u16;
        if (bar4 & 1) != 0 {
            io = (bar4 & 0xFFFC) as u16;
        } else {
            // try BAR0
            let b0 = pci_r32(bus, dev, func, 0x10);
            if (b0 & 1) != 0 {
                io = (b0 & 0xFFFC) as u16;
            }
        }
        if io == 0 {
            serial::write_str("[UHCI] no IO BAR\n");
            return false;
        }
        // Enable IO + BM
        let cmd = pci_r32(bus, dev, func, 0x04);
        pci_w16(bus, dev, func, 0x04, (cmd as u16) | 0x05);

        // Legacy support: disable USBLEGSUP SMI if present (capability)
        // PCI config 0xC0 often USBLEGSUP for Intel UHCI
        let legs = pci_r32(bus, dev, func, 0xC0);
        if legs != 0 && legs != 0xFFFF_FFFF {
            pci_w16(bus, dev, func, 0xC0, 0x8F00); // claim ownership
            delay(50_000);
        }

        IO_BASE = io;
        serial::write_str("[UHCI] IO=");
        serial::write_hex(io as usize);
        serial::write_str("\n");

        // Global reset
        outw(io, 0x0004); // HCRESET
        delay(100_000);
        outw(io, 0);
        delay(50_000);

        let fl = match mm::alloc_pages(1) {
            Some(p) => p,
            None => return false,
        };
        let pool = match mm::alloc_pages(1) {
            Some(p) => p,
            None => return false,
        };
        mm::zero_pages(fl, 1);
        mm::zero_pages(pool, 1);
        FRAMELIST = fl;
        TD_POOL = pool;
        BUF = pool + 256;
        REPORT = pool + 512;
        INTR_TD = pool + 64;

        // Empty frame list
        let flp = fl as *mut u32;
        let mut i = 0usize;
        while i < 1024 {
            *flp.add(i) = 1; // terminate
            i += 1;
        }
        outl(io + 0x08, fl as u32); // FLBASEADD
        outw(io + 0x04, 0); // USBINTR
        outw(io + 0x06, 0x40); // FRNUM
        // Run + MaxPacket 64
        outw(io, 0x0001 | 0x0080); // RS | MaxPacket
        delay(30_000);
        DIAG_OK = true;

        // Ports at +0x10 and +0x12
        let mut found = 0xFFu8;
        for (idx, off) in [(0u8, 0x10u16), (1u8, 0x12u16)].iter() {
            let ps = inw(io + *off);
            if (ps & 1) != 0 {
                found = *idx;
                serial::write_str("[UHCI] port");
                serial::write_usize(*idx as usize);
                serial::write_str(" CCS PORTSC=");
                serial::write_hex(ps as usize);
                serial::write_str("\n");
                if port_reset(*off) {
                    DIAG_DEV = true;
                }
                break;
            }
        }
        if found == 0xFF {
            serial::write_str("[UHCI] no device on ports\n");
            return true;
        }
        if !DIAG_DEV {
            serial::write_str("[UHCI] port reset fail\n");
            return true;
        }

        // GET_DESCRIPTOR device 8 bytes addr0
        let mut setup = [0u8; 8];
        setup[0] = 0x80;
        setup[1] = 0x06;
        setup[3] = 0x01;
        setup[6] = 8;
        let mut desc = [0u8; 18];
        if !control_xfer(0, &setup, desc.as_mut_ptr(), 8, true) {
            serial::write_str("[UHCI] GET_DESC fail\n");
            return true;
        }
        // SET_ADDRESS 1
        setup = [0u8; 8];
        setup[1] = 0x05;
        setup[2] = 1;
        if !control_xfer(0, &setup, core::ptr::null_mut(), 0, false) {
            serial::write_str("[UHCI] SET_ADDR fail\n");
            return true;
        }
        DEV_ADDR = 1;
        delay(30_000);

        // SET_CONFIGURATION 1
        setup = [0u8; 8];
        setup[1] = 0x09;
        setup[2] = 1;
        if !control_xfer(1, &setup, core::ptr::null_mut(), 0, false) {
            serial::write_str("[UHCI] SET_CONFIG fail\n");
            return true;
        }
        // SET_PROTOCOL boot
        setup = [0u8; 8];
        setup[0] = 0x21;
        setup[1] = 0x0B;
        let _ = control_xfer(1, &setup, core::ptr::null_mut(), 0, false);
        // SET_IDLE
        setup = [0u8; 8];
        setup[0] = 0x21;
        setup[1] = 0x0A;
        let _ = control_xfer(1, &setup, core::ptr::null_mut(), 0, false);

        EP_IN = 1;
        MPS = 4;
        DIAG_HID = true;

        // Interrupt TD: IN ep1, toggle 0, active, IOC
        let tok = make_token(PID_IN, DEV_ADDR, EP_IN, 0, MPS);
        td_write(INTR_TD, 1, TD_ACTIVE | TD_IOC | (3 << 27), tok, REPORT as u32);
        // Place in every 8th frame
        i = 0;
        while i < 1024 {
            if (i % 8) == 0 {
                *flp.add(i) = INTR_TD as u32;
            }
            i += 1;
        }
        LIVE = true;
        serial::write_str("[UHCI] HID mouse LIVE\n");
        true
    }
}

pub fn probe() {
    serial::write_str("\n[UHCI] scan...\n");
    unsafe {
        DIAG_OK = false;
        DIAG_DEV = false;
        DIAG_HID = false;
        DIAG_REPORTS = 0;
        DIAG_EVENTS = 0;
        LIVE = false;
        IO_BASE = 0;
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
                    // UHCI: 0C 03 00
                    if class == 0x0C && sub == 0x03 && prog == 0x00 {
                        serial::write_str("[UHCI] found ");
                        serial::write_hex(((id >> 16) & 0xFFFF) as usize);
                        serial::write_str(":");
                        serial::write_hex((id & 0xFFFF) as usize);
                        serial::write_str("\n");
                        if init_one(bus, dev, func) && unsafe { LIVE } {
                            return;
                        }
                    }
                }
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
    serial::write_str("[UHCI] no live mouse\n");
}

pub fn poll_mouse_live() {
    unsafe {
        if !LIVE || INTR_TD == 0 {
            return;
        }
        if td_active(INTR_TD) {
            return;
        }
        let cs = core::ptr::read_volatile((INTR_TD + 4) as *const u32);
        if (cs & (1 << 22)) != 0 {
            // stalled - requeue
        } else {
            let mut report = [0u8; 8];
            let mut i = 0usize;
            while i < 8 {
                report[i] = *((REPORT + i) as *const u8);
                i += 1;
            }
            DIAG_REPORTS = DIAG_REPORTS.wrapping_add(1);
            if report[0] != 0 || report[1] != 0 || report[2] != 0 {
                crate::input::process_hid_boot_mouse(&report);
                DIAG_EVENTS = DIAG_EVENTS.wrapping_add(1);
            }
            // flip data toggle in token
            let tok = core::ptr::read_volatile((INTR_TD + 8) as *const u32);
            let toggle = ((tok >> 19) & 1) ^ 1;
            let new_tok = make_token(PID_IN, DEV_ADDR, EP_IN, toggle as u8, MPS);
            // clear report
            i = 0;
            while i < 8 {
                *((REPORT + i) as *mut u8) = 0;
                i += 1;
            }
            td_write(INTR_TD, 1, TD_ACTIVE | TD_IOC | (3 << 27), new_tok, REPORT as u32);
            return;
        }
        // requeue on stall
        let tok = make_token(PID_IN, DEV_ADDR, EP_IN, 0, MPS);
        td_write(INTR_TD, 1, TD_ACTIVE | TD_IOC | (3 << 27), tok, REPORT as u32);
    }
}

pub fn rescan() {
    probe();
}
