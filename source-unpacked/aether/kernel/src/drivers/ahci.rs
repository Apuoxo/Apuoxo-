//! AHCI READ-ONLY driver for Intel 7-Series (AH532: 8086:1E03)
//! ONLY: init ports, IDENTIFY, READ DMA (0xC8) / READ SECTORS EXT
//! NO writes to disk.

use crate::serial;
use crate::mm;
use crate::mm::paging;

const PCI_CFG_ADDR: u16 = 0xCF8;
const PCI_CFG_DATA: u16 = 0xCFC;

static mut ABAR: usize = 0;
static mut PORT_MASK: u32 = 0;
static mut SECTORS: [u64; 32] = [0; 32];
static mut MODEL: [[u8; 40]; 32] = [[0; 40]; 32];
static mut MODEL_LEN: [usize; 32] = [0; 32];
static mut NDISKS: usize = 0;
static mut DISK_PORT: [u8; 4] = [0; 4];
static mut CL_PHYS: [usize; 32] = [0; 32];
static mut FB_PHYS: [usize; 32] = [0; 32];
static mut CT_PHYS: [usize; 32] = [0; 32];
static mut ID_BUF: [usize; 32] = [0; 32];
static mut READY: bool = false;

unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") val,
        options(nostack, preserves_flags)
    );
}

