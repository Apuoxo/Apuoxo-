//! Aether cooperative scheduler

use crate::process::{self, State};
use crate::serial;

extern "C" {
    fn enter_user_mode(entry: u64, stack: u64) -> !;
}

pub fn run_all() {
    serial::write_str("\n======== SCHEDULER ========\n");
    loop {
        match process::next_ready() {
            Some(pid) => {
                process::account_run(pid, 1);
                serial::write_str("  [SCHED] switch PID=");
                serial::write_usize(pid);
                serial::write_str("\n");
                process::set_state(pid, State::Running);
                process::set_current(pid);
                if let Some(p) = process::get(pid) {
                    unsafe { enter_user_mode(p.entry as u64, p.stack as u64); }
                }
            }
            None => {
                serial::write_str("  [SCHED] idle\n");
                break;
            }
        }
    }
}

pub fn current() -> usize {
    process::current_pid()
}

pub fn schedule() {}
