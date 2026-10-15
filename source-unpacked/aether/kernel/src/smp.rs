//! Symmetric Multiprocessing (Stage 1): detect CPUs, enable LAPIC, bring APs online.
//! APs: long mode + mailbox work loop. Scheduler still BSP; runqueue/IPI scaffolding ready.
//!
//! Target: Intel Sandy/Ivy Bridge (Fujitsu AH532) - xAPIC at 0xFEE00000.

use crate::mm;
use crate::mm::paging::{self, PAGE_PRESENT, PAGE_WRITE, PAGE_PCD};
use crate::serial;

const MAX_CPUS: usize = 16;
const LAPIC_DEFAULT: usize = 0xFEE0_0000;

// Shared low-memory mailbox for AP trampoline (identity-mapped)
const TRAMP_PHYS: usize = 0x8000;
const SHARED_PHYS: usize = 0x7000;
// Layout at SHARED_PHYS (identity, low mem):
// +0x00: u64 CR3 (AP page tables)
// +0x08: u64 stack top for current AP
// +0x10: u32 online count
// +0x14: u32 magic AML6
// +0x18: u32 global WORK
// +0x1C: u32 global DONE
// +0x20: GDTR (limit+base)
// +0x30: GDT entries
// +0x80: per-AP DONE[8] u32 each (indexed by APIC id & 7)

static mut LAPIC_MMIO: usize = 0;
static mut CPU_COUNT: usize = 1;
static mut ONLINE: usize = 1;
static mut BSP_APIC_ID: u32 = 0;
static mut APIC_IDS: [u32; MAX_CPUS] = [0; MAX_CPUS];
static mut READY: bool = false;
static mut LAST_ERR: u32 = 0; // 0=ok 1=no apic 2=map 3=madt 4=sipi 5=x2apic
static mut LOGICAL_RAW: u32 = 0; // CPUID.1 EBX[23:16] raw (often power-of-2 ID space, NOT core count)
static mut CORES: u32 = 1;       // physical cores (best effort)
static mut THREADS: u32 = 1;     // logical threads (best effort)
static mut DETECT_SRC: u32 = 0;  // 0=cpuid1 1=leaf4 2=leafB 3=capped
static mut AP_STATUS: [u8; MAX_CPUS] = [0; MAX_CPUS]; // 0=n/a 1=ok 2=timeout 3=skip_bsp
static mut APS_STARTED: u32 = 0;

/// Per-CPU control block (filled on bring-up; APs idle in HLT until scheduler).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PerCpu {
    pub apic_id: u32,
    pub online: u8,   // 1 = running (HLT ok)
    pub is_bsp: u8,
    pub _pad: u16,
    pub idle_halts: u64,
}

static mut PER_CPU: [PerCpu; MAX_CPUS] = [PerCpu {
    apic_id: 0,
    online: 0,
    is_bsp: 0,
    _pad: 0,
    idle_halts: 0,
}; MAX_CPUS];

/// Simple test-and-set spinlock for future SMP critical sections.
pub struct SpinLock {
    ticket: core::sync::atomic::AtomicU32,
}

impl SpinLock {
    pub const fn new() -> Self {
        Self {
            ticket: core::sync::atomic::AtomicU32::new(0),
        }
    }
    #[inline]
    pub fn lock(&self) {
        use core::sync::atomic::{AtomicU32, Ordering};
        while self
            .ticket
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }
    #[inline]
    pub fn unlock(&self) {
        use core::sync::atomic::Ordering;
        self.ticket.store(0, Ordering::Release);
    }
    pub fn test(&self) -> bool {
        use core::sync::atomic::Ordering;
        self.ticket
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }
}

static mut GLOBAL_LOCK: SpinLock = SpinLock {
    ticket: core::sync::atomic::AtomicU32::new(0),
};


#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") lo,
        out("edx") hi,
        options(nostack, preserves_flags)
    );
    ((hi as u64) << 32) | (lo as u64)
}

#[inline]
unsafe fn wrmsr(msr: u32, val: u64) {
    let lo = val as u32;
    let hi = (val >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") lo,
        in("edx") hi,
        options(nostack, preserves_flags)
    );
}

#[inline]
unsafe fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let a: u32;
    let b: u32;
    let c: u32;
    let d: u32;
    // LLVM reserves rbx - save/restore via RSI
    core::arch::asm!(
        "xchg rsi, rbx",
        "cpuid",
        "xchg rsi, rbx",
        inout("eax") leaf => a,
        out("rsi") b,
        lateout("ecx") c,
        lateout("edx") d,
        options(nostack, preserves_flags)
    );
    (a, b, c, d)
}

#[inline]
unsafe fn lapic_r(off: usize) -> u32 {
    core::ptr::read_volatile((LAPIC_MMIO + off) as *const u32)
}

#[inline]
unsafe fn lapic_w(off: usize, v: u32) {
    core::ptr::write_volatile((LAPIC_MMIO + off) as *mut u32, v);
}

fn delay_loop(n: u32) {
    let mut i = 0u32;
    while i < n {
        core::hint::spin_loop();
        i = i.wrapping_add(1);
    }
}

/// Parse ACPI MADT for Local APIC entries. Returns number of usable APIC IDs stored.
unsafe fn parse_madt_apic_ids() -> usize {
    // Find RSDP
    let mut rsdp: usize = 0;
    // EBDA
    let ebda_seg = core::ptr::read_volatile(0x40E as *const u16) as usize;
    if ebda_seg != 0 {
        let base = ebda_seg << 4;
        let mut p = base;
        while p + 20 < base + 0x400 {
            if core::ptr::read_volatile(p as *const u32) == 0x20445352
                && core::ptr::read_volatile((p + 4) as *const u32) == 0x20525450
            {
                // "RSD PTR "
                rsdp = p;
                break;
            }
            p += 16;
        }
    }
    if rsdp == 0 {
        let mut p = 0xE0000usize;
        while p + 20 < 0x100000 {
            if core::ptr::read_volatile(p as *const u32) == 0x20445352
                && core::ptr::read_volatile((p + 4) as *const u32) == 0x20525450
            {
                rsdp = p;
                break;
            }
            p += 16;
        }
    }
    if rsdp == 0 {
        return 0;
    }
    let rsdt = core::ptr::read_volatile((rsdp + 16) as *const u32) as usize;
    if rsdt < 0x1000 || rsdt >= 0x4000_0000 {
        return 0;
    }
    let rsdt_len = core::ptr::read_volatile((rsdt + 4) as *const u32) as usize;
    if rsdt_len < 36 || rsdt_len > 0x10000 {
        return 0;
    }
    let n_entries = (rsdt_len - 36) / 4;
    let mut madt = 0usize;
    let mut i = 0usize;
    while i < n_entries {
        let entry = core::ptr::read_volatile((rsdt + 36 + i * 4) as *const u32) as usize;
        if entry >= 0x1000 && entry < 0x4000_0000 {
            // signature "APIC"
            if core::ptr::read_volatile(entry as *const u32) == 0x43495041 {
                madt = entry;
                break;
            }
        }
        i += 1;
    }
    if madt == 0 {
        return 0;
    }
    let madt_len = core::ptr::read_volatile((madt + 4) as *const u32) as usize;
    if madt_len < 44 || madt_len > 0x10000 {
        return 0;
    }
    // Local APIC address at MADT+0x24
    let lapic_addr = core::ptr::read_volatile((madt + 0x24) as *const u32) as usize;
    if lapic_addr != 0 {
        // prefer MADT address if non-zero (usually 0xFEE00000)
        let _ = lapic_addr;
    }
    let mut off = 44usize;
    let mut n = 0usize;
    while off + 2 <= madt_len && n < MAX_CPUS {
        let typ = core::ptr::read_volatile((madt + off) as *const u8);
        let len = core::ptr::read_volatile((madt + off + 1) as *const u8) as usize;
        if len < 2 {
            break;
        }
        if typ == 0 && len >= 8 {
            // Processor Local APIC
            let apic_id = core::ptr::read_volatile((madt + off + 3) as *const u8) as u32;
            let flags = core::ptr::read_volatile((madt + off + 4) as *const u32);
            if (flags & 1) != 0 {
                APIC_IDS[n] = apic_id;
                n += 1;
            }
        }
        off += len;
    }
    n
}

/// Fallback: estimate logical CPU count from CPUID, invent sequential APIC IDs.
unsafe fn fallback_cpu_ids() -> usize {
    let (_a, b, _c, d) = cpuid(1);
    if (d & (1 << 9)) == 0 {
        return 1; // no APIC
    }
    let logical = ((b >> 16) & 0xFF) as usize;
    let n = if logical == 0 { 1 } else if logical > MAX_CPUS { MAX_CPUS } else { logical };
    // BSP APIC id
    let bsp = (b >> 24) & 0xFF;
    BSP_APIC_ID = bsp;
    APIC_IDS[0] = bsp;
    // Common pattern: sequential IDs 0..n-1 or bsp, bsp+1, ...
    let mut i = 1usize;
    while i < n {
        APIC_IDS[i] = bsp.wrapping_add(i as u32);
        i += 1;
    }
    n
}

