//! Linux Personality

use crate::personality::PersonalityId;
use crate::serial;
use crate::process;

pub fn init() {
    serial::write_str("  [linux] init\n");
}

pub fn syscall(num: u64, a1: u64, _a2: u64, _a3: u64) -> u64 {
    match num {
        1 => { // write
            serial::write_str("  [linux] sys_write\n");
            a1
        }
        60 => { // exit
            serial::write_str("  [linux] sys_exit\n");
            0
        }
        39 => { // getpid
            1
        }
        _ => {
            serial::write_str("  [linux] syscall?\n");
            u64::MAX
        }
    }
}

pub fn start_hello_process() -> Option<usize> {
    process::create(PersonalityId::Linux, 0x400000, 0x7fff0000)
}
