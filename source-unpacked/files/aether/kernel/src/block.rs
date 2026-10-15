//! Abstract block device - AHCI + legacy ATA (read + write)

use crate::drivers::ata;
use crate::drivers::ahci;
use crate::serial;

#[derive(Clone, Copy)]
pub struct BlockDev {
    pub id: u8,
    pub sectors: u32,
    pub is_hw: bool,
    pub is_ahci: bool,
    pub name: [u8; 40],
    pub name_len: usize,
}

static mut DEVS: [BlockDev; 4] = [
    BlockDev { id: 0, sectors: 0, is_hw: false, is_ahci: false, name: [0; 40], name_len: 0 },
    BlockDev { id: 1, sectors: 0, is_hw: false, is_ahci: false, name: [0; 40], name_len: 0 },
    BlockDev { id: 2, sectors: 0, is_hw: false, is_ahci: false, name: [0; 40], name_len: 0 },
    BlockDev { id: 3, sectors: 0, is_hw: false, is_ahci: false, name: [0; 40], name_len: 0 },
];
static mut NDEV: usize = 0;

pub fn init() {
    unsafe {
        NDEV = 0;
        // Prefer AHCI disks
        if ahci::is_ready() {
            let n = ahci::disk_count();
            let mut i = 0usize;
            while i < n && NDEV < 4 {
                let secs = ahci::disk_sectors(i);
                DEVS[NDEV].id = NDEV as u8;
                DEVS[NDEV].sectors = if secs > 0xFFFF_FFFF { 0xFFFF_FFFF } else { secs as u32 };
                DEVS[NDEV].is_hw = true;
                DEVS[NDEV].is_ahci = true;
                let mut model = [0u8; 40];
                let ml = ahci::disk_model(i, &mut model);
                let mut j = 0usize;
                while j < ml && j < 40 {
                    DEVS[NDEV].name[j] = model[j];
                    j += 1;
                }
                DEVS[NDEV].name_len = ml;
                serial::write_str("[BLOCK] AHCI disk");
                serial::write_usize(NDEV);
                serial::write_str(" secs=");
                serial::write_usize(DEVS[NDEV].sectors as usize);
                serial::write_str("\n");
                NDEV += 1;
                i += 1;
            }
        }
        // Fallback legacy ATA HW
        if NDEV == 0 && ata::hw_present() {
            DEVS[0].id = 0;
            DEVS[0].sectors = ata::hw_sectors();
            DEVS[0].is_hw = true;
            DEVS[0].is_ahci = false;
            let label = b"ATA Disk0";
            let mut i = 0usize;
            while i < label.len() {
                DEVS[0].name[i] = label[i];
                i += 1;
            }
            DEVS[0].name_len = label.len();
            NDEV = 1;
        }
        if NDEV == 0 {
            serial::write_str("[BLOCK] no HW disk\n");
        }
    }
}

pub fn count() -> usize {
    unsafe { NDEV }
}

pub fn get(i: usize) -> Option<BlockDev> {
    unsafe {
        if i < NDEV {
            Some(DEVS[i])
        } else {
            None
        }
    }
}

pub fn read(dev: u8, lba: u32, count: u8, buf: &mut [u8]) -> bool {
    if count == 0 {
        return true;
    }
    unsafe {
        if (dev as usize) >= NDEV || !DEVS[dev as usize].is_hw {
            return false;
        }
        if DEVS[dev as usize].is_ahci {
            return ahci::read_sectors(dev as usize, lba as u64, count, buf);
        }
    }
    ata::read_hw_sectors(lba, count, buf)
}

pub fn write(dev: u8, lba: u32, count: u8, buf: &[u8]) -> bool {
    if count == 0 {
        return true;
    }
    unsafe {
        if (dev as usize) >= NDEV || !DEVS[dev as usize].is_hw {
            return false;
        }
        if DEVS[dev as usize].is_ahci {
            return ahci::write_sectors(dev as usize, lba as u64, count, buf);
        }
    }
    ata::write_sectors(lba, count, buf)
}
