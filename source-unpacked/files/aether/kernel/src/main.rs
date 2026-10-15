#![no_std]
#![no_main]

mod serial;
mod smp;
mod cpu;
mod mm;
mod capability;
mod personality;
mod process;
mod sched;
mod graphics;
mod wallpaper_data;
mod gui;
mod ui;
mod compositor;
mod input;
mod fs;
mod block;
mod part;
mod fs_fat;
mod fs_ntfs;
mod storage;
mod storage_hw_diag;
mod video_diag;
mod fb;
mod desktop;
mod files_mgr;
mod ring3_resume;
mod time;
mod mbi_mod;
mod log;
mod shell;
mod media_builtin;
mod media_player;
mod video_player;
mod video_embed;
mod video_samples;
mod jpeg_dec;
mod alarm;
mod mp3_decoder;
mod userspace;
mod elf;
mod pe;
mod elf_blobs;
mod drivers { pub mod ps2; pub mod xhci; pub mod ehci; pub mod uhci; pub mod ata; pub mod ahci; pub mod pci_usb_diag; pub mod video;
    pub mod intel_igpu;
    pub mod intel_kms; pub mod intel_gmbus; pub mod intel_gtt; pub mod audio; pub mod net; pub mod wifi; pub mod usb_phone; }
mod personalities {
    pub mod linux;
    pub mod windows;
    pub mod android;
}
mod arch {
    pub mod x86_64 {
        pub mod gdt;
        pub mod idt;
        pub mod handlers;
    }
}

use core::panic::PanicInfo;
use crate::mm::paging;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    serial::write_str("PANIC\n");
    loop { unsafe { core::arch::asm!("hlt"); } }
}

#[no_mangle]
pub unsafe extern "C" fn memmove(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
    if (d as usize) < (s as usize) {
        let mut i = 0;
        while i < n { *d.add(i) = *s.add(i); i += 1; }
    } else {
        let mut i = n;
        while i > 0 { i -= 1; *d.add(i) = *s.add(i); }
    }
    d
}
#[no_mangle]
pub unsafe extern "C" fn memcpy(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
    let mut i=0; while i<n { *d.add(i)=*s.add(i); i+=1; } d
}
#[no_mangle]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut i=0; while i<n { *s.add(i)=c as u8; i+=1; } s
}
#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i=0; while i<n { let x=*a.add(i); let y=*b.add(i); if x!=y { return x as i32-y as i32; } i+=1; } 0
}

extern "C" { fn enter_user_mode(entry: u64, stack: u64) -> !; }


/// VGA text marker at column `col` (0..80), white on black - visible on real BIOS
fn vga_mark(col: usize, ch: u8) {
    unsafe {
        let p = (0xB8000 + col * 2) as *mut u16;
        *p = 0x0F00u16 | (ch as u16);
    }
}