unsafe fn setup_trampoline(cr3: u64) {
    // Shared state
    let s = SHARED_PHYS as *mut u8;
    core::ptr::write_bytes(s, 0, 0x100);
    core::ptr::write_volatile(SHARED_PHYS as *mut u64, cr3);
    core::ptr::write_volatile((SHARED_PHYS + 0x10) as *mut u32, 1); // online = BSP

    // Minimal GDT at SHARED+0x30: null, code32, data32, code64, data64
    let gdt = (SHARED_PHYS + 0x30) as *mut u64;
    *gdt.add(0) = 0;
    *gdt.add(1) = 0x00CF9A000000FFFF; // 32-bit code
    *gdt.add(2) = 0x00CF92000000FFFF; // 32-bit data
    *gdt.add(3) = 0x00AF9A000000FFFF; // 64-bit code
    *gdt.add(4) = 0x00AF92000000FFFF; // 64-bit data
    // GDTR at SHARED+0x20: limit + base
    let gdtr = (SHARED_PHYS + 0x20) as *mut u8;
    core::ptr::write_unaligned(gdtr as *mut u16, (5 * 8 - 1) as u16);
    core::ptr::write_unaligned((gdtr as usize + 2) as *mut u32, (SHARED_PHYS + 0x30) as u32);

    // Trampoline machine code at 0x8000
    // SIPI starts at CS=0x800 IP=0 → physical 0x8000
    let t = TRAMP_PHYS as *mut u8;
    core::ptr::write_bytes(t, 0x90, 0x200); // NOP pad

    // We assemble a compact real→prot→long trampoline as bytes.
    // See comments for instruction stream.
    // Real-mode entry at 0x8000, then contiguous pmode + long64
    // pmode @ 0x801B, long64 @ 0x805E (computed from layout)
    let code: &[u8] = &[
        // --- real 16-bit @ 0x8000 ---
        0xFA, 0x31, 0xC0, 0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0,
        0x0F, 0x01, 0x16, 0x20, 0x70, // lgdt [0x7020]
        0x0F, 0x20, 0xC0, 0x0C, 0x01, 0x0F, 0x22, 0xC0,
        0xEA, 0x1B, 0x80, 0x08, 0x00, // jmp 0x08:0x801B
        // --- protected 32-bit @ 0x801B ---
        0x66, 0xB8, 0x10, 0x00, 0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0,
        0xA1, 0x00, 0x70, 0x00, 0x00, 0x0F, 0x22, 0xD8, // CR3
        0x0F, 0x20, 0xE0, 0x83, 0xC8, 0x20, 0x0F, 0x22, 0xE0, // PAE
        0xB9, 0x80, 0x00, 0x00, 0xC0, 0x0F, 0x32, // RDMSR EFER
        0x0D, 0x00, 0x01, 0x00, 0x00, 0x0F, 0x30, // LME + WRMSR
        0x0F, 0x20, 0xC0, 0x0D, 0x00, 0x00, 0x00, 0x80, 0x0F, 0x22, 0xC0, // PG
        0x0F, 0x01, 0x15, 0x20, 0x70, 0x00, 0x00, // lgdt
        0x6A, 0x18, // push 0x18 (64-bit CS)
        0x68, 0x5E, 0x80, 0x00, 0x00, // push 0x805E
        0xCB, // retf
        // --- long 64-bit @ 0x805E ---
        0x48, 0x8B, 0x24, 0x25, 0x08, 0x70, 0x00, 0x00, // mov rsp,[0x7008]
        0xF0, 0xFF, 0x05, 0x10, 0x70, 0x00, 0x00, // lock inc [0x7010]
        0xFA, // cli
        0xF4, // hlt
        0xEB, 0xFD, // jmp hlt
    ];
    core::ptr::copy_nonoverlapping(code.as_ptr(), t, code.len());
}

unsafe fn icr_wait_delivery() {
    // bit 12 of ICR low = Delivery Status (1 = pending)
    let mut n = 0u32;
    while n < 100_000 {
        if (lapic_r(0x300) & (1 << 12)) == 0 {
            break;
        }
        n += 1;
    }
}

unsafe fn send_init_only(apic_id: u32) {
    // Clear ESR
    lapic_w(0x280, 0);
    let _ = lapic_r(0x280);
    lapic_w(0x280, 0);
    let _ = lapic_r(0x280);

    // Edge-triggered INIT (Intel P4+: deassert optional; Linux uses level 0xC500/0x8500)
    // Use level-assert / level-deassert matching Linux to be safe on SNB.
    lapic_w(0x310, apic_id << 24);
    lapic_w(0x300, 0x0000_C500); // INIT | LEVEL | ASSERT
    icr_wait_delivery();
    delay_loop(2_000_000);

    lapic_w(0x310, apic_id << 24);
    lapic_w(0x300, 0x0000_8500); // INIT | LEVEL (deassert)
    icr_wait_delivery();
    delay_loop(1_000_000);
}

unsafe fn send_sipi_only(apic_id: u32) {
    let vector = (TRAMP_PHYS >> 12) as u32; // page number 0x08
    lapic_w(0x280, 0);
    let _ = lapic_r(0x280);

    lapic_w(0x310, apic_id << 24);
    lapic_w(0x300, 0x0000_4600 | vector); // Startup IPI
    icr_wait_delivery();
    delay_loop(200_000);

    lapic_w(0x310, apic_id << 24);
    lapic_w(0x300, 0x0000_4600 | vector);
    icr_wait_delivery();
    delay_loop(500_000);
}

unsafe fn send_init_sipi(apic_id: u32) {
    send_init_only(apic_id);
    send_sipi_only(apic_id);
}

/// Install trampoline + shared mailbox only. No IPI.
pub fn stage_trampoline() -> u32 {
    unsafe {
        if LAPIC_MMIO == 0 {
            let e = probe_lapic_only();
            if e != 0 {
                return e;
            }
        }
        let cr3 = paging::read_cr3();
        setup_trampoline(cr3 as u64);
        // Verify trampoline bytes
        let b0 = core::ptr::read_volatile(TRAMP_PHYS as *const u8);
        let b1 = core::ptr::read_volatile((TRAMP_PHYS + 1) as *const u8);
        serial::write_str("[SMP] TRAMP b0=");
        serial::write_hex(b0 as usize);
        serial::write_str(" b1=");
        serial::write_hex(b1 as usize);
        serial::write_str(" CR3=");
        serial::write_hex(cr3);
        serial::write_str("\n");
        if b0 != 0xFA {
            LAST_ERR = 6;
            return 6;
        }
        LAST_ERR = 0;
        0
    }
}

/// INIT only to first non-BSP APIC id. No SIPI. Tests if INIT alone reboots.
pub fn stage_init_only() -> u32 {
    unsafe {
        let e = stage_trampoline();
        if e != 0 {
            return e;
        }
        let my = BSP_APIC_ID;
        let mut target = 0xFFFFFFFFu32;
        let mut i = 0usize;
        while i < CPU_COUNT {
            let id = APIC_IDS[i];
            if id != my {
                target = id;
                break;
            }
            i += 1;
        }
        if target == 0xFFFFFFFF {
            // Fallback: try APIC id 1
            target = my.wrapping_add(1);
        }
        serial::write_str("[SMP] INIT-ONLY target=");
        serial::write_usize(target as usize);
        serial::write_str("\n");
        send_init_only(target);
        // Read ESR after
        let esr = lapic_r(0x280);
        serial::write_str("[SMP] INIT done ESR=");
        serial::write_hex(esr as usize);
        serial::write_str("\n");
        LAST_ERR = 0;
        0
    }
}

/// Real-mode-only trampoline (SIPI smoke test).
unsafe fn setup_trampoline_realmode() {
    let s = SHARED_PHYS as *mut u8;
    core::ptr::write_bytes(s, 0, 0x100);
    core::ptr::write_volatile((SHARED_PHYS + 0x10) as *mut u32, 1u32);
    core::ptr::write_volatile((SHARED_PHYS + 0x14) as *mut u32, 0u32);

    let t = TRAMP_PHYS as *mut u8;
    core::ptr::write_bytes(t, 0x90, 0x100);
    let code: &[u8] = &[
        0xFA,
        0x31, 0xC0,
        0x8E, 0xD8,
        0x8E, 0xC0,
        0x8E, 0xD0,
        0xC7, 0x06, 0x10, 0x70, 0x02, 0x00,
        0xC7, 0x06, 0x14, 0x70, 0xEF, 0xBE,
        0xF4,
        0xEB, 0xFD,
    ];
    core::ptr::copy_nonoverlapping(code.as_ptr(), t, code.len());
}

/// Build minimal identity page tables for AP transition (Linux-style trampoline PGD).
/// Maps only 0..2MiB as one 2MiB huge page - enough for trampoline/stack/shared.
/// Physical layout:
///   0xA000 PML4
///   0xB000 PDPT
///   0xC000 PD
const AP_PML4: usize = 0xA000;
const AP_PDPT: usize = 0xB000;
const AP_PD: usize = 0xC000;


/// Default AP scheduler quantum (ticks credited per JOB / RQ drain).
const AP_QUANTUM: u64 = 10;

/// AP long-mode Rust entry: process JOB/WORK and account_run on the AP itself.
/// JOB credits AP_QUANTUM ticks (real scheduler quantum path).
/// Also drains this CPU's runqueue entries with the same quantum.
/// Per-AP lock-stress completions at SHARED+0x100 (8 u32).
const LOCK_OPS_BASE: usize = SHARED_PHYS + 0x100;
/// Per-AP heartbeat (loop iterations) at SHARED+0x120.
const HB_BASE: usize = SHARED_PHYS + 0x120;

