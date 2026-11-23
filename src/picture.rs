//! AnyCompare Picture Compare engine (`PixColors`, `PixContent`, `PixMap`,
//! `PixRules` — verified via spec Hex-Rays in `ida/out/pix`).
//!
//! - Image decoding uses Windows Imaging Component (WIC, matching BC5's
//!   `TPixWICGraphic` at ) for native PNG, JPEG, BMP, GIF, TIFF,
//!   ICO, WebP, HEIF, WDP/JXR and RAW support, with `image` (PNG) fallback.
//! - Comparison modes reproduce BC5's exact per-pixel formulas:
//!   - `CompareMode::Tolerance` (`TPixToleranceProcessor` )
//!   - `CompareMode::MismatchRange` (`TPixRangeProcessor` )
//!   - `CompareMode::Blend` (`TPixBlendProcessor` )

use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const PICTURE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "jfif", "bmp", "dib", "gif", "tif", "tiff", "ico", "cur", "webp",
    "wdp", "hdp", "jxr", "heic", "heif", "avif",
];

pub fn is_picture_path(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        PICTURE_EXTENSIONS
            .iter()
            .any(|candidate| ext.eq_ignore_ascii_case(candidate))
    })
}

/// BC5 `TPixColors.GetColorsDefault` () RGB defaults.
pub const COLOR_OUTSIDE: [u8; 3] = [46, 46, 35]; // COLORREF 0x00232E2E
pub const COLOR_DIFF: [u8; 3] = [255, 0, 0]; // COLORREF 0x000000FF
pub const COLOR_SAME: [u8; 3] = [0, 0, 0]; // COLORREF 0x00000000
pub const COLOR_TOLERATED: [u8; 3] = [0, 0, 255]; // COLORREF 0x00FF0000

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompareMode {
    #[default]
    Tolerance,
    MismatchRange,
    Blend,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImageTransform {
    /// Clockwise 90-degree steps (0..=3).
    pub rotation: u8,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl ImageTransform {
    pub fn rotate_right(&mut self) {
        self.rotation = (self.rotation + 1) % 4;
    }

    pub fn rotate_left(&mut self) {
        self.rotation = (self.rotation + 3) % 4;
    }

    pub fn is_identity(self) -> bool {
        self.rotation.is_multiple_of(4) && !self.flip_h && !self.flip_v
    }

    pub fn rotated_dimensions(self, width: u32, height: u32) -> (u32, u32) {
        if self.rotation % 2 == 1 {
            (height, width)
        } else {
            (width, height)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CompareOptions {
    pub mode: CompareMode,
    /// Per-channel max delta (`0..=255`) classified as tolerated/unimportant.
    pub tolerance: u8,
    /// BC5 "Ghost" luminance modulation (`lum = max(96, avg)`).
    pub ghost: bool,
    /// Treat tolerated pixels as matching (`Color[9]`) instead of blue (`Color[10]`).
    pub ignore_unimportant: bool,
    /// Blend right-side weight `0..=100` (`TPixBlendProcessor`).
    pub blend_percent: u8,
    /// Scale the smaller image proportionally to match the larger (`TPixMapTree.ProcessNeedsRun`).
    pub auto_scale: bool,
    /// Right image offset relative to left image in pixels.
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            mode: CompareMode::Tolerance,
            tolerance: 0,
            ghost: true,
            ignore_unimportant: false,
            blend_percent: 50,
            auto_scale: false,
            offset_x: 0,
            offset_y: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PictureImage {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA8 (`width * height * 4` bytes).
    pub rgba: Vec<u8>,
    pub has_alpha: bool,
    pub bits_per_pixel: u8,
    pub format_name: String,
    pub dpi: Option<(f64, f64)>,
    pub file_size: u64,
    pub modified: Option<SystemTime>,
    pub sha256: [u8; 32],
    pub path: Option<PathBuf>,
}

impl PictureImage {
    pub fn load(path: &Path) -> Result<Self, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.len() > 256 * 1024 * 1024 {
            return Err("Imagem acima do limite de 256 MiB.".into());
        }
        let raw_bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (sha256, decoded) = std::thread::scope(|scope| {
            let h = scope.spawn(|| {
                let d: [u8; 32] = Sha256::digest(&raw_bytes).into();
                d
            });
            let dec = decode_image(path, &raw_bytes);
            (h.join().unwrap(), dec)
        });
        let decoded = decoded?;
        let expected_len = (decoded.width as usize)
            .checked_mul(decoded.height as usize)
            .and_then(|px| px.checked_mul(4))
            .ok_or_else(|| "Dimensões da imagem inválidas.".to_owned())?;
        if decoded.width == 0 || decoded.height == 0 || decoded.rgba.len() != expected_len {
            return Err("Imagem vazia ou corrompida.".into());
        }
        Ok(Self {
            width: decoded.width,
            height: decoded.height,
            rgba: decoded.rgba,
            has_alpha: decoded.has_alpha,
            bits_per_pixel: decoded.bits_per_pixel,
            format_name: decoded.format_name,
            dpi: decoded.dpi,
            file_size: meta.len(),
            modified: meta.modified().ok(),
            sha256,
            path: Some(path.to_path_buf()),
        })
    }

    #[cfg(test)]
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>, has_alpha: bool) -> Self {
        assert_eq!(rgba.len(), (width as usize) * (height as usize) * 4);
        Self {
            width,
            height,
            rgba,
            has_alpha,
            bits_per_pixel: if has_alpha { 32 } else { 24 },
            format_name: "RGBA".into(),
            dpi: None,
            file_size: (width as u64) * (height as u64) * 4,
            modified: None,
            sha256: [0; 32],
            path: None,
        }
    }

    /// Apply rotation/flip and optional percentage scaling (`scale_pct == 100` keeps original size).
    pub fn transformed(&self, tf: ImageTransform, scale_pct: u32) -> Surface {
        let (rot_w, rot_h) = tf.rotated_dimensions(self.width, self.height);
        let mut buf = if tf.is_identity() {
            self.rgba.clone()
        } else {
            let mut out = vec![0u8; (rot_w as usize) * (rot_h as usize) * 4];
            let sw = self.width as usize;
            let sh = self.height as usize;
            let dw = rot_w as usize;
            let dh = rot_h as usize;
            let rot = tf.rotation % 4;
            for sy in 0..sh {
                for sx in 0..sw {
                    let (mut dx, mut dy) = match rot {
                        1 => (sh - 1 - sy, sx),
                        2 => (sw - 1 - sx, sh - 1 - sy),
                        3 => (sy, sw - 1 - sx),
                        _ => (sx, sy),
                    };
                    if tf.flip_h {
                        dx = dw - 1 - dx;
                    }
                    if tf.flip_v {
                        dy = dh - 1 - dy;
                    }
                    let src = (sy * sw + sx) * 4;
                    let dst = (dy * dw + dx) * 4;
                    out[dst..dst + 4].copy_from_slice(&self.rgba[src..src + 4]);
                }
            }
            out
        };
        let (mut w, mut h) = (rot_w, rot_h);
        if scale_pct != 100 && scale_pct > 0 {
            let nw = (((w as u64) * (scale_pct as u64) + 50) / 100).max(1) as u32;
            let nh = (((h as u64) * (scale_pct as u64) + 50) / 100).max(1) as u32;
            buf = resize_bilinear(&buf, w, h, nw, nh);
            w = nw;
            h = nh;
        }
        Surface {
            width: w,
            height: h,
            rgba: buf,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Surface {
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        Some([
            self.rgba[idx],
            self.rgba[idx + 1],
            self.rgba[idx + 2],
            self.rgba[idx + 3],
        ])
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComparisonStats {
    pub same_pixels: u64,
    pub tolerated_pixels: u64,
    pub different_pixels: u64,
    pub left_only_pixels: u64,
    pub right_only_pixels: u64,
}

impl ComparisonStats {
    pub fn compared_pixels(self) -> u64 {
        self.same_pixels + self.tolerated_pixels + self.different_pixels
    }

    pub fn total_pixels(self) -> u64 {
        self.compared_pixels() + self.left_only_pixels + self.right_only_pixels
    }

    pub fn diff_percent(self) -> f64 {
        let total = self.total_pixels();
        if total == 0 {
            return 0.0;
        }
        let non_same = self.different_pixels + self.left_only_pixels + self.right_only_pixels;
        (non_same as f64) * 100.0 / (total as f64)
    }

    pub fn same_percent(self) -> f64 {
        let total = self.total_pixels();
        if total == 0 {
            return 100.0;
        }
        (self.same_pixels as f64) * 100.0 / (total as f64)
    }
}

#[derive(Clone, Debug)]
pub struct PictureComparison {
    pub left_surface: Option<Surface>,
    pub right_surface: Option<Surface>,
    pub diff_surface: Surface,
    pub left_origin: (i32, i32),
    pub right_origin: (i32, i32),
    pub left_scale_pct: u32,
    pub right_scale_pct: u32,
    pub stats: ComparisonStats,
}

/// BC5 `TPixMapTree.ProcessNeedsRun` ( lines 162-194) auto-scale calculation.
pub fn compute_auto_scales(w0: u32, h0: u32, w1: u32, h1: u32) -> (u32, u32) {
    if w0 == 0 || h0 == 0 || w1 == 0 || h1 == 0 {
        return (100, 100);
    }
    let scale_x = (100u64 * (w0.max(w1) as u64) / (w0.min(w1) as u64)) as u32;
    let scale_y = (100u64 * (h0.max(h1) as u64) / (h0.min(h1) as u64)) as u32;
    let mut scales = [100u32, 100u32];
    if scale_x >= scale_y {
        scales[usize::from(h0 >= h1)] = scale_y;
    } else {
        scales[usize::from(w0 >= w1)] = scale_x;
    }
    (scales[0], scales[1])
}

/// BC5 `TPixToleranceProcessor` LUT generator (`sub_141195A70` + `sub_141195B30` + `sub_141195B60`).
fn build_tolerance_lut(base: [u8; 3], ghost: bool) -> [[u8; 3]; 256] {
    let norm = if ghost {
        let max_c = base[0].max(base[1]).max(base[2]);
        if max_c == 0 {
            [255, 255, 255]
        } else {
            [
                ((255u32 * base[0] as u32) / max_c as u32) as u8,
                ((255u32 * base[1] as u32) / max_c as u32) as u8,
                ((255u32 * base[2] as u32) / max_c as u32) as u8,
            ]
        }
    } else {
        base
    };
    let mut lut = [[0u8; 3]; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let lum = i.max(96) as u32;
        if ghost {
            *slot = [
                ((norm[0] as u32 * lum) / 255) as u8,
                ((norm[1] as u32 * lum) / 255) as u8,
                ((norm[2] as u32 * lum) / 255) as u8,
            ];
        } else {
            *slot = norm;
        }
    }
    lut
}

/// BC5 `TPixBlendProcessor.Create` () LUTs using Win32 `MulDiv` rounding.
fn build_blend_luts(blend_percent: u8) -> ([u8; 256], [u8; 256]) {
    let b = blend_percent.min(100) as u32;
    let a = 100 - b;
    let mut left_lut = [0u8; 256];
    let mut right_lut = [0u8; 256];
    for i in 0..256u32 {
        left_lut[i as usize] = ((i * a + 50) / 100).min(255) as u8;
        right_lut[i as usize] = ((i * b + 50) / 100).min(255) as u8;
    }
    (left_lut, right_lut)
}

pub fn compare_pictures(
    left: Option<&PictureImage>,
    right: Option<&PictureImage>,
    left_tf: ImageTransform,
    right_tf: ImageTransform,
    options: CompareOptions,
) -> PictureComparison {
    let uses_alpha = left.is_some_and(|i| i.has_alpha) || right.is_some_and(|i| i.has_alpha);
    let (left_scale_pct, right_scale_pct) = match (left, right, options.auto_scale) {
        (Some(l), Some(r), true) => {
            let (lw, lh) = left_tf.rotated_dimensions(l.width, l.height);
            let (rw, rh) = right_tf.rotated_dimensions(r.width, r.height);
            compute_auto_scales(lw, lh, rw, rh)
        }
        _ => (100, 100),
    };

    let (left_surface, right_surface) = std::thread::scope(|scope| {
        let hr = scope.spawn(|| right.map(|img| img.transformed(right_tf, right_scale_pct)));
        let l = left.map(|img| img.transformed(left_tf, left_scale_pct));
        (l, hr.join().unwrap())
    });

    let (lw, lh) = left_surface
        .as_ref()
        .map_or((0, 0), |s| (s.width, s.height));
    let (rw, rh) = right_surface
        .as_ref()
        .map_or((0, 0), |s| (s.width, s.height));

    // Align origins so both images lie in non-negative coordinates (matching BC5 GetLeft2/GetTop2).
    let left_x = (-options.offset_x).max(0);
    let left_y = (-options.offset_y).max(0);
    let right_x = options.offset_x.max(0);
    let right_y = options.offset_y.max(0);

    let canvas_w = ((left_x as u64 + lw as u64).max(right_x as u64 + rw as u64)).max(1) as u32;
    let canvas_h = ((left_y as u64 + lh as u64).max(right_y as u64 + rh as u64)).max(1) as u32;

    let mut diff_rgba = vec![0u8; (canvas_w as usize) * (canvas_h as usize) * 4];
    let full_overlap = matches!((&left_surface, &right_surface), (Some(ls), Some(rs)) if left_x == 0 && left_y == 0 && right_x == 0 && right_y == 0 && ls.width == canvas_w && ls.height == canvas_h && rs.width == canvas_w && rs.height == canvas_h);
    if !full_overlap {
        for px in diff_rgba.as_chunks_mut::<4>().0 {
            *px = [COLOR_OUTSIDE[0], COLOR_OUTSIDE[1], COLOR_OUTSIDE[2], 255];
        }
    }

    let mut stats = ComparisonStats::default();
    if let Some(ls) = &left_surface {
        stats.left_only_pixels = (ls.width as u64) * (ls.height as u64);
    }
    if let Some(rs) = &right_surface {
        stats.right_only_pixels = (rs.width as u64) * (rs.height as u64);
    }

    if let (Some(ls), Some(rs)) = (&left_surface, &right_surface) {
        let ov_x0 = left_x.max(right_x) as u32;
        let ov_y0 = left_y.max(right_y) as u32;
        let ov_x1 = ((left_x as u32 + ls.width).min(right_x as u32 + rs.width)).min(canvas_w);
        let ov_y1 = ((left_y as u32 + ls.height).min(right_y as u32 + rs.height)).min(canvas_h);

        if ov_x0 < ov_x1 && ov_y0 < ov_y1 {
            let ov_w = (ov_x1 - ov_x0) as u64;
            let ov_h = (ov_y1 - ov_y0) as u64;
            let overlap = ov_w * ov_h;
            stats.left_only_pixels = stats.left_only_pixels.saturating_sub(overlap);
            stats.right_only_pixels = stats.right_only_pixels.saturating_sub(overlap);

            let same_lut = build_tolerance_lut(COLOR_SAME, options.ghost);
            let tol_color = if options.ignore_unimportant {
                COLOR_SAME
            } else {
                COLOR_TOLERATED
            };
            let tol_lut = build_tolerance_lut(tol_color, options.ghost);
            let diff_lut = build_tolerance_lut(COLOR_DIFF, options.ghost);
            let (blend_l, blend_r) = build_blend_luts(options.blend_percent);
            let tol = options.tolerance as i32;

            let cw = canvas_w as usize;
            let lsw = ls.width as usize;
            let rsw = rs.width as usize;
            let lx_off = (ov_x0 - left_x as u32) as usize;
            let rx_off = (ov_x0 - right_x as u32) as usize;
            let dx_len = (ov_x1 - ov_x0) as usize;
            let dst_x0 = ov_x0 as usize;
            let row_stride_bytes = cw * 4;
            let mode = options.mode;

            let process_band = |y_start: u32, band_rgba: &mut [u8]| -> (u64, u64, u64) {
                let mut same = 0u64;
                let mut tolerated = 0u64;
                let mut different = 0u64;
                for (row_idx, dst_row_bytes) in
                    band_rgba.chunks_exact_mut(row_stride_bytes).enumerate()
                {
                    let y = y_start + row_idx as u32;
                    let ly = (y - left_y as u32) as usize;
                    let ry = (y - right_y as u32) as usize;
                    let l_slice = &ls.rgba[(ly * lsw + lx_off) * 4..];
                    let r_slice = &rs.rgba[(ry * rsw + rx_off) * 4..];
                    let d_slice = &mut dst_row_bytes[dst_x0 * 4..];
                    for dx in 0..dx_len {
                        let off = dx * 4;
                        let lr = l_slice[off];
                        let lg = l_slice[off + 1];
                        let lb = l_slice[off + 2];
                        let la = l_slice[off + 3];
                        let rr = r_slice[off];
                        let rg = r_slice[off + 1];
                        let rb = r_slice[off + 2];
                        let ra = r_slice[off + 3];

                        let dr = (lr as i32 - rr as i32).abs();
                        let dg = (lg as i32 - rg as i32).abs();
                        let db = (lb as i32 - rb as i32).abs();
                        let da = if uses_alpha {
                            (la as i32 - ra as i32).abs()
                        } else {
                            0
                        };

                        if dr == 0 && dg == 0 && db == 0 && da == 0 {
                            same += 1;
                        } else if dr <= tol && dg <= tol && db <= tol && da <= tol {
                            tolerated += 1;
                        } else {
                            different += 1;
                        }

                        match mode {
                            CompareMode::Tolerance => {
                                let lum = if uses_alpha {
                                    ((lr as u32
                                        + lg as u32
                                        + lb as u32
                                        + la as u32
                                        + rr as u32
                                        + rg as u32
                                        + rb as u32
                                        + ra as u32)
                                        >> 3)
                                        .max(96) as usize
                                } else {
                                    ((lr as u32
                                        + lg as u32
                                        + lb as u32
                                        + rr as u32
                                        + rg as u32
                                        + rb as u32)
                                        / 6)
                                    .max(96) as usize
                                };
                                let rgb = if dr == 0 && dg == 0 && db == 0 && da == 0 {
                                    same_lut[lum]
                                } else if dr <= tol && dg <= tol && db <= tol && da <= tol {
                                    tol_lut[lum]
                                } else {
                                    diff_lut[lum]
                                };
                                d_slice[off] = rgb[0];
                                d_slice[off + 1] = rgb[1];
                                d_slice[off + 2] = rgb[2];
                                d_slice[off + 3] = 255;
                            }
                            CompareMode::MismatchRange => {
                                let sum = dr + dg + db + da;
                                let v = if sum > 0 {
                                    let scaled = if uses_alpha { sum >> 2 } else { sum / 3 };
                                    (scaled + 48).min(255) as u8
                                } else {
                                    0
                                };
                                d_slice[off] = v;
                                d_slice[off + 1] = v;
                                d_slice[off + 2] = 0;
                                d_slice[off + 3] = 255;
                            }
                            CompareMode::Blend => {
                                if !uses_alpha || (la == 255 && ra == 255) {
                                    d_slice[off] =
                                        blend_l[lr as usize].saturating_add(blend_r[rr as usize]);
                                    d_slice[off + 1] =
                                        blend_l[lg as usize].saturating_add(blend_r[rg as usize]);
                                    d_slice[off + 2] =
                                        blend_l[lb as usize].saturating_add(blend_r[rb as usize]);
                                    d_slice[off + 3] = 255;
                                } else {
                                    let al = blend_l[la as usize] as u32;
                                    let ar = blend_r[ra as usize] as u32;
                                    if al == 0 && ar == 0 {
                                        d_slice[off..off + 4].copy_from_slice(&[0, 0, 0, 0]);
                                    } else {
                                        d_slice[off] = ((al * lr as u32 + ar * rr as u32) / 255)
                                            .min(255) as u8;
                                        d_slice[off + 1] = ((al * lg as u32 + ar * rg as u32)
                                            / 255)
                                            .min(255)
                                            as u8;
                                        d_slice[off + 2] = ((al * lb as u32 + ar * rb as u32)
                                            / 255)
                                            .min(255)
                                            as u8;
                                        d_slice[off + 3] = (al + ar).min(255) as u8;
                                    }
                                }
                            }
                        }
                    }
                }
                (same, tolerated, different)
            };

            let ov_byte_start = (ov_y0 as usize) * row_stride_bytes;
            let ov_byte_end = (ov_y1 as usize) * row_stride_bytes;
            let ov_slice = &mut diff_rgba[ov_byte_start..ov_byte_end];
            let threads = if overlap >= 65_536 {
                std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(4)
                    .clamp(1, 16)
            } else {
                1
            };

            if threads <= 1 {
                let (s, t, d) = process_band(ov_y0, ov_slice);
                stats.same_pixels = s;
                stats.tolerated_pixels = t;
                stats.different_pixels = d;
            } else {
                let total_rows = (ov_y1 - ov_y0) as usize;
                let rows_per_chunk = total_rows.div_ceil(threads).max(1);
                let chunk_bytes = rows_per_chunk * row_stride_bytes;
                std::thread::scope(|scope| {
                    let handles: Vec<_> = ov_slice
                        .chunks_mut(chunk_bytes)
                        .enumerate()
                        .map(|(idx, chunk)| {
                            let y_start = ov_y0 + (idx * rows_per_chunk) as u32;
                            scope.spawn(move || process_band(y_start, chunk))
                        })
                        .collect();
                    for h in handles {
                        let (s, t, d) = h.join().unwrap();
                        stats.same_pixels += s;
                        stats.tolerated_pixels += t;
                        stats.different_pixels += d;
                    }
                });
            }
        }
    }

    PictureComparison {
        left_surface,
        right_surface,
        diff_surface: Surface {
            width: canvas_w,
            height: canvas_h,
            rgba: diff_rgba,
        },
        left_origin: (left_x, left_y),
        right_origin: (right_x, right_y),
        left_scale_pct,
        right_scale_pct,
        stats,
    }
}

fn resize_bilinear(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut dst = vec![0u8; (dw as usize) * (dh as usize) * 4];
    let sx_scale = if dw > 1 {
        (sw - 1) as f64 / (dw - 1) as f64
    } else {
        0.0
    };
    let sy_scale = if dh > 1 {
        (sh - 1) as f64 / (dh - 1) as f64
    } else {
        0.0
    };
    for dy in 0..dh {
        let fy = dy as f64 * sy_scale;
        let y0 = (fy.floor() as u32).min(sh - 1);
        let y1 = (y0 + 1).min(sh - 1);
        let wy = fy - y0 as f64;
        for dx in 0..dw {
            let fx = dx as f64 * sx_scale;
            let x0 = (fx.floor() as u32).min(sw - 1);
            let x1 = (x0 + 1).min(sw - 1);
            let wx = fx - x0 as f64;

            let i00 = ((y0 * sw + x0) * 4) as usize;
            let i10 = ((y0 * sw + x1) * 4) as usize;
            let i01 = ((y1 * sw + x0) * 4) as usize;
            let i11 = ((y1 * sw + x1) * 4) as usize;
            let out = ((dy * dw + dx) * 4) as usize;
            for c in 0..4 {
                let top = src[i00 + c] as f64 * (1.0 - wx) + src[i10 + c] as f64 * wx;
                let bot = src[i01 + c] as f64 * (1.0 - wx) + src[i11 + c] as f64 * wx;
                dst[out + c] = (top * (1.0 - wy) + bot * wy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    dst
}

struct DecodedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    has_alpha: bool,
    bits_per_pixel: u8,
    format_name: String,
    dpi: Option<(f64, f64)>,
}

fn decode_image(path: &Path, raw_bytes: &[u8]) -> Result<DecodedImage, String> {
    #[cfg(windows)]
    let wic_err = match wic_decode(path) {
        Ok(decoded) => return Ok(decoded),
        Err(e) => Some(e),
    };
    #[cfg(not(windows))]
    let wic_err: Option<String> = None;
    // Fallback for PNG (or non-Windows builds) via the bundled `image` crate.
    let img = image::load_from_memory(raw_bytes).map_err(|e| {
        if let Some(we) = wic_err {
            format!(
                "Não foi possível decodificar a imagem {}: {e} ({we})",
                path.display()
            )
        } else {
            format!(
                "Não foi possível decodificar a imagem {}: {e}",
                path.display()
            )
        }
    })?;
    let has_alpha = img.color().has_alpha();
    let bits_per_pixel = img.color().bits_per_pixel() as u8;
    let rgba = img.to_rgba8();
    let format_name = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "IMG".into());
    Ok(DecodedImage {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
        has_alpha,
        bits_per_pixel,
        format_name,
        dpi: None,
    })
}

/// Native Windows Imaging Component (WIC) decoder — the exact OS codec stack
/// used by AnyCompare's `TPixWICGraphic` ().
#[cfg(windows)]
fn wic_decode(path: &Path) -> Result<DecodedImage, String> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{GUID, HRESULT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };

    const CLSID_WIC_IMAGING_FACTORY: GUID = GUID::from_u128(0xcacaf262_9370_4615_a13b_9f5539da4c0a);
    const IID_IWIC_IMAGING_FACTORY: GUID = GUID::from_u128(0xec5ec8a9_c395_4314_9c77_54d7a935ff70);
    const GUID_WIC_PIXEL_FORMAT_32BPP_RGBA: GUID =
        GUID::from_u128(0xf5c7ad2d_6a8d_43dd_a7a8_a29935261ae9);

    const CONTAINER_PNG: GUID = GUID::from_u128(0x1b7cfaf4_713f_473c_bbcd_6137425faeaf);
    const CONTAINER_JPEG: GUID = GUID::from_u128(0x19e4a5aa_5662_4fc5_a0c0_1758028e1057);
    const CONTAINER_BMP: GUID = GUID::from_u128(0x0af1d87e_fcfe_4188_bdeb_a7906471cbe3);
    const CONTAINER_GIF: GUID = GUID::from_u128(0x1f8a5601_7d4d_4cbd_9c82_1bc8d4eeb9a5);
    const CONTAINER_TIFF: GUID = GUID::from_u128(0x163bcc30_e2e9_4f0b_961d_a3e9fdb788a3);
    const CONTAINER_ICO: GUID = GUID::from_u128(0xa3a860c4_338f_4c17_919a_fba4b5628f21);
    const CONTAINER_WMP: GUID = GUID::from_u128(0x57a37caa_367a_4540_916b_f183c5093a4b);
    const CONTAINER_WEBP: GUID = GUID::from_u128(0xe094b0e2_67f2_45b3_b0ea_115337ca7cf3);
    const CONTAINER_HEIF: GUID = GUID::from_u128(0xe1e62521_6787_405b_a339_500715b5763f);

    const PIX_24BGR: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc90c);
    const PIX_24RGB: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc90d);
    const PIX_32BGR: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc90e);
    const PIX_32BGRA: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc90f);
    const PIX_32PBGRA: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc910);
    const PIX_32RGBA: GUID = GUID::from_u128(0xf5c7ad2d_6a8d_43dd_a7a8_a29935261ae9);
    const PIX_32PRGBA: GUID = GUID::from_u128(0x3cc4a650_a527_4d37_a916_3142c7ebedba);
    const PIX_64RGBA: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc916);
    const PIX_8GRAY: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc908);
    const PIX_8IDX: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc904);

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    struct ComPtr(*mut c_void);
    impl Drop for ComPtr {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let vtbl = *(self.0 as *const *const *const c_void);
                    let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                        std::mem::transmute(*vtbl.add(2));
                    release(self.0);
                }
            }
        }
    }

    let check = |hr: HRESULT, ctx: &str| -> Result<(), String> {
        if hr.is_ok() {
            Ok(())
        } else {
            Err(format!("WIC {ctx}: 0x{:08X}", hr.0 as u32))
        }
    };

    unsafe {
        let init_hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _com = ComGuard(init_hr.is_ok());

        // Create IWICImagingFactory via standard CoCreateInstance on raw IUnknown.
        let factory_raw: windows::core::IUnknown =
            CoCreateInstance(&CLSID_WIC_IMAGING_FACTORY, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("WIC factory: {e}"))?;
        let mut factory_ptr: *mut c_void = std::ptr::null_mut();
        {
            let unk =
                std::mem::transmute_copy::<windows::core::IUnknown, *mut c_void>(&factory_raw);
            let vtbl = *(unk as *const *const *const c_void);
            let qi: unsafe extern "system" fn(
                *mut c_void,
                *const GUID,
                *mut *mut c_void,
            ) -> HRESULT = std::mem::transmute(*vtbl.add(0));
            check(
                qi(unk, &IID_IWIC_IMAGING_FACTORY, &mut factory_ptr),
                "QI Factory",
            )?;
        }
        let factory = ComPtr(factory_ptr);
        let f_vtbl = *(factory.0 as *const *const *const c_void);

        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);

        // IWICImagingFactory::CreateDecoderFromFilename (vtable[3])
        let create_decoder: unsafe extern "system" fn(
            *mut c_void,
            *const u16,
            *const GUID,
            u32,
            u32,
            *mut *mut c_void,
        ) -> HRESULT = std::mem::transmute(*f_vtbl.add(3));
        let mut decoder_ptr: *mut c_void = std::ptr::null_mut();
        const GENERIC_READ: u32 = 0x8000_0000;
        const WIC_DECODE_METADATA_CACHE_ON_LOAD: u32 = 1;
        check(
            create_decoder(
                factory.0,
                wide.as_ptr(),
                std::ptr::null(),
                GENERIC_READ,
                WIC_DECODE_METADATA_CACHE_ON_LOAD,
                &mut decoder_ptr,
            ),
            "CreateDecoderFromFilename",
        )?;
        let decoder = ComPtr(decoder_ptr);
        let d_vtbl = *(decoder.0 as *const *const *const c_void);

        // IWICBitmapDecoder::GetContainerFormat (vtable[5])
        let get_container: unsafe extern "system" fn(*mut c_void, *mut GUID) -> HRESULT =
            std::mem::transmute(*d_vtbl.add(5));
        let mut container = GUID::zeroed();
        let _ = get_container(decoder.0, &mut container);

        // IWICBitmapDecoder::GetFrame (vtable[13])
        let get_frame: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT =
            std::mem::transmute(*d_vtbl.add(13));
        let mut frame_ptr: *mut c_void = std::ptr::null_mut();
        check(get_frame(decoder.0, 0, &mut frame_ptr), "GetFrame(0)")?;
        let frame = ComPtr(frame_ptr);
        let fr_vtbl = *(frame.0 as *const *const *const c_void);

        // IWICBitmapSource::GetSize (vtable[3])
        let get_size: unsafe extern "system" fn(*mut c_void, *mut u32, *mut u32) -> HRESULT =
            std::mem::transmute(*fr_vtbl.add(3));
        let (mut width, mut height) = (0u32, 0u32);
        check(get_size(frame.0, &mut width, &mut height), "GetSize")?;
        if width == 0 || height == 0 || width > 32_768 || height > 32_768 {
            return Err("Dimensões da imagem fora do limite suportado.".into());
        }

        // IWICBitmapSource::GetPixelFormat (vtable[4])
        let get_pf: unsafe extern "system" fn(*mut c_void, *mut GUID) -> HRESULT =
            std::mem::transmute(*fr_vtbl.add(4));
        let mut src_pf = GUID::zeroed();
        let _ = get_pf(frame.0, &mut src_pf);

        // IWICBitmapSource::GetResolution (vtable[5])
        let get_res: unsafe extern "system" fn(*mut c_void, *mut f64, *mut f64) -> HRESULT =
            std::mem::transmute(*fr_vtbl.add(5));
        let (mut dpi_x, mut dpi_y) = (0.0f64, 0.0f64);
        let dpi = if get_res(frame.0, &mut dpi_x, &mut dpi_y).is_ok() && dpi_x > 0.0 && dpi_y > 0.0
        {
            Some((dpi_x, dpi_y))
        } else {
            None
        };

        // IWICImagingFactory::CreateFormatConverter (vtable[10])
        let create_conv: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT =
            std::mem::transmute(*f_vtbl.add(10));
        let mut conv_ptr: *mut c_void = std::ptr::null_mut();
        check(
            create_conv(factory.0, &mut conv_ptr),
            "CreateFormatConverter",
        )?;
        let conv = ComPtr(conv_ptr);
        let c_vtbl = *(conv.0 as *const *const *const c_void);

        // IWICFormatConverter::Initialize (vtable[8])
        let init_conv: unsafe extern "system" fn(
            *mut c_void,
            *mut c_void,
            *const GUID,
            u32,
            *mut c_void,
            f64,
            u32,
        ) -> HRESULT = std::mem::transmute(*c_vtbl.add(8));
        check(
            init_conv(
                conv.0,
                frame.0,
                &GUID_WIC_PIXEL_FORMAT_32BPP_RGBA,
                0,
                std::ptr::null_mut(),
                0.0,
                0,
            ),
            "FormatConverter::Initialize",
        )?;

        let stride = width
            .checked_mul(4)
            .ok_or_else(|| "Overflow no stride da imagem.".to_owned())?;
        let buf_len = (stride as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| "Overflow no tamanho da imagem.".to_owned())?;
        let mut rgba = vec![0u8; buf_len];

        // IWICBitmapSource::CopyPixels (vtable[7])
        let copy_pixels: unsafe extern "system" fn(
            *mut c_void,
            *const c_void,
            u32,
            u32,
            *mut u8,
        ) -> HRESULT = std::mem::transmute(*c_vtbl.add(7));
        check(
            copy_pixels(
                conv.0,
                std::ptr::null(),
                stride,
                buf_len as u32,
                rgba.as_mut_ptr(),
            ),
            "CopyPixels",
        )?;

        let format_name = if container == CONTAINER_PNG {
            "PNG"
        } else if container == CONTAINER_JPEG {
            "JPEG"
        } else if container == CONTAINER_BMP {
            "BMP"
        } else if container == CONTAINER_GIF {
            "GIF"
        } else if container == CONTAINER_TIFF {
            "TIFF"
        } else if container == CONTAINER_ICO {
            "ICO"
        } else if container == CONTAINER_WMP {
            "WMP/JXR"
        } else if container == CONTAINER_WEBP {
            "WebP"
        } else if container == CONTAINER_HEIF {
            "HEIF"
        } else {
            "WIC"
        }
        .to_owned();

        let (bits_per_pixel, pf_has_alpha) = if src_pf == PIX_24BGR || src_pf == PIX_24RGB {
            (24, false)
        } else if src_pf == PIX_32BGR {
            (32, false)
        } else if src_pf == PIX_32BGRA
            || src_pf == PIX_32PBGRA
            || src_pf == PIX_32RGBA
            || src_pf == PIX_32PRGBA
            || src_pf == PIX_64RGBA
        {
            (32, true)
        } else if src_pf == PIX_8GRAY || src_pf == PIX_8IDX {
            (8, false)
        } else {
            (32, false)
        };
        let has_alpha = pf_has_alpha || rgba.as_chunks::<4>().0.iter().any(|px| px[3] < 255);

        Ok(DecodedImage {
            width,
            height,
            rgba,
            has_alpha,
            bits_per_pixel,
            format_name,
            dpi,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerance_range_and_blend_match_bc5_formulas() {
        // Two 2x1 opaque RGB images:
        // pixel 0: identical (100, 100, 100)
        // pixel 1: left=(100, 100, 100), right=(110, 100, 100) -> delta R=10
        let left =
            PictureImage::from_rgba(2, 1, vec![100, 100, 100, 255, 100, 100, 100, 255], false);
        let right =
            PictureImage::from_rgba(2, 1, vec![100, 100, 100, 255, 110, 100, 100, 255], false);

        // 1) Tolerance = 0, ghost = false -> pixel 0 black, pixel 1 pure red.
        let cmp = compare_pictures(
            Some(&left),
            Some(&right),
            ImageTransform::default(),
            ImageTransform::default(),
            CompareOptions {
                mode: CompareMode::Tolerance,
                tolerance: 0,
                ghost: false,
                ..Default::default()
            },
        );
        assert_eq!(cmp.diff_surface.pixel(0, 0), Some([0, 0, 0, 255]));
        assert_eq!(cmp.diff_surface.pixel(1, 0), Some([255, 0, 0, 255]));
        assert_eq!(cmp.stats.same_pixels, 1);
        assert_eq!(cmp.stats.different_pixels, 1);

        // 2) Tolerance = 12, ghost = false -> pixel 1 becomes tolerated blue (0, 0, 255).
        let cmp_tol = compare_pictures(
            Some(&left),
            Some(&right),
            ImageTransform::default(),
            ImageTransform::default(),
            CompareOptions {
                mode: CompareMode::Tolerance,
                tolerance: 12,
                ghost: false,
                ..Default::default()
            },
        );
        assert_eq!(cmp_tol.diff_surface.pixel(1, 0), Some([0, 0, 255, 255]));
        assert_eq!(cmp_tol.stats.tolerated_pixels, 1);
        assert_eq!(cmp_tol.stats.different_pixels, 0);

        // 3) MismatchRange -> pixel 0 black (0,0,0), pixel 1 yellow (10/3 + 48 = 51, 51, 0).
        let cmp_rng = compare_pictures(
            Some(&left),
            Some(&right),
            ImageTransform::default(),
            ImageTransform::default(),
            CompareOptions {
                mode: CompareMode::MismatchRange,
                ..Default::default()
            },
        );
        assert_eq!(cmp_rng.diff_surface.pixel(0, 0), Some([0, 0, 0, 255]));
        assert_eq!(cmp_rng.diff_surface.pixel(1, 0), Some([51, 51, 0, 255]));

        // 4) Blend 50% -> pixel 1 R = round(100*0.5) + round(110*0.5) = 50 + 55 = 105.
        let cmp_blend = compare_pictures(
            Some(&left),
            Some(&right),
            ImageTransform::default(),
            ImageTransform::default(),
            CompareOptions {
                mode: CompareMode::Blend,
                blend_percent: 50,
                ..Default::default()
            },
        );
        assert_eq!(
            cmp_blend.diff_surface.pixel(1, 0),
            Some([105, 100, 100, 255])
        );
    }

    #[test]
    fn autoscale_transform_and_offset_work_and_real_png_loads_via_wic() {
        assert_eq!(compute_auto_scales(100, 50, 200, 100), (200, 100));
        assert_eq!(compute_auto_scales(200, 100, 100, 50), (100, 200));

        // Load a real PNG from the repository assets via WIC.
        let icon =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/home_icons/100/HomeImages/imgHomePixCompareSession.png");
        let img = PictureImage::load(&icon).expect("WIC/PNG load must succeed");
        assert!(img.width > 0 && img.height > 0);

        let mut tf = ImageTransform::default();
        tf.rotate_right();
        let rotated = img.transformed(tf, 100);
        assert_eq!((rotated.width, rotated.height), (img.height, img.width));

        // Self-comparison with offset=0 must have 0 differences.
        let cmp = compare_pictures(
            Some(&img),
            Some(&img),
            ImageTransform::default(),
            ImageTransform::default(),
            CompareOptions::default(),
        );
        assert_eq!(cmp.stats.different_pixels, 0);
        assert_eq!(
            cmp.stats.same_pixels,
            (img.width as u64) * (img.height as u64)
        );

        // Offset (2, 0) shifts right image right by 2 px, producing left/right orphan strips.
        let shifted = compare_pictures(
            Some(&img),
            Some(&img),
            ImageTransform::default(),
            ImageTransform::default(),
            CompareOptions {
                offset_x: 2,
                ..Default::default()
            },
        );
        assert_eq!(shifted.diff_surface.width, img.width + 2);
        assert_eq!(shifted.stats.left_only_pixels, 2 * img.height as u64);
        assert_eq!(shifted.stats.right_only_pixels, 2 * img.height as u64);
    }
}
