//! The inverse MDCT (stb_vorbis's inverse_mdct and its helpers), and the tables it needs.
//!
//! The following is stb_vorbis's hand-optimised version of the algorithm from "The use of
//! multirate filter banks for coding of high quality digital audio" (Sporer, Brandenburg,
//! Edler), with every float operation in the same order, so the samples come out the same.

use crate::codebook::{bit_reverse, ilog};
use crate::Pi;

/// stb_vorbis's own M_PI: `3.14159265358979323846264f`.
const PI: f32 = std::f32::consts::PI;

pub(crate) struct BlockTables {
    pub a: Vec<f32>,
    pub b: Vec<f32>,
    pub c: Vec<f32>,
    pub window: Vec<f32>,
    pub bit_reverse: Vec<u16>,
}

fn square(x: f32) -> f32 {
    x * x
}

impl BlockTables {
    /// stb_vorbis's init_blocksize for a block of `n` samples.
    pub fn new(n: usize, pi: Pi) -> BlockTables {
        let (n2, n4, n8) = (n >> 1, n >> 2, n >> 3);
        // The cosine and sine of `x * M_PI / n` and of `x * M_PI / n / 2` as the C library works
        // them out: in doubles where M_PI is one; in floats where it is stb_vorbis's own, which C
        // makes doubles for cos and sin, and C++ gives to the float cos and sin.
        let pi_over_n = |x: usize| x as f32 * PI / n as f32;
        let pi_over_2n = |x: usize| x as f32 * PI / n as f32 / 2.0;
        let cos_sin = |double: f64, float: f32| match pi {
            Pi::Double => (double.cos() as f32, double.sin() as f32),
            Pi::Float => (f64::from(float).cos() as f32, f64::from(float).sin() as f32),
            Pi::FloatCpp => (float.cos(), float.sin()),
        };
        let cos_sin_pi_over_n = |x: usize| cos_sin(x as f64 * std::f64::consts::PI / n as f64, pi_over_n(x));
        let cos_sin_pi_over_2n = |x: usize| cos_sin(x as f64 * std::f64::consts::PI / n as f64 / 2.0, pi_over_2n(x));
        let pi_as_double = match pi {
            Pi::Double => std::f64::consts::PI,
            Pi::Float | Pi::FloatCpp => f64::from(PI),
        };
        // compute_twiddle_factors
        let (mut a, mut b, mut c) = (vec![0.0f32; n2], vec![0.0f32; n2], vec![0.0f32; n4]);
        let mut k2 = 0usize;
        for k in 0..n4 {
            let (cos, sin) = cos_sin_pi_over_n(4 * k);
            a[k2] = cos;
            a[k2 + 1] = -sin;
            let (cos, sin) = cos_sin_pi_over_2n(k2 + 1);
            b[k2] = cos * 0.5;
            b[k2 + 1] = sin * 0.5;
            k2 += 2;
        }
        k2 = 0;
        for _ in 0..n8 {
            let (cos, sin) = cos_sin_pi_over_n(2 * (k2 + 1));
            c[k2] = cos;
            c[k2 + 1] = -sin;
            k2 += 2;
        }
        // compute_window
        let mut window = vec![0.0f32; n2];
        for (i, w) in window.iter_mut().enumerate() {
            let inner = ((i as f64 + 0.5) / n2 as f64 * 0.5 * pi_as_double).sin() as f32;
            *w = (0.5 * pi_as_double * f64::from(square(inner))).sin() as f32;
        }
        // compute_bitreverse
        let ld = ilog(n as i32) - 1; // ilog is off-by-one from normal definitions
        let mut reverse = vec![0u16; n8];
        for (i, r) in reverse.iter_mut().enumerate() {
            *r = ((bit_reverse(i as u32) >> (32 - ld + 3)) << 2) as u16;
        }
        BlockTables { a, b, c, window, bit_reverse: reverse }
    }
}

