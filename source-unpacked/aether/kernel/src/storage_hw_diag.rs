//! Storage HARDWARE diagnostics - READ ONLY
//! Results stored for Terminal on-screen display (no serial cable required).

use crate::serial;

const PCI_CFG_ADDR: u16 = 0xCF8;
const PCI_CFG_DATA: u16 = 0xCFC;

#[derive(Clone, Copy)]
pub struct MassDev {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vid: u16,
    pub did: u16,
    pub sub: u8,
    pub progif: u8,
    pub abar: u32,
    pub is_ahci: bool,
    pub ports_impl: u32,
    pub ports_present: u32, // bit per port DET==3
    pub cap: u32,
    pub ghc: u32,
    pub pi: u32,
}

#[derive(Clone, Copy)]
pub struct DiagSnapshot {
    pub legacy_status: u8,
    pub mass_n: usize,
    pub mass: [MassDev; 8],
    pub ahci_n: usize,
    pub ide_n: usize,
    pub nvme_n: usize,
}

static mut SNAP: DiagSnapshot = DiagSnapshot {
    legacy_status: 0xFF,
    mass_n: 0,
    mass: [MassDev {
        bus: 0, dev: 0, func: 0, vid: 0, did: 0, sub: 0, progif: 0,
        abar: 0, is_ahci: false, ports_impl: 0, ports_present: 0,
        cap: 0, ghc: 0, pi: 0,
    }; 8],
    ahci_n: 0,
    ide_n: 0,
    nvme_n: 0,
};

pub fn snapshot() -> DiagSnapshot {
    unsafe { SNAP }
}

unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nostack, preserves_flags));
}
unsafe fn inl(port: u16) -> u32 {
    let v: u32;
    core::arch::asm!("in eax, dx", in("dx") port, out("eax") v, options(nostack, preserves_flags));
    v
}
unsafe fn pci_r32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    outl(PCI_CFG_ADDR, addr);
    inl(PCI_CFG_DATA)
}
unsafe fn pci_w16(bus: u8, dev: u8, func: u8, off: u8, val: u16) {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    outl(PCI_CFG_ADDR, addr);
    let mut v = inl(PCI_CFG_DATA);
    let shift = ((off & 2) * 8) as u32;
    v = (v & !(0xFFFFu32 << shift)) | ((val as u32) << shift);
    outl(PCI_CFG_DATA, v);
}

fn map_abar(phys: u64) -> bool {
    let cr3 = unsafe { crate::mm::paging::read_cr3() };
    let start = (phys as usize) & !0xFFF;
    let mut va = start;
    let end = start + 0x2000;
    while va < end {
        let _ = unsafe {
            crate::mm::paging::map_page(
                cr3,
                va,
                va,
                crate::mm::paging::PAGE_PRESENT | crate::mm::paging::PAGE_WRITE,
            )
        };
        va += 0x1000;
    }
    unsafe { crate::mm::paging::load_cr3(cr3) };
    true
}

fn rd32(mmio: usize, off: usize) -> u32 {
    unsafe { core::ptr::read_volatile((mmio + off) as *const u32) }
}

