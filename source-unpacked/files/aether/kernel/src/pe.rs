//! PE/COFF probe + PE32+ section loader for Windows personality.
//! Stage 1: map sections into user CR3, apply base relocs, create process.
//! Stage 2 (later): import resolve + NT ABI - required before real app entry.

use crate::serial;
use crate::mm;
use crate::mm::paging;

#[derive(Clone, Copy, PartialEq)]
pub enum PeKind {
    NotPe,
    Pe32,
    Pe32Plus,
}

#[derive(Clone, Copy)]
pub struct PeInfo {
    pub kind: PeKind,
    pub machine: u16,
    pub sections: u16,
    pub entry_rva: u32,
    pub image_base: u64,
    pub subsystem: u16,
    pub dll_chars: u16,
    pub import_rva: u32,
    pub import_size: u32,
    pub reloc_rva: u32,
    pub reloc_size: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub is_dll: bool,
    pub is_gui: bool,
    pub is_x64: bool,
    pub is_i386: bool,
    pub has_imports: bool,
    pub has_relocs: bool,
}

impl PeInfo {
    pub const fn empty() -> Self {
        PeInfo {
            kind: PeKind::NotPe,
            machine: 0,
            sections: 0,
            entry_rva: 0,
            image_base: 0,
            subsystem: 0,
            dll_chars: 0,
            import_rva: 0,
            import_size: 0,
            reloc_rva: 0,
            reloc_size: 0,
            size_of_image: 0,
            size_of_headers: 0,
            is_dll: false,
            is_gui: false,
            is_x64: false,
            is_i386: false,
            has_imports: false,
            has_relocs: false,
        }
    }
}

fn r16(b: &[u8], o: usize) -> u16 {
    if o + 2 > b.len() {
        return 0;
    }
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn r32(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn r64(b: &[u8], o: usize) -> u64 {
    if o + 8 > b.len() {
        return 0;
    }
    let mut a = [0u8; 8];
    let mut i = 0;
    while i < 8 {
        a[i] = b[o + i];
        i += 1;
    }
    u64::from_le_bytes(a)
}

pub fn parse(buf: &[u8]) -> PeInfo {
    let mut info = PeInfo::empty();
    if buf.len() < 64 {
        return info;
    }
    if buf[0] != b'M' || buf[1] != b'Z' {
        return info;
    }
    let e_lfanew = r32(buf, 0x3C) as usize;
    if e_lfanew == 0 || e_lfanew + 24 > buf.len() {
        return info;
    }
    if buf[e_lfanew] != b'P'
        || buf[e_lfanew + 1] != b'E'
        || buf[e_lfanew + 2] != 0
        || buf[e_lfanew + 3] != 0
    {
        return info;
    }
    let coff = e_lfanew + 4;
    info.machine = r16(buf, coff);
    info.sections = r16(buf, coff + 2);
    let characteristics = r16(buf, coff + 18);
    info.is_dll = (characteristics & 0x2000) != 0;
    let opt = coff + 20;
    if opt + 2 > buf.len() {
        return info;
    }
    let magic = r16(buf, opt);
    match magic {
        0x10B => info.kind = PeKind::Pe32,
        0x20B => info.kind = PeKind::Pe32Plus,
        _ => {
            info.kind = PeKind::NotPe;
            return info;
        }
    }
    info.is_i386 = info.machine == 0x14C;
    info.is_x64 = info.machine == 0x8664;

    if info.kind == PeKind::Pe32 {
        if opt + 96 > buf.len() {
            return info;
        }
        info.entry_rva = r32(buf, opt + 16);
        info.image_base = r32(buf, opt + 28) as u64;
        info.subsystem = r16(buf, opt + 68);
        info.dll_chars = r16(buf, opt + 70);
        info.size_of_image = r32(buf, opt + 56);
        info.size_of_headers = r32(buf, opt + 60);
        let dd = opt + 96;
        if dd + 48 <= buf.len() {
            info.import_rva = r32(buf, dd + 8);
            info.import_size = r32(buf, dd + 12);
            info.reloc_rva = r32(buf, dd + 40);
            info.reloc_size = r32(buf, dd + 44);
        }
    } else {
        if opt + 112 > buf.len() {
            return info;
        }
        info.entry_rva = r32(buf, opt + 16);
        info.image_base = r64(buf, opt + 24);
        info.subsystem = r16(buf, opt + 68);
        info.dll_chars = r16(buf, opt + 70);
        info.size_of_image = r32(buf, opt + 56);
        info.size_of_headers = r32(buf, opt + 60);
        let dd = opt + 112;
        if dd + 48 <= buf.len() {
            info.import_rva = r32(buf, dd + 8);
            info.import_size = r32(buf, dd + 12);
            info.reloc_rva = r32(buf, dd + 40);
            info.reloc_size = r32(buf, dd + 44);
        }
    }
    info.has_imports = info.import_rva != 0 && info.import_size != 0;
    info.has_relocs = info.reloc_rva != 0 && info.reloc_size != 0;
    info.is_gui = info.subsystem == 2;
    info
}

pub fn is_exe_name(name: &[u8]) -> bool {
    if name.len() < 4 {
        return false;
    }
    let n = name.len();
    let a = name[n - 4];
    let b = name[n - 3];
    let c = name[n - 2];
    let d = name[n - 1];
    a == b'.'
        && (b == b'e' || b == b'E')
        && (c == b'x' || c == b'X')
        && (d == b'e' || d == b'E')
}

pub fn machine_name(m: u16) -> &'static str {
    match m {
        0x14C => "i386",
        0x8664 => "x86-64",
        0x1C0 => "ARM",
        0xAA64 => "ARM64",
        _ => "unknown",
    }
}

pub fn subsystem_name(s: u16) -> &'static str {
    match s {
        1 => "Native",
        2 => "Windows GUI",
        3 => "Windows CUI",
        5 => "OS/2",
        7 => "POSIX",
        9 => "Windows CE",
        10 => "EFI app",
        _ => "Other",
    }
}

