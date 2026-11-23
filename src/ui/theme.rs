use egui::Color32;
use std::sync::LazyLock;

/// Cached read-only Windows system font slices (`Cow::Borrowed` -> zero-copy in `egui::FontData`).
struct WindowsFontBytes {
    ui: Option<&'static [u8]>,
    mono: Option<&'static [u8]>,
    table_mono: Option<&'static [u8]>,
    symbols: Option<&'static [u8]>,
    thai: Option<&'static [u8]>,
    cjk_sc: Option<&'static [u8]>,
    cjk_jp: Option<&'static [u8]>,
    cjk_kr: Option<&'static [u8]>,
}

static WINDOWS_FONTS: LazyLock<WindowsFontBytes> = LazyLock::new(|| {
    let read_font = |file: &str| -> Option<&'static [u8]> {
        let windows = std::env::var_os("WINDIR")?;
        let path = std::path::Path::new(&windows).join("Fonts").join(file);
        let bytes = std::fs::read(path).ok()?;
        Some(Box::leak(bytes.into_boxed_slice()))
    };
    WindowsFontBytes {
        ui: read_font("segoeui.ttf"),
        mono: read_font("cour.ttf"),
        table_mono: read_font("consola.ttf"),
        symbols: read_font("seguisym.ttf"),
        thai: read_font("LeelawUI.ttf"),
        cjk_sc: read_font("msyh.ttc"),
        cjk_jp: read_font("YuGothM.ttc"),
        cjk_kr: read_font("malgun.ttf"),
    }
});

/// AnyCompare color palette and styling constants
pub struct BcTheme;

