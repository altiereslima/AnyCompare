use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::HashMap;

include!("icon_catalog.rs");

/// Icon texture premultiplied in gamma (sRGB) space. egui's `from_rgba_unmultiplied`
/// premultiplies in linear space while the painter blends in gamma space, which made
/// antialiased icon edges and the 40% disabled glyphs too light (a white disabled pixel
/// came out at 255 instead of the original 216 over the toolbar).
fn icon_image(size: [usize; 2], rgba: &[u8]) -> ColorImage {
    let premultiplied: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| {
            let a = u16::from(px[3]);
            let mul = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            [mul(px[0]), mul(px[1]), mul(px[2]), px[3]]
        })
        .collect();
    ColorImage::from_rgba_premultiplied(size, &premultiplied)
}

/// Disabled toolbar/menu glyph of the original: gray = 0.86 x channel average, drawn at
/// 40% of the source alpha over the background. Fitted on `imgStopRed` and
/// `imgCopyLeftOrRight` in folder-empty.png at 100% (mean error about 2 levels).
fn disabled_pixel(r: u8, g: u8, b: u8, a: u8) -> [u8; 2] {
    let average = (u32::from(r) + u32::from(g) + u32::from(b)) as f32 / 3.0;
    [
        (average * 0.86).round() as u8,
        (f32::from(a) * 0.40).round() as u8,
    ]
}

pub struct IconManager {
    textures: HashMap<String, TextureHandle>,
}

