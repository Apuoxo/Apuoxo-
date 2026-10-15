//! Aether Ethernet - Realtek RTL8168/8111 (AH532: 10EC:8168)
//! PCI → MMIO → reset → DMA rings → TX/RX → DHCP (Discover/Offer/Request/ACK)
//! UDP headers are built correctly (dst port 67, length in length field).

use crate::mm::paging;
use crate::serial;

const CR: usize = 0x37;
const TPPOLL: usize = 0x38;
const ISR: usize = 0x3E;
const TCR: usize = 0x40;
const RCR: usize = 0x44;
const TNPDS: usize = 0x20;
const RDSAR: usize = 0xE4;
const RMS: usize = 0xDA;
const CPLUS: usize = 0xE0;
const PHYSTATUS: usize = 0x6C;

const CMD_RESET: u8 = 0x10;
const CMD_TE: u8 = 0x04;
const CMD_RE: u8 = 0x08;

const DESC_OWN: u32 = 1 << 31;
const DESC_EOR: u32 = 1 << 30;
const DESC_FS: u32 = 1 << 29;
const DESC_LS: u32 = 1 << 28;

const TX_COUNT: usize = 4;
const RX_COUNT: usize = 8;
const BUF_SIZE: usize = 1536;

#[repr(C)]
#[derive(Copy, Clone)]
struct Desc {
    opts1: u32,
    opts2: u32,
    addr_lo: u32,
    addr_hi: u32,
}

const EMPTY: Desc = Desc {
    opts1: 0,
    opts2: 0,
    addr_lo: 0,
    addr_hi: 0,
};

#[repr(align(256))]
struct TxRing([Desc; TX_COUNT]);
#[repr(align(256))]
struct RxRing([Desc; RX_COUNT]);
#[repr(align(16))]
struct PacketBuf([u8; BUF_SIZE]);

static mut TX_RING: TxRing = TxRing([EMPTY; TX_COUNT]);
static mut RX_RING: RxRing = RxRing([EMPTY; RX_COUNT]);
static mut TX_BUFS: [PacketBuf; TX_COUNT] =
    unsafe { core::mem::transmute([[0u8; BUF_SIZE]; TX_COUNT]) };
static mut RX_BUFS: [PacketBuf; RX_COUNT] =
    unsafe { core::mem::transmute([[0u8; BUF_SIZE]; RX_COUNT]) };

static mut READY: bool = false;
static mut ETH_FOUND: bool = false;
static mut WIFI_FOUND: bool = false;
static mut ETH_BUS: u8 = 0;
static mut ETH_DEV: u8 = 0;
static mut ETH_FUNC: u8 = 0;
static mut ETH_VID: u16 = 0;
static mut ETH_DID: u16 = 0;
static mut ETH_BAR0: u64 = 0;
static mut ETH_MMIO: usize = 0;
static mut ETH_MAC: [u8; 6] = [0; 6];
static mut ETH_MAC_OK: bool = false;
static mut ETH_RTL: bool = false;
static mut WIFI_VID: u16 = 0;
static mut WIFI_DID: u16 = 0;
static mut LINK_UP: bool = false;
static mut DMA_READY: bool = false;
static mut TX_IDX: usize = 0;

/// DHCP lease result
static mut DHCP_OK: bool = false;
static mut DHCP_IP: [u8; 4] = [0; 4];
static mut DHCP_MASK: [u8; 4] = [0; 4];
static mut DHCP_GW: [u8; 4] = [0; 4];
static mut DHCP_DNS: [u8; 4] = [0; 4];
static mut DHCP_SERVER: [u8; 4] = [0; 4];
static mut LAST_ERR: u8 = 0; // 0=ok 1=no_dev 2=reset 3=tx 4=no_offer 5=no_ack 6=bad_udp
static mut GW_MAC: [u8; 6] = [0; 6];
static mut GW_MAC_OK: bool = false;
static mut PING_OK: bool = false;
static mut PING_ERR: u8 = 0; // 0=ok 1=no_dhcp 2=arp 3=tx 4=timeout 5=bad_reply
static mut PING_LOOPS: u32 = 0;

const DHCP_XID: u32 = 0xA37E_8168;
const PING_ID: u16 = 0xAE01;

unsafe fn pci_r32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let a = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let v: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") v, options(nostack, preserves_flags));
    v
}
unsafe fn pci_w16(bus: u8, dev: u8, func: u8, off: u8, val: u16) {
    let a = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") a, options(nostack, preserves_flags));
    let mut old: u32;
    core::arch::asm!("in eax, dx", in("dx") 0xCFCu16, out("eax") old, options(nostack, preserves_flags));
    let shift = (off as u32 & 2) * 8;
    old = (old & !(0xFFFF << shift)) | ((val as u32) << shift);
    core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") old, options(nostack, preserves_flags));
}

fn map_mmio(phys: u64, size: usize) -> bool {
    if phys == 0 {
        return false;
    }
    let cr3 = unsafe { paging::read_cr3() };
    let start = (phys as usize) & !0xFFF;
    let end = (phys as usize + size + 0xFFF) & !0xFFF;
    let mut va = start;
    while va < end {
        if va >= 0x4000_0000 {
            let _ = unsafe {
                paging::map_page(cr3, va, va, paging::PAGE_PRESENT | paging::PAGE_WRITE)
            };
        }
        va += 0x1000;
    }
    unsafe { paging::load_cr3(cr3) };
    true
}

#[inline]
unsafe fn r8(o: usize) -> u8 {
    core::ptr::read_volatile((ETH_MMIO + o) as *const u8)
}
#[inline]
unsafe fn r16(o: usize) -> u16 {
    core::ptr::read_volatile((ETH_MMIO + o) as *const u16)
}
#[inline]
unsafe fn r32(o: usize) -> u32 {
    core::ptr::read_volatile((ETH_MMIO + o) as *const u32)
}
#[inline]
unsafe fn w8(o: usize, v: u8) {
    core::ptr::write_volatile((ETH_MMIO + o) as *mut u8, v);
}
#[inline]
unsafe fn w16(o: usize, v: u16) {
    core::ptr::write_volatile((ETH_MMIO + o) as *mut u16, v);
}
#[inline]
unsafe fn w32(o: usize, v: u32) {
    core::ptr::write_volatile((ETH_MMIO + o) as *mut u32, v);
}

fn delay(n: u32) {
    let mut i = 0u32;
    while i < n {
        i += 1;
        core::hint::spin_loop();
    }
}

fn phys_addr<T>(p: *const T) -> u32 {
    p as usize as u32
}