pub fn probe_ntfs(mft_ref: u32) -> Option<PeInfo> {
    let mut buf = [0u8; 4096];
    let n = match crate::fs_ntfs::read_file_range(mft_ref, 0, &mut buf) {
        Some(n) if n >= 64 => n,
        _ => crate::fs_ntfs::read_file_prefix(mft_ref, &mut buf)?,
    };
    if n < 64 {
        return None;
    }
    let info = parse(&buf[..n]);
    if info.kind == PeKind::NotPe {
        return None;
    }
    Some(info)
}

const MAX_PE_PAGES: usize = 64; // 256 KiB max stage-1 image
const FALLBACK_BASE: u64 = 0x0000_0000_0040_0000;

pub struct PeLoadResult {
    pub pid: usize,
    pub entry: usize,
    pub cr3: usize,
    pub image_base: u64,
    pub pages: usize,
    pub has_imports: bool,
    pub is_x64: bool,
}

fn free_pages(pages: &mut [usize], count: usize) {
    let mut i = 0usize;
    while i < count {
        if pages[i] != 0 {
            mm::free_page(pages[i]);
            pages[i] = 0;
        }
        i += 1;
    }
}

/// Stage 1 Windows personality: load PE32+ sections from NTFS into a user address space.
/// Does NOT resolve imports or enter Ring3 entry if imports are required.
pub fn load_ntfs_x64(mft_ref: u32, name: &[u8]) -> Result<PeLoadResult, &'static str> {
    let mut hdr = [0u8; 4096];
    let hn = match crate::fs_ntfs::read_file_range(mft_ref, 0, &mut hdr) {
        Some(n) if n >= 64 => n,
        Some(n) if n > 0 => {
            // Too short for PE - try prefix path
            match crate::fs_ntfs::read_file_prefix(mft_ref, &mut hdr) {
                Some(m) if m >= 64 => m,
                Some(_) => return Err("File too small for PE"),
                None => return Err("PE header short/unreadable"),
            }
        }
        _ => {
            // Range failed: ATTRIBUTE_LIST / runlist / mount
            match crate::fs_ntfs::read_file_prefix(mft_ref, &mut hdr) {
                Some(m) if m >= 64 => m,
                Some(_) => return Err("File too small for PE"),
                None => return Err("Cannot read file data ($DATA)"),
            }
        }
    };
    // Sanity: MZ
    if hdr[0] != b'M' || hdr[1] != b'Z' {
        return Err("Not MZ/PE (bad magic)");
    }
    let info = parse(&hdr[..hn]);
    if info.kind == PeKind::NotPe {
        return Err("Not a PE executable");
    }
    if info.is_dll {
        return Err("DLL cannot run as program");
    }
    // Stage 1 only PE32+ x86-64 (kernel is long mode).
    if info.kind != PeKind::Pe32Plus || !info.is_x64 {
        return Err("Need PE32+ x86-64 (stage1)");
    }
    if info.size_of_image == 0 || info.size_of_image > (MAX_PE_PAGES as u32) * 4096 {
        return Err("Image too large for stage1");
    }

    crate::personality::load(crate::personality::PersonalityId::Windows);

    let preferred = info.image_base;
    // Prefer preferred base if in a safe user range; else fallback.
    let load_base = if preferred >= 0x400000 && preferred < 0x0000_7000_0000_0000 {
        preferred
    } else {
        FALLBACK_BASE
    };
    let delta = load_base as i64 - preferred as i64;

    let e_lfanew = r32(&hdr, 0x3C) as usize;
    let coff = e_lfanew + 4;
    let opt = coff + 20;
    let size_opt = r16(&hdr, coff + 16) as usize;
    let sec_off = opt + size_opt;
    let nsec = info.sections as usize;

    let kcr3 = unsafe { paging::read_cr3() };
    let cr3 = match unsafe { paging::create_user_pml4() } {
        Some(c) => c,
        None => return Err("create_user_pml4 failed"),
    };

    let mut pages = [0usize; MAX_PE_PAGES];
    let mut page_count = 0usize;

    let img_pages = ((info.size_of_image as usize) + 4095) / 4096;
    if img_pages > MAX_PE_PAGES - 1 {
        // leave 1 for stack
        return Err("Image pages exceed stage1 limit");
    }

    // Map and zero entire image span
    let mut pi = 0usize;
    while pi < img_pages {
        let phys = match mm::alloc_page() {
            Some(p) => p,
            None => {
                free_pages(&mut pages, page_count);
                return Err("OOM mapping PE");
            }
        };
        mm::zero_pages(phys, 1);
        let va = (load_base as usize) + pi * 4096;
        if !unsafe {
            paging::map_page(
                cr3,
                va,
                phys,
                paging::PAGE_PRESENT | paging::PAGE_WRITE | paging::PAGE_USER,
            )
        } {
            mm::free_page(phys);
            free_pages(&mut pages, page_count);
            return Err("map_page failed");
        }
        pages[page_count] = phys;
        page_count += 1;
        pi += 1;
    }

    // Copy headers into image
    let hdr_copy = (info.size_of_headers as usize).min(hn).min(info.size_of_image as usize);
    {
        let mut off = 0usize;
        while off < hdr_copy {
            let page_i = off / 4096;
            let po = off % 4096;
            let phys = pages[page_i];
            unsafe {
                *((phys + po) as *mut u8) = hdr[off];
            }
            off += 1;
        }
    }

    // Load each section from NTFS file
    let mut si = 0usize;
    while si < nsec {
        let so = sec_off + si * 40;
        if so + 40 > hn {
            break;
        }
        let vsize = r32(&hdr, so + 8) as usize;
        let va_rva = r32(&hdr, so + 12) as usize;
        let raw_size = r32(&hdr, so + 16) as usize;
        let raw_ptr = r32(&hdr, so + 20) as usize;
        if vsize == 0 && raw_size == 0 {
            si += 1;
            continue;
        }
        let copy_n = if raw_size < vsize { raw_size } else { vsize };
        let copy_n = if va_rva + copy_n > info.size_of_image as usize {
            info.size_of_image as usize - va_rva
        } else {
            copy_n
        };
        // Stream from disk in 512-byte chunks into mapped pages
        let mut done = 0usize;
        let mut chunk = [0u8; 512];
        while done < copy_n {
            let want = (copy_n - done).min(512);
            match crate::fs_ntfs::read_file_range(mft_ref, raw_ptr + done, &mut chunk[..want]) {
                Some(got) if got > 0 => {
                    let mut j = 0usize;
                    while j < got {
                        let img_off = va_rva + done + j;
                        let page_i = img_off / 4096;
                        let po = img_off % 4096;
                        if page_i < page_count {
                            unsafe {
                                *((pages[page_i] + po) as *mut u8) = chunk[j];
                            }
                        }
                        j += 1;
                    }
                    done += got;
                }
                _ => break,
            }
        }
        si += 1;
    }

    // Apply base relocations if load address != preferred
    if delta != 0 && info.has_relocs {
        apply_relocs(
            &pages[..page_count],
            load_base as usize,
            info.reloc_rva as usize,
            info.reloc_size as usize,
            delta,
        );
    }

    // Stack page after image
    let stack_va = (load_base as usize) + img_pages * 4096;
    let stack_phys = match mm::alloc_page() {
        Some(p) => p,
        None => {
            free_pages(&mut pages, page_count);
            return Err("OOM stack");
        }
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
        mm::free_page(stack_phys);
        free_pages(&mut pages, page_count);
        return Err("stack map fail");
    }
    if page_count < MAX_PE_PAGES {
        pages[page_count] = stack_phys;
        page_count += 1;
    }

    let entry = (load_base as usize).wrapping_add(info.entry_rva as usize);
    let stack_top = stack_va + 0xFF0;

    // Build short name for process table
    let mut name_str = [0u8; 16];
    let mut nl = 0usize;
    let mut i = 0usize;
    while i < name.len() && nl < 16 {
        name_str[nl] = name[i];
        nl += 1;
        i += 1;
    }
    let nref = core::str::from_utf8(&name_str[..nl]).unwrap_or("pe.exe");

    let pid = match crate::process::create_from_pe(
        nref,
        entry,
        stack_top,
        cr3,
        &pages[..page_count],
        page_count,
    ) {
        Some(p) => p,
        None => {
            free_pages(&mut pages, page_count);
            return Err("process create failed");
        }
    };

    // Switch back is not needed - we never switched CR3 for loading (mapped into user CR3 from kernel)
    let _ = kcr3;

    serial::write_str("[PE] LOAD OK pid=");
    serial::write_usize(pid);
    serial::write_str(" base=");
    serial::write_hex(load_base as usize);
    serial::write_str(" entry=");
    serial::write_hex(entry);
    serial::write_str(" pages=");
    serial::write_usize(page_count);
    serial::write_str(" imports=");
    serial::write_usize(if info.has_imports { 1 } else { 0 });
    serial::write_str("\n");

    Ok(PeLoadResult {
        pid,
        entry,
        cr3,
        image_base: load_base,
        pages: page_count,
        has_imports: info.has_imports,
        is_x64: true,
    })
}