#[no_mangle]
pub extern "C" fn ap_kernel_entry() -> ! {
    loop {
        let apic = {
            let r = unsafe { core::arch::x86_64::__cpuid(1) };
            (r.ebx >> 24) & 7
        };
        let i = apic as usize;
        unsafe {
            // Heartbeat: prove this AP is still spinning
            if i < 8 {
                let hp = (HB_BASE + i * 4) as *mut u32;
                core::ptr::write_volatile(hp, core::ptr::read_volatile(hp).wrapping_add(1));
            }
            let job_p = (SHARED_PHYS + 0xA0 + i * 4) as *mut u32;
            let job = core::ptr::read_volatile(job_p);
            if job != 0 {
                core::ptr::write_volatile(job_p, 0u32);
                let arg = core::ptr::read_volatile((SHARED_PHYS + 0xE0 + i * 4) as *const u32);
                // JOB==2: spinlock stress - arg = iteration count
                if job == 2 {
                    let n = if arg == 0 { 100u32 } else { arg };
                    let mut k = 0u32;
                    while k < n {
                        GLOBAL_LOCK.lock();
                        // critical section: touch shared DONE lightly
                        let d = core::ptr::read_volatile((SHARED_PHYS + 0x1C) as *const u32);
                        core::ptr::write_volatile((SHARED_PHYS + 0x1C) as *mut u32, d);
                        GLOBAL_LOCK.unlock();
                        k = k.wrapping_add(1);
                    }
                    let lp = (LOCK_OPS_BASE + i * 4) as *mut u32;
                    core::ptr::write_volatile(
                        lp,
                        core::ptr::read_volatile(lp).wrapping_add(n),
                    );
                    let dp = (SHARED_PHYS + 0x80 + i * 4) as *mut u32;
                    core::ptr::write_volatile(dp, core::ptr::read_volatile(dp).wrapping_add(1));
                    continue;
                }
                // JOB==1 (default): quantum path
                let pid = arg;
                core::ptr::write_volatile((SHARED_PHYS + 0xC0 + i * 4) as *mut u32, pid);
                crate::process::set_state(pid as usize, crate::process::State::Running);
                crate::process::account_run(pid as usize, AP_QUANTUM);
                crate::process::set_state(pid as usize, crate::process::State::Ready);
                core::ptr::write_volatile(
                    (SHARED_PHYS + 0x1C) as *mut u32,
                    core::ptr::read_volatile((SHARED_PHYS + 0x1C) as *const u32).wrapping_add(1),
                );
                let dp = (SHARED_PHYS + 0x80 + i * 4) as *mut u32;
                core::ptr::write_volatile(dp, core::ptr::read_volatile(dp).wrapping_add(1));
                continue;
            }
            // Drain per-CPU runqueue (BSP enqueued PIDs for this AP)
            if i < MAX_CPUS {
                if let Some(pid) = rq_dequeue(i) {
                    core::ptr::write_volatile((SHARED_PHYS + 0xC0 + i * 4) as *mut u32, pid);
                    crate::process::set_state(pid as usize, crate::process::State::Running);
                    crate::process::account_run(pid as usize, AP_QUANTUM);
                    crate::process::set_state(pid as usize, crate::process::State::Ready);
                    core::ptr::write_volatile(
                        (SHARED_PHYS + 0x1C) as *mut u32,
                        core::ptr::read_volatile((SHARED_PHYS + 0x1C) as *const u32).wrapping_add(1),
                    );
                    let dp = (SHARED_PHYS + 0x80 + i * 4) as *mut u32;
                    core::ptr::write_volatile(dp, core::ptr::read_volatile(dp).wrapping_add(1));
                    continue;
                }
            }
            let work_p = (SHARED_PHYS + 0x18) as *mut u32;
            if core::ptr::read_volatile(work_p) != 0 {
                core::ptr::write_volatile(work_p, 0u32);
                core::ptr::write_volatile(
                    (SHARED_PHYS + 0x1C) as *mut u32,
                    core::ptr::read_volatile((SHARED_PHYS + 0x1C) as *const u32).wrapping_add(1),
                );
                let dp = (SHARED_PHYS + 0x80 + i * 4) as *mut u32;
                core::ptr::write_volatile(dp, core::ptr::read_volatile(dp).wrapping_add(1));
                continue;
            }
        }
        core::hint::spin_loop();
    }
}

unsafe fn build_ap_page_tables() {
    core::ptr::write_bytes(AP_PML4 as *mut u8, 0, 4096);
    core::ptr::write_bytes(AP_PDPT as *mut u8, 0, 4096);
    core::ptr::write_bytes(AP_PD as *mut u8, 0, 4096);

    // Identity-map 0..8MiB as 4x 2MiB huge pages (kernel ~1M + 1.7M image).
    // Present|Write|Huge = 0x83
    let mut i = 0usize;
    while i < 4 {
        let phys = (i as u64) * 0x20_0000u64;
        core::ptr::write_volatile((AP_PD as *mut u64).add(i), phys | 0x83u64);
        i += 1;
    }
    core::ptr::write_volatile(AP_PDPT as *mut u64, (AP_PD as u64) | 0x03);
    core::ptr::write_volatile(AP_PML4 as *mut u64, (AP_PDPT as u64) | 0x03);
}

