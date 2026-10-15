//! x86-64 page tables

use crate::mm;

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_PRESENT: u64 = 1 << 0;
pub const PAGE_WRITE: u64 = 1 << 1;
pub const PAGE_USER: u64 = 1 << 2;
pub const PAGE_ACCESSED: u64 = 1 << 5;
pub const PAGE_HUGE: u64 = 1 << 7;
pub const PAGE_PWT: u64 = 1 << 3; // write-through / PAT index bit0
pub const PAGE_PCD: u64 = 1 << 4; // cache disable / PAT index bit1
/// PTE PAT bit for 4K pages (bit 7) - selects high PAT entries when set with PWT/PCD
pub const PAGE_PAT: u64 = 1 << 7;
/// Write-Combining via PAT index 1 (PWT=1, PCD=0, PAT=0) after init_pat_wc().
/// Safe default if PAT not programmed: behaves as WT (still better than full WB for FB).
pub const PAGE_WC: u64 = PAGE_PWT;

static mut PAT_WC_READY: bool = false;


#[repr(align(4096))]
pub struct PageTable {
    pub entries: [u64; 512],
}

impl PageTable {
    pub const fn new() -> Self {
        PageTable { entries: [0; 512] }
    }
    pub fn clear(&mut self) {
        for e in self.entries.iter_mut() { *e = 0; }
    }
    pub fn set(&mut self, index: usize, phys: u64, flags: u64) {
        if index < 512 {
            self.entries[index] = (phys & !0xFFF) | flags;
        }
    }
    pub fn get(&self, index: usize) -> u64 {
        if index < 512 { self.entries[index] } else { 0 }
    }
    /// Ensure USER (and optionally WRITE) bits are set on an existing entry
    pub fn or_flags(&mut self, index: usize, flags: u64) {
        if index < 512 && self.entries[index] & PAGE_PRESENT != 0 {
            self.entries[index] |= flags;
        }
    }
}

pub fn alloc_table() -> Option<usize> {
    mm::alloc_page()
}

pub unsafe fn identity_map_2mb(pml4_phys: usize, n_2mb: usize) -> bool {
    // Map n_2mb consecutive 2MiB pages starting at phys 0 (up to 512 per PD / 1 GiB).
    let gib = (n_2mb + 511) / 512;
    extend_identity_gib(pml4_phys, if gib < 1 { 1 } else { gib })
}

/// Identity-map the first `gib` GiB with 2 MiB pages (max 512 GiB via one PDPT).
/// Boot maps 1 GiB; call this after PMM init to cover full managed window (8 GiB).
pub unsafe fn extend_identity_gib(pml4_phys: usize, gib: usize) -> bool {
    let gib = if gib > 512 { 512 } else if gib == 0 { 1 } else { gib };
    let pml4 = &mut *(pml4_phys as *mut PageTable);
    let mut pdpt_phys = pml4.get(0) & !0xFFF;
    if pdpt_phys == 0 {
        pdpt_phys = match alloc_table() {
            Some(p) => p as u64,
            None => return false,
        };
        let pdpt = &mut *(pdpt_phys as *mut PageTable);
        pdpt.clear();
        pml4.set(0, pdpt_phys, PAGE_PRESENT | PAGE_WRITE);
    }
    let pdpt = &mut *(pdpt_phys as *mut PageTable);
    let mut g = 0usize;
    while g < gib {
        let mut pd_phys = pdpt.get(g) & !0xFFF;
        if pd_phys == 0 {
            pd_phys = match alloc_table() {
                Some(p) => p as u64,
                None => return false,
            };
            let pd = &mut *(pd_phys as *mut PageTable);
            pd.clear();
            pdpt.set(g, pd_phys, PAGE_PRESENT | PAGE_WRITE);
        }
        let pd = &mut *(pd_phys as *mut PageTable);
        let mut i = 0usize;
        while i < 512 {
            // 2 MiB page index across all GiB
            let page2m = (g * 512 + i) as u64;
            let phys = page2m << 21;
            // Present | Write | Huge (PS)
            pd.set(i, phys, PAGE_PRESENT | PAGE_WRITE | PAGE_HUGE);
            i += 1;
        }
        g += 1;
    }
    // Reload CR3 to flush TLB for new mappings
    let cr3 = read_cr3();
    core::arch::asm!("mov cr3, {0}", in(reg) cr3, options(nostack, preserves_flags));
    true
}

