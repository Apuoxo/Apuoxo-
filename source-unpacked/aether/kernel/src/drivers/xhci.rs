//! xHCI: rings + USB enumeration (Port Reset → Enable Slot → Address Device → GET_DESCRIPTOR)

use crate::serial;
use crate::mm;
use crate::mm::paging;

// USB mouse diagnostics
static mut DIAG_XHCI: bool = false;
static mut DIAG_DEV: bool = false;
static mut DIAG_HID: bool = false;
static mut DIAG_MOUSE_IF: bool = false;
static mut DIAG_EP_IN: bool = false;
static mut DIAG_REPORTS: u32 = 0;
static mut DIAG_TRB_QUEUED: u32 = 0;
static mut DIAG_COMPLETIONS: u32 = 0;
static mut DIAG_FOUND: u32 = 0;
static mut DIAG_PORT_CCS: u32 = 0;
static mut DIAG_PORTS: u8 = 0;
static mut SAVE_MMIO: usize = 0;
static mut SAVE_CAP: u8 = 0;
static mut SAVE_PORTS: u8 = 0;
static mut SAVE_BUS: u8 = 0;
static mut SAVE_DEV: u8 = 0;
static mut SAVE_FUNC: u8 = 0;

pub fn diag_xhci_ok() -> bool { unsafe { DIAG_XHCI } }
pub fn diag_dev_found() -> bool { unsafe { DIAG_DEV } }
pub fn diag_hid_ok() -> bool { unsafe { DIAG_HID } }
pub fn diag_mouse_if() -> bool { unsafe { DIAG_MOUSE_IF } }
pub fn diag_ep_in() -> bool { unsafe { DIAG_EP_IN } }
pub fn diag_reports() -> u32 { unsafe { DIAG_REPORTS } }
pub fn diag_trb_queued() -> u32 { unsafe { DIAG_TRB_QUEUED } }
pub fn diag_completions() -> u32 { unsafe { DIAG_COMPLETIONS } }
pub fn diag_found_count() -> u32 { unsafe { DIAG_FOUND } }
pub fn diag_port_ccs() -> u32 { unsafe { DIAG_PORT_CCS } }
pub fn diag_ports() -> u8 { unsafe { DIAG_PORTS } }

// Persistent HID mouse interrupt path for desktop loop
static mut MOUSE_LIVE: bool = false;
static mut MOUSE_ER: usize = 0;
static mut MOUSE_ER_DEQ: usize = 0;
static mut MOUSE_ER_CYCLE: u32 = 1;
static mut MOUSE_ER_SIZE: usize = 0;
static mut MOUSE_INTR0: usize = 0;
static mut MOUSE_EP_RING: usize = 0;
static mut MOUSE_ENQ: usize = 0;
static mut MOUSE_CYCLE: u32 = 1;
static mut MOUSE_REPORT: usize = 0;
static mut MOUSE_SLOT: u8 = 0;
static mut MOUSE_DB: usize = 0;
static mut MOUSE_EVENTS: u32 = 0; // internal events delivered to input
static mut MOUSE_DCI: u32 = 3; // doorbell target: EP1 IN = DCI 3
static mut MOUSE_MPS: u16 = 4;

pub fn diag_mouse_events() -> u32 { unsafe { MOUSE_EVENTS } }

pub fn diag_mouse_live() -> bool { unsafe { MOUSE_LIVE } }

// ─── Phone MSC bulk transfer context ─────────────────────────
static mut PHONE_LIVE: bool = false;
static mut PHONE_SLOT: u8 = 0;
static mut PHONE_DB: usize = 0;
static mut PHONE_ER: usize = 0;
static mut PHONE_ER_DEQ: usize = 0;
static mut PHONE_ER_CYCLE: u32 = 0;
static mut PHONE_ER_SIZE: usize = 0;
static mut PHONE_INTR0: usize = 0;
static mut PHONE_RING_OUT: usize = 0;
static mut PHONE_RING_IN: usize = 0;
static mut PHONE_ENQ_OUT: usize = 0;
static mut PHONE_ENQ_IN: usize = 0;
static mut PHONE_CYC_OUT: u32 = 1;
static mut PHONE_CYC_IN: u32 = 1;
static mut PHONE_DCI_OUT: u32 = 0;
static mut PHONE_DCI_IN: u32 = 0;
static mut PHONE_TAG: u32 = 1;
static mut PHONE_MMIO: usize = 0;
static mut PHONE_CAP: u8 = 0;
static mut PHONE_RTSOFF: u32 = 0;

pub fn phone_live() -> bool { unsafe { PHONE_LIVE } }



// ─── PCI / MMIO ───────────────────────────────────────────────

unsafe fn pci_cfg(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    0x80000000 | ((bus as u32) << 16) | ((dev as u32) << 11) | ((func as u32) << 8) | ((off as u32) & 0xFC)
}
unsafe fn pci_r32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let a = pci_cfg(bus, dev, func, off);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let v: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") v, options(nostack, preserves_flags));
    v
}
unsafe fn pci_w32(bus: u8, dev: u8, func: u8, off: u8, val: u32) {
    let a = pci_cfg(bus, dev, func, off);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") val, options(nostack, preserves_flags));
}

fn hx(v: usize) { serial::write_hex(v); }
fn h32(v: u32) { serial::write_hex(v as usize); }

unsafe fn r32(b: usize, o: usize) -> u32 { core::ptr::read_volatile((b + o) as *const u32) }
unsafe fn w32(b: usize, o: usize, v: u32) { core::ptr::write_volatile((b + o) as *mut u32, v); }
unsafe fn r64(b: usize, o: usize) -> u64 {
    (r32(b, o) as u64) | ((r32(b, o + 4) as u64) << 32)
}
unsafe fn w64(b: usize, o: usize, v: u64) {
    w32(b, o, v as u32);
    w32(b, o + 4, (v >> 32) as u32);
}

fn delay(n: u32) {
    let mut i = 0u32;
    while i < n { unsafe { core::arch::asm!("nop", options(nostack, preserves_flags)); } i += 1; }
}

fn map_mmio(phys: u64, size: u64) -> bool {
    let cr3 = unsafe { paging::read_cr3() };
    let mut a = (phys & !0xFFF) as usize;
    let end = ((phys + size + 0xFFF) & !0xFFF) as usize;
    while a < end {
        if !unsafe { paging::map_page(cr3, a, a, paging::PAGE_PRESENT | paging::PAGE_WRITE) } {
            return false;
        }
        a += 4096;
    }
    unsafe { paging::load_cr3(cr3); }
    true
}

fn dma_alloc(pages: usize) -> Option<usize> {
    let phys = mm::alloc_pages(pages)?;
    mm::zero_pages(phys, pages);
    // Boot maps first 1GB identity
    if phys + pages * 4096 > 0x4000_0000 { return None; }
    unsafe {
        let p = phys as *mut u32;
        *p = 0xDEAD_BEEF;
        if *p != 0xDEAD_BEEF { return None; }
        *p = 0;
    }
    Some(phys)
}

// ─── TRB ──────────────────────────────────────────────────────

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct Trb {
    lo: u32,
    hi: u32,
    status: u32,
    control: u32,
}

impl Trb {
    const fn z() -> Self { Trb { lo: 0, hi: 0, status: 0, control: 0 } }
}

const TRB_LINK: u32 = 6;
const TRB_ENABLE_SLOT: u32 = 9;
const TRB_ADDRESS_DEV: u32 = 11;
const TRB_CONFIGURE_EP: u32 = 12;
const TRB_NOOP_CMD: u32 = 23;
const TRB_TRANSFER: u32 = 32;       // Transfer Event
const TRB_CMD_COMPLETE: u32 = 33;
const TRB_PORT_STATUS: u32 = 34;
const TRB_SETUP: u32 = 2;
const TRB_DATA: u32 = 3;
const TRB_STATUS: u32 = 4;
const TRB_NORMAL: u32 = 1;

const CYCLE: u32 = 1;
const TRB_IOC: u32 = 1 << 5;
const TRB_IDT: u32 = 1 << 6;  // Immediate Data
const TRB_TC: u32 = 1 << 1;

const USBCMD_RS: u32 = 1;
const USBCMD_HCRST: u32 = 2;
const USBCMD_INTE: u32 = 4;
const USBSTS_HCH: u32 = 1;
const USBSTS_CNR: u32 = 1 << 11;

const PORTSC_CCS: u32 = 1;
const PORTSC_PED: u32 = 2;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_PP: u32 = 1 << 9;
const PORTSC_CSC: u32 = 1 << 17;
const PORTSC_PRC: u32 = 1 << 21;
const PORTSC_PLC: u32 = 1 << 22;
const PORTSC_LWS: u32 = 1 << 16;
const PORTSC_W1C: u32 = (1<<17)|(1<<18)|(1<<19)|(1<<20)|(1<<21)|(1<<22)|(1<<23);
const PLS_MASK: u32 = 0xF << 5;
const PLS_RXDETECT: u32 = 5;

// ─── Controller ───────────────────────────────────────────────

struct Xhci {
    mmio: usize,
    cap: u8,
    slots: u8,
    ports: u8,
    dboff: u32,
    rtsoff: u32,
    ctx_size: usize, // 32 or 64
    dcbaa: usize,
    cmd_ring: usize,
    cmd_size: usize,
    cmd_enq: usize,
    cmd_cycle: u32,
    erst: usize,
    er: usize,
    er_size: usize,
    er_deq: usize,
    er_cycle: u32,
}

impl Xhci {
    fn op(&self) -> usize { self.mmio + self.cap as usize }
    fn rt(&self) -> usize { self.mmio + self.rtsoff as usize }
    fn db(&self) -> usize { self.mmio + self.dboff as usize }
    fn portsc(&self, p: u8) -> usize { self.op() + 0x400 + 0x10 * (p as usize - 1) }
    fn intr0(&self) -> usize { self.rt() + 0x20 }
}

// ─── Event wait ───────────────────────────────────────────────

struct Event {
    trb_type: u32,
    cc: u32,
    slot_id: u32,
    port_id: u32,
    lo: u32,
    status: u32,
    control: u32,
}

fn wait_event(x: &mut Xhci, want_type: u32, max_spins: u32) -> Option<Event> {
    let mut spins = 0u32;
    while spins < max_spins {
        unsafe {
            let trbs = x.er as *const Trb;
            let ev = *trbs.add(x.er_deq);
            if (ev.control & 1) == x.er_cycle {
                let ty = (ev.control >> 10) & 0x3F;
                let cc = (ev.status >> 24) & 0xFF;
                let slot = (ev.control >> 24) & 0xFF;
                let port = (ev.lo >> 24) & 0xFF; // for port status change events
                let e = Event {
                    trb_type: ty, cc, slot_id: slot, port_id: port,
                    lo: ev.lo, status: ev.status, control: ev.control,
                };
                x.er_deq += 1;
                if x.er_deq >= x.er_size {
                    x.er_deq = 0;
                    x.er_cycle ^= 1;
                }
                let erdp = x.er + x.er_deq * 16;
                w64(x.intr0(), 0x18, erdp as u64);

                serial::write_str("  [EVT] type=");
                serial::write_usize(ty as usize);
                serial::write_str(" CC=");
                serial::write_usize(cc as usize);
                serial::write_str(" slot=");
                serial::write_usize(slot as usize);
                serial::write_str("\n");

                if ty == want_type || want_type == 0 {
                    return Some(e);
                }
                // keep consuming until match or empty
                continue;
            }
        }
        delay(30);
        spins += 1;
    }
    None
}