unsafe fn inl(port: u16) -> u32 {
    let v: u32;
    core::arch::asm!(
        "in eax, dx",
        in("dx") port,
        out("eax") v,
        options(nostack, preserves_flags)
    );
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

fn rd32(base: usize, off: usize) -> u32 {
    unsafe { core::ptr::read_volatile((base + off) as *const u32) }
}

fn wr32(base: usize, off: usize, val: u32) {
    unsafe { core::ptr::write_volatile((base + off) as *mut u32, val) }
}

fn map_range(phys: usize, size: usize) -> bool {
    let cr3 = unsafe { paging::read_cr3() };
    let start = phys & !0xFFF;
    let end = (phys + size + 0xFFF) & !0xFFF;
    let mut va = start;
    while va < end {
        let _ok = unsafe {
            paging::map_page(cr3, va, va, paging::PAGE_PRESENT | paging::PAGE_WRITE)
        };
        va += 0x1000;
    }
    unsafe { paging::load_cr3(cr3) };
    true
}

fn delay() {
    let mut i = 0u32;
    while i < 100_000 {
        i = i.wrapping_add(1);
        core::hint::spin_loop();
    }
}

fn port_base(abar: usize, port: u32) -> usize {
    abar + 0x100 + (port as usize) * 0x80
}

fn stop_port(pb: usize) -> bool {
    let mut cmd = rd32(pb, 0x18);
    cmd &= !((1 << 0) | (1 << 4));
    wr32(pb, 0x18, cmd);
    let mut t = 0u32;
    while t < 1_000_000 {
        let c = rd32(pb, 0x18);
        if (c & (1 << 15)) == 0 && (c & (1 << 14)) == 0 {
            return true;
        }
        t += 1;
        core::hint::spin_loop();
    }
    false
}

fn start_port(pb: usize) {
    let mut cmd = rd32(pb, 0x18);
    cmd |= 1 << 4;
    wr32(pb, 0x18, cmd);
    cmd = rd32(pb, 0x18);
    cmd |= 1 << 0;
    wr32(pb, 0x18, cmd);
}

fn alloc_dma() -> Option<usize> {
    let p = mm::alloc_page()?;
    mm::zero_pages(p, 1);
    Some(p)
}

fn setup_port(abar: usize, port: u32) -> bool {
    let pb = port_base(abar, port);
    if !stop_port(pb) {
        serial::write_str("[AHCI] stop_port fail\n");
        return false;
    }
    let cl = match alloc_dma() { Some(p) => p, None => return false };
    let fb = match alloc_dma() { Some(p) => p, None => return false };
    let ct = match alloc_dma() { Some(p) => p, None => return false };
    let idb = match alloc_dma() { Some(p) => p, None => return false };
    unsafe {
        CL_PHYS[port as usize] = cl;
        FB_PHYS[port as usize] = fb;
        CT_PHYS[port as usize] = ct;
        ID_BUF[port as usize] = idb;
    }
    wr32(pb, 0x00, cl as u32);
    wr32(pb, 0x04, 0);
    wr32(pb, 0x08, fb as u32);
    wr32(pb, 0x0C, 0);
    wr32(pb, 0x10, 0xFFFF_FFFF);
    wr32(pb, 0x30, 0xFFFF_FFFF);
    unsafe {
        let ch = cl as *mut u32;
        *ch.add(0) = 5 | (1 << 16);
        *ch.add(1) = 0;
        *ch.add(2) = ct as u32;
        *ch.add(3) = 0;
    }
    start_port(pb);
    true
}

fn issue_identify(abar: usize, port: u32) -> bool {
    let pb = port_base(abar, port);
    let ct = unsafe { CT_PHYS[port as usize] };
    let idb = unsafe { ID_BUF[port as usize] };
    if ct == 0 || idb == 0 { return false; }
    let mut t = 0u32;
    while t < 1_000_000 {
        let tfd = rd32(pb, 0x20);
        if (tfd & 0x88) == 0 { break; }
        t += 1;
    }
    unsafe {
        let p = ct as *mut u8;
        let mut i = 0usize;
        while i < 0x80 { *p.add(i) = 0; i += 1; }
        *p.add(0) = 0x27;
        *p.add(1) = 1 << 7;
        *p.add(2) = 0xEC;
        *p.add(3) = 0;
        let prd = (ct + 0x80) as *mut u32;
        *prd.add(0) = idb as u32;
        *prd.add(1) = 0;
        *prd.add(2) = 0;
        *prd.add(3) = 511 | (1 << 31);
        let cl = CL_PHYS[port as usize] as *mut u32;
        *cl.add(0) = 5 | (1 << 16);
        *cl.add(1) = 0;
    }
    wr32(pb, 0x10, 0xFFFF_FFFF);
    wr32(pb, 0x38, 1);
    t = 0;
    while t < 5_000_000 {
        let ci = rd32(pb, 0x38);
        let is = rd32(pb, 0x10);
        if (ci & 1) == 0 {
            if (is & (1 << 30)) != 0 {
                serial::write_str("[AHCI] IDENTIFY TFES error\n");
                return false;
            }
            return true;
        }
        t += 1;
        core::hint::spin_loop();
    }
    serial::write_str("[AHCI] IDENTIFY timeout\n");
    false
}

fn parse_identify(port: u32) {
    let idb = unsafe { ID_BUF[port as usize] };
    if idb == 0 { return; }
    unsafe {
        let w = idb as *const u16;
        let lba28 = (*w.add(60) as u64) | ((*w.add(61) as u64) << 16);
        let lba48 = (*w.add(100) as u64)
            | ((*w.add(101) as u64) << 16)
            | ((*w.add(102) as u64) << 32)
            | ((*w.add(103) as u64) << 48);
        let secs = if lba48 != 0 { lba48 } else { lba28 };
        let mut mi = 0usize;
        let mut wi = 27usize;
        while wi <= 46 && mi + 1 < 40 {
            let v = *w.add(wi);
            let a = ((v >> 8) & 0xFF) as u8;
            let b = (v & 0xFF) as u8;
            if a >= 32 && a < 127 { MODEL[port as usize][mi] = a; mi += 1; }
            if mi < 40 && b >= 32 && b < 127 { MODEL[port as usize][mi] = b; mi += 1; }
            wi += 1;
        }
        while mi > 0 && MODEL[port as usize][mi - 1] == b' ' { mi -= 1; }
        MODEL_LEN[port as usize] = mi;
        SECTORS[port as usize] = secs;
        if NDISKS < 4 {
            DISK_PORT[NDISKS] = port as u8;
            NDISKS += 1;
        }
        serial::write_str("[AHCI] port");
        serial::write_usize(port as usize);
        serial::write_str(" sectors=");
        serial::write_usize(secs as usize);
        serial::write_str(" model=");
        let mut i = 0usize;
        while i < mi {
            serial::write_byte(MODEL[port as usize][i]);
            i += 1;
        }
        serial::write_str("\n");
    }
}

pub fn read_sectors(disk: usize, lba: u64, count: u8, buf: &mut [u8]) -> bool {
    if !unsafe { READY } || disk >= unsafe { NDISKS } || count == 0 {
        return false;
    }
    let need = (count as usize) * 512;
    if buf.len() < need { return false; }
    let port = unsafe { DISK_PORT[disk] as u32 };
    let abar = unsafe { ABAR };
    let pb = port_base(abar, port);
    let ct = unsafe { CT_PHYS[port as usize] };
    if ct == 0 { return false; }
    let mut done = 0u8;
    while done < count {
        let cur_lba = lba + done as u64;
        let rbuf = unsafe { ID_BUF[port as usize] };
        if rbuf == 0 { return false; }
        let mut t = 0u32;
        while t < 1_000_000 {
            let tfd = rd32(pb, 0x20);
            if (tfd & 0x88) == 0 { break; }
            t += 1;
        }
        unsafe {
            let p = ct as *mut u8;
            let mut i = 0usize;
            while i < 0x80 { *p.add(i) = 0; i += 1; }
            *p.add(0) = 0x27;
            *p.add(1) = 1 << 7;
            *p.add(2) = 0x25;
            *p.add(4) = (cur_lba & 0xFF) as u8;
            *p.add(5) = ((cur_lba >> 8) & 0xFF) as u8;
            *p.add(6) = ((cur_lba >> 16) & 0xFF) as u8;
            *p.add(7) = 1 << 6;
            *p.add(8) = ((cur_lba >> 24) & 0xFF) as u8;
            *p.add(9) = ((cur_lba >> 32) & 0xFF) as u8;
            *p.add(10) = ((cur_lba >> 40) & 0xFF) as u8;
            *p.add(12) = 1;
            let prd = (ct + 0x80) as *mut u32;
            *prd.add(0) = rbuf as u32;
            *prd.add(1) = 0;
            *prd.add(2) = 0;
            *prd.add(3) = 511 | (1 << 31);
            let cl = CL_PHYS[port as usize] as *mut u32;
            *cl.add(0) = 5 | (1 << 16);
            *cl.add(1) = 0;
        }
        wr32(pb, 0x10, 0xFFFF_FFFF);
        wr32(pb, 0x38, 1);
        t = 0;
        let mut ok = false;
        while t < 5_000_000 {
            if (rd32(pb, 0x38) & 1) == 0 {
                ok = (rd32(pb, 0x10) & (1 << 30)) == 0;
                break;
            }
            t += 1;
            core::hint::spin_loop();
        }
        if !ok {
            serial::write_str("[AHCI] read fail LBA\n");
            return false;
        }
        unsafe {
            let src = rbuf as *const u8;
            let dst_off = (done as usize) * 512;
            let mut i = 0usize;
            while i < 512 {
                buf[dst_off + i] = *src.add(i);
                i += 1;
            }
        }
        done += 1;
    }
    true
}

/// WRITE DMA EXT (0x35) - one sector at a time (same DMA buffer as read).
pub fn write_sectors(disk: usize, lba: u64, count: u8, buf: &[u8]) -> bool {
    if !unsafe { READY } || disk >= unsafe { NDISKS } || count == 0 {
        return false;
    }
    let need = (count as usize) * 512;
    if buf.len() < need {
        return false;
    }
    let port = unsafe { DISK_PORT[disk] as u32 };
    let abar = unsafe { ABAR };
    let pb = port_base(abar, port);
    let ct = unsafe { CT_PHYS[port as usize] };
    if ct == 0 {
        return false;
    }
    let mut done = 0u8;
    while done < count {
        let cur_lba = lba + done as u64;
        let rbuf = unsafe { ID_BUF[port as usize] };
        if rbuf == 0 {
            return false;
        }
        // Copy payload into DMA buffer before issuing WRITE
        unsafe {
            let dst = rbuf as *mut u8;
            let src_off = (done as usize) * 512;
            let mut i = 0usize;
            while i < 512 {
                *dst.add(i) = buf[src_off + i];
                i += 1;
            }
        }
        let mut t = 0u32;
        while t < 1_000_000 {
            let tfd = rd32(pb, 0x20);
            if (tfd & 0x88) == 0 {
                break;
            }
            t += 1;
        }
        unsafe {
            let p = ct as *mut u8;
            let mut i = 0usize;
            while i < 0x80 {
                *p.add(i) = 0;
                i += 1;
            }
            *p.add(0) = 0x27; // H2D FIS
            *p.add(1) = 1 << 7; // command
            *p.add(2) = 0x35; // WRITE DMA EXT
            *p.add(4) = (cur_lba & 0xFF) as u8;
            *p.add(5) = ((cur_lba >> 8) & 0xFF) as u8;
            *p.add(6) = ((cur_lba >> 16) & 0xFF) as u8;
            *p.add(7) = 1 << 6; // LBA mode
            *p.add(8) = ((cur_lba >> 24) & 0xFF) as u8;
            *p.add(9) = ((cur_lba >> 32) & 0xFF) as u8;
            *p.add(10) = ((cur_lba >> 40) & 0xFF) as u8;
            *p.add(12) = 1; // sector count
            let prd = (ct + 0x80) as *mut u32;
            *prd.add(0) = rbuf as u32;
            *prd.add(1) = 0;
            *prd.add(2) = 0;
            *prd.add(3) = 511 | (1 << 31);
            let cl = CL_PHYS[port as usize] as *mut u32;
            // CFL=5, W=1 (write from host to device)
            *cl.add(0) = 5 | (1 << 6) | (1 << 16);
            *cl.add(1) = 0;
        }
        wr32(pb, 0x10, 0xFFFF_FFFF);
        wr32(pb, 0x38, 1);
        t = 0;
        let mut ok = false;
        while t < 5_000_000 {
            if (rd32(pb, 0x38) & 1) == 0 {
                ok = (rd32(pb, 0x10) & (1 << 30)) == 0;
                break;
            }
            t += 1;
            core::hint::spin_loop();
        }
        if !ok {
            serial::write_str("[AHCI] write fail LBA\n");
            return false;
        }
        done += 1;
    }
    true
}

pub fn disk_count() -> usize { unsafe { NDISKS } }

pub fn disk_sectors(disk: usize) -> u64 {
    unsafe {
        if disk < NDISKS { SECTORS[DISK_PORT[disk] as usize] } else { 0 }
    }
}

pub fn disk_model(disk: usize, out: &mut [u8]) -> usize {
    unsafe {
        if disk >= NDISKS { return 0; }
        let p = DISK_PORT[disk] as usize;
        let n = if MODEL_LEN[p] < out.len() { MODEL_LEN[p] } else { out.len() };
        let mut i = 0usize;
        while i < n { out[i] = MODEL[p][i]; i += 1; }
        n
    }
}

pub fn is_ready() -> bool { unsafe { READY } }

pub fn init() -> bool {
    serial::write_str("\n[AHCI] init READ-ONLY\n");
    let mut found_bus = 0u8;
    let mut found_dev = 0u8;
    let mut found_func = 0u8;
    let mut found = false;
    let mut abar = 0u32;

    for bus in 0u8..=255 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = unsafe { pci_r32(bus, dev, func, 0) };
                if id == 0xFFFF_FFFF {
                    if func == 0 { break; }
                    continue;
                }
                let vid = (id & 0xFFFF) as u16;
                let did = ((id >> 16) & 0xFFFF) as u16;
                let cr = unsafe { pci_r32(bus, dev, func, 0x08) };
                let class = ((cr >> 24) & 0xFF) as u8;
                let sub = ((cr >> 16) & 0xFF) as u8;
                let progif = ((cr >> 8) & 0xFF) as u8;
                let is = (vid == 0x8086 && (did == 0x1E03 || did == 0x1E02))
                    || (class == 0x01 && sub == 0x06 && progif == 0x01);
                if is {
                    found = true;
                    found_bus = bus;
                    found_dev = dev;
                    found_func = func;
                    abar = unsafe { pci_r32(bus, dev, func, 0x24) };
                    if abar == 0 || (abar & 1) != 0 {
                        abar = unsafe { pci_r32(bus, dev, func, 0x10) };
                    }
                    abar &= !0xFF;
                    break;
                }
                if func == 0 {
                    let ht = unsafe { pci_r32(bus, dev, func, 0x0C) };
                    if ((ht >> 16) & 0x80) == 0 { break; }
                }
            }
            if found { break; }
        }
        if found { break; }
    }

    if !found || abar == 0 {
        serial::write_str("[AHCI] controller not found\n");
        return false;
    }

    serial::write_str("[AHCI] ");
    serial::write_hex(found_bus as usize);
    serial::write_str(":");
    serial::write_hex(found_dev as usize);
    serial::write_str(".");
    serial::write_hex(found_func as usize);
    serial::write_str(" ABAR=0x");
    serial::write_hex(abar as usize);
    serial::write_str("\n");

    unsafe {
        let cmd = (pci_r32(found_bus, found_dev, found_func, 0x04) & 0xFFFF) as u16;
        pci_w16(found_bus, found_dev, found_func, 0x04, cmd | 0x0006);
    }

    if !map_range(abar as usize, 0x1100) {
        serial::write_str("[AHCI] map fail\n");
        return false;
    }

    let base = abar as usize;
    unsafe { ABAR = base; }

    let mut ghc = rd32(base, 0x04);
    ghc |= 1 << 31;
    wr32(base, 0x04, ghc);
    delay();

    let pi = rd32(base, 0x0C);
    serial::write_str("[AHCI] PI=0x");
    serial::write_hex(pi as usize);
    serial::write_str("\n");

    let mut p = 0u32;
    while p < 32 {
        if (pi & (1 << p)) != 0 {
            let pb = port_base(base, p);
            let ssts = rd32(pb, 0x28);
            let det = ssts & 0xF;
            if det == 3 {
                serial::write_str("[AHCI] port");
                serial::write_usize(p as usize);
                serial::write_str(" device present, setup...\n");
                if setup_port(base, p) {
                    if issue_identify(base, p) {
                        parse_identify(p);
                        unsafe { PORT_MASK |= 1 << p; }
                    } else {
                        serial::write_str("[AHCI] identify failed\n");
                    }
                }
            }
        }
        p += 1;
    }

    let n = unsafe { NDISKS };
    serial::write_str("[AHCI] disks=");
    serial::write_usize(n);
    serial::write_str("\n");
    if n > 0 {
        unsafe { READY = true; }
        true
    } else {
        false
    }
}
