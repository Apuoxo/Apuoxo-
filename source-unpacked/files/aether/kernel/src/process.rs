//! Aether Process Manager - native, not Linux process model

use crate::serial;
use crate::mm;
use crate::elf;

pub const MAX_PROCESSES: usize = 32;

#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Empty,
    Ready,
    Running,
    Exited,
}

#[derive(Clone, Copy)]
pub struct Process {
    pub pid: usize,
    pub state: State,
    pub entry: usize,
    pub stack: usize,
    pub cr3: usize,
    pub pages: [usize; 64],
    pub page_count: usize,
    pub name: [u8; 16],
    pub name_len: usize,
    pub start_tick: u64,
    pub run_ticks: u64,
    pub personality: crate::personality::PersonalityId,
}

impl Process {
    pub const fn empty() -> Self {
        Process {
            pid: 0,
            state: State::Empty,
            entry: 0,
            stack: 0,
            cr3: 0,
            pages: [0; 64],
            page_count: 0,
            name: [0; 16],
            name_len: 0,
            start_tick: 0,
            run_ticks: 0,
            personality: crate::personality::PersonalityId::None,
        }
    }
}

static mut TABLE: [Process; MAX_PROCESSES] = [Process::empty(); MAX_PROCESSES];
static mut NEXT_PID: usize = 1;
static mut CURRENT_PID: usize = 0;

pub fn current_pid() -> usize {
    unsafe { CURRENT_PID }
}

pub fn set_current(pid: usize) {
    unsafe { CURRENT_PID = pid; }
}

pub fn free_count_before() -> usize {
    mm::free_count()
}

/// Total pages currently owned by non-empty process slots (from common PMM pool).
pub fn pages_in_use() -> usize {
    unsafe {
        let mut s = 0usize;
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].state != State::Empty {
                s += TABLE[i].page_count;
            }
            i += 1;
        }
        s
    }
}

/// Runtime proof: alloc pages into a temp process, destroy, pages return to pool.
/// Returns (before, after_alloc, after_free, pages_n, ok).
pub fn reclaim_self_test() -> (usize, usize, usize, usize, bool) {
    let before = mm::free_count();
    let n = 8usize;
    let mut pages = [0usize; 8];
    let mut got = 0usize;
    let mut i = 0usize;
    while i < n {
        if let Some(pg) = mm::alloc_page() {
            pages[i] = pg;
            got += 1;
        } else {
            break;
        }
        i += 1;
    }
    let after_alloc = mm::free_count();
    // Create process owning these pages, then destroy (must free via process path).
    let img = elf::LoadedImage {
        entry: 0,
        pages,
        page_count: got,
        stack_top: 0,
        cr3: 0,
    };
    let pid = create_from_image("rct", &img);
    let after_free;
    let ok;
    if let Some(pid) = pid {
        destroy(pid);
        after_free = mm::free_count();
        // Expect free restored (allow small noise if create allocated nothing else).
        // PASS if we allocated something and destroy restored free count to before
        // (exact match: no leak from process path).
        ok = got > 0 && after_free == before;
    } else {
        // fallback free manually if create failed
        let mut j = 0usize;
        while j < got {
            mm::free_page(pages[j]);
            j += 1;
        }
        after_free = mm::free_count();
        ok = false;
    }
    (before, after_alloc, after_free, got, ok)
}


/// Create process from already-loaded ELF image

/// Create process from PE-loaded image (Windows personality).
pub fn create_from_pe(name: &str, entry: usize, stack_top: usize, cr3: usize, pages: &[usize], page_count: usize) -> Option<usize> {
    unsafe {
        let mut slot = None;
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].state == State::Empty {
                slot = Some(i);
                break;
            }
            i += 1;
        }
        let s = slot?;
        let pid = NEXT_PID;
        NEXT_PID += 1;
        let mut p = Process::empty();
        p.pid = pid;
        p.state = State::Ready;
        p.entry = entry;
        p.stack = stack_top;
        p.cr3 = cr3;
        let n = if page_count < 64 { page_count } else { 64 };
        let mut j = 0usize;
        while j < n {
            p.pages[j] = pages[j];
            j += 1;
        }
        p.page_count = n;
        let nb = name.as_bytes();
        let nlen = if nb.len() > 16 { 16 } else { nb.len() };
        let mut j = 0usize;
        while j < nlen {
            p.name[j] = nb[j];
            j += 1;
        }
        p.name_len = nlen;
        p.start_tick = crate::time::uptime();
        p.run_ticks = 0;
        p.personality = crate::personality::PersonalityId::Windows;
        crate::personality::load(crate::personality::PersonalityId::Windows);
        crate::personality::process_attach(p.personality);
        TABLE[s] = p;
        serial::write_str("  [PROC] PE create PID=");
        serial::write_usize(pid);
        serial::write_str(" name=");
        serial::write_str(name);
        serial::write_str(" entry=");
        serial::write_hex(entry);
        serial::write_str(" user_CR3=");
        serial::write_hex(cr3);
        serial::write_str(" pages=");
        serial::write_usize(n);
        serial::write_str(" personality=windows\n");
        Some(pid)
    }
}