/// Full AP trampoline: real → prot32 → long64.
/// Uses dedicated AP page tables (not BSP CR3) - matches Linux trampoline_pgd approach.
/// No port 0x92 (A20/reset risk on Fujitsu firmware).
/// Final magic 0x364C4D41; intermediate not visible after reboot.
unsafe fn setup_trampoline_longmode(_bsp_cr3: u64) {
    build_ap_page_tables();

    let s = SHARED_PHYS as *mut u8;
    core::ptr::write_bytes(s, 0, 0x200);
    // CR3 for AP = our minimal tables
    core::ptr::write_volatile(SHARED_PHYS as *mut u64, AP_PML4 as u64);
    core::ptr::write_volatile((SHARED_PHYS + 0x08) as *mut u64, 0x8F000u64);
    core::ptr::write_volatile((SHARED_PHYS + 0x10) as *mut u32, 1u32);
    core::ptr::write_volatile((SHARED_PHYS + 0x14) as *mut u32, 0u32);
    core::ptr::write_volatile((SHARED_PHYS + 0x18) as *mut u32, 0u32); // WORK
    core::ptr::write_volatile((SHARED_PHYS + 0x1C) as *mut u32, 0u32); // DONE
    // AP Rust entry (identity VA==PA under 8MiB map)
    core::ptr::write_volatile((SHARED_PHYS + 0x28) as *mut u64, ap_kernel_entry as usize as u64);
    // per-AP DONE +0x80, JOB +0xA0, LAST_PID +0xC0
    let mut zi = 0usize;
    while zi < 8 {
        core::ptr::write_volatile((SHARED_PHYS + 0x80 + zi * 4) as *mut u32, 0u32);
        core::ptr::write_volatile((SHARED_PHYS + 0xA0 + zi * 4) as *mut u32, 0u32);
        core::ptr::write_volatile((SHARED_PHYS + 0xC0 + zi * 4) as *mut u32, 0u32);
        core::ptr::write_volatile((SHARED_PHYS + 0xE0 + zi * 4) as *mut u32, 0u32);
        zi += 1;
    }

    let gdt = (SHARED_PHYS + 0x30) as *mut u64;
    *gdt.add(0) = 0;
    *gdt.add(1) = 0x00CF9A000000FFFF; // 08 code32
    *gdt.add(2) = 0x00CF92000000FFFF; // 10 data32
    *gdt.add(3) = 0x00AF9A000000FFFF; // 18 code64
    *gdt.add(4) = 0x00CF92000000FFFF; // 20 data

    let gdtr = (SHARED_PHYS + 0x20) as *mut u8;
    core::ptr::write_unaligned(gdtr as *mut u16, 39u16);
    core::ptr::write_unaligned((gdtr as usize + 2) as *mut u32, (SHARED_PHYS + 0x30) as u32);

    let tramp = TRAMP_PHYS as *mut u8;
    core::ptr::write_bytes(tramp, 0x90, 0x300);

    // Offsets: real@8000, pm@8030, lm@8090
    let mut code = [0x90u8; 0x1C0];
    let mut i = 0usize;
    let put = |c: &mut [u8], i: &mut usize, b: &[u8]| {
        for &x in b {
            c[*i] = x;
            *i += 1;
        }
    };

    // REAL @ 0x8000
    put(&mut code, &mut i, &[0xFA]); // cli
    put(&mut code, &mut i, &[0x31, 0xC0]); // xor ax,ax
    put(&mut code, &mut i, &[0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0]); // ds=es=ss=0
    // lgdt [0x7020]
    put(&mut code, &mut i, &[0x0F, 0x01, 0x16, 0x20, 0x70]);
    // mov eax,cr0; or eax,1; mov cr0,eax
    put(&mut code, &mut i, &[0x0F, 0x20, 0xC0, 0x66, 0x83, 0xC8, 0x01, 0x0F, 0x22, 0xC0]);
    // jmp far 0x08:0x8030
    put(&mut code, &mut i, &[0xEA, 0x30, 0x80, 0x08, 0x00]);

    while i < 0x30 {
        code[i] = 0x90;
        i += 1;
    }

    // PM32 @ 0x8030 - Linux order: CR4.PAE → CR3 → EFER.LME → CR0.PG → far ret to 64
    put(&mut code, &mut i, &[0xB8, 0x10, 0x00, 0x00, 0x00]); // mov eax,0x10
    put(&mut code, &mut i, &[0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0, 0x8E, 0xE0, 0x8E, 0xE8]); // ds/es/ss/fs/gs
    // CRITICAL: ESP must be valid before push/retf (PM32-only path had this; LM path missed it → #SS reboot)
    put(&mut code, &mut i, &[0xBC, 0x00, 0xF0, 0x08, 0x00]); // mov esp, 0x8F000
    // mov eax,cr4; or eax,0x20; mov cr4,eax
    put(&mut code, &mut i, &[0x0F, 0x20, 0xE0, 0x83, 0xC8, 0x20, 0x0F, 0x22, 0xE0]);
    // mov eax,[0x7000]; mov cr3,eax
    put(&mut code, &mut i, &[0xA1, 0x00, 0x70, 0x00, 0x00, 0x0F, 0x22, 0xD8]);
    // mov ecx,0xC0000080; rdmsr; or eax,0x100; wrmsr
    put(&mut code, &mut i, &[0xB9, 0x80, 0x00, 0x00, 0xC0, 0x0F, 0x32]);
    put(&mut code, &mut i, &[0x0D, 0x00, 0x01, 0x00, 0x00, 0x0F, 0x30]);
    // mov eax,cr0; or eax,0x80000001; mov cr0,eax  (PG|PE already set)
    put(&mut code, &mut i, &[0x0F, 0x20, 0xC0, 0x0D, 0x01, 0x00, 0x00, 0x80, 0x0F, 0x22, 0xC0]);
    // push 0x18; push 0x8090; retf
    put(&mut code, &mut i, &[0x6A, 0x18, 0x68, 0x90, 0x80, 0x00, 0x00, 0xCB]);

    while i < 0x90 {
        code[i] = 0x90;
        i += 1;
    }

    // LM64 @ 0x8090
    // IMPORTANT: in 64-bit mode ModRM rm=5 is RIP-relative; absolute needs SIB (mod=0 rm=4 sib=0x25).
    put(&mut code, &mut i, &[0xB8, 0x20, 0x00, 0x00, 0x00]); // mov eax,0x20
    put(&mut code, &mut i, &[0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0]); // ds=es=ss
    // mov rsp, qword [0x7008]  (SIB absolute)
    put(&mut code, &mut i, &[0x48, 0x8B, 0x24, 0x25, 0x08, 0x70, 0x00, 0x00]);
    // lock inc dword [0x7010]
    put(&mut code, &mut i, &[0xF0, 0xFF, 0x04, 0x25, 0x10, 0x70, 0x00, 0x00]);
    // mov dword [0x7014], 'AML6'
    put(&mut code, &mut i, &[0xC7, 0x04, 0x25, 0x14, 0x70, 0x00, 0x00, 0x41, 0x4D, 0x4C, 0x36]);
    put(&mut code, &mut i, &[0xFA]); // cli
    // If BSP stored ap_kernel_entry at 0x7028, jump there (Rust AP main).
    // mov rax, [0x7028]; test rax,rax; jz loop_head; jmp rax
    put(&mut code, &mut i, &[0x48, 0xA1, 0x28, 0x70, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // mov rax, [0x7028]
    put(&mut code, &mut i, &[0x48, 0x85, 0xC0]); // test rax,rax
    let jz_loop_at = i;
    put(&mut code, &mut i, &[0x74, 0x00]); // jz loop_head (patch)
    put(&mut code, &mut i, &[0xFF, 0xE0]); // jmp rax

    // ---- idle loop fallback (assembly) ----
    let loop_head = i;
    // patch jz → loop_head
    let jz_rel = (loop_head as isize) - ((jz_loop_at + 2) as isize);
    code[jz_loop_at + 1] = jz_rel as u8;

    // mov eax,1; cpuid; shr ebx,24; and ebx,7  → APIC id low bits in ebx
    put(&mut code, &mut i, &[0xB8, 0x01, 0x00, 0x00, 0x00]);
    put(&mut code, &mut i, &[0x0F, 0xA2]);
    put(&mut code, &mut i, &[0xC1, 0xEB, 0x18]);
    put(&mut code, &mut i, &[0x83, 0xE3, 0x07]);

    // JOB slot = 0x70A0 + ebx*4
    put(&mut code, &mut i, &[0x89, 0xD8]); // mov eax,ebx
    put(&mut code, &mut i, &[0xC1, 0xE0, 0x02]); // shl eax,2
    put(&mut code, &mut i, &[0x05, 0xA0, 0x70, 0x00, 0x00]); // add eax,0x70A0
    put(&mut code, &mut i, &[0x8B, 0x10]); // mov edx,[eax]
    put(&mut code, &mut i, &[0x85, 0xD2]); // test edx,edx
    // jz check_work - patch after we know offset
    let jz_work_at = i;
    put(&mut code, &mut i, &[0x74, 0x00]); // placeholder

    // clear JOB
    put(&mut code, &mut i, &[0xC7, 0x00, 0x00, 0x00, 0x00, 0x00]);
    // edx = PID from 0x70E0+id*4
    put(&mut code, &mut i, &[0x89, 0xD8, 0xC1, 0xE0, 0x02, 0x05, 0xE0, 0x70, 0x00, 0x00]);
    put(&mut code, &mut i, &[0x8B, 0x10]);
    // [0x70C0+id*4] = edx (LAST_PID)
    put(&mut code, &mut i, &[0x89, 0xD8, 0xC1, 0xE0, 0x02, 0x05, 0xC0, 0x70, 0x00, 0x00]);
    put(&mut code, &mut i, &[0x89, 0x10]);
    // lock inc [0x701C] global DONE
    put(&mut code, &mut i, &[0xF0, 0xFF, 0x04, 0x25, 0x1C, 0x70, 0x00, 0x00]);
    // lock inc [0x7080+id*4] per-AP DONE
    put(&mut code, &mut i, &[0x89, 0xD8, 0xC1, 0xE0, 0x02, 0x05, 0x80, 0x70, 0x00, 0x00]);
    put(&mut code, &mut i, &[0xF0, 0xFF, 0x00]);
    // jmp loop_tail (skip work path)
    let jmp_tail_at = i;
    put(&mut code, &mut i, &[0xEB, 0x00]); // placeholder short

    let check_work = i;
    // patch jz → check_work
    let jz_off = (check_work as isize) - ((jz_work_at + 2) as isize);
    code[jz_work_at + 1] = jz_off as u8;

    // WORK path: mov eax,[0x7018]; test; jz loop_tail
    put(&mut code, &mut i, &[0x8B, 0x04, 0x25, 0x18, 0x70, 0x00, 0x00]);
    put(&mut code, &mut i, &[0x85, 0xC0]);
    let jz_tail_at = i;
    put(&mut code, &mut i, &[0x74, 0x00]); // placeholder
    // lock inc global DONE; clear WORK
    put(&mut code, &mut i, &[0xF0, 0xFF, 0x04, 0x25, 0x1C, 0x70, 0x00, 0x00]);
    put(
        &mut code,
        &mut i,
        &[0xC7, 0x04, 0x25, 0x18, 0x70, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    );
    // lock inc per-AP DONE
    put(&mut code, &mut i, &[0x89, 0xD8, 0xC1, 0xE0, 0x02, 0x05, 0x80, 0x70, 0x00, 0x00]);
    put(&mut code, &mut i, &[0xF0, 0xFF, 0x00]);

    let loop_tail = i;
    // patch short jumps to loop_tail
    let jt1 = (loop_tail as isize) - ((jmp_tail_at + 2) as isize);
    code[jmp_tail_at + 1] = jt1 as u8;
    let jt2 = (loop_tail as isize) - ((jz_tail_at + 2) as isize);
    code[jz_tail_at + 1] = jt2 as u8;

    // pause
    put(&mut code, &mut i, &[0xF3, 0x90]);
    // near jmp back to loop_head: E9 rel32
    put(&mut code, &mut i, &[0xE9, 0x00, 0x00, 0x00, 0x00]);
    let after_jmp = i;
    let rel = (loop_head as i32) - (after_jmp as i32);
    let rb = rel.to_le_bytes();
    code[after_jmp - 4] = rb[0];
    code[after_jmp - 3] = rb[1];
    code[after_jmp - 2] = rb[2];
    code[after_jmp - 1] = rb[3];

    core::ptr::copy_nonoverlapping(code.as_ptr(), tramp, code.len());
}

unsafe fn setup_trampoline_pm32() {
    let s = SHARED_PHYS as *mut u8;
    core::ptr::write_bytes(s, 0, 0x200);
    core::ptr::write_volatile((SHARED_PHYS + 0x10) as *mut u32, 1u32);
    core::ptr::write_volatile((SHARED_PHYS + 0x14) as *mut u32, 0u32);
    let gdt = (SHARED_PHYS + 0x30) as *mut u64;
    *gdt.add(0) = 0;
    *gdt.add(1) = 0x00CF9A000000FFFF;
    *gdt.add(2) = 0x00CF92000000FFFF;
    let gdtr = (SHARED_PHYS + 0x20) as *mut u8;
    core::ptr::write_unaligned(gdtr as *mut u16, 23u16);
    core::ptr::write_unaligned((gdtr as usize + 2) as *mut u32, (SHARED_PHYS + 0x30) as u32);
    let tramp = TRAMP_PHYS as *mut u8;
    core::ptr::write_bytes(tramp, 0x90, 0x80);
    let code: &[u8] = &[
        0xFA, 0x31, 0xC0, 0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0,
        0x0F, 0x01, 0x16, 0x20, 0x70,
        0x0F, 0x20, 0xC0, 0x66, 0x83, 0xC8, 0x01, 0x0F, 0x22, 0xC0,
        0xEA, 0x20, 0x80, 0x08, 0x00,
        // @8020
        0xB8, 0x10, 0x00, 0x00, 0x00, 0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0,
        0xBC, 0x00, 0xF0, 0x08, 0x00,
        0xC7, 0x05, 0x10, 0x70, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
        0xC7, 0x05, 0x14, 0x70, 0x00, 0x00, 0x50, 0x4D, 0x33, 0x32,
        0xFA, 0xF4, 0xEB, 0xFD,
    ];
    // Need padding: real code ends before 0x20, then pm at 0x20
    // Rebuild with pad
    let mut buf = [0x90u8; 0x60];
    // real part length until jmp = 9+5+10+5 = 29? count:
    // FA =1, 31C0=2, 8ED88EC08ED0=6, total 9
    // 0F01162070=5, total 14
    // 0F20C0 6683C801 0F22C0 = 3+4+3=10, total 24
    // EA20800800 =5, total 29
    let real: &[u8] = &[
        0xFA, 0x31, 0xC0, 0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0,
        0x0F, 0x01, 0x16, 0x20, 0x70,
        0x0F, 0x20, 0xC0, 0x66, 0x83, 0xC8, 0x01, 0x0F, 0x22, 0xC0,
        0xEA, 0x20, 0x80, 0x08, 0x00,
    ];
    for (j, &b) in real.iter().enumerate() {
        buf[j] = b;
    }
    let pm: &[u8] = &[
        0xB8, 0x10, 0x00, 0x00, 0x00, 0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0,
        0xBC, 0x00, 0xF0, 0x08, 0x00,
        0xC7, 0x05, 0x10, 0x70, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
        0xC7, 0x05, 0x14, 0x70, 0x00, 0x00, 0x50, 0x4D, 0x33, 0x32,
        0xFA, 0xF4, 0xEB, 0xFD,
    ];
    for (j, &b) in pm.iter().enumerate() {
        buf[0x20 + j] = b;
    }
    core::ptr::copy_nonoverlapping(buf.as_ptr(), tramp, buf.len());
}

/// Send only one SIPI first (second SIPI can re-enter mid-transition → triple fault on SNB).
unsafe fn send_sipi_once(apic_id: u32) {
    let vector = (TRAMP_PHYS >> 12) as u32;
    lapic_w(0x280, 0);
    let _ = lapic_r(0x280);
    lapic_w(0x310, apic_id << 24);
    lapic_w(0x300, 0x0000_4600 | vector);
    icr_wait_delivery();
    delay_loop(500_000);
}

pub fn stage_one_ap() -> u32 {
    unsafe {
        if LAPIC_MMIO == 0 {
            let e = probe_lapic_only();
            if e != 0 {
                return e;
            }
        }
        setup_trampoline_longmode(0);
        if core::ptr::read_volatile(TRAMP_PHYS as *const u8) != 0xFA {
            LAST_ERR = 6;
            return 0;
        }

        let my = BSP_APIC_ID;
        let mut target = my.wrapping_add(1);
        let mut i = 0usize;
        while i < CPU_COUNT {
            if APIC_IDS[i] != my {
                target = APIC_IDS[i];
                break;
            }
            i += 1;
        }
        if target == my {
            LAST_ERR = 4;
            return 0;
        }

        serial::write_str("[SMP] CPU4 LM64+PGT target=");
        serial::write_usize(target as usize);
        serial::write_str(" CR3=");
        serial::write_hex(AP_PML4);
        serial::write_str("\n");

        // INIT with longer settle (Intel ~10ms)
        send_init_only(target);
        delay_loop(2_000_000);
        // ONE SIPI only
        send_sipi_once(target);

        let mut wait = 0u32;
        let mut ok = false;
        while wait < 150 {
            delay_loop(300_000);
            let now = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);
            let mag = core::ptr::read_volatile((SHARED_PHYS + 0x14) as *const u32);
            if now >= 2 || mag == 0x364C4D41 {
                ok = true;
                break;
            }
            wait += 1;
        }
        // If not yet, second SIPI (legacy P5 compat) then wait more
        if !ok {
            serial::write_str("[SMP] 2nd SIPI\n");
            send_sipi_once(target);
            wait = 0;
            while wait < 100 {
                delay_loop(300_000);
                let now = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);
                let mag = core::ptr::read_volatile((SHARED_PHYS + 0x14) as *const u32);
                if now >= 2 || mag == 0x364C4D41 {
                    ok = true;
                    break;
                }
                wait += 1;
            }
        }

        let now = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);
        let mag = core::ptr::read_volatile((SHARED_PHYS + 0x14) as *const u32);
        serial::write_str("[SMP] ONLINE=");
        serial::write_usize(now as usize);
        serial::write_str(" MAGIC=");
        serial::write_hex(mag as usize);
        serial::write_str("\n");

        if ok {
            ONLINE = if now >= 2 { now as usize } else { 2 };
            APS_STARTED = 1;
            LAST_ERR = 0;
            serial::write_str("[SMP] CPU4 LM64 OK\n");
            1
        } else {
            LAST_ERR = 4;
            serial::write_str("[SMP] CPU4 LM64 TIMEOUT\n");
            0
        }
    }
}