fn apply_relocs(pages: &[usize], load_base: usize, reloc_rva: usize, reloc_size: usize, delta: i64) {
    if reloc_size == 0 || delta == 0 {
        return;
    }
    let mut off = 0usize;
    while off + 8 <= reloc_size {
        // Read page RVA + block size from mapped image
        let block_page = reloc_rva + off;
        let page_rva = read_u32_at(pages, load_base, block_page);
        let block_size = read_u32_at(pages, load_base, block_page + 4) as usize;
        if block_size < 8 {
            break;
        }
        let mut e = 8usize;
        while e + 2 <= block_size {
            let entry = read_u16_at(pages, load_base, block_page + e) as u32;
            let typ = (entry >> 12) as u16;
            let roff = (entry & 0xFFF) as usize;
            let target = (page_rva as usize) + roff;
            // IMAGE_REL_BASED_DIR64 = 10, HIGHLOW = 3
            if typ == 10 {
                let old = read_u64_at(pages, load_base, target);
                let new = (old as i64).wrapping_add(delta) as u64;
                write_u64_at(pages, load_base, target, new);
            } else if typ == 3 {
                let old = read_u32_at(pages, load_base, target);
                let new = (old as i64).wrapping_add(delta) as u32;
                write_u32_at(pages, load_base, target, new);
            }
            e += 2;
        }
        off += block_size;
        if block_size == 0 {
            break;
        }
    }
}

