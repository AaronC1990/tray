/// Shrinks a straight-alpha RGBA image by area averaging: every source
/// pixel adds to each destination pixel in proportion to how much of it the
/// destination covers. Colours are averaged premultiplied, so a transparent
/// pixel's colour never bleeds into an opaque edge.
pub fn area_rgba(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let (sw, sh, dw, dh) = (
        sw as usize,
        sh as usize,
        dw.max(1) as usize,
        dh.max(1) as usize,
    );
    let xs = spans(sw, dw);
    let ys = spans(sh, dh);
    let mut out = vec![0u8; dw * dh * 4];
    let mut row = vec![[0f32; 4]; sw];
    for (dy, ycov) in ys.iter().enumerate() {
        row.iter_mut().for_each(|p| *p = [0.0; 4]);
        for &(sy, wy) in ycov {
            for (sx, acc) in row.iter_mut().enumerate() {
                let o = (sy * sw + sx) * 4;
                let a = src[o + 3] as f32 / 255.0;
                acc[0] += src[o] as f32 * a * wy;
                acc[1] += src[o + 1] as f32 * a * wy;
                acc[2] += src[o + 2] as f32 * a * wy;
                acc[3] += a * wy;
            }
        }
        for (dx, xcov) in xs.iter().enumerate() {
            let mut p = [0f32; 4];
            for &(sx, wx) in xcov {
                for c in 0..4 {
                    p[c] += row[sx][c] * wx;
                }
            }
            let o = (dy * dw + dx) * 4;
            let a = p[3];
            if a > 1e-6 {
                for c in 0..3 {
                    out[o + c] = (p[c] / a).round().clamp(0.0, 255.0) as u8;
                }
            }
            out[o + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// For each destination pixel, the source pixels it covers and their
/// weights, which sum to one.
fn spans(src: usize, dst: usize) -> Vec<Vec<(usize, f32)>> {
    let step = src as f32 / dst as f32;
    (0..dst)
        .map(|d| {
            let (a, b) = (d as f32 * step, (d + 1) as f32 * step);
            let mut cov = Vec::new();
            let mut s = a.floor() as usize;
            while (s as f32) < b && s < src {
                let w = (b.min(s as f32 + 1.0) - a.max(s as f32)).max(0.0);
                if w > 0.0 {
                    cov.push((s, w / step));
                }
                s += 1;
            }
            cov
        })
        .collect()
}
