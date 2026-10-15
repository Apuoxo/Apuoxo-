//! AES-128-GCM encrypt/decrypt (TLS record size).

use super::aes::Aes128;

fn gf_mult(x: &[u8; 16], y: &[u8; 16]) -> [u8; 16] {
    let mut z = [0u8; 16];
    let mut v = *y;
    for i in 0..16 {
        for j in 0..8 {
            if x[i] & (1 << (7 - j)) != 0 {
                for k in 0..16 {
                    z[k] ^= v[k];
                }
            }
            let lsb = v[15] & 1;
            for k in (1..16).rev() {
                v[k] = (v[k] >> 1) | (v[k - 1] << 7);
            }
            v[0] >>= 1;
            if lsb != 0 {
                v[0] ^= 0xe1;
            }
        }
    }
    z
}

fn ghash(h: &[u8; 16], aad: &[u8], ct: &[u8]) -> [u8; 16] {
    let mut y = [0u8; 16];
    // process AAD
    let mut i = 0usize;
    while i + 16 <= aad.len() {
        let mut block = [0u8; 16];
        block.copy_from_slice(&aad[i..i + 16]);
        for k in 0..16 {
            y[k] ^= block[k];
        }
        y = gf_mult(&y, h);
        i += 16;
    }
    if i < aad.len() {
        let mut block = [0u8; 16];
        let n = aad.len() - i;
        block[..n].copy_from_slice(&aad[i..]);
        for k in 0..16 {
            y[k] ^= block[k];
        }
        y = gf_mult(&y, h);
    }
    // process ciphertext
    i = 0;
    while i + 16 <= ct.len() {
        let mut block = [0u8; 16];
        block.copy_from_slice(&ct[i..i + 16]);
        for k in 0..16 {
            y[k] ^= block[k];
        }
        y = gf_mult(&y, h);
        i += 16;
    }
    if i < ct.len() {
        let mut block = [0u8; 16];
        let n = ct.len() - i;
        block[..n].copy_from_slice(&ct[i..]);
        for k in 0..16 {
            y[k] ^= block[k];
        }
        y = gf_mult(&y, h);
    }
    // lengths
    let mut lenb = [0u8; 16];
    let al = (aad.len() as u64) * 8;
    let cl = (ct.len() as u64) * 8;
    for k in 0..8 {
        lenb[7 - k] = (al >> (8 * k)) as u8;
        lenb[15 - k] = (cl >> (8 * k)) as u8;
    }
    for k in 0..16 {
        y[k] ^= lenb[k];
    }
    gf_mult(&y, h)
}

pub struct AesGcm {
    aes: Aes128,
    h: [u8; 16],
}

impl AesGcm {
    pub fn new(key: &[u8; 16]) -> Self {
        let aes = Aes128::new(key);
        let mut h = [0u8; 16];
        aes.encrypt_block(&mut h);
        Self { aes, h }
    }

    fn ctr_block(&self, j0: &[u8; 16], counter: u32, out: &mut [u8; 16]) {
        *out = *j0;
        out[12] = (counter >> 24) as u8;
        out[13] = (counter >> 16) as u8;
        out[14] = (counter >> 8) as u8;
        out[15] = counter as u8;
        self.aes.encrypt_block(out);
    }

    /// Encrypt: returns ciphertext length written to ct_out, tag to tag_out.
    /// nonce is 12 bytes (TLS explicit nonce is 8, combined with fixed IV).
    pub fn encrypt(
        &self,
        nonce12: &[u8; 12],
        aad: &[u8],
        plain: &[u8],
        ct_out: &mut [u8],
        tag_out: &mut [u8; 16],
    ) -> usize {
        let mut j0 = [0u8; 16];
        j0[..12].copy_from_slice(nonce12);
        j0[15] = 1; // for tag; counter starts at 2 for data

        let mut ctr = 2u32;
        let mut off = 0usize;
        while off < plain.len() {
            let mut keystream = [0u8; 16];
            self.ctr_block(&{
                let mut j = [0u8; 16];
                j[..12].copy_from_slice(nonce12);
                j
            }, ctr, &mut keystream);
            // fix ctr_block - j0 base with counter
            let mut base = [0u8; 16];
            base[..12].copy_from_slice(nonce12);
            self.ctr_block(&base, ctr, &mut keystream);
            let n = core::cmp::min(16, plain.len() - off);
            for i in 0..n {
                ct_out[off + i] = plain[off + i] ^ keystream[i];
            }
            off += n;
            ctr = ctr.wrapping_add(1);
        }
        let s = ghash(&self.h, aad, &ct_out[..plain.len()]);
        let mut t = [0u8; 16];
        let mut base = [0u8; 16];
        base[..12].copy_from_slice(nonce12);
        self.ctr_block(&base, 1, &mut t);
        for i in 0..16 {
            tag_out[i] = t[i] ^ s[i];
        }
        plain.len()
    }

    pub fn decrypt(
        &self,
        nonce12: &[u8; 12],
        aad: &[u8],
        ct: &[u8],
        tag: &[u8; 16],
        plain_out: &mut [u8],
    ) -> Option<usize> {
        let s = ghash(&self.h, aad, ct);
        let mut t = [0u8; 16];
        let mut base = [0u8; 16];
        base[..12].copy_from_slice(nonce12);
        self.ctr_block(&base, 1, &mut t);
        let mut expect = [0u8; 16];
        for i in 0..16 {
            expect[i] = t[i] ^ s[i];
        }
        let mut diff = 0u8;
        for i in 0..16 {
            diff |= expect[i] ^ tag[i];
        }
        if diff != 0 {
            return None;
        }
        let mut ctr = 2u32;
        let mut off = 0usize;
        while off < ct.len() {
            let mut keystream = [0u8; 16];
            self.ctr_block(&base, ctr, &mut keystream);
            let n = core::cmp::min(16, ct.len() - off);
            for i in 0..n {
                plain_out[off + i] = ct[off + i] ^ keystream[i];
            }
            off += n;
            ctr = ctr.wrapping_add(1);
        }
        Some(ct.len())
    }
}
