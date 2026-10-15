//! Window Manager state (foundation for multi-window)
#[derive(Clone, Copy)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub z: i32,
    pub visible: bool,
    pub focused: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub rx: i32,
    pub ry: i32,
    pub rw: i32,
    pub rh: i32,
}

impl WindowState {
    pub const fn empty() -> Self {
        WindowState {
            x: 0, y: 0, w: 0, h: 0, z: 0,
            visible: false, focused: false,
            minimized: false, maximized: false,
            rx: 0, ry: 0, rw: 0, rh: 0,
        }
    }
}
