//! 트레이 / 작업표시줄 아이콘을 코드로 그린다. 외부 파일 의존 없음.
//! 잠긴 자물쇠 = 고정, 열린 자물쇠 = 해제.

pub struct Rgba {
    pub px: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

const SHELL: (u8, u8, u8) = (24, 24, 30);
const LOCKED: (u8, u8, u8) = (78, 222, 128);
const RELEASED: (u8, u8, u8) = (242, 98, 98);

fn sd_round_rect(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let dx = (px - cx).abs() - (hx - r);
    let dy = (py - cy).abs() - (hy - r);
    let ox = dx.max(0.0);
    let oy = dy.max(0.0);
    (ox * ox + oy * oy).sqrt() + dx.max(dy).min(0.0) - r
}

fn sd_circle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - r
}

fn cov(d: f32, aa: f32) -> f32 {
    (0.5 - d / aa).clamp(0.0, 1.0)
}

fn over(dst: &mut [f32; 4], rgb: (u8, u8, u8), a: f32) {
    if a <= 0.0 {
        return;
    }
    let src = [
        rgb.0 as f32 / 255.0,
        rgb.1 as f32 / 255.0,
        rgb.2 as f32 / 255.0,
    ];
    let out_a = a + dst[3] * (1.0 - a);
    if out_a <= 0.0 {
        return;
    }
    for i in 0..3 {
        dst[i] = (src[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

pub fn badge(locked: bool, size: u32) -> Rgba {
    let s = size as f32;
    let u = s / 32.0;
    let aa = 1.0_f32.max(u * 0.9);
    let accent = if locked { LOCKED } else { RELEASED };

    let body_cy = 20.5 * u;
    let shackle_cx = if locked { 16.0 * u } else { 20.5 * u };
    let shackle_cy = if locked { 13.0 * u } else { 11.5 * u };

    let mut px = vec![0u8; (size * size * 4) as usize];

    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut c = [0.0f32; 4];

            let shell = sd_round_rect(fx, fy, s * 0.5, s * 0.5, s * 0.5, s * 0.5, 7.0 * u);
            over(&mut c, SHELL, cov(shell, aa));

            let ring = sd_circle(fx, fy, shackle_cx, shackle_cy, 5.0 * u).abs() - 1.6 * u;
            let below = fy - (shackle_cy + 0.5 * u);
            over(&mut c, accent, cov(ring.max(below), aa));

            let body = sd_round_rect(fx, fy, s * 0.5, body_cy, 8.5 * u, 6.5 * u, 2.2 * u);
            over(&mut c, accent, cov(body, aa));

            let keyhole = sd_circle(fx, fy, s * 0.5, body_cy, 2.0 * u);
            over(&mut c, SHELL, cov(keyhole, aa));

            let i = ((y * size + x) * 4) as usize;
            px[i] = (c[0] * 255.0) as u8;
            px[i + 1] = (c[1] * 255.0) as u8;
            px[i + 2] = (c[2] * 255.0) as u8;
            px[i + 3] = (c[3] * 255.0) as u8;
        }
    }

    Rgba {
        px,
        w: size,
        h: size,
    }
}
