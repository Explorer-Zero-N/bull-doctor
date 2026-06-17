// 应用图标像素渲染（托盘、窗口与 Windows .ico 共用）。
// 牛头图案 — 橙色牛头 + 深蓝背景。

const CORNER_RADIUS_32: f32 = 8.0;

pub fn render_icon_rgba(size: u32) -> Vec<u8> {
    if size <= 48 {
        let scale = if size <= 16 { 4 } else { 2 };
        let big = render_icon_rgba_inner(size * scale);
        downscale_box(&big, size * scale, size)
    } else {
        render_icon_rgba_inner(size)
    }
}

fn render_icon_rgba_inner(size: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let unit = size as f32 / 32.0;
    let canvas = size as f32;
    let radius = CORNER_RADIUS_32 * unit;

    // 颜色定义
    let bg_dark = (26u8, 26, 46);       // #1a1a2e
    let bull_orange = (249u8, 115, 22);  // #f97316
    let bull_dark = (194u8, 65, 12);     // #c2410c
    let eye_dark = (26u8, 26, 46);       // #1a1a2e
    let eye_white = (255u8, 255, 255);
    let ring_gold = (251u8, 191, 36);    // #fbbf24

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let idx = ((y * size + x) * 4) as usize;

            // 圆角矩形背景
            if !in_rounded_rect(px, py, canvas, canvas, radius) {
                continue;
            }
            rgba[idx] = bg_dark.0;
            rgba[idx + 1] = bg_dark.1;
            rgba[idx + 2] = bg_dark.2;
            rgba[idx + 3] = 255;

            let cx = 16.0 * unit;
            let cy = 17.0 * unit;
            let dx = px - cx;
            let dy = py - cy;

            // 牛头主体（椭圆）
            let rx = 8.0 * unit;
            let ry = 9.0 * unit;
            if (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1.0 {
                rgba[idx] = bull_orange.0;
                rgba[idx + 1] = bull_orange.1;
                rgba[idx + 2] = bull_orange.2;
            }

            // 牛角（左）
            let horn_lx = 6.0 * unit;
            let horn_ly = 7.0 * unit;
            if dist_to_line_seg(px, py, horn_lx, horn_ly, 10.0 * unit, 11.0 * unit) < 1.5 * unit {
                rgba[idx] = bull_orange.0;
                rgba[idx + 1] = bull_orange.1;
                rgba[idx + 2] = bull_orange.2;
            }
            if dist_to_line_seg(px, py, horn_lx, horn_ly, 4.0 * unit, 3.5 * unit) < 1.5 * unit {
                rgba[idx] = bull_orange.0;
                rgba[idx + 1] = bull_orange.1;
                rgba[idx + 2] = bull_orange.2;
            }

            // 牛角（右）
            let horn_rx = 26.0 * unit;
            let horn_ry = 7.0 * unit;
            if dist_to_line_seg(px, py, horn_rx, horn_ry, 22.0 * unit, 11.0 * unit) < 1.5 * unit {
                rgba[idx] = bull_orange.0;
                rgba[idx + 1] = bull_orange.1;
                rgba[idx + 2] = bull_orange.2;
            }
            if dist_to_line_seg(px, py, horn_rx, horn_ry, 28.0 * unit, 3.5 * unit) < 1.5 * unit {
                rgba[idx] = bull_orange.0;
                rgba[idx + 1] = bull_orange.1;
                rgba[idx + 2] = bull_orange.2;
            }

            // 耳朵（左）
            let elx = 7.5 * unit;
            let ely = 12.0 * unit;
            let edx = px - elx;
            let edy = py - ely;
            if (edx * edx) / (2.0 * unit * 2.0 * unit) + (edy * edy) / (2.8 * unit * 2.8 * unit) <= 1.0 {
                rgba[idx] = bull_dark.0;
                rgba[idx + 1] = bull_dark.1;
                rgba[idx + 2] = bull_dark.2;
            }

            // 耳朵（右）
            let erx = 24.5 * unit;
            let ery = 12.0 * unit;
            let edx = px - erx;
            let edy = py - ery;
            if (edx * edx) / (2.0 * unit * 2.0 * unit) + (edy * edy) / (2.8 * unit * 2.8 * unit) <= 1.0 {
                rgba[idx] = bull_dark.0;
                rgba[idx + 1] = bull_dark.1;
                rgba[idx + 2] = bull_dark.2;
            }

            // 眼睛（左）
            let eye_lx = 12.5 * unit;
            let eye_ly = 15.0 * unit;
            let eye_r = 1.8 * unit;
            let edx = px - eye_lx;
            let edy = py - eye_ly;
            if edx * edx + edy * edy <= eye_r * eye_r {
                rgba[idx] = eye_dark.0;
                rgba[idx + 1] = eye_dark.1;
                rgba[idx + 2] = eye_dark.2;
            }
            // 眼睛高光
            let hl_r = 0.6 * unit;
            if (px - 13.0 * unit).powi(2) + (py - 14.5 * unit).powi(2) <= hl_r * hl_r {
                rgba[idx] = eye_white.0;
                rgba[idx + 1] = eye_white.1;
                rgba[idx + 2] = eye_white.2;
            }

            // 眼睛（右）
            let eye_rx = 19.5 * unit;
            let eye_ry = 15.0 * unit;
            let edx = px - eye_rx;
            let edy = py - eye_ry;
            if edx * edx + edy * edy <= eye_r * eye_r {
                rgba[idx] = eye_dark.0;
                rgba[idx + 1] = eye_dark.1;
                rgba[idx + 2] = eye_dark.2;
            }
            if (px - 20.0 * unit).powi(2) + (py - 14.5 * unit).powi(2) <= hl_r * hl_r {
                rgba[idx] = eye_white.0;
                rgba[idx + 1] = eye_white.1;
                rgba[idx + 2] = eye_white.2;
            }

            // 鼻子
            let nose_r = 1.3 * unit;
            if (px - 14.0 * unit).powi(2) + (py - 21.0 * unit).powi(2) <= nose_r * nose_r {
                rgba[idx] = bull_dark.0;
                rgba[idx + 1] = bull_dark.1;
                rgba[idx + 2] = bull_dark.2;
            }
            if (px - 18.0 * unit).powi(2) + (py - 21.0 * unit).powi(2) <= nose_r * nose_r {
                rgba[idx] = bull_dark.0;
                rgba[idx + 1] = bull_dark.1;
                rgba[idx + 2] = bull_dark.2;
            }

            // 鼻环
            let ring_cx = 16.0 * unit;
            let ring_cy = 23.5 * unit;
            let ring_r = 2.0 * unit;
            let ring_thick = 1.0 * unit;
            let rdx = px - ring_cx;
            let rdy = py - ring_cy;
            let rdist = (rdx * rdx + rdy * rdy).sqrt();
            if (rdist - ring_r).abs() < ring_thick * 0.5 && rdy > 0.0 {
                rgba[idx] = ring_gold.0;
                rgba[idx + 1] = ring_gold.1;
                rgba[idx + 2] = ring_gold.2;
            }
        }
    }

    rgba
}

