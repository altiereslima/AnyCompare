use egui::Color32;

/// Beyond Compare 4 color palette and styling constants
pub struct BcTheme;

pub fn apply(ctx: &egui::Context) {
    // BC4 is always light. Pin the theme: `style_mut` only changes the active theme's style,
    // and eframe switches to the system theme after start-up, which left menus with egui's
    // default 12.5 px button font (13 px glyphs instead of the original 12 px).
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.set_visuals_of(egui::Theme::Light, egui::Visuals::light());
    let mut fonts = egui::FontDefinitions::default();
    if let Some(windows) = std::env::var_os("WINDIR") {
        for (name, file, family) in [
            ("Windows UI", "segoeui.ttf", egui::FontFamily::Proportional),
            ("Windows Mono", "cour.ttf", egui::FontFamily::Monospace),
        ] {
            if let Ok(bytes) =
                std::fs::read(std::path::Path::new(&windows).join("Fonts").join(file))
            {
                fonts
                    .font_data
                    .insert(name.into(), egui::FontData::from_owned(bytes));
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, name.into());
            }
        }
    }
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        style.animation_time = 0.0;
        style.visuals.menu_rounding = egui::Rounding::ZERO;
        style.visuals.popup_shadow = egui::epaint::Shadow::NONE;
        style.visuals.window_fill = BcTheme::WINDOW_BG;
        style.visuals.override_text_color = Some(Color32::BLACK);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(12.0));
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.visuals.panel_fill = BcTheme::WINDOW_BG;
        style.visuals.selection.bg_fill = Color32::from_rgb(204, 232, 255);
        style.visuals.selection.stroke =
            egui::Stroke::new(1.0_f32, Color32::from_rgb(153, 209, 255));
    });
}

impl BcTheme {
    // Window & Panels
    pub const WINDOW_BG: Color32 = Color32::from_rgb(240, 240, 240);
    pub const PANEL_BG: Color32 = Color32::from_rgb(255, 255, 255);

    // Text & Line numbers
    pub const TEXT_NORMAL: Color32 = Color32::BLACK;
    pub const GUTTER_BG: Color32 = Color32::from_rgb(240, 240, 240);
    // Hover frame of a gutter arrow, sampled on the original at 100%.
    pub const GUTTER_HOT_BORDER: Color32 = Color32::from_rgb(204, 232, 255);
    pub const GUTTER_HOT_FILL: Color32 = Color32::from_rgb(229, 243, 255);
    pub const GUTTER_TEXT: Color32 = Color32::from_rgb(120, 120, 120);

    // Diff highlights (Exact BC4 colors)
    // Sampled from user-supplied reference screenshots, 2026-10-14.
    pub const DIFF_BG: Color32 = Color32::from_rgb(255, 227, 227);
    pub const DIFF_TEXT: Color32 = Color32::from_rgb(255, 0, 0);
    pub const MINOR_BG: Color32 = Color32::from_rgb(238, 238, 255);
    pub const MINOR_TEXT: Color32 = Color32::from_rgb(0, 0, 255);
    pub const DIFF_INLINE_BG: Color32 = Color32::TRANSPARENT;
    pub const ORPHAN_BG: Color32 = Color32::from_rgb(228, 238, 248); // Soft blue

    // Thumbnail overview (InThumbnail)
    pub const THUMBNAIL_ORPHAN: Color32 = Color32::from_rgb(70, 120, 210);

    // Active Line & Selection
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::*;

    #[test]
    fn ui_text_uses_windows_hinted_advances_at_each_dpi() {
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            let ctx = egui::Context::default();
            apply(&ctx);
            ctx.set_pixels_per_point(scale);
            let _ = ctx.run(Default::default(), |ctx| {
                let text = "Próximo Salvar arquivo Ação";
                let actual = ctx.fonts(|fonts| {
                    fonts.layout_no_wrap(
                        text.into(),
                        egui::FontId::proportional(12.0),
                        Color32::BLACK,
                    )
                });
                // Compare the integrated atlas/layout with the OS text metrics,
                // including accents and the physical scale used by the viewport.
                // SAFETY: temporary handles are selected/restored in this scope.
                unsafe {
                    let dc = CreateCompatibleDC(std::ptr::null_mut());
                    assert!(!dc.is_null());
                    let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
                    let font = CreateFontW(
                        -(12.0 * scale).round() as i32,
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
                        face.as_ptr(),
                    );
                    assert!(!font.is_null());
                    let previous = SelectObject(dc, font);
                    let wide: Vec<u16> = text.encode_utf16().collect();
                    let mut extent = std::mem::zeroed();
                    let success =
                        GetTextExtentPoint32W(dc, wide.as_ptr(), wide.len() as i32, &mut extent);
                    for character in text.chars() {
                        let mut buffer = [0u16; 2];
                        let character_utf16 = character.encode_utf16(&mut buffer);
                        let mut character_extent = std::mem::zeroed();
                        GetTextExtentPoint32W(
                            dc,
                            character_utf16.as_ptr(),
                            character_utf16.len() as i32,
                            &mut character_extent,
                        );
                        let actual_width = ctx.fonts(|fonts| {
                            fonts.glyph_width(&egui::FontId::proportional(12.0), character)
                        }) * scale;
                        assert!(
                            (actual_width - character_extent.cx as f32).abs() <= 0.1,
                            "glyph {character:?} at {scale}: egui={actual_width} GDI={}",
                            character_extent.cx
                        );
                    }
                    SelectObject(dc, previous);
                    DeleteObject(font);
                    DeleteDC(dc);
                    assert_ne!(success, 0);
                    assert!(
                        (actual.size().x * scale - extent.cx as f32).abs() <= 1.0,
                        "scale={scale}: egui={} GDI={}",
                        actual.size().x * scale,
                        extent.cx
                    );
                }
            });
        }
    }
}
