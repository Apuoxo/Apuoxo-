//! Personality Manager
//! Личности загружаются лениво и могут быть полностью выгружены.

#[derive(Clone, Copy, PartialEq)]
pub enum PersonalityId {
    None = 0,
    Linux = 1,
    Android = 2,
    Windows = 3,
}

pub struct Personality {
    pub id: PersonalityId,
    pub name: &'static str,
    pub loaded: bool,
    pub process_count: usize,
}

impl Personality {
    pub const fn empty() -> Self {
        Personality {
            id: PersonalityId::None,
            name: "none",
            loaded: false,
            process_count: 0,
        }
    }
}

static mut PERSONALITIES: [Personality; 4] = [
    Personality { id: PersonalityId::None, name: "none", loaded: false, process_count: 0 },
    Personality { id: PersonalityId::Linux, name: "linux", loaded: false, process_count: 0 },
    Personality { id: PersonalityId::Android, name: "android", loaded: false, process_count: 0 },
    Personality { id: PersonalityId::Windows, name: "windows", loaded: false, process_count: 0 },
];

pub fn load(id: PersonalityId) -> bool {
    let idx = id as usize;
    if idx == 0 || idx >= 4 {
        return false;
    }
    unsafe {
        if !PERSONALITIES[idx].loaded {
            PERSONALITIES[idx].loaded = true;
            // Здесь в будущем будет реальная загрузка модуля личности
        }
        true
    }
}

pub fn unload(id: PersonalityId) -> bool {
    let idx = id as usize;
    if idx == 0 || idx >= 4 {
        return false;
    }
    unsafe {
        if PERSONALITIES[idx].process_count == 0 {
            PERSONALITIES[idx].loaded = false;
            true
        } else {
            false
        }
    }
}

pub fn is_loaded(id: PersonalityId) -> bool {
    let idx = id as usize;
    if idx >= 4 { return false; }
    unsafe { PERSONALITIES[idx].loaded }
}

pub fn name(id: PersonalityId) -> &'static str {
    let idx = id as usize;
    if idx >= 4 { return "invalid"; }
    unsafe { PERSONALITIES[idx].name }
}

pub fn process_attach(id: PersonalityId) {
    let idx = id as usize;
    if idx > 0 && idx < 4 {
        unsafe { PERSONALITIES[idx].process_count += 1; }
    }
}

pub fn process_detach(id: PersonalityId) {
    let idx = id as usize;
    if idx > 0 && idx < 4 {
        unsafe {
            if PERSONALITIES[idx].process_count > 0 {
                PERSONALITIES[idx].process_count -= 1;
            }
        }
    }
}
