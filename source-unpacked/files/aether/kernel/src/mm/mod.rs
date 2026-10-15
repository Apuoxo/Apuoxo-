//! Physical Memory Manager + paging re-export
//!
//! Bitmap uses absolute physical page numbers.
//! Boot identity-maps the first 8 GiB (boot.s: 8× PD of 512×2MiB pages).
//! PMM manages all AVAILABLE pages from Multiboot mmap within that window.
//! Installed RAM above 8 GiB is reported but not managed until map expands.

use core::sync::atomic::{AtomicUsize, Ordering};

pub mod paging;

const PAGE_SIZE: usize = 4096;
/// Physical 0 .. 8 GiB (matches boot.s 8×512×2MiB identity map).
const MAX_PAGES: usize = 2_097_152; // 8 GiB / 4 KiB
pub const PHYS_IDENTITY_LIMIT: usize = MAX_PAGES * PAGE_SIZE; // 8 GiB

static mut BITMAP: [u64; MAX_PAGES / 64] = [0; MAX_PAGES / 64];
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static FREE: AtomicUsize = AtomicUsize::new(0);
static START_PAGE: AtomicUsize = AtomicUsize::new(0);
static END_PAGE: AtomicUsize = AtomicUsize::new(0);
/// Sum of all Multiboot AVAILABLE regions (may exceed identity limit).
static INSTALLED_BYTES: AtomicUsize = AtomicUsize::new(0);

fn read_u32(addr: usize) -> u32 {
    unsafe { core::ptr::read_unaligned(addr as *const u32) }
}

fn read_u64(addr: usize) -> u64 {
    unsafe { core::ptr::read_unaligned(addr as *const u64) }
}

unsafe fn mark_free(page: usize) {
    if page >= MAX_PAGES {
        return;
    }
    let idx = page / 64;
    let bit = page % 64;
    if (BITMAP[idx] & (1u64 << bit)) == 0 {
        BITMAP[idx] |= 1u64 << bit;
        FREE.fetch_add(1, Ordering::SeqCst);
        TOTAL.fetch_add(1, Ordering::SeqCst);
    }
}

/// Contiguous fallback (legacy).
pub fn init(start: usize, size: usize) {
    let start_page = start / PAGE_SIZE;
    let mut count = size / PAGE_SIZE;
    if start_page >= MAX_PAGES {
        START_PAGE.store(start_page, Ordering::SeqCst);
        END_PAGE.store(start_page, Ordering::SeqCst);
        TOTAL.store(0, Ordering::SeqCst);
        FREE.store(0, Ordering::SeqCst);
        return;
    }
    if start_page + count > MAX_PAGES {
        count = MAX_PAGES - start_page;
    }
    TOTAL.store(0, Ordering::SeqCst);
    FREE.store(0, Ordering::SeqCst);
    unsafe {
        let mut i = 0usize;
        while i < count {
            mark_free(start_page + i);
            i += 1;
        }
    }
    START_PAGE.store(start_page, Ordering::SeqCst);
    END_PAGE.store(start_page + count, Ordering::SeqCst);
}

/// Multiboot2 memory map (tag 6) → free AVAILABLE pages in [pmm_start, 8GiB).
/// Also records full installed AVAILABLE bytes (even above identity limit).
pub fn init_from_mbi(mbi: usize, pmm_start: usize) -> usize {
    START_PAGE.store(pmm_start / PAGE_SIZE, Ordering::SeqCst);
    END_PAGE.store(pmm_start / PAGE_SIZE, Ordering::SeqCst);
    TOTAL.store(0, Ordering::SeqCst);
    FREE.store(0, Ordering::SeqCst);
    INSTALLED_BYTES.store(0, Ordering::SeqCst);

    if mbi < 0x1000 {
        let span = PHYS_IDENTITY_LIMIT.saturating_sub(pmm_start);
        init(pmm_start, span);
        INSTALLED_BYTES.store(span, Ordering::SeqCst);
        return FREE.load(Ordering::SeqCst);
    }

    let total = read_u32(mbi) as usize;
    if total < 16 || total > 0x100_000 {
        let span = PHYS_IDENTITY_LIMIT.saturating_sub(pmm_start);
        init(pmm_start, span);
        INSTALLED_BYTES.store(span, Ordering::SeqCst);
        return FREE.load(Ordering::SeqCst);
    }

    let mut found_map = false;
    let limit = PHYS_IDENTITY_LIMIT;
    let start_floor = pmm_start;
    let mut installed: usize = 0;

    let mut off = 8usize;
    while off + 8 <= total {
        let tag_type = read_u32(mbi + off);
        let tag_size = read_u32(mbi + off + 4) as usize;
        if tag_size < 8 || off + tag_size > total {
            break;
        }
        if tag_type == 0 {
            break;
        }

        if tag_type == 6 && tag_size >= 16 {
            found_map = true;
            let entry_size = read_u32(mbi + off + 8) as usize;
            if entry_size >= 20 {
                let mut eoff = off + 16;
                while eoff + entry_size <= off + tag_size {
                    let base = read_u64(mbi + eoff) as usize;
                    let length = read_u64(mbi + eoff + 8) as usize;
                    let ty = read_u32(mbi + eoff + 16);
                    if ty == 1 && length > 0 {
                        installed = installed.saturating_add(length);
                        let mut a = base;
                        let mut b = base.saturating_add(length);
                        if a < start_floor {
                            a = start_floor;
                        }
                        if b > limit {
                            b = limit;
                        }
                        if b > a {
                            let page0 = (a + PAGE_SIZE - 1) / PAGE_SIZE;
                            let page1 = b / PAGE_SIZE;
                            let mut p = page0;
                            unsafe {
                                while p < page1 && p < MAX_PAGES {
                                    mark_free(p);
                                    p += 1;
                                }
                            }
                            let end_p = END_PAGE.load(Ordering::SeqCst);
                            if page1 > end_p {
                                END_PAGE.store(page1.min(MAX_PAGES), Ordering::SeqCst);
                            }
                        }
                    }
                    eoff += entry_size;
                }
            }
        }

        off = (off + tag_size + 7) & !7;
    }

    INSTALLED_BYTES.store(installed, Ordering::SeqCst);

    if !found_map || FREE.load(Ordering::SeqCst) == 0 {
        let max_from_start = limit.saturating_sub(start_floor);
        init(pmm_start, max_from_start);
        if installed == 0 {
            INSTALLED_BYTES.store(max_from_start, Ordering::SeqCst);
        }
        return FREE.load(Ordering::SeqCst);
    }

    FREE.load(Ordering::SeqCst)
}

