//! Windows Personality - stage 1: PE32+ load into process address space.
//! Stage 2 will add import resolution + minimal NT ABI before entry is invoked.

use crate::personality::PersonalityId;
use crate::serial;
use crate::process;
use crate::pe;

pub fn init() {
    serial::write_str("  [windows] stage1 PE loader ready\n");
}

pub fn syscall(num: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // Placeholder NT statuses - not a real syscall surface yet.
    match num {
        0x01 => {
            serial::write_str("  [windows] NtWriteFile (stub)\n");
            0
        }
        0x2C => {
            serial::write_str("  [windows] NtTerminateProcess (stub)\n");
            0
        }
        _ => {
            serial::write_str("  [windows] NT stub unknown\n");
            u64::MAX
        }
    }
}

pub fn start_hello_process() -> Option<usize> {
    process::create(PersonalityId::Windows, 0x401000, 0x800000)
}

pub fn try_run_from_ntfs(mft_ref: u32, name: &[u8]) -> &'static str {
    pe::try_launch_ntfs(mft_ref, name)
}

pub fn pe_info_from_ntfs(mft_ref: u32) -> Option<pe::PeInfo> {
    pe::probe_ntfs(mft_ref)
}