/// One of the algorithm's butterflies on `e[i0]`, `e[i0 - 1]` and `e[i2]`, `e[i2 - 1]`.
#[inline(always)]
fn butterfly(e: &mut [f32], i0: usize, i2: usize, a0: f32, a1: f32) {
    let k00_20 = e[i0] - e[i2];
    let k01_21 = e[i0 - 1] - e[i2 - 1];
    e[i0] += e[i2];
    e[i0 - 1] += e[i2 - 1];
    e[i2] = k00_20 * a0 - k01_21 * a1;
    e[i2 - 1] = k01_21 * a0 + k00_20 * a1;
}

// the following were split out into separate functions while optimizing;
// they could be pushed back up but eh. __forceinline showed no change;
// they're probably already being inlined.
fn imdct_step3_iter0_loop(n: usize, e: &mut [f32], i_off: usize, k_off: usize, a: &[f32]) {
    let mut ee0 = i_off;
    let mut ee2 = i_off - k_off;
    let mut ai = 0usize;
    for _ in 0..(n >> 2) {
        for step in [0usize, 2, 4, 6] {
            butterfly(e, ee0 - step, ee2 - step, a[ai], a[ai + 1]);
            ai += 8;
        }
        ee0 = ee0.wrapping_sub(8);
        ee2 = ee2.wrapping_sub(8);
    }
}

fn imdct_step3_inner_r_loop(lim: usize, e: &mut [f32], d0: usize, k_off: usize, a: &[f32], k1: usize) {
    let mut e0 = d0;
    let mut e2 = d0 - k_off;
    let mut ai = 0usize;
    for _ in 0..(lim >> 2) {
        for step in [0usize, 2, 4, 6] {
            butterfly(e, e0 - step, e2 - step, a[ai], a[ai + 1]);
            ai += k1;
        }
        e0 = e0.wrapping_sub(8);
        e2 = e2.wrapping_sub(8);
    }
}

fn imdct_step3_inner_s_loop(n: usize, e: &mut [f32], i_off: usize, k_off: usize, a: &[f32], a_off: usize, k0: usize) {
    let factors = [(a[0], a[1]), (a[a_off], a[a_off + 1]), (a[a_off * 2], a[a_off * 2 + 1]), (a[a_off * 3], a[a_off * 3 + 1])];
    let mut ee0 = i_off;
    let mut ee2 = i_off - k_off;
    for _ in 0..n {
        for (step, (a0, a1)) in [0usize, 2, 4, 6].into_iter().zip(factors) {
            butterfly(e, ee0 - step, ee2 - step, a0, a1);
        }
        ee0 = ee0.wrapping_sub(k0);
        ee2 = ee2.wrapping_sub(k0);
    }
}

#[inline(always)]
fn iter_54(e: &mut [f32], z: usize) {
    let k00 = e[z] - e[z - 4];
    let y0 = e[z] + e[z - 4];
    let y2 = e[z - 2] + e[z - 6];
    let k22 = e[z - 2] - e[z - 6];

    e[z] = y0 + y2; // z0 + z4 + z2 + z6
    e[z - 2] = y0 - y2; // z0 + z4 - z2 - z6

    // done with y0,y2

    let k33 = e[z - 3] - e[z - 7];

    e[z - 4] = k00 + k33; // z0 - z4 + z3 - z7
    e[z - 6] = k00 - k33; // z0 - z4 - z3 + z7

    // done with k33

    let k11 = e[z - 1] - e[z - 5];
    let y1 = e[z - 1] + e[z - 5];
    let y3 = e[z - 3] + e[z - 7];

    e[z - 1] = y1 + y3; // z1 + z5 + z3 + z7
    e[z - 3] = y1 - y3; // z1 + z5 - z3 - z7
    e[z - 5] = k11 - k22; // z1 - z5 + z2 - z6
    e[z - 7] = k11 + k22; // z1 - z5 - z2 + z6
}

