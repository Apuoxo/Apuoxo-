#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    let vga = 0xb8000 as *mut u8;
    let text = b"ANDROID-LIKE OS  //  FIRST BOOT";
    unsafe {
        for (i, &b) in text.iter().enumerate() {
            core::ptr::write_volatile(vga.add(i * 2), b);
            core::ptr::write_volatile(vga.add(i * 2 + 1), 0x0f);
        }
    }
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
