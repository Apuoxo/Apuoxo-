//! AH532 hardware diagnostics only:
//! 1) Full PCI enumeration
//! 2) USB host + port topology / plug-unplug monitor
//! Does NOT fix HID. Does NOT replace xhci driver.

use crate::serial;
use crate::mm::paging;
use crate::mm;

unsafe fn pci_addr(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC)
}
unsafe fn pci_r32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let a = pci_addr(bus, dev, func, off);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let v: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") v, options(nostack, preserves_flags));
    v
}
unsafe fn pci_r16(bus: u8, dev: u8, func: u8, off: u8) -> u16 {
    let v = pci_r32(bus, dev, func, off & !3);
    ((v >> ((off as u32 & 2) * 8)) & 0xFFFF) as u16
}
unsafe fn pci_r8(bus: u8, dev: u8, func: u8, off: u8) -> u8 {
    let v = pci_r32(bus, dev, func, off & !3);
    ((v >> ((off as u32 & 3) * 8)) & 0xFF) as u8
}
unsafe fn pci_w16(bus: u8, dev: u8, func: u8, off: u8, val: u16) {
    let a = pci_addr(bus, dev, func, off & !3);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let mut old: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") old, options(nostack, preserves_flags));
    let shift = (off as u32 & 2) * 8;
    old = (old & !(0xFFFF << shift)) | ((val as u32) << shift);
    core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") old, options(nostack, preserves_flags));
}

fn map_bar(phys: u64, size: usize) -> bool {
    if phys == 0 || size == 0 {
        return false;
    }
    let cr3 = unsafe { paging::read_cr3() };
    let start = (phys as usize) & !0xFFF;
    let end = (phys as usize + size + 0xFFF) & !0xFFF;
    let mut va = start;
    while va < end {
        if va >= 0x4000_0000 {
            let _ = unsafe {
                paging::map_page(cr3, va, va, paging::PAGE_PRESENT | paging::PAGE_WRITE)
            };
        }
        va += 0x1000;
    }
    unsafe { paging::load_cr3(cr3) };
    true
}

unsafe fn r32(b: usize, o: usize) -> u32 {
    core::ptr::read_volatile((b + o) as *const u32)
}
unsafe fn w32(b: usize, o: usize, v: u32) {
    core::ptr::write_volatile((b + o) as *mut u32, v);
}

fn delay(n: u32) {
    let mut i = 0u32;
    while i < n {
        i += 1;
        core::hint::spin_loop();
    }
}

/// Full PCI dump - all devices
pub fn dump_all_pci() {
    serial::write_str("\n======== FULL PCI ENUMERATION ========\n");
    serial::write_str("B:D.F  VID:DID  CLASS/SUB/IF  BAR0..BAR5\n");
    let mut count = 0u32;
    for bus in 0u8..=0 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                unsafe {
                    let id = pci_r32(bus, dev, func, 0);
                    if id == 0xFFFF_FFFF || id == 0 {
                        continue;
                    }
                    let vid = (id & 0xFFFF) as u16;
                    let did = ((id >> 16) & 0xFFFF) as u16;
                    let cr = pci_r32(bus, dev, func, 0x08);
                    let rev = (cr & 0xFF) as u8;
                    let progif = ((cr >> 8) & 0xFF) as u8;
                    let sub = ((cr >> 16) & 0xFF) as u8;
                    let class = ((cr >> 24) & 0xFF) as u8;
                    count += 1;
                    serial::write_str("[PCI] ");
                    serial::write_usize(bus as usize);
                    serial::write_str(":");
                    serial::write_usize(dev as usize);
                    serial::write_str(".");
                    serial::write_usize(func as usize);
                    serial::write_str(" ");
                    serial::write_hex(vid as usize);
                    serial::write_str(":");
                    serial::write_hex(did as usize);
                    serial::write_str(" class=");
                    serial::write_hex(class as usize);
                    serial::write_str("/");
                    serial::write_hex(sub as usize);
                    serial::write_str("/");
                    serial::write_hex(progif as usize);
                    serial::write_str(" rev=");
                    serial::write_hex(rev as usize);
                    serial::write_str("\n");
                    let mut bi = 0u8;
                    while bi < 6 {
                        let bar = pci_r32(bus, dev, func, 0x10 + bi * 4);
                        if bar != 0 {
                            serial::write_str("  BAR");
                            serial::write_usize(bi as usize);
                            serial::write_str("=");
                            serial::write_hex(bar as usize);
                            serial::write_str("\n");
                        }
                        bi += 1;
                    }
                }
            }
        }
    }
    serial::write_str("[PCI] total devices=");
    serial::write_usize(count as usize);
    serial::write_str("\n");
}

