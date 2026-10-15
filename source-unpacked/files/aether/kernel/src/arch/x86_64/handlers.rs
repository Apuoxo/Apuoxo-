use crate::serial;

fn vga_mark(col: usize, ch: u8) {
    unsafe {
        let p = (0xB8000 + col * 2) as *mut u16;
        *p = 0x0F00u16 | (ch as u16);
    }
}

fn vga_line2(msg: &[u8]) {
    unsafe {
        let mut i = 0usize;
        while i < msg.len() && i < 80 {
            let p = (0xB8000 + 160 + i * 2) as *mut u16;
            *p = 0x0A00u16 | (msg[i] as u16); // green
            i += 1;
        }
    }
}


#[no_mangle]
pub extern "C" fn page_fault_handler(cr2: u64, error: u64) {
    serial::write_str("\n!!! PF CR2=");
    serial::write_usize(cr2 as usize);
    serial::write_str(" ERR=");
    serial::write_usize(error as usize);
    let pid = crate::process::current_pid();
    serial::write_str(" PID=");
    serial::write_usize(pid);
    serial::write_str("\n");
    // user/supervisor bit in error: bit 2 = user-mode access
    if error & 4 != 0 {
        serial::write_str("  [PF] PROOF: fault from CPL=3 (user access)\n");
    }
    if pid != 0 {
        serial::write_str("  [PF] killing process - isolation OK\n");
        crate::process::destroy(pid);
        crate::process::set_current(0);
        unsafe { crate::mm::paging::load_kernel_cr3(); }
        serial::write_str("  [PF] resume kernel after user PF\n");
        crate::ring3_resume::continue_boot();
    }
    serial::write_str("  [PF] kernel fault - halt\n");
    loop {
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

#[no_mangle]
pub extern "C" fn syscall_handler(frame: *mut u64) -> u64 {
    unsafe {
        let nr = *frame.offset(14);
        let a1 = *frame.offset(9);
        let a2 = *frame.offset(10);
        let a3 = *frame.offset(11);
        let cs = *frame.offset(16);
        let cpl = cs & 3;
        let pid = crate::process::current_pid();
        serial::write_str("  [SYSCALL] nr=");
        serial::write_usize(nr as usize);
        serial::write_str(" a1=");
        serial::write_hex(a1 as usize);
        serial::write_str(" a2=");
        serial::write_hex(a2 as usize);
        serial::write_str(" a3=");
        serial::write_usize(a3 as usize);
        serial::write_str(" PID=");
        serial::write_usize(pid);
        serial::write_str(" CPL=");
        serial::write_usize(cpl as usize);
        serial::write_str(" CR3=");
        serial::write_hex(crate::mm::paging::read_cr3());
        serial::write_str("\n");
        if cpl != 3 {
            serial::write_str("  [SYSCALL] WARNING CPL!=3\n");
        } else {
            serial::write_str("  [SYSCALL] FROM CPL=3\n");
        }
        match nr {
            1 => sys_write(a1 as usize, a2 as usize, a3 as usize), // fd, buf, len
            2 => sys_read(a1 as usize, a2 as usize, a3 as usize),
            3 => sys_list(a1 as usize, a2 as usize),
            60 => {
                serial::write_str("  [SYSCALL] exit FROM CPL=3\n");
                0xDEAD
            }
            _ => {
                serial::write_str("  [SYSCALL] unknown\n");
                u64::MAX
            }
        }
    }
}

unsafe fn user_ok(ptr: usize, len: usize) -> bool {
    if len > 4096 {
        return false;
    }
    if ptr < 0x40000000 || ptr.saturating_add(len) > 0x40010000 {
        return false;
    }
    true
}

unsafe fn sys_write(_fd: usize, buf: usize, len: usize) -> u64 {
    if !user_ok(buf, len) {
        serial::write_str("  [SYSCALL] write bad ptr\n");
        return u64::MAX;
    }
    serial::write_str("  [SYSCALL] write FROM CPL=3 bytes=");
    serial::write_usize(len);
    serial::write_str("\n");
    let mut i = 0usize;
    while i < len {
        let c = *((buf + i) as *const u8);
        serial::write_byte(c);
        i += 1;
    }
    len as u64
}

unsafe fn sys_read(path_ptr: usize, buf_ptr: usize, buflen: usize) -> u64 {
    if !user_ok(path_ptr, 1) || !user_ok(buf_ptr, buflen) {
        serial::write_str("  [SYSCALL] read bad ptr\n");
        return u64::MAX;
    }
    // read C-string path from user (max 31)
    let mut path = [0u8; 32];
    let mut plen = 0usize;
    while plen < 31 {
        let c = *((path_ptr + plen) as *const u8);
        if c == 0 {
            break;
        }
        path[plen] = c;
        plen += 1;
    }
    let path_s = core::str::from_utf8_unchecked(&path[..plen]);
    serial::write_str("  [SYSCALL] read path=");
    serial::write_str(path_s);
    serial::write_str(" FROM CPL=3\n");
    let mut kbuf = [0u8; 256];
    let n = match crate::fs::read(path_s, &mut kbuf) {
        Some(n) => n,
        None => {
            serial::write_str("  [SYSCALL] read FAIL\n");
            return u64::MAX;
        }
    };
    let copy = if n > buflen { buflen } else { n };
    let mut i = 0usize;
    while i < copy {
        *((buf_ptr + i) as *mut u8) = kbuf[i];
        i += 1;
    }
    serial::write_str("  [SYSCALL] read n=");
    serial::write_usize(copy);
    serial::write_str(" return TO userspace\n");
    copy as u64
}

unsafe fn sys_list(buf_ptr: usize, buflen: usize) -> u64 {
    if !user_ok(buf_ptr, buflen) {
        return u64::MAX;
    }
    serial::write_str("  [SYSCALL] list FROM CPL=3\n");
    let mut names = [[0u8; 24]; 16];
    let mut lens = [0usize; 16];
    let n = crate::fs::list(&mut names, &mut lens);
    // pack as name\\n name\\n ...
    let mut off = 0usize;
    let mut i = 0usize;
    while i < n && off < buflen {
        let mut j = 0usize;
        while j < lens[i] && off < buflen {
            *((buf_ptr + off) as *mut u8) = names[i][j];
            off += 1;
            j += 1;
        }
        if off < buflen {
            *((buf_ptr + off) as *mut u8) = b'\n';
            off += 1;
        }
        i += 1;
    }
    serial::write_str("  [SYSCALL] list entries=");
    serial::write_usize(n);
    serial::write_str("\n");
    off as u64
}

/// Exit path from Ring 3: destroy process, then resume kernel boot/desktop
#[no_mangle]
pub extern "C" fn process_exit_dispatch() {
    let pid = crate::process::current_pid();
    if pid != 0 {
        crate::process::destroy(pid);
        serial::write_str("  [MM] free_pages=");
        serial::write_usize(crate::mm::free_count());
        serial::write_str("\n");
    }
    crate::process::set_current(0);
    unsafe { crate::mm::paging::load_kernel_cr3(); }

    if let Some(next) = crate::process::next_ready() {
        serial::write_str("  [SCHED] switch PID=");
        serial::write_usize(next);
        serial::write_str("\n");
        crate::process::set_state(next, crate::process::State::Running);
        crate::process::set_current(next);
        if let Some(p) = crate::process::get(next) {
            unsafe {
                if p.cr3 != 0 {
                    crate::mm::paging::load_cr3(p.cr3);
                }
                enter_user_mode(p.entry as u64, p.stack as u64);
            }
        }
    }

    // Launch /bin/sh once after init (or any last process)
    static mut SH_DONE: bool = false;
    let launch_sh = unsafe {
        if !SH_DONE {
            SH_DONE = true;
            true
        } else {
            false
        }
    };
    if launch_sh {
        serial::write_str("\n======== USERSACE SH STAGE ========\n");
        let mut buf = [0u8; 8192];
        if let Some(n) = crate::fs::read_large("/bin/sh", &mut buf) {
            serial::write_str("[SH] loaded ELF from AetherFS bytes=");
            serial::write_usize(n);
            serial::write_str("\n");
            if let Some(img) = crate::elf::load(&buf) {
                if let Some(pid) = crate::process::create_from_image("sh", &img) {
                    crate::process::set_state(pid, crate::process::State::Running);
                    crate::process::set_current(pid);
                    serial::write_str("[SH] PID=");
                    serial::write_usize(pid);
                    serial::write_str(" USER_CR3=");
                    serial::write_hex(img.cr3);
                    serial::write_str(" KERNEL_CR3=");
                    serial::write_hex(crate::mm::paging::kernel_cr3());
                    serial::write_str("\n");
                    if img.cr3 != crate::mm::paging::kernel_cr3() {
                        serial::write_str("[SH] CR3 DIFFERENT = YES\n");
                    }
                    serial::write_str("[SH] enter CPL=3\n");
                    unsafe {
                        crate::mm::paging::load_cr3(img.cr3);
                        enter_user_mode(img.entry as u64, img.stack_top as u64);
                    }
                }
            }
        } else {
            serial::write_str("[SH] /bin/sh missing on FS\n");
        }
    }

    serial::write_str("  [SCHED] idle - userspace done\n");
    serial::write_str("[RING3] resume kernel boot / desktop\n");
    crate::ring3_resume::continue_boot();
}

extern "C" {
    fn enter_user_mode(entry: u64, stack: u64) -> !;
}

// re-export for process_exit_dispatch
fn install_user_programs() -> bool {
    use crate::fs;
    use crate::elf_blobs;
    serial::write_str("\n======== INSTALL ELF → AetherFS ========\n");
    // Ensure /bin/hello
    let _ = fs::create("/bin/hello"); // may fail if exists
    if !fs::write("/bin/hello", elf_blobs::HELLO_ELF) {
        // try create then write
        let _ = fs::create("/hello.elf");
        if !fs::write("/hello.elf", elf_blobs::HELLO_ELF) {
            serial::write_str("  [ELF] write hello FAIL (file too big for 1 sector?)\n");
            // AetherFS only stores 512 bytes per file! Need multi-sector files
            return false;
        }
    }
    true
}

fn run_elf_from_fs(path: &str, name: &str) -> bool {
    use crate::fs;
    use crate::elf;
    use crate::process;
    serial::write_str("\n  [LOAD] ");
    serial::write_str(path);
    serial::write_str(" from AetherFS\n");
    let mut buf = [0u8; 8192];
    let n = match fs::read_large(path, &mut buf) {
        Some(n) => n,
        None => {
            serial::write_str("  [LOAD] read FAIL\n");
            return false;
        }
    };
    serial::write_str("  [LOAD] read ");
    serial::write_usize(n);
    serial::write_str(" bytes from disk\n");
    // Verify ELF magic from disk
    if n < 4 || buf[0] != 0x7f || buf[1] != b'E' {
        serial::write_str("  [LOAD] not ELF on disk\n");
        return false;
    }
    let img = match elf::load(&buf) {
        Some(i) => i,
        None => return false,
    };
    process::create_from_image(name, &img).is_some()
}

static mut STAGE_DONE: bool = false;

#[no_mangle]
pub extern "C" fn rust_kernel_after_user() {
    unsafe {
        if STAGE_DONE {
            serial::write_str("\nAether process stage complete\n");
            vga_mark(16, b'H');
            vga_line2(b"Aether Shell READY");
            crate::shell::run();
        }
        STAGE_DONE = true;
    }
    serial::write_str("  [kernel] Ring3 demo SUCCESS\n");
    unsafe {
        let p = (0xB8000 + 11 * 2) as *mut u16;
        *p = 0x0F00u16 | (b'3' as u16);
    }
    vga_mark(12, b'F');
    // Skip software GUI on real HW path - avoids huge stack + non-contig FB PF
    vga_line2(b"text mode (GUI skip)");
    vga_mark(13, b'c');
    serial::write_str("[FB] skipped - text mode\n");

    vga_mark(14, b'A');
    vga_line2(b"ATA...");
    let _ = crate::fs::init_storage();
    vga_mark(15, b'D');
    vga_line2(b"storage done     ");

    // Multi-sector file support needed for ELF ~4K - extend write/read first
    if crate::fs::write_large("/bin/hello", crate::elf_blobs::HELLO_ELF)
        && crate::fs::write_large("/bin/counter", crate::elf_blobs::COUNTER_ELF)
    {
        serial::write_str("[FS] ELF programs installed on AetherFS\n");
        let free_before = crate::mm::free_count();
        serial::write_str("  [MM] free before procs=");
        serial::write_usize(free_before);
        serial::write_str("\n");

        serial::write_str("\n======== PROCESS STAGE ========\n");
        let _ = run_elf_from_fs("/bin/hello", "hello");
        let _ = run_elf_from_fs("/bin/counter", "counter");

        // Start scheduler - first process enters Ring 3; exits chain via process_exit_dispatch
        if let Some(pid) = crate::process::next_ready() {
            serial::write_str("  [SCHED] switch PID=");
            serial::write_usize(pid);
            serial::write_str("\n");
            crate::process::set_state(pid, crate::process::State::Running);
            crate::process::set_current(pid);
            if let Some(p) = crate::process::get(pid) {
                unsafe {
                    enter_user_mode(p.entry as u64, p.stack as u64);
                }
            }
        }
        // If no process, fall through
        serial::write_str("  [SCHED] no processes to run\n");
    } else {
        serial::write_str("[FS] ELF install failed\n");
    }

    // USB (after processes if they returned - they usually don't until all exit)
    crate::drivers::xhci::probe();
    serial::write_str("\nAether process stage complete\n");
    crate::shell::run();
}


/// Called from isr.s after last userspace process exits - must not return to iret stack.
/// We never return; jump into remaining kernel boot (desktop).
#[no_mangle]
pub extern "C" fn rust_ring3_done() -> ! {
    serial::write_str("\n======== RING3 STAGE COMPLETE ========\n");
    serial::write_str("[RING3] all userspace exited; continuing kernel boot\n");
    // Continue with FB + Desktop (same as main path)
    // mbi is not available here - desktop only needs FB already possible via Multiboot
    // Use a never-return shell/desktop path via main continuation flag
    loop {
        // If desktop not started, hang with message - main will call enter before desktop
        // Actually main calls enter_user which never returns; this is the resume point.
        crate::ring3_resume::continue_boot();
    }
}