pub fn eth_found() -> bool {
    unsafe { ETH_FOUND }
}
pub fn wifi_found() -> bool {
    unsafe { WIFI_FOUND }
}
pub fn ready() -> bool {
    unsafe { READY }
}
pub fn link_up() -> bool {
    unsafe { LINK_UP }
}
pub fn eth_vid() -> u16 {
    unsafe { ETH_VID }
}
pub fn eth_did() -> u16 {
    unsafe { ETH_DID }
}
pub fn eth_bar0() -> u32 {
    unsafe { ETH_BAR0 as u32 }
}
pub fn eth_mac_ok() -> bool {
    unsafe { ETH_MAC_OK }
}
pub fn eth_is_rtl() -> bool {
    unsafe { ETH_RTL }
}
pub fn eth_mac(out: &mut [u8; 6]) {
    unsafe {
        let mut i = 0usize;
        while i < 6 {
            out[i] = ETH_MAC[i];
            i += 1;
        }
    }
}
pub fn dma_ready() -> bool {
    unsafe { DMA_READY }
}
pub fn dhcp_ok() -> bool {
    unsafe { DHCP_OK }
}
pub fn dhcp_ip(out: &mut [u8; 4]) {
    unsafe {
        *out = DHCP_IP;
    }
}
pub fn dhcp_mask(out: &mut [u8; 4]) {
    unsafe {
        *out = DHCP_MASK;
    }
}
pub fn dhcp_gw(out: &mut [u8; 4]) {
    unsafe {
        *out = DHCP_GW;
    }
}
pub fn dhcp_dns(out: &mut [u8; 4]) {
    unsafe {
        *out = DHCP_DNS;
    }
}
pub fn last_err() -> u8 {
    unsafe { LAST_ERR }
}
pub fn ping_ok() -> bool {
    unsafe { PING_OK }
}
pub fn ping_err() -> u8 {
    unsafe { PING_ERR }
}
pub fn ping_loops() -> u32 {
    unsafe { PING_LOOPS }
}
pub fn gw_mac(out: &mut [u8; 6]) {
    unsafe {
        *out = GW_MAC;
    }
}
pub fn gw_mac_ok() -> bool {
    unsafe { GW_MAC_OK }
}

fn rtl_reset() -> bool {
    unsafe {
        w8(CR, CMD_RESET);
        let mut t = 0u32;
        while t < 50_000 {
            if r8(CR) & CMD_RESET == 0 {
                return true;
            }
            delay(50);
            t += 1;
        }
        false
    }
}

fn read_mac() {
    unsafe {
        let mut i = 0usize;
        let mut ok = false;
        while i < 6 {
            ETH_MAC[i] = r8(i);
            if ETH_MAC[i] != 0 && ETH_MAC[i] != 0xFF {
                ok = true;
            }
            i += 1;
        }
        ETH_MAC_OK = ok;
    }
}

fn init_rings() {
    unsafe {
        let mut i = 0usize;
        while i < TX_COUNT {
            let buf_pa = phys_addr(TX_BUFS[i].0.as_ptr());
            TX_RING.0[i] = Desc {
                opts1: if i + 1 == TX_COUNT { DESC_EOR } else { 0 },
                opts2: 0,
                addr_lo: buf_pa,
                addr_hi: 0,
            };
            i += 1;
        }
        i = 0;
        while i < RX_COUNT {
            let buf_pa = phys_addr(RX_BUFS[i].0.as_ptr());
            RX_RING.0[i] = Desc {
                opts1: DESC_OWN
                    | (BUF_SIZE as u32)
                    | if i + 1 == RX_COUNT { DESC_EOR } else { 0 },
                opts2: 0,
                addr_lo: buf_pa,
                addr_hi: 0,
            };
            i += 1;
        }
        TX_IDX = 0;

        let tx_pa = phys_addr(TX_RING.0.as_ptr());
        let rx_pa = phys_addr(RX_RING.0.as_ptr());
        w32(TNPDS, tx_pa);
        w32(TNPDS + 4, 0);
        w32(RDSAR, rx_pa);
        w32(RDSAR + 4, 0);

        // Max RX packet size
        w16(RMS, 0x1FFF);
        // C+CR: RX threshold / early TX
        w16(CPLUS, 0x003F);

        // Accept broadcast + multicast + unicast + runt ok-ish
        // RCR: 0x000F_0E7E style simplified - AcceptAll / AB / AM / APM / AAP
        w32(RCR, 0x000F_0E7A);
        // TCR: IFG + DMA burst
        w32(TCR, 0x0300_0700);

        // Clear ISR, enable TE+RE
        w16(ISR, 0xFFFF);
        w8(CR, CMD_TE | CMD_RE);

        DMA_READY = true;
        LINK_UP = (r8(PHYSTATUS) & 0x02) != 0;
        serial::write_str("[NET] DMA rings ready TX=");
        serial::write_hex(tx_pa as usize);
        serial::write_str(" RX=");
        serial::write_hex(rx_pa as usize);
        serial::write_str(if LINK_UP { " LINK=UP\n" } else { " LINK=?\n" });
    }
}