fn read_u16_at(pages: &[usize], base: usize, rva: usize) -> u16 {
    let abs = rva; // rva is offset within image
    let page_i = abs / 4096;
    let po = abs % 4096;
    if page_i >= pages.len() {
        return 0;
    }
    unsafe { *((pages[page_i] + po) as *const u16) }
}
fn read_u32_at(pages: &[usize], _base: usize, rva: usize) -> u32 {
    let page_i = rva / 4096;
    let po = rva % 4096;
    if page_i >= pages.len() || po + 4 > 4096 {
        // handle page cross simply if needed
        if page_i >= pages.len() {
            return 0;
        }
    }
    if page_i >= pages.len() {
        return 0;
    }
    if po + 4 <= 4096 {
        unsafe { *((pages[page_i] + po) as *const u32) }
    } else {
        0
    }
}
fn read_u64_at(pages: &[usize], _base: usize, rva: usize) -> u64 {
    let page_i = rva / 4096;
    let po = rva % 4096;
    if page_i >= pages.len() || po + 8 > 4096 {
        return 0;
    }
    unsafe { *((pages[page_i] + po) as *const u64) }
}
fn write_u32_at(pages: &[usize], _base: usize, rva: usize, v: u32) {
    let page_i = rva / 4096;
    let po = rva % 4096;
    if page_i < pages.len() && po + 4 <= 4096 {
        unsafe { *((pages[page_i] + po) as *mut u32) = v; }
    }
}
fn write_u64_at(pages: &[usize], _base: usize, rva: usize, v: u64) {
    let page_i = rva / 4096;
    let po = rva % 4096;
    if page_i < pages.len() && po + 8 <= 4096 {
        unsafe { *((pages[page_i] + po) as *mut u64) = v; }
    }
}