impl IconManager {
    /// MainModels stores red masks which the native folder view recolors per side.
    pub fn folder_model(
        &mut self,
        ctx: &Context,
        name: &str,
        color: egui::Color32,
    ) -> Option<TextureHandle> {
        let scale = match ctx.pixels_per_point() {
            p if p <= 1.0 => 100,
            p if p <= 1.25 => 125,
            p if p <= 1.5 => 150,
            _ => 200,
        };
        let key = format!("{name}@{scale}:{color:?}");
        if let Some(texture) = self.textures.get(&key) {
            return Some(texture.clone());
        }
        let (_, _, _, bytes) = ICON_SOURCES
            .iter()
            .find(|(n, group, s, _)| *n == name && *group == "MainModels" && *s == scale)?;
        let mut pixels = image::load_from_memory(bytes).ok()?.to_rgba8();
        for pixel in pixels.pixels_mut() {
            let red = pixel[0];
            let base = pixel[1];
            let saturation = red.saturating_sub(base);
            for channel in 0..3 {
                pixel[channel] = base.saturating_add(
                    (u16::from(saturation) * u16::from(color[channel]) / 255) as u8,
                );
            }
        }
        let texture = ctx.load_texture(
            &key,
            icon_image(
                [pixels.width() as usize, pixels.height() as usize],
                pixels.as_raw(),
            ),
            TextureOptions::NEAREST,
        );
        self.textures.insert(key, texture.clone());
        Some(texture)
    }
    pub fn menu_gutter(&mut self, ctx: &Context) -> TextureHandle {
        self.textures
            .entry("menu_gutter".into())
            .or_insert_with(|| {
                ctx.load_texture(
                    "menu_gutter",
                    ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]),
                    TextureOptions::NEAREST,
                )
            })
            .clone()
    }
    pub fn get_icon_with_state(
        &mut self,
        ctx: &Context,
        name: &str,
        enabled: bool,
    ) -> Option<TextureHandle> {
        if enabled {
            return self.get_icon(ctx, name);
        }
        let source = self.get_icon(ctx, name)?;
        let source_name = source.name();
        let original = source_name.split('@').next()?;
        let key = format!("{name}@{}:disabled", ctx.pixels_per_point());
        if let Some(texture) = self.textures.get(&key) {
            return Some(texture.clone());
        }
        let scale = match ctx.pixels_per_point() {
            p if p <= 1.0 => 100,
            p if p <= 1.25 => 125,
            p if p <= 1.5 => 150,
            _ => 200,
        };
        let (_, _, _, bytes) = ICON_SOURCES.iter().find(|(n, group, s, _)| {
            *n == original && *s == scale && *group != "HomeSmallImages"
        })?;
        let mut image = image::load_from_memory(bytes).ok()?.to_rgba8();
        for pixel in image.pixels_mut() {
            let [gray, alpha] = disabled_pixel(pixel[0], pixel[1], pixel[2], pixel[3]);
            pixel[0] = gray;
            pixel[1] = gray;
            pixel[2] = gray;
            pixel[3] = alpha;
        }
        let texture = ctx.load_texture(
            &key,
            icon_image(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            TextureOptions::NEAREST,
        );
        self.textures.insert(key, texture.clone());
        Some(texture)
    }
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }

    pub fn get_icon(&mut self, ctx: &Context, name: &str) -> Option<TextureHandle> {
        let original = match name {
            "home_dir_compare" => "imgHomeDirCompareSession",
            "home_dir_merge" => "imgHomeDirMergeSession",
            "home_dir_sync" => "imgHomeDirSyncSession",
            "home_text_compare" => "imgHomeTextCompareSession",
            "home_text_merge" => "imgHomeTextMergeSession",
            "home_hex_compare" => "imgHomeHexCompareSession",
            "home_data_compare" => "imgHomeDataCompareSession",
            "home_pix_compare" => "imgHomePixCompareSession",
            "home_next" => "imgHomeNext",
            "home_prev" => "imgHomePrev",
            "tb_home" => "imgSessionHome",
            "tb_sessions" => "imgBrowse",
            "tb_all" => "imgFavAll",
            "tb_diff" => "imgFavDiff",
            "tb_same" => "imgFavSame",
            "tb_rules" => "imgRules",
            "tb_format" => "imgTextFormatEdit",
            "tb_copy" => "imgCopy",
            "tb_edit" => "imgTextEditSession",
            "tb_next_sec" => "imgDown",
            "tb_prev_sec" => "imgUp",
            "tb_swap" => "imgSwap",
            "tb_refresh" => "imgRefresh",
            "status_diff" => "imgContentDifferent",
            "status_same" => "imgContentSame",
            "arrow_right" => "imgContentArrowRight",
            "arrow_left" => "imgContentArrowLeft",
            other => other,
        };
        // HomeSmallImages shares names with HomeImages. Prefer the full-size list.
        let scale = match ctx.pixels_per_point() {
            p if p <= 1.0 => 100,
            p if p <= 1.25 => 125,
            p if p <= 1.5 => 150,
            _ => 200,
        };
        let key = format!("{original}@{scale}");
        if let Some(texture) = self.textures.get(&key) {
            return Some(texture.clone());
        }
        let (_, _, _, bytes) = ICON_SOURCES.iter().find(|(n, group, s, _)| {
            *n == original && *s == scale && *group != "HomeSmallImages"
        })?;
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let texture = ctx.load_texture(
            &key,
            icon_image(size, image.as_raw()),
            TextureOptions::NEAREST,
        );
        self.textures.insert(key, texture.clone());
        Some(texture)
    }

    /// Gutter arrow for "Copy Line(s) to Left/Right". The resource in
    /// GutterModels is a model: its fill keeps a gradient in the green channel
    /// and BC4 paints it blue at run time. Pointing right = arrow of the left pane.
    pub fn gutter_selection_arrow(
        &mut self,
        ctx: &Context,
        pointing_right: bool,
    ) -> Option<TextureHandle> {
        let name = if pointing_right {
            "imgGutterArrowSelRight"
        } else {
            "imgGutterArrowSelLeft"
        };
        let scale = match ctx.pixels_per_point() {
            p if p <= 1.0 => 100,
            p if p <= 1.25 => 125,
            p if p <= 1.5 => 150,
            _ => 200,
        };
        let key = format!("{name}@{scale}:selection");
        if let Some(texture) = self.textures.get(&key) {
            return Some(texture.clone());
        }
        let (_, _, _, bytes) = ICON_SOURCES
            .iter()
            .find(|(n, _, s, _)| *n == name && *s == scale)?;
        let mut image = image::load_from_memory(bytes).ok()?.to_rgba8();
        for pixel in image.pixels_mut() {
            if pixel[3] > 0 && pixel[0] >= 0xf0 {
                let [r, g, b] = selection_arrow_color(pixel[1]);
                pixel[0] = r;
                pixel[1] = g;
                pixel[2] = b;
            }
        }
        let texture = ctx.load_texture(
            &key,
            icon_image(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            TextureOptions::NEAREST,
        );
        self.textures.insert(key, texture.clone());
        Some(texture)
    }
}

