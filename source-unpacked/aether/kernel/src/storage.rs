//! Host storage orchestration - READ ONLY
use crate::block;
use crate::part;
use crate::fs_fat;
use crate::serial;
use crate::drivers::ata;

pub fn init() {
    serial::write_str("\n======== STORAGE DIAGNOSTICS ========\n");
    serial::write_str("[STORAGE] controller: ATA PIO primary master\n");
    if ata::hw_present() {
        serial::write_str("[STORAGE] disk0: ATA present sectors=");
        serial::write_usize(ata::hw_sectors() as usize);
        serial::write_str(" (");
        serial::write_usize((ata::hw_sectors() as usize * 512) / (1024 * 1024));
        serial::write_str(" MB)\n");
        let mut model = [0u8; 40];
        let n = ata::model_bytes(&mut model);
        if n > 0 {
            serial::write_str("[STORAGE] model: ");
            let mut i = 0usize;
            while i < n {
                if model[i] >= 32 && model[i] < 127 {
                    serial::write_byte(model[i]);
                }
                i += 1;
            }
            serial::write_str("\n");
        }
    } else {
        serial::write_str("[STORAGE] no ATA HW disk (AHCI/NVMe not yet)\n");
    }
    block::init();
    serial::write_str("[STORAGE] chain: block after AHCI
");
    part::scan();
    serial::write_str("[STORAGE] chain: part::scan done n=");
    serial::write_usize(part::count());
    serial::write_str("
");
    // Try mount first FAT partition
    let np = part::count();
    let mut i = 0usize;
    let mut mounted = false;
    while i < np {
        if let Some(p) = part::get(i) {
            let tn = part::type_name(p.ptype);
            serial::write_str("[STORAGE] partition type ");
            serial::write_str(tn);
            serial::write_str("\n");
            if p.ptype == 0x0B || p.ptype == 0x0C || p.ptype == 0x06 || p.ptype == 0x0E || p.ptype == 0x04 {
                // test first sector readable
                let mut buf = [0u8; 512];
                if block::read(p.disk, p.lba_start, 1, &mut buf) {
                    serial::write_str("[STORAGE] sector0 read OK\n");
                    if fs_fat::mount_partition(i) {
                        serial::write_str("[STORAGE] FAT mount OK (READ-ONLY)\n");
                        mounted = true;
                        break;
                    }
                }
            } else if p.ptype == 0x07 {
                serial::write_str("[STORAGE] NTFS detected - NOT IMPLEMENTED (read-only FAT only)\n");
            }
        }
        i += 1;
    }
    if !mounted {
        serial::write_str("[STORAGE] no mountable FAT volume\n");
    }
    // NTFS RO
    if crate::fs_ntfs::mount_first() {
        serial::write_str("[STORAGE] NTFS root RO mounted\n");
    } else {
        serial::write_str("[STORAGE] NTFS mount skip/fail\n");
    }
    serial::write_str("======== END STORAGE DIAG ========\n\n");
}

pub fn diag_to_serial() {
    init();
}
