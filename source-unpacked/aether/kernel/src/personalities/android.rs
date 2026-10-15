//! Android Personality

use crate::personality::PersonalityId;
use crate::serial;
use crate::process;

pub fn init() {
    serial::write_str("  [android] init\n");
}

pub fn syscall(num: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    match num {
        1 => {
            serial::write_str("  [android] write\n");
            0
        }
        60 => {
            serial::write_str("  [android] exit\n");
            0
        }
        0x100 => {
            serial::write_str("  [android] binder\n");
            0
        }
        _ => {
            serial::write_str("  [android] ?\n");
            u64::MAX
        }
    }
}

pub fn start_hello_process() -> Option<usize> {
    process::create(PersonalityId::Android, 0x400000, 0x700000)
}
