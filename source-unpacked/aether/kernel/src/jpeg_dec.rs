//! Baseline JPEG decoder for MJPEG (SOF0, 8-bit, YCbCr 4:2:0 / 4:2:2 / 4:4:4).
//! Goals: correct full-frame colour, fast Huffman, usable up to ~1280×720 on CPU.

const ZZ: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52,
    45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Fast Huffman: 8-bit LUT + slow path for longer codes.
#[derive(Clone, Copy)]
struct HuffTab {
    /// lut_val[b] / lut_len[b]: if lut_len[b] != 0, symbol for top bits of b
    lut_val: [u8; 256],
    lut_len: [u8; 256],
    /// slow entries for len > 8
    n: usize,
    code: [u16; 256],
    len: [u8; 256],
    val: [u8; 256],
}

impl HuffTab {
    const fn empty() -> Self {
        Self {
            lut_val: [0; 256],
            lut_len: [0; 256],
            n: 0,
            code: [0; 256],
            len: [0; 256],
            val: [0; 256],
        }
    }
}

fn build_huff(bits: &[u8; 16], vals: &[u8], nvals: usize) -> HuffTab {
    let mut h = HuffTab::empty();
    let mut code: u32 = 0;
    let mut k = 0usize;
    let mut i = 0usize;
    while i < 16 {
        let mut j = 0u8;
        while j < bits[i] {
            if k < nvals {
                let clen = (i + 1) as u8;
                let c = code as u16;
                let v = vals[k];
                if clen <= 8 {
                    // fill all 8-bit prefixes
                    let shift = 8 - clen;
                    let base = (c as usize) << shift;
                    let count = 1usize << shift;
                    let mut t = 0usize;
                    while t < count {
                        let idx = base + t;
                        if idx < 256 {
                            h.lut_val[idx] = v;
                            h.lut_len[idx] = clen;
                        }
                        t += 1;
                    }
                } else if h.n < 256 {
                    h.code[h.n] = c;
                    h.len[h.n] = clen;
                    h.val[h.n] = v;
                    h.n += 1;
                }
                k += 1;
            }
            code += 1;
            j += 1;
        }
        code <<= 1;
        i += 1;
    }
    h
}

// MJPEG often omits DHT/DQT on later frames
static mut CACHE_VALID: bool = false;
static mut CACHE_Q: [[u8; 64]; 4] = [[16; 64]; 4];
static mut CACHE_HDC: [HuffTab; 4] = [HuffTab::empty(); 4];
static mut CACHE_HAC: [HuffTab; 4] = [HuffTab::empty(); 4];

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    nbits: i32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            buf: 0,
            nbits: 0,
        }
    }
    #[inline]
    fn pull_byte(&mut self) -> Option<u8> {
        if self.pos >= self.data.len() {
            return None;
        }
        let mut b = self.data[self.pos];
        self.pos += 1;
        if b == 0xFF {
            if self.pos < self.data.len() && self.data[self.pos] == 0x00 {
                self.pos += 1;
            } else {
                self.pos = self.data.len();
                return None;
            }
        }
        Some(b)
    }
    #[inline]
    fn ensure(&mut self, n: i32) {
        while self.nbits < n {
            match self.pull_byte() {
                Some(b) => {
                    self.buf = (self.buf << 8) | (b as u32);
                    self.nbits += 8;
                }
                None => break,
            }
        }
    }
    #[inline]
    fn peek8(&mut self) -> u32 {
        self.ensure(8);
        if self.nbits < 8 {
            return 0;
        }
        (self.buf >> (self.nbits - 8)) & 0xFF
    }
    #[inline]
    fn consume(&mut self, n: i32) {
        self.nbits -= n;
        if self.nbits < 0 {
            self.nbits = 0;
        }
    }
    #[inline]
    fn get_bits(&mut self, n: i32) -> i32 {
        if n <= 0 {
            return 0;
        }
        self.ensure(n);
        if self.nbits < n {
            return 0;
        }
        self.nbits -= n;
        ((self.buf >> self.nbits) & ((1u32 << n) - 1)) as i32
    }
}

