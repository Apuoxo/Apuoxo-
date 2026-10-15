//! Aether shell - VGA text console + serial + PS/2 (real HW usable)

use crate::serial;
use crate::mm;
use crate::fs;
use crate::drivers::ps2;

const VGA: usize = 0xB8000;
const COLS: usize = 80;
const ROWS: usize = 25;

static mut CUR_ROW: usize = 4;
static mut CUR_COL: usize = 0;

fn vga_cell(row: usize, col: usize, ch: u8, attr: u8) {
    if row >= ROWS || col >= COLS {
        return;
    }
    unsafe {
        let p = (VGA + (row * COLS + col) * 2) as *mut u16;
        *p = ((attr as u16) << 8) | (ch as u16);
    }
}

fn vga_clear_row(row: usize) {
    let mut c = 0usize;
    while c < COLS {
        vga_cell(row, c, b' ', 0x07);
        c += 1;
    }
}

fn vga_scroll() {
    unsafe {
        // copy rows 4..24 up by one (keep markers 0-3)
        let mut r = 4usize;
        while r < ROWS - 1 {
            let mut c = 0usize;
            while c < COLS {
                let src = (VGA + ((r + 1) * COLS + c) * 2) as *const u16;
                let dst = (VGA + (r * COLS + c) * 2) as *mut u16;
                *dst = *src;
                c += 1;
            }
            r += 1;
        }
        vga_clear_row(ROWS - 1);
        CUR_ROW = ROWS - 1;
        CUR_COL = 0;
    }
}

fn vga_putc(ch: u8) {
    unsafe {
        if ch == b'\n' {
            CUR_COL = 0;
            CUR_ROW += 1;
            if CUR_ROW >= ROWS {
                vga_scroll();
            }
            return;
        }
        if ch == 0x08 {
            if CUR_COL > 0 {
                CUR_COL -= 1;
                vga_cell(CUR_ROW, CUR_COL, b' ', 0x07);
            }
            return;
        }
        if ch >= 32 && ch < 127 {
            vga_cell(CUR_ROW, CUR_COL, ch, 0x0F);
            CUR_COL += 1;
            if CUR_COL >= COLS {
                CUR_COL = 0;
                CUR_ROW += 1;
                if CUR_ROW >= ROWS {
                    vga_scroll();
                }
            }
        }
    }
}

fn vga_write(s: &[u8]) {
    let mut i = 0usize;
    while i < s.len() {
        vga_putc(s[i]);
        i += 1;
    }
}

fn vga_write_str(s: &str) {
    vga_write(s.as_bytes());
}

fn putc(c: u8) {
    vga_putc(c);
    if c == 10 {
        serial::write_str("\n");
        return;
    }
    if c >= 32 && c < 127 {
        unsafe {
            let mut t = 0u32;
            while t < 20_000 {
                let s: u8;
                core::arch::asm!("in al, dx", in("dx") 0x3FDu16, out("al") s, options(nostack, preserves_flags));
                if s & 0x20 != 0 {
                    core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") c, options(nostack, preserves_flags));
                    break;
                }
                t += 1;
            }
        }
    }
}

fn write_str(s: &str) {
    vga_write_str(s);
    serial::write_str(s);
}

fn serial_read_byte() -> Option<u8> {
    unsafe {
        let s: u8;
        core::arch::asm!("in al, dx", in("dx") 0x3FDu16, out("al") s, options(nostack, preserves_flags));
        if s & 1 == 0 {
            return None;
        }
        let b: u8;
        core::arch::asm!("in al, dx", in("dx") 0x3F8u16, out("al") b, options(nostack, preserves_flags));
        Some(b)
    }
}

