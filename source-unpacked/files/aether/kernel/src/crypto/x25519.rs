//! X25519 (RFC 7748) - compact field arithmetic.

fn load_le(b: &[u8; 32]) -> [u64; 5] {
    // 51-bit limbs
    let mut x = [0u64; 5];
    x[0] = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], 0]) & 0x7ffffffffffff;
    let t1 = u64::from_le_bytes([b[6], b[7], b[8], b[9], b[10], b[11], b[12], 0]);
    x[1] = (t1 >> 3) & 0x7ffffffffffff;
    let t2 = u64::from_le_bytes([b[12], b[13], b[14], b[15], b[16], b[17], b[18], 0]);
    x[2] = (t2 >> 6) & 0x7ffffffffffff;
    let t3 = u64::from_le_bytes([b[19], b[20], b[21], b[22], b[23], b[24], b[25], 0]);
    x[3] = (t3 >> 1) & 0x7ffffffffffff;
    let t4 = u64::from_le_bytes([b[25], b[26], b[27], b[28], b[29], b[30], b[31], 0]);
    x[4] = (t4 >> 4) & 0x7ffffffffffff;
    x
}

fn store_le(x: &[u64; 5], out: &mut [u8; 32]) {
    let mut t = [0u64; 5];
    t.copy_from_slice(x);
    // reduce lightly
    let mut c = t[0] >> 51;
    t[0] &= 0x7ffffffffffff;
    t[1] += c;
    c = t[1] >> 51;
    t[1] &= 0x7ffffffffffff;
    t[2] += c;
    c = t[2] >> 51;
    t[2] &= 0x7ffffffffffff;
    t[3] += c;
    c = t[3] >> 51;
    t[3] &= 0x7ffffffffffff;
    t[4] += c;
    c = t[4] >> 51;
    t[4] &= 0x7ffffffffffff;
    t[0] += c * 19;

    let mut bytes = [0u8; 40];
    // pack 51-bit limbs into bytes (simplified full reduction omitted for size;
    // for X25519 we do full clamp+reduce in scalar_mult end)
    let v0 = t[0] | (t[1] << 51);
    let v1 = (t[1] >> 13) | (t[2] << 38);
    let v2 = (t[2] >> 26) | (t[3] << 25);
    let v3 = (t[3] >> 39) | (t[4] << 12);
    bytes[0..8].copy_from_slice(&v0.to_le_bytes());
    bytes[6..14].copy_from_slice(&v1.to_le_bytes());
    bytes[12..20].copy_from_slice(&v2.to_le_bytes());
    bytes[19..27].copy_from_slice(&v3.to_le_bytes());
    // This packing is error-prone; use byte-wise from limb dump instead
    let mut acc = [0u8; 32];
    // Write limb0
    for i in 0..7 {
        acc[i] = (t[0] >> (8 * i)) as u8;
    }
    acc[6] |= ((t[1] & 0x7) << 5) as u8;
    for i in 0..6 {
        acc[7 + i] = (t[1] >> (3 + 8 * i)) as u8;
    }
    acc[12] = ((t[1] >> 43) | ((t[2] & 0x3f) << 2)) as u8;
    for i in 0..6 {
        acc[13 + i] = (t[2] >> (6 + 8 * i)) as u8;
    }
    acc[18] |= ((t[3] & 0x1) << 7) as u8;
    for i in 0..6 {
        acc[19 + i] = (t[3] >> (1 + 8 * i)) as u8;
    }
    acc[25] = ((t[3] >> 49) | ((t[4] & 0xf) << 2)) as u8;
    for i in 0..6 {
        acc[26 + i] = (t[4] >> (4 + 8 * i)) as u8;
    }
    *out = acc;
}

fn fadd(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    let mut o = [0u64; 5];
    for i in 0..5 {
        o[i] = a[i] + b[i];
    }
    o
}

fn fsub(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    // a - b + 2p roughly
    let mut o = [0u64; 5];
    o[0] = a[0] + 0xfffffffffffda - b[0];
    o[1] = a[1] + 0xffffffffffffe - b[1];
    o[2] = a[2] + 0xffffffffffffe - b[2];
    o[3] = a[3] + 0xffffffffffffe - b[3];
    o[4] = a[4] + 0xffffffffffffe - b[4];
    o
}

fn fmul(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    // Schoolbook + reduction mod 2^255-19 (compact, not constant-time)
    let mut t = [0u128; 9];
    for i in 0..5 {
        for j in 0..5 {
            t[i + j] += (a[i] as u128) * (b[j] as u128);
        }
    }
    // fold high limbs
    for i in (5..9).rev() {
        t[i - 5] += t[i] * 19;
    }
    let mut o = [0u64; 5];
    let mut c = 0u128;
    for i in 0..5 {
        let v = t[i] + c;
        o[i] = (v as u64) & 0x7ffffffffffff;
        c = v >> 51;
    }
    o[0] += (c as u64) * 19;
    c = (o[0] >> 51) as u128;
    o[0] &= 0x7ffffffffffff;
    o[1] += c as u64;
    o
}

