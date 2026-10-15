//! Capability system (minimal)

#[derive(Clone, Copy)]
pub struct Cap {
    pub id: u64,
    pub rights: u32,
}

impl Cap {
    pub const fn null() -> Self {
        Cap { id: 0, rights: 0 }
    }

    pub fn is_valid(&self) -> bool {
        self.id != 0
    }
}

#[derive(Clone, Copy)]
pub struct CapTable {
    pub entries: [Cap; 16],
    pub used: usize,
}

impl CapTable {
    pub const fn new() -> Self {
        CapTable {
            entries: [Cap::null(); 16],
            used: 0,
        }
    }
}