fn imdct_step3_inner_s_loop_ld654(n: usize, e: &mut [f32], i_off: usize, a: &[f32], base_n: usize) {
    let a_off = base_n >> 3;
    let a2 = a[a_off];
    let mut z = i_off;
    for _ in 0..n {
        let k00 = e[z] - e[z - 8];
        let k11 = e[z - 1] - e[z - 9];
        let l00 = e[z - 2] - e[z - 10];
        let l11 = e[z - 3] - e[z - 11];
        e[z] += e[z - 8];
        e[z - 1] += e[z - 9];
        e[z - 2] += e[z - 10];
        e[z - 3] += e[z - 11];
        e[z - 8] = k00;
        e[z - 9] = k11;
        e[z - 10] = (l00 + l11) * a2;
        e[z - 11] = (l11 - l00) * a2;

        let k00 = e[z - 4] - e[z - 12];
        let k11 = e[z - 5] - e[z - 13];
        let l00 = e[z - 6] - e[z - 14];
        let l11 = e[z - 7] - e[z - 15];
        e[z - 4] += e[z - 12];
        e[z - 5] += e[z - 13];
        e[z - 6] += e[z - 14];
        e[z - 7] += e[z - 15];
        e[z - 12] = k11;
        e[z - 13] = -k00;
        e[z - 14] = (l11 - l00) * a2;
        e[z - 15] = (l00 + l11) * -a2;

        iter_54(e, z);
        iter_54(e, z - 8);
        z = z.wrapping_sub(16);
    }
}