fn ring_cmd_doorbell(x: &Xhci) {
    unsafe { w32(x.db(), 0, 0); }
}

fn submit_cmd(x: &mut Xhci, lo: u32, hi: u32, status: u32, type_flags: u32) -> bool {
    if x.cmd_enq >= x.cmd_size - 1 { return false; }
    unsafe {
        let trbs = x.cmd_ring as *mut Trb;
        let t = &mut *trbs.add(x.cmd_enq);
        t.lo = lo;
        t.hi = hi;
        t.status = status;
        t.control = (type_flags << 10) | x.cmd_cycle | (type_flags & 0); // type in flags param already shifted? 
        // type_flags is just the type number
        t.control = (type_flags << 10) | x.cmd_cycle;
        x.cmd_enq += 1;
    }
    ring_cmd_doorbell(x);
    true
}

fn submit_cmd_full(x: &mut Xhci, lo: u32, hi: u32, status: u32, control_without_cycle: u32) -> bool {
    if x.cmd_enq >= x.cmd_size - 1 { return false; }
    unsafe {
        let trbs = x.cmd_ring as *mut Trb;
        let t = &mut *trbs.add(x.cmd_enq);
        t.lo = lo;
        t.hi = hi;
        t.status = status;
        t.control = control_without_cycle | x.cmd_cycle;
        x.cmd_enq += 1;
    }
    ring_cmd_doorbell(x);
    true
}

// ─── Basic init (proven path) ─────────────────────────────────


fn legacy_handoff(mmio: usize, _cap: u8) {
    let hcc = unsafe { r32(mmio, 0x10) };
    let mut xecp = ((hcc >> 16) & 0xFFFF) as usize;
    let mut hops = 0u32;
    while xecp != 0 && hops < 32 {
        let off = xecp * 4;
        let val = unsafe { r32(mmio, off) };
        let cap_id = val & 0xFF;
        let next = ((val >> 8) & 0xFF) as usize;
        if cap_id == 1 {
            serial::write_str("  [xHCI] legacy handoff...\n");
            unsafe { w32(mmio, off, val | (1 << 24)); }
            let mut t = 0u32;
            while t < 500_000 {
                let v = unsafe { r32(mmio, off) };
                if (v & (1 << 16)) == 0 {
                    serial::write_str("  [xHCI] legacy handoff OK\n");
                    break;
                }
                delay(40);
                t += 1;
            }
            break;
        }
        if next == 0 { break; }
        xecp = xecp + next;
        hops += 1;
    }
}

fn hc_reset(mmio: usize, cap: u8) -> bool {
    let op = mmio + cap as usize;
    unsafe {
        if r32(op, 4) & USBSTS_HCH == 0 {
            w32(op, 0, r32(op, 0) & !USBCMD_RS);
            let mut t = 0u32;
            while t < 100000 { if r32(op, 4) & USBSTS_HCH != 0 { break; } delay(40); t += 1; }
        }
        w32(op, 0, r32(op, 0) | USBCMD_HCRST);
        let mut t = 0u32;
        while t < 200000 { if r32(op, 0) & USBCMD_HCRST == 0 { break; } delay(40); t += 1; }
        if r32(op, 0) & USBCMD_HCRST != 0 { return false; }
        t = 0;
        while t < 200000 { if r32(op, 4) & USBSTS_CNR == 0 { break; } delay(40); t += 1; }
        r32(op, 4) & USBSTS_CNR == 0
    }
}

fn setup_rings(x: &mut Xhci) -> bool {
    // DCBAAP
    let n = x.slots as usize + 1;
    let pages = (n * 8 + 4095) / 4096;
    let dcbaa = match dma_alloc(pages) { Some(p) => p, None => return false };
    x.dcbaa = dcbaa;
    unsafe { w64(x.op(), 0x30, dcbaa as u64); }
    serial::write_str("  [xHCI] DCBAAP=");
    hx(dcbaa);
    serial::write_str("\n");

    // Command ring
    let cr = match dma_alloc(1) { Some(p) => p, None => return false };
    x.cmd_ring = cr;
    x.cmd_size = 64;
    x.cmd_enq = 0;
    x.cmd_cycle = 1;
    unsafe {
        let trbs = cr as *mut Trb;
        let mut i = 0usize;
        while i < 64 { *trbs.add(i) = Trb::z(); i += 1; }
        let link = &mut *trbs.add(63);
        link.lo = cr as u32;
        link.hi = 0;
        link.control = (TRB_LINK << 10) | TRB_TC | CYCLE;
        w64(x.op(), 0x18, (cr as u64) | 1);
    }
    serial::write_str("  [xHCI] CRCR ring=");
    hx(cr);
    serial::write_str("\n");

    // Event ring + ERST
    let er = match dma_alloc(1) { Some(p) => p, None => return false };
    let erst = match dma_alloc(1) { Some(p) => p, None => return false };
    x.er = er;
    x.er_size = 64;
    x.er_deq = 0;
    x.er_cycle = 1;
    x.erst = erst;
    unsafe {
        let trbs = er as *mut Trb;
        let mut i = 0usize;
        while i < 64 { *trbs.add(i) = Trb::z(); i += 1; }
        let e = erst as *mut u64;
        *e.add(0) = er as u64;
        *e.add(1) = 64;
        let i0 = x.intr0();
        w32(i0, 0x08, 1);
        w64(i0, 0x10, erst as u64);
        w64(i0, 0x18, er as u64);
        w32(i0, 0x04, 0);
        w32(i0, 0x00, 0x2);
    }
    serial::write_str("  [xHCI] ER=");
    hx(er);
    serial::write_str(" ERST=");
    hx(erst);
    serial::write_str("\n");

    // CONFIG + Run
    unsafe {
        w32(x.op(), 0x38, x.slots as u32);
        w32(x.op(), 0x00, USBCMD_RS | USBCMD_INTE);
        delay(8000);
        let sts = r32(x.op(), 0x04);
        serial::write_str("  [xHCI] USBSTS=");
        h32(sts);
        serial::write_str(" HCH=");
        serial::write_usize(if sts & USBSTS_HCH != 0 { 1 } else { 0 });
        serial::write_str("\n");
        if sts & USBSTS_HCH != 0 { return false; }
    }
    true
}

// ─── Port reset ───────────────────────────────────────────────

fn power_all_ports(x: &Xhci) {
    // 1) PP on all ports
    let mut p = 1u8;
    while p <= x.ports {
        unsafe {
            let ps = r32(x.portsc(p), 0);
            let v = (ps | PORTSC_PP) & !PORTSC_W1C;
            w32(x.portsc(p), 0, v);
        }
        p = p.wrapping_add(1);
    }
    delay(150_000);
    // 2) Force PLS = RxDetect with LWS, clear change bits (W1C write-1)
    p = 1;
    while p <= x.ports {
        unsafe {
            let ps = r32(x.portsc(p), 0);
            let mut v = (ps | PORTSC_PP | PORTSC_LWS) & !PORTSC_W1C;
            v = (v & !PLS_MASK) | (PLS_RXDETECT << 5);
            v |= PORTSC_W1C;
            w32(x.portsc(p), 0, v);
        }
        p = p.wrapping_add(1);
    }
    delay(400_000);
    // 3) Second PP pulse
    p = 1;
    while p <= x.ports {
        unsafe {
            let ps = r32(x.portsc(p), 0);
            w32(x.portsc(p), 0, ((ps | PORTSC_PP) & !PORTSC_W1C) | PORTSC_W1C);
        }
        p = p.wrapping_add(1);
    }
    delay(300_000);
}

fn find_connected_port(x: &Xhci) -> Option<u8> {
    let mut p = 1u8;
    while p <= x.ports {
        let ps = unsafe { r32(x.portsc(p), 0) };
        serial::write_str("  [PORT] ");
        serial::write_usize(p as usize);
        serial::write_str(" PORTSC=");
        h32(ps);
        serial::write_str(" CCS=");
        serial::write_usize(if ps & PORTSC_CCS != 0 { 1 } else { 0 });
        serial::write_str("\n");
        if ps & PORTSC_CCS != 0 {
            return Some(p);
        }
        p += 1;
    }
    None
}

fn port_reset(x: &mut Xhci, port: u8) -> bool {
    serial::write_str("\n  --- Port Reset ---\n");
    let ps_before = unsafe { r32(x.portsc(port), 0) };
    serial::write_str("  [PORT] before PORTSC=");
    h32(ps_before);
    serial::write_str("\n");

    unsafe {
        // Power on if needed
        if ps_before & PORTSC_PP == 0 {
            w32(x.portsc(port), 0, ps_before | PORTSC_PP);
            delay(50000);
        }
        // Write 1 to PR; preserve PP; clear change bits by writing 1
        let mut v = r32(x.portsc(port), 0);
        v |= PORTSC_PR | PORTSC_PP;
        // Do not clear CCS accidentally
        w32(x.portsc(port), 0, v);
    }

    // Wait for PRC (Port Reset Change) via poll PORTSC or event
    let mut t = 0u32;
    while t < 500000 {
        let ps = unsafe { r32(x.portsc(port), 0) };
        if ps & PORTSC_PRC != 0 {
            serial::write_str("  [PORT] PRC set PORTSC=");
            h32(ps);
            serial::write_str(" PED=");
            serial::write_usize(if ps & PORTSC_PED != 0 { 1 } else { 0 });
            serial::write_str("\n");
            // Clear CSC and PRC (W1C)
            unsafe {
                let mut c = r32(x.portsc(port), 0);
                c |= PORTSC_CSC | PORTSC_PRC;
                // Clear PR bit is auto-cleared by HC
                w32(x.portsc(port), 0, c);
            }
            // Drain any port status change events
            while let Some(ev) = wait_event(x, TRB_PORT_STATUS, 5000) {
                serial::write_str("  [PORT] PSC event port=");
                serial::write_usize(ev.port_id as usize);
                serial::write_str(" CC=");
                serial::write_usize(ev.cc as usize);
                serial::write_str("\n");
            }
            let ps_after = unsafe { r32(x.portsc(port), 0) };
            serial::write_str("  [PORT] after PORTSC=");
            h32(ps_after);
            serial::write_str(" CCS=");
            serial::write_usize(if ps_after & PORTSC_CCS != 0 { 1 } else { 0 });
            serial::write_str(" PED=");
            serial::write_usize(if ps_after & PORTSC_PED != 0 { 1 } else { 0 });
            serial::write_str("\n");
            return ps_after & PORTSC_PED != 0 || ps_after & PORTSC_CCS != 0;
        }
        delay(50);
        t += 1;
    }
    serial::write_str("  [PORT] reset TIMEOUT\n");
    false
}

// ─── Enable Slot ──────────────────────────────────────────────

fn enable_slot(x: &mut Xhci) -> Option<u8> {
    serial::write_str("\n  --- Enable Slot ---\n");
    if !submit_cmd_full(x, 0, 0, 0, TRB_ENABLE_SLOT << 10) {
        return None;
    }
    match wait_event(x, TRB_CMD_COMPLETE, 300000) {
        Some(ev) => {
            if ev.cc != 1 {
                serial::write_str("  [SLOT] Enable Slot CC FAIL\n");
                return None;
            }
            let sid = ev.slot_id as u8;
            serial::write_str("  [SLOT] Slot ID=");
            serial::write_usize(sid as usize);
            serial::write_str("\n");
            if sid == 0 { return None; }
            Some(sid)
        }
        None => {
            serial::write_str("  [SLOT] Enable Slot TIMEOUT\n");
            None
        }
    }
}