pub fn alloc_page() -> Option<usize> {
    if FREE.load(Ordering::SeqCst) == 0 {
        return None;
    }
    unsafe {
        let start_page = START_PAGE.load(Ordering::SeqCst);
        let end_page = END_PAGE.load(Ordering::SeqCst);
        let first_idx = start_page / 64;
        let end_idx = (end_page + 63) / 64;
        let mut idx = first_idx;
        while idx < end_idx && idx < BITMAP.len() {
            let word = BITMAP[idx];
            if word != 0 {
                let bit = word.trailing_zeros() as usize;
                BITMAP[idx] &= !(1u64 << bit);
                FREE.fetch_sub(1, Ordering::SeqCst);
                return Some((idx * 64 + bit) * PAGE_SIZE);
            }
            idx += 1;
        }
    }
    None
}

pub fn free_page(addr: usize) {
    let page = addr / PAGE_SIZE;
    if page >= MAX_PAGES {
        return;
    }
    let idx = page / 64;
    let bit = page % 64;
    unsafe {
        if (BITMAP[idx] & (1u64 << bit)) == 0 {
            BITMAP[idx] |= 1u64 << bit;
            FREE.fetch_add(1, Ordering::SeqCst);
        }
    }
}

pub fn free_count() -> usize {
    FREE.load(Ordering::SeqCst)
}

pub fn total_mb() -> usize {
    (total_count() * 4096) / (1024 * 1024)
}
pub fn free_mb() -> usize {
    (free_count() * 4096) / (1024 * 1024)
}
/// Installed AVAILABLE RAM from Multiboot (bytes), may exceed managed window.
pub fn installed_bytes() -> usize {
    INSTALLED_BYTES.load(Ordering::SeqCst)
}
pub fn installed_mb() -> usize {
    installed_bytes() / (1024 * 1024)
}
pub fn total_count() -> usize {
    TOTAL.load(Ordering::SeqCst)
}

pub fn alloc_pages(count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    if count == 1 {
        return alloc_page();
    }
    unsafe {
        let start_page = START_PAGE.load(Ordering::SeqCst);
        let end_page = END_PAGE.load(Ordering::SeqCst);
        if end_page <= start_page || count > end_page - start_page {
            return None;
        }
        let mut page = start_page;
        while page + count <= end_page && page + count <= MAX_PAGES {
            let mut ok = true;
            let mut i = 0usize;
            while i < count {
                let p = page + i;
                let idx = p / 64;
                let bit = p % 64;
                if idx >= BITMAP.len() || (BITMAP[idx] & (1u64 << bit)) == 0 {
                    ok = false;
                    break;
                }
                i += 1;
            }
            if ok {
                i = 0;
                while i < count {
                    let p = page + i;
                    let idx = p / 64;
                    let bit = p % 64;
                    BITMAP[idx] &= !(1u64 << bit);
                    i += 1;
                }
                FREE.fetch_sub(count, Ordering::SeqCst);
                return Some(page * PAGE_SIZE);
            }
            page += 1;
        }
    }
    None
}

pub fn zero_pages(phys: usize, count: usize) {
    unsafe {
        let p = phys as *mut u8;
        let mut i = 0usize;
        while i < count * PAGE_SIZE {
            *p.add(i) = 0;
            i += 1;
        }
    }
}