/// The inverse MDCT of the first `n / 2` values of `buffer`, giving `n` samples in place.
/// `buf2` is scratch space of at least `n / 2` values.
pub(crate) fn inverse_mdct(buffer: &mut [f32], n: usize, tables: &BlockTables, buf2: &mut [f32]) {
    let (n2, n4, n8) = (n >> 1, n >> 2, n >> 3);
    let a = &tables.a[..];
    let buffer = &mut buffer[..n];
    let buf2 = &mut buf2[..n2];

    // IMDCT algorithm from "The use of multirate filter banks for coding of high quality digital audio"
    // See notes about bugs in that paper in less-optimal implementation 'inverse_mdct_old' in stb_vorbis.

    // kernel from paper

    // merged:
    //   copy and reflect spectral data
    //   step 0

    // note that it turns out that the items added together during
    // this step are, in fact, being added to themselves (as reflected
    // by step 0). inexplicable inefficiency! this became obvious
    // once I combined the passes.

    // so there's a missing 'times 2' here (for adding X to itself).
    // this propagates through linearly to the end, where the numbers
    // are 1/2 too small, and need to be compensated for.
    {
        let mut d = n2 as isize - 2;
        let mut aa = 0usize;
        let mut e = 0usize;
        while e != n2 {
            let d0 = d as usize;
            buf2[d0 + 1] = buffer[e] * a[aa] - buffer[e + 2] * a[aa + 1];
            buf2[d0] = buffer[e] * a[aa + 1] + buffer[e + 2] * a[aa];
            d -= 2;
            aa += 2;
            e += 4;
        }

        let mut e = n2 as isize - 3;
        while d >= 0 {
            let d0 = d as usize;
            let e0 = e as usize;
            buf2[d0 + 1] = -buffer[e0 + 2] * a[aa] - -buffer[e0] * a[aa + 1];
            buf2[d0] = -buffer[e0 + 2] * a[aa + 1] + -buffer[e0] * a[aa];
            d -= 2;
            aa += 2;
            e -= 4;
        }
    }

    // now we use symbolic names for these, so that we can
    // possibly swap their meaning as we change which operations
    // are in place

    let u = buffer;
    let v = buf2;

    // step 2    (paper output is w, now u)
    // this could be in place, but the data ends up in the wrong
    // place... _somebody_'s got to swap it, so this is nominated
    {
        let mut aa = n2 as isize - 8;
        let (mut e0, mut e1, mut d0, mut d1) = (n4, 0usize, n4, 0usize);
        while aa >= 0 {
            let ai = aa as usize;
            let v41_21 = v[e0 + 1] - v[e1 + 1];
            let v40_20 = v[e0] - v[e1];
            u[d0 + 1] = v[e0 + 1] + v[e1 + 1];
            u[d0] = v[e0] + v[e1];
            u[d1 + 1] = v41_21 * a[ai + 4] - v40_20 * a[ai + 5];
            u[d1] = v40_20 * a[ai + 4] + v41_21 * a[ai + 5];

            let v41_21 = v[e0 + 3] - v[e1 + 3];
            let v40_20 = v[e0 + 2] - v[e1 + 2];
            u[d0 + 3] = v[e0 + 3] + v[e1 + 3];
            u[d0 + 2] = v[e0 + 2] + v[e1 + 2];
            u[d1 + 3] = v41_21 * a[ai] - v40_20 * a[ai + 1];
            u[d1 + 2] = v40_20 * a[ai] + v41_21 * a[ai + 1];

            aa -= 8;
            d0 += 4;
            d1 += 4;
            e0 += 4;
            e1 += 4;
        }
    }

    // step 3
    let ld = ilog(n as i32) - 1; // ilog is off-by-one from normal definitions

    // optimized step 3:

    // the original step3 loop can be nested r inside s or s inside r;
    // it's written originally as s inside r, but this is dumb when r
    // iterates many times, and s few. So I have two copies of it and
    // switch between them halfway.

    // this is iteration 0 of step 3
    imdct_step3_iter0_loop(n >> 4, u, n2 - 1, n >> 3, a);
    imdct_step3_iter0_loop(n >> 4, u, n2 - 1 - n4, n >> 3, a);

    // this is iteration 1 of step 3
    for i in 0..4 {
        imdct_step3_inner_r_loop(n >> 5, u, n2 - 1 - n8 * i, n >> 4, a, 16);
    }

    let mut l = 2i32;
    while l < (ld - 3) >> 1 {
        let k0 = n >> (l + 2);
        let k0_2 = k0 >> 1;
        let lim = 1usize << (l + 1);
        for i in 0..lim {
            imdct_step3_inner_r_loop(n >> (l + 4), u, n2 - 1 - k0 * i, k0_2, a, 1 << (l + 3));
        }
        l += 1;
    }

    while l < ld - 6 {
        let k0 = n >> (l + 2);
        let k1 = 1usize << (l + 3);
        let k0_2 = k0 >> 1;
        let rlim = n >> (l + 6);
        let lim = 1usize << (l + 1);
        let mut a0 = 0usize;
        let mut i_off = n2 - 1;
        for _ in 0..rlim {
            imdct_step3_inner_s_loop(lim, u, i_off, k0_2, &a[a0..], k1, k0);
            a0 += k1 * 4;
            i_off = i_off.wrapping_sub(8);
        }
        l += 1;
    }

    // iterations with count:
    //   ld-6,-5,-4 all interleaved together
    //       the big win comes from getting rid of needless flops
    //         due to the constants on pass 5 & 4 being all 1 and 0;
    //       combining them to be simultaneous to improve cache made little difference
    imdct_step3_inner_s_loop_ld654(n >> 5, u, n2 - 1, a, n);

    // output is u

    // step 4, 5, and 6
    // cannot be in-place because of step 5
    {
        let bitrev = &tables.bit_reverse[..];
        // weirdly, I'd have thought reading sequentially and writing
        // erratically would have been better than vice-versa, but in
        // fact that's not what my testing showed. (That is, with
        // j = bitreverse(i), do you read i and write j, or read j and write i.)
        let mut d0 = n4 as isize - 4;
        let mut d1 = n2 as isize - 4;
        let mut br = 0usize;
        while d0 >= 0 {
            let (p0, p1) = (d0 as usize, d1 as usize);
            let k4 = usize::from(bitrev[br]);
            v[p1 + 3] = u[k4];
            v[p1 + 2] = u[k4 + 1];
            v[p0 + 3] = u[k4 + 2];
            v[p0 + 2] = u[k4 + 3];

            let k4 = usize::from(bitrev[br + 1]);
            v[p1 + 1] = u[k4];
            v[p1] = u[k4 + 1];
            v[p0 + 1] = u[k4 + 2];
            v[p0] = u[k4 + 3];

            d0 -= 4;
            d1 -= 4;
            br += 2;
        }
    }
    // (paper output is u, now v)

    // data must be in buf2

    // step 7   (paper output is v, now v)
    // this is now in place
    {
        let c = &tables.c[..];
        let mut ci = 0usize;
        let mut d = 0usize;
        let mut e = n2 - 4;
        while d < e {
            let a02 = v[d] - v[e + 2];
            let a11 = v[d + 1] + v[e + 3];

            let b0 = c[ci + 1] * a02 + c[ci] * a11;
            let b1 = c[ci + 1] * a11 - c[ci] * a02;

            let b2 = v[d] + v[e + 2];
            let b3 = v[d + 1] - v[e + 3];

            v[d] = b2 + b0;
            v[d + 1] = b3 + b1;
            v[e + 2] = b2 - b0;
            v[e + 3] = b1 - b3;

            let a02 = v[d + 2] - v[e];
            let a11 = v[d + 3] + v[e + 1];

            let b0 = c[ci + 3] * a02 + c[ci + 2] * a11;
            let b1 = c[ci + 3] * a11 - c[ci + 2] * a02;

            let b2 = v[d + 2] + v[e];
            let b3 = v[d + 3] - v[e + 1];

            v[d + 2] = b2 + b0;
            v[d + 3] = b3 + b1;
            v[e] = b2 - b0;
            v[e + 1] = b1 - b3;

            ci += 4;
            d += 4;
            e -= 4;
        }
    }

    // data must be in buf2

    // step 8+decode   (paper output is X, now buffer)
    // this generates pairs of data a la 8 and pushes them directly through
    // the decode kernel (pushing rather than pulling) to avoid having
    // to make another pass later

    // this cannot POSSIBLY be in place, so we refer to the buffers directly
    {
        let b = &tables.b[..];
        let mut bi = n2 as isize - 8;
        let mut e = n2 as isize - 8;
        let (mut d0, mut d1, mut d2, mut d3) = (0usize, n2 - 4, n2, n - 4);
        while e >= 0 {
            let (ei, bb) = (e as usize, bi as usize);

            let p3 = v[ei + 6] * b[bb + 7] - v[ei + 7] * b[bb + 6];
            let p2 = -v[ei + 6] * b[bb + 6] - v[ei + 7] * b[bb + 7];

            u[d0] = p3;
            u[d1 + 3] = -p3;
            u[d2] = p2;
            u[d3 + 3] = p2;

            let p1 = v[ei + 4] * b[bb + 5] - v[ei + 5] * b[bb + 4];
            let p0 = -v[ei + 4] * b[bb + 4] - v[ei + 5] * b[bb + 5];

            u[d0 + 1] = p1;
            u[d1 + 2] = -p1;
            u[d2 + 1] = p0;
            u[d3 + 2] = p0;

            let p3 = v[ei + 2] * b[bb + 3] - v[ei + 3] * b[bb + 2];
            let p2 = -v[ei + 2] * b[bb + 2] - v[ei + 3] * b[bb + 3];

            u[d0 + 2] = p3;
            u[d1 + 1] = -p3;
            u[d2 + 2] = p2;
            u[d3 + 1] = p2;

            let p1 = v[ei] * b[bb + 1] - v[ei + 1] * b[bb];
            let p0 = -v[ei] * b[bb] - v[ei + 1] * b[bb + 1];

            u[d0 + 3] = p1;
            u[d1] = -p1;
            u[d2 + 3] = p0;
            u[d3] = p0;

            bi -= 8;
            e -= 8;
            d0 += 4;
            d2 += 4;
            d1 = d1.wrapping_sub(4);
            d3 = d3.wrapping_sub(4);
        }
    }
}