pub fn run() {
    serial::write_str("\n======== STORAGE HARDWARE DIAGNOSTICS ========\n");
    unsafe {
        SNAP.mass_n = 0;
        SNAP.ahci_n = 0;
        SNAP.ide_n = 0;
        SNAP.nvme_n = 0;
    }

    // Legacy STATUS
    let st = unsafe {
        let v: u8;
        core::arch::asm!("in al, dx", in("dx") 0x1F7u16, out("al") v, options(nostack, preserves_flags));
        v
    };
    unsafe { SNAP.legacy_status = st; }
    serial::write_str("Legacy 0x1F7=");
    serial::write_hex(st as usize);
    serial::write_str("\n");

    for bus in 0u8..=255 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = unsafe { pci_r32(bus, dev, func, 0) };
                if id == 0xFFFF_FFFF || (id & 0xFFFF) == 0xFFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let cr = unsafe { pci_r32(bus, dev, func, 0x08) };
                let class = ((cr >> 24) & 0xFF) as u8;
                let sub = ((cr >> 16) & 0xFF) as u8;
                let progif = ((cr >> 8) & 0xFF) as u8;
                if class != 0x01 {
                    if func == 0 {
                        let ht = unsafe { pci_r32(bus, dev, func, 0x0C) };
                        if ((ht >> 16) & 0x80) == 0 {
                            break;
                        }
                    }
                    continue;
                }
                let vid = (id & 0xFFFF) as u16;
                let did = ((id >> 16) & 0xFFFF) as u16;
                let mut md = MassDev {
                    bus, dev, func, vid, did, sub, progif,
                    abar: 0, is_ahci: false, ports_impl: 0, ports_present: 0,
                    cap: 0, ghc: 0, pi: 0,
                };

                let is_ahci = sub == 0x06 && (progif == 0x01 || progif == 0x00);
                // Also treat Intel 7-series known DIDs as AHCI even if prog IF odd
                let intel_ahci = vid == 0x8086 && matches!(did, 0x1E02 | 0x1E03 | 0x1C02 | 0x1C03 | 0x2922 | 0x2929);
                if is_ahci || intel_ahci {
                    md.is_ahci = true;
                    unsafe { SNAP.ahci_n += 1; }
                    // ABAR = BAR5 typically
                    let mut abar = unsafe { pci_r32(bus, dev, func, 0x24) };
                    if abar == 0 || (abar & 1) != 0 {
                        abar = unsafe { pci_r32(bus, dev, func, 0x10) };
                    }
                    abar &= !0xFFu32;
                    md.abar = abar;
                    if abar != 0 {
                        unsafe {
                            let cmd = (pci_r32(bus, dev, func, 0x04) & 0xFFFF) as u16;
                            pci_w16(bus, dev, func, 0x04, cmd | 0x0006);
                        }
                        if map_abar(abar as u64) {
                            let base = abar as usize;
                            md.cap = rd32(base, 0x00);
                            md.ghc = rd32(base, 0x04);
                            md.pi = rd32(base, 0x0C);
                            md.ports_impl = md.pi;
                            let mut p = 0u32;
                            while p < 32 {
                                if (md.pi & (1 << p)) != 0 {
                                    let poff = 0x100 + (p as usize) * 0x80;
                                    let ssts = rd32(base, poff + 0x28);
                                    let det = ssts & 0xF;
                                    if det == 3 {
                                        md.ports_present |= 1 << p;
                                    }
                                    serial::write_str("  AHCI Port");
                                    serial::write_usize(p as usize);
                                    serial::write_str(" SSTS=");
                                    serial::write_hex(ssts as usize);
                                    if det == 3 {
                                        serial::write_str(" DEVICE\n");
                                    } else {
                                        serial::write_str("\n");
                                    }
                                }
                                p += 1;
                            }
                        }
                    }
                } else if sub == 0x01 || progif == 0x8A || progif == 0x80 {
                    unsafe { SNAP.ide_n += 1; }
                } else if sub == 0x08 {
                    unsafe { SNAP.nvme_n += 1; }
                }

                serial::write_str("MASS ");
                serial::write_hex(vid as usize);
                serial::write_str(":");
                serial::write_hex(did as usize);
                serial::write_str(" AHCI=");
                serial::write_str(if md.is_ahci { "Y" } else { "N" });
                serial::write_str("\n");

                unsafe {
                    if SNAP.mass_n < 8 {
                        SNAP.mass[SNAP.mass_n] = md;
                        SNAP.mass_n += 1;
                    }
                }

                if func == 0 {
                    let ht = unsafe { pci_r32(bus, dev, func, 0x0C) };
                    if ((ht >> 16) & 0x80) == 0 {
                        break;
                    }
                }
            }
        }
    }
    serial::write_str("======== END STORAGE HW DIAG ========\n");
}