#[inline]
fn huff_decode(br: &mut BitReader, h: &HuffTab) -> u8 {
    let b = br.peek8() as usize;
    let l = h.lut_len[b];
    if l != 0 {
        br.consume(l as i32);
        return h.lut_val[b];
    }
    // slow path: codes longer than 8
    let mut code: u32 = br.peek8();
    br.consume(8);
    let mut len = 8u8;
    while len < 16 {
        code = (code << 1) | (br.get_bits(1) as u32);
        len += 1;
        let mut i = 0usize;
        while i < h.n {
            if h.len[i] == len && h.code[i] as u32 == code {
                return h.val[i];
            }
            i += 1;
        }
    }
    0
}

#[inline]
fn receive_extend(br: &mut BitReader, s: i32) -> i32 {
    if s == 0 {
        return 0;
    }
    let v = br.get_bits(s);
    let vt = 1 << (s - 1);
    if v < vt {
        v + (((-1i32) << s) + 1)
    } else {
        v
    }
}

/// Integer inverse DCT (IJG-style fixed-point, separable).
fn idct8(blk: &mut [i32; 64]) {
    // Constants: FIX(x) = x * 1<<8
    const C1: i32 = 251; // 0.9808
    const C2: i32 = 237; // 0.9239
    const C3: i32 = 213; // 0.8315
    const C4: i32 = 181; // 0.7071
    const C5: i32 = 142; // 0.5556
    const C6: i32 = 98;  // 0.3827
    const C7: i32 = 50;  // 0.1951

    let mut tmp = [0i32; 64];

    // Pass 1: process rows
    let mut y = 0usize;
    while y < 8 {
        let o = y * 8;
        // Short-circuit pure DC
        if blk[o + 1] == 0
            && blk[o + 2] == 0
            && blk[o + 3] == 0
            && blk[o + 4] == 0
            && blk[o + 5] == 0
            && blk[o + 6] == 0
            && blk[o + 7] == 0
        {
            let dc = blk[o] << 3;
            let mut x = 0usize;
            while x < 8 {
                tmp[o + x] = dc;
                x += 1;
            }
            y += 1;
            continue;
        }

        let p0 = blk[o];
        let p1 = blk[o + 4];
        let p2 = blk[o + 2];
        let p3 = blk[o + 6];
        let p4 = blk[o + 5];
        let p5 = blk[o + 1];
        let p6 = blk[o + 7];
        let p7 = blk[o + 3];

        // stage 1
        let mut z0 = (p0 + p1) << 8;
        let mut z1 = (p0 - p1) << 8;
        let mut z2 = p2 * C6 + p3 * C2;
        let mut z3 = p2 * C2 - p3 * C6;

        let t0 = z0 + z3;
        let t1 = z1 + z2;
        let t2 = z1 - z2;
        let t3 = z0 - z3;

        z0 = p4 + p7;
        z1 = p5 + p6;
        z2 = p5 - p6;
        z3 = p4 - p7;

        let t4 = (z0 + z1) * C4;
        let t7 = (z0 - z1) * C4;
        let t5 = z2 * C2 + z3 * C6;
        let t6 = z3 * C2 - z2 * C6;

        tmp[o] = (t0 + t4) >> 8;
        tmp[o + 7] = (t0 - t4) >> 8;
        tmp[o + 1] = (t1 + t6) >> 8;
        tmp[o + 6] = (t1 - t6) >> 8;
        tmp[o + 2] = (t2 + t7) >> 8;
        tmp[o + 5] = (t2 - t7) >> 8;
        tmp[o + 3] = (t3 + t5) >> 8;
        tmp[o + 4] = (t3 - t5) >> 8;
        y += 1;
    }

    // Pass 2: process columns + descaling
    let mut x = 0usize;
    while x < 8 {
        if tmp[x + 8] == 0
            && tmp[x + 16] == 0
            && tmp[x + 24] == 0
            && tmp[x + 32] == 0
            && tmp[x + 40] == 0
            && tmp[x + 48] == 0
            && tmp[x + 56] == 0
        {
            let dc = (tmp[x] + 4) >> 3;
            let mut y = 0usize;
            while y < 8 {
                blk[y * 8 + x] = dc;
                y += 1;
            }
            x += 1;
            continue;
        }

        let p0 = tmp[x];
        let p1 = tmp[x + 32];
        let p2 = tmp[x + 16];
        let p3 = tmp[x + 48];
        let p4 = tmp[x + 40];
        let p5 = tmp[x + 8];
        let p6 = tmp[x + 56];
        let p7 = tmp[x + 24];

        let mut z0 = (p0 + p1) << 8;
        let mut z1 = (p0 - p1) << 8;
        let mut z2 = p2 * C6 + p3 * C2;
        let mut z3 = p2 * C2 - p3 * C6;

        let t0 = z0 + z3;
        let t1 = z1 + z2;
        let t2 = z1 - z2;
        let t3 = z0 - z3;

        z0 = p4 + p7;
        z1 = p5 + p6;
        z2 = p5 - p6;
        z3 = p4 - p7;

        let t4 = (z0 + z1) * C4;
        let t7 = (z0 - z1) * C4;
        let t5 = z2 * C2 + z3 * C6;
        let t6 = z3 * C2 - z2 * C6;

        // +4 then >>3 for final descaling (<<8 from pass1, *C ~<<8 → total ~>>11; use >>11)
        blk[x] = (t0 + t4) >> 11;
        blk[x + 56] = (t0 - t4) >> 11;
        blk[x + 8] = (t1 + t6) >> 11;
        blk[x + 48] = (t1 - t6) >> 11;
        blk[x + 16] = (t2 + t7) >> 11;
        blk[x + 40] = (t2 - t7) >> 11;
        blk[x + 24] = (t3 + t5) >> 11;
        blk[x + 32] = (t3 - t5) >> 11;
        x += 1;
    }
    let _ = (C1, C3, C5, C7);
}