/// Map a 4K page. If flags contain USER, force USER on ALL intermediate entries.
pub unsafe fn map_page(pml4_phys: usize, virt: usize, phys: usize, flags: u64) -> bool {
    let pml4 = &mut *(pml4_phys as *mut PageTable);
    let pml4_idx = (virt >> 39) & 0x1FF;
    let pdpt_idx = (virt >> 30) & 0x1FF;
    let pd_idx   = (virt >> 21) & 0x1FF;
    let pt_idx   = (virt >> 12) & 0x1FF;

    let user = flags & PAGE_USER;
    let inter_flags = PAGE_PRESENT | PAGE_WRITE | user; // intermediate always writable for kernel updates

    // PML4 → PDPT
    let mut pdpt_phys = pml4.get(pml4_idx) & !0xFFF;
    if pdpt_phys == 0 {
        pdpt_phys = match alloc_table() { Some(p) => p as u64, None => return false };
        let t = &mut *(pdpt_phys as *mut PageTable);
        t.clear();
        pml4.set(pml4_idx, pdpt_phys, inter_flags);
    } else {
        // CRITICAL: add USER to existing entry so user-mode walk succeeds
        pml4.or_flags(pml4_idx, user);
    }
    let pdpt = &mut *(pdpt_phys as *mut PageTable);

    // PDPT → PD
    let mut pd_phys = pdpt.get(pdpt_idx) & !0xFFF;
    if pd_phys == 0 {
        pd_phys = match alloc_table() { Some(p) => p as u64, None => return false };
        let t = &mut *(pd_phys as *mut PageTable);
        t.clear();
        pdpt.set(pdpt_idx, pd_phys, inter_flags);
    } else {
        pdpt.or_flags(pdpt_idx, user);
    }
    let pd = &mut *(pd_phys as *mut PageTable);

    // If a 2MiB huge page is mapped, split it into 512x4K so we can apply UC/PCD etc.
    if pd.get(pd_idx) & PAGE_HUGE != 0 {
        let huge_entry = pd.get(pd_idx);
        let huge_phys = huge_entry & !0x1F_FFFF; // 2MiB aligned
        // Preserve present/write from huge; drop HUGE; per-page flags applied below
        let base_flags = (huge_entry & (PAGE_PRESENT | PAGE_WRITE | PAGE_USER)) | PAGE_ACCESSED;
        let pt_phys = match alloc_table() { Some(p) => p as u64, None => return false };
        let pt = &mut *(pt_phys as *mut PageTable);
        pt.clear();
        let mut i = 0u64;
        while i < 512 {
            pt.set(i as usize, huge_phys + i * 4096, base_flags);
            i += 1;
        }
        // Replace huge PD entry with PT pointer (no PAGE_HUGE)
        pd.set(pd_idx, pt_phys, inter_flags);
    }

    // PD → PT
    let mut pt_phys = pd.get(pd_idx) & !0xFFF;
    if pt_phys == 0 {
        pt_phys = match alloc_table() { Some(p) => p as u64, None => return false };
        let t = &mut *(pt_phys as *mut PageTable);
        t.clear();
        pd.set(pd_idx, pt_phys, inter_flags);
    } else {
        pd.or_flags(pd_idx, user);
    }
    let pt = &mut *(pt_phys as *mut PageTable);

    pt.set(pt_idx, phys as u64, flags | PAGE_ACCESSED);
    // Invalidate TLB for this page
    core::arch::asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags));
    true
}

/// New PML4 for a process: distinct CR3, shares kernel PDPT trees (no USER on 2MiB kernel identity).
/// User PT_LOAD pages are mapped only via map_page(..., PAGE_USER) into this PML4.
pub unsafe fn create_user_pml4() -> Option<usize> {
    let kcr3 = read_cr3();
    let new_phys = alloc_table()?;
    let kpml4 = &*(kcr3 as *const PageTable);
    let upml4 = &mut *(new_phys as *mut PageTable);
    upml4.clear();
    // Shallow-copy: share kernel page directory trees so kernel code/data remain reachable
    // after CR3 switch (needed for int 0x80 → kernel). Kernel 2MiB identity entries have
    // PAGE_PRESENT|PAGE_WRITE only - no PAGE_USER → Ring3 cannot R/W them.
    let mut i = 0usize;
    while i < 512 {
        upml4.entries[i] = kpml4.entries[i];
        i += 1;
    }
    Some(new_phys)
}

