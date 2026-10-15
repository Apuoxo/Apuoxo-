//! Minimal crypto for TLS 1.2 (AES-128-GCM, SHA-256, HMAC, X25519).

pub mod sha256;
pub mod aes;
pub mod gcm;
pub mod x25519;

use sha256::{sha256, Sha256};

pub fn hmac_sha256(key: &[u8], data: &[u8], out: &mut [u8; 32]) {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let h = sha256(key);
        k[..32].copy_from_slice(&h);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut s = Sha256::new();
    s.update(&ipad);
    s.update(data);
    let inner = s.finish();
    let mut s2 = Sha256::new();
    s2.update(&opad);
    s2.update(&inner);
    let mac = s2.finish();
    out.copy_from_slice(&mac);
}

/// TLS 1.2 PRF (SHA256): P_hash(secret, label || seed)
pub fn prf_sha256(secret: &[u8], label: &[u8], seed: &[u8], out: &mut [u8]) {
    let mut a = [0u8; 32];
    // A(1) = HMAC(secret, label || seed)
    let mut tmp = [0u8; 128];
    let mut tl = 0usize;
    for &b in label {
        if tl < tmp.len() {
            tmp[tl] = b;
            tl += 1;
        }
    }
    for &b in seed {
        if tl < tmp.len() {
            tmp[tl] = b;
            tl += 1;
        }
    }
    hmac_sha256(secret, &tmp[..tl], &mut a);
    let mut offset = 0usize;
    while offset < out.len() {
        // HMAC(secret, A(i) || label || seed)
        let mut buf = [0u8; 160];
        let mut p = 0usize;
        for &b in &a {
            buf[p] = b;
            p += 1;
        }
        for &b in label {
            buf[p] = b;
            p += 1;
        }
        for &b in seed {
            buf[p] = b;
            p += 1;
        }
        let mut block = [0u8; 32];
        hmac_sha256(secret, &buf[..p], &mut block);
        let n = core::cmp::min(32, out.len() - offset);
        out[offset..offset + n].copy_from_slice(&block[..n]);
        offset += n;
        // A(i+1) = HMAC(secret, A(i))
        hmac_sha256(secret, &a, &mut a);
    }
}