/// Bring up ALL non-BSP APs (long mode). Same INIT/SIPI path as CPU4 for each.
/// Prefer after reboot: CPU1 then CPU6 (avoid CPU5 PM32 trampoline residue).
pub fn stage_all_aps() -> u32 {
    unsafe {
        if LAPIC_MMIO == 0 {
            let e = probe_lapic_only();
            if e != 0 {
                return 0;
            }
        }
        // Refresh topology + sequential APIC ID guess
        let n = detect_topology(BSP_APIC_ID);
        CPU_COUNT = if n == 0 { 1 } else if n > MAX_CPUS { MAX_CPUS } else { n };
        fill_apic_ids(BSP_APIC_ID, CPU_COUNT);

        // Always rebuild LM trampoline (CPU5 leaves PM32 code at 0x8000)
        setup_trampoline_longmode(0);
        if core::ptr::read_volatile(TRAMP_PHYS as *const u8) != 0xFA {
            LAST_ERR = 6;
            return 0;
        }

        // Fresh online counter (prior CPU4 AP is re-INIT'd below)
        core::ptr::write_volatile((SHARED_PHYS + 0x10) as *mut u32, 1u32);
        core::ptr::write_volatile((SHARED_PHYS + 0x14) as *mut u32, 0u32);
        core::ptr::write_volatile((SHARED_PHYS + 0x18) as *mut u32, 0u32); // WORK
        core::ptr::write_volatile((SHARED_PHYS + 0x1C) as *mut u32, 0u32); // DONE
        // clear per-AP JOB/DONE/LAST/PID slots so CPU14 is not stuck BUSY
        let mut zi = 0usize;
        while zi < 8 {
            core::ptr::write_volatile((SHARED_PHYS + 0x80 + zi * 4) as *mut u32, 0u32);
            core::ptr::write_volatile((SHARED_PHYS + 0xA0 + zi * 4) as *mut u32, 0u32);
            core::ptr::write_volatile((SHARED_PHYS + 0xC0 + zi * 4) as *mut u32, 0u32);
            core::ptr::write_volatile((SHARED_PHYS + 0xE0 + zi * 4) as *mut u32, 0u32);
            zi += 1;
        }

        let my = BSP_APIC_ID;
        let mut started = 0u32;
        let mut idx = 0usize;
        let mut stack_slot = 0usize;

        while idx < CPU_COUNT && idx < MAX_CPUS {
            let id = APIC_IDS[idx];
            if id == my {
                AP_STATUS[idx] = 3;
                idx += 1;
                continue;
            }

            // 8K stacks downward from 0x8F000, stay above trampoline/shared
            let stack_top = (0x8F000usize).saturating_sub(stack_slot * 0x2000) as u64;
            if stack_top < 0x82000 {
                AP_STATUS[idx] = 2;
                idx += 1;
                continue;
            }
            core::ptr::write_volatile((SHARED_PHYS + 0x08) as *mut u64, stack_top);

            let before = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);
            let mag_before = core::ptr::read_volatile((SHARED_PHYS + 0x14) as *const u32);

            serial::write_str("[SMP] CPU6 AP id=");
            serial::write_usize(id as usize);
            serial::write_str(" stk=");
            serial::write_hex(stack_top as usize);
            serial::write_str(" before=");
            serial::write_usize(before as usize);
            serial::write_str("\n");

            // Match CPU4: ONE SIPI only. Second SIPI mid-transition → reboot on SNB.
            send_init_only(id);
            delay_loop(3_000_000); // ~longer settle before SIPI
            send_sipi_once(id);

            let mut wait = 0u32;
            let mut ok = false;
            while wait < 200 {
                delay_loop(300_000);
                let now = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);
                let mag = core::ptr::read_volatile((SHARED_PHYS + 0x14) as *const u32);
                if now > before || (mag == 0x364C4D41 && mag != mag_before) {
                    ok = true;
                    break;
                }
                wait += 1;
            }
            // No second SIPI - SNB triple-fault risk (was CPU6 reboot cause #2)

            let now = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);
            let mag = core::ptr::read_volatile((SHARED_PHYS + 0x14) as *const u32);
            serial::write_str("[SMP] CPU6 after id=");
            serial::write_usize(id as usize);
            serial::write_str(" on=");
            serial::write_usize(now as usize);
            serial::write_str(" mag=");
            serial::write_hex(mag as usize);
            serial::write_str("\n");

            if ok || now > before {
                AP_STATUS[idx] = 1;
                started = started.wrapping_add(1);
                // clear magic so next AP can set it again (still OK if same)
                core::ptr::write_volatile((SHARED_PHYS + 0x14) as *mut u32, 0u32);
            } else {
                AP_STATUS[idx] = 2;
            }
            stack_slot += 1;
            idx += 1;
            // settle before next INIT (give previous AP time to enter idle loop)
            delay_loop(2_000_000);
        }

        let now = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);
        ONLINE = if (now as usize) > 1 {
            now as usize
        } else {
            1usize.saturating_add(started as usize)
        };
        APS_STARTED = started;
        LAST_ERR = if started > 0 { 0 } else { 4 };
        serial::write_str("[SMP] CPU6 done started=");
        serial::write_usize(started as usize);
        serial::write_str(" online=");
        serial::write_usize(ONLINE);
        serial::write_str("\n");
        sync_per_cpu();
        started
    }
}