/// Transmit one Ethernet frame. Returns true if OWN cleared (TX done).
pub fn tx_frame(data: &[u8]) -> bool {
    unsafe {
        if !DMA_READY || data.is_empty() || data.len() > BUF_SIZE {
            return false;
        }
        let idx = TX_IDX % TX_COUNT;
        // Wait if still owned by NIC
        let mut wait = 0u32;
        while wait < 2_000_000 {
            let o = core::ptr::read_volatile(&TX_RING.0[idx].opts1);
            if o & DESC_OWN == 0 {
                break;
            }
            core::hint::spin_loop();
            wait += 1;
        }
        if core::ptr::read_volatile(&TX_RING.0[idx].opts1) & DESC_OWN != 0 {
            return false;
        }
        let mut i = 0usize;
        while i < data.len() {
            TX_BUFS[idx].0[i] = data[i];
            i += 1;
        }
        // Ethernet minimum frame is 60 bytes without FCS. Short TCP SYN/ACK
        // (54 bytes) must be padded or many switches/NICs drop them.
        let mut flen = data.len();
        if flen < 60 {
            while flen < 60 {
                TX_BUFS[idx].0[flen] = 0;
                flen += 1;
            }
        }
        let mut opts = (flen as u32) | DESC_OWN | DESC_FS | DESC_LS;
        if idx + 1 == TX_COUNT {
            opts |= DESC_EOR;
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        core::ptr::write_volatile(&mut TX_RING.0[idx].opts1, opts);
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        w8(TPPOLL, 0x40); // NPQ normal priority queue poll

        wait = 0;
        while wait < 2_000_000 {
            let o = core::ptr::read_volatile(&TX_RING.0[idx].opts1);
            if o & DESC_OWN == 0 {
                TX_IDX = (idx + 1) % TX_COUNT;
                return true;
            }
            core::hint::spin_loop();
            wait += 1;
        }
        false
    }
}

/// Poll RX ring; returns (index, length) if a packet is ready.
fn rx_poll() -> Option<(usize, usize)> {
    unsafe {
        let mut i = 0usize;
        while i < RX_COUNT {
            let o = core::ptr::read_volatile(&RX_RING.0[i].opts1);
            if o & DESC_OWN == 0 {
                let len = (o & 0x3FFF) as usize;
                if len >= 14 {
                    return Some((i, len));
                }
                // recycle empty/error
                rearm_rx(i);
            }
            i += 1;
        }
        None
    }
}

fn rearm_rx(i: usize) {
    unsafe {
        let mut opts = DESC_OWN | (BUF_SIZE as u32);
        if i + 1 == RX_COUNT {
            opts |= DESC_EOR;
        }
        core::ptr::write_volatile(&mut RX_RING.0[i].opts1, opts);
        w16(ISR, 0xFFFF);
    }
}

fn inet_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0usize;
    while i + 1 < data.len() {
        sum += ((data[i] as u32) << 8) | (data[i + 1] as u32);
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// ARP request for gateway MAC. Needs DHCP lease first.
pub fn arp_gateway() -> bool {
    unsafe {
        GW_MAC_OK = false;
        if !DHCP_OK || !DMA_READY {
            return false;
        }
        let mut pkt = [0u8; 64];
        // Ethernet broadcast
        for j in 0..6 {
            pkt[j] = 0xFF;
            pkt[6 + j] = ETH_MAC[j];
        }
        pkt[12] = 0x08;
        pkt[13] = 0x06; // ARP
        pkt[14] = 0x00;
        pkt[15] = 0x01; // Ethernet
        pkt[16] = 0x08;
        pkt[17] = 0x00; // IPv4
        pkt[18] = 6;
        pkt[19] = 4; // MAC len, IP len
        pkt[20] = 0x00;
        pkt[21] = 0x01; // request
        for j in 0..6 {
            pkt[22 + j] = ETH_MAC[j];
        }
        for j in 0..4 {
            pkt[28 + j] = DHCP_IP[j];
        }
        for j in 0..6 {
            pkt[32 + j] = 0;
        }
        for j in 0..4 {
            pkt[38 + j] = DHCP_GW[j];
        }
        // pad to 60
        if !tx_frame(&pkt[..60]) {
            return false;
        }
        serial::write_str("[NET] ARP who-has gateway\n");
        let mut spins = 0u32;
        while spins < 4_000_000 {
            if let Some((idx, len)) = rx_poll() {
                let f = &RX_BUFS[idx].0[..len];
                if len >= 42 && f[12] == 0x08 && f[13] == 0x06 {
                    let op = ((f[20] as u16) << 8) | f[21] as u16;
                    if op == 2 {
                        // reply
                        let spa = [f[28], f[29], f[30], f[31]];
                        if spa == DHCP_GW {
                            for j in 0..6 {
                                GW_MAC[j] = f[22 + j];
                            }
                            GW_MAC_OK = true;
                            rearm_rx(idx);
                            serial::write_str("[NET] ARP reply GW MAC ok\n");
                            return true;
                        }
                    }
                }
                rearm_rx(idx);
            }
            core::hint::spin_loop();
            spins += 1;
        }
        false
    }
}

/// ICMP echo to 8.8.8.8 (Google Public DNS) via default gateway.
/// Returns true if echo reply received.
pub fn ping_google() -> bool {
    unsafe {
        PING_OK = false;
        PING_ERR = 0;
        PING_LOOPS = 0;
        if !DHCP_OK {
            // try DHCP once
            if !dhcp_run() {
                PING_ERR = 1;
                return false;
            }
        }
        if !GW_MAC_OK && !arp_gateway() {
            PING_ERR = 2;
            return false;
        }
        let dst = [8u8, 8, 8, 8];
        let mut pkt = [0u8; 98];
        // Ethernet → gateway
        for j in 0..6 {
            pkt[j] = GW_MAC[j];
            pkt[6 + j] = ETH_MAC[j];
        }
        pkt[12] = 0x08;
        pkt[13] = 0x00;
        // IPv4 header
        pkt[14] = 0x45;
        pkt[15] = 0;
        pkt[16] = 0x00;
        pkt[17] = 0x54; // 84 bytes total (20 IP + 64 ICMP)
        pkt[18] = 0xAE;
        pkt[19] = 0x01; // id
        pkt[20] = 0x00;
        pkt[21] = 0; // flags
        pkt[22] = 64; // TTL
        pkt[23] = 1; // ICMP
        pkt[24] = 0;
        pkt[25] = 0; // checksum later
        for j in 0..4 {
            pkt[26 + j] = DHCP_IP[j];
            pkt[30 + j] = dst[j];
        }
        ipv4_checksum(&mut pkt, 14);
        // ICMP echo request
        let icmp_off = 34usize;
        pkt[icmp_off] = 8; // type echo request
        pkt[icmp_off + 1] = 0; // code
        pkt[icmp_off + 2] = 0;
        pkt[icmp_off + 3] = 0; // checksum
        pkt[icmp_off + 4] = (PING_ID >> 8) as u8;
        pkt[icmp_off + 5] = PING_ID as u8;
        pkt[icmp_off + 6] = 0;
        pkt[icmp_off + 7] = 1; // seq
        let mut p = 0usize;
        while p < 56 {
            pkt[icmp_off + 8 + p] = (0x41 + (p % 26)) as u8; // payload 'A'..
            p += 1;
        }
        let csum = inet_checksum(&pkt[icmp_off..icmp_off + 64]);
        pkt[icmp_off + 2] = (csum >> 8) as u8;
        pkt[icmp_off + 3] = csum as u8;

        let frame_len = 14 + 84;
        if !tx_frame(&pkt[..frame_len]) {
            PING_ERR = 3;
            return false;
        }
        serial::write_str("[NET] ICMP echo -> 8.8.8.8\n");

        let mut spins = 0u32;
        while spins < 8_000_000 {
            if let Some((idx, len)) = rx_poll() {
                let f = &RX_BUFS[idx].0[..len];
                // IPv4 + ICMP echo reply
                if len >= 42
                    && f[12] == 0x08
                    && f[13] == 0x00
                    && f[23] == 1
                    && f[34] == 0
                {
                    // type 0 = echo reply
                    let id = ((f[38] as u16) << 8) | f[39] as u16;
                    if id == PING_ID {
                        // src IP should be 8.8.8.8
                        if f[26] == 8 && f[27] == 8 && f[28] == 8 && f[29] == 8 {
                            PING_OK = true;
                            PING_LOOPS = spins;
                            rearm_rx(idx);
                            serial::write_str("[NET] ICMP reply from 8.8.8.8\n");
                            return true;
                        }
                    }
                }
                // Also accept ARP while waiting
                if len >= 42 && f[12] == 0x08 && f[13] == 0x06 {
                    let op = ((f[20] as u16) << 8) | f[21] as u16;
                    if op == 2 {
                        let spa = [f[28], f[29], f[30], f[31]];
                        if spa == DHCP_GW {
                            for j in 0..6 {
                                GW_MAC[j] = f[22 + j];
                            }
                            GW_MAC_OK = true;
                        }
                    }
                }
                rearm_rx(idx);
            }
            core::hint::spin_loop();
            spins += 1;
        }
        PING_ERR = 4;
        PING_LOOPS = spins;
        serial::write_str("[NET] ICMP timeout\n");
        false
    }
}

fn ipv4_checksum(buf: &mut [u8], ip_off: usize) {
    buf[ip_off + 10] = 0;
    buf[ip_off + 11] = 0;
    let mut sum = 0u32;
    let mut k = 0usize;
    while k < 20 {
        sum += ((buf[ip_off + k] as u32) << 8) | (buf[ip_off + k + 1] as u32);
        k += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    let cs = !(sum as u16);
    buf[ip_off + 10] = (cs >> 8) as u8;
    buf[ip_off + 11] = cs as u8;
}

/// Build DHCPDISCOVER (broadcast). Frame length = 14+296 = 310.
fn build_discover(out: &mut [u8; BUF_SIZE]) -> usize {
    unsafe {
        let mut j = 0usize;
        while j < 310 {
            out[j] = 0;
            j += 1;
        }
        // Ethernet
        for j in 0..6 {
            out[j] = 0xFF;
            out[6 + j] = ETH_MAC[j];
        }
        out[12] = 0x08;
        out[13] = 0x00;
        // IPv4
        out[14] = 0x45;
        out[15] = 0;
        out[16] = 0x01;
        out[17] = 0x28; // total length 296
        out[18] = 0x81;
        out[19] = 0x68;
        out[20] = 0x40;
        out[21] = 0; // DF
        out[22] = 64;
        out[23] = 17; // UDP
                      // src 0.0.0.0 dst 255.255.255.255
        for j in 26..30 {
            out[j] = 0;
        }
        for j in 30..34 {
            out[j] = 0xFF;
        }
        // UDP: src=68 dst=67 len=276 (0x0114) csum=0
        out[34] = 0;
        out[35] = 68;
        out[36] = 0;
        out[37] = 67;
        out[38] = 0x01;
        out[39] = 0x14;
        out[40] = 0;
        out[41] = 0;
        // BOOTP/DHCP
        let b = 42usize;
        out[b] = 1; // BOOTREQUEST
        out[b + 1] = 1; // Ethernet
        out[b + 2] = 6;
        out[b + 3] = 0;
        out[b + 4] = (DHCP_XID >> 24) as u8;
        out[b + 5] = (DHCP_XID >> 16) as u8;
        out[b + 6] = (DHCP_XID >> 8) as u8;
        out[b + 7] = DHCP_XID as u8;
        out[b + 10] = 0x80; // broadcast flag
        for j in 0..6 {
            out[b + 28 + j] = ETH_MAC[j];
        }
        out[b + 236] = 99;
        out[b + 237] = 130;
        out[b + 238] = 83;
        out[b + 239] = 99;
        let mut p = b + 240;
        out[p] = 53;
        out[p + 1] = 1;
        out[p + 2] = 1;
        p += 3; // DISCOVER
        out[p] = 55;
        out[p + 1] = 4;
        out[p + 2] = 1;
        out[p + 3] = 3;
        out[p + 4] = 6;
        out[p + 5] = 15;
        p += 6;
        out[p] = 61;
        out[p + 1] = 7;
        out[p + 2] = 1;
        for j in 0..6 {
            out[p + 3 + j] = ETH_MAC[j];
        }
        p += 9;
        out[p] = 57;
        out[p + 1] = 2;
        out[p + 2] = 0x02;
        out[p + 3] = 0x40;
        p += 4;
        out[p] = 255;
        ipv4_checksum(out, 14);
        310
    }
}

/// Build DHCPREQUEST. Frame = 14+303 = 317. **UDP dst must be 67, length 283.**
fn build_request(out: &mut [u8; BUF_SIZE], offered: [u8; 4], server: [u8; 4]) -> usize {
    unsafe {
        let mut j = 0usize;
        while j < 317 {
            out[j] = 0;
            j += 1;
        }
        for j in 0..6 {
            out[j] = 0xFF;
            out[6 + j] = ETH_MAC[j];
        }
        out[12] = 0x08;
        out[13] = 0x00;
        out[14] = 0x45;
        out[15] = 0;
        out[16] = 0x01;
        out[17] = 0x2F; // 303
        out[18] = 0x81;
        out[19] = 0x68;
        out[20] = 0x40;
        out[21] = 0;
        out[22] = 64;
        out[23] = 17;
        for j in 26..30 {
            out[j] = 0;
        }
        for j in 30..34 {
            out[j] = 0xFF;
        }
        // CRITICAL: correct UDP header (bug that blocked ChatGPT branch)
        out[34] = 0;
        out[35] = 68; // src
        out[36] = 0;
        out[37] = 67; // dst BOOTPS
        out[38] = 0x01;
        out[39] = 0x1B; // length 283
        out[40] = 0;
        out[41] = 0;
        let b = 42usize;
        out[b] = 1;
        out[b + 1] = 1;
        out[b + 2] = 6;
        out[b + 3] = 0;
        out[b + 4] = (DHCP_XID >> 24) as u8;
        out[b + 5] = (DHCP_XID >> 16) as u8;
        out[b + 6] = (DHCP_XID >> 8) as u8;
        out[b + 7] = DHCP_XID as u8;
        out[b + 10] = 0x80;
        for j in 0..6 {
            out[b + 28 + j] = ETH_MAC[j];
        }
        out[b + 236] = 99;
        out[b + 237] = 130;
        out[b + 238] = 83;
        out[b + 239] = 99;
        let mut p = b + 240;
        out[p] = 53;
        out[p + 1] = 1;
        out[p + 2] = 3;
        p += 3; // REQUEST
        out[p] = 50;
        out[p + 1] = 4;
        for j in 0..4 {
            out[p + 2 + j] = offered[j];
        }
        p += 6;
        out[p] = 54;
        out[p + 1] = 4;
        for j in 0..4 {
            out[p + 2 + j] = server[j];
        }
        p += 6;
        out[p] = 61;
        out[p + 1] = 7;
        out[p + 2] = 1;
        for j in 0..6 {
            out[p + 3 + j] = ETH_MAC[j];
        }
        p += 9;
        out[p] = 55;
        out[p + 1] = 4;
        out[p + 2] = 1;
        out[p + 3] = 3;
        out[p + 4] = 6;
        out[p + 5] = 15;
        p += 6;
        out[p] = 57;
        out[p + 1] = 2;
        out[p + 2] = 0x02;
        out[p + 3] = 0x40;
        p += 4;
        out[p] = 255;
        ipv4_checksum(out, 14);
        317
    }
}

fn parse_dhcp(
    frame: &[u8],
    len: usize,
) -> Option<(u8, [u8; 4], [u8; 4], [u8; 4], [u8; 4], [u8; 4])> {
    // returns (msg_type, yiaddr, server, mask, router, dns)
    if len < 282 {
        return None;
    }
    if frame[12] != 0x08 || frame[13] != 0x00 {
        return None;
    }
    if frame[23] != 17 {
        return None;
    }
    let sport = ((frame[34] as u16) << 8) | frame[35] as u16;
    let dport = ((frame[36] as u16) << 8) | frame[37] as u16;
    if sport != 67 || dport != 68 {
        return None;
    }
    let xid = ((frame[46] as u32) << 24)
        | ((frame[47] as u32) << 16)
        | ((frame[48] as u32) << 8)
        | frame[49] as u32;
    if xid != DHCP_XID {
        return None;
    }
    let yiaddr = [frame[58], frame[59], frame[60], frame[61]];
    let mut msg = 0u8;
    let mut server = [0u8; 4];
    let mut mask = [0u8; 4];
    let mut router = [0u8; 4];
    let mut dns = [0u8; 4];
    let mut p = 282usize;
    while p + 1 < len {
        let code = frame[p];
        if code == 255 {
            break;
        }
        if code == 0 {
            p += 1;
            continue;
        }
        let ln = frame[p + 1] as usize;
        if p + 2 + ln > len {
            break;
        }
        match code {
            53 if ln == 1 => msg = frame[p + 2],
            54 if ln == 4 => server = [frame[p + 2], frame[p + 3], frame[p + 4], frame[p + 5]],
            1 if ln == 4 => mask = [frame[p + 2], frame[p + 3], frame[p + 4], frame[p + 5]],
            3 if ln >= 4 => router = [frame[p + 2], frame[p + 3], frame[p + 4], frame[p + 5]],
            6 if ln >= 4 => dns = [frame[p + 2], frame[p + 3], frame[p + 4], frame[p + 5]],
            _ => {}
        }
        p += 2 + ln;
    }
    if msg == 0 {
        return None;
    }
    Some((msg, yiaddr, server, mask, router, dns))
}

/// Full DHCP: Discover → Offer → Request → ACK. Safe to call repeatedly.
pub fn dhcp_run() -> bool {
    unsafe {
        LAST_ERR = 0;
        DHCP_OK = false;
        if !ETH_FOUND || !ETH_RTL {
            LAST_ERR = 1;
            return false;
        }
        if ETH_MMIO == 0 {
            LAST_ERR = 1;
            return false;
        }
        if !rtl_reset() {
            LAST_ERR = 2;
            return false;
        }
        read_mac();
        init_rings();

        // --- DISCOVER ---
        let mut pkt = [0u8; BUF_SIZE];
        let dlen = build_discover(&mut pkt);
        if !tx_frame(&pkt[..dlen]) {
            LAST_ERR = 3;
            return false;
        }
        serial::write_str("[NET] DHCPDISCOVER TX OK\n");

        let mut offered = [0u8; 4];
        let mut server = [0u8; 4];
        let mut got_offer = false;
        let mut spins = 0u32;
        while spins < 8_000_000 {
            if let Some((idx, len)) = rx_poll() {
                let frame = &RX_BUFS[idx].0[..len];
                if let Some((msg, yi, srv, _, _, _)) = parse_dhcp(frame, len) {
                    if msg == 2 {
                        // OFFER
                        offered = yi;
                        server = srv;
                        got_offer = true;
                        serial::write_str("[NET] DHCPOFFER ");
                        serial::write_usize(yi[0] as usize);
                        serial::write_str(".");
                        serial::write_usize(yi[1] as usize);
                        serial::write_str(".");
                        serial::write_usize(yi[2] as usize);
                        serial::write_str(".");
                        serial::write_usize(yi[3] as usize);
                        serial::write_str("\n");
                    }
                }
                rearm_rx(idx);
                if got_offer {
                    break;
                }
            }
            core::hint::spin_loop();
            spins += 1;
        }
        if !got_offer {
            LAST_ERR = 4;
            serial::write_str("[NET] DHCPOFFER TIMEOUT\n");
            return false;
        }

        // --- REQUEST (correct UDP!) ---
        let rlen = build_request(&mut pkt, offered, server);
        // Sanity: dst port 67, len 283
        if pkt[36] != 0 || pkt[37] != 67 || pkt[38] != 0x01 || pkt[39] != 0x1B {
            LAST_ERR = 6;
            serial::write_str("[NET] BUG: UDP header malformed\n");
            return false;
        }
        if !tx_frame(&pkt[..rlen]) {
            LAST_ERR = 3;
            return false;
        }
        serial::write_str("[NET] DHCPREQUEST TX OK (UDP dst=67 len=283)\n");

        let mut got_ack = false;
        spins = 0;
        while spins < 12_000_000 {
            if let Some((idx, len)) = rx_poll() {
                let frame = &RX_BUFS[idx].0[..len];
                if let Some((msg, yi, srv, mask, gw, dns)) = parse_dhcp(frame, len) {
                    if msg == 5 {
                        // ACK
                        DHCP_IP = yi;
                        DHCP_SERVER = srv;
                        DHCP_MASK = mask;
                        DHCP_GW = gw;
                        DHCP_DNS = dns;
                        DHCP_OK = true;
                        got_ack = true;
                        serial::write_str("[NET] DHCPACK ");
                        serial::write_usize(yi[0] as usize);
                        serial::write_str(".");
                        serial::write_usize(yi[1] as usize);
                        serial::write_str(".");
                        serial::write_usize(yi[2] as usize);
                        serial::write_str(".");
                        serial::write_usize(yi[3] as usize);
                        serial::write_str("\n");
                    } else if msg == 6 {
                        serial::write_str("[NET] DHCPNAK\n");
                    }
                }
                rearm_rx(idx);
                if got_ack {
                    break;
                }
            }
            core::hint::spin_loop();
            spins += 1;
        }
        if !got_ack {
            LAST_ERR = 5;
            serial::write_str("[NET] DHCPACK TIMEOUT\n");
            return false;
        }
        true
    }
}

fn rtl_init_and_mac(mmio: usize) -> bool {
    unsafe {
        ETH_MMIO = mmio;
        if !rtl_reset() {
            return false;
        }
        read_mac();
        serial::write_str("[NET] RTL MAC=");
        let mut i = 0usize;
        while i < 6 {
            serial::write_hex(ETH_MAC[i] as usize);
            if i + 1 < 6 {
                serial::write_str(":");
            }
            i += 1;
        }
        serial::write_str(if ETH_MAC_OK { " OK\n" } else { " (empty)\n" });
        ETH_MAC_OK
    }
}

/// Strip scheme / path / port → bare hostname for DNS.
fn dns_hostname<'a>(raw: &'a [u8]) -> &'a [u8] {
    let mut s = raw;
    // skip http:// or https://
    if s.len() >= 7 && s[0] == b'h' && s[1] == b't' && s[2] == b't' && s[3] == b'p' {
        if s.len() >= 8 && s[4] == b':' {
            s = &s[7..]; // http://
        } else if s.len() >= 9 && s[4] == b's' && s[5] == b':' {
            s = &s[8..]; // https://
        }
    }
    // strip path
    let mut end = s.len();
    let mut i = 0usize;
    while i < s.len() {
        if s[i] == b'/' || s[i] == b':' || s[i] == b'?' || s[i] == b'#' {
            end = i;
            break;
        }
        i += 1;
    }
    &s[..end]
}

/// Built-in A records for common hosts (used when live DNS fails).
fn dns_static(name: &[u8]) -> Option<[u8; 4]> {
    // case-insensitive compare helper
    fn eq_ci(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut i = 0usize;
        while i < a.len() {
            let x = if a[i] >= b'A' && a[i] <= b'Z' {
                a[i] + 32
            } else {
                a[i]
            };
            let y = if b[i] >= b'A' && b[i] <= b'Z' {
                b[i] + 32
            } else {
                b[i]
            };
            if x != y {
                return false;
            }
            i += 1;
        }
        true
    }
    // NOTE: 93.184.216.34 is DEAD (times out) - do not use.
    if eq_ci(name, b"example.com") || eq_ci(name, b"www.example.com") {
        return Some([172, 66, 147, 243]); // Cloudflare, HTTP 200 verified 2026-10
    }
    if eq_ci(name, b"google.com") {
        return Some([192, 178, 155, 102]);
    }
    if eq_ci(name, b"www.google.com") {
        return Some([142, 251, 154, 119]);
    }
    if eq_ci(name, b"info.cern.ch") {
        return Some([188, 184, 67, 127]);
    }
    if eq_ci(name, b"httpforever.com") || eq_ci(name, b"www.httpforever.com") {
        return Some([104, 21, 4, 210]);
    }
    if eq_ci(name, b"neverssl.com") || eq_ci(name, b"www.neverssl.com") {
        return Some([34, 223, 124, 45]);
    }
    // IP literal already? e.g. 1.1.1.1
    let mut dots = 0u8;
    let mut ok = true;
    let mut i = 0usize;
    while i < name.len() {
        let c = name[i];
        if c == b'.' {
            dots += 1;
        } else if c < b'0' || c > b'9' {
            ok = false;
            break;
        }
        i += 1;
    }
    if ok && dots == 3 {
        let mut ip = [0u8; 4];
        let mut part = 0u16;
        let mut idx = 0usize;
        let mut j = 0usize;
        while j <= name.len() {
            if j == name.len() || name[j] == b'.' {
                if idx >= 4 || part > 255 {
                    return None;
                }
                ip[idx] = part as u8;
                idx += 1;
                part = 0;
            } else {
                part = part * 10 + (name[j] - b'0') as u16;
            }
            j += 1;
        }
        if idx == 4 {
            return Some(ip);
        }
    }
    None
}

fn dns_query_one(name: &[u8], dns_ip: [u8; 4], xid: u16) -> Option<[u8; 4]> {
    unsafe {
        let mut pkt = [0u8; 512];
        for j in 0..6 {
            pkt[j] = GW_MAC[j];
            pkt[6 + j] = ETH_MAC[j];
        }
        pkt[12] = 0x08;
        pkt[13] = 0x00;
        // IPv4
        pkt[14] = 0x45;
        pkt[15] = 0;
        pkt[18] = (xid >> 8) as u8;
        pkt[19] = xid as u8;
        pkt[20] = 0;
        pkt[21] = 0;
        pkt[22] = 64;
        pkt[23] = 17; // UDP
        for j in 0..4 {
            pkt[26 + j] = DHCP_IP[j];
            pkt[30 + j] = dns_ip[j];
        }
        // UDP: sport 49152, dport 53
        pkt[34] = 0xC0;
        pkt[35] = 0x00;
        pkt[36] = 0;
        pkt[37] = 53;
        // DNS header
        let d = 42usize;
        pkt[d] = (xid >> 8) as u8;
        pkt[d + 1] = xid as u8;
        pkt[d + 2] = 0x01; // RD
        pkt[d + 3] = 0x00;
        pkt[d + 4] = 0x00;
        pkt[d + 5] = 0x01; // QDCOUNT=1
        // AN/NS/AR = 0 already
        let mut p = d + 12;
        let mut i = 0usize;
        while i < name.len() {
            let start = i;
            while i < name.len() && name[i] != b'.' {
                i += 1;
            }
            let lab = i - start;
            if lab == 0 || lab > 63 || p >= 500 {
                return None;
            }
            pkt[p] = lab as u8;
            p += 1;
            let mut k = 0usize;
            while k < lab {
                let c = name[start + k];
                // lowercase for DNS
                pkt[p] = if c >= b'A' && c <= b'Z' { c + 32 } else { c };
                p += 1;
                k += 1;
            }
            if i < name.len() && name[i] == b'.' {
                i += 1;
            }
        }
        pkt[p] = 0;
        p += 1;
        pkt[p] = 0;
        pkt[p + 1] = 1; // A
        pkt[p + 2] = 0;
        pkt[p + 3] = 1; // IN
        p += 4;
        let udp_len = (p - 34) as u16;
        pkt[38] = (udp_len >> 8) as u8;
        pkt[39] = udp_len as u8;
        let ip_total = (p - 14) as u16;
        pkt[16] = (ip_total >> 8) as u8;
        pkt[17] = ip_total as u8;
        ipv4_checksum(&mut pkt, 14);
        // UDP checksum = 0 (valid for IPv4)
        if !tx_frame(&pkt[..p]) {
            return None;
        }
        let mut spins = 0u32;
        while spins < 8_000_000 {
            if let Some((idx, len)) = rx_poll() {
                let f = &RX_BUFS[idx].0[..len];
                if len > 60 && f[12] == 0x08 && f[13] == 0x00 && f[23] == 17 {
                    let sport = ((f[34] as u16) << 8) | f[35] as u16;
                    let dport = ((f[36] as u16) << 8) | f[37] as u16;
                    // response from DNS: sport 53, our dport 49152
                    if sport == 53 && (dport == 0xC000 || dport == 53) {
                        // verify xid
                        let rxid = ((f[42] as u16) << 8) | f[43] as u16;
                        if rxid == xid {
                            // QR must be response (flags at 44-45)
                            if f[44] & 0x80 != 0 {
                                // DNS: 42 xid, 44 flags, 46 qdcount, 48 ancount
                                let qdcount = ((f[46] as u16) << 8) | f[47] as u16;
                                let ancount2 = ((f[48] as u16) << 8) | f[49] as u16;
                                let mut off = 54usize;
                                let mut q = 0u16;
                                while q < qdcount && off < len {
                                    // name
                                    if f[off] & 0xC0 == 0xC0 {
                                        off += 2;
                                    } else {
                                        while off < len && f[off] != 0 {
                                            let l = f[off] as usize;
                                            if l > 63 {
                                                break;
                                            }
                                            off += 1 + l;
                                        }
                                        if off < len {
                                            off += 1; // null
                                        }
                                    }
                                    off += 4; // type+class
                                    q += 1;
                                }
                                let mut a = 0u16;
                                while a < ancount2 && off + 10 <= len {
                                    // name
                                    if f[off] & 0xC0 == 0xC0 {
                                        off += 2;
                                    } else {
                                        while off < len && f[off] != 0 {
                                            let l = f[off] as usize;
                                            if l > 63 {
                                                break;
                                            }
                                            off += 1 + l;
                                        }
                                        if off < len {
                                            off += 1;
                                        }
                                    }
                                    if off + 10 > len {
                                        break;
                                    }
                                    let rtype =
                                        ((f[off] as u16) << 8) | f[off + 1] as u16;
                                    let rdlen =
                                        ((f[off + 8] as u16) << 8) | f[off + 9] as u16;
                                    off += 10;
                                    if rtype == 1 && rdlen == 4 && off + 4 <= len {
                                        let ip = [
                                            f[off],
                                            f[off + 1],
                                            f[off + 2],
                                            f[off + 3],
                                        ];
                                        rearm_rx(idx);
                                        return Some(ip);
                                    }
                                    off += rdlen as usize;
                                    a += 1;
                                }
                            }
                        }
                    }
                }
                rearm_rx(idx);
            }
            core::hint::spin_loop();
            spins += 1;
        }
        None
    }
}

/// Resolve hostname → IPv4.
/// Order: static table → DHCP DNS → 8.8.8.8 → 1.1.1.1
pub fn dns_resolve(name: &[u8]) -> Option<[u8; 4]> {
    unsafe {
        if !DHCP_OK || !DMA_READY {
            return dns_static(name);
        }
        if !GW_MAC_OK && !arp_gateway() {
            return dns_static(name);
        }
        let host = dns_hostname(name);
        if host.is_empty() {
            return None;
        }
        // IP literal?
        let mut only_ip = true;
        let mut i = 0usize;
        while i < host.len() {
            let c = host[i];
            if !((c >= b'0' && c <= b'9') || c == b'.') {
                only_ip = false;
                break;
            }
            i += 1;
        }
        if only_ip {
            return dns_static(host);
        }
        // Known hosts: use verified static IP first (live DNS may fail on some routers)
        if let Some(ip) = dns_static(host) {
            return Some(ip);
        }
        // Live DNS for unknown names
        let mut servers = [[0u8; 4]; 3];
        let mut nsrv = 0usize;
        if DHCP_DNS != [0, 0, 0, 0] && DHCP_DNS != DHCP_IP {
            servers[nsrv] = DHCP_DNS;
            nsrv += 1;
        }
        servers[nsrv] = [8, 8, 8, 8];
        nsrv += 1;
        servers[nsrv] = [1, 1, 1, 1];
        nsrv += 1;
        let mut s = 0usize;
        while s < nsrv {
            let xid = 0xAE50u16.wrapping_add(s as u16);
            if let Some(ip) = dns_query_one(host, servers[s], xid) {
                return Some(ip);
            }
            s += 1;
        }
        None
    }
}

fn build_tcp_frame(
    pkt: &mut [u8],
    dst_ip: [u8; 4],
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> usize {
    unsafe {
        for j in 0..6 {
            pkt[j] = GW_MAC[j];
            pkt[6 + j] = ETH_MAC[j];
        }
        pkt[12] = 0x08;
        pkt[13] = 0x00;
        pkt[14] = 0x45;
        pkt[15] = 0;
        let plen = payload.len();
        // Include MSS option on pure SYN (no payload) for better compatibility
        let syn_only = (flags & 0x02) != 0 && (flags & 0x10) == 0 && plen == 0;
        let tcp_hdr = if syn_only { 24usize } else { 20usize }; // +4 for MSS
        let ip_total = (20 + tcp_hdr + plen) as u16;
        pkt[16] = (ip_total >> 8) as u8;
        pkt[17] = ip_total as u8;
        pkt[18] = 0x10;
        pkt[19] = 0x00;
        pkt[20] = 0x40; // DF
        pkt[21] = 0;
        pkt[22] = 64;
        pkt[23] = 6;
        pkt[24] = 0;
        pkt[25] = 0;
        for j in 0..4 {
            pkt[26 + j] = DHCP_IP[j];
            pkt[30 + j] = dst_ip[j];
        }
        // TCP
        pkt[34] = 0xC0;
        pkt[35] = 0x50; // sport 49232
        pkt[36] = 0;
        pkt[37] = 80; // dport 80
        pkt[38] = (seq >> 24) as u8;
        pkt[39] = (seq >> 16) as u8;
        pkt[40] = (seq >> 8) as u8;
        pkt[41] = seq as u8;
        pkt[42] = (ack >> 24) as u8;
        pkt[43] = (ack >> 16) as u8;
        pkt[44] = (ack >> 8) as u8;
        pkt[45] = ack as u8;
        pkt[46] = ((tcp_hdr / 4) as u8) << 4; // data offset
        pkt[47] = flags;
        pkt[48] = 0xFF;
        pkt[49] = 0xFF; // window
        pkt[50] = 0;
        pkt[51] = 0; // checksum placeholder
        pkt[52] = 0;
        pkt[53] = 0; // urgent
        let mut data_off = 54usize;
        if syn_only {
            // MSS 1460: kind=2 len=4 mss=0x05B4
            pkt[54] = 2;
            pkt[55] = 4;
            pkt[56] = 0x05;
            pkt[57] = 0xB4;
            data_off = 58;
        }
        let mut i = 0usize;
        while i < plen {
            pkt[data_off + i] = payload[i];
            i += 1;
        }
        let end = data_off + plen;
        // TCP checksum (pseudo-header + TCP)
        let mut sum = 0u32;
        sum += ((DHCP_IP[0] as u32) << 8) | (DHCP_IP[1] as u32);
        sum += ((DHCP_IP[2] as u32) << 8) | (DHCP_IP[3] as u32);
        sum += ((dst_ip[0] as u32) << 8) | (dst_ip[1] as u32);
        sum += ((dst_ip[2] as u32) << 8) | (dst_ip[3] as u32);
        sum += 6; // protocol
        let tcp_len = (tcp_hdr + plen) as u32;
        sum += tcp_len;
        let mut k = 34usize;
        while k + 1 < end {
            if k == 50 {
                k += 2;
                continue;
            }
            sum += ((pkt[k] as u32) << 8) | (pkt[k + 1] as u32);
            k += 2;
        }
        if (end - 34) % 2 == 1 {
            sum += (pkt[end - 1] as u32) << 8;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        let cs = !(sum as u16);
        pkt[50] = (cs >> 8) as u8;
        pkt[51] = cs as u8;
        ipv4_checksum(pkt, 14);
        end
    }
}

/// HTTP/1.0 GET - writes response into out, returns length.
/// Returns 0 on failure. Distinguishes stages via LAST_ERR:
///   20 = no SYN-ACK, 21 = RST, 22 = GET tx fail, 23 = no HTTP body
pub fn http_get(ip: [u8; 4], host: &[u8], path: &[u8], out: &mut [u8]) -> usize {
    unsafe {
        if !DHCP_OK || !DMA_READY {
            LAST_ERR = 10;
            return 0;
        }
        // Refresh gateway MAC before TCP (stale ARP → blackhole SYN)
        GW_MAC_OK = false;
        if !arp_gateway() {
            LAST_ERR = 11;
            return 0;
        }
        let host_clean = dns_hostname(host);
        let mut iss = 0x1000_0001u32;
        let mut pkt = [0u8; BUF_SIZE];
        // 1) SYN (with MSS option)
        let syn_len = build_tcp_frame(&mut pkt, ip, iss, 0, 0x02, &[]);
        if !tx_frame(&pkt[..syn_len]) {
            LAST_ERR = 12;
            return 0;
        }
        let mut ack_n = 0u32;
        let mut syn_ok = false;
        let mut spins = 0u32;
        let mut rx_tcp = 0u32;
        while spins < 12_000_000 {
            if let Some((idx, len)) = rx_poll() {
                let f = &RX_BUFS[idx].0[..len];
                if len >= 54 && f[12] == 0x08 && f[13] == 0x00 {
                    let ihl = ((f[14] & 0x0F) as usize) * 4;
                    if ihl >= 20 && f[23] == 6 && len >= 14 + ihl + 20 {
                        let th = 14 + ihl;
                        let sport = ((f[th] as u16) << 8) | f[th + 1] as u16;
                        let dport = ((f[th + 2] as u16) << 8) | f[th + 3] as u16;
                        let flags = f[th + 13];
                        rx_tcp = rx_tcp.wrapping_add(1);
                        // our sport is 0xC050 (49232); accept SYN-ACK from port 80
                        // also accept if only dport matches (some middleboxes)
                        if dport == 0xC050 {
                            if flags & 0x04 != 0 {
                                rearm_rx(idx);
                                LAST_ERR = 21;
                                return 0;
                            }
                            if flags & 0x12 == 0x12 {
                                let seq = ((f[th + 4] as u32) << 24)
                                    | ((f[th + 5] as u32) << 16)
                                    | ((f[th + 6] as u32) << 8)
                                    | f[th + 7] as u32;
                                ack_n = seq.wrapping_add(1);
                                syn_ok = true;
                                rearm_rx(idx);
                                break;
                            }
                        }
                        let _ = sport;
                    }
                }
                rearm_rx(idx);
            }
            core::hint::spin_loop();
            spins += 1;
        }
        if !syn_ok {
            // encode whether any TCP was seen: 20 = silence, 24 = TCP but no SYN-ACK
            LAST_ERR = if rx_tcp > 0 { 24 } else { 20 };
            return 0;
        }
        // 2) ACK the SYN
        iss = iss.wrapping_add(1);
        let alen = build_tcp_frame(&mut pkt, ip, iss, ack_n, 0x10, &[]);
        let _ = tx_frame(&pkt[..alen]);
        // brief pause so peer processes ACK before data
        spins = 0;
        while spins < 200_000 {
            core::hint::spin_loop();
            spins += 1;
        }
        // 3) HTTP GET
        let mut req = [0u8; 192];
        let mut r = 0usize;
        let append = |dst: &mut [u8], rp: &mut usize, s: &[u8]| {
            let mut i = 0usize;
            while i < s.len() && *rp < dst.len() {
                dst[*rp] = s[i];
                *rp += 1;
                i += 1;
            }
        };
        append(&mut req, &mut r, b"GET ");
        if path.is_empty() {
            append(&mut req, &mut r, b"/");
        } else {
            append(&mut req, &mut r, path);
        }
        append(&mut req, &mut r, b" HTTP/1.0\r\nHost: ");
        append(&mut req, &mut r, host_clean);
        append(
            &mut req,
            &mut r,
            b"\r\nUser-Agent: AetherOS/1.0\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        );
        let gl = build_tcp_frame(&mut pkt, ip, iss, ack_n, 0x18, &req[..r]);
        if !tx_frame(&pkt[..gl]) {
            LAST_ERR = 22;
            return 0;
        }
        iss = iss.wrapping_add(r as u32);
        // 4) collect response
        let mut written = 0usize;
        let mut got_fin = false;
        spins = 0;
        while spins < 4_000_000 && written < out.len() && !got_fin {
            if let Some((idx, len)) = rx_poll() {
                let f = &RX_BUFS[idx].0[..len];
                if len >= 54 && f[12] == 0x08 && f[13] == 0x00 {
                    let ihl = ((f[14] & 0x0F) as usize) * 4;
                    if ihl >= 20 && f[23] == 6 && len >= 14 + ihl + 20 {
                        let th = 14 + ihl;
                        let sport = ((f[th] as u16) << 8) | f[th + 1] as u16;
                        let dport = ((f[th + 2] as u16) << 8) | f[th + 3] as u16;
                        if dport == 0xC050 && sport == 80 {
                            let doff = ((f[th + 12] >> 4) as usize) * 4;
                            if doff < 20 {
                                rearm_rx(idx);
                                spins += 1;
                                continue;
                            }
                            let po = th + doff;
                            let flags = f[th + 13];
                            if flags & 0x04 != 0 {
                                rearm_rx(idx);
                                break;
                            }
                            let seq = ((f[th + 4] as u32) << 24)
                                | ((f[th + 5] as u32) << 16)
                                | ((f[th + 6] as u32) << 8)
                                | f[th + 7] as u32;
                            // Payload length from IP Total Length, NOT frame length
                            // (frame may include Ethernet padding / trailing junk).
                            let ip_total =
                                ((f[16] as usize) << 8) | (f[17] as usize);
                            let tcp_seg = if ip_total > ihl {
                                ip_total - ihl
                            } else {
                                0
                            };
                            let plen_usize = if tcp_seg > doff {
                                tcp_seg - doff
                            } else {
                                0
                            };
                            // Clamp to what we actually have in the buffer
                            let max_have = if len > po { len - po } else { 0 };
                            let plen_usize = if plen_usize > max_have {
                                max_have
                            } else {
                                plen_usize
                            };
                            let plen = plen_usize as u32;
                            if plen_usize > 0 {
                                let mut i = 0usize;
                                while i < plen_usize && written < out.len() {
                                    out[written] = f[po + i];
                                    written += 1;
                                    i += 1;
                                }
                                ack_n = seq.wrapping_add(plen);
                                let al = build_tcp_frame(&mut pkt, ip, iss, ack_n, 0x10, &[]);
                                let _ = tx_frame(&pkt[..al]);
                            }
                            if flags & 0x01 != 0 {
                                // FIN - ACK it and done
                                if plen == 0 {
                                    ack_n = seq.wrapping_add(1);
                                    let al = build_tcp_frame(&mut pkt, ip, iss, ack_n, 0x10, &[]);
                                    let _ = tx_frame(&pkt[..al]);
                                }
                                got_fin = true;
                                rearm_rx(idx);
                                break;
                            }
                        }
                    }
                }
                rearm_rx(idx);
            }
            core::hint::spin_loop();
            spins += 1;
        }
        if written == 0 {
            LAST_ERR = 23;
        } else {
            LAST_ERR = 0;
        }
        written
    }
}

pub fn init() {
    unsafe {
        ETH_FOUND = false;
        WIFI_FOUND = false;
        READY = false;
        LINK_UP = false;
        ETH_MAC_OK = false;
        ETH_RTL = false;
        ETH_MMIO = 0;
        DMA_READY = false;
        DHCP_OK = false;
        let mut best_rtl = false;
        let mut saved = false;

        for bus in 0u8..=31 {
            for dev in 0u8..32 {
                for func in 0u8..8 {
                    let id = pci_r32(bus, dev, func, 0);
                    if id == 0xFFFF_FFFF || id == 0 {
                        continue;
                    }
                    let vid = (id & 0xFFFF) as u16;
                    let did = ((id >> 16) & 0xFFFF) as u16;
                    let cr = pci_r32(bus, dev, func, 0x08);
                    let class = ((cr >> 24) & 0xFF) as u8;
                    let sub = ((cr >> 16) & 0xFF) as u8;
                    if class != 0x02 {
                        continue;
                    }
                    let cmd = (pci_r32(bus, dev, func, 0x04) & 0xFFFF) as u16;
                    pci_w16(bus, dev, func, 0x04, cmd | 0x0006); // MEM+BM
                    let mut bar0 = pci_r32(bus, dev, func, 0x10) as u64;
                    // Prefer BAR2 for some RTL (MMIO); try BAR0 first then BAR2
                    if (bar0 & 1) != 0 {
                        // I/O BAR - try BAR2
                        bar0 = pci_r32(bus, dev, func, 0x18) as u64;
                    }
                    if bar0 & 0x4 != 0 {
                        let bar1 = pci_r32(bus, dev, func, 0x14) as u64;
                        bar0 = (bar0 & !0xF) | (bar1 << 32);
                    } else {
                        bar0 &= !0xF;
                    }
                    // AH532 often has MMIO at BAR2
                    if bar0 == 0 {
                        let b2 = pci_r32(bus, dev, func, 0x18) as u64;
                        if (b2 & 1) == 0 {
                            bar0 = b2 & !0xF;
                        }
                    }

                    if sub == 0x00 {
                        let is_rtl = vid == 0x10EC
                            && (did == 0x8168 || did == 0x8169 || did == 0x8136 || did == 0x8129);
                        if !saved || (is_rtl && !best_rtl) {
                            ETH_FOUND = true;
                            ETH_BUS = bus;
                            ETH_DEV = dev;
                            ETH_FUNC = func;
                            ETH_VID = vid;
                            ETH_DID = did;
                            ETH_BAR0 = bar0;
                            ETH_RTL = is_rtl;
                            saved = true;
                            if is_rtl {
                                best_rtl = true;
                            }
                        }
                        serial::write_str("[NET] ETH ");
                        serial::write_usize(bus as usize);
                        serial::write_str(":");
                        serial::write_usize(dev as usize);
                        serial::write_str(".");
                        serial::write_usize(func as usize);
                        serial::write_str(" VID=");
                        serial::write_hex(vid as usize);
                        serial::write_str(" DID=");
                        serial::write_hex(did as usize);
                        serial::write_str(" BAR=");
                        serial::write_hex(bar0 as usize);
                        if is_rtl {
                            serial::write_str(" RTL81xx\n");
                        } else {
                            serial::write_str("\n");
                        }
                    } else if sub == 0x80 {
                        WIFI_FOUND = true;
                        WIFI_VID = vid;
                        WIFI_DID = did;
                    }
                }
            }
        }

        if ETH_FOUND && ETH_BAR0 != 0 {
            if map_mmio(ETH_BAR0, 0x1000) {
                ETH_MMIO = ETH_BAR0 as usize;
                if ETH_RTL {
                    if rtl_init_and_mac(ETH_MMIO) {
                        READY = true;
                    }
                } else {
                    READY = true;
                }
            }
        }
        serial::write_str(if READY {
            "[NET] READY\n"
        } else {
            "[NET] NOT READY\n"
        });
    }
}