pub fn apply(ctx: &egui::Context) {
    // BC4 is always light. Pin the theme: `style_mut` only changes the active theme's style,
    // and eframe switches to the system theme after start-up, which left menus with egui's
    // default 12.5 px button font (13 px glyphs instead of the original 12 px).
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.set_visuals_of(egui::Theme::Light, egui::Visuals::light());
    let mut fonts = egui::FontDefinitions::default();
    let wf = &*WINDOWS_FONTS;
    for (name, bytes, family) in [
        ("Windows UI", wf.ui, egui::FontFamily::Proportional),
        ("Windows Mono", wf.mono, egui::FontFamily::Monospace),
        (
            "Table Mono",
            wf.table_mono,
            egui::FontFamily::Name("Table Mono".into()),
        ),
    ] {
        if let Some(slice) = bytes {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_static(slice));
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, name.into());
        }
    }
    // Append Windows fallback fonts for symbols, Thai, and CJK scripts without
    // altering primary Latin/Cyrillic/Vietnamese glyph metrics. Prefer the active
    // language's CJK face first so Japanese/Korean/Chinese render native forms.
    // Note: `YuGothM.ttc` index 1 is `Yu Gothic UI Regular` (ascender/descender
    // metrics match `Segoe UI` 1:1), whereas index 0 (`Yu Gothic Medium`) sits high.
    let lang = crate::i18n::language();
    let cjk_order: [(&str, Option<&'static [u8]>, u32); 3] = if lang.starts_with("ja") {
        [
            ("Windows CJK JP", wf.cjk_jp, 1),
            ("Windows CJK SC", wf.cjk_sc, 0),
            ("Windows CJK KR", wf.cjk_kr, 0),
        ]
    } else if lang.starts_with("ko") {
        [
            ("Windows CJK KR", wf.cjk_kr, 0),
            ("Windows CJK SC", wf.cjk_sc, 0),
            ("Windows CJK JP", wf.cjk_jp, 1),
        ]
    } else {
        [
            ("Windows CJK SC", wf.cjk_sc, 0),
            ("Windows CJK JP", wf.cjk_jp, 1),
            ("Windows CJK KR", wf.cjk_kr, 0),
        ]
    };
    let mut fallbacks: Vec<(&str, Option<&'static [u8]>, u32)> = vec![
        ("Windows Symbols", wf.symbols, 0),
        ("Windows Thai", wf.thai, 0),
    ];
    fallbacks.extend(cjk_order);
    for (fb_name, bytes, index) in fallbacks {
        if let Some(slice) = bytes {
            let name = fb_name.to_owned();
            let mut data = egui::FontData::from_static(slice);
            data.index = index;
            fonts.font_data.insert(name.clone(), data);
            for family in [
                egui::FontFamily::Proportional,
                egui::FontFamily::Monospace,
                egui::FontFamily::Name("Table Mono".into()),
            ] {
                let entry = fonts.families.entry(family).or_default();
                if !entry.contains(&name) {
                    entry.push(name.clone());
                }
            }
        }
    }
    // Always append Monospace fallback fonts to "Table Mono" so symbols and
    // non-Latin scripts in spreadsheets never drop to missing-glyph `?`.
    let mono_fallbacks = fonts.families[&egui::FontFamily::Monospace].clone();
    {
        let table_mono = fonts
            .families
            .entry(egui::FontFamily::Name("Table Mono".into()))
            .or_default();
        for name in &mono_fallbacks {
            if !table_mono.contains(name) {
                table_mono.push(name.clone());
            }
        }
    }
    // Text Compare panes: Consolas through the same hinted GDI path as tables
    // (the "Table Mono" face), then the monospace fallbacks for other scripts.
    let text_mono = fonts.families[&egui::FontFamily::Name("Table Mono".into())].clone();
    fonts
        .families
        .insert(egui::FontFamily::Name("Text Mono".into()), text_mono);
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
    pub const ORPHAN_BG: Color32 = Color32::from_rgb(255, 227, 227);

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

    #[test]
    fn table_and_text_mono_resolve_return_symbol_without_missing_glyph() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            for family in ["Table Mono", "Text Mono"] {
                let font_id = egui::FontId::new(13.0, egui::FontFamily::Name(family.into()));
                let has_glyph = ctx.fonts(|fonts| {
                    let g_ret = fonts.layout_no_wrap("↵".into(), font_id.clone(), Color32::BLACK);
                    let g_q = fonts.layout_no_wrap("?".into(), font_id.clone(), Color32::BLACK);
                    // Both glyphs must exist and the return arrow must not fall
                    // back to the replacement glyph (`?` / missing uv rect).
                    let ret_uv = g_ret.rows[0].glyphs[0].uv_rect;
                    let q_uv = g_q.rows[0].glyphs[0].uv_rect;
                    ret_uv != q_uv && ret_uv.size.x > 0.0 && ret_uv.size.y > 0.0
                });
                assert!(has_glyph, "{family} failed to resolve U+21B5 (↵)");
            }
        });
    }

    #[test]
    fn ui_and_editor_fonts_resolve_all_builtin_language_scripts() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let samples = [
            ("Japanese", "日"),
            ("Japanese kana", "あ"),
            ("Korean", "한"),
            ("Simplified Chinese", "简"),
            ("Thai", "ไ"),
            ("Russian", "Я"),
            ("Vietnamese", "ệ"),
            ("Slovenian", "š"),
        ];
        let _ = ctx.run(Default::default(), |ctx| {
            for font_id in [
                egui::FontId::proportional(12.0),
                egui::FontId::new(13.0, egui::FontFamily::Name("Text Mono".into())),
            ] {
                let q_uv = ctx.fonts(|fonts| {
                    fonts
                        .layout_no_wrap("?".into(), font_id.clone(), Color32::BLACK)
                        .rows[0]
                        .glyphs[0]
                        .uv_rect
                });
                let repl_uv = ctx.fonts(|fonts| {
                    fonts
                        .layout_no_wrap("\u{FFFD}".into(), font_id.clone(), Color32::BLACK)
                        .rows[0]
                        .glyphs[0]
                        .uv_rect
                });
                for (script, ch) in samples {
                    let uv = ctx.fonts(|fonts| {
                        fonts
                            .layout_no_wrap(ch.into(), font_id.clone(), Color32::BLACK)
                            .rows[0]
                            .glyphs[0]
                            .uv_rect
                    });
                    assert!(
                        uv != q_uv && uv != repl_uv && uv.size.x > 0.0 && uv.size.y > 0.0,
                        "{:?} failed to resolve {script} sample {ch:?}",
                        font_id.family
                    );
                }
            }
        });
    }
}