// --- Per-CPU runqueue (BSP posts; APs execute via mailbox until full migrate) ---
const RQ_CAP: usize = 16;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RqEntry {
    pub pid: u32,
    pub flags: u32,
}

#[repr(C)]
pub struct RunQueue {
    pub head: u32,
    pub tail: u32,
    pub entries: [RqEntry; RQ_CAP],
}

static mut RUNQ_RAW: [u8; core::mem::size_of::<RunQueue>() * MAX_CPUS] = [0u8; core::mem::size_of::<RunQueue>() * MAX_CPUS];

unsafe fn runq_mut(cpu: usize) -> &'static mut RunQueue {
    let base = RUNQ_RAW.as_mut_ptr() as *mut RunQueue;
    &mut *base.add(cpu)
}

unsafe fn runq_ref(cpu: usize) -> &'static RunQueue {
    let base = RUNQ_RAW.as_ptr() as *const RunQueue;
    &*base.add(cpu)
}

pub fn rq_len(cpu: usize) -> u32 {
    unsafe {
        if cpu >= MAX_CPUS {
            return 0;
        }
        let q = runq_ref(cpu);
        q.tail.wrapping_sub(q.head)
    }
}

pub fn rq_enqueue(cpu: usize, pid: u32) -> bool {
    unsafe {
        if cpu >= MAX_CPUS {
            return false;
        }
        let q = runq_mut(cpu);
        if q.tail.wrapping_sub(q.head) as usize >= RQ_CAP {
            return false;
        }
        let slot = (q.tail as usize) % RQ_CAP;
        q.entries[slot] = RqEntry { pid, flags: 1 };
        q.tail = q.tail.wrapping_add(1);
        // Pure runqueue path: do NOT mirror empty JOB (would account_run pid=0).
        // AP polls rq_dequeue after JOB in ap_kernel_entry.
        true
    }
}

/// Enqueue + wake AP via JOB with correct PID (for targeted migrate).
pub fn rq_enqueue_wake(cpu: usize, pid: u32) -> bool {
    if !rq_enqueue(cpu, pid) {
        return false;
    }
    if cpu > 0 && cpu < 8 {
        let _ = ap_post_job(cpu as u32, pid);
    }
    true
}

pub fn rq_dequeue(cpu: usize) -> Option<u32> {
    unsafe {
        if cpu >= MAX_CPUS {
            return None;
        }
        let q = runq_mut(cpu);
        if q.head == q.tail {
            return None;
        }
        let slot = (q.head as usize) % RQ_CAP;
        let pid = q.entries[slot].pid;
        q.entries[slot].flags = 0;
        q.head = q.head.wrapping_add(1);
        Some(pid)
    }
}

/// Least-loaded online AP (skip BSP=0). Returns APIC/cpu index or 0 if none.
pub fn least_loaded_ap() -> usize {
    let n = online_count();
    if n < 2 {
        return 0;
    }
    let mut best = 1usize;
    let mut best_len = rq_len(1);
    let mut c = 2usize;
    while c < n && c < MAX_CPUS {
        let l = rq_len(c);
        if l < best_len {
            best_len = l;
            best = c;
        }
        c += 1;
    }
    best
}

/// BSP helper: place pid on least-loaded AP runqueue.
pub fn balance_enqueue(pid: u32) -> usize {
    let ap = least_loaded_ap();
    if ap == 0 {
        return 0;
    }
    if rq_enqueue(ap, pid) {
        ap
    } else {
        0
    }
}

/// Per-AP DONE counter (APIC id & 7).
pub fn ap_done_for(apic_id: u32) -> u32 {
    unsafe {
        let i = (apic_id & 7) as usize;
        core::ptr::read_volatile((SHARED_PHYS + 0x80 + i * 4) as *const u32)
    }
}

/// Fixed IPI to one AP (vector). Requires AP IDT for real delivery - scaffolding.
pub fn send_ipi_fixed(apic_id: u32, vector: u8) {
    unsafe {
        if LAPIC_MMIO == 0 {
            return;
        }
        lapic_w(0x280, 0);
        let _ = lapic_r(0x280);
        // ICR high: destination
        lapic_w(0x310, apic_id << 24);
        // ICR low: Fixed | edge | vector
        lapic_w(0x300, (vector as u32) & 0xFF);
        icr_wait_delivery();
    }
}

/// Post one global WORK and wait; returns DONE delta (may be >1 if multi-AP race).

/// Post a targeted job to one AP (APIC id). Writes PID; AP stores LAST_PID and incs DONE.
pub fn ap_post_job(apic_id: u32, pid: u32) -> bool {
    unsafe {
        let i = (apic_id & 7) as usize;
        let slot = (SHARED_PHYS + 0xA0 + i * 4) as *mut u32;
        let stage = (SHARED_PHYS + 0xE0 + i * 4) as *mut u32;
        // Stale JOB left by offline AP caused permanent ERR=1 BUSY - force clear.
        core::ptr::write_volatile(slot, 0u32);
        core::ptr::write_volatile(stage, pid); // AP will copy stage -> LAST_PID
        core::ptr::write_volatile(slot, 1u32);
        true
    }
}

/// Post spinlock-stress job (JOB=2). `iters` lock/unlock cycles on that AP.
pub fn ap_post_lock_job(apic_id: u32, iters: u32) -> bool {
    unsafe {
        let i = (apic_id & 7) as usize;
        let slot = (SHARED_PHYS + 0xA0 + i * 4) as *mut u32;
        let stage = (SHARED_PHYS + 0xE0 + i * 4) as *mut u32;
        core::ptr::write_volatile(slot, 0u32);
        core::ptr::write_volatile(stage, if iters == 0 { 100 } else { iters });
        core::ptr::write_volatile(slot, 2u32);
        true
    }
}

pub fn ap_lock_ops(apic_id: u32) -> u32 {
    unsafe {
        let i = (apic_id & 7) as usize;
        core::ptr::read_volatile((LOCK_OPS_BASE + i * 4) as *const u32)
    }
}

pub fn ap_heartbeat(apic_id: u32) -> u32 {
    unsafe {
        let i = (apic_id & 7) as usize;
        core::ptr::read_volatile((HB_BASE + i * 4) as *const u32)
    }
}

pub fn clear_lock_ops() {
    unsafe {
        let mut i = 0usize;
        while i < 8 {
            core::ptr::write_volatile((LOCK_OPS_BASE + i * 4) as *mut u32, 0u32);
            i += 1;
        }
    }
}

pub fn ap_last_pid(apic_id: u32) -> u32 {
    unsafe {
        let i = (apic_id & 7) as usize;
        core::ptr::read_volatile((SHARED_PHYS + 0xC0 + i * 4) as *const u32)
    }
}

/// Wait until per-AP DONE increases (or timeout). Returns true on progress.

/// Simulate thread migrate: post PID to AP, wait DONE, verify LAST_PID matches.
pub fn migrate_token(apic_id: u32, pid: u32) -> u32 {
    // 0=ok 1=post_fail 2=timeout 3=pid_mismatch
    let before = ap_done_for(apic_id);
    if !ap_post_job(apic_id, pid) {
        return 1;
    }
    if !ap_wait_done(apic_id, before) {
        return 2;
    }
    let got = ap_last_pid(apic_id);
    if got != pid {
        return 3;
    }
    0
}

pub fn ap_wait_done(apic_id: u32, before: u32) -> bool {
    unsafe {
        let mut w = 0u32;
        while w < 500_000 {
            if ap_done_for(apic_id) > before {
                return true;
            }
            core::hint::spin_loop();
            w = w.wrapping_add(1);
        }
        false
    }
}

pub fn ap_post_one_work() -> u32 {
    unsafe {
        let done_ptr = (SHARED_PHYS + 0x1C) as *mut u32;
        let work_ptr = (SHARED_PHYS + 0x18) as *mut u32;
        let before = core::ptr::read_volatile(done_ptr);
        core::ptr::write_volatile(work_ptr, 1u32);
        let mut wait = 0u32;
        while wait < 500_000 {
            let now = core::ptr::read_volatile(done_ptr);
            if now > before {
                return now.wrapping_sub(before);
            }
            core::hint::spin_loop();
            wait = wait.wrapping_add(1);
        }
        core::ptr::write_volatile(work_ptr, 0u32);
        0
    }
}

/// Ask online APs to run one unit of work (poll mailbox). Proves APs execute after bring-up.
/// Returns number of work completions observed (DONE delta).
pub fn stage_ap_work(rounds: u32) -> u32 {
    unsafe {
        if ONLINE < 2 {
            LAST_ERR = 4;
            return 0;
        }
        let done_ptr = (SHARED_PHYS + 0x1C) as *mut u32;
        let work_ptr = (SHARED_PHYS + 0x18) as *mut u32;
        let before = core::ptr::read_volatile(done_ptr);
        let mut got = 0u32;
        let mut r = 0u32;
        while r < rounds {
            // post work
            core::ptr::write_volatile(work_ptr, 1u32);
            // wait for any AP to clear work and inc done
            let mut wait = 0u32;
            let target = before.wrapping_add(got).wrapping_add(1);
            while wait < 200_000 {
                let now = core::ptr::read_volatile(done_ptr);
                if now >= target {
                    got = now.wrapping_sub(before);
                    break;
                }
                // help: if work still set, pause
                core::hint::spin_loop();
                wait = wait.wrapping_add(1);
            }
            // ensure work cleared
            if core::ptr::read_volatile(work_ptr) != 0 {
                // timeout this round
                core::ptr::write_volatile(work_ptr, 0u32);
            }
            r = r.wrapping_add(1);
        }
        let after = core::ptr::read_volatile(done_ptr);
        let delta = after.wrapping_sub(before);
        serial::write_str("[SMP] AP_WORK before=");
        serial::write_usize(before as usize);
        serial::write_str(" after=");
        serial::write_usize(after as usize);
        serial::write_str(" delta=");
        serial::write_usize(delta as usize);
        serial::write_str("\n");
        LAST_ERR = if delta > 0 { 0 } else { 4 };
        delta
    }
}