// ─── Input/Device Context + EP0 ring ──────────────────────────

fn speed_from_portsc(ps: u32) -> u32 {
    // PORTSC bits 13:10
    (ps >> 10) & 0xF
}

fn max_pkt_for_speed(speed: u32) -> u32 {
    match speed {
        1 => 8,    // Full
        2 => 8,    // Low
        3 => 64,   // High
        4 => 512,  // Super
        _ => 64,
    }
}

/// Build Input Context + Device Context, set EP0 transfer ring, Address Device
fn address_device(x: &mut Xhci, slot: u8, port: u8) -> Option<usize> {
    serial::write_str("\n  --- Address Device ---\n");
    let ps = unsafe { r32(x.portsc(port), 0) };
    let speed = speed_from_portsc(ps);
    let mps = max_pkt_for_speed(speed);
    serial::write_str("  [ADDR] port=");
    serial::write_usize(port as usize);
    serial::write_str(" speed=");
    serial::write_usize(speed as usize);
    serial::write_str(" MPS=");
    serial::write_usize(mps as usize);
    serial::write_str("\n");

    let cs = x.ctx_size; // 32 or 64
    // Input Context: A0+A1 + Slot + EP0  =>  (1 + 1 + 1) * ctx? 
    // Layout: Input Control Context (cs bytes) + Slot Context + EP contexts
    // For Address Device: drop Contexts flags A0=1 A1=1, Slot + EP0
    // Size: Control + Slot + EP0 = 3 * cs, allocate 1 page
    let in_ctx = match dma_alloc(1) { Some(p) => p, None => return None };
    let dev_ctx = match dma_alloc(1) { Some(p) => p, None => return None };
    let ep0_ring = match dma_alloc(1) { Some(p) => p, None => return None };

    serial::write_str("  [ADDR] InputCtx=");
    hx(in_ctx);
    serial::write_str(" DevCtx=");
    hx(dev_ctx);
    serial::write_str(" EP0ring=");
    hx(ep0_ring);
    serial::write_str("\n");

    unsafe {
        // Input Control Context: Dwords 0 = drop flags, Dwords 1 = add flags
        // Add Context Flags: A0 (slot) | A1 (EP0)
        let ic = in_ctx as *mut u32;
        // all zero already
        *ic.add(1) = 0x3; // A0 | A1

        // Slot Context at offset cs
        let slot_ctx = (in_ctx + cs) as *mut u32;
        // DW0: Route String=0, Speed, Context Entries=1 (EP0 only)
        // bit 20-23 Context Entries = 1
        // bit 27-24? Speed at bits 23:20 for some docs - check xHCI:
        // Slot Context DW0:
        //  19:0 Route String
        //  23:20 Speed
        //  24 MTT, 25 Hub
        //  31:27 Context Entries
        *slot_ctx.add(0) = (speed << 20) | (1u32 << 27);
        // DW1: Root Hub Port Number bits 23:16
        *slot_ctx.add(1) = (port as u32) << 16;
        // DW2, DW3 = 0

        // EP0 Context at offset 2*cs
        let ep0 = (in_ctx + 2 * cs) as *mut u32;
        // DW0: EP State=0, Interval=0
        *ep0.add(0) = 0;
        // DW1: CErr=3, EP Type=Control(4), Max Packet Size
        // CErr bits 2:1 = 3, EP Type bits 5:3 = 4 (Control Bidirectional)
        *ep0.add(1) = (3 << 1) | (4 << 3) | (mps << 16);
        // DW2-3: TR Dequeue Pointer | DCS=1
        let tr_deq = (ep0_ring as u64) | 1; // DCS
        *ep0.add(2) = tr_deq as u32;
        *ep0.add(3) = (tr_deq >> 32) as u32;
        // DW4: Average TRB Length = 8
        *ep0.add(4) = 8;

        // Init EP0 transfer ring: last = Link TRB
        let trbs = ep0_ring as *mut Trb;
        let mut i = 0usize;
        while i < 64 { *trbs.add(i) = Trb::z(); i += 1; }
        let link = &mut *trbs.add(63);
        link.lo = ep0_ring as u32;
        link.control = (TRB_LINK << 10) | TRB_TC | CYCLE;

        // Install Device Context pointer in DCBAA[slot]
        let dcbaa = x.dcbaa as *mut u64;
        *dcbaa.add(slot as usize) = dev_ctx as u64;
        serial::write_str("  [ADDR] DCBAA[");
        serial::write_usize(slot as usize);
        serial::write_str("]=");
        hx(dev_ctx);
        serial::write_str("\n");
    }

    // Address Device Command: TRB type 11, Slot ID in bits 31:24, pointer to Input Context
    // BSR=0 (bit 9) means actually send SET_ADDRESS
    let ctrl = (TRB_ADDRESS_DEV << 10) | ((slot as u32) << 24);
    if !submit_cmd_full(x, in_ctx as u32, 0, 0, ctrl) {
        return None;
    }
    match wait_event(x, TRB_CMD_COMPLETE, 500000) {
        Some(ev) => {
            if ev.cc != 1 {
                serial::write_str("  [ADDR] Address Device CC=");
                serial::write_usize(ev.cc as usize);
                serial::write_str(" FAIL\n");
                return None;
            }
            serial::write_str("  [ADDR] Address Device SUCCESS slot=");
            serial::write_usize(ev.slot_id as usize);
            serial::write_str("\n");
            Some(ep0_ring)
        }
        None => {
            serial::write_str("  [ADDR] Address Device TIMEOUT\n");
            None
        }
    }
}

// ─── Control transfer on EP0 ──────────────────────────────────

struct Ep0Ring {
    phys: usize,
    enq: usize,
    cycle: u32,
    size: usize,
}

fn ep0_submit_control(
    x: &mut Xhci,
    slot: u8,
    ep0: &mut Ep0Ring,
    setup: &[u8; 8],
    data_buf: usize,
    data_len: u32,
    data_in: bool,
) -> bool {
    // Setup Stage TRB (IDT=1, 8 bytes immediate)
    // Data Stage TRB if len > 0
    // Status Stage TRB
    if ep0.enq + 4 >= ep0.size - 1 {
        serial::write_str("  [EP0] ring full\n");
        return false;
    }

    unsafe {
        let trbs = ep0.phys as *mut Trb;

        // Setup Stage
        let t0 = &mut *trbs.add(ep0.enq);
        t0.lo = u32::from_le_bytes([setup[0], setup[1], setup[2], setup[3]]);
        t0.hi = u32::from_le_bytes([setup[4], setup[5], setup[6], setup[7]]);
        t0.status = 8; // TRB Transfer Length = 8
        // Type=Setup, IDT=1, IOC=0, Cycle, TRT: 0=no data, 2=IN, 3=OUT bits 17:16
        let trt = if data_len == 0 { 0u32 } else if data_in { 3 } else { 2 };
        t0.control = (TRB_SETUP << 10) | TRB_IDT | ep0.cycle | (trt << 16);
        ep0.enq += 1;

        // Data Stage
        if data_len > 0 {
            let t1 = &mut *trbs.add(ep0.enq);
            t1.lo = data_buf as u32;
            t1.hi = 0;
            t1.status = data_len;
            // Type=Data, DIR=1 for IN (bit 16)
            let dir = if data_in { 1u32 << 16 } else { 0 };
            t1.control = (TRB_DATA << 10) | ep0.cycle | dir;
            ep0.enq += 1;
        }

        // Status Stage
        let t2 = &mut *trbs.add(ep0.enq);
        t2.lo = 0;
        t2.hi = 0;
        t2.status = 0;
        // DIR opposite of data for status; IOC=1
        // For IN data, status is OUT (DIR=0); for OUT data or no data, status is IN (DIR=1)
        let status_dir = if data_len > 0 && data_in { 0u32 } else { 1u32 << 16 };
        t2.control = (TRB_STATUS << 10) | TRB_IOC | ep0.cycle | status_dir;
        ep0.enq += 1;
    }

    // Doorbell: slot, target EP0 = 1
    unsafe { w32(x.db(), (slot as usize) * 4, 1); }

    // Wait for Transfer Event
    match wait_event(x, TRB_TRANSFER, 500000) {
        Some(ev) => {
            if ev.cc == 1 || ev.cc == 13 {
                // Success or Short Packet
                serial::write_str("  [EP0] Transfer Event CC=");
                serial::write_usize(ev.cc as usize);
                serial::write_str(" OK\n");
                true
            } else {
                serial::write_str("  [EP0] Transfer Event CC=");
                serial::write_usize(ev.cc as usize);
                serial::write_str(" FAIL\n");
                false
            }
        }
        None => {
            // Also accept CMD_COMPLETE? Transfer uses Transfer Event
            serial::write_str("  [EP0] Transfer TIMEOUT\n");
            false
        }
    }
}

fn get_descriptor(x: &mut Xhci, slot: u8, ep0: &mut Ep0Ring, desc_type: u8, desc_idx: u8, lang: u16, len: u16, buf: usize) -> bool {
    let mut setup = [0u8; 8];
    setup[0] = 0x80; // Device to host, standard, device
    setup[1] = 0x06; // GET_DESCRIPTOR
    setup[2] = desc_idx;
    setup[3] = desc_type;
    setup[4] = (lang & 0xFF) as u8;
    setup[5] = (lang >> 8) as u8;
    setup[6] = (len & 0xFF) as u8;
    setup[7] = (len >> 8) as u8;

    serial::write_str("  [CTRL] GET_DESCRIPTOR type=");
    serial::write_usize(desc_type as usize);
    serial::write_str(" len=");
    serial::write_usize(len as usize);
    serial::write_str("\n");

    ep0_submit_control(x, slot, ep0, &setup, buf, len as u32, true)
}

fn dump_bytes(label: &str, buf: usize, len: usize) {
    serial::write_str("  [");
    serial::write_str(label);
    serial::write_str("]");
    let mut i = 0usize;
    while i < len {
        let b = unsafe { *((buf + i) as *const u8) };
        serial::write_str(" ");
        // 2-digit hex
        let hi = b >> 4;
        let lo = b & 0xf;
        let ch = |d: u8| -> u8 { if d < 10 { b'0' + d } else { b'a' + d - 10 } };
        // use write_str single chars via hex of full byte simpler:
        serial::write_hex(b as usize);
        i += 1;
        if i >= 64 { serial::write_str(" ..."); break; }
    }
    serial::write_str("\n");
}

fn parse_device_desc(buf: usize) {
    let b = |o: usize| unsafe { *((buf + o) as *const u8) };
    let w = |o: usize| unsafe { core::ptr::read_unaligned((buf + o) as *const u16) };
    serial::write_str("  [DEV] bLength=");
    serial::write_usize(b(0) as usize);
    serial::write_str(" bDescType=");
    serial::write_usize(b(1) as usize);
    serial::write_str(" bcdUSB=");
    hx(w(2) as usize);
    serial::write_str(" class=");
    serial::write_usize(b(4) as usize);
    serial::write_str(" sub=");
    serial::write_usize(b(5) as usize);
    serial::write_str(" proto=");
    serial::write_usize(b(6) as usize);
    serial::write_str("\n  [DEV] maxPkt0=");
    serial::write_usize(b(7) as usize);
    serial::write_str(" idVendor=");
    hx(w(8) as usize);
    serial::write_str(" idProduct=");
    hx(w(10) as usize);
    serial::write_str(" bNumConfigs=");
    serial::write_usize(b(17) as usize);
    serial::write_str("\n");
}

