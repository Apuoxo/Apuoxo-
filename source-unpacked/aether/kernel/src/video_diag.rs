//! GRAF - video/GPU forensics (static report buffer for Terminal)

use crate::graphics;
use crate::serial;

const PCI_CFG_ADDR: u16 = 0xCF8;
const PCI_CFG_DATA: u16 = 0xCFC;

static mut REPORT: [u8; 3500] = [0; 3500];
static mut REPORT_LEN: usize = 0;

unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nostack, preserves_flags));
}
unsafe fn inl(port: u16) -> u32 {
    let v: u32;
    core::arch::asm!("in eax, dx", in("dx") port, out("eax") v, options(nostack, preserves_flags));
    v
}
unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") v, options(nostack, preserves_flags));
    v
}
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
}
unsafe fn inw(port: u16) -> u16 {
    let v: u16;
    core::arch::asm!("in ax, dx", in("dx") port, out("ax") v, options(nostack, preserves_flags));
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

fn w_str(s: &str) {
    unsafe {
        for b in s.bytes() {
            if REPORT_LEN < REPORT.len() {
                REPORT[REPORT_LEN] = b;
                REPORT_LEN += 1;
            }
        }
    }
}
fn w_hex(mut v: usize) {
    w_str("0x");
    if v == 0 {
        w_str("0");
        return;
    }
    let mut buf = [0u8; 16];
    let mut n = 0usize;
    while v > 0 && n < 16 {
        let d = (v & 0xF) as u8;
        buf[n] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        v >>= 4;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        unsafe {
            if REPORT_LEN < REPORT.len() {
                REPORT[REPORT_LEN] = buf[n];
                REPORT_LEN += 1;
            }
        }
    }
}
fn w_nl() {
    w_str("\n");
}

pub fn report() -> &'static [u8] {
    unsafe { core::slice::from_raw_parts(REPORT.as_ptr(), REPORT_LEN) }
}