pub fn ap_work_done() -> u32 {
    unsafe { core::ptr::read_volatile((SHARED_PHYS + 0x1C) as *const u32) }
}

pub fn ap_work_pending() -> u32 {
    unsafe { core::ptr::read_volatile((SHARED_PHYS + 0x18) as *const u32) }
}

pub fn stage_pm32_only() -> u32 {
    unsafe {
        if LAPIC_MMIO == 0 {
            let e = probe_lapic_only();
            if e != 0 {
                return e;
            }
        }
        setup_trampoline_pm32();
        let my = BSP_APIC_ID;
        let mut target = my.wrapping_add(1);
        let mut i = 0usize;
        while i < CPU_COUNT {
            if APIC_IDS[i] != my {
                target = APIC_IDS[i];
                break;
            }
            i += 1;
        }
        send_init_only(target);
        delay_loop(1_000_000);
        send_sipi_once(target);
        let mut wait = 0u32;
        while wait < 120 {
            delay_loop(300_000);
            let mag = core::ptr::read_volatile((SHARED_PHYS + 0x14) as *const u32);
            if mag == 0x32334D50 {
                ONLINE = 2;
                APS_STARTED = 1;
                LAST_ERR = 0;
                return 1;
            }
            wait += 1;
        }
        LAST_ERR = 4;
        0
    }
}

/// Accurate topology for Intel (Sandy Bridge era AH532: typically 2c/4t).
/// CPUID.1 EBX[23:16] is often "max APIC ID space" (e.g. 16), NOT enabled CPU count.
unsafe fn detect_topology(bsp_apic: u32) -> usize {
    LOGICAL_RAW = 0;
    CORES = 1;
    THREADS = 1;
    DETECT_SRC = 0;

    let (max_leaf, _, _, _) = cpuid(0);
    let (_a1, b1, _c1, d1) = cpuid(1);
    let raw = (b1 >> 16) & 0xFF;
    LOGICAL_RAW = raw;
    let ht = (d1 & (1 << 28)) != 0;

    // --- Prefer CPUID leaf 0xB (x2APIC / extended topology) ---
    if max_leaf >= 0xB {
        // Level 0 = SMT (threads per core), Level 1 = core
        let mut threads_per_core = 1u32;
        let mut cores_pkg = 1u32;
        let mut level = 0u32;
        while level < 8 {
            let (eax, ebx, ecx, _edx) = {
                let a: u32;
                let b: u32;
                let c: u32;
                let d: u32;
                core::arch::asm!(
                    "xchg rsi, rbx",
                    "cpuid",
                    "xchg rsi, rbx",
                    in("eax") 0xBu32,
                    in("ecx") level,
                    lateout("eax") a,
                    out("rsi") b,
                    lateout("ecx") c,
                    lateout("edx") d,
                    options(nostack, preserves_flags)
                );
                (a, b, c, d)
            };
            let lvl_type = (ecx >> 8) & 0xFF;
            let nlog = ebx & 0xFFFF;
            if lvl_type == 0 {
                break;
            }
            if lvl_type == 1 {
                // SMT
                if nlog > 0 {
                    threads_per_core = nlog;
                }
            } else if lvl_type == 2 {
                // Core
                if nlog > 0 {
                    cores_pkg = nlog;
                }
            }
            level += 1;
        }
        if cores_pkg > 1 || threads_per_core > 1 {
            // leaf B: cores_pkg is often "logical processors at core level" = cores * threads
            // On Intel, level-2 EBX is number of logical processors sharing the cache level
            // Safer: THREADS = cores_pkg if level2 counts logical, else cores * tpc
            let total = if cores_pkg >= threads_per_core {
                cores_pkg
            } else {
                cores_pkg.saturating_mul(threads_per_core)
            };
            let total = if total == 0 { 1 } else { total };
            THREADS = total;
            CORES = if threads_per_core > 1 {
                total / threads_per_core
            } else {
                total
            };
            if CORES == 0 {
                CORES = 1;
            }
            DETECT_SRC = 2;
            fill_apic_ids(bsp_apic, total as usize);
            return total as usize;
        }
    }

    // --- CPUID leaf 4: max cores per package ---
    if max_leaf >= 4 {
        let (eax, _ebx, _ecx, _edx) = {
            let a: u32;
            let b: u32;
            let c: u32;
            let d: u32;
            core::arch::asm!(
                "xchg rsi, rbx",
                "cpuid",
                "xchg rsi, rbx",
                in("eax") 4u32,
                in("ecx") 0u32,
                lateout("eax") a,
                out("rsi") b,
                lateout("ecx") c,
                lateout("edx") d,
                options(nostack, preserves_flags)
            );
            (a, b, c, d)
        };
        let max_cores = ((eax >> 26) & 0x3F) + 1;
        if max_cores >= 1 && max_cores <= 8 {
            CORES = max_cores;
            THREADS = if ht { max_cores * 2 } else { max_cores };
            DETECT_SRC = 1;
            fill_apic_ids(bsp_apic, THREADS as usize);
            return THREADS as usize;
        }
    }

    // --- Fallback: CPUID.1 logical, but cap absurd values ---
    let mut n = if raw == 0 { 1 } else { raw as usize };
    // Sandy Bridge mobile never has 16 real threads; 16 is ID width
    if n > 8 {
        // Typical AH532: dual-core + HT = 4
        if ht {
            CORES = 2;
            THREADS = 4;
            n = 4;
        } else {
            CORES = 2;
            THREADS = 2;
            n = 2;
        }
        DETECT_SRC = 3;
    } else {
        THREADS = n as u32;
        CORES = if ht && n >= 2 { (n as u32) / 2 } else { n as u32 };
        if CORES == 0 {
            CORES = 1;
        }
        DETECT_SRC = 0;
    }
    fill_apic_ids(bsp_apic, n);
    n
}

unsafe fn fill_apic_ids(bsp: u32, n: usize) {
    let n = if n > MAX_CPUS { MAX_CPUS } else if n == 0 { 1 } else { n };
    APIC_IDS[0] = bsp;
    // Intel HT: sibling often APIC_ID xor 1; cores differ by higher bits.
    // Until MADT: sequential from BSP is a usable first guess for dual-core.
    let mut i = 1usize;
    while i < n {
        APIC_IDS[i] = bsp.wrapping_add(i as u32);
        i += 1;
    }
}

pub fn init() {
    // BOOT-SAFE path only. No MADT walk, no LAPIC MMIO, no INIT/SIPI.
    // Those caused post-GRUB reboot on AH532 (ACPI/LAPIC outside identity map
    // or APIC programming fault → triple fault).
    serial::write_str("[SMP] probe (safe)\n");
    unsafe {
        READY = false;
        LAST_ERR = 0;
        ONLINE = 1;
        CPU_COUNT = 1;
        LAPIC_MMIO = 0;

        let (_a, b, _c, d) = cpuid(1);
        if (d & (1 << 9)) == 0 {
            serial::write_str("[SMP] no local APIC in CPUID\n");
            LAST_ERR = 1;
            APIC_IDS[0] = 0;
            READY = true;
            return;
        }
        let bsp = (b >> 24) & 0xFF;
        BSP_APIC_ID = bsp;
        let n = detect_topology(bsp);
        let n = if n == 0 { 1 } else if n > MAX_CPUS { MAX_CPUS } else { n };
        CPU_COUNT = n;
        ONLINE = 1; // only BSP running until explicit bring-up
        READY = true;
        serial::write_str("[SMP] BSP id=");
        serial::write_usize(bsp as usize);
        serial::write_str(" cores=");
        serial::write_usize(CORES as usize);
        serial::write_str(" threads=");
        serial::write_usize(THREADS as usize);
        serial::write_str(" raw_id_space=");
        serial::write_usize(LOGICAL_RAW as usize);
        serial::write_str(" src=");
        serial::write_usize(DETECT_SRC as usize);
        serial::write_str(" online=1 (AP deferred)\n");
    }
}