#[no_mangle]
pub extern "C" fn kernel_main(mbi: usize) -> ! {
    // Stage markers on VGA (after boot.s wrote OK at cols 0-1)
    vga_mark(2, b'K'); // entered kernel_main
    serial::init();
    vga_mark(3, b'S'); // serial init done (or skipped)
    serial::write_str("\nAETHER v1.8-os - kernel / CPU / RAM / video\n\n");
    cpu::init();
    serial::write_str("[CPU] ");
    let br = cpu::brand();
    let mut bi = 0usize;
    while bi < br.len() {
        serial::write_byte(br[bi]);
        bi += 1;
    }
    serial::write_str("\n");
    // PMM must start AFTER Multiboot modules (WiFi ucode). Module at ~0x1A2000..0x24EB40
    // was overwritten by PMM alloc from 2MiB - CLASS B / INIT=0 root cause.
    let mut pmm_start = 2 * 1024 * 1024;
    if let Some((_s, end, _l)) = mbi_mod::find_module_range(mbi, b"") {
        let after = (end + 0xFFF) & !0xFFF;
        if after > pmm_start {
            pmm_start = after;
        }
        serial::write_str("[PMM] raise start past module end=");
        serial::write_hex(end);
        serial::write_str(" -> ");
        serial::write_hex(pmm_start);
        serial::write_str("
");
    } else if let Some((_s, end, _l)) = mbi_mod::find_module_range(mbi, b"iwl") {
        let after = (end + 0xFFF) & !0xFFF;
        if after > pmm_start {
            pmm_start = after;
        }
    }
    // also try any module
    if let Some((_s, end, _l)) = mbi_mod::find_module_range(mbi, b"ucode") {
        let after = (end + 0xFFF) & !0xFFF;
        if after > pmm_start {
            pmm_start = after;
        }
    }
    if let Some((_s, end, _l)) = mbi_mod::find_module_range(mbi, b"2030") {
        let after = (end + 0xFFF) & !0xFFF;
        if after > pmm_start {
            pmm_start = after;
        }
    }
    // Multiboot2 memory map → AVAILABLE RAM within 8 GiB identity map
    let pages = mm::init_from_mbi(mbi, pmm_start);
    serial::write_str("[PMM] free_pages=");
    serial::write_usize(pages);
    serial::write_str(" total=");
    serial::write_usize(mm::total_count());
    serial::write_str(" (~");
    serial::write_usize(mm::total_mb());
    serial::write_str(" MiB managed, installed ~");
    serial::write_usize(mm::installed_mb());
    serial::write_str(" MiB, identity 8 GiB)\n");
    vga_mark(4, b'P'); // PMM
    serial::write_str("[OK] PMM\n");
    // Expand identity map to 8 GiB so all managed RAM is addressable
    unsafe {
        let cr3 = paging::read_cr3();
        if paging::extend_identity_gib(cr3, 8) {
            serial::write_str("[OK] identity map 8 GiB\n");
        } else {
            serial::write_str("[WARN] identity extend failed\n");
        }
    }

    // Kernel stack must be large: rust_kernel_after_user has big locals; Ring3 TSS uses rsp0
    let mut kstack_base = 0usize;
    let mut ki = 0usize;
    while ki < 16 {
        if let Some(p) = mm::alloc_page() {
            if kstack_base == 0 { kstack_base = p; }
            // prefer contiguous
            ki += 1;
        } else { break; }
    }
    let kstack_top = if kstack_base != 0 { kstack_base + ki * 4096 } else { 0x200000 + 0x10000 };
    vga_mark(5, b'G');
    arch::x86_64::gdt::init(kstack_top as u64);
    arch::x86_64::idt::init();
    vga_mark(6, b'I'); // IDT
    serial::write_str("[OK] GDT+IDT\n");
    unsafe { paging::set_kernel_cr3(paging::read_cr3()); }
    serial::write_str("[OK] kernel CR3 saved=");
    serial::write_hex(unsafe { paging::kernel_cr3() });
    serial::write_str("\n");
    // SMP stage 1: detect cores, enable LAPIC, start APs (idle)
    smp::init();
    serial::write_str("[SMP] online=");
    serial::write_usize(smp::online_count());
    serial::write_str("/");
    serial::write_usize(smp::cpu_count());
    serial::write_str("\n");
    // Mask PIC early - avoid spurious IRQs on real HW
    unsafe {
        core::arch::asm!(
            "mov al, 0xFF; out 0x21, al; out 0xA1, al",
            options(nostack, preserves_flags)
        );
    }
    vga_mark(7, b'M'); // PIC masked

    // USER maps kept for later Ring3 - but demo deferred (HW-safe path)
    let code_phys = mm::alloc_page().unwrap_or(0x300000);
    let stack_phys = mm::alloc_page().unwrap_or(0x301000);
    unsafe {
        let c = code_phys as *mut u8;
        *c.add(0) = 0xB8; *c.add(1) = 1; *c.add(2) = 0; *c.add(3) = 0; *c.add(4) = 0;
        *c.add(5) = 0xCD; *c.add(6) = 0x80;
        *c.add(7) = 0xB8; *c.add(8) = 60; *c.add(9) = 0; *c.add(10) = 0; *c.add(11) = 0;
        *c.add(12) = 0xCD; *c.add(13) = 0x80;
        *c.add(14) = 0xF4;
        let cr3 = paging::read_cr3();
        let _ = paging::map_page(cr3, 0x40000000, code_phys,
            paging::PAGE_PRESENT | paging::PAGE_WRITE | paging::PAGE_USER);
        let _ = paging::map_page(cr3, 0x40001000, stack_phys,
            paging::PAGE_PRESENT | paging::PAGE_WRITE | paging::PAGE_USER);
        paging::load_cr3(cr3);
    }
    vga_mark(8, b'U');
    serial::write_str("[OK] USER\n");

    drivers::ps2::init();
    vga_mark(9, b'Y');
    unsafe {
        core::arch::asm!("mov al, 0xFF; out 0x21, al; out 0xA1, al", options(nostack, preserves_flags));
    }
    // Skip Ring3 demo on boot stack path - avoids return on 4K TSS stack
    vga_mark(10, b'r'); // ring3 demo skipped
    vga_mark(11, b'3'); // placeholder compatibility
    serial::write_str("[HW] skip Ring3 demo - stay on boot stack\n");

    vga_mark(12, b'F');
    vga_mark(13, b'c'); // GUI skip
    serial::write_str("[FB] text mode\n");

    vga_mark(14, b'A');
    let stor_ok = fs::init_storage();
    storage_hw_diag::run();
    let _ahci = drivers::ahci::init();
    storage::init();
    vga_mark(15, b'D');
    unsafe {
        let msg = if stor_ok {
            b"RAMDISK OK | AetherFS mounted"
        } else {
            b"STORAGE FAIL                 "
        };
        let mut i = 0usize;
        while i < msg.len() && i < 80 {
            let p = (0xB8000 + 160 + i * 2) as *mut u16;
            *p = 0x0A00u16 | (msg[i] as u16);
            i += 1;
        }
    }

    // ---- Permanent userspace: /bin/init then /bin/sh (via exit path) ----
    ring3_resume::arm(mbi);
    serial::write_str("\n======== USERSPACE INIT STAGE ========\n");
    serial::write_str("[INIT] install ELF /bin/init /bin/sh to AetherFS\n");
    let _ = fs::write_large("/bin/init", elf_blobs::INIT_ELF);
    let _ = fs::write_large("/bin/sh", elf_blobs::SH_ELF);
    let mut elfbuf = [0u8; 8192];
    let n = match fs::read_large("/bin/init", &mut elfbuf) {
        Some(n) => n,
        None => {
            serial::write_str("[INIT] FATAL read /bin/init\n");
            0
        }
    };
    if n >= 4 && elfbuf[0] == 0x7f && elfbuf[1] == b'E' {
        serial::write_str("[INIT] ELF from AetherFS bytes=");
        serial::write_usize(n);
        serial::write_str("\n");
        if let Some(img) = elf::load(&elfbuf) {
            if let Some(pid) = process::create_from_image("init", &img) {
                process::set_state(pid, process::State::Running);
                process::set_current(pid);
                serial::write_str("[INIT] PID=");
                serial::write_usize(pid);
                serial::write_str("\n");
                serial::write_str("[INIT] USER_CR3=");
                serial::write_hex(img.cr3);
                serial::write_str(" KERNEL_CR3=");
                serial::write_hex(unsafe { paging::kernel_cr3() });
                serial::write_str("\n");
                if img.cr3 != unsafe { paging::kernel_cr3() } {
                    serial::write_str("[INIT] USER_CR3 != KERNEL_CR3\n");
                }
                serial::write_str("[INIT] enter CPL=3 via process path\n");
                extern "C" { fn enter_user_mode(entry: u64, stack: u64) -> !; }
                unsafe {
                    paging::load_cr3(img.cr3);
                    enter_user_mode(img.entry as u64, img.stack_top as u64);
                }
            }
        } else {
            serial::write_str("[INIT] elf::load FAIL\n");
        }
    } else {
        serial::write_str("[INIT] no ELF - continue\n");
    }

    // If enter_user returned (should not) or load failed:
    vga_mark(16, b'G');
    fb::try_init(mbi);
    // KMS always after FB probe (even if desktop path differs)
    serial::write_str("[BOOT] KMS after FB\n");
    drivers::intel_igpu::init();
    drivers::intel_kms::init();
    // Decode GMCH stolen memory size (real pre-allocated VRAM) - PCI only, safe.
    let _ = drivers::intel_gtt::probe();
    let _ = drivers::intel_gtt::aper_smoke();
    // Auto-native: Intel KMS panel timing when available.
    if drivers::intel_kms::ready() {
        serial::write_str("[BOOT] KMS auto-native modeset\n");
        if drivers::intel_kms::modeset_panel() {
            serial::write_str("[BOOT] modeset_panel OK\n");
        } else {
            serial::write_str("[BOOT] modeset_panel fail - keep Multiboot LFB\n");
        }
    } else {
        serial::write_str("[BOOT] KMS not ready - Multiboot LFB\n");
    }
    if graphics::ready() && fb::is_ready() {
        serial::write_str("[DESKTOP] starting (no Ring3)\n");
        drivers::video::init();
        drivers::audio::init();
        crate::video_player::init();
        drivers::net::init();
        drivers::wifi::init(mbi);
        drivers::pci_usb_diag::dump_usb_controllers();
        drivers::xhci::probe();
    drivers::ehci::probe();
    drivers::uhci::probe();
    serial::write_str("\n[BOOT] auto SCANUSB\n");
    let _ = drivers::xhci::scan_usb_detail();
    let _ = drivers::ehci::scan_usb_detail();
        desktop::run();
    }

    vga_mark(17, b'X');
    drivers::pci_usb_diag::dump_usb_controllers();
    drivers::xhci::probe();
    drivers::ehci::probe();
    drivers::uhci::probe();
    serial::write_str("\n[BOOT] auto SCANUSB\n");
    let _ = drivers::xhci::scan_usb_detail();
    let _ = drivers::ehci::scan_usb_detail();
    vga_mark(18, b'H');
    // clear line 2 status
    unsafe {
        let mut i = 0usize;
        let msg = b"Aether READY - shell";
        while i < msg.len() {
            let p = (0xB8000 + 160 + i * 2) as *mut u16;
            *p = 0x0A00u16 | (msg[i] as u16);
            i += 1;
        }
    }
    serial::write_str("[OK] entering shell\n");
    shell::run();
}