pub fn run() {
    unsafe {
        REPORT_LEN = 0;
        let mut i = 0usize;
        while i < REPORT.len() {
            REPORT[i] = 0;
            i += 1;
        }
    }
    serial::write_str("[GRAF] start\n");

    w_str("=== GRAF VIDEO FORENSICS ===");
    w_nl();
    w_str("-- ACTIVE FB --");
    w_nl();
    if graphics::ready() {
        w_str("ready=YES");
        w_nl();
        w_str("addr=");
        w_hex(graphics::fb_addr());
        w_nl();
        w_str("w=");
        w_hex(graphics::width());
        w_str(" h=");
        w_hex(graphics::height());
        w_nl();
        w_str("pitch=");
        w_hex(graphics::pitch());
        w_str(" bpp=");
        w_hex(graphics::bpp() as usize);
        w_nl();
        w_str("soft=");
        w_str(if graphics::software() { "Y" } else { "N" });
        w_nl();
        let bpp = graphics::bpp() as usize;
        let pitch = graphics::pitch();
        let w = graphics::width();
        let row = if bpp >= 8 { w * (bpp / 8) } else { 0 };
        w_str("row_bytes=");
        w_hex(row);
        w_str(" pad=");
        w_hex(if pitch >= row { pitch - row } else { 0 });
        w_nl();
        w_str("fb_size~=");
        w_hex(pitch.saturating_mul(graphics::height()));
        w_nl();
        if graphics::fb_addr() != 0 && bpp == 32 {
            unsafe {
                let p0 = *(graphics::fb_addr() as *const u32);
                w_str("px00=");
                w_hex(p0 as usize);
                w_nl();
            }
        }
    } else {
        w_str("ready=NO");
        w_nl();
    }

    w_str("-- VGA LEGACY --");
    w_nl();
    unsafe {
        w_str("MISC=");
        w_hex(inb(0x3CC) as usize);
        w_nl();
        w_str("INPUT_ST=");
        w_hex(inb(0x3DA) as usize);
        w_nl();
    }

    w_str("-- PCI DISPLAY class03 --");
    w_nl();
    let mut found = 0u32;
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
                let rev = (cr & 0xFF) as u8;
                if class != 0x03 {
                    if func == 0 {
                        let ht = unsafe { pci_r32(bus, dev, func, 0x0C) };
                        if ((ht >> 16) & 0x80) == 0 {
                            break;
                        }
                    }
                    continue;
                }
                found += 1;
                let vid = (id & 0xFFFF) as u16;
                let did = ((id >> 16) & 0xFFFF) as u16;
                w_str("GPU ");
                w_hex(bus as usize);
                w_str(":");
                w_hex(dev as usize);
                w_str(".");
                w_hex(func as usize);
                w_nl();
                w_str(" VID:DID=");
                w_hex(vid as usize);
                w_str(":");
                w_hex(did as usize);
                w_nl();
                w_str(" class=");
                w_hex(class as usize);
                w_str(" sub=");
                w_hex(sub as usize);
                w_str(" if=");
                w_hex(progif as usize);
                w_str(" rev=");
                w_hex(rev as usize);
                w_nl();
                let sn = match sub {
                    0x00 => "VGA/compatible",
                    0x01 => "XGA",
                    0x02 => "3D",
                    0x80 => "Other display",
                    _ => "unknown",
                };
                w_str(" type=");
                w_str(sn);
                w_nl();
                let mut bi = 0u8;
                while bi < 6 {
                    let bar = unsafe { pci_r32(bus, dev, func, 0x10 + bi * 4) };
                    if bar != 0 && bar != 0xFFFF_FFFF {
                        w_str(" BAR");
                        w_hex(bi as usize);
                        w_str("=");
                        w_hex(bar as usize);
                        if (bar & 1) != 0 {
                            w_str(" IO");
                        } else {
                            w_str(" MEM");
                            if (bar & 6) == 4 {
                                w_str("64");
                            }
                            if (bar & 8) != 0 {
                                w_str(" PREF");
                            }
                        }
                        w_nl();
                    }
                    bi += 1;
                }
                let cmd = unsafe { pci_r32(bus, dev, func, 0x04) };
                w_str(" CMD=");
                w_hex((cmd & 0xFFFF) as usize);
                w_str(" STS=");
                w_hex(((cmd >> 16) & 0xFFFF) as usize);
                w_nl();
                let irq = unsafe { pci_r32(bus, dev, func, 0x3C) };
                w_str(" IRQ_LINE=");
                w_hex((irq & 0xFF) as usize);
                w_str(" PIN=");
                w_hex(((irq >> 8) & 0xFF) as usize);
                w_nl();
                if vid == 0x8086 {
                    w_str(" note=Intel iGPU");
                    w_nl();
                    w_str(" need=MMIO,GTT,pipes");
                    w_nl();
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
    if found == 0 {
        w_str("(no PCI display)");
        w_nl();
    }
    w_str("GPU_count=");
    w_hex(found as usize);
    w_nl();

    w_str("-- BOCHS/QEMU VBE --");
    w_nl();
    unsafe {
        outw(0x01CE, 0x0000);
        let id = inw(0x01CF);
        w_str("DISPI_ID=");
        w_hex(id as usize);
        if (0xB0C0..=0xB0C5).contains(&id) {
            w_str(" BOCHS");
        } else {
            w_str(" absent");
        }
        w_nl();
    }

    crate::drivers::intel_kms::ensure();
    w_str("-- KMS STATUS --");
    w_nl();
    w_str("step=");
    w_hex(crate::drivers::intel_kms::diag_step() as usize);
    w_nl();
    if crate::drivers::intel_kms::ready() {
        w_str("KMS=READY");
        w_nl();
    } else {
        w_str("KMS=NOT_READY");
        w_nl();
    }
    w_str("err=");
    {
        let e = crate::drivers::intel_kms::last_error();
        let mut i = 0usize;
        while i < e.len() {
            unsafe {
                if REPORT_LEN < REPORT.len() {
                    REPORT[REPORT_LEN] = e[i];
                    REPORT_LEN += 1;
                }
            }
            i += 1;
        }
    }
    w_nl();
    w_str("MMIO=");
    w_hex(crate::drivers::intel_kms::mmio_base());
    w_str(" diag=");
    w_hex(crate::drivers::intel_kms::diag_mmio() as usize);
    w_nl();
    w_str("HW=");
    w_hex(crate::drivers::intel_kms::hw_width() as usize);
    w_str("x");
    w_hex(crate::drivers::intel_kms::hw_height() as usize);
    w_nl();
    w_str("APER=");
    w_hex(crate::drivers::intel_kms::aperture());
    w_nl();
    w_str("-- KMS REGS --");
    crate::drivers::intel_kms::dump_regs();
    // compact on-screen
    w_str("see serial for full reg dump");
    w_nl();
    w_str("-- ROADMAP --");
    w_nl();
    w_str("1 LFB Multiboot (current)");
    w_nl();
    w_str("2 Map iGPU MMIO BARs");
    w_nl();
    w_str("3 Intel GTT/aperture");
    w_nl();
    w_str("4 Mode-set pipes (later)");
    w_nl();
    w_str("=== END GRAF ===");
    w_nl();
    serial::write_str("[GRAF] done\n");
}


/// Dump GRAF report into Desktop Terminal (no serial required for user).
pub fn run_to_terminal() {
    run();
    let r = report();
    let mut i = 0usize;
    while i < r.len() {
        let b = r[i];
        if b == 0 {
            break;
        }
        crate::desktop::terminal_write_char(b);
        i += 1;
    }
    if i == 0 || r.get(i.saturating_sub(1)).copied() != Some(b'\n') {
        crate::desktop::terminal_write("\n");
    }
}