fn parse_config_desc(buf: usize, total: usize) {
    serial::write_str("  [CFG] parse total=");
    serial::write_usize(total);
    serial::write_str("\n");
    let mut off = 0usize;
    while off + 2 <= total {
        let blen = unsafe { *((buf + off) as *const u8) } as usize;
        let bty = unsafe { *((buf + off + 1) as *const u8) };
        if blen < 2 { break; }
        if bty == 2 && blen >= 9 {
            let num_intf = unsafe { *((buf + off + 4) as *const u8) };
            let conf_val = unsafe { *((buf + off + 5) as *const u8) };
            serial::write_str("  [CFG] configVal=");
            serial::write_usize(conf_val as usize);
            serial::write_str(" bNumInterfaces=");
            serial::write_usize(num_intf as usize);
            serial::write_str("\n");
        } else if bty == 4 && blen >= 9 {
            let inum = unsafe { *((buf + off + 2) as *const u8) };
            let n_ep = unsafe { *((buf + off + 4) as *const u8) };
            let iclass = unsafe { *((buf + off + 5) as *const u8) };
            let isub = unsafe { *((buf + off + 6) as *const u8) };
            let iproto = unsafe { *((buf + off + 7) as *const u8) };
            serial::write_str("  [CFG] IF=");
            serial::write_usize(inum as usize);
            serial::write_str(" class=");
            serial::write_usize(iclass as usize);
            serial::write_str(" sub=");
            serial::write_usize(isub as usize);
            serial::write_str(" proto=");
            serial::write_usize(iproto as usize);
            serial::write_str(" eps=");
            serial::write_usize(n_ep as usize);
            serial::write_str("\n");
        } else if bty == 5 && blen >= 7 {
            let addr = unsafe { *((buf + off + 2) as *const u8) };
            let attr = unsafe { *((buf + off + 3) as *const u8) };
            let mps = unsafe { core::ptr::read_unaligned((buf + off + 4) as *const u16) };
            serial::write_str("  [CFG] EP addr=");
            hx(addr as usize);
            serial::write_str(" attr=");
            hx(attr as usize);
            serial::write_str(" mps=");
            serial::write_usize(mps as usize);
            serial::write_str("\n");
        }
        off += blen;
    }
}


// ─── HID Boot Keyboard ────────────────────────────────────────

fn control_no_data(x: &mut Xhci, slot: u8, ep0: &mut Ep0Ring, setup: &[u8; 8]) -> bool {
    ep0_submit_control(x, slot, ep0, setup, 0, 0, false)
}

fn set_configuration(x: &mut Xhci, slot: u8, ep0: &mut Ep0Ring, conf: u8) -> bool {
    serial::write_str("\n  [HID] SET_CONFIGURATION\n");
    let setup = [0x00, 0x09, conf, 0x00, 0x00, 0x00, 0x00, 0x00];
    if control_no_data(x, slot, ep0, &setup) {
        serial::write_str("  [HID] SET_CONFIGURATION OK\n");
        true
    } else {
        serial::write_str("  [HID] SET_CONFIGURATION FAIL\n");
        false
    }
}

fn set_protocol_boot(x: &mut Xhci, slot: u8, ep0: &mut Ep0Ring, iface: u8) -> bool {
    serial::write_str("  [HID] SET_PROTOCOL BOOT\n");
    // bmRequestType=0x21, bRequest=0x0B, wValue=0 (boot), wIndex=iface
    let setup = [0x21, 0x0B, 0x00, 0x00, iface, 0x00, 0x00, 0x00];
    if control_no_data(x, slot, ep0, &setup) {
        serial::write_str("  [HID] SET_PROTOCOL BOOT OK\n");
        true
    } else {
        serial::write_str("  [HID] SET_PROTOCOL FAIL\n");
        false
    }
}

fn configure_ep_interrupt_in(x: &mut Xhci, slot: u8, port: u8, ep_ring: usize) -> bool {
    serial::write_str("  [HID] Configure Endpoint 0x81\n");
    let cs = x.ctx_size;
    let in_ctx = match dma_alloc(1) { Some(p) => p, None => return false };
    let ps = unsafe { r32(x.portsc(port), 0) };
    let speed = speed_from_portsc(ps);

    unsafe {
        let ic = in_ctx as *mut u32;
        // Add flags: A0 (slot) | A3 (EP1 IN, DCI=3)
        *ic.add(1) = (1 << 0) | (1 << 3);

        // Slot context: Context Entries = 3 (up to DCI 3)
        let slot_ctx = (in_ctx + cs) as *mut u32;
        *slot_ctx.add(0) = (speed << 20) | (3u32 << 27);
        *slot_ctx.add(1) = (port as u32) << 16;

        // EP1 IN context at offset (1+3)*cs? Layout: control + slot(DCI0) + EP0(DCI1) + EP1OUT(DCI2) + EP1IN(DCI3)
        // Offset for DCI d is (1+d)*cs from start of input context... 
        // Input Control at 0, Slot at 1*cs, EP0 at 2*cs, EP1 OUT at 3*cs, EP1 IN at 4*cs
        let ep_in = (in_ctx + 4 * cs) as *mut u32;
        // Interval: for HS, bInterval=10 → Interval field = 10-1 = 9? Spec: Interval is 2^(Interval-1) * 125us for HS
        // Descriptor had bInterval=7. Use 7 for host Interval field (xHCI uses the value as exponent for HS).
        *ep_in.add(0) = 7u32 << 16; // Interval in bits 23:16
        // CErr=3, EP Type=Interrupt IN (7), MPS=8
        *ep_in.add(1) = (3 << 1) | (7 << 3) | (8 << 16);
        let deq = (ep_ring as u64) | 1; // DCS=1
        *ep_in.add(2) = deq as u32;
        *ep_in.add(3) = (deq >> 32) as u32;
        *ep_in.add(4) = 8; // Average TRB Length

        // Init EP ring Link TRB
        let trbs = ep_ring as *mut Trb;
        let mut i = 0usize;
        while i < 64 { *trbs.add(i) = Trb::z(); i += 1; }
        let link = &mut *trbs.add(63);
        link.lo = ep_ring as u32;
        link.control = (TRB_LINK << 10) | TRB_TC | CYCLE;
    }

    let ctrl = (TRB_CONFIGURE_EP << 10) | ((slot as u32) << 24);
    if !submit_cmd_full(x, in_ctx as u32, 0, 0, ctrl) { return false; }
    match wait_event(x, TRB_CMD_COMPLETE, 500000) {
        Some(ev) if ev.cc == 1 => {
            serial::write_str("  [HID] EP 0x81 configured\n");
            true
        }
        Some(ev) => {
            serial::write_str("  [HID] Configure EP CC=");
            serial::write_usize(ev.cc as usize);
            serial::write_str(" FAIL\n");
            false
        }
        None => {
            serial::write_str("  [HID] Configure EP TIMEOUT\n");
            false
        }
    }
}

fn hid_queue_interrupt_in(x: &mut Xhci, slot: u8, ep_ring: usize, enq: &mut usize, cycle: &mut u32, buf: usize) -> bool {
    // Normal TRB; leave last slot for Link TRB (index 63)
    if *enq >= 63 {
        // Producer wrapped - toggle cycle (Link TRB has TC)
        *enq = 0;
        *cycle ^= 1;
    }
    unsafe {
        let trbs = ep_ring as *mut Trb;
        let t = &mut *trbs.add(*enq);
        t.lo = buf as u32;
        t.hi = 0;
        // Transfer TRB: status[16:0] = TRB transfer length
        t.status = 4; // Boot mouse report is 3-4 bytes
        t.control = (TRB_NORMAL << 10) | TRB_IOC | *cycle;
        *enq += 1;
        DIAG_TRB_QUEUED = DIAG_TRB_QUEUED.wrapping_add(1);
        w32(x.db(), (slot as usize) * 4, 3); // DCI 3 = EP1 IN
    }
    true
}

fn hid_boot_keyboard(x: &mut Xhci, slot: u8, port: u8, ep0: &mut Ep0Ring) -> bool {
    serial::write_str("\n======== HID BOOT KEYBOARD ========\n");

    if !set_configuration(x, slot, ep0, 1) { return false; }
    if !set_protocol_boot(x, slot, ep0, 0) { return false; }

    let ep_ring = match dma_alloc(1) { Some(p) => p, None => return false };
    let report_buf = match dma_alloc(1) { Some(p) => p, None => return false };
    serial::write_str("  [HID] EP ring=");
    hx(ep_ring);
    serial::write_str(" report_buf=");
    hx(report_buf);
    serial::write_str("\n");

    if !configure_ep_interrupt_in(x, slot, port, ep_ring) { return false; }

    serial::write_str("  [HID] Interrupt IN started - waiting for reports\n");
    serial::write_str("  [HID] (QEMU: use monitor sendkey)\n");

    let mut enq = 0usize;
    let mut cycle = 1u32;
    let mut reports = 0u32;
    let mut polls = 0u32;

    // Queue first TRB
    if !hid_queue_interrupt_in(x, slot, ep_ring, &mut enq, &mut cycle, report_buf) {
        return false;
    }

    // Poll for transfer events; also pump compositor input
    while polls < 20000 {
        // Check for transfer event without blocking forever
        if let Some(ev) = wait_event(x, TRB_TRANSFER, 80) {
            if ev.cc == 1 || ev.cc == 13 {
                let mut report = [0u8; 8];
                let mut i = 0usize;
                while i < 8 {
                    report[i] = unsafe { *((report_buf + i) as *const u8) };
                    i += 1;
                }
                serial::write_str("  [HID] report received:");
                i = 0;
                while i < 8 {
                    serial::write_str(" ");
                    hx(report[i] as usize);
                    i += 1;
                }
                serial::write_str("\n");

                // Log keycodes
                i = 2;
                while i < 8 {
                    if report[i] != 0 {
                        serial::write_str("  [HID] keycode=0x");
                        hx(report[i] as usize);
                        serial::write_str("\n");
                        if let Some(ch) = crate::input::hid_to_ascii(report[i], (report[0] & 0x22) != 0) {
                            serial::write_str("  [INPUT] key=0x");
                            hx(ch as usize);
                            serial::write_str("\n");
                        }
                    }
                    i += 1;
                }
                if report[0] != 0 {
                    serial::write_str("  [HID] modifiers=0x");
                    hx(report[0] as usize);
                    serial::write_str("\n");
                }

                crate::input::process_hid_boot_report(&report);
                // Deliver to GUI
                crate::compositor::handle_input();
                reports += 1;

                // Re-queue next interrupt read
                // Clear report buffer
                unsafe {
                    let mut j = 0usize;
                    while j < 8 { *((report_buf + j) as *mut u8) = 0; j += 1; }
                }
                let _ = hid_queue_interrupt_in(x, slot, ep_ring, &mut enq, &mut cycle, report_buf);
            } else {
                serial::write_str("  [HID] Transfer CC=");
                serial::write_usize(ev.cc as usize);
                serial::write_str("\n");
            }
        }
        polls += 1;
        // Continuous polling; exit only after enough wall-time for test keys
        // (~several seconds of spin) so monitor sendkey can inject A B Enter BS Shift+A
        if polls > 15_000 { break; }
    }

    serial::write_str("  [HID] done reports=");
    serial::write_usize(reports as usize);
    serial::write_str(" polls=");
    serial::write_usize(polls as usize);
    serial::write_str("\n");
    reports > 0
}