fn dist_to_line_seg(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 0.001 {
        return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
    }
    let t = ((px - x1) * dx + (py - y1) * dy).clamp(0.0, len_sq) / len_sq;
    let proj_x = x1 + t * dx;
    let proj_y = y1 + t * dy;
    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

fn in_rounded_rect(px: f32, py: f32, w: f32, h: f32, r: f32) -> bool {
    let r = r.min(w * 0.5).min(h * 0.5);
    let (cx, cy) = if px < r {
        if py < r { (r, r) }
        else if py > h - r { (r, h - r) }
        else { return px >= 0.0 && px <= w && py >= 0.0 && py <= h; }
    } else if px > w - r {
        if py < r { (w - r, r) }
        else if py > h - r { (w - r, h - r) }
        else { return px >= 0.0 && px <= w && py >= 0.0 && py <= h; }
    } else {
        return px >= 0.0 && px <= w && py >= 0.0 && py <= h;
    };
    (px - cx).powi(2) + (py - cy).powi(2) <= r * r
}

fn downscale_box(src: &[u8], src_size: u32, dst_size: u32) -> Vec<u8> {
    let ratio = src_size / dst_size;
    let mut dst = vec![0u8; (dst_size * dst_size * 4) as usize];
    for dy in 0..dst_size {
        for dx in 0..dst_size {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for sy in dy * ratio..(dy + 1) * ratio {
                for sx in dx * ratio..(dx + 1) * ratio {
                    let i = ((sy * src_size + sx) * 4) as usize;
                    r += src[i] as u32;
                    g += src[i + 1] as u32;
                    b += src[i + 2] as u32;
                    a += src[i + 3] as u32;
                }
            }
            let n = ratio * ratio;
            let i = ((dy * dst_size + dx) * 4) as usize;
            dst[i] = (r / n) as u8;
            dst[i + 1] = (g / n) as u8;
            dst[i + 2] = (b / n) as u8;
            dst[i + 3] = (a / n) as u8;
        }
    }
    dst
}