pub unsafe fn load_cr3(pml4_phys: usize) {
    core::arch::asm!("mov cr3, {}", in(reg) pml4_phys, options(nostack, preserves_flags));
}

pub unsafe fn read_cr3() -> usize {
    let v: usize;
    core::arch::asm!("mov {}, cr3", out(reg) v, options(nostack, preserves_flags));
    v
}

static mut KERNEL_CR3: usize = 0;

pub fn set_kernel_cr3(c: usize) {
    unsafe { KERNEL_CR3 = c; }
}
pub fn kernel_cr3() -> usize {
    unsafe { if KERNEL_CR3 != 0 { KERNEL_CR3 } else { read_cr3() } }
}
pub unsafe fn load_kernel_cr3() {
    let c = kernel_cr3();
    if c != 0 {
        load_cr3(c);
    }
}

pub unsafe fn debug_walk(pml4_phys: usize, virt: usize) {
    use crate::serial;
    let pml4 = &*(pml4_phys as *const PageTable);
    let i4 = (virt >> 39) & 0x1FF;
    let i3 = (virt >> 30) & 0x1FF;
    let i2 = (virt >> 21) & 0x1FF;
    let i1 = (virt >> 12) & 0x1FF;
    serial::write_str("  walk virt=");
    serial::write_usize(virt);
    let e4 = pml4.get(i4);
    serial::write_str(" pml4e=");
    serial::write_usize(e4 as usize);
    if e4 & PAGE_USER != 0 { serial::write_str("U"); }
    let pdpt_p = (e4 & !0xFFF) as usize;
    if pdpt_p == 0 { serial::write_str(" NO-PDPT\n"); return; }
    let pdpt = &*(pdpt_p as *const PageTable);
    let e3 = pdpt.get(i3);
    serial::write_str(" pdpte=");
    serial::write_usize(e3 as usize);
    if e3 & PAGE_USER != 0 { serial::write_str("U"); }
    let pd_p = (e3 & !0xFFF) as usize;
    if pd_p == 0 { serial::write_str(" NO-PD\n"); return; }
    let pd = &*(pd_p as *const PageTable);
    let e2 = pd.get(i2);
    serial::write_str(" pde=");
    serial::write_usize(e2 as usize);
    if e2 & PAGE_USER != 0 { serial::write_str("U"); }
    if e2 & PAGE_HUGE != 0 { serial::write_str(" HUGE\n"); return; }
    let pt_p = (e2 & !0xFFF) as usize;
    if pt_p == 0 { serial::write_str(" NO-PT\n"); return; }
    let pt = &*(pt_p as *const PageTable);
    let e1 = pt.get(i1);
    serial::write_str(" pte=");
    serial::write_usize(e1 as usize);
    if e1 & PAGE_USER != 0 { serial::write_str("U"); }
    serial::write_str("\n");
}


/// Program IA32_PAT so entry 1 = Write-Combining (type 0x01).
/// Default PAT after reset: 0=WB,1=WT,2=UC-,3=UC,... - we set PA1=WC.
/// PTE with PWT=1,PCD=0,PAT=0 selects PA1 → WC.
pub unsafe fn init_pat_wc() {
    let mut lo: u32;
    let mut hi: u32;
    core::arch::asm!(
        "mov ecx, 0x277",
        "rdmsr",
        out("eax") lo,
        out("edx") hi,
        out("ecx") _,
        options(nostack, preserves_flags)
    );
    // Clear PA1 (bits 8..15 of lo) and set to WC = 0x01
    lo = (lo & !0xFF00) | 0x0100;
    core::arch::asm!(
        "mov ecx, 0x277",
        "wrmsr",
        in("eax") lo,
        in("edx") hi,
        in("ecx") 0x277u32,
        options(nostack, preserves_flags)
    );
    core::arch::asm!("wbinvd", options(nostack, preserves_flags));
    PAT_WC_READY = true;
    crate::serial::write_str("[PAT] PA1=WC (PTE PWT selects WC)\n");
}

pub fn pat_wc_ready() -> bool {
    unsafe { PAT_WC_READY }
}

/// Recommended flags for framebuffer / aperture mappings.
pub fn fb_map_flags() -> u64 {
    PAGE_PRESENT | PAGE_WRITE | PAGE_WC
}