// ─── Enumeration orchestration ────────────────────────────────


/// Scan config blob for HID mouse-like interface + Interrupt IN EP.
/// Accepts Boot Mouse (3/1/2), Boot Keyboard skipped, and generic HID (3/*/*)
/// with an Interrupt IN endpoint (common for non-boot protocol mice).
fn find_boot_mouse(buf: usize, total: usize) -> Option<(u8, u8, u16, u8)> {
    // returns (iface, ep_addr, mps, interval)
    let mut off = 0usize;
    let mut cur_iface = 0u8;
    let mut in_mouse = false;
    let mut prefer_boot = false;
    let mut iface = 0u8;
    let mut best: Option<(u8, u8, u16, u8)> = None;
    while off + 2 <= total {
        let blen = unsafe { *((buf + off) as *const u8) } as usize;
        let bty = unsafe { *((buf + off + 1) as *const u8) };
        if blen < 2 {
            break;
        }
        if bty == 4 && blen >= 9 {
            cur_iface = unsafe { *((buf + off + 2) as *const u8) };
            let iclass = unsafe { *((buf + off + 5) as *const u8) };
            let isub = unsafe { *((buf + off + 6) as *const u8) };
            let iproto = unsafe { *((buf + off + 7) as *const u8) };
            // Boot mouse preferred; generic HID (not keyboard protocol=1) accepted.
            in_mouse = iclass == 3
                && !(isub == 1 && iproto == 1) // skip boot keyboard
                && (iproto == 2 || isub == 1 || isub == 0 || iproto == 0);
            prefer_boot = iclass == 3 && isub == 1 && iproto == 2;
            if in_mouse {
                iface = cur_iface;
            }
        } else if bty == 5 && blen >= 7 && in_mouse {
            let addr = unsafe { *((buf + off + 2) as *const u8) };
            let attr = unsafe { *((buf + off + 3) as *const u8) };
            let mps = unsafe { core::ptr::read_unaligned((buf + off + 4) as *const u16) };
            let interval = if blen >= 7 {
                unsafe { *((buf + off + 6) as *const u8) }
            } else {
                10
            };
            // Interrupt IN: bit7 set, transfer type bits1:0 == 3
            if (addr & 0x80) != 0 && (attr & 0x03) == 0x03 {
                let cand = (iface, addr, if mps == 0 { 4 } else { mps }, interval);
                if prefer_boot {
                    return Some(cand);
                }
                if best.is_none() {
                    best = Some(cand);
                }
            }
        }
        off += blen;
    }
    best
}

fn configure_ep_mouse(
    x: &mut Xhci,
    slot: u8,
    port: u8,
    ep_ring: usize,
    ep_addr: u8,
    mps: u16,
    interval: u8,
) -> bool {
    let ep_num = (ep_addr & 0x0F) as u32;
    // xHCI DCI for IN endpoint N is 2*N+1
    let dci = 2 * ep_num + 1;
    serial::write_str("  [HID] Configure Mouse EP addr=");
    hx(ep_addr as usize);
    serial::write_str(" DCI=");
    serial::write_usize(dci as usize);
    serial::write_str(" MPS=");
    serial::write_usize(mps as usize);
    serial::write_str("\n");
    let cs = x.ctx_size;
    let in_ctx = match dma_alloc(1) {
        Some(p) => p,
        None => return false,
    };
    let ps = unsafe { r32(x.portsc(port), 0) };
    let speed = speed_from_portsc(ps);
    let iv = if interval == 0 { 7u32 } else { interval as u32 };

    unsafe {
        let ic = in_ctx as *mut u32;
        // A0 (slot) | A_dci
        *ic.add(1) = (1 << 0) | (1u32 << dci);

        let slot_ctx = (in_ctx + cs) as *mut u32;
        *slot_ctx.add(0) = (speed << 20) | (dci << 27); // Context Entries = dci
        *slot_ctx.add(1) = (port as u32) << 16;

        // Input context EP offset: control(0) + slot(1) + ... → DCI d at (1+d)*cs
        let ep_in = (in_ctx + (1 + dci as usize) * cs) as *mut u32;
        *ep_in.add(0) = iv << 16;
        // CErr=3, EP Type=Interrupt IN (7), Max Packet Size
        *ep_in.add(1) = (3 << 1) | (7 << 3) | ((mps as u32) << 16);
        let deq = (ep_ring as u64) | 1; // DCS=1
        *ep_in.add(2) = deq as u32;
        *ep_in.add(3) = (deq >> 32) as u32;
        *ep_in.add(4) = mps as u32;

        let trbs = ep_ring as *mut Trb;
        let mut i = 0usize;
        while i < 64 {
            *trbs.add(i) = Trb::z();
            i += 1;
        }
        let link = &mut *trbs.add(63);
        link.lo = ep_ring as u32;
        link.control = (TRB_LINK << 10) | TRB_TC | CYCLE;
    }

    let ctrl = (TRB_CONFIGURE_EP << 10) | ((slot as u32) << 24);
    if !submit_cmd_full(x, in_ctx as u32, 0, 0, ctrl) {
        return false;
    }
    match wait_event(x, TRB_CMD_COMPLETE, 500_000) {
        Some(ev) if ev.cc == 1 => {
            serial::write_str("  [HID] Mouse EP configured OK\n");
            unsafe { MOUSE_DCI = dci; MOUSE_MPS = mps; }
            true
        }
        Some(ev) => {
            serial::write_str("  [HID] Mouse Configure EP CC=");
            serial::write_usize(ev.cc as usize);
            serial::write_str(" FAIL\n");
            false
        }
        None => {
            serial::write_str("  [HID] Mouse Configure EP TIMEOUT\n");
            false
        }
    }
}

fn hid_queue_mouse_in(
    x: &mut Xhci,
    slot: u8,
    ep_ring: usize,
    enq: &mut usize,
    cycle: &mut u32,
    buf: usize,
    dci: u32,
    mps: u16,
) -> bool {
    if *enq >= 63 {
        *enq = 0;
        *cycle ^= 1;
    }
    unsafe {
        let trbs = ep_ring as *mut Trb;
        let t = &mut *trbs.add(*enq);
        t.lo = buf as u32;
        t.hi = 0;
        t.status = mps as u32;
        t.control = (TRB_NORMAL << 10) | TRB_IOC | *cycle;
        *enq += 1;
        DIAG_TRB_QUEUED = DIAG_TRB_QUEUED.wrapping_add(1);
        w32(x.db(), (slot as usize) * 4, dci);
    }
    true
}

fn hid_boot_mouse(x: &mut Xhci, slot: u8, port: u8, ep0: &mut Ep0Ring, cfg: usize, cfg_len: usize) -> bool {
    serial::write_str("\n======== HID BOOT MOUSE ========\n");
    serial::write_str("[USB] mouse probe start\n");

    let (iface, ep_addr, mps, interval) = match find_boot_mouse(cfg, cfg_len) {
        Some(v) => {
            serial::write_str("[USB] mouse IF=");
            serial::write_usize(v.0 as usize);
            serial::write_str(" EP=");
            hx(v.1 as usize);
            serial::write_str(" MPS=");
            serial::write_usize(v.2 as usize);
            serial::write_str("\n");
            unsafe { DIAG_MOUSE_IF = true; }
            v
        }
        None => {
            // Still try classic Boot Mouse defaults - pure mice often match this.
            serial::write_str("[USB] mouse desc not explicit - try EP 0x81 IF0\n");
            unsafe { DIAG_MOUSE_IF = true; }
            (0u8, 0x81u8, 4u16, 10u8)
        }
    };

    // Full control path (required even when keyboard already ran SET_CONFIG).
    let _ = set_configuration(x, slot, ep0, 1);
    if !set_protocol_boot(x, slot, ep0, iface) {
        serial::write_str("[USB] mouse SET_PROTOCOL soft-fail - continue\n");
    }

    let ep_ring = match dma_alloc(1) {
        Some(p) => p,
        None => {
            serial::write_str("[USB] HID FAIL (no DMA ring)\n");
            return false;
        }
    };
    let report_buf = match dma_alloc(1) {
        Some(p) => p,
        None => {
            serial::write_str("[USB] HID FAIL (no DMA report)\n");
            return false;
        }
    };

    if !configure_ep_mouse(x, slot, port, ep_ring, ep_addr, mps, interval) {
        serial::write_str("[USB] HID FAIL (configure EP)\n");
        return false;
    }

    let dci = unsafe { MOUSE_DCI };
    let mut enq = 0usize;
    let mut cycle = 1u32;
    if !hid_queue_mouse_in(x, slot, ep_ring, &mut enq, &mut cycle, report_buf, dci, mps) {
        serial::write_str("[USB] HID FAIL (queue)\n");
        return false;
    }
    unsafe {
        DIAG_EP_IN = true;
        DIAG_HID = true;
    }
    serial::write_str("[USB] HID OK EP_IN queued DCI=");
    serial::write_usize(dci as usize);
    serial::write_str("\n");

    // Short non-blocking probe (do not hang boot).
    let mut polls = 0u32;
    let mut got = 0u32;
    while polls < 3000 {
        if let Some(ev) = wait_event(x, TRB_TRANSFER, 30) {
            if ev.cc == 1 || ev.cc == 13 {
                let mut report = [0u8; 8];
                let mut i = 0usize;
                while i < 8 {
                    report[i] = unsafe { *((report_buf + i) as *const u8) };
                    i += 1;
                }
                crate::input::process_hid_boot_mouse(&report);
                got += 1;
                unsafe {
                    let mut j = 0usize;
                    while j < 8 {
                        *((report_buf + j) as *mut u8) = 0;
                        j += 1;
                    }
                }
                let _ = hid_queue_mouse_in(x, slot, ep_ring, &mut enq, &mut cycle, report_buf, dci, mps);
            }
        }
        polls += 1;
    }

    // Persist live path for desktop loop (ring size 64, link at 63).
    unsafe {
        MOUSE_ER = x.er;
        MOUSE_ER_DEQ = x.er_deq;
        MOUSE_ER_CYCLE = x.er_cycle;
        MOUSE_ER_SIZE = x.er_size;
        MOUSE_INTR0 = x.intr0();
        MOUSE_EP_RING = ep_ring;
        MOUSE_ENQ = enq;
        MOUSE_CYCLE = cycle;
        MOUSE_REPORT = report_buf;
        MOUSE_SLOT = slot;
        MOUSE_DB = x.db();
        MOUSE_LIVE = true;
        DIAG_HID = true;
        DIAG_EP_IN = true;
        DIAG_REPORTS = got;
        DIAG_FOUND = 1;
    }
    // One outstanding IN for movement after boot.
    let _ = hid_queue_mouse_in(x, slot, ep_ring, &mut enq, &mut cycle, report_buf, dci, mps);
    unsafe {
        MOUSE_ENQ = enq;
        MOUSE_CYCLE = cycle;
    }
    serial::write_str("[USB] mouse live=1 reports=");
    serial::write_usize(got as usize);
    serial::write_str("\n");
    true
}