struct UsbHc {
    bus: u8,
    dev: u8,
    func: u8,
    kind: u8, // 0x20 EHCI, 0x30 xHCI
    bar: u64,
    mmio: usize,
    ports: u8,
}

static mut HCS: [UsbHc; 8] = [
    UsbHc { bus: 0, dev: 0, func: 0, kind: 0, bar: 0, mmio: 0, ports: 0 },
    UsbHc { bus: 0, dev: 0, func: 0, kind: 0, bar: 0, mmio: 0, ports: 0 },
    UsbHc { bus: 0, dev: 0, func: 0, kind: 0, bar: 0, mmio: 0, ports: 0 },
    UsbHc { bus: 0, dev: 0, func: 0, kind: 0, bar: 0, mmio: 0, ports: 0 },
    UsbHc { bus: 0, dev: 0, func: 0, kind: 0, bar: 0, mmio: 0, ports: 0 },
    UsbHc { bus: 0, dev: 0, func: 0, kind: 0, bar: 0, mmio: 0, ports: 0 },
    UsbHc { bus: 0, dev: 0, func: 0, kind: 0, bar: 0, mmio: 0, ports: 0 },
    UsbHc { bus: 0, dev: 0, func: 0, kind: 0, bar: 0, mmio: 0, ports: 0 },
];
static mut HC_N: usize = 0;
static mut LAST_PORTSC: [[u32; 16]; 8] = [[0; 16]; 8];

fn enable_bus_master(bus: u8, dev: u8, func: u8) {
    unsafe {
        let cmd = pci_r16(bus, dev, func, 0x04);
        // Memory Space + Bus Master
        pci_w16(bus, dev, func, 0x04, cmd | 0x0006);
    }
}

fn discover_usb_hosts() {
    serial::write_str("\n======== USB HOST TOPOLOGY ========\n");
    unsafe {
        HC_N = 0;
    }
    for bus in 0u8..=0 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                unsafe {
                    let id = pci_r32(bus, dev, func, 0);
                    if id == 0xFFFF_FFFF || id == 0 {
                        continue;
                    }
                    let cr = pci_r32(bus, dev, func, 0x08);
                    let progif = ((cr >> 8) & 0xFF) as u8;
                    let sub = ((cr >> 16) & 0xFF) as u8;
                    let class = ((cr >> 24) & 0xFF) as u8;
                    if class != 0x0C || sub != 0x03 {
                        continue;
                    }
                    if progif != 0x30 && progif != 0x20 {
                        // still report other USB HCs
                        serial::write_str("[USB-HC] ");
                        serial::write_usize(bus as usize);
                        serial::write_str(":");
                        serial::write_usize(dev as usize);
                        serial::write_str(".");
                        serial::write_usize(func as usize);
                        serial::write_str(" ProgIF=");
                        serial::write_hex(progif as usize);
                        serial::write_str(" (not EHCI/xHCI detail)\n");
                        continue;
                    }
                    if HC_N >= 8 {
                        continue;
                    }
                    let mut bar0 = pci_r32(bus, dev, func, 0x10) as u64;
                    // 64-bit BAR
                    if bar0 & 0x4 != 0 {
                        let bar1 = pci_r32(bus, dev, func, 0x14) as u64;
                        bar0 = (bar0 & !0xF) | (bar1 << 32);
                    } else {
                        bar0 &= !0xF;
                    }
                    enable_bus_master(bus, dev, func);
                    let map_sz = if progif == 0x30 { 0x10000 } else { 0x1000 };
                    let _ = map_bar(bar0, map_sz);
                    let mmio = bar0 as usize;

                    let ports = if progif == 0x30 {
                        // xHCI: CAPLENGTH, HCSPARAMS1 max ports
                        let cap = r32(mmio, 0) & 0xFF;
                        let hcs1 = r32(mmio, 0x04);
                        let maxports = ((hcs1 >> 24) & 0xFF) as u8;
                        let hci_ver = (r32(mmio, 0) >> 16) & 0xFFFF;
                        serial::write_str("[USB-HC] xHCI ");
                        serial::write_usize(bus as usize);
                        serial::write_str(":");
                        serial::write_usize(dev as usize);
                        serial::write_str(".");
                        serial::write_usize(func as usize);
                        serial::write_str(" BAR=");
                        serial::write_hex(bar0 as usize);
                        serial::write_str(" CAPLEN=");
                        serial::write_usize(cap as usize);
                        serial::write_str(" HCIVER=");
                        serial::write_hex(hci_ver as usize);
                        serial::write_str(" ports=");
                        serial::write_usize(maxports as usize);
                        serial::write_str("\n");
                        // Minimal: try leave controller readable; read USBSTS
                        let op = mmio + cap as usize;
                        let usbsts = r32(op, 0x04);
                        serial::write_str("  USBSTS=");
                        serial::write_hex(usbsts as usize);
                        serial::write_str(" HCH=");
                        serial::write_usize(if usbsts & 1 != 0 { 1 } else { 0 });
                        serial::write_str("\n");
                        if maxports == 0 {
                            8
                        } else if maxports > 16 {
                            16
                        } else {
                            maxports
                        }
                    } else {
                        // EHCI: CAPLENGTH at +0, HCSPARAMS N_PORTS bits 3:0
                        let cap = (r32(mmio, 0) & 0xFF) as usize;
                        let hcs = r32(mmio, 0x04);
                        let nports = (hcs & 0xF) as u8;
                        serial::write_str("[USB-HC] EHCI ");
                        serial::write_usize(bus as usize);
                        serial::write_str(":");
                        serial::write_usize(dev as usize);
                        serial::write_str(".");
                        serial::write_usize(func as usize);
                        serial::write_str(" BAR=");
                        serial::write_hex(bar0 as usize);
                        serial::write_str(" ports=");
                        serial::write_usize(nports as usize);
                        serial::write_str("\n");
                        if nports == 0 {
                            6
                        } else if nports > 16 {
                            16
                        } else {
                            nports
                        }
                    };

                    HCS[HC_N] = UsbHc {
                        bus,
                        dev,
                        func,
                        kind: progif,
                        bar: bar0,
                        mmio,
                        ports,
                    };
                    HC_N += 1;
                }
            }
        }
    }
    unsafe {
        serial::write_str("[USB-HC] hosts=");
        serial::write_usize(HC_N);
        serial::write_str("\n");
    }
}