fn clamp(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

/// Decode baseline JPEG → out RGB 0x00RRGGBB, size ≤ max_w×max_h.
pub fn decode_rgb(data: &[u8], out: &mut [u32], max_w: usize, max_h: usize) -> Option<(usize, usize)> {
    // find SOI
    let mut start = 0usize;
    while start + 1 < data.len() {
        if data[start] == 0xFF && data[start + 1] == 0xD8 {
            break;
        }
        start += 1;
    }
    if start + 100 >= data.len() {
        return None;
    }
    let data = &data[start..];

    let mut qtab = [[16u8; 64]; 4];
    let mut hdc = [HuffTab::empty(); 4];
    let mut hac = [HuffTab::empty(); 4];
    let mut w = 0usize;
    let mut h = 0usize;
    let mut ncomp = 0usize;
    let mut cid = [0u8; 3];
    let mut hs = [1u8; 3];
    let mut vs = [1u8; 3];
    let mut qt = [0u8; 3];
    let mut scan_c = [0u8; 3];
    let mut scan_dc = [0u8; 3];
    let mut scan_ac = [0u8; 3];
    let mut nscan = 0usize;
    let mut ent_off = 0usize;

    let mut i = 2usize;
    while i + 3 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        while i < data.len() && data[i] == 0xFF {
            i += 1;
        }
        if i >= data.len() {
            break;
        }
        let m = data[i];
        i += 1;
        if m == 0xD8 {
            continue;
        }
        if m == 0xD9 {
            break;
        }
        if m == 0x01 || (m >= 0xD0 && m <= 0xD7) {
            continue;
        }
        if i + 1 >= data.len() {
            break;
        }
        let seglen = ((data[i] as usize) << 8) | (data[i + 1] as usize);
        if seglen < 2 || i + seglen > data.len() {
            break;
        }
        let seg = &data[i + 2..i + seglen];
        let next = i + seglen;

        match m {
            0xDB => {
                let mut p = 0usize;
                while p < seg.len() {
                    let info = seg[p];
                    p += 1;
                    let pq = info >> 4;
                    let tq = (info & 15) as usize;
                    if tq > 3 {
                        break;
                    }
                    if pq != 0 {
                        if p + 128 > seg.len() {
                            break;
                        }
                        p += 128;
                        continue;
                    }
                    if p + 64 > seg.len() {
                        break;
                    }
                    let mut k = 0usize;
                    while k < 64 {
                        qtab[tq][ZZ[k]] = seg[p + k];
                        k += 1;
                    }
                    p += 64;
                }
            }
            0xC0 | 0xC1 => {
                if seg.len() < 6 {
                    return None;
                }
                h = ((seg[1] as usize) << 8) | seg[2] as usize;
                w = ((seg[3] as usize) << 8) | seg[4] as usize;
                ncomp = seg[5] as usize;
                if ncomp < 1 || ncomp > 3 || seg.len() < 6 + 3 * ncomp {
                    return None;
                }
                let mut c = 0usize;
                while c < ncomp {
                    cid[c] = seg[6 + c * 3];
                    hs[c] = seg[7 + c * 3] >> 4;
                    vs[c] = seg[7 + c * 3] & 15;
                    qt[c] = seg[8 + c * 3] & 3;
                    c += 1;
                }
            }
            0xC2 => {
                // progressive - not supported
                return None;
            }
            0xC4 => {
                let mut p = 0usize;
                while p + 17 <= seg.len() {
                    let tc = seg[p] >> 4;
                    let th = (seg[p] & 15) as usize;
                    p += 1;
                    let mut bits = [0u8; 16];
                    let mut nv = 0usize;
                    let mut b = 0usize;
                    while b < 16 {
                        bits[b] = seg[p + b];
                        nv += bits[b] as usize;
                        b += 1;
                    }
                    p += 16;
                    if p + nv > seg.len() || nv > 256 {
                        break;
                    }
                    let mut vals = [0u8; 256];
                    let mut v = 0usize;
                    while v < nv {
                        vals[v] = seg[p + v];
                        v += 1;
                    }
                    p += nv;
                    if th < 4 {
                        let tab = build_huff(&bits, &vals, nv);
                        if tc == 0 {
                            hdc[th] = tab;
                        } else {
                            hac[th] = tab;
                        }
                    }
                }
            }
            0xDA => {
                if seg.len() < 4 {
                    return None;
                }
                nscan = seg[0] as usize;
                if nscan < 1 || nscan > 3 || seg.len() < 1 + 2 * nscan {
                    return None;
                }
                let mut s = 0usize;
                while s < nscan {
                    scan_c[s] = seg[1 + s * 2];
                    scan_dc[s] = seg[2 + s * 2] >> 4;
                    scan_ac[s] = seg[2 + s * 2] & 15;
                    s += 1;
                }
                ent_off = next;
                break;
            }
            _ => {}
        }
        i = next;
    }

    // Apply Huffman/quant cache for MJPEG frames without tables
    unsafe {
        let mut any = false;
        let mut hi = 0usize;
        while hi < 4 {
            if hdc[hi].lut_len.iter().any(|&x| x != 0) || hdc[hi].n > 0 || hac[hi].n > 0
                || hac[hi].lut_len.iter().any(|&x| x != 0)
            {
                any = true;
                break;
            }
            hi += 1;
        }
        if any {
            CACHE_VALID = true;
            let mut hi = 0usize;
            while hi < 4 {
                CACHE_HDC[hi] = hdc[hi];
                CACHE_HAC[hi] = hac[hi];
                CACHE_Q[hi] = qtab[hi];
                hi += 1;
            }
        } else if CACHE_VALID {
            let mut hi = 0usize;
            while hi < 4 {
                hdc[hi] = CACHE_HDC[hi];
                hac[hi] = CACHE_HAC[hi];
                qtab[hi] = CACHE_Q[hi];
                hi += 1;
            }
        }
    }

    if w == 0 || h == 0 || ent_off == 0 || ncomp == 0 || nscan == 0 {
        return None;
    }
    if w > 1920 || h > 1200 {
        return None;
    }

    let mut hmax = 1u8;
    let mut vmax = 1u8;
    let mut c = 0usize;
    while c < ncomp {
        if hs[c] > hmax {
            hmax = hs[c];
        }
        if vs[c] > vmax {
            vmax = vs[c];
        }
        c += 1;
    }
    if hmax == 0 || vmax == 0 {
        return None;
    }
    let mcu_w = (hmax as usize) * 8;
    let mcu_h = (vmax as usize) * 8;
    let nx = (w + mcu_w - 1) / mcu_w;
    let ny = (h + mcu_h - 1) / mcu_h;

    // Display plane: full-frame scaled into PW×PH (RGB later)
    const PW: usize = 320;
    const PH: usize = 240;
    let mut ypl = [0u8; PW * PH];
    let mut cbpl = [128u8; PW * PH];
    let mut crpl = [128u8; PW * PH];

    let mut br = BitReader::new(&data[ent_off..]);
    let mut dcpred = [0i32; 3];

    let mut my = 0usize;
    while my < ny {
        let mut mx = 0usize;
        while mx < nx {
            let mut ci = 0usize;
            while ci < ncomp {
                let mut si = 0usize;
                while si < nscan {
                    if scan_c[si] == cid[ci] {
                        break;
                    }
                    si += 1;
                }
                if si >= nscan {
                    si = ci.min(nscan - 1);
                }
                let dci = (scan_dc[si] & 3) as usize;
                let aci = (scan_ac[si] & 3) as usize;
                let q = &qtab[(qt[ci] & 3) as usize];
                let du_h = hs[ci] as usize;
                let du_v = vs[ci] as usize;
                // How many image pixels one sample of this component covers
                let sx = (hmax as usize) / du_h.max(1);
                let sy = (vmax as usize) / du_v.max(1);

                let mut by = 0usize;
                while by < du_v {
                    let mut bx = 0usize;
                    while bx < du_h {
                        let mut block = [0i32; 64];
                        let t = huff_decode(&mut br, &hdc[dci]) as i32;
                        let diff = receive_extend(&mut br, t);
                        dcpred[ci] = dcpred[ci].wrapping_add(diff);
                        block[0] = dcpred[ci] * (q[0] as i32);
                        let mut k = 1usize;
                        while k < 64 {
                            let rs = huff_decode(&mut br, &hac[aci]);
                            let rrr = (rs >> 4) as usize;
                            let ssss = (rs & 15) as i32;
                            if ssss == 0 {
                                if rrr == 15 {
                                    k += 16;
                                    continue;
                                }
                                break;
                            }
                            k += rrr;
                            if k >= 64 {
                                break;
                            }
                            let zz = ZZ[k];
                            block[zz] = receive_extend(&mut br, ssss) * (q[zz] as i32);
                            k += 1;
                        }
                        idct8(&mut block);

                        // Component DU origin in component samples
                        let cox = (mx * du_h + bx) * 8;
                        let coy = (my * du_v + by) * 8;

                        let mut yy = 0usize;
                        while yy < 8 {
                            let mut xx = 0usize;
                            while xx < 8 {
                                let val = clamp(block[yy * 8 + xx] + 128);
                                // Map component sample → image pixel region
                                let img_x0 = (cox + xx) * sx;
                                let img_y0 = (coy + yy) * sy;
                                // Write into all image pixels this sample covers (upsample chroma)
                                let mut iy = 0usize;
                                while iy < sy {
                                    let mut ix = 0usize;
                                    while ix < sx {
                                        let ixp = img_x0 + ix;
                                        let iyp = img_y0 + iy;
                                        if ixp < w && iyp < h {
                                            let px = ixp * PW / w;
                                            let py = iyp * PH / h;
                                            if px < PW && py < PH {
                                                let idx = py * PW + px;
                                                if ci == 0 {
                                                    ypl[idx] = val;
                                                } else if ci == 1 {
                                                    cbpl[idx] = val;
                                                } else {
                                                    crpl[idx] = val;
                                                }
                                            }
                                        }
                                        ix += 1;
                                    }
                                    iy += 1;
                                }
                                xx += 1;
                            }
                            yy += 1;
                        }
                        bx += 1;
                    }
                    by += 1;
                }
                ci += 1;
            }
            mx += 1;
        }
        // Restart interval handling is rare in MJPEG - skip
        my += 1;
    }

    let dw = max_w.min(PW);
    let dh = max_h.min(PH);
    if out.len() < dw * dh {
        return None;
    }

    let mut y = 0usize;
    while y < dh {
        let sy = y * PH / dh.max(1);
        let mut x = 0usize;
        while x < dw {
            let sx = x * PW / dw.max(1);
            let yy = ypl[sy * PW + sx] as i32;
            let cb = cbpl[sy * PW + sx] as i32 - 128;
            let cr = crpl[sy * PW + sx] as i32 - 128;
            let r = clamp(yy + ((359 * cr) >> 8));
            let g = clamp(yy - ((88 * cb + 183 * cr) >> 8));
            let b = clamp(yy + ((454 * cb) >> 8));
            out[y * dw + x] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            x += 1;
        }
        y += 1;
    }
    Some((dw, dh))
}