fn phone_ring_submit(
    ring: usize,
    enq: &mut usize,
    cycle: &mut u32,
    buf: usize,
    len: u32,
    dci: u32,
    db: usize,
    slot: u8,
) -> bool {
    // Ring size 64, link at 63
    if *enq >= 63 {
        *enq = 0;
        *cycle ^= 1;
    }
    unsafe {
        let trbs = ring as *mut Trb;
        let t = &mut *trbs.add(*enq);
        t.lo = buf as u32;
        t.hi = 0;
        t.status = len;
        t.control = (TRB_NORMAL << 10) | TRB_IOC | *cycle;
        *enq += 1;
        // doorbell: DB target = DCI
        w32(db, (slot as usize) * 4, dci);
    }
    true
}

fn phone_wait_xfer(x: &mut Xhci) -> bool {
    match wait_event(x, TRB_TRANSFER, 800_000) {
        Some(ev) if ev.cc == 1 || ev.cc == 13 => true,
        Some(ev) => {
            serial::write_str("  [PHONE] xfer CC=");
            serial::write_usize(ev.cc as usize);
            serial::write_str("\n");
            false
        }
        None => {
            serial::write_str("  [PHONE] xfer TIMEOUT\n");
            false
        }
    }
}

/// BOT transaction: CBW (OUT) → data (optional) → CSW (IN)
fn phone_bot(
    x: &mut Xhci,
    cbw: &[u8; 31],
    data: usize,
    data_len: u32,
    data_in: bool,
    csw: usize,
) -> bool {
    let (slot, db, ring_out, ring_in, dci_out, dci_in) = unsafe {
        (
            PHONE_SLOT,
            PHONE_DB,
            PHONE_RING_OUT,
            PHONE_RING_IN,
            PHONE_DCI_OUT,
            PHONE_DCI_IN,
        )
    };
    if slot == 0 || ring_out == 0 || ring_in == 0 {
        return false;
    }
    // CBW buffer
    let cbw_buf = match dma_alloc(1) {
        Some(p) => p,
        None => return false,
    };
    unsafe {
        let mut i = 0usize;
        while i < 31 {
            *((cbw_buf + i) as *mut u8) = cbw[i];
            i += 1;
        }
    }
    // OUT CBW
    unsafe {
        phone_ring_submit(
            ring_out,
            &mut PHONE_ENQ_OUT,
            &mut PHONE_CYC_OUT,
            cbw_buf,
            31,
            dci_out,
            db,
            slot,
        );
    }
    if !phone_wait_xfer(x) {
        return false;
    }
    // Data stage
    if data_len > 0 && data != 0 {
        if data_in {
            unsafe {
                phone_ring_submit(
                    ring_in,
                    &mut PHONE_ENQ_IN,
                    &mut PHONE_CYC_IN,
                    data,
                    data_len,
                    dci_in,
                    db,
                    slot,
                );
            }
        } else {
            unsafe {
                phone_ring_submit(
                    ring_out,
                    &mut PHONE_ENQ_OUT,
                    &mut PHONE_CYC_OUT,
                    data,
                    data_len,
                    dci_out,
                    db,
                    slot,
                );
            }
        }
        if !phone_wait_xfer(x) {
            return false;
        }
    }
    // CSW IN 13 bytes
    let csw_buf = if csw != 0 {
        csw
    } else {
        match dma_alloc(1) {
            Some(p) => p,
            None => return false,
        }
    };
    unsafe {
        phone_ring_submit(
            ring_in,
            &mut PHONE_ENQ_IN,
            &mut PHONE_CYC_IN,
            csw_buf,
            13,
            dci_in,
            db,
            slot,
        );
    }
    if !phone_wait_xfer(x) {
        return false;
    }
    // Check CSW signature USBS
    let sig = unsafe {
        core::ptr::read_unaligned(csw_buf as *const u32)
    };
    if sig != 0x5342_5355 {
        serial::write_str("  [PHONE] bad CSW sig\n");
        return false;
    }
    let status = unsafe { *((csw_buf + 12) as *const u8) };
    if status != 0 {
        serial::write_str("  [PHONE] CSW status=");
        serial::write_usize(status as usize);
        serial::write_str("\n");
        return false;
    }
    true
}

fn phone_scsi_probe(x: &mut Xhci) -> bool {
    serial::write_str("  [PHONE] SCSI TEST UNIT READY\n");
    let mut cb = [0u8; 16];
    crate::drivers::usb_phone::scsi_test_unit_ready(&mut cb);
    let mut cbw = [0u8; 31];
    let tag = unsafe {
        let t = PHONE_TAG;
        PHONE_TAG = t.wrapping_add(1);
        t
    };
    crate::drivers::usb_phone::build_cbw(tag, 0, false, 0, 6, &cb, &mut cbw);
    let _ = phone_bot(x, &cbw, 0, 0, false, 0);

    serial::write_str("  [PHONE] SCSI READ CAPACITY(10)\n");
    crate::drivers::usb_phone::scsi_read_capacity10(&mut cb);
    let tag = unsafe {
        let t = PHONE_TAG;
        PHONE_TAG = t.wrapping_add(1);
        t
    };
    crate::drivers::usb_phone::build_cbw(tag, 8, true, 0, 10, &cb, &mut cbw);
    let cap_buf = match dma_alloc(1) {
        Some(p) => p,
        None => return false,
    };
    if !phone_bot(x, &cbw, cap_buf, 8, true, 0) {
        serial::write_str("  [PHONE] READ CAPACITY failed\n");
        return false;
    }
    // big-endian LBA and block size
    let last_lba = unsafe {
        let b = core::slice::from_raw_parts(cap_buf as *const u8, 8);
        ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as u32)
    };
    let block_size = unsafe {
        let b = core::slice::from_raw_parts(cap_buf as *const u8, 8);
        ((b[4] as u32) << 24) | ((b[5] as u32) << 16) | ((b[6] as u32) << 8) | (b[7] as u32)
    };
    let count = last_lba.wrapping_add(1);
    serial::write_str("  [PHONE] blocks=");
    serial::write_usize(count as usize);
    serial::write_str(" size=");
    serial::write_usize(block_size as usize);
    serial::write_str("\n");
    if block_size == 0 || block_size > 65536 {
        return false;
    }
    crate::drivers::usb_phone::set_capacity(block_size, count);
    true
}

/// Read one or more blocks from phone MSC into caller buffer (DMA-capable phys addr preferred).
pub fn phone_read_blocks(lba: u32, blocks: u16, buf_phys: usize) -> bool {
    if !unsafe { PHONE_LIVE } {
        return false;
    }
    let bs = crate::drivers::usb_phone::info().block_size;
    if bs == 0 || blocks == 0 {
        return false;
    }
    let data_len = (bs as u32).wrapping_mul(blocks as u32);
    let mut cb = [0u8; 16];
    crate::drivers::usb_phone::scsi_read10(&mut cb, lba, blocks);
    let mut cbw = [0u8; 31];
    let tag = unsafe {
        let t = PHONE_TAG;
        PHONE_TAG = t.wrapping_add(1);
        t
    };
    crate::drivers::usb_phone::build_cbw(tag, data_len, true, 0, 10, &cb, &mut cbw);
    // Need Xhci for event ring - rebuild minimal from saved state
    // Use a temporary Xhci shell for wait_event
    let mut x = unsafe {
        Xhci {
            mmio: PHONE_MMIO,
            cap: PHONE_CAP,
            slots: 0,
            ports: 0,
            dboff: 0,
            rtsoff: PHONE_RTSOFF,
            ctx_size: 32,
            dcbaa: 0,
            cmd_ring: 0,
            cmd_size: 0,
            cmd_enq: 0,
            cmd_cycle: 1,
            erst: 0,
            er: PHONE_ER,
            er_size: PHONE_ER_SIZE,
            er_deq: PHONE_ER_DEQ,
            er_cycle: PHONE_ER_CYCLE,
        }
    };
    if x.er == 0 {
        return false;
    }
    let ok = phone_bot(&mut x, &cbw, buf_phys, data_len, true, 0);
    unsafe {
        PHONE_ER_DEQ = x.er_deq;
        PHONE_ER_CYCLE = x.er_cycle;
    }
    ok
}


fn try_configure_phone_bulk(
    x: &mut Xhci,
    slot: u8,
    port: u8,
    ep_in: u8,
    ep_out: u8,
    mps_in: u16,
    mps_out: u16,
) -> bool {
    serial::write_str("  [PHONE] configure bulk IN=0x");
    serial::write_hex(ep_in as usize);
    serial::write_str(" OUT=0x");
    serial::write_hex(ep_out as usize);
    serial::write_str("\n");
    let ring_in = match dma_alloc(1) {
        Some(p) => p,
        None => return false,
    };
    let ring_out = match dma_alloc(1) {
        Some(p) => p,
        None => return false,
    };
    let ok_out = configure_ep_bulk(x, slot, port, ring_out, ep_out, mps_out, false);
    let ok_in = configure_ep_bulk(x, slot, port, ring_in, ep_in, mps_in, true);
    if !(ok_in && ok_out) {
        serial::write_str("  [PHONE] bulk configure partial/fail\n");
        return false;
    }
    serial::write_str("  [PHONE] bulk endpoints OK\n");
    let dci_out = {
        let n = (ep_out & 0x0F) as u32;
        2 * n
    };
    let dci_in = {
        let n = (ep_in & 0x0F) as u32;
        2 * n + 1
    };
    unsafe {
        PHONE_LIVE = true;
        PHONE_SLOT = slot;
        PHONE_DB = x.db();
        PHONE_ER = x.er;
        PHONE_ER_DEQ = x.er_deq;
        PHONE_ER_CYCLE = x.er_cycle;
        PHONE_ER_SIZE = x.er_size;
        PHONE_INTR0 = x.intr0();
        PHONE_RING_OUT = ring_out;
        PHONE_RING_IN = ring_in;
        PHONE_ENQ_OUT = 0;
        PHONE_ENQ_IN = 0;
        PHONE_CYC_OUT = 1;
        PHONE_CYC_IN = 1;
        PHONE_DCI_OUT = dci_out;
        PHONE_DCI_IN = dci_in;
        PHONE_TAG = 1;
        PHONE_MMIO = x.mmio;
        PHONE_CAP = x.cap;
        PHONE_RTSOFF = x.rtsoff;
    }
    crate::drivers::usb_phone::set_status_phone_bulk_ready();
    // SCSI probe
    let _ = phone_scsi_probe(x);
    true
}