fn read_portsc(hc: &UsbHc, port: u8) -> u32 {
    unsafe {
        if hc.kind == 0x30 {
            let cap = r32(hc.mmio, 0) & 0xFF;
            let op = hc.mmio + cap as usize;
            // xHCI PORTSC: op + 0x400 + 0x10*(port-1)
            r32(op, 0x400 + 0x10 * (port as usize - 1))
        } else {
            // EHCI: operational = mmio + caplength; PORTSC base +0x44
            let cap = (r32(hc.mmio, 0) & 0xFF) as usize;
            let op = hc.mmio + cap;
            r32(op, 0x44 + 4 * (port as usize - 1))
        }
    }
}

fn decode_xhci_port(ps: u32) {
    let ccs = ps & 1;
    let ped = (ps >> 1) & 1;
    let oca = (ps >> 3) & 1;
    let pr = (ps >> 4) & 1;
    let pls = (ps >> 5) & 0xF;
    let pp = (ps >> 9) & 1;
    let speed = (ps >> 10) & 0xF; // Port Speed
    let csc = (ps >> 17) & 1;
    serial::write_str(" CCS=");
    serial::write_usize(ccs as usize);
    serial::write_str(" PED=");
    serial::write_usize(ped as usize);
    serial::write_str(" PR=");
    serial::write_usize(pr as usize);
    serial::write_str(" PP=");
    serial::write_usize(pp as usize);
    serial::write_str(" PLS=");
    serial::write_usize(pls as usize);
    serial::write_str(" SPEED=");
    serial::write_usize(speed as usize);
    serial::write_str(" CSC=");
    serial::write_usize(csc as usize);
    serial::write_str(" raw=");
    serial::write_hex(ps as usize);
}

