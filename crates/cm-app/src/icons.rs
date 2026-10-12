//! Raster icons drawn in code: the tray icon (the two-squares logo in coral).

/// Coverage of a rounded rectangle at (px, py), via a signed distance estimate.
fn rrect_sdf(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let qx = (px - cx).abs() - (w / 2.0 - r);
    let qy = (py - cy).abs() - (h / 2.0 - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// RGBA pixels of the app logo at `size`×`size`, in `rgb` (0xRRGGBB).
pub fn logo_rgba(size: u32, rgb: u32) -> Vec<u8> {
    let (r, g, b) = (
        ((rgb >> 16) & 0xFF) as f32,
        ((rgb >> 8) & 0xFF) as f32,
        (rgb & 0xFF) as f32,
    );
    let scale = size as f32 / 24.0;
    let stroke = 2.0 * scale;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    const SS: u32 = 4;
    for py in 0..size {
        for px in 0..size {
            let mut alpha = 0.0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = px as f32 + (sx as f32 + 0.5) / SS as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SS as f32;
                    // Back square: translucent fill plus outline. Front square: outline only.
                    let back = rrect_sdf(
                        x,
                        y,
                        9.0 * scale,
                        3.0 * scale,
                        12.0 * scale,
                        12.0 * scale,
                        3.5 * scale,
                    );
                    let front = rrect_sdf(
                        x,
                        y,
                        3.0 * scale,
                        7.0 * scale,
                        12.0 * scale,
                        12.0 * scale,
                        3.5 * scale,
                    );
                    let on_stroke = |d: f32| d.abs() <= stroke / 2.0;
                    let a = if on_stroke(front) {
                        1.0
                    } else if front < 0.0 {
                        0.0 // the front square's inside punches through the back fill
                    } else if on_stroke(back) {
                        1.0
                    } else if back < 0.0 {
                        0.35
                    } else {
                        0.0
                    };
                    alpha += a;
                }
            }
            alpha /= (SS * SS) as f32;
            out.extend_from_slice(&[r as u8, g as u8, b as u8, (alpha * 255.0).round() as u8]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn logo_has_ink() {
        let px = super::logo_rgba(32, 0xE2725B);
        assert_eq!(px.len(), 32 * 32 * 4);
        let opaque = px.chunks(4).filter(|p| p[3] > 200).count();
        assert!(
            opaque > 50,
            "logo should draw outlines, got {opaque} opaque pixels"
        );
    }
}
