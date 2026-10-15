//! Aether ELF64 loader - loads PT_LOAD from buffer into user address space

use crate::serial;
use crate::mm;
use crate::mm::paging;

#[repr(C)]
struct Ehdr {
    ident: [u8; 16],
    etype: u16,
    machine: u16,
    version: u32,
    entry: u64,
    phoff: u64,
    shoff: u64,
    flags: u32,
    ehsize: u16,
    phentsize: u16,
    phnum: u16,
    shentsize: u16,
    shnum: u16,
    shstrndx: u16,
}

#[repr(C)]
struct Phdr {
    ptype: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}

const PT_LOAD: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3e;

pub struct LoadedImage {
    pub entry: usize,
    pub pages: [usize; 8],
    pub page_count: usize,
    pub stack_top: usize,
    pub cr3: usize,
}

fn read_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn read_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_u64(b: &[u8], off: usize) -> u64 {
    let mut a = [0u8; 8];
    let mut i = 0;
    while i < 8 {
        a[i] = b[off + i];
        i += 1;
    }
    u64::from_le_bytes(a)
}

/// Load ELF into a NEW user CR3 (distinct from kernel CR3) with USER mappings.
pub fn load(buf: &[u8]) -> Option<LoadedImage> {
    if buf.len() < 64 {
        serial::write_str("  [ELF] too small\n");
        return None;
    }
    if buf[0] != 0x7f || buf[1] != b'E' || buf[2] != b'L' || buf[3] != b'F' {
        serial::write_str("  [ELF] bad magic\n");
        return None;
    }
    if buf[4] != 2 || buf[5] != 1 {
        serial::write_str("  [ELF] not ELF64 LE\n");
        return None;
    }
    let etype = read_u16(buf, 16);
    let machine = read_u16(buf, 18);
    let entry = read_u64(buf, 24) as usize;
    let _ = (etype, machine);
    let phoff = read_u64(buf, 32) as usize;
    let phentsize = read_u16(buf, 54) as usize;
    let phnum = read_u16(buf, 56) as usize;
    if etype != ET_EXEC || machine != EM_X86_64 {
        serial::write_str("  [ELF] not x86_64 EXEC\n");
        return None;
    }
    serial::write_str("  [ELF] entry=");
    serial::write_hex(entry);
    serial::write_str(" phnum=");
    serial::write_usize(phnum);
    serial::write_str("\n");

    let kcr3 = unsafe { paging::read_cr3() };
    let cr3 = match unsafe { paging::create_user_pml4() } {
        Some(c) => c,
        None => {
            serial::write_str("  [ELF] create_user_pml4 FAIL\n");
            return None;
        }
    };
    serial::write_str("  [ELF] kernel CR3=");
    serial::write_hex(kcr3);
    serial::write_str(" user CR3=");
    serial::write_hex(cr3);
    if kcr3 != cr3 {
        serial::write_str(" DIFFERENT=YES\n");
    } else {
        serial::write_str(" DIFFERENT=NO\n");
    }
    let mut pages = [0usize; 8];
    let mut page_count = 0usize;

    let mut i = 0usize;
    while i < phnum {
        let off = phoff + i * phentsize;
        if off + 56 > buf.len() {
            break;
        }
        let ptype = read_u32(buf, off);
        if ptype != PT_LOAD {
            i += 1;
            continue;
        }
        let p_offset = read_u64(buf, off + 8) as usize;
        let vaddr = read_u64(buf, off + 16) as usize;
        let filesz = read_u64(buf, off + 32) as usize;
        let memsz = read_u64(buf, off + 40) as usize;
        serial::write_str("  [ELF] PT_LOAD vaddr=");
        serial::write_hex(vaddr);
        serial::write_str(" filesz=");
        serial::write_usize(filesz);
        serial::write_str(" memsz=");
        serial::write_usize(memsz);
        serial::write_str("\n");

        if memsz == 0 || memsz > 0x10000 {
            serial::write_str("  [ELF] memsz reject\n");
            return None;
        }
        let start = vaddr & !0xFFF;
        let end = (vaddr + memsz + 0xFFF) & !0xFFF;
        let mut va = start;
        while va < end {
            let phys = match mm::alloc_page() {
                Some(p) => p,
                None => {
                    serial::write_str("  [ELF] OOM\n");
                    return None;
                }
            };
            mm::zero_pages(phys, 1);
            if !unsafe {
                paging::map_page(
                    cr3,
                    va,
                    phys,
                    paging::PAGE_PRESENT | paging::PAGE_WRITE | paging::PAGE_USER,
                )
            } {
                serial::write_str("  [ELF] map fail\n");
                return None;
            }
            if page_count < 8 {
                pages[page_count] = phys;
                page_count += 1;
            }
            // Copy file data into this page if overlapping
            let page_file_start = if va >= vaddr { va - vaddr } else { 0 };
            // data in file at p_offset corresponding to vaddr
            let mut off_in_seg = if va > vaddr { va - vaddr } else { 0 };
            while off_in_seg < memsz && (vaddr + off_in_seg) < va + 4096 {
                let dst = phys + ((vaddr + off_in_seg) & 0xFFF);
                if off_in_seg < filesz && p_offset + off_in_seg < buf.len() {
                    unsafe {
                        *((dst) as *mut u8) = buf[p_offset + off_in_seg];
                    }
                }
                off_in_seg += 1;
            }
            let _ = page_file_start;
            va += 4096;
        }
        i += 1;
    }

    // User stack page at 0x402000
    let stack_va = 0x40002000usize;
    let stack_phys = match mm::alloc_page() {
        Some(p) => p,
        None => return None,
    };
    mm::zero_pages(stack_phys, 1);
    if !unsafe {
        paging::map_page(
            cr3,
            stack_va,
            stack_phys,
            paging::PAGE_PRESENT | paging::PAGE_WRITE | paging::PAGE_USER,
        )
    } {
        return None;
    }
    if page_count < 8 {
        pages[page_count] = stack_phys;
        page_count += 1;
    }
    unsafe { paging::load_cr3(cr3); }

    // Switch back to kernel CR3 for remaining boot (loader runs in kernel)
    unsafe { paging::load_cr3(kcr3); }

    Some(LoadedImage {
        entry,
        pages,
        page_count,
        stack_top: stack_va + 0xFF0,
        cr3,
    })
}
