use crate::serial;
use crate::graphics;
use crate::personality::{self, PersonalityId};
use crate::mm;
use crate::process;

pub fn draw_boot_screen() {
    serial::write_str("\n  AETHER v0.5 - Polymorphic Native Kernel\n\n");
}

pub fn try_init_graphics() {
    // В QEMU с -vga std часто есть linear FB, но адрес зависит от прошивки.
    // Для демонстрации пробуем типичный адрес Bochs/QEMU VBE (если доступен).
    // Если нет - просто работаем в text/serial.
    // Реальный GOP/VBE парсинг будет позже.
    let candidate = 0xFD000000usize; // common QEMU VBE
    // Не пишем в неизвестную память. Только если явно инициализировано.
    let _ = candidate;
}

pub fn draw_status() {
    serial::write_str("  +--------------------------------------+\n");
    serial::write_str("  | Memory free pages : ");
    serial::write_usize(mm::free_count());
    serial::write_str("\n");
    serial::write_str("  | Linux   : ");
    if personality::is_loaded(PersonalityId::Linux) { serial::write_str("LOADED"); } else { serial::write_str("----"); }
    serial::write_str("\n");
    serial::write_str("  | Windows : ");
    if personality::is_loaded(PersonalityId::Windows) { serial::write_str("LOADED"); } else { serial::write_str("----"); }
    serial::write_str("\n");
    serial::write_str("  | Android : ");
    if personality::is_loaded(PersonalityId::Android) { serial::write_str("LOADED"); } else { serial::write_str("----"); }
    serial::write_str("\n");
    serial::write_str("  | Processes: ");
    serial::write_usize(
        process::count_by_personality(PersonalityId::Linux) +
        process::count_by_personality(PersonalityId::Windows) +
        process::count_by_personality(PersonalityId::Android)
    );
    serial::write_str("\n");
    serial::write_str("  +--------------------------------------+\n");
}

pub fn draw_desktop() {
    if graphics::ready() {
        // Простой "рабочий стол"
        graphics::fill(0x00101828); // тёмный фон
        graphics::fill_rect(0, 0, graphics::width(), 32, 0x00204060); // панель
        graphics::draw_str(12, 10, "AETHER", 0x00FFFFFF);
        graphics::fill_rect(20, 60, 200, 120, 0x00305070); // окно
        graphics::draw_str(30, 70, "LINUX", 0x00AAFFAA);
        graphics::fill_rect(240, 60, 200, 120, 0x00503050);
        graphics::draw_str(250, 70, "WINDOWS", 0x00FFAAAA);
    }
}