/// Launch path for Explorer: load image into Windows process; refuse entry if imports needed.
pub fn try_launch_ntfs(mft_ref: u32, name: &[u8]) -> &'static str {
    serial::write_str("[PE] Windows personality launch mft=");
    serial::write_usize(mft_ref as usize);
    serial::write_str("\n");

    match load_ntfs_x64(mft_ref, name) {
        Ok(r) => {
            if r.has_imports {
                // Image is in memory + process exists, but entry not safe without ABI.
                serial::write_str("[PE] LOADED pid=");
                serial::write_usize(r.pid);
                serial::write_str(" - RUN blocked (imports / Win32 ABI)\n");
                "LOADED - need Win32 ABI to run"
            } else {
                // No imports: still need PEB; do not jump yet.
                serial::write_str("[PE] LOADED no-imports pid=");
                serial::write_usize(r.pid);
                serial::write_str(" - entry not started (PEB pending)\n");
                "LOADED - PEB/entry next stage"
            }
        }
        Err(e) => {
            serial::write_str("[PE] LOAD FAIL: ");
            serial::write_str(e);
            serial::write_str("\n");
            e
        }
    }
}

pub fn format_summary(info: &PeInfo, out: &mut [u8]) -> usize {
    let mut p = 0usize;
    let put = |out: &mut [u8], p: &mut usize, s: &[u8]| {
        let mut i = 0;
        while i < s.len() && *p < out.len() {
            out[*p] = s[i];
            *p += 1;
            i += 1;
        }
    };
    put(out, &mut p, b"PE ");
    put(
        out,
        &mut p,
        match info.kind {
            PeKind::Pe32 => b"32 ",
            PeKind::Pe32Plus => b"32+ ",
            PeKind::NotPe => b"? ",
        },
    );
    put(out, &mut p, machine_name(info.machine).as_bytes());
    put(out, &mut p, b" ");
    put(out, &mut p, subsystem_name(info.subsystem).as_bytes());
    if info.has_imports {
        put(out, &mut p, b" +imports");
    }
    p
}
