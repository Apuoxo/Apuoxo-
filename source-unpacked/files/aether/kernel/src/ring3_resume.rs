//! Resume kernel boot after Ring3 stage (no stack return from enter_user)

use crate::serial;
use crate::fb;
use crate::graphics;
use crate::drivers;
use crate::desktop;
use crate::shell;

static mut MBI: usize = 0;
static mut ARMED: bool = false;

pub fn arm(mbi: usize) {
    unsafe {
        MBI = mbi;
        ARMED = true;
    }
}

pub fn continue_boot() -> ! {
    let mbi = unsafe { MBI };
    serial::write_str("[BOOT] resume after Ring3, mbi=");
    serial::write_hex(mbi as usize);
    serial::write_str("\n");
    fb::try_init(mbi);
    if graphics::ready() && fb::is_ready() {
        serial::write_str("[DESKTOP] post-Ring3 start\n");
        drivers::video::init();
        drivers::intel_igpu::init();
        serial::write_str("[BOOT] KMS init (post-Ring3)\n");
        drivers::intel_kms::init();
        serial::write_str("[BOOT] KMS step logged above\n");
        drivers::audio::init();
        drivers::net::init();
        drivers::wifi::init(mbi);
        drivers::pci_usb_diag::dump_usb_controllers();
        drivers::xhci::probe();
        desktop::run();
    }
    serial::write_str("[BOOT] desktop unavailable - shell\n");
    shell::run();
}