fn decode_ehci_port(ps: u32) {
    let ccs = ps & 1;
    let csc = (ps >> 1) & 1;
    let ped = (ps >> 2) & 1;
    let pr = (ps >> 8) & 1;
    let ls = (ps >> 10) & 3; // line status
    let pp = (ps >> 12) & 1;
    serial::write_str(" CCS=");
    serial::write_usize(ccs as usize);
    serial::write_str(" PED=");
    serial::write_usize(ped as usize);
    serial::write_str(" PR=");
    serial::write_usize(pr as usize);
    serial::write_str(" PP=");
    serial::write_usize(pp as usize);
    serial::write_str(" LS=");
    serial::write_usize(ls as usize);
    serial::write_str(" CSC=");
    serial::write_usize(csc as usize);
    serial::write_str(" raw=");
    serial::write_hex(ps as usize);
}

fn dump_all_ports(tag: &str) {
    serial::write_str("\n--- PORT SNAPSHOT: ");
    serial::write_str(tag);
    serial::write_str(" ---\n");
    unsafe {
        let mut hi = 0usize;
        while hi < HC_N {
            let hc = &HCS[hi];
            serial::write_str("Controller ");
            serial::write_usize(hc.bus as usize);
            serial::write_str(":");
            serial::write_usize(hc.dev as usize);
            serial::write_str(".");
            serial::write_usize(hc.func as usize);
            serial::write_str(if hc.kind == 0x30 { " xHCI" } else { " EHCI" });
            serial::write_str("\n");
            let mut p = 1u8;
            while p <= hc.ports && p <= 16 {
                let ps = read_portsc(hc, p);
                serial::write_str("  PORT ");
                serial::write_usize(p as usize);
                if hc.kind == 0x30 {
                    decode_xhci_port(ps);
                } else {
                    decode_ehci_port(ps);
                }
                serial::write_str("\n");
                LAST_PORTSC[hi][p as usize] = ps;
                p += 1;
            }
            hi += 1;
        }
    }
}

/// Poll ports and log only changes (plug/unplug)
fn monitor_ports_changes(iterations: u32) {
    serial::write_str("\n======== USB PORT MONITOR (plug/unplug) ========\n");
    serial::write_str("Insert/remove USB mouse on each port. Logging CCS changes only.\n");
    let mut it = 0u32;
    while it < iterations {
        unsafe {
            let mut hi = 0usize;
            while hi < HC_N {
                let hc = &HCS[hi];
                let mut p = 1u8;
                while p <= hc.ports && p <= 16 {
                    let ps = read_portsc(hc, p);
                    let prev = LAST_PORTSC[hi][p as usize];
                    let ccs = ps & 1;
                    let prev_ccs = prev & 1;
                    if ps != prev {
                        serial::write_str("[PORT-CHANGE] ");
                        serial::write_usize(hc.bus as usize);
                        serial::write_str(":");
                        serial::write_usize(hc.dev as usize);
                        serial::write_str(".");
                        serial::write_usize(hc.func as usize);
                        serial::write_str(if hc.kind == 0x30 { " xHCI" } else { " EHCI" });
                        serial::write_str(" PORT ");
                        serial::write_usize(p as usize);
                        serial::write_str(" CCS ");
                        serial::write_usize(prev_ccs as usize);
                        serial::write_str("->");
                        serial::write_usize(ccs as usize);
                        if hc.kind == 0x30 {
                            decode_xhci_port(ps);
                        } else {
                            decode_ehci_port(ps);
                        }
                        serial::write_str("\n");
                        LAST_PORTSC[hi][p as usize] = ps;
                    }
                    p += 1;
                }
                hi += 1;
            }
        }
        delay(500_000); // ~poll cadence
        it += 1;
    }
    serial::write_str("[PORT-MONITOR] done\n");
}

/// Entry: full PCI + USB topology + port monitor window, then return (caller may start Desktop)
pub fn run_hardware_diag() {
    serial::write_str("\n******** AUTOSTART: HW DIAG SCRIPT ********\n");
    dump_all_pci();
    discover_usb_hosts();
    dump_all_ports("initial");
    // ~20s window for physical plug tests (iterations * delay)
    monitor_ports_changes(8);  // short on boot; use terminal "1" for long
    dump_all_ports("final");
    serial::write_str("\n[DIAG] First FAIL stage is wherever CCS never becomes 1 on any port.\n");
    serial::write_str("[DIAG] If CCS 0->1 on EHCI only → need EHCI path, not xHCI HID.\n");
    serial::write_str("[DIAG] If CCS 0->1 on xHCI → next is Port Reset / Address (not HID yet).\n");
    serial::write_str("******** AUTOSTART DIAG COMPLETE ********\n");
}

// Keep old name for main.rs compatibility
pub fn dump_usb_controllers() {
    run_hardware_diag();
}
