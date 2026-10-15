//! Native Aether userspace launcher (Ring3 ELF from AetherFS)

use crate::elf;
use crate::fs;
use crate::process;
use crate::serial;
use crate::mm::paging;

/// Max ELF image we load from AetherFS for desktop launch
const ELF_CAP: usize = 16384;
static mut ELF_BUF: [u8; ELF_CAP] = [0; ELF_CAP];

fn is_elf(buf: &[u8]) -> bool {
    buf.len() >= 4 && buf[0] == 0x7f && buf[1] == b'E' && buf[2] == b'L' && buf[3] == b'F'
}

/// Load ELF from AetherFS path and enter Ring3. Does not return on success
/// (process_exit_dispatch → continue_boot → desktop).
pub fn run_elf_path(path: &str) -> bool {
    serial::write_str("[USER] run_elf ");
    serial::write_str(path);
    serial::write_str("\n");
    let n = unsafe {
        match fs::read_large(path, &mut ELF_BUF) {
            Some(n) => n,
            None => {
                serial::write_str("[USER] read FAIL\n");
                return false;
            }
        }
    };
    if n < 64 {
        serial::write_str("[USER] too small\n");
        return false;
    }
    let slice = unsafe { &ELF_BUF[..n] };
    if !is_elf(slice) {
        serial::write_str("[USER] not ELF\n");
        return false;
    }
    let img = match elf::load(slice) {
        Some(i) => i,
        None => {
            serial::write_str("[USER] elf::load FAIL\n");
            return false;
        }
    };
    let pid = match process::create_from_image(path, &img) {
        Some(p) => p,
        None => {
            serial::write_str("[USER] create process FAIL\n");
            return false;
        }
    };
    process::set_current(pid);
    serial::write_str("[USER] PID=");
    serial::write_usize(pid);
    serial::write_str(" CR3=");
    serial::write_hex(img.cr3);
    serial::write_str(" entry=");
    serial::write_hex(img.entry);
    serial::write_str(" -> CPL=3\n");
    extern "C" {
        fn enter_user_mode(entry: u64, stack: u64) -> !;
    }
    unsafe {
        paging::load_cr3(img.cr3);
        enter_user_mode(img.entry as u64, img.stack_top as u64);
    }
}

/// Non-! probe: is this path an ELF on AetherFS?
pub fn path_is_elf(path: &str) -> bool {
    let mut hdr = [0u8; 4];
    match fs::read(path, &mut hdr) {
        Some(n) if n >= 4 => is_elf(&hdr),
        _ => false,
    }
}