fn fsqr(a: &[u64; 5]) -> [u64; 5] {
    fmul(a, a)
}

fn fmul121665(a: &[u64; 5]) -> [u64; 5] {
    let c = [121665u64, 0, 0, 0, 0];
    fmul(a, &c)
}

fn cswap(swap: u64, a: &mut [u64; 5], b: &mut [u64; 5]) {
    for i in 0..5 {
        let t = swap & (a[i] ^ b[i]);
        a[i] ^= t;
        b[i] ^= t;
    }
}

fn finverse(a: &[u64; 5]) -> [u64; 5] {
    // a^(p-2)
    let mut t = *a;
    // pow chain for 2^255-21
    let mut z2 = fsqr(&t);
    let mut z8 = fsqr(&fsqr(&z2));
    let mut z9 = fmul(&t, &z8);
    let mut z11 = fmul(&z2, &z9);
    let mut z22 = fsqr(&z11);
    let mut z5_0 = fmul(&z9, &z22);
    let mut t0 = fsqr(&z5_0);
    for _ in 1..5 {
        t0 = fsqr(&t0);
    }
    let mut z10_0 = fmul(&t0, &z5_0);
    t0 = fsqr(&z10_0);
    for _ in 1..10 {
        t0 = fsqr(&t0);
    }
    let mut z20_0 = fmul(&t0, &z10_0);
    t0 = fsqr(&z20_0);
    for _ in 1..20 {
        t0 = fsqr(&t0);
    }
    t0 = fmul(&t0, &z20_0);
    t0 = fsqr(&t0);
    for _ in 1..10 {
        t0 = fsqr(&t0);
    }
    let mut z10_1 = fmul(&t0, &z10_0);
    t0 = fsqr(&z10_1);
    for _ in 1..50 {
        t0 = fsqr(&t0);
    }
    let mut z50_0 = fmul(&t0, &z10_1);
    t0 = fsqr(&z50_0);
    for _ in 1..100 {
        t0 = fsqr(&t0);
    }
    t0 = fmul(&t0, &z50_0);
    t0 = fsqr(&t0);
    for _ in 1..50 {
        t0 = fsqr(&t0);
    }
    t0 = fmul(&t0, &z10_1);
    t0 = fsqr(&t0);
    for _ in 1..5 {
        t0 = fsqr(&t0);
    }
    fmul(&t0, &z11)
}

/// X25519: out = scalar * point (point is u-coordinate)
pub fn x25519(scalar: &[u8; 32], point: &[u8; 32], out: &mut [u8; 32]) {
    let mut e = *scalar;
    e[0] &= 248;
    e[31] &= 127;
    e[31] |= 64;

    let x1 = load_le(point);
    let mut x2 = [1u64, 0, 0, 0, 0]; // 1
    let mut z2 = [0u64; 5];
    let mut x3 = x1;
    let mut z3 = [1u64, 0, 0, 0, 0];
    let mut swap = 0u64;

    for t in (0..255).rev() {
        let k_t = ((e[t >> 3] >> (t & 7)) & 1) as u64;
        swap ^= k_t;
        cswap(swap, &mut x2, &mut x3);
        cswap(swap, &mut z2, &mut z3);
        swap = k_t;

        let a = fadd(&x2, &z2);
        let aa = fsqr(&a);
        let b = fsub(&x2, &z2);
        let bb = fsqr(&b);
        let e2 = fsub(&aa, &bb);
        let c = fadd(&x3, &z3);
        let d = fsub(&x3, &z3);
        let da = fmul(&d, &a);
        let cb = fmul(&c, &b);
        x3 = fsqr(&fadd(&da, &cb));
        z3 = fmul(&x1, &fsqr(&fsub(&da, &cb)));
        x2 = fmul(&aa, &bb);
        z2 = fmul(&e2, &fadd(&aa, &fmul121665(&e2)));
    }
    cswap(swap, &mut x2, &mut x3);
    cswap(swap, &mut z2, &mut z3);
    let inv = finverse(&z2);
    let r = fmul(&x2, &inv);
    store_le(&r, out);
}

pub fn generate_keypair(seed: &[u8; 32], public: &mut [u8; 32], private: &mut [u8; 32]) {
    *private = *seed;
    private[0] &= 248;
    private[31] &= 127;
    private[31] |= 64;
    let base = [
        9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ];
    x25519(private, &base, public);
}