pub fn create_from_image(name: &str, img: &elf::LoadedImage) -> Option<usize> {
    unsafe {
        let mut slot = None;
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].state == State::Empty {
                slot = Some(i);
                break;
            }
            i += 1;
        }
        let s = slot?;
        let pid = NEXT_PID;
        NEXT_PID += 1;
        let mut p = Process::empty();
        p.pid = pid;
        p.state = State::Ready;
        p.entry = img.entry;
        p.stack = img.stack_top;
        p.cr3 = img.cr3;
        {
            let mut j = 0usize;
            while j < 64 {
                p.pages[j] = 0;
                j += 1;
            }
            let n = if img.page_count < 64 { img.page_count } else { 64 };
            let mut j = 0usize;
            while j < n {
                // elf pages array is 8 long
                if j < 8 {
                    p.pages[j] = img.pages[j];
                }
                j += 1;
            }
            p.page_count = n.min(8);
        }
        let nb = name.as_bytes();
        let nlen = if nb.len() > 16 { 16 } else { nb.len() };
        let mut j = 0usize;
        while j < nlen {
            p.name[j] = nb[j];
            j += 1;
        }
        p.name_len = nlen;
        p.start_tick = crate::time::uptime();
        p.run_ticks = 0;
        p.personality = crate::personality::PersonalityId::Linux;
        crate::personality::load(crate::personality::PersonalityId::Linux);
        crate::personality::process_attach(p.personality);
        TABLE[s] = p;
        serial::write_str("  [PROC] create PID=");
        serial::write_usize(pid);
        serial::write_str(" name=");
        serial::write_str(name);
        serial::write_str(" entry=");
        serial::write_hex(img.entry);
        serial::write_str(" user_CR3=");
        serial::write_hex(img.cr3);
        serial::write_str(" pages=");
        serial::write_usize(img.page_count);
        serial::write_str("\n");
        Some(pid)
    }
}

pub fn destroy(pid: usize) {
    unsafe {
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].pid == pid && TABLE[i].state != State::Empty {
                crate::personality::process_detach(TABLE[i].personality);
                break;
            }
            i += 1;
        }
    }

    unsafe {
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].pid == pid && TABLE[i].state != State::Empty {
                let n = TABLE[i].page_count;
                let mut j = 0usize;
                while j < n {
                    let pg = TABLE[i].pages[j];
                    if pg != 0 {
                        mm::free_page(pg);
                    }
                    j += 1;
                }
                serial::write_str("  [PROC] PID=");
                serial::write_usize(pid);
                serial::write_str(" EXIT pages_freed=");
                serial::write_usize(n);
                serial::write_str("\n");
                TABLE[i] = Process::empty();
                if CURRENT_PID == pid {
                    CURRENT_PID = 0;
                }
                return;
            }
            i += 1;
        }
    }
}

pub fn next_ready() -> Option<usize> {
    unsafe {
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].state == State::Ready {
                return Some(TABLE[i].pid);
            }
            i += 1;
        }
        None
    }
}

pub fn get(pid: usize) -> Option<Process> {
    unsafe {
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].pid == pid && TABLE[i].state != State::Empty {
                return Some(TABLE[i]);
            }
            i += 1;
        }
        None
    }
}

pub fn set_state(pid: usize, st: State) {
    unsafe {
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].pid == pid {
                TABLE[i].state = st;
                return;
            }
            i += 1;
        }
    }
}

/// Legacy stubs for other modules
pub fn count_by_personality(id: crate::personality::PersonalityId) -> usize {
    unsafe {
        let mut c = 0usize;
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].state != State::Empty && TABLE[i].personality == id {
                c += 1;
            }
            i += 1;
        }
        c
    }
}
pub fn page_count(pid: usize) -> usize {
    get(pid).map(|p| p.page_count).unwrap_or(0)
}
pub fn exit(pid: usize) {
    destroy(pid);
}
pub fn create(
    _p: crate::personality::PersonalityId,
    entry: usize,
    stack: usize,
) -> Option<usize> {
    // minimal stub
    let img = elf::LoadedImage {
        entry,
        pages: [0; 8],
        page_count: 0,
        stack_top: stack,
        cr3: 0,
    };
    create_from_image("stub", &img)
}

/// Free all non-empty process slots (test harness / reclaim after AP tests).
pub fn destroy_all() -> usize {
    let mut n = 0usize;
    let mut i = 0usize;
    while i < MAX_PROCESSES {
        let pid = unsafe {
            if TABLE[i].state != State::Empty {
                TABLE[i].pid
            } else {
                0
            }
        };
        if pid != 0 {
            destroy(pid);
            n += 1;
        }
        i += 1;
    }
    n
}

pub fn active_count() -> usize {
    unsafe {
        let mut c = 0usize;
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].state != State::Empty && TABLE[i].state != State::Exited {
                c += 1;
            }
            i += 1;
        }
        c
    }
}

pub fn slot_count() -> usize {
    MAX_PROCESSES
}

/// Copy process from table slot if non-empty. Returns true if valid.
pub fn slot_get(slot: usize, out: &mut Process) -> bool {
    if slot >= MAX_PROCESSES {
        return false;
    }
    unsafe {
        if TABLE[slot].state == State::Empty {
            return false;
        }
        *out = TABLE[slot];
        true
    }
}

pub fn state_str(st: State) -> &'static str {
    match st {
        State::Empty => "Empty",
        State::Ready => "Ready",
        State::Running => "Running",
        State::Exited => "Exited",
    }
}

/// Account a scheduler quantum for PID (cooperative).
pub fn account_run(pid: usize, ticks: u64) {
    unsafe {
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].pid == pid && TABLE[i].state != State::Empty {
                TABLE[i].run_ticks = TABLE[i].run_ticks.saturating_add(ticks);
                return;
            }
            i += 1;
        }
    }
}

pub fn get_run_ticks(pid: usize) -> u64 {
    unsafe {
        let mut i = 0usize;
        while i < MAX_PROCESSES {
            if TABLE[i].pid == pid && TABLE[i].state != State::Empty {
                return TABLE[i].run_ticks;
            }
            i += 1;
        }
        0
    }
}


