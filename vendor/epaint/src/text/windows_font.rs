//! Windows hinted UI glyphs. Only the application's named Segoe UI face uses this
//! path; all other fonts and platforms retain upstream's outline rasterizer.
//! GDI handles live inside one call, so none are shared between render threads.
#![allow(unsafe_code)]

use windows_sys::Win32::Graphics::Gdi::*;

pub(super) struct NativeFont {
    height: i32,
    pub ascent: f32,
    pub row_height: f32,
}

pub(super) struct Raster {
    pub width: usize,
    pub height: usize,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
    pub coverage: Vec<f32>,
}

struct SelectedFont {
    dc: HDC,
    font: HFONT,
    previous: HGDIOBJ,
}

impl SelectedFont {
    fn new(height: i32) -> Option<Self> {
        let name: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
        // SAFETY: no borrowed pointers escape these synchronous GDI calls.
        // The owned DC/font are released below on every successful path.
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            if dc.is_null() {
                return None;
            }
            let font = CreateFontW(
                -height,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                DEFAULT_QUALITY as u32,
                DEFAULT_PITCH as u32,
                name.as_ptr(),
            );
            if font.is_null() {
                DeleteDC(dc);
                return None;
            }
            let previous = SelectObject(dc, font);
            if previous.is_null() || previous as isize == -1 {
                DeleteObject(font);
                DeleteDC(dc);
                return None;
            }
            Some(Self { dc, font, previous })
        }
    }
}

impl Drop for SelectedFont {
    fn drop(&mut self) {
        // SAFETY: these are live handles created by SelectedFont::new. Restore
        // the original object before deleting the selected font and its DC.
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.font);
            DeleteDC(self.dc);
        }
    }
}

impl NativeFont {
    pub fn new(name: &str, scale: f32) -> Option<Self> {
        if name != "Windows UI"
            || std::env::var_os("ANYCOMPARE_FONT_RENDERER").as_deref()
                == Some(std::ffi::OsStr::new("outline"))
        {
            return None;
        }
        // UI FontId 12 maps to LOGFONT height -12, observed through WM_GETFONT
        // and GetObjectW on the original's active controls with REA.
        let height = scale.round().max(1.0) as i32;
        let selected = SelectedFont::new(height)?;
        // SAFETY: selected.dc remains valid and metrics is a writable structure.
        let metrics = unsafe {
            let mut metrics: TEXTMETRICW = std::mem::zeroed();
            if GetTextMetricsW(selected.dc, &mut metrics) == 0 {
                return None;
            }
            metrics
        };
        Some(Self {
            height,
            ascent: metrics.tmAscent as f32,
            row_height: (metrics.tmHeight + metrics.tmExternalLeading) as f32,
        })
    }

    pub fn raster(&self, glyph: u16) -> Option<Raster> {
        let selected = SelectedFont::new(self.height)?;
        let zero = FIXED { fract: 0, value: 0 };
        let one = FIXED { fract: 0, value: 1 };
        let matrix = MAT2 {
            eM11: one,
            eM12: zero,
            eM21: zero,
            eM22: one,
        };
        let flags = GGO_GRAY8_BITMAP | GGO_GLYPH_INDEX;
        // SAFETY: structures and the bounded bitmap allocation outlive GDI's
        // synchronous writes. The first call obtains the required byte count.
        unsafe {
            let mut metrics: GLYPHMETRICS = std::mem::zeroed();
            let size = GetGlyphOutlineW(
                selected.dc,
                glyph as u32,
                flags,
                &mut metrics,
                0,
                std::ptr::null_mut(),
                &matrix,
            );
            if size == GDI_ERROR as u32 || size > 4 * 1024 * 1024 {
                return None;
            }
            if size == 0 {
                return Some(Raster {
                    width: 0,
                    height: 0,
                    x: 0.0,
                    y: 0.0,
                    advance: metrics.gmCellIncX as f32,
                    coverage: Vec::new(),
                });
            }
            let width = metrics.gmBlackBoxX as usize;
            let height = metrics.gmBlackBoxY as usize;
            let stride = (width + 3) & !3;
            if stride.checked_mul(height)? > size as usize {
                return None;
            }
            let mut bitmap = vec![0u8; size as usize];
            if size > 0
                && GetGlyphOutlineW(
                    selected.dc,
                    glyph as u32,
                    flags,
                    &mut metrics,
                    size,
                    bitmap.as_mut_ptr().cast(),
                    &matrix,
                ) == GDI_ERROR as u32
            {
                return None;
            }
            let mut coverage = Vec::with_capacity(width * height);
            for row in 0..height {
                for col in 0..width {
                    coverage.push(f32::from(bitmap[row * stride + col].min(64)) / 64.0);
                }
            }
            Some(Raster {
                width,
                height,
                x: metrics.gmptGlyphOrigin.x as f32,
                y: -metrics.gmptGlyphOrigin.y as f32,
                advance: metrics.gmCellIncX as f32,
                coverage,
            })
        }
    }
}