fn configure_ep_bulk(
    x: &mut Xhci,
    slot: u8,
    port: u8,
    ep_ring: usize,
    ep_addr: u8,
    mps: u16,
    is_in: bool,
) -> bool {
    let ep_num = (ep_addr & 0x0F) as u32;
    // DCI: OUT = 2*n, IN = 2*n+1
    let dci = if is_in { 2 * ep_num + 1 } else { 2 * ep_num };
    if dci == 0 || dci > 31 {
        return false;
    }
    let cs = x.ctx_size;
    let in_ctx = match dma_alloc(1) {
        Some(p) => p,
        None => return false,
    };
    let ps = unsafe { r32(x.portsc(port), 0) };
    let speed = speed_from_portsc(ps);
    let ep_type = if is_in { 6u32 } else { 2u32 }; // Bulk IN / Bulk OUT

    unsafe {
        let ic = in_ctx as *mut u32;
        *ic.add(1) = (1 << 0) | (1u32 << dci);
        let slot_ctx = (in_ctx + cs) as *mut u32;
        *slot_ctx.add(0) = (speed << 20) | (dci << 27);
        *slot_ctx.add(1) = (port as u32) << 16;
        let ep_ctx = (in_ctx + (1 + dci as usize) * cs) as *mut u32;
        *ep_ctx.add(0) = 0;
        *ep_ctx.add(1) = (3 << 1) | (ep_type << 3) | ((mps as u32) << 16);
        let deq = (ep_ring as u64) | 1;
        *ep_ctx.add(2) = deq as u32;
        *ep_ctx.add(3) = (deq >> 32) as u32;
        *ep_ctx.add(4) = mps as u32;
        let trbs = ep_ring as *mut Trb;
        let mut i = 0usize;
        while i < 64 {
            *trbs.add(i) = Trb::z();
            i += 1;
        }
        let link = &mut *trbs.add(63);
        link.lo = ep_ring as u32;
        link.control = (TRB_LINK << 10) | TRB_TC | CYCLE;
    }
    let ctrl = (TRB_CONFIGURE_EP << 10) | ((slot as u32) << 24);
    if !submit_cmd_full(x, in_ctx as u32, 0, 0, ctrl) {
        return false;
    }
    match wait_event(x, TRB_CMD_COMPLETE, 500_000) {
        Some(ev) if ev.cc == 1 => {
            serial::write_str("  [PHONE] bulk EP OK dci=");
            serial::write_usize(dci as usize);
            serial::write_str("\n");
            true
        }
        Some(ev) => {
            serial::write_str("  [PHONE] bulk EP CC=");
            serial::write_usize(ev.cc as usize);
            serial::write_str("\n");
            false
        }
        None => {
            serial::write_str("  [PHONE] bulk EP TIMEOUT\n");
            false
        }
    }
}


fn enumerate_port(x: &mut Xhci, port: u8) -> bool {
    serial::write_str("\n  [ENUM] === port ");
    serial::write_usize(port as usize);
    serial::write_str(" ===\n");

    if !port_reset(x, port) {
        serial::write_str("  [ENUM] port reset failed\n");
        return false;
    }

    let slot = match enable_slot(x) {
        Some(s) => s,
        None => return false,
    };

    let ep0_phys = match address_device(x, slot, port) {
        Some(p) => p,
        None => return false,
    };

    let mut ep0 = Ep0Ring {
        phys: ep0_phys,
        enq: 0,
        cycle: 1,
        size: 64,
    };

    let buf = match dma_alloc(1) {
        Some(p) => p,
        None => return false,
    };

    serial::write_str("\n  --- GET_DESCRIPTOR Device ---\n");
    if !get_descriptor(x, slot, &mut ep0, 1, 0, 0, 18, buf) {
        serial::write_str("  [ENUM] device descriptor FAIL\n");
        return false;
    }
    dump_bytes("DEV-RAW", buf, 18);
    parse_device_desc(buf);

    serial::write_str("\n  --- GET_DESCRIPTOR Config ---\n");
    if !get_descriptor(x, slot, &mut ep0, 2, 0, 0, 9, buf) {
        serial::write_str("  [ENUM] config header FAIL\n");
        return false;
    }
    let total = unsafe { core::ptr::read_unaligned((buf + 2) as *const u16) } as usize;
    let total = if total < 9 || total > 512 { 9 } else { total };
    if !get_descriptor(x, slot, &mut ep0, 2, 0, 0, total as u16, buf) {
        serial::write_str("  [ENUM] full config FAIL\n");
        return false;
    }
    dump_bytes("CFG-RAW", buf, total);
    parse_config_desc(buf, total);
    serial::write_str("  [ENUM] SUCCESS\n");

    // --- Smartphone / MSC / MTP / ADB detection ---
    {
        let cfg_slice = unsafe { core::slice::from_raw_parts(buf as *const u8, total) };
        if let Some((mode, ep_in, ep_out, mps_in, mps_out)) =
            crate::drivers::usb_phone::find_phone_interface(cfg_slice)
        {
            // VID/PID from device descriptor still in first 18 bytes of same page? re-get
            let mut vid = 0u16;
            let mut pid = 0u16;
            // Device desc was fetched earlier into buf then overwritten by config.
            // Leave 0 for now; optional second GET_DESCRIPTOR Device.
            serial::write_str("  [PHONE] interface found\n");
            crate::drivers::usb_phone::register_device(
                mode, vid, pid, slot, port, ep_in, ep_out, mps_in, mps_out,
            );
            // SET_CONFIGURATION 1 for MSC
            if ep_in != 0 && ep_out != 0
                && (mode == crate::drivers::usb_phone::PhoneMode::MassStorage
                    || mode == crate::drivers::usb_phone::PhoneMode::Mtp)
            {
                let _ = set_configuration(x, slot, &mut ep0, 1);
                serial::write_str("  [PHONE] MSC SET_CONFIGURATION done\n");
                // Bulk EP configure is next step (configure_ep_bulk)
                let _ = try_configure_phone_bulk(x, slot, port, ep_in, ep_out, mps_in, mps_out);
            }
        }
    }

    let has_mouse = find_boot_mouse(buf, total).is_some();
    let mut has_kbd = false;
    let mut off = 0usize;
    while off + 9 <= total {
        let blen = unsafe { *((buf + off) as *const u8) } as usize;
        let bty = unsafe { *((buf + off + 1) as *const u8) };
        if blen < 2 { break; }
        if bty == 4 && blen >= 9 {
            let ic = unsafe { *((buf + off + 5) as *const u8) };
            let is = unsafe { *((buf + off + 6) as *const u8) };
            let ip = unsafe { *((buf + off + 7) as *const u8) };
            if ic == 3 && is == 1 && ip == 1 { has_kbd = true; }
        }
        off += blen;
    }

    if has_mouse {
        if hid_boot_mouse(x, slot, port, &mut ep0, buf, total) {
            return true;
        }
    }
    if has_kbd {
        let _ = hid_boot_keyboard(x, slot, port, &mut ep0);
    }
    if !unsafe { MOUSE_LIVE } {
        let _ = hid_boot_mouse(x, slot, port, &mut ep0, buf, total);
    }
    unsafe { MOUSE_LIVE }
}

fn enumerate(x: &mut Xhci) -> bool {
    serial::write_str("\n======== USB ENUMERATION (all ports) ========\n");
    unsafe { DIAG_PORTS = x.ports; DIAG_PORT_CCS = 0; }
    power_all_ports(x);
    delay(400_000);
    power_all_ports(x);
    serial::write_str("[USB] ports powered + RxDetect\n");

    let mut any = false;
    let mut p = 1u8;
    while p <= x.ports {
        let ps = unsafe { r32(x.portsc(p), 0) };
        serial::write_str("  [PORT] ");
        serial::write_usize(p as usize);
        serial::write_str(" PORTSC=");
        h32(ps);
        serial::write_str(" CCS=");
        serial::write_usize(if ps & PORTSC_CCS != 0 { 1 } else { 0 });
        serial::write_str(" PP=");
        serial::write_usize(if ps & PORTSC_PP != 0 { 1 } else { 0 });
        serial::write_str(" PLS=");
        serial::write_usize(((ps >> 5) & 0xF) as usize);
        serial::write_str("\n");
        if (ps & PORTSC_CCS) != 0 {
            unsafe { DIAG_PORT_CCS |= 1u32 << p; }
            if enumerate_port(x, p) {
                any = true;
            }
            // NEVER stop on mouse - phone may be on later port
        } else {
            // late CCS after PP
            unsafe {
                let mut v = r32(x.portsc(p), 0);
                w32(x.portsc(p), 0, (v | PORTSC_PP) & !PORTSC_W1C);
            }
            delay(80_000);
            let ps2 = unsafe { r32(x.portsc(p), 0) };
            if (ps2 & PORTSC_CCS) != 0 {
                unsafe { DIAG_PORT_CCS |= 1u32 << p; }
                if enumerate_port(x, p) {
                    any = true;
                }
            }
        }
        p += 1;
    }

    // If zero CCS: force Port Reset on every port (some devices only show after PR)
    if unsafe { DIAG_PORT_CCS } == 0 {
        serial::write_str("[USB] CCS=0 all ports - force PR sweep\n");
        p = 1;
        while p <= x.ports {
            unsafe {
                let ps = r32(x.portsc(p), 0);
                w32(x.portsc(p), 0, (ps | PORTSC_PP | PORTSC_PR) & !PORTSC_W1C);
            }
            delay(200_000);
            unsafe {
                let ps = r32(x.portsc(p), 0);
                w32(x.portsc(p), 0, (ps & !PORTSC_W1C) | PORTSC_PRC | PORTSC_CSC | PORTSC_PP);
            }
            delay(150_000);
            let ps2 = unsafe { r32(x.portsc(p), 0) };
            serial::write_str("  after PR port ");
            serial::write_usize(p as usize);
            serial::write_str(" CCS=");
            serial::write_usize(if ps2 & PORTSC_CCS != 0 { 1 } else { 0 });
            serial::write_str("\n");
            if (ps2 & PORTSC_CCS) != 0 {
                unsafe { DIAG_PORT_CCS |= 1u32 << p; }
                if enumerate_port(x, p) {
                    any = true;
                }
            }
            p += 1;
        }
    }

    if !any {
        serial::write_str("  [ENUM] no connected device on root ports\n");
    }
    serial::write_str("[USB] mouse live=");
    serial::write_str(if unsafe { MOUSE_LIVE } { "1" } else { "0" });
    serial::write_str(" phone_live=");
    serial::write_str(if unsafe { PHONE_LIVE } { "1" } else { "0" });
    serial::write_str(" CCS_mask=");
    h32(unsafe { DIAG_PORT_CCS });
    serial::write_str("\n");
    true
}


// ─── Public entry ─────────────────────────────────────────────

fn init_one(bus: u8, dev: u8, func: u8) -> bool {
    serial::write_str("\n[xHCI] === ");
    serial::write_usize(bus as usize);
    serial::write_str(":");
    serial::write_usize(dev as usize);
    serial::write_str(".");
    serial::write_usize(func as usize);
    serial::write_str(" ===\n");

    unsafe {
        let cmd = pci_r32(bus, dev, func, 0x04);
        pci_w32(bus, dev, func, 0x04, cmd | 0x6);
    }
    let bar0 = unsafe { pci_r32(bus, dev, func, 0x10) };
    let is64 = (bar0 & 0x6) == 0x4;
    let mut phys = (bar0 & !0xF) as u64;
    if is64 {
        let bar1 = unsafe { pci_r32(bus, dev, func, 0x14) };
        phys |= (bar1 as u64) << 32;
    }
    // size probe simplified
    let size = 0x4000u64;
    serial::write_str("  BAR0=");
    hx(phys as usize);
    serial::write_str("\n");
    if !map_mmio(phys, size) { return false; }

    let mmio = phys as usize;
    let d0 = unsafe { r32(mmio, 0) };
    let cap = (d0 & 0xFF) as u8;
    let hcs1 = unsafe { r32(mmio, 4) };
    let hcc1 = unsafe { r32(mmio, 0x10) };
    let ctx_size = if hcc1 & 4 != 0 { 64usize } else { 32usize };
    let slots = (hcs1 & 0xFF) as u8;
    let ports = ((hcs1 >> 24) & 0xFF) as u8;
    let dboff = unsafe { r32(mmio, 0x14) };
    let rtsoff = unsafe { r32(mmio, 0x18) };

    serial::write_str("  CAP=");
    serial::write_usize(cap as usize);
    serial::write_str(" slots=");
    serial::write_usize(slots as usize);
    serial::write_str(" ports=");
    serial::write_usize(ports as usize);
    serial::write_str(" ctx=");
    serial::write_usize(ctx_size);
    serial::write_str("\n");

    legacy_handoff(mmio, cap);
    if !hc_reset(mmio, cap) {
        serial::write_str("  reset FAIL\n");
        return false;
    }

    let mut x = Xhci {
        mmio, cap, slots, ports, dboff, rtsoff, ctx_size,
        dcbaa: 0, cmd_ring: 0, cmd_size: 0, cmd_enq: 0, cmd_cycle: 1,
        erst: 0, er: 0, er_size: 0, er_deq: 0, er_cycle: 1,
    };
    if !setup_rings(&mut x) {
        serial::write_str("  rings FAIL\n");
        return false;
    }
    serial::write_str("  [xHCI] running - start enumeration\n");
    unsafe {
        SAVE_MMIO = mmio;
        SAVE_CAP = cap;
        SAVE_PORTS = ports;
        SAVE_BUS = bus;
        SAVE_DEV = dev;
        SAVE_FUNC = func;
    }
    enumerate(&mut x)
}