/// Model green value -> colour painted by BC4. The pairs were sampled on the
/// original at 100% (selection arrow under the mouse, 16/10/2026); values in
/// between and outside are interpolated linearly from the nearest pair.
fn selection_arrow_color(model: u8) -> [u8; 3] {
    const PAIRS: [(u8, [u8; 3]); 10] = [
        (44, [195, 221, 245]),
        (48, [198, 223, 246]),
        (53, [202, 225, 246]),
        (58, [205, 227, 247]),
        (64, [212, 231, 248]),
        (71, [215, 233, 249]),
        (78, [222, 236, 250]),
        (85, [229, 240, 251]),
        (94, [236, 244, 252]),
        (102, [243, 248, 253]),
    ];
    let segment = PAIRS
        .windows(2)
        .find(|pair| model <= pair[1].0)
        .unwrap_or(&PAIRS[PAIRS.len() - 2..]);
    let ((low, from), (high, to)) = (segment[0], segment[1]);
    let t = (f32::from(model) - f32::from(low)) / f32::from(high - low);
    std::array::from_fn(|i| {
        (f32::from(from[i]) + (f32::from(to[i]) - f32::from(from[i])) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_arrow_is_painted_with_the_colours_sampled_on_the_original() {
        assert_eq!(selection_arrow_color(102), [243, 248, 253]);
        assert_eq!(selection_arrow_color(71), [215, 233, 249]);
        assert_eq!(selection_arrow_color(44), [195, 221, 245]);
        // The 100% model also uses 41, just below the last sampled pair.
        let darker = selection_arrow_color(41);
        assert!(darker[0] < 195 && darker[2] <= 245);
        let ctx = Context::default();
        let mut icons = IconManager::new();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            ctx.set_pixels_per_point(scale);
            let _ = ctx.run(Default::default(), |ctx| {
                let right = icons.gutter_selection_arrow(ctx, true).unwrap();
                let left = icons.gutter_selection_arrow(ctx, false).unwrap();
                assert_ne!(right.id(), left.id());
                assert!(right.name().starts_with("imgGutterArrowSelRight@"));
            });
        }
    }

    #[test]
    fn section_arrows_use_the_action_resources_and_keep_their_distinct_pixels() {
        let ctx = Context::default();
        let mut icons = IconManager::new();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            ctx.set_pixels_per_point(scale);
            let _ = ctx.run(Default::default(), |ctx| {
                let next = icons.get_icon(ctx, "tb_next_sec").unwrap();
                let previous = icons.get_icon(ctx, "tb_prev_sec").unwrap();
                assert!(next.name().starts_with("imgDown@"));
                assert!(previous.name().starts_with("imgUp@"));
                assert_ne!(next.id(), previous.id());
            });
        }
        let source = |name| {
            ICON_SOURCES
                .iter()
                .find(|(n, _, s, _)| *n == name && *s == 100)
                .unwrap()
                .3
        };
        assert_ne!(source("imgDown"), source("imgDiffSectionNextNone"));
        assert_ne!(source("imgUp"), source("imgDiffSectionPrevNone"));
    }

    #[test]
    fn save_icon_is_colored_only_for_enabled_state() {
        let ctx = Context::default();
        let mut icons = IconManager::new();
        let mut ids = None;
        let output = ctx.run(Default::default(), |ctx| {
            ids = Some((
                icons
                    .get_icon_with_state(ctx, "imgSave", true)
                    .unwrap()
                    .id(),
                icons
                    .get_icon_with_state(ctx, "imgSave", false)
                    .unwrap()
                    .id(),
            ));
        });
        let (enabled, disabled) = ids.unwrap();
        assert_ne!(enabled, disabled);
        let pixels = |id| {
            let image = &output
                .textures_delta
                .set
                .iter()
                .find(|(key, _)| *key == id)
                .unwrap()
                .1
                .image;
            match image {
                egui::ImageData::Color(image) => image.pixels.as_slice(),
                _ => panic!("expected color icon"),
            }
        };
        assert!(pixels(enabled)
            .iter()
            .any(|p| p.a() > 0 && (p.r() != p.g() || p.g() != p.b())));
        assert!(pixels(disabled)
            .iter()
            .all(|p| p.r() == p.g() && p.g() == p.b()));
    }

    #[test]
    fn original_home_icons_retain_full_resolution_at_every_scale() {
        for (scale, expected) in [(100, 64), (125, 80), (150, 96), (200, 128)] {
            let (_, _, _, bytes) = ICON_SOURCES
                .iter()
                .find(|(name, group, s, _)| {
                    *name == "imgHomeTextCompareSession" && *group == "HomeImages" && *s == scale
                })
                .unwrap();
            let image = image::load_from_memory(bytes).unwrap();
            assert_eq!([image.width(), image.height()], [expected, expected]);
        }
    }
}
