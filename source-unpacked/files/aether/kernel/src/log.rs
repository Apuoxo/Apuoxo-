//! Simple kernel log

use crate::serial;

pub fn info(msg: &str) {
    serial::write_str("[INFO] ");
    serial::write_str(msg);
    serial::write_str("\n");
}

pub fn ok(msg: &str) {
    serial::write_str("[ OK ] ");
    serial::write_str(msg);
    serial::write_str("\n");
}