/// Safe LAPIC probe: MSR + map + read ID. Never sends INIT/SIPI.
/// Returns 0=ok, else error code.
pub fn probe_lapic_only() -> u32 {
    unsafe {
        LAST_ERR = 0;
        let apic_base_msr = rdmsr(0x1B);
        serial::write_str("[SMP] MSR_APIC_BASE=");
        serial::write_hex(apic_base_msr as usize);
        serial::write_str("\n");

        let mut lapic_phys = (apic_base_msr & 0xFFFF_F000) as usize;
        if lapic_phys == 0 {
            lapic_phys = LAPIC_DEFAULT;
        }

        // x2APIC mode bit 10 - MMIO may #GP if set
        let x2 = (apic_base_msr & (1 << 10)) != 0;
        let en = (apic_base_msr & (1 << 11)) != 0;
        serial::write_str("[SMP] APIC_EN=");
        serial::write_str(if en { "1" } else { "0" });
        serial::write_str(" X2APIC=");
        serial::write_str(if x2 { "1" } else { "0" });
        serial::write_str("\n");

        if x2 {
            // Try switch back to xAPIC: clear enable, clear x2, re-enable
            // (some CPUs allow this before full OS takeover)
            wrmsr(0x1B, apic_base_msr & !((1 << 10) | (1 << 11)));
            delay_loop(10000);
            wrmsr(0x1B, (lapic_phys as u64) | (1 << 11));
            let again = rdmsr(0x1B);
            if (again & (1 << 10)) != 0 {
                LAST_ERR = 5; // x2apic stuck
                serial::write_str("[SMP] x2APIC stuck, skip MMIO\n");
                return 5;
            }
        } else if !en {
            wrmsr(0x1B, (lapic_phys as u64) | (1 << 11));
        }

        let cr3 = paging::read_cr3();
        let flags = PAGE_PRESENT | PAGE_WRITE | PAGE_PCD;
        if !paging::map_page(cr3, lapic_phys, lapic_phys, flags) {
            LAST_ERR = 2;
            return 2;
        }
        // Also map with same phys as virt (identity above 1GiB)
        core::arch::asm!("invlpg [{}]", in(reg) lapic_phys, options(nostack, preserves_flags));
        // Full serializing
        core::arch::asm!("mfence", options(nostack, preserves_flags));

        LAPIC_MMIO = lapic_phys;

        // Read-only probe first (no SVR write yet)
        let id_reg = lapic_r(0x20);
        let ver = lapic_r(0x30);
        let svr = lapic_r(0xF0);
        serial::write_str("[SMP] LAPIC_ID_REG=");
        serial::write_hex(id_reg as usize);
        serial::write_str(" VER=");
        serial::write_hex(ver as usize);
        serial::write_str(" SVR=");
        serial::write_hex(svr as usize);
        serial::write_str("\n");

        let my_id = (id_reg >> 24) & 0xFF;
        BSP_APIC_ID = my_id;
        APIC_IDS[0] = my_id;

        // Soft enable APIC (may be needed later for SIPI; optional here)
        lapic_w(0xF0, (svr & !0xFF) | 0x100 | 0xFF);
        let svr2 = lapic_r(0xF0);
        serial::write_str("[SMP] SVR_AFTER=");
        serial::write_hex(svr2 as usize);
        serial::write_str(" BSP_ID=");
        serial::write_usize(my_id as usize);
        serial::write_str("\n");

        LAST_ERR = 0;
        0
    }
}

/// Explicit AP bring-up (optional). Not called at boot.
/// Returns number of APs that reached trampoline (0 if none).
pub fn try_bringup_aps() -> u32 {
    unsafe {
        APS_STARTED = 0;
        let mut i = 0usize;
        while i < MAX_CPUS {
            AP_STATUS[i] = 0;
            i += 1;
        }
        if CPU_COUNT <= 1 {
            LAST_ERR = 0;
            return 0;
        }

        let apic_base_msr = rdmsr(0x1B);
        let mut lapic_phys = (apic_base_msr & 0xFFFF_F000) as usize;
        if lapic_phys == 0 {
            lapic_phys = LAPIC_DEFAULT;
        }
        if (apic_base_msr & (1 << 11)) == 0 {
            wrmsr(0x1B, apic_base_msr | (1 << 11));
        }
        let cr3 = paging::read_cr3();
        let flags = PAGE_PRESENT | PAGE_WRITE | PAGE_PCD;
        if !paging::map_page(cr3, lapic_phys, lapic_phys, flags) {
            LAST_ERR = 2;
            return 0;
        }
        core::arch::asm!("invlpg [{}]", in(reg) lapic_phys, options(nostack, preserves_flags));
        LAPIC_MMIO = lapic_phys;

        // Spurious vector + software enable
        let svr = lapic_r(0xF0);
        lapic_w(0xF0, (svr & !0xFF) | 0x100 | 0xFF);

        let my_id = (lapic_r(0x20) >> 24) & 0xFF;
        BSP_APIC_ID = my_id;
        APIC_IDS[0] = my_id;

        // Rebuild sequential APIC IDs from real BSP id
        fill_apic_ids(my_id, CPU_COUNT);

        setup_trampoline(cr3 as u64);
        core::ptr::write_volatile((SHARED_PHYS + 0x10) as *mut u32, 1);

        serial::write_str("[SMP] CPUGO LAPIC=");
        serial::write_hex(lapic_phys);
        serial::write_str(" BSP=");
        serial::write_usize(my_id as usize);
        serial::write_str("\n");

        let mut started = 0u32;
        let mut idx = 0usize;
        while idx < CPU_COUNT {
            let id = APIC_IDS[idx];
            if id == my_id {
                AP_STATUS[idx] = 3; // bsp skip
                idx += 1;
                continue;
            }

            let mut stack_base = 0usize;
            let mut pg = 0usize;
            while pg < 4 {
                if let Some(p) = mm::alloc_page() {
                    if stack_base == 0 {
                        stack_base = p;
                    }
                    pg += 1;
                } else {
                    break;
                }
            }
            if stack_base == 0 {
                AP_STATUS[idx] = 2;
                LAST_ERR = 4;
                break;
            }
            let stack_top = (stack_base + pg * 4096) as u64;
            core::ptr::write_volatile((SHARED_PHYS + 0x08) as *mut u64, stack_top);
            let before = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);

            serial::write_str("[SMP] INIT+SIPI APIC=");
            serial::write_usize(id as usize);
            serial::write_str("\n");
            send_init_sipi(id);

            let mut wait = 0u32;
            let mut ok = false;
            while wait < 40 {
                delay_loop(400_000);
                let now = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32);
                if now > before {
                    ok = true;
                    break;
                }
                wait += 1;
            }
            if ok {
                AP_STATUS[idx] = 1;
                started = started.wrapping_add(1);
                serial::write_str("[SMP] AP OK id=");
                serial::write_usize(id as usize);
                serial::write_str("\n");
            } else {
                AP_STATUS[idx] = 2;
                LAST_ERR = 4;
                serial::write_str("[SMP] AP TIMEOUT id=");
                serial::write_usize(id as usize);
                serial::write_str("\n");
            }
            idx += 1;
        }

        let online = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32) as usize;
        ONLINE = if online > 0 { online } else { 1 };
        APS_STARTED = started;
        if started > 0 {
            LAST_ERR = 0;
        }
        started
    }
}

pub fn ap_status(i: usize) -> u8 {
    unsafe {
        if i < MAX_CPUS {
            AP_STATUS[i]
        } else {
            0
        }
    }
}

pub fn aps_started() -> u32 {
    unsafe { APS_STARTED }
}


/// Refresh PER_CPU[] from APIC_IDS + AP_STATUS after bring-up.
pub fn sync_per_cpu() {
    unsafe {
        let mut i = 0usize;
        while i < MAX_CPUS {
            PER_CPU[i] = PerCpu {
                apic_id: APIC_IDS[i],
                online: if i == 0 {
                    1
                } else if i < CPU_COUNT && AP_STATUS[i] == 1 {
                    1
                } else if i < CPU_COUNT && AP_STATUS[i] == 3 {
                    1
                } else {
                    0
                },
                is_bsp: if i == 0 || APIC_IDS[i] == BSP_APIC_ID {
                    1
                } else {
                    0
                },
                _pad: 0,
                idle_halts: 0,
            };
            i += 1;
        }
        // BSP always online
        if CPU_COUNT > 0 {
            PER_CPU[0].online = 1;
            PER_CPU[0].is_bsp = 1;
            PER_CPU[0].apic_id = BSP_APIC_ID;
        }
    }
}

/// Current CPU's APIC ID via local APIC (or BSP if MMIO not ready).
pub fn current_apic_id() -> u32 {
    unsafe {
        if LAPIC_MMIO != 0 {
            (lapic_r(0x20) >> 24) & 0xFF
        } else {
            BSP_APIC_ID
        }
    }
}

pub fn per_cpu_online(i: usize) -> bool {
    unsafe {
        if i >= MAX_CPUS {
            false
        } else {
            PER_CPU[i].online != 0
        }
    }
}

pub fn per_cpu_apic(i: usize) -> u32 {
    unsafe {
        if i >= MAX_CPUS {
            0
        } else {
            PER_CPU[i].apic_id
        }
    }
}

/// BSP-only smoke test: acquire/release global lock.
pub fn lock_selftest() -> u32 {
    unsafe {
        GLOBAL_LOCK.lock();
        // critical section no-op
        GLOBAL_LOCK.unlock();
        if GLOBAL_LOCK.test() {
            GLOBAL_LOCK.unlock();
            0 // ok
        } else {
            1 // failed re-acquire
        }
    }
}

pub fn smp_milestone_ready() -> bool {
    unsafe { ONLINE >= 2 && APS_STARTED >= 1 }
}

pub fn cpu_count() -> usize {
    unsafe { CPU_COUNT }
}
pub fn cores() -> u32 {
    unsafe { CORES }
}
pub fn threads() -> u32 {
    unsafe { THREADS }
}
pub fn logical_raw() -> u32 {
    unsafe { LOGICAL_RAW }
}
pub fn detect_src() -> u32 {
    unsafe { DETECT_SRC }
}
pub fn online_count() -> usize {
    unsafe {
        if READY {
            let v = core::ptr::read_volatile((SHARED_PHYS + 0x10) as *const u32) as usize;
            if v > ONLINE {
                ONLINE = v;
            }
        }
        ONLINE
    }
}
pub fn bsp_apic_id() -> u32 {
    unsafe { BSP_APIC_ID }
}
pub fn ready() -> bool {
    unsafe { READY }
}
pub fn last_err() -> u32 {
    unsafe { LAST_ERR }
}
pub fn apic_id_at(i: usize) -> u32 {
    unsafe {
        if i < MAX_CPUS {
            APIC_IDS[i]
        } else {
            0
        }
    }
}
pub fn lapic_base() -> usize {
    unsafe { LAPIC_MMIO }
}