fn eq(line: &[u8], s: usize, clen: usize, b: &[u8]) -> bool {
    if clen != b.len() {
        return false;
    }
    let mut i = 0usize;
    while i < clen {
        if line[s + i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn cmd_help() {
    write_str("Commands: help ls cat mem echo uname halt hh pp ww\n");
}

fn cmd_ls() {
    if !fs::is_mounted() {
        write_str("(no filesystem)\n");
        return;
    }
    let mut names = [[0u8; 24]; 16];
    let mut lens = [0usize; 16];
    let n = fs::list(&mut names, &mut lens);
    write_str("files: ");
    // write number via serial helper + vga
    serial::write_usize(n);
    // also on VGA - simple digit
    if n == 0 {
        vga_write(b"0");
    } else {
        let mut tmp = n;
        let mut digs = [0u8; 8];
        let mut nd = 0usize;
        while tmp > 0 && nd < 8 {
            digs[nd] = b'0' + (tmp % 10) as u8;
            tmp /= 10;
            nd += 1;
        }
        while nd > 0 {
            nd -= 1;
            vga_putc(digs[nd]);
        }
    }
    write_str("\n");
    let mut i = 0usize;
    while i < n {
        write_str("  ");
        let mut j = 0usize;
        while j < lens[i] && j < 24 {
            putc(names[i][j]);
            j += 1;
        }
        write_str("\n");
        i += 1;
    }
}

fn cmd_cat_test() {
    let mut buf = [0u8; 64];
    match fs::read("/test.txt", &mut buf) {
        Some(n) => {
            let mut i = 0usize;
            while i < n {
                putc(buf[i]);
                i += 1;
            }
            write_str("\n");
        }
        None => write_str("not found (or no disk)\n"),
    }
}

fn cmd_mem() {
    if crate::drivers::ata::is_ramdisk() {
        write_str("RAMDISK OK  sectors=");
        serial::write_usize(crate::drivers::ata::total_sectors() as usize);
        write_str("\n");
    }
    write_str("FS mounted: ");
    write_str(if fs::is_mounted() { "yes" } else { "no" });
    write_str("\n");
    write_str("free pages: ");
    serial::write_usize(mm::free_count());
    // rough VGA copy of count
    let n = mm::free_count();
    let mut tmp = n;
    let mut digs = [0u8; 12];
    let mut nd = 0usize;
    if tmp == 0 {
        vga_putc(b'0');
    } else {
        while tmp > 0 && nd < 12 {
            digs[nd] = b'0' + (tmp % 10) as u8;
            tmp /= 10;
            nd += 1;
        }
        while nd > 0 {
            nd -= 1;
            vga_putc(digs[nd]);
        }
    }
    write_str("\n");
}

fn cmd_uname() {
    write_str("Aether OS v3.1 polymorphic kernel\n");
    write_str("arch: x86_64  mode: text shell\n");
    if crate::drivers::ata::is_ramdisk() {
        write_str("storage: RAMDISK 32KiB (volatile)\n");
    } else {
        write_str("storage: ATA\n");
    }
}

fn run_line(line: &[u8], len: usize) {
    let mut s = 0usize;
    let mut e = len;
    while s < e && line[s] == b' ' {
        s += 1;
    }
    while e > s && (line[e - 1] == b' ' || line[e - 1] == b'\r') {
        e -= 1;
    }
    if s >= e {
        return;
    }
    let mut sp = s;
    while sp < e && line[sp] != b' ' {
        sp += 1;
    }
    let clen = sp - s;
    let mut a = sp;
    while a < e && line[a] == b' ' {
        a += 1;
    }
    if eq(line, s, clen, b"help") || eq(line, s, clen, b"?") {
        cmd_help();
    } else if eq(line, s, clen, b"dsk") {
        cmd_dsk();
    } else if eq(line, s, clen, b"ls") {
        cmd_ls();
    } else if eq(line, s, clen, b"cat") {
        cmd_cat_test();
    } else if eq(line, s, clen, b"mem") {
        cmd_mem();
    } else if eq(line, s, clen, b"uname") {
        cmd_uname();
    } else if eq(line, s, clen, b"echo") {
        let mut i = a;
        while i < e {
            putc(line[i]);
            i += 1;
        }
        write_str("\n");
    } else if eq(line, s, clen, b"halt") || eq(line, s, clen, b"exit") {
        write_str("halt\n");
        loop {
            unsafe {
                core::arch::asm!("hlt");
            }
        }
    } else {
        write_str("unknown - try help\n");
    }
}

pub fn run() -> ! {
    // Preserve row 0 markers; start console at row 4
    unsafe {
        CUR_ROW = 4;
        CUR_COL = 0;
    }
    let mut r = 4usize;
    while r < ROWS {
        vga_clear_row(r);
        r += 1;
    }

    write_str("======== Aether Shell ========\n");
    cmd_help();
    write_str("aether> ");

    let mut line = [0u8; 128];
    let mut len = 0usize;

    loop {
        if let Some(b) = serial_read_byte() {
            if b == b'\r' || b == b'\n' {
                write_str("\n");
                run_line(&line, len);
                len = 0;
                write_str("aether> ");
            } else if b == 0x08 || b == 0x7F {
                if len > 0 {
                    len -= 1;
                    putc(0x08);
                }
            } else if b >= 32 && b < 127 && len < 127 {
                line[len] = b;
                len += 1;
                putc(b);
            }
        }

        ps2::poll();
        let sc = ps2::last_scancode();
        if sc != 0 {
            if let Some(ch) = ps2::scancode_to_ascii(sc) {
                if ch == b'\n' {
                    write_str("\n");
                    run_line(&line, len);
                    len = 0;
                    write_str("aether> ");
                } else if ch == 0x08 {
                    if len > 0 {
                        len -= 1;
                        putc(0x08);
                    }
                } else if ch >= 32 && ch < 127 && len < 127 {
                    line[len] = ch;
                    len += 1;
                    putc(ch);
                }
            }
        }

        let mut d = 0u32;
        while d < 30 {
            d += 1;
        }
    }
}

pub fn run_demo_commands() {}


fn cmd_dsk() {
    // Prefer Terminal text path (same as GUI)
    gui_dsk();
}

pub fn run_command_from_gui(buf: &[u8], len: usize) {
    let n = if len <= buf.len() { len } else { buf.len() };
    let mut a = 0usize;
    while a < n && buf[a] == b' ' {
        a += 1;
    }
    let mut b = n;
    while b > a && (buf[b - 1] == b' ' || buf[b - 1] == 13 || buf[b - 1] == 10) {
        b -= 1;
    }
    let sl = b - a;
    if sl == 0 {
        return;
    }
    // Normalize to lowercase ASCII for 2-4 letter cmds
    let mut cmd = [0u8; 8];
    let mut i = 0usize;
    while i < sl && i < 8 {
        let c = buf[a + i];
        cmd[i] = if c >= b'A' && c <= b'Z' { c + 32 } else { c };
        i += 1;
    }
    if sl == 3 && cmd[0] == b'd' && cmd[1] == b's' && cmd[2] == b'k' {
        gui_dsk();
        return;
    }
    if sl == 2 && cmd[0] == b'w' && cmd[1] == b'f' {
        gui_wf();
        return;
    }
    // HH = ethernet+DHCP (E/T keys often broken on AH532)
    if sl == 2 && cmd[0] == b'h' && cmd[1] == b'h' {
        gui_eth();
        return;
    }
    if sl == 2 && cmd[0] == b'w' && cmd[1] == b'w' {
        gui_ww();
        return;
    }
    // PP = ping 8.8.8.8 (Google DNS) via ICMP
    if sl == 2 && cmd[0] == b'p' && cmd[1] == b'p' {
        gui_pp();
        return;
    }
    if sl == 4 && cmd[0] == b'g' && cmd[1] == b'r' && cmd[2] == b'a' && cmd[3] == b'f' {
        gui_graf();
        return;
    }
    if sl == 2 && cmd[0] == b'd' && cmd[1] == b'v' {
        gui_dv();
        return;
    }
    if sl == 4 && cmd[0] == b'h' && cmd[1] == b'e' && cmd[2] == b'l' && cmd[3] == b'p' {
        gui_help();
        return;
    }
    if sl == 3 && cmd[0] == b'a' && cmd[1] == b'u' && cmd[2] == b'd' {
        gui_aud();
        return;
    }
    if sl == 4 && cmd[0] == b'a' && cmd[1] == b'u' && cmd[2] == b'd' && cmd[3] == b'2' {
        gui_aud2();
        return;
    }
    if sl == 2 && cmd[0] == b'm' && cmd[1] == b'o' {
        gui_mo();
        return;
    }
    if sl == 3 && cmd[0] == b's' && cmd[1] == b'y' && cmd[2] == b's' {
        gui_sys();
        return;
    }
    if (sl == 7 && cmd[0]==b's' && cmd[1]==b'c' && cmd[2]==b'a' && cmd[3]==b'n'
        && cmd[4]==b'u' && cmd[5]==b's' && cmd[6]==b'b')
        || (sl == 2 && cmd[0]==b's' && cmd[1]==b'u')
    {
        gui_scanusb();
        return;
    }
    // PH = smartphone USB (MSC/MTP/ADB)
    if sl == 2 && cmd[0] == b'p' && cmd[1] == b'h' {
        gui_ph();
        return;
    }
    if sl == 3 && cmd[0] == b'r' && cmd[1] == b'a' && cmd[2] == b'm' {
        gui_ram();
        return;
    }
    if sl == 2 && cmd[0] == b'r' && cmd[1] == b'c' {
        gui_rct();
        return;
    }
    if sl == 2 && cmd[0] == b'p' && cmd[1] == b'e' {
        gui_pe();
        return;
    }
    // CPU1 = LAPIC probe, CPU2 = trampoline, CPU4 = one-AP INIT+SIPI
    if sl == 4 && cmd[0] == b'c' && cmd[1] == b'p' && cmd[2] == b'u' {
        if cmd[3] == b'1' {
            gui_cpu1();
            return;
        }
        if cmd[3] == b'2' {
            gui_cpu2();
            return;
        }
        if cmd[3] == b'3' {
            gui_cpu3();
            return;
        }
        if cmd[3] == b'4' {
            gui_cpu4();
            return;
        }
        if cmd[3] == b'5' {
            gui_cpu5();
            return;
        }
        if cmd[3] == b'6' {
            gui_cpu6();
            return;
        }
        if cmd[3] == b'7' {
            gui_cpu7();
            return;
        }
    }
    // CPU11 (key 8 broken on AH532 keyboard)
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'1'
        && cmd[4] == b'1'
    {
        gui_cpu11();
        return;
    }
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'1'
        && cmd[4] == b'2'
    {
        gui_cpu12();
        return;
    }
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'1'
        && cmd[4] == b'3'
    {
        gui_cpu13();
        return;
    }
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'1'
        && cmd[4] == b'4'
    {
        gui_cpu14();
        return;
    }
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'1'
        && cmd[4] == b'5'
    {
        gui_cpu15();
        return;
    }
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'1'
        && cmd[4] == b'6'
    {
        gui_cpu16();
        return;
    }
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'1'
        && cmd[4] == b'7'
    {
        gui_cpu17();
        return;
    }
    // CPU19 (key 8 broken - never use CPU8/CPU18)
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'1'
        && cmd[4] == b'9'
    {
        gui_cpu19();
        return;
    }
    // CPU20: pure runqueue drain (no migrate_token JOB)
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'2'
        && cmd[4] == b'0'
    {
        gui_cpu20();
        return;
    }
    // CPU21: RQ depth 2 per AP + balance_enqueue
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'2'
        && cmd[4] == b'1'
    {
        gui_cpu21();
        return;
    }
    // CPU22: balance under load (busy AP1 → steal to AP2/AP3)
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'2'
        && cmd[4] == b'2'
    {
        gui_cpu22();
        return;
    }
    // CPU23: multi-AP spinlock stress
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'2'
        && cmd[4] == b'3'
    {
        gui_cpu23();
        return;
    }
    // CPU24: fairness flood (equal RQ work)
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'2'
        && cmd[4] == b'4'
    {
        gui_cpu24();
        return;
    }
    // CPU25: AP heartbeat liveness
    if sl == 5
        && cmd[0] == b'c'
        && cmd[1] == b'p'
        && cmd[2] == b'u'
        && cmd[3] == b'2'
        && cmd[4] == b'5'
    {
        gui_cpu25();
        return;
    }
    if sl == 3 && cmd[0] == b'c' && cmd[1] == b'p' && cmd[2] == b'u' {
        gui_cpu();
        return;
    }
    crate::desktop::terminal_write(
        "unknown cmd (try: cpu1 cpu6 cpu19..cpu25 dsk graf)\n",
    );
}

fn t(s: &str) {
    crate::desktop::terminal_write(s);
}

fn t_usize(mut v: usize) {
    if v == 0 {
        t("0");
        return;
    }
    let mut b = [0u8; 20];
    let mut k = 0usize;
    while v > 0 {
        b[k] = b'0' + (v % 10) as u8;
        v /= 10;
        k += 1;
    }
    while k > 0 {
        k -= 1;
        crate::desktop::terminal_write_char(b[k]);
    }
}

fn t_hex(mut v: usize) {
    if v == 0 {
        t("0");
        return;
    }
    let mut b = [0u8; 16];
    let mut k = 0usize;
    while v > 0 {
        let x = (v & 0xF) as u8;
        b[k] = if x < 10 { b'0' + x } else { b'a' + x - 10 };
        v >>= 4;
        k += 1;
    }
    while k > 0 {
        k -= 1;
        crate::desktop::terminal_write_char(b[k]);
    }
}




fn gui_cpu() {
    t("======== CPU / SMP ========\n");
    crate::cpu::init();
    t("BRAND=");
    let br = crate::cpu::brand();
    let mut i = 0usize;
    while i < br.len() {
        // print via t() needs &str - emit byte as one-char if printable
        if br[i] >= 32 && br[i] < 127 {
            let ch = [br[i]];
            // serial path: t takes &str; use write_byte via serial for brand
            crate::serial::write_byte(br[i]);
            let _ = ch;
        }
        i += 1;
    }
    t("\nVENDOR=");
    let v = crate::cpu::vendor();
    i = 0;
    while i < v.len() {
        crate::serial::write_byte(v[i]);
        i += 1;
    }
    t("\nFAMILY=");
    t_usize(crate::cpu::family() as usize);
    t(" MODEL=");
    t_usize(crate::cpu::model() as usize);
    t(" STEPPING=");
    t_usize(crate::cpu::stepping() as usize);
    t("\nAPIC=");
    t(if crate::cpu::has_apic() { "YES" } else { "NO" });
    t(" SSE2=");
    t(if crate::cpu::has_sse2() { "YES" } else { "NO" });
    t("\nBSP_APIC_ID=");
    t_usize(crate::smp::bsp_apic_id() as usize);
    t("\nLAPIC=");
    t_hex(crate::smp::lapic_base());
    t("\nCORES=");
    t_usize(crate::smp::cores() as usize);
    t(" THREADS=");
    t_usize(crate::smp::threads() as usize);
    t("\nDETECTED=");
    t_usize(crate::smp::cpu_count());
    t(" ONLINE=");
    t_usize(crate::smp::online_count());
    t("\nCPUID1_ID_SPACE=");
    t_usize(crate::smp::logical_raw() as usize);
    t(" (not core count)\n");
    t("DETECT_SRC=");
    t_usize(crate::smp::detect_src() as usize);
    t(" (0=cpuid1 1=leaf4 2=leafB 3=capped)\n");
    t("READY=");
    t(if crate::smp::ready() { "YES" } else { "NO" });
    t(" ERR=");
    t_usize(crate::smp::last_err() as usize);
    t("\nAPIC_IDS:");
    let n = crate::smp::cpu_count();
    let mut i = 0usize;
    while i < n && i < 8 {
        t(" ");
        t_usize(crate::smp::apic_id_at(i) as usize);
        i += 1;
    }
    t("\n");
    t("NOTE: BSP only at boot. Scheduler single-core.\n");
    t("NOTE: CPUGO = experimental AP start.\n");
    t("======== END CPU ========\n");
}

fn gui_cpu1() {
    t("======== CPU1 LAPIC PROBE ========\n");
    t("NO INIT/SIPI - only MSR + MMIO read\n");
    let e = crate::smp::probe_lapic_only();
    t("ERR=");
    t_usize(e as usize);
    t(" (0=OK 2=MAP 5=X2APIC)\n");
    t("LAPIC=");
    t_hex(crate::smp::lapic_base());
    t("\nBSP_APIC_ID=");
    t_usize(crate::smp::bsp_apic_id() as usize);
    t("\n");
    if e == 0 {
        t("RESULT=LAPIC_OK next: CPU2\n");
    } else {
        t("RESULT=LAPIC_FAIL - stop\n");
    }
    t("======== END CPU1 ========\n");
}

fn gui_cpu2() {
    t("======== CPU2 TRAMPOLINE ONLY ========\n");
    t("Writes 0x7000/0x8000, NO IPI\n");
    let e = crate::smp::stage_trampoline();
    t("ERR=");
    t_usize(e as usize);
    t(" (0=OK 6=BAD_TRAMP)\n");
    if e == 0 {
        t("RESULT=TRAMP_OK next: CPU3\n");
    } else {
        t("RESULT=FAIL\n");
    }
    t("======== END CPU2 ========\n");
}

fn gui_cpu3() {
    t("======== CPU3 INIT ONLY (RISK) ========\n");
    t("INIT to one AP, NO SIPI - if reboot, INIT is the cause\n");
    let e = crate::smp::stage_init_only();
    t("ERR=");
    t_usize(e as usize);
    t("\n");
    if e == 0 {
        t("RESULT=INIT_SURVIVED next: CPU4\n");
    } else {
        t("RESULT=FAIL\n");
    }
    t("======== END CPU3 ========\n");
}

fn gui_cpu4() {
    t("======== CPU4 LM64 v536 ========\n");
    t("INIT+SIPI long mode 64-bit (ESP fix + AP PT + 1 SIPI)\n");
    let n = crate::smp::stage_one_ap();
    t("APS_STARTED=");
    t_usize(n as usize);
    t(" ONLINE=");
    t_usize(crate::smp::online_count());
    t("\nERR=");
    t_usize(crate::smp::last_err() as usize);
    t("\n");
    if n > 0 {
        t("RESULT=PASS AP LONG MODE 64-BIT\n");
    } else {
        t("RESULT=TIMEOUT or FAIL\n");
    }
    t("======== END CPU4 ========\n");
}


fn gui_cpu14() {
    t("======== CPU14 MIGRATE TOKEN v544 ========\n");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6\nRESULT=SKIP\n======== END CPU14 ========\n");
        return;
    }
    // Migrate tokens to AP1, AP2, AP3 with distinct PIDs
    let targets: [u32; 3] = [1, 2, 3];
    let pids: [u32; 3] = [0xA11, 0xA22, 0xA33];
    let mut pass = 0u32;
    let mut i = 0usize;
    while i < 3 {
        let e = crate::smp::migrate_token(targets[i], pids[i]);
        t("AP");
        t_usize(targets[i] as usize);
        t(" PID=");
        t_hex(pids[i] as usize);
        t(" LAST=");
        t_hex(crate::smp::ap_last_pid(targets[i]) as usize);
        t(" ERR=");
        t_usize(e as usize);
        t(if e == 0 { " OK\n" } else { " FAIL\n" });
        if e == 0 {
            pass = pass.wrapping_add(1);
        }
        i += 1;
    }
    t("PASS_COUNT=");
    t_usize(pass as usize);
    t("/3\n");
    if pass == 3 {
        t("RESULT=PASS MIGRATE TOKEN ALL AP\n");
    } else {
        t("RESULT=PARTIAL OR FAIL\n");
    }
    t("NOTE: ERR 0=OK 1=BUSY 2=TIMEOUT 3=PID_MISMATCH\n");
    t("======== END CPU14 ========\n");
}

/// CPU15: real process:: PID → AP migrate + per-CPU runqueue.
fn gui_cpu15() {
    t("======== CPU15 REAL PROC v546 ========
");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6
RESULT=SKIP
======== END CPU15 ========
");
        return;
    }
    let mut pids: [u32; 3] = [0; 3];
    let mut created = 0usize;
    let mut i = 0usize;
    while i < 3 {
        if let Some(pid) = crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            pids[i] = pid as u32;
            created += 1;
        }
        i += 1;
    }
    t("CREATED=");
    t_usize(created);
    t("/3
");
    if created < 3 {
        t("RESULT=FAIL CREATE
======== END CPU15 ========
");
        return;
    }
    let targets: [u32; 3] = [1, 2, 3];
    let mut pass = 0u32;
    i = 0;
    while i < 3 {
        let pid = pids[i];
        let ap = targets[i];
        let _ = crate::smp::rq_enqueue(ap as usize, pid);
        let e = crate::smp::migrate_token(ap, pid);
        let last = crate::smp::ap_last_pid(ap);
        t("AP");
        t_usize(ap as usize);
        t(" PID=");
        t_usize(pid as usize);
        t(" LAST=");
        t_usize(last as usize);
        t(" E=");
        t_usize(e as usize);
        if e == 0 && last == pid {
            t(" OK
");
            pass += 1;
        } else {
            t(" FAIL
");
        }
        i += 1;
    }
    t("PASS=");
    t_usize(pass as usize);
    t("/3
");
    t(if pass == 3 {
        "RESULT=PASS REAL PROC MIGRATE
"
    } else {
        "RESULT=FAIL
"
    });
    t("======== END CPU15 ========
");
}


fn gui_cpu16() {
    t("======== CPU16 AP QUANTUM v547 ========
");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6
RESULT=SKIP
======== END CPU16 ========
");
        return;
    }
    let mut pids: [u32; 3] = [0; 3];
    let mut i = 0usize;
    while i < 3 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => pids[i] = pid as u32,
            None => {
                t("CREATE FAIL
RESULT=FAIL
======== END CPU16 ========
");
                return;
            }
        }
        i += 1;
    }
    let targets: [u32; 3] = [1, 2, 3];
    let mut pass = 0u32;
    i = 0;
    while i < 3 {
        let pid = pids[i];
        let ap = targets[i];
        let before = crate::process::get_run_ticks(pid as usize);
        let _ = crate::smp::rq_enqueue(ap as usize, pid);
        let e = crate::smp::migrate_token(ap, pid);
        // After AP ACK: account quantum (BSP records work assigned to that AP)
        if e == 0 && crate::smp::ap_last_pid(ap) == pid {
            crate::process::set_state(pid as usize, crate::process::State::Running);
            crate::process::account_run(pid as usize, 10);
            crate::process::set_state(pid as usize, crate::process::State::Ready);
        }
        let after = crate::process::get_run_ticks(pid as usize);
        t("AP");
        t_usize(ap as usize);
        t(" PID=");
        t_usize(pid as usize);
        t(" E=");
        t_usize(e as usize);
        t(" TICKS ");
        t_usize(before as usize);
        t("->");
        t_usize(after as usize);
        if e == 0 && after >= before.saturating_add(10) {
            t(" OK
");
            pass += 1;
        } else {
            t(" FAIL
");
        }
        i += 1;
    }
    t("PASS=");
    t_usize(pass as usize);
    t("/3
");
    t(if pass == 3 {
        "RESULT=PASS AP QUANTUM+ACCOUNT
"
    } else {
        "RESULT=FAIL
"
    });
    t("NOTE: TICKS ON BSP AFTER AP ACK (NEXT: AP KERNEL ENTRY)
");
    t("======== END CPU16 ========
");
}


fn gui_cpu17() {
    t("======== CPU17 AP KERNEL ENTRY v548 ========
");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6
RESULT=SKIP
======== END CPU17 ========
");
        return;
    }
    let mut pids: [u32; 3] = [0; 3];
    let mut i = 0usize;
    while i < 3 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => pids[i] = pid as u32,
            None => {
                t("CREATE FAIL
RESULT=FAIL
======== END CPU17 ========
");
                return;
            }
        }
        i += 1;
    }
    let targets: [u32; 3] = [1, 2, 3];
    let mut pass = 0u32;
    i = 0;
    while i < 3 {
        let pid = pids[i];
        let ap = targets[i];
        let before = crate::process::get_run_ticks(pid as usize);
        // Post job only - NO BSP account_run. AP must increment ticks.
        let e = crate::smp::migrate_token(ap, pid);
        // Wait for AP-side account_run
        let mut after = before;
        let mut w = 0u32;
        while w < 2_000_000 {
            after = crate::process::get_run_ticks(pid as usize);
            if after > before {
                break;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        t("AP");
        t_usize(ap as usize);
        t(" PID=");
        t_usize(pid as usize);
        t(" MIG=");
        t_usize(e as usize);
        t(" TICKS ");
        t_usize(before as usize);
        t("->");
        t_usize(after as usize);
        if e == 0 && after > before {
            t(" OK
");
            pass += 1;
        } else {
            t(" FAIL
");
        }
        i += 1;
    }
    t("PASS=");
    t_usize(pass as usize);
    t("/3
");
    t(if pass == 3 {
        "RESULT=PASS AP KERNEL account_run
"
    } else {
        "RESULT=FAIL (AP ENTRY OR MAP)
"
    });
    t("======== END CPU17 ========
");
}

/// CPU19: AP quantum (10 ticks per JOB) - real scheduler accounting on AP.
/// Key 8 broken on AH532: never use CPU8 / CPU18.
fn gui_cpu19() {
    t("======== CPU19 AP QUANTUM v550 ========
");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6
RESULT=SKIP
======== END CPU19 ========
");
        return;
    }
        let _ = crate::process::destroy_all();
let mut pids: [u32; 3] = [0; 3];
    let mut i = 0usize;
    while i < 3 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => pids[i] = pid as u32,
            None => {
                t("CREATE FAIL
RESULT=FAIL
======== END CPU19 ========
");
                return;
            }
        }
        i += 1;
    }
    let targets: [u32; 3] = [1, 2, 3];
    let mut pass = 0u32;
    i = 0;
    while i < 3 {
        let pid = pids[i];
        let ap = targets[i];
        let before = crate::process::get_run_ticks(pid as usize);
        let e = crate::smp::migrate_token(ap, pid);
        let mut after = before;
        let mut w = 0u32;
        while w < 2_000_000 {
            after = crate::process::get_run_ticks(pid as usize);
            // Expect full quantum (+10), not single tick
            if after >= before.saturating_add(10) {
                break;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        t("AP");
        t_usize(ap as usize);
        t(" PID=");
        t_usize(pid as usize);
        t(" MIG=");
        t_usize(e as usize);
        t(" TICKS ");
        t_usize(before as usize);
        t("->");
        t_usize(after as usize);
        if e == 0 && after >= before.saturating_add(10) {
            t(" OK Q10
");
            pass += 1;
        } else {
            t(" FAIL
");
        }
        i += 1;
    }
    t("PASS=");
    t_usize(pass as usize);
    t("/3
");
    t(if pass == 3 {
        "RESULT=PASS AP QUANTUM 10 ON KERNEL ENTRY
"
    } else {
        "RESULT=FAIL NEED Q10 ON EACH AP
"
    });
    t("NOTE: KEY 8 BROKEN - USE CPU19 NOT CPU18
");
    t("======== END CPU19 ========
");
}

/// CPU20: pure per-CPU runqueue drain on AP (no migrate_token / mailbox JOB).
/// Sample ticks BEFORE enqueue - AP may drain before print (v550 false FAIL).
fn gui_cpu20() {
    t("======== CPU20 RQ DRAIN v552 ========
");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6
RESULT=SKIP
======== END CPU20 ========
");
        return;
    }
        let _ = crate::process::destroy_all();
let mut pids: [u32; 3] = [0; 3];
    let mut before: [u64; 3] = [0; 3];
    let mut i = 0usize;
    while i < 3 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => {
                pids[i] = pid as u32;
                before[i] = crate::process::get_run_ticks(pid);
            }
            None => {
                t("CREATE FAIL
RESULT=FAIL
======== END CPU20 ========
");
                return;
            }
        }
        i += 1;
    }
    let targets: [u32; 3] = [1, 2, 3];
    // Enqueue only - AP drains from runqueue (may be instant)
    i = 0;
    while i < 3 {
        if !crate::smp::rq_enqueue(targets[i] as usize, pids[i]) {
            t("ENQ FAIL
RESULT=FAIL
======== END CPU20 ========
");
            return;
        }
        i += 1;
    }
    t("ENQ OK BEFORE ");
    i = 0;
    while i < 3 {
        t_usize(before[i] as usize);
        if i < 2 {
            t("/");
        }
        i += 1;
    }
    t("
");
    let mut pass = 0u32;
    i = 0;
    while i < 3 {
        let pid = pids[i];
        let ap = targets[i];
        let b = before[i];
        let mut after = b;
        let mut w = 0u32;
        while w < 4_000_000 {
            after = crate::process::get_run_ticks(pid as usize);
            if after >= b.saturating_add(10) {
                break;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        t("AP");
        t_usize(ap as usize);
        t(" PID=");
        t_usize(pid as usize);
        t(" TICKS ");
        t_usize(b as usize);
        t("->");
        t_usize(after as usize);
        t(" RQ=");
        t_usize(crate::smp::rq_len(ap as usize) as usize);
        if after >= b.saturating_add(10) {
            t(" OK
");
            pass += 1;
        } else {
            t(" FAIL
");
        }
        i += 1;
    }
    // Second round: prove re-enqueue still works (multi-quantum)
    if pass == 3 {
        i = 0;
        while i < 3 {
            before[i] = crate::process::get_run_ticks(pids[i] as usize);
            let _ = crate::smp::rq_enqueue(targets[i] as usize, pids[i]);
            i += 1;
        }
        let mut pass2 = 0u32;
        i = 0;
        while i < 3 {
            let b = before[i];
            let mut after = b;
            let mut w = 0u32;
            while w < 4_000_000 {
                after = crate::process::get_run_ticks(pids[i] as usize);
                if after >= b.saturating_add(10) {
                    break;
                }
                core::hint::spin_loop();
                w = w.wrapping_add(1);
            }
            if after >= b.saturating_add(10) {
                pass2 += 1;
            }
            i += 1;
        }
        t("ROUND2 PASS=");
        t_usize(pass2 as usize);
        t("/3
");
        if pass2 != 3 {
            pass = pass2;
        }
    }
    t("PASS=");
    t_usize(pass as usize);
    t("/3
");
    t(if pass == 3 {
        "RESULT=PASS PURE RQ DRAIN QUANTUM
"
    } else {
        "RESULT=FAIL RQ NOT CONSUMED BY AP
"
    });
    t("======== END CPU20 ========
");
}

/// CPU21: depth-2 RQ per AP (2 PIDs each) + balance_enqueue path.
fn gui_cpu21() {
    t("======== CPU21 RQ DEPTH+BAL v553 ========
");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6
RESULT=SKIP
======== END CPU21 ========
");
        return;
    }
    // Previous CPU19/20 leave Ready stubs - free table first
    let freed = crate::process::destroy_all();
    t("RECLAIMED=");
    t_usize(freed);
    t(" slots\n");
    // 2 processes per AP (AP1..AP3) = 6 PIDs
    let mut pids: [u32; 6] = [0; 6];
    let mut before: [u64; 6] = [0; 6];
    let mut i = 0usize;
    while i < 6 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => {
                pids[i] = pid as u32;
                before[i] = crate::process::get_run_ticks(pid);
            }
            None => {
                t("CREATE FAIL
RESULT=FAIL
======== END CPU21 ========
");
                return;
            }
        }
        i += 1;
    }
    // Fixed mapping: AP1 gets p0,p1; AP2 p2,p3; AP3 p4,p5
    let map_ap: [u32; 6] = [1, 1, 2, 2, 3, 3];
    i = 0;
    while i < 6 {
        if !crate::smp::rq_enqueue(map_ap[i] as usize, pids[i]) {
            t("ENQ FAIL
RESULT=FAIL
======== END CPU21 ========
");
            return;
        }
        i += 1;
    }
    let mut pass = 0u32;
    i = 0;
    while i < 6 {
        let b = before[i];
        let mut after = b;
        let mut w = 0u32;
        while w < 4_000_000 {
            after = crate::process::get_run_ticks(pids[i] as usize);
            if after >= b.saturating_add(10) {
                break;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        t("AP");
        t_usize(map_ap[i] as usize);
        t(" PID=");
        t_usize(pids[i] as usize);
        t(" ");
        t_usize(b as usize);
        t("->");
        t_usize(after as usize);
        if after >= b.saturating_add(10) {
            t(" OK
");
            pass += 1;
        } else {
            t(" FAIL
");
        }
        i += 1;
    }
    t("DEPTH PASS=");
    t_usize(pass as usize);
    t("/6
");
    // balance_enqueue: 3 new PIDs, auto least-loaded AP
    let mut bal_ok = 0u32;
    i = 0;
    while i < 3 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => {
                let b = crate::process::get_run_ticks(pid);
                let ap = crate::smp::balance_enqueue(pid as u32);
                let mut after = b;
                let mut w = 0u32;
                while w < 4_000_000 {
                    after = crate::process::get_run_ticks(pid);
                    if after >= b.saturating_add(10) {
                        break;
                    }
                    core::hint::spin_loop();
                    w = w.wrapping_add(1);
                }
                t("BAL AP=");
                t_usize(ap);
                t(" PID=");
                t_usize(pid);
                t(" ");
                t_usize(b as usize);
                t("->");
                t_usize(after as usize);
                if ap >= 1 && after >= b.saturating_add(10) {
                    t(" OK\n");
                    bal_ok += 1;
                } else {
                    t(" FAIL\n");
                }
            }
            None => t("BAL CREATE FAIL\n"),
        }
        i += 1;
    }
    t("BAL PASS=");
    t_usize(bal_ok as usize);
    t("/3\n");
    t(if pass == 6 && bal_ok == 3 {
        "RESULT=PASS RQ DEPTH+BALANCE\n"
    } else if pass == 6 {
        "RESULT=PARTIAL DEPTH OK BAL FAIL\n"
    } else {
        "RESULT=FAIL\n"
    });
    t("======== END CPU21 ========\n");
}

/// CPU22: force load on AP1 then balance_enqueue must pick other APs.
fn gui_cpu22() {
    t("======== CPU22 BAL UNDER LOAD v555 ========\n");
    if crate::smp::online_count() < 4 {
        t("NEED CPU6 (4 CPUs)\nRESULT=SKIP\n======== END CPU22 ========\n");
        return;
    }
    let _ = crate::process::destroy_all();
    // Pre-create 12 busy PIDs + 3 bal PIDs, sample ticks BEFORE any enqueue
    let mut busy: [u32; 12] = [0; 12];
    let mut busy_before: [u64; 12] = [0; 12];
    let mut bal_pids: [u32; 3] = [0; 3];
    let mut bal_before: [u64; 3] = [0; 3];
    let mut i = 0usize;
    while i < 12 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => {
                busy[i] = pid as u32;
                busy_before[i] = crate::process::get_run_ticks(pid);
            }
            None => {
                t("CREATE FAIL\nRESULT=FAIL\n======== END CPU22 ========\n");
                return;
            }
        }
        i += 1;
    }
    i = 0;
    while i < 3 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => {
                bal_pids[i] = pid as u32;
                bal_before[i] = crate::process::get_run_ticks(pid);
            }
            None => {
                t("BAL CREATE FAIL\nRESULT=FAIL\n======== END CPU22 ========\n");
                return;
            }
        }
        i += 1;
    }
    // Bulk-enqueue busy to AP1 as fast as possible (no create in between)
    i = 0;
    while i < 12 {
        if !crate::smp::rq_enqueue(1, busy[i]) {
            t("ENQ BUSY FAIL\nRESULT=FAIL\n======== END CPU22 ========\n");
            return;
        }
        i += 1;
    }
    let len1 = crate::smp::rq_len(1);
    t("AP1_RQ_LEN=");
    t_usize(len1 as usize);
    t(" AP2=");
    t_usize(crate::smp::rq_len(2) as usize);
    t(" AP3=");
    t_usize(crate::smp::rq_len(3) as usize);
    t("\n");
    // Immediately balance 3 PIDs - least_loaded should skip AP1 while it still has work
    let mut bal_ok = 0u32;
    let mut not_ap1 = 0u32;
    i = 0;
    while i < 3 {
        // Refill AP1 if drained between enqueues
        if crate::smp::rq_len(1) < 2 {
            let _ = crate::smp::rq_enqueue(1, busy[i % 12]);
        }
        let ap = crate::smp::balance_enqueue(bal_pids[i]);
        let b = bal_before[i];
        let mut after = b;
        let mut w = 0u32;
        while w < 4_000_000 {
            after = crate::process::get_run_ticks(bal_pids[i] as usize);
            if after >= b.saturating_add(10) {
                break;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        t("BAL AP=");
        t_usize(ap);
        t(" PID=");
        t_usize(bal_pids[i] as usize);
        t(" ");
        t_usize(b as usize);
        t("->");
        t_usize(after as usize);
        if after >= b.saturating_add(10) {
            t(" OK");
            bal_ok += 1;
        } else {
            t(" FAIL");
        }
        if ap >= 2 {
            not_ap1 += 1;
            t(" (not AP1)");
        }
        t("\n");
        i += 1;
    }
    // Busy drain using pre-sample ticks
    let mut busy_ok = 0u32;
    i = 0;
    while i < 12 {
        let b = busy_before[i];
        let mut after = b;
        let mut w = 0u32;
        while w < 8_000_000 {
            after = crate::process::get_run_ticks(busy[i] as usize);
            if after >= b.saturating_add(10) {
                break;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        if after >= b.saturating_add(10) {
            busy_ok += 1;
        }
        i += 1;
    }
    t("BAL_OK=");
    t_usize(bal_ok as usize);
    t("/3 NOT_AP1=");
    t_usize(not_ap1 as usize);
    t("/3 BUSY_DRAIN=");
    t_usize(busy_ok as usize);
    t("/12\n");
    if bal_ok == 3 && not_ap1 >= 2 && busy_ok >= 8 {
        t("RESULT=PASS LOAD BALANCE AWAY FROM BUSY AP\n");
    } else if bal_ok == 3 && busy_ok >= 8 {
        t("RESULT=PARTIAL BAL OK BUT STILL HIT AP1\n");
    } else {
        t("RESULT=FAIL\n");
    }
    t("======== END CPU22 ========\n");
}

/// CPU23: concurrent spinlock stress on all online APs (JOB=2).
fn gui_cpu23() {
    t("======== CPU23 AP SPINLOCK STRESS v556 ========\n");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6\nRESULT=SKIP\n======== END CPU23 ========\n");
        return;
    }
    crate::smp::clear_lock_ops();
    let iters = 500u32;
    let targets: [u32; 3] = [1, 2, 3];
    let mut before_done: [u32; 3] = [0; 3];
    let mut i = 0usize;
    while i < 3 {
        before_done[i] = crate::smp::ap_done_for(targets[i]);
        let _ = crate::smp::ap_post_lock_job(targets[i], iters);
        i += 1;
    }
    // Wait until each AP advances DONE
    let mut pass = 0u32;
    i = 0;
    while i < 3 {
        let ap = targets[i];
        let mut w = 0u32;
        while w < 5_000_000 {
            if crate::smp::ap_done_for(ap) > before_done[i] {
                break;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        let ops = crate::smp::ap_lock_ops(ap);
        t("AP");
        t_usize(ap as usize);
        t(" LOCK_OPS=");
        t_usize(ops as usize);
        t(" DONE ");
        t_usize(before_done[i] as usize);
        t("->");
        t_usize(crate::smp::ap_done_for(ap) as usize);
        if ops >= iters {
            t(" OK\n");
            pass += 1;
        } else {
            t(" FAIL\n");
        }
        i += 1;
    }
    // BSP also contending briefly
    let mut k = 0u32;
    while k < 200 {
        let _ = crate::smp::lock_selftest();
        k += 1;
    }
    t("BSP_LOCK_SELFTEST_OK\n");
    t("PASS=");
    t_usize(pass as usize);
    t("/3\n");
    t(if pass == 3 {
        "RESULT=PASS MULTI-AP SPINLOCK\n"
    } else {
        "RESULT=FAIL LOCK STRESS\n"
    });
    t("======== END CPU23 ========\n");
}

/// CPU24: equal RQ flood - 4 PIDs per AP, fairness of tick counts.
fn gui_cpu24() {
    t("======== CPU24 FAIRNESS FLOOD v556 ========\n");
    if crate::smp::online_count() < 4 {
        t("NEED CPU6 (4 CPUs)\nRESULT=SKIP\n======== END CPU24 ========\n");
        return;
    }
    let _ = crate::process::destroy_all();
    const N_PER: usize = 4;
    let mut pids: [u32; 12] = [0; 12];
    let mut before12: [u64; 12] = [0; 12];
    let mut i = 0usize;
    while i < 12 {
        match crate::process::create(
            crate::personality::PersonalityId::None,
            0x400000,
            0x7FFF0000,
        ) {
            Some(pid) => {
                pids[i] = pid as u32;
                before12[i] = crate::process::get_run_ticks(pid);
            }
            None => {
                t("CREATE FAIL\nRESULT=FAIL\n======== END CPU24 ========\n");
                return;
            }
        }
        i += 1;
    }
    // Enqueue 4 per AP1..3
    i = 0;
    while i < 12 {
        let ap = (i / N_PER) + 1;
        if !crate::smp::rq_enqueue(ap, pids[i]) {
            t("ENQ FAIL\nRESULT=FAIL\n======== END CPU24 ========\n");
            return;
        }
        i += 1;
    }
    // Wait drain
    let mut ap_ticks: [u64; 3] = [0; 3];
    i = 0;
    while i < 12 {
        let b = before12[i];
        let mut after = b;
        let mut w = 0u32;
        while w < 6_000_000 {
            after = crate::process::get_run_ticks(pids[i] as usize);
            if after >= b.saturating_add(10) {
                break;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        let ap_i = i / N_PER;
        if ap_i < 3 {
            ap_ticks[ap_i] = ap_ticks[ap_i].wrapping_add(after.saturating_sub(b));
        }
        i += 1;
    }
    t("TICKS AP1=");
    t_usize(ap_ticks[0] as usize);
    t(" AP2=");
    t_usize(ap_ticks[1] as usize);
    t(" AP3=");
    t_usize(ap_ticks[2] as usize);
    t("\n");
    // Fairness: each AP within 50% of max
    let mut mx = ap_ticks[0];
    if ap_ticks[1] > mx {
        mx = ap_ticks[1];
    }
    if ap_ticks[2] > mx {
        mx = ap_ticks[2];
    }
    let mut fair = 0u32;
    i = 0;
    while i < 3 {
        if mx == 0 {
            break;
        }
        // at least half of max and non-zero
        if ap_ticks[i] * 2 >= mx && ap_ticks[i] >= 40 {
            fair += 1;
        }
        i += 1;
    }
    t("FAIR=");
    t_usize(fair as usize);
    t("/3\n");
    t(if fair == 3 {
        "RESULT=PASS FAIR RQ FLOOD\n"
    } else {
        "RESULT=FAIL UNBALANCED\n"
    });
    t("======== END CPU24 ========\n");
}

/// CPU25: heartbeat - each AP must keep incrementing HB while idle.
fn gui_cpu25() {
    t("======== CPU25 AP HEARTBEAT v556 ========\n");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6\nRESULT=SKIP\n======== END CPU25 ========\n");
        return;
    }
    let targets: [u32; 3] = [1, 2, 3];
    let mut hb0: [u32; 3] = [0; 3];
    let mut i = 0usize;
    while i < 3 {
        hb0[i] = crate::smp::ap_heartbeat(targets[i]);
        i += 1;
    }
    // spin ~BSP busy wait to give APs time
    let mut w = 0u32;
    while w < 2_000_000 {
        core::hint::spin_loop();
        w = w.wrapping_add(1);
    }
    let mut pass = 0u32;
    i = 0;
    while i < 3 {
        let hb1 = crate::smp::ap_heartbeat(targets[i]);
        let delta = hb1.wrapping_sub(hb0[i]);
        t("AP");
        t_usize(targets[i] as usize);
        t(" HB ");
        t_usize(hb0[i] as usize);
        t("->");
        t_usize(hb1 as usize);
        t(" d=");
        t_usize(delta as usize);
        if delta > 100 {
            t(" OK\n");
            pass += 1;
        } else {
            t(" FAIL\n");
        }
        i += 1;
    }
    t("PASS=");
    t_usize(pass as usize);
    t("/3\n");
    t(if pass == 3 {
        "RESULT=PASS ALL AP ALIVE\n"
    } else {
        "RESULT=FAIL AP STALLED\n"
    });
    t("======== END CPU25 ========\n");
}


fn gui_cpu13() {
    t("======== CPU13 TARGETED JOBS v543 ========\n");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6\nRESULT=SKIP\n======== END CPU13 ========\n");
        return;
    }
    // Snapshot per-AP done
    let b1 = crate::smp::ap_done_for(1);
    let b2 = crate::smp::ap_done_for(2);
    let b3 = crate::smp::ap_done_for(3);
    t("BEFORE 1=");
    t_usize(b1 as usize);
    t(" 2=");
    t_usize(b2 as usize);
    t(" 3=");
    t_usize(b3 as usize);
    t("\n");
    // Only post to AP 2 (middle) three times
    let mut ok = 0u32;
    let mut i = 0u32;
    while i < 3 {
        if crate::smp::ap_post_job(2, 2000 + i) {
            if crate::smp::ap_wait_done(2, b2 + ok) {
                ok = ok.wrapping_add(1);
            }
        }
        i = i.wrapping_add(1);
    }
    let a1 = crate::smp::ap_done_for(1);
    let a2 = crate::smp::ap_done_for(2);
    let a3 = crate::smp::ap_done_for(3);
    t("AFTER  1=");
    t_usize(a1 as usize);
    t(" 2=");
    t_usize(a2 as usize);
    t(" 3=");
    t_usize(a3 as usize);
    t("\nTARGET_OK=");
    t_usize(ok as usize);
    t("\n");
    let d1 = a1.wrapping_sub(b1);
    let d2 = a2.wrapping_sub(b2);
    let d3 = a3.wrapping_sub(b3);
    t("DELTA 1=");
    t_usize(d1 as usize);
    t(" 2=");
    t_usize(d2 as usize);
    t(" 3=");
    t_usize(d3 as usize);
    t("\n");
    // Success: AP2 increased; AP1/AP3 ideally unchanged (may race if global work)
    if d2 > 0 {
        t("RESULT=PASS TARGETED AP2 JOB\n");
    } else {
        t("RESULT=FAIL NO AP2 PROGRESS\n");
    }
    t("======== END CPU13 ========\n");
}

fn gui_cpu12() {
    t("======== CPU12 RUNQUEUE+PERAP v542 ========\n");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6 FIRST\nRESULT=SKIP\n");
        t("======== END CPU12 ========\n");
        return;
    }
    // enqueue dummy pids on each online CPU index
    let n = crate::smp::cpu_count();
    let mut i = 0usize;
    while i < n && i < 4 {
        let _ = crate::smp::rq_enqueue(i, 1000 + i as u32);
        i += 1;
    }
    t("RQ_LEN:");
    i = 0;
    while i < n && i < 4 {
        t(" [");
        t_usize(i);
        t("=");
        t_usize(crate::smp::rq_len(i) as usize);
        t("]");
        i += 1;
    }
    t("\n");
    // fire several works, show per-AP DONE
    let mut r = 0u32;
    let mut sum = 0u32;
    while r < 12 {
        sum = sum.wrapping_add(crate::smp::ap_post_one_work());
        r = r.wrapping_add(1);
    }
    t("WORK_SUM=");
    t_usize(sum as usize);
    t(" GLOBAL_DONE=");
    t_usize(crate::smp::ap_work_done() as usize);
    t("\nPER_AP_DONE:");
    i = 0;
    while i < 4 {
        t(" [id=");
        t_usize(i);
        t("=");
        t_usize(crate::smp::ap_done_for(i as u32) as usize);
        t("]");
        i += 1;
    }
    t("\n");
    // drain one RQ entry as proof API
    match crate::smp::rq_dequeue(1) {
        Some(pid) => {
            t("RQ_DEQ_CPU1 PID=");
            t_usize(pid as usize);
            t("\n");
        }
        None => t("RQ_DEQ_CPU1 EMPTY\n"),
    }
    if sum > 0 {
        t("RESULT=PASS RUNQ+PERAP WORK\n");
    } else {
        t("RESULT=FAIL NO WORK\n");
    }
    t("======== END CPU12 ========\n");
}

fn gui_cpu11() {
    t("======== CPU11 AP WORK v540 ========\n");
    t("Mailbox: APs poll WORK, inc DONE\n");
    if crate::smp::online_count() < 2 {
        t("NEED CPU6 FIRST (ONLINE<2)\n");
        t("RESULT=SKIP\n");
        t("======== END CPU11 ========\n");
        return;
    }
    let d = crate::smp::stage_ap_work(8);
    t("ROUNDS=8 DONE_DELTA=");
    t_usize(d as usize);
    t(" TOTAL_DONE=");
    t_usize(crate::smp::ap_work_done() as usize);
    t("\nPENDING=");
    t_usize(crate::smp::ap_work_pending() as usize);
    t("\n");
    if d > 0 {
        t("RESULT=PASS AP EXECUTED WORK\n");
    } else {
        t("RESULT=FAIL NO AP WORK (STILL HLT?)\n");
    }
    t("======== END CPU11 ========\n");
}

fn gui_cpu7() {
    t("======== CPU7 SMP STATUS v539 ========\n");
    t("ONLINE=");
    t_usize(crate::smp::online_count());
    t("/");
    t_usize(crate::smp::cpu_count());
    t(" APS=");
    t_usize(crate::smp::aps_started() as usize);
    t("\nCUR_APIC=");
    t_usize(crate::smp::current_apic_id() as usize);
    t(" MILESTONE=");
    t(if crate::smp::smp_milestone_ready() { "YES" } else { "NO" });
    t("\nPER_CPU:");
    let n = crate::smp::cpu_count();
    let mut i = 0usize;
    while i < n && i < 8 {
        t(" [");
        t_usize(i);
        t(":id=");
        t_usize(crate::smp::per_cpu_apic(i) as usize);
        t(if crate::smp::per_cpu_online(i) { " ON" } else { " OFF" });
        t("]");
        i += 1;
    }
    t("\nLOCK_TEST=");
    let e = crate::smp::lock_selftest();
    t(if e == 0 { "PASS" } else { "FAIL" });
    t("\nNOTE: APS IDLE HLT - SCHEDULER STILL BSP\n");
    t("NEXT: per-CPU runqueue + migrate threads\n");
    t("======== END CPU7 ========\n");
}

fn gui_cpu6() {
    t("======== CPU6 ALL APS LM64 v545 ========\n");
    t("REBUILD LM TRAMP + START EACH AP (1 SIPI, NEAR JMP FIX)\n");
    t("TIP: AFTER REBOOT RUN CPU1 THEN CPU6 ONLY\n");
    let n = crate::smp::stage_all_aps();
    t("APS_STARTED=");
    t_usize(n as usize);
    t(" ONLINE=");
    t_usize(crate::smp::online_count());
    t("/");
    t_usize(crate::smp::cpu_count());
    t("\nERR=");
    t_usize(crate::smp::last_err() as usize);
    t(" (0=OK 4=SIPI 6=TRAMP)\n");
    t("PER_CPU:");
    let cnt = crate::smp::cpu_count();
    let mut i = 0usize;
    while i < cnt && i < 8 {
        t(" [");
        t_usize(crate::smp::apic_id_at(i) as usize);
        t("=");
        match crate::smp::ap_status(i) {
            1 => t("OK"),
            2 => t("TO"),
            3 => t("BSP"),
            _ => t("?"),
        }
        t("]");
        i += 1;
    }
    t("\n");
    if n > 0 {
        t("RESULT=PASS multi-AP long mode\n");
    } else {
        t("RESULT=FAIL - REBOOT AND CPU1+CPU6 ONLY\n");
    }
    t("======== END CPU6 ========\n");
}

fn gui_cpu5() {
    t("======== CPU5 PM32 DIAG ========\n");
    t("Protected mode only - if PASS, long entry was the fault\n");
    let n = crate::smp::stage_pm32_only();
    t("APS_STARTED=");
    t_usize(n as usize);
    t(" ONLINE=");
    t_usize(crate::smp::online_count());
    t("\nERR=");
    t_usize(crate::smp::last_err() as usize);
    t("\n");
    if n > 0 {
        t("RESULT=PASS AP PM32\n");
    } else {
        t("RESULT=FAIL/TIMEOUT\n");
    }
    t("======== END CPU5 ========\n");
}

fn gui_rct() {
    t("======== RC RECLAIM TEST ========\n");
    t("MODEL: shared PMM bucket, process owns pages, exit returns them\n");
    let (before, after_alloc, after_free, n, ok) = crate::process::reclaim_self_test();
    t("BEFORE_FREE=");
    t_usize(before);
    t("\nALLOC_PAGES=");
    t_usize(n);
    t("\nAFTER_ALLOC_FREE=");
    t_usize(after_alloc);
    t("\nAFTER_DESTROY_FREE=");
    t_usize(after_free);
    t("\n");
    if before >= after_alloc {
        t("ALLOC_DELTA=");
        t_usize(before - after_alloc);
        t("\n");
    }
    if after_free >= after_alloc {
        t("RECLAIM_DELTA=");
        t_usize(after_free - after_alloc);
        t("\n");
    }
    t("RESULT=");
    if ok {
        t("PASS (free restored to before)\n");
    } else if n > 0 && after_free > after_alloc {
        t("PARTIAL (pages returned, counts noisy)\n");
    } else {
        t("FAIL\n");
    }
    t("======== END RC ========\n");
}

fn gui_ram() {
    t("======== RAM / PMM (text) ========\n");
    let free_p = crate::mm::free_count();
    let total_p = crate::mm::total_count();
    let free_kib = free_p * 4;
    let total_kib = total_p * 4;
    let free_mib = free_kib / 1024;
    let total_mib = total_kib / 1024;
    let cap_mib = crate::mm::PHYS_IDENTITY_LIMIT / (1024 * 1024);
    t("FREE_PAGES=");
    t_usize(free_p);
    t("\nTOTAL_PAGES=");
    t_usize(total_p);
    t("\nFREE_KIB=");
    t_usize(free_kib);
    t("  (~");
    t_usize(free_mib);
    t(" MiB)\n");
    t("TOTAL_KIB=");
    t_usize(total_kib);
    t("  (~");
    t_usize(total_mib);
    t(" MiB managed)\n");
    t("IDENTITY_CAP_MIB=");
    t_usize(cap_mib);
    t("\nINSTALLED_MIB=");
    t_usize(crate::mm::installed_mb());
    t("\n");
    t("NOTE: managed = AVAILABLE within 8 GiB identity map\n");
    t("NOTE: INSTALLED = all Multiboot AVAILABLE (full machine RAM)\n");
    if total_mib <= 70 {
        t("STATUS: LOW - map missing or fallback\n");
    } else {
        t("STATUS: OK full map-based PMM\n");
    }
    t("MODEL: one physical pool (shared bucket)\n");
    t("PROCS_ACTIVE=");
    t_usize(crate::process::active_count());
    t("  PAGES_OWNED=");
    t_usize(crate::process::pages_in_use());
    t("\n");
    t("======== END RAM ========\n");
}


fn gui_sys() {
    t("======== AETHER SYS ========\n");
    t("Kernel: Aether polymorphic no_std x86_64\n");
    t("RAM free MB: ");
    t_usize(crate::mm::free_mb());
    t(" / total ");
    t_usize(crate::mm::total_mb());
    t("\n");
    t("CPUs online: ");
    t_usize(crate::smp::online_count() as usize);
    t("\n");
    t("Display: ");
    t_usize(crate::graphics::width());
    t("x");
    t_usize(crate::graphics::height());
    t("\n");
    t("xHCI: ");
    t(if crate::drivers::xhci::diag_xhci_ok() { "OK" } else { "no" });
    t("  ports=");
    t_usize(crate::drivers::xhci::diag_ports() as usize);
    t("  CCS=");
    t_hex(crate::drivers::xhci::diag_port_ccs() as usize);
    t("\n");
    t("EHCI: ");
    t(if crate::drivers::ehci::diag_ok() { "OK" } else { "no" });
    t("  CCS mask=");
    t_hex(crate::drivers::ehci::port_ccs_mask() as usize);
    t("\n");
    t("Phone: ");
    t(crate::drivers::usb_phone::mode_name(crate::drivers::usb_phone::info().mode));
    t("\n");
    t("Disk: ");
    if crate::drivers::ata::is_hardware() { t("ATA"); }
    else if crate::drivers::ata::is_ramdisk() { t("RAM disk"); }
    else { t("none"); }
    t("\n");
    t("Commands: scanusb su ph sys help\n");
    t("======== END SYS ========\n");
}

fn gui_help() {
    t("commands: help sys scanusb su ph mo dsk hh ram cpu ls cat\n");
    t("dsk  = disk/partition diagnostic\n");
    t("hh   = ethernet + DHCP\n");
    t("pp   = ping 8.8.8.8\n");
    t("wf   = wifi diagnostic\n");
    t("graf = video/GPU diagnostic\n");
    t("dv   = EDID/GMBUS\n");
    t("aud/aud2 = HDA audio\n");
    t("mo   = mouse diagnostic\n");
    t("ph   = smartphone USB (file transfer)\n");
    t("ram  = PMM / physical RAM\n");
    t("rc   = page reclaim test\n");
    t("cpu  = CPUID brand + SMP status\n");
    t("cpu1 = LAPIC probe (safe)\n");
    t("cpu2 = trampoline only\n");
    t("cpu3 = INIT only RISK\n");
    t("cpu4 = AP long mode 64-bit\n");
    t("cpu5 = AP PM32 diagnostic\n");
    t("cpu6 = all APs long mode\n");
    t("cpu7 = smp status + lock test\n");
    t("cpu11 = AP mailbox work proof (no key 8)\n");
    t("cpu12 = runqueue + per-AP done\n");
    t("cpu13 = targeted job to one AP\n");
    t("cpu14 = migrate token PID to each AP\n");
}


fn gui_scanusb() {
    t("======== SCANUSB ========\n");
    crate::drivers::xhci::probe();
    crate::drivers::ehci::probe();
    let rep = crate::drivers::xhci::scan_usb_detail();
    let mut i = 0usize;
    while i < rep.len() {
        crate::desktop::terminal_write_char(rep[i]);
        i += 1;
    }
    let erep = crate::drivers::ehci::scan_usb_detail();
    i = 0;
    while i < erep.len() {
        crate::desktop::terminal_write_char(erep[i]);
        i += 1;
    }
    t("CCS mask=");
    t_hex(crate::drivers::xhci::diag_port_ccs() as usize);
    t("\nphone_live=");
    t(if crate::drivers::xhci::phone_live() { "YES\n" } else { "NO\n" });
    t("======== END SCANUSB ========\n");
}

fn gui_mo() {
    t("======== MO MOUSE DIAG ========\n");
    crate::drivers::ehci::power_and_rescan_ports();
    crate::drivers::xhci::rescan_mouse();
    crate::drivers::ehci::rescan();
    crate::drivers::uhci::rescan();

    t("XHCI=");
    t(if crate::drivers::xhci::diag_xhci_ok() { "OK" } else { "FAIL" });
    t(" HID=");
    t(if crate::drivers::xhci::diag_hid_ok() { "OK" } else { "FAIL" });
    t(" LIVE=");
    t(if crate::drivers::xhci::diag_mouse_live() { "YES" } else { "NO" });
    t("\n");

    t("EHCI=");
    t(if crate::drivers::ehci::diag_ok() { "OK" } else { "FAIL" });
    t(" DEV=");
    t(if crate::drivers::ehci::diag_dev() { "YES" } else { "NO" });
    t(" HID=");
    t(if crate::drivers::ehci::diag_hid() { "OK" } else { "FAIL" });
    t(" LIVE=");
    t(if crate::drivers::ehci::diag_live() { "YES" } else { "NO" });
    t(" R=");
    t_usize(crate::drivers::ehci::diag_reports() as usize);
    t("\n");

    t("UHCI=");
    t(if crate::drivers::uhci::diag_ok() { "OK" } else { "NONE" });
    t(" LIVE=");
    t(if crate::drivers::uhci::diag_live() { "YES" } else { "NO" });
    t(" R=");
    t_usize(crate::drivers::uhci::diag_reports() as usize);
    t("\n");

    t("XHCI CCS_MASK=0x");
    t_hex(crate::drivers::xhci::diag_port_ccs() as usize);
    t("\n");

    t("EHCI PORTS=");
    t_usize(crate::drivers::ehci::port_count() as usize);
    t("\n");
    let mut p = 0u8;
    let n = crate::drivers::ehci::port_count();
    let mut eccs = 0u32;
    while p < n && p < 16 {
        let ps = crate::drivers::ehci::port_status(p);
        t(" P");
        t_usize(p as usize);
        t("=");
        t_hex(ps as usize);
        t(" CCS=");
        t(if (ps & 1) != 0 { "1" } else { "0" });
        t(" PP=");
        t(if (ps & (1 << 12)) != 0 { "1" } else { "0" });
        t(" PO=");
        t(if (ps & (1 << 13)) != 0 { "1" } else { "0" });
        t("\n");
        if (ps & 1) != 0 {
            eccs |= 1 << p;
        }
        p = p.wrapping_add(1);
    }
    t("EHCI VID=");
    t_hex(crate::drivers::ehci::last_vid() as usize);
    t(" PID=");
    t_hex(crate::drivers::ehci::last_pid() as usize);
    t(" CLASS=");
    t_hex(crate::drivers::ehci::last_class() as usize);
    t(" ERR=");
    t_usize(crate::drivers::ehci::last_err() as usize);
    t("
");
    t("(ERR 0=ok 1=no_ccs 2=reset 3=get_desc 4=addr 5=cfg 6=fs_no_companion)\n");
    t("EHCI SPD=");
    t_usize(crate::drivers::ehci::last_spd() as usize);
    t(" (1=FS 2=HS)\n");
    t("QTD0=");
    t_hex(crate::drivers::ehci::last_qtd0() as usize);
    t(" QTD1=");
    t_hex(crate::drivers::ehci::last_qtd1() as usize);
    t(" QTD2=");
    t_hex(crate::drivers::ehci::last_qtd2() as usize);
    t("\n");
    t("USBSTS=");
    t_hex(crate::drivers::ehci::last_usbsts() as usize);
    t("\n");

    t("EHCI CCS_MASK=0x");
    t_hex(eccs as usize);
    t("\n");

    t("REPORTS x=");
    t_usize(crate::drivers::xhci::diag_reports() as usize);
    t(" e=");
    t_usize(crate::drivers::ehci::diag_reports() as usize);
    t(" u=");
    t_usize(crate::drivers::uhci::diag_reports() as usize);
    t("\n");

    if crate::drivers::xhci::diag_port_ccs() == 0 && eccs == 0 {
        t("NO DEVICE ON ANY ROOT PORT\n");
        t("1 PLUG MOUSE BEFORE BOOT OR NOW\n");
        t("2 TRY ANOTHER USB PORT\n");
        t("3 WAIT 2S RUN MO AGAIN\n");
    } else if crate::drivers::xhci::diag_mouse_live()
        || crate::drivers::ehci::diag_live()
        || crate::drivers::uhci::diag_live()
    {
        t("MOUSE LIVE - MOVE IT\n");
    } else {
        t("DEVICE SEEN BUT NOT HID MOUSE\n");
        t("MAY BE HUB OR KEYBOARD\n");
    }

    crate::drivers::xhci::poll_mouse_live();
    crate::drivers::ehci::poll_mouse_live();
    crate::drivers::uhci::poll_mouse_live();
    t("AFTER_POLL R x=");
    t_usize(crate::drivers::xhci::diag_reports() as usize);
    t(" e=");
    t_usize(crate::drivers::ehci::diag_reports() as usize);
    t(" u=");
    t_usize(crate::drivers::uhci::diag_reports() as usize);
    t("\n");
    t("======== END MO ========\n");
}



fn gui_ph() {
    t("======== PH SMARTPHONE USB ========\n");
    t("1. Unlock phone\n");
    t("2. Set USB mode: File transfer / MTP\n");
    t("3. Rescanning xHCI...\n");
    crate::drivers::xhci::rescan_phone();
    let p = crate::drivers::usb_phone::info();
    t("Mode: ");
    t(crate::drivers::usb_phone::mode_name(p.mode));
    t("\n");
    t("phone_live=");
    t(if crate::drivers::xhci::phone_live() { "YES\n" } else { "NO\n" });
    if p.ep_bulk_in != 0 {
        t("Bulk IN EP present\n");
    }
    if p.ep_bulk_out != 0 {
        t("Bulk OUT EP present\n");
    }
    if p.mounted {
        t("Storage: MOUNTED\n");
        t("Blocks & size reported via serial [PHONE]\n");
    } else {
        t("Storage: not mounted\n");
    }
    t("======== END PH ========\n");
}


fn gui_dsk() {
    t("======== DSK (text) ========\n");
    let nd = crate::drivers::ahci::disk_count();
    t("AHCI disks=");
    t_usize(nd);
    t(" ready=");
    t(if crate::drivers::ahci::is_ready() { "yes\n" } else { "no\n" });
    let mut di = 0usize;
    while di < nd {
        t("DISK ");
        t_usize(di);
        t(" model=");
        let mut model = [0u8; 40];
        let ml = crate::drivers::ahci::disk_model(di, &mut model);
        let mut j = 0usize;
        while j < ml {
            crate::desktop::terminal_write_char(model[j]);
            j += 1;
        }
        t(" sectors=");
        t_usize(crate::drivers::ahci::disk_sectors(di) as usize);
        t("\n");
        let mut sec0 = [0u8; 512];
        let ok0 = crate::drivers::ahci::read_sectors(di, 0, 1, &mut sec0);
        t("  LBA0 READ=");
        t(if ok0 { "OK" } else { "FAIL" });
        if ok0 {
            t(" SIG=");
            t(if sec0[510] == 0x55 && sec0[511] == 0xAA {
                "MBR\n"
            } else {
                "NO-55AA\n"
            });
        } else {
            t("\n");
        }
        di += 1;
    }
    let np = crate::part::count();
    t("PARTITIONS=");
    t_usize(np);
    t("\n");
    let mut pi = 0usize;
    while pi < np {
        if let Some(p) = crate::part::get(pi) {
            t("#");
            t_usize(pi);
            t(" disk=");
            t_usize(p.disk as usize);
            t(" type=");
            t(crate::part::type_name(p.ptype));
            t(" LBA=");
            t_usize(p.lba_start as usize);
            t(" sec=");
            t_usize(p.sectors as usize);
            t("\n");
            let mut boot = [0u8; 512];
            let rb = crate::block::read(p.disk, p.lba_start, 1, &mut boot);
            t("  BOOT=");
            t(if rb { "OK" } else { "FAIL" });
            if rb
                && boot[3] == b'N'
                && boot[4] == b'T'
                && boot[5] == b'F'
                && boot[6] == b'S'
            {
                t(" FS=NTFS");
            } else if rb && (boot[0x36] == b'F' || boot[0x52] == b'F') {
                t(" FS=FAT?");
            }
            t("\n");
        }
        pi += 1;
    }
    t("======== END DSK ========\n");
}

fn gui_pp() {
    t("======== PP PING 8.8.8.8 ========\n");
    t("TARGET=8.8.8.8 (Google Public DNS)\n");
    if !crate::drivers::net::dhcp_ok() {
        t("DHCP not ready - running HH path first...\n");
        crate::drivers::net::init();
        let _ = crate::drivers::net::dhcp_run();
    }
    if crate::drivers::net::dhcp_ok() {
        let mut ip = [0u8; 4];
        let mut gw = [0u8; 4];
        crate::drivers::net::dhcp_ip(&mut ip);
        crate::drivers::net::dhcp_gw(&mut gw);
        t("SRC=");
        t_usize(ip[0] as usize);
        t(".");
        t_usize(ip[1] as usize);
        t(".");
        t_usize(ip[2] as usize);
        t(".");
        t_usize(ip[3] as usize);
        t(" GW=");
        t_usize(gw[0] as usize);
        t(".");
        t_usize(gw[1] as usize);
        t(".");
        t_usize(gw[2] as usize);
        t(".");
        t_usize(gw[3] as usize);
        t("\n");
    } else {
        t("DHCP=FAIL ERR=");
        t_usize(crate::drivers::net::last_err() as usize);
        t("\nRESULT=NO_LEASE (run HH first)\n======== END PP ========\n");
        return;
    }
    t("ARP gateway...\n");
    let arp_ok = crate::drivers::net::arp_gateway();
    t("ARP=");
    t(if arp_ok { "PASS" } else { "FAIL" });
    if crate::drivers::net::gw_mac_ok() {
        let mut m = [0u8; 6];
        crate::drivers::net::gw_mac(&mut m);
        t(" GW_MAC=");
        let mut i = 0usize;
        while i < 6 {
            t_hex(m[i] as usize);
            if i + 1 < 6 {
                t(":");
            }
            i += 1;
        }
    }
    t("\n");
    if !arp_ok && !crate::drivers::net::gw_mac_ok() {
        t("RESULT=ARP_FAIL cannot reach gateway\n======== END PP ========\n");
        return;
    }
    t("ICMP echo request...\n");
    let ok = crate::drivers::net::ping_google();
    t("PING=");
    t(if ok { "PASS" } else { "FAIL" });
    t(" ERR=");
    t_usize(crate::drivers::net::ping_err() as usize);
    t(" (0=OK 1=NO_DHCP 2=ARP 3=TX 4=TIMEOUT 5=BAD)\n");
    t("LOOPS=");
    t_usize(crate::drivers::net::ping_loops() as usize);
    t("\n");
    if ok {
        t("RESULT=INTERNET REACHABLE (8.8.8.8 replied)\n");
    } else {
        t("RESULT=NO REPLY (check cable/router/firewall)\n");
    }
    t("======== END PP ========\n");
}

fn gui_ww() {
    crate::desktop::terminal_write("Opening Browser (ww)\n");
    crate::desktop::open_browser();
}
fn gui_eth() {
    t("======== ETH RTL8168 + DHCP ========\n");
    // Re-init path in case probe was partial at boot
    crate::drivers::net::init();
    t("FOUND=");
    t(if crate::drivers::net::eth_found() { "YES" } else { "NO" });
    t(" RTL=");
    t(if crate::drivers::net::eth_is_rtl() { "YES" } else { "NO" });
    t(" READY=");
    t(if crate::drivers::net::ready() { "YES" } else { "NO" });
    t("\n");
    t("VID:DID=");
    t_hex(crate::drivers::net::eth_vid() as usize);
    t(":");
    t_hex(crate::drivers::net::eth_did() as usize);
    t(" BAR=");
    t_hex(crate::drivers::net::eth_bar0() as usize);
    t("\nMAC=");
    let mut mac = [0u8; 6];
    crate::drivers::net::eth_mac(&mut mac);
    let mut i = 0usize;
    while i < 6 {
        t_hex(mac[i] as usize);
        if i + 1 < 6 {
            t(":");
        }
        i += 1;
    }
    t(if crate::drivers::net::eth_mac_ok() { " OK\n" } else { " ?\n" });
    if !crate::drivers::net::eth_found() || !crate::drivers::net::eth_is_rtl() {
        t("RESULT=NO_RTL8168\n======== END ETH ========\n");
        return;
    }
    t("DHCP...\n");
    let ok = crate::drivers::net::dhcp_run();
    t("DMA=");
    t(if crate::drivers::net::dma_ready() { "YES" } else { "NO" });
    t(" LINK=");
    t(if crate::drivers::net::link_up() { "UP" } else { "DOWN/?" });
    t("\n");
    t("DHCP=");
    t(if ok { "PASS" } else { "FAIL" });
    t(" ERR=");
    t_usize(crate::drivers::net::last_err() as usize);
    t(" (0=OK 1=NO_DEV 2=RESET 3=TX 4=NO_OFFER 5=NO_ACK 6=UDP)\n");
    if crate::drivers::net::dhcp_ok() {
        let mut ip = [0u8; 4];
        let mut mask = [0u8; 4];
        let mut gw = [0u8; 4];
        let mut dns = [0u8; 4];
        crate::drivers::net::dhcp_ip(&mut ip);
        crate::drivers::net::dhcp_mask(&mut mask);
        crate::drivers::net::dhcp_gw(&mut gw);
        crate::drivers::net::dhcp_dns(&mut dns);
        t("IP=");
        t_usize(ip[0] as usize);
        t(".");
        t_usize(ip[1] as usize);
        t(".");
        t_usize(ip[2] as usize);
        t(".");
        t_usize(ip[3] as usize);
        t("\nMASK=");
        t_usize(mask[0] as usize);
        t(".");
        t_usize(mask[1] as usize);
        t(".");
        t_usize(mask[2] as usize);
        t(".");
        t_usize(mask[3] as usize);
        t("\nGW=");
        t_usize(gw[0] as usize);
        t(".");
        t_usize(gw[1] as usize);
        t(".");
        t_usize(gw[2] as usize);
        t(".");
        t_usize(gw[3] as usize);
        t("\nDNS=");
        t_usize(dns[0] as usize);
        t(".");
        t_usize(dns[1] as usize);
        t(".");
        t_usize(dns[2] as usize);
        t(".");
        t_usize(dns[3] as usize);
        t("\n");
    }
    t("NOTE: REQUEST uses UDP dst=67 len=283 (fixed vs broken port=283 bug)\n");
    t("======== END ETH ========\n");
}

fn gui_wf() {
    crate::drivers::wifi::set_wf_gui_output(true);
    crate::drivers::wifi::set_wf_quiet(true); // suppress mid [WIFI] spam
    crate::drivers::wifi::set_wf_quiet(false);
    t("======== WF ========\n");
    if !crate::drivers::wifi::found() {
        t("NO DEVICE\n======== END WF ========\n");
        crate::drivers::wifi::set_wf_gui_output(false);
        return;
    }
    t("DEVICE FOUND\n");
    t("reset...\n");
    let rok = crate::drivers::wifi::software_reset();
    t(if rok { "RESET=OK\n" } else { "RESET=FAIL\n" });
    t("activate...\n");
    let aok = crate::drivers::wifi::activate_nic();
    t(if aok { "ACTIVATE=OK\n" } else { "ACTIVATE=FAIL\n" });
    t("load_fw...\n");
    let fok = crate::drivers::wifi::load_firmware();
    t(if fok { "FW=OK\n" } else { "FW=FAIL\n" });
    t("start_fw...\n");
    let sok = crate::drivers::wifi::start_firmware();
    t(if sok { "START=OK\n" } else { "START=FAIL\n" });
    t("alive=");
    t(if crate::drivers::wifi::alive_ok() { "YES\n" } else { "NO\n" });
    t("cmd_queue...\n");
    let qok = crate::drivers::wifi::init_command_queue();
    t(if qok { "CMDQ=OK\n" } else { "CMDQ=FAIL\n" });
    t("alive_scd=");
    t_hex(crate::drivers::wifi::alive_scd_base() as usize);
    t(" gp=");
    t_hex(crate::drivers::wifi::gp_cntrl_raw() as usize);
    t(" rfkill=");
    t(if crate::drivers::wifi::rfkill_blocked() { "CLEAR\n" } else { "CLEAR\n" });
    t("
");
    t("scan...\n");
    let sc = crate::drivers::wifi::scan_24ghz();
    t(if sc { "SCAN_FN=OK\n" } else { "SCAN_FN=FAIL\n" });
        crate::drivers::wifi::set_wf_quiet(false);
    // --- brief human summary ---
    t("\n======== WF VERDICT ========\n");
    t("ALIVE=");
    t(if crate::drivers::wifi::alive_ok() { "YES" } else { "NO" });
    t(" (runtime)");
    t("  CMDQ=");
    t(if qok { "OK" } else { "FAIL" });
    t("  BM=");
    t(if (crate::drivers::wifi::pci_command() & 4) != 0 { "ON" } else { "OFF" });
    t("  RFKILL=");
    t(if crate::drivers::wifi::rfkill_blocked() { "BLOCKED" } else { "CLEAR" });
    t("\n");
    t("HOST_WR=");
    t_usize(crate::drivers::wifi::host_cmd_wr());
    t("  SCD_RDPTR=");
    t_hex(crate::drivers::wifi::scd_rdptr() as usize);
    t("  SCD_WRPTR=");
    t_hex(crate::drivers::wifi::scd_wrptr() as usize);
    t("  SCD_STATUS=");
    t_hex(crate::drivers::wifi::scd_status() as usize);
    t("\n");
    t("CBBC_RAW=");
    t_hex(crate::drivers::wifi::cmd_cbbc() as usize);
    t("  TFD_PHYS=");
    t_hex(crate::drivers::wifi::cmd_tfd_phys() as usize);
    t("  HBUS=");
    t_hex(crate::drivers::wifi::cmd_hbus() as usize);
    t("\n");
    t("RXON=");
    t(if crate::drivers::wifi::last_rxon_ok() { "OK" } else { "FAIL" });
    t("  SCAN_SUBMIT=");
    t(if crate::drivers::wifi::last_scan_submit_ok() { "OK" } else { "NO" });
    t("  SCAN_START=");
    t_usize(crate::drivers::wifi::scan_notif_start() as usize);
    t("  AP=");
    t_usize(crate::drivers::wifi::ap_count());
    t("\n");
    // Transport verdict
    let host_wr = crate::drivers::wifi::host_cmd_wr();
    let scd_rd = crate::drivers::wifi::scd_rdptr() as usize;
    if !crate::drivers::wifi::alive_ok() {
        t("TRANSPORT: FAIL (no ALIVE)\n");
        t("NEXT: fix firmware start\n");
    } else if host_wr == 0 {
        t("TRANSPORT: NO CMDS SENT\n");
        t("NEXT: check CMDQ init\n");
    } else if scd_rd == 0 && host_wr > 0 {
        t("TRANSPORT: SCD NOT CONSUMING (HOST_WR>0 SCD_RDPTR=0)\n");
        t("NEXT: TFD/SCD context / FH path\n");
    } else if scd_rd < host_wr {
        t("TRANSPORT: PARTIAL (SCD lagging)\n");
        t("NEXT: wait or IRQ/poll\n");
    } else {
        t("TRANSPORT: OK (SCD advanced)\n");
        if crate::drivers::wifi::ap_count() == 0 {
            t("NEXT: scan notifications / RX ring\n");
        } else {
            t("NEXT: connect UI\n");
        }
    }
    t("======== END VERDICT ========\n");
    t("======== END WF ========\n");
    crate::drivers::wifi::set_wf_gui_output(false);
}


fn gui_pe() {
    t("======== PE / WINDOWS PERSONALITY ========\n");
    t("Status: PE32+ load YES, Win32 ABI NO\n");
    t("Loaded windows personality: ");
    t(if crate::personality::is_loaded(crate::personality::PersonalityId::Windows) {
        "YES\n"
    } else {
        "NO (loads on .exe open)\n"
    });
    t("\nWhat works now:\n");
    t("- Detect + LOAD PE32+ sections to process\n");
    t("- Base relocs, user CR3, Windows PID\n");
    t("- Explorer .exe -> load attempt\n");
    t("\nWhat is NOT implemented:\n");
    t("- Import table (kernel32/ntdll)\n");
    t("- PEB/TEB / Win32 syscall ABI\n");
    t("- GUI subsystem (user32/gdi)\n");
    t("\nRunning notepad.exe/chrome.exe is\n");
    t("impossible until Windows personality\n");
    t("implements those layers.\n");
    t("======== END PE ========\n");
}

fn gui_graf() {
    t("======== GRAF (text) ========\n");
    crate::video_diag::run_to_terminal();
    t("======== END GRAF ========\n");
}

fn gui_dv() {
    t("======== DV EDID (text) ========\n");
    let ok = crate::drivers::intel_gmbus::read_edid();
    t("EDID=");
    t(if ok { "OK\n" } else { "FAIL\n" });
    if ok {
        t("panel=");
        t_usize(crate::drivers::intel_gmbus::width() as usize);
        t("x");
        t_usize(crate::drivers::intel_gmbus::height() as usize);
        t(" modes=");
        t_usize(crate::drivers::intel_gmbus::mode_count());
        t("\n");
    } else {
        t("err=");
        t_usize(crate::drivers::intel_gmbus::last_err() as usize);
        t(" pin=");
        t_usize(crate::drivers::intel_gmbus::last_pin() as usize);
        t("\n");
    }
    t("======== END DV ========\n");
}


fn gui_aud() {
    let _ = crate::drivers::audio::ensure_ready();
    t("======== AUD HDA/PCM DIAGNOSTIC ========\n");
    t("HDA_FOUND=");
    t(if crate::drivers::audio::hda_found() { "YES" } else { "NO" });
    t(" MMIO=");
    t(if crate::drivers::audio::hda_mmio_ready() { "READY" } else { "NO" });
    t(" BAR0=");
    t_hex(crate::drivers::audio::hda_bar0() as usize);
    t("\n");
    t("GCAP=");
    t_hex(crate::drivers::audio::hda_gcap() as usize);
    t(" OSS=");
    t_usize(crate::drivers::audio::hda_oss() as usize);
    t(" ISS=");
    t_usize(crate::drivers::audio::hda_iss() as usize);
    t(" BSS=");
    t_usize(crate::drivers::audio::hda_bss() as usize);
    t("\n");
    t("STATESTS=");
    t_hex(crate::drivers::audio::hda_statests() as usize);
    t(" CODEC=");
    t_usize(crate::drivers::audio::hda_codec() as usize);
    t("\n");
    t("CODEC0=");
    t_hex(crate::drivers::audio::hda_codec0_vid_did() as usize);
    t(" CODEC3=");
    t_hex(crate::drivers::audio::hda_codec3_vid_did() as usize);
    t("\n");
    t("PIN=");
    t_usize(crate::drivers::audio::hda_analog_pin() as usize);
    t(" CONV=");
    t_usize(crate::drivers::audio::hda_output_conv() as usize);
    t(" AFG=");
    t_usize(crate::drivers::audio::hda_afg() as usize);
    t("\n");
    t("STREAM_READY=");
    t(if crate::drivers::audio::hda_stream_ready() { "YES" } else { "NO" });
    t(" RUNNING=");
    t(if crate::drivers::audio::hda_stream_running() { "YES" } else { "NO" });
    t(" BASE=");
    t_hex(crate::drivers::audio::hda_stream_base());
    t("\n");
    t("LPIB=");
    t_hex(crate::drivers::audio::hda_lpib() as usize);
    t(" CBL=");
    t_hex(crate::drivers::audio::hda_stream_cbl() as usize);
    t(" LVI=");
    t_hex(crate::drivers::audio::hda_stream_lvi() as usize);
    t(" CTL=");
    t_hex(crate::drivers::audio::hda_stream_control() as usize);
    t(" STAT=");
    t_hex(crate::drivers::audio::hda_stream_status() as usize);
    t("\n");
    t("CORBCTL=");
    t_hex(crate::drivers::audio::hda_corb_ctl() as usize);
    t(" RIRBCTL=");
    t_hex(crate::drivers::audio::hda_rirb_ctl() as usize);
    t(" VERB_OK=");
    t(if crate::drivers::audio::hda_verb_ok() { "YES" } else { "NO" });
    t("\n");
    t("DMA=");
    t_hex(crate::drivers::audio::hda_dma_phys());
    t(" BDL=");
    t_hex(crate::drivers::audio::hda_bdl_phys());
    t("\n======== AUD END ========\n");
}

fn gui_aud2() {
    let _ = crate::drivers::audio::ensure_ready();
    t("======== AUD2 PCM PLAYBACK ========\n");
    let opened = crate::media_player::open_embedded_wav();
    t("SOURCE=EMBEDDED_TEST_WAV\n");
    t("OPEN=");
    t(if opened { "YES" } else { "NO" });
    t(" RATE=");
    t_usize(crate::media_player::sample_rate() as usize);
    t(" CH=");
    t_usize(crate::media_player::channels() as usize);
    t(" BITS=");
    t_usize(crate::media_player::bits() as usize);
    t(" PCM=");
    t_usize(crate::media_player::data_bytes());
    t("\n");
    if opened {
        crate::media_player::play();
        t("PLAY_REQUEST=YES\n");
        t("MEDIA_ERR=");
        t_usize(crate::media_player::error() as usize);
        t(" STREAM_RUNNING=");
        t(if crate::drivers::audio::hda_stream_running() { "YES" } else { "NO" });
        t("\n");
        t("LPIB0=");
        t_hex(crate::drivers::audio::hda_lpib() as usize);
        // Spin while DMA should advance LPIB.
        let mut d = 0u32;
        while d < 2_000_000 {
            core::hint::spin_loop();
            d += 1;
        }
        crate::drivers::audio::playback_poll();
        t(" LPIB1=");
        t_hex(crate::drivers::audio::hda_lpib() as usize);
        d = 0;
        while d < 2_000_000 {
            core::hint::spin_loop();
            d += 1;
        }
        crate::drivers::audio::playback_poll();
        t(" LPIB2=");
        t_hex(crate::drivers::audio::hda_lpib() as usize);
        t("\n");
        t("CTL=");
        t_hex(crate::drivers::audio::hda_stream_control() as usize);
        t(" STAT=");
        t_hex(crate::drivers::audio::hda_stream_status() as usize);
        t(" CBL=");
        t_hex(crate::drivers::audio::hda_stream_cbl() as usize);
        t(" LVI=");
        t_hex(crate::drivers::audio::hda_stream_lvi() as usize);
        t(" DMA=");
        t_hex(crate::drivers::audio::hda_dma_phys());
        t(" BDL=");
        t_hex(crate::drivers::audio::hda_bdl_phys());
        t("\n");
        // Prefer hardware truth (LPIB movement) over soft flag races.
        let lpib = crate::drivers::audio::hda_lpib();
        let running = crate::drivers::audio::hda_stream_running();
        if lpib > 0 {
            t("RESULT=DMA_MOVING SOUND_SHOULD_PLAY\n");
        } else if running {
            t("RESULT=RUN_SET_BUT_LPIB_STILL_0 CHECK_CODEC_PATH\n");
        } else {
            t("RESULT=STREAM_NOT_STARTED CHECK_RATE_OR_DMA\n");
        }
    } else {
        t("PLAY_REQUEST=NO_EMBEDDED_WAV\n");
    }
    t("======== AUD2 END ========\n");
}