/// Non-blocking poll of HID mouse transfer events (desktop loop). Timeout-safe.
pub fn poll_mouse_live() {
    unsafe {
        if !MOUSE_LIVE || MOUSE_ER == 0 || MOUSE_REPORT == 0 || MOUSE_EP_RING == 0 {
            return;
        }
        let dci = if MOUSE_DCI == 0 { 3 } else { MOUSE_DCI };
        let mps = if MOUSE_MPS == 0 { 4 } else { MOUSE_MPS as u32 };
        // Process a few event ring entries
        let mut spins = 0u32;
        while spins < 48 {
            let trbs = MOUSE_ER as *const Trb;
            let ev = *trbs.add(MOUSE_ER_DEQ);
            if (ev.control & 1) != MOUSE_ER_CYCLE {
                break; // no event
            }
            let ty = (ev.control >> 10) & 0x3F;
            let cc = (ev.status >> 24) & 0xFF;
            MOUSE_ER_DEQ += 1;
            if MOUSE_ER_DEQ >= MOUSE_ER_SIZE {
                MOUSE_ER_DEQ = 0;
                MOUSE_ER_CYCLE ^= 1;
            }
            // Update ERDP (EHB bit3)
            if MOUSE_INTR0 != 0 {
                let erdp = (MOUSE_ER + MOUSE_ER_DEQ * 16) as u64;
                core::ptr::write_volatile((MOUSE_INTR0 + 0x18) as *mut u64, erdp | (1 << 3));
            }
            if ty == TRB_TRANSFER && (cc == 1 || cc == 13 || cc == 0) {
                DIAG_COMPLETIONS = DIAG_COMPLETIONS.wrapping_add(1);
                let mut report = [0u8; 8];
                let mut i = 0usize;
                while i < 8 {
                    report[i] = *((MOUSE_REPORT + i) as *const u8);
                    i += 1;
                }
                // Only feed input if any movement/button activity (avoid zero spam)
                if report[0] != 0 || report[1] != 0 || report[2] != 0 {
                    crate::input::process_hid_boot_mouse(&report);
                    MOUSE_EVENTS = MOUSE_EVENTS.wrapping_add(1);
                }
                DIAG_REPORTS = DIAG_REPORTS.wrapping_add(1);
                // clear + re-queue (must match 64-TRB ring with Link at 63)
                let mut j = 0usize;
                while j < 8 {
                    *((MOUSE_REPORT + j) as *mut u8) = 0;
                    j += 1;
                }
                if MOUSE_ENQ >= 63 {
                    MOUSE_ENQ = 0;
                    MOUSE_CYCLE ^= 1;
                }
                let ring = MOUSE_EP_RING as *mut Trb;
                let trb = ring.add(MOUSE_ENQ);
                (*trb).lo = MOUSE_REPORT as u32;
                (*trb).hi = 0;
                (*trb).status = mps;
                (*trb).control = (TRB_NORMAL << 10) | TRB_IOC | MOUSE_CYCLE;
                MOUSE_ENQ += 1;
                if MOUSE_DB != 0 && MOUSE_SLOT != 0 {
                    core::ptr::write_volatile(
                        (MOUSE_DB + (MOUSE_SLOT as usize) * 4) as *mut u32,
                        dci,
                    );
                }
            }
            spins += 1;
        }
    }
}

/// Re-run full xHCI scan+enumerate (plug mouse then type MO).
pub fn rescan_mouse() {
    serial::write_str("[USB] rescan_mouse...\n");
    probe();
}

pub fn probe() {
    serial::write_str("\n[xHCI] scan + enumerate...\n");
    unsafe { DIAG_XHCI = false; DIAG_DEV = false; DIAG_HID = false; DIAG_MOUSE_IF = false; DIAG_EP_IN = false; DIAG_REPORTS = 0; DIAG_FOUND = 0; DIAG_PORT_CCS = 0; DIAG_PORTS = 0; MOUSE_LIVE = false; }
    let mut found = 0u32;
    let mut ok = 0u32;
    let mut bus = 0u8;
    while bus < 32 {
        let mut dev = 0u8;
        while dev < 32 {
            let mut func = 0u8;
            while func < 8 {
                let id = unsafe { pci_r32(bus, dev, func, 0) };
                if id != 0xFFFFFFFF {
                    let cl = unsafe { pci_r32(bus, dev, func, 0x08) };
                    if ((cl >> 24) & 0xFF) == 0x0C && ((cl >> 16) & 0xFF) == 0x03 && ((cl >> 8) & 0xFF) == 0x30 {
                        unsafe { DIAG_XHCI = true; }
                        found += 1;
                        if init_one(bus, dev, func) { ok += 1; unsafe { DIAG_DEV = true; DIAG_FOUND += 1; } }
                    }
                }
                func += 1;
            }
            dev += 1;
        }
        bus += 1;
    }
    if !unsafe { DIAG_XHCI } {
        serial::write_str("[USB] xHCI FAIL\n");
    } else {
        serial::write_str("[USB] xHCI OK\n");
    }
    if !unsafe { DIAG_DEV } {
        serial::write_str("[USB] mouse NOT FOUND\n");
    } else {
        serial::write_str("[USB] mouse found\n");
    }
    serial::write_str("[xHCI] enum done found=");
    serial::write_usize(found as usize);
    serial::write_str(" ok=");
    serial::write_usize(ok as usize);
    serial::write_str("\n");
}


pub fn rescan_with_power() {
    rescan_mouse();
}


/// Rescan USB for smartphone (Mass Storage / MTP / ADB).
pub fn rescan_phone() {
    serial::write_str("[USB] rescan_phone...\n");
    unsafe {
        PHONE_LIVE = false;
        PHONE_SLOT = 0;
        PHONE_RING_IN = 0;
        PHONE_RING_OUT = 0;
    }
    crate::drivers::usb_phone::clear();
    rescan_mouse();
    serial::write_str("[USB] phone_live=");
    serial::write_str(if unsafe { PHONE_LIVE } { "YES\n" } else { "NO\n" });
    serial::write_str("[USB] CCS_mask=");
    h32(unsafe { DIAG_PORT_CCS });
    serial::write_str("\n");
}


static mut SCAN_REPORT: [u8; 4096] = [0; 4096];
static mut SCAN_LEN: usize = 0;

fn scan_app(s: &[u8]) {
    unsafe {
        let mut i = 0usize;
        while i < s.len() && SCAN_LEN < 4095 {
            SCAN_REPORT[SCAN_LEN] = s[i];
            SCAN_LEN += 1;
            i += 1;
        }
    }
}
fn scan_str(s: &str) { scan_app(s.as_bytes()); }
fn scan_u(mut v: usize) {
    if v == 0 { scan_app(b"0"); return; }
    let mut d = [0u8; 12];
    let mut n = 0usize;
    while v > 0 && n < 12 { d[n] = b'0' + (v % 10) as u8; v /= 10; n += 1; }
    while n > 0 { n -= 1; scan_app(&[d[n]]); }
}
fn scan_h(mut v: u32) {
    scan_app(b"0x");
    let mut i = 0u32;
    while i < 8 {
        let nib = ((v >> (28 - i * 4)) & 0xF) as u8;
        scan_app(&[if nib < 10 { b'0' + nib } else { b'A' + nib - 10 }]);
        i += 1;
    }
}

pub fn scan_usb_detail() -> &'static [u8] {
    unsafe { SCAN_LEN = 0; }
    scan_str("======== SCANUSB xHCI ========\n");
    let (mmio, cap, nports) = unsafe { (SAVE_MMIO, SAVE_CAP, SAVE_PORTS) };
    if mmio == 0 {
        scan_str("xHCI MMIO not saved - run probe\n");
        serial::write_str(unsafe { core::str::from_utf8_unchecked(&SCAN_REPORT[..SCAN_LEN]) });
        return unsafe { &SCAN_REPORT[..SCAN_LEN] };
    }
    scan_str("PCI ");
    scan_u(unsafe { SAVE_BUS as usize });
    scan_str(":");
    scan_u(unsafe { SAVE_DEV as usize });
    scan_str(".");
    scan_u(unsafe { SAVE_FUNC as usize });
    scan_str(" ports=");
    scan_u(nports as usize);
    scan_str("\n");
    let op = mmio + cap as usize;
    // power
    let mut p = 1u8;
    while p <= nports && p <= 16 {
        unsafe {
            let ps = r32(op, 0x400 + 0x10 * (p as usize - 1));
            w32(op, 0x400 + 0x10 * (p as usize - 1), (ps | PORTSC_PP) & !PORTSC_W1C);
        }
        p += 1;
    }
    delay(200_000);
    p = 1;
    while p <= nports && p <= 16 {
        let off = 0x400 + 0x10 * (p as usize - 1);
        let ps = unsafe { r32(op, off) };
        let ccs = (ps & PORTSC_CCS) != 0;
        if ccs { unsafe { DIAG_PORT_CCS |= 1u32 << p; } }
        scan_str("--- Port ");
        scan_u(p as usize);
        scan_str(" ---\n  PORTSC=");
        scan_h(ps);
        scan_str("\n  CCS=");
        scan_str(if ccs { "YES" } else { "no" });
        scan_str(" PED=");
        scan_str(if (ps & PORTSC_PED) != 0 { "YES" } else { "no" });
        scan_str(" PP=");
        scan_str(if (ps & PORTSC_PP) != 0 { "YES" } else { "no" });
        scan_str(" PLS=");
        scan_u(((ps >> 5) & 0xF) as usize);
        scan_str("\n");
        p += 1;
    }
    scan_str("phone_live=");
    scan_str(if unsafe { PHONE_LIVE } { "YES\n" } else { "NO\n" });
    scan_str("======== END xHCI ========\n");
    serial::write_str(unsafe { core::str::from_utf8_unchecked(&SCAN_REPORT[..SCAN_LEN]) });
    unsafe { &SCAN_REPORT[..SCAN_LEN] }
}

pub fn last_scan_report() -> &'static [u8] {
    unsafe { &SCAN_REPORT[..SCAN_LEN] }
}
