//! AnyCompare Picture Compare screen (`ViewPix.pas` + `PixMap.pas`).
//! Displays Left & Right image panes above the synchronized Difference pane,
//! supporting Tolerance (with BC5 Ghost LUT), Mismatch Range, Blend, Auto-Scale,
//! per-side Rotate/Flip, X/Y offset alignment, synchronized zoom/pan, mirrored
//! pixel crosshair, and image metadata/tags comparison.

use super::{
    chrome,
    dialogs::{DialogField, DialogHost},
    history_combo,
    icons::IconManager,
    session_menu, session_tabs,
    theme::BcTheme,
    toolbar::{self, ToolEvent, ToolState},
    toolbar_spec,
};
use crate::{
    i18n::tr,
    picture::{
        compare_pictures, CompareMode, CompareOptions, ImageTransform, PictureComparison,
        PictureImage, Surface,
    },
};
use egui::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct SavedOptions {
    compare: CompareOptions,
    left_transform: ImageTransform,
    right_transform: ImageTransform,
    show_tags: bool,
    fit_to_window: bool,
    zoom: f32,
}

impl Default for SavedOptions {
    fn default() -> Self {
        Self {
            compare: CompareOptions::default(),
            left_transform: ImageTransform::default(),
            right_transform: ImageTransform::default(),
            show_tags: false,
            fit_to_window: true,
            zoom: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
enum PictureCtxCmd {
    ZoomIn,
    ZoomOut,
    ZoomActual,
    ZoomFit,
    Mode(CompareMode),
    ToggleGhost,
    ToggleAutoScale,
    ToggleProperties,
    RotateLeft,
    RotateRight,
    FlipH,
    FlipV,
    CopyFilename,
    OpenHex,
    AssocApp,
    Explore(bool),
}

pub struct PictureCompareScreen {
    pub left_path: String,
    pub right_path: String,
    left: Option<PictureImage>,
    right: Option<PictureImage>,
    options: SavedOptions,
    comparison: Option<PictureComparison>,
    revision: u64,
    textures_rev: u64,
    left_tex: Option<TextureHandle>,
    right_tex: Option<TextureHandle>,
    diff_tex: Option<TextureHandle>,
    checker_tex: Option<TextureHandle>,
    pan: Vec2,
    hover_pixel: Option<(i32, i32)>,
    active_left: bool,
    split_ratio: f32,
    error: Option<String>,
    recent_paths: Vec<String>,
    compare_time_ms: u128,
    show_toolbar: bool,
    show_file_info: bool,
    full_screen: bool,
    prev_non_blend_mode: CompareMode,
}

impl PictureCompareScreen {
    pub fn new() -> Self {
        Self {
            left_path: String::new(),
            right_path: String::new(),
            left: None,
            right: None,
            options: SavedOptions::default(),
            comparison: None,
            revision: 0,
            textures_rev: u64::MAX,
            left_tex: None,
            right_tex: None,
            diff_tex: None,
            checker_tex: None,
            pan: Vec2::ZERO,
            hover_pixel: None,
            active_left: true,
            split_ratio: 0.48,
            error: None,
            recent_paths: Vec::new(),
            compare_time_ms: 0,
            show_toolbar: true,
            show_file_info: true,
            full_screen: false,
            prev_non_blend_mode: CompareMode::Tolerance,
        }
    }

    pub fn reset_difference_offset(&mut self) {
        self.options.compare.offset_x = 0;
        self.options.compare.offset_y = 0;
        self.recompare();
    }

    #[cfg(test)]
    pub fn difference_offset(&self) -> (i32, i32) {
        (self.options.compare.offset_x, self.options.compare.offset_y)
    }

    #[cfg(test)]
    pub fn set_difference_offset(&mut self, ox: i32, oy: i32) {
        self.options.compare.offset_x = ox;
        self.options.compare.offset_y = oy;
        self.recompare();
    }

    pub fn toggle_blend(&mut self) {
        if self.options.compare.mode == CompareMode::Blend {
            self.options.compare.mode = self.prev_non_blend_mode;
        } else {
            self.prev_non_blend_mode = self.options.compare.mode;
            self.options.compare.mode = CompareMode::Blend;
        }
        self.recompare();
    }

    #[cfg(test)]
    pub fn is_blend_mode(&self) -> bool {
        self.options.compare.mode == CompareMode::Blend
    }

    pub fn report(&self, dialogs: &mut dyn DialogHost) {
        let l_dim = self
            .left
            .as_ref()
            .map(|i| format!("{}x{}", i.width, i.height))
            .unwrap_or_else(|| "-".into());
        let r_dim = self
            .right
            .as_ref()
            .map(|i| format!("{}x{}", i.width, i.height))
            .unwrap_or_else(|| "-".into());
        let (same_px, minor_px, diff_px) = self
            .comparison
            .as_ref()
            .map(|c| {
                (
                    c.stats.same_pixels,
                    c.stats.tolerated_pixels,
                    c.stats.different_pixels,
                )
            })
            .unwrap_or((0, 0, 0));
        let rows = vec![
            crate::report::Row {
                key: tr("common.size"),
                left: l_dim.clone(),
                right: r_dim.clone(),
                different: l_dim != r_dim,
            },
            crate::report::Row {
                key: tr("common.same"),
                left: same_px.to_string(),
                right: same_px.to_string(),
                different: false,
            },
            crate::report::Row {
                key: tr("options.unimportant_differences"),
                left: minor_px.to_string(),
                right: minor_px.to_string(),
                different: minor_px > 0,
            },
            crate::report::Row {
                key: tr("common.differences"),
                left: diff_px.to_string(),
                right: diff_px.to_string(),
                different: diff_px > 0,
            },
        ];
        let mut inputs = Vec::new();
        if !self.left_path.is_empty() {
            inputs.push(PathBuf::from(&self.left_path));
        }
        if !self.right_path.is_empty() {
            inputs.push(PathBuf::from(&self.right_path));
        }
        crate::report::export(
            "picture.report",
            &self.left_path,
            &self.right_path,
            &rows,
            &inputs,
            dialogs,
        );
    }

    fn open_info_modal(&self, ctx: &Context) {
        let l_dim = self
            .left
            .as_ref()
            .map(|i| format!("{}x{}", i.width, i.height))
            .unwrap_or_else(|| "-".into());
        let r_dim = self
            .right
            .as_ref()
            .map(|i| format!("{}x{}", i.width, i.height))
            .unwrap_or_else(|| "-".into());
        let diff_px = self
            .comparison
            .as_ref()
            .map(|c| c.stats.different_pixels)
            .unwrap_or(0);
        let rows = vec![
            (tr("folder.side_left"), format!("{} ({})", self.left_path, l_dim)),
            (tr("folder.side_right"), format!("{} ({})", self.right_path, r_dim)),
            (tr("common.differences"), diff_px.to_string()),
        ];
        super::info_modals::open_info(ctx, tr("picture.compare_info"), rows);
    }

    pub fn active_left(&self) -> bool {
        self.active_left
    }

    pub fn rules_draft(&self) -> super::session_settings::PictureRulesDraft {
        super::session_settings::PictureRulesDraft {
            mode: self.options.compare.mode,
            tolerance: self.options.compare.tolerance,
            blend_percent: self.options.compare.blend_percent,
            ghost: self.options.compare.ghost,
            ignore_unimportant: self.options.compare.ignore_unimportant,
            auto_scale: self.options.compare.auto_scale,
            offset_x: self.options.compare.offset_x,
            offset_y: self.options.compare.offset_y,
            fit_to_window: self.options.fit_to_window,
            show_tags: self.options.show_tags,
        }
    }

    pub fn apply_rules(&mut self, draft: super::session_settings::PictureRulesDraft) {
        self.options.compare.mode = draft.mode;
        self.options.compare.tolerance = draft.tolerance;
        self.options.compare.blend_percent = draft.blend_percent;
        self.options.compare.ghost = draft.ghost;
        self.options.compare.ignore_unimportant = draft.ignore_unimportant;
        self.options.compare.auto_scale = draft.auto_scale;
        self.options.compare.offset_x = draft.offset_x;
        self.options.compare.offset_y = draft.offset_y;
        self.options.fit_to_window = draft.fit_to_window;
        self.options.show_tags = draft.show_tags;
        self.recompare();
    }

    pub fn sync_preferences(&self, prefs: &mut crate::i18n::AppPreferences) {
        prefs.picture_default_mode = match self.options.compare.mode {
            CompareMode::Tolerance => 0,
            CompareMode::MismatchRange => 1,
            CompareMode::Blend => 2,
        };
        prefs.picture_tolerance = self.options.compare.tolerance;
        prefs.picture_ghost = self.options.compare.ghost;
        prefs.picture_auto_scale = self.options.compare.auto_scale;
        prefs.picture_fit_to_window = self.options.fit_to_window;
    }

    pub fn apply_preferences(&mut self, prefs: &crate::i18n::AppPreferences) {
        self.options.compare.mode = match prefs.picture_default_mode {
            1 => CompareMode::MismatchRange,
            2 => CompareMode::Blend,
            _ => CompareMode::Tolerance,
        };
        self.options.compare.tolerance = prefs.picture_tolerance;
        self.options.compare.ghost = prefs.picture_ghost;
        self.options.compare.auto_scale = prefs.picture_auto_scale;
        self.options.fit_to_window = prefs.picture_fit_to_window;
        self.recompare();
    }

    pub fn is_side_empty(&self, left: bool) -> bool {
        if left {
            self.left_path.is_empty() && self.left.is_none()
        } else {
            self.right_path.is_empty() && self.right.is_none()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.is_side_empty(true) && self.is_side_empty(false)
    }

    pub fn record_paths(&self) -> Option<(PathBuf, PathBuf)> {
        Some((
            self.left.as_ref()?.path.clone()?,
            self.right.as_ref()?.path.clone()?,
        ))
    }

    pub fn session_options(&self) -> serde_json::Value {
        serde_json::to_value(&self.options).unwrap_or_default()
    }

    pub fn apply_session_options(&mut self, value: &serde_json::Value) {
        if let Ok(opts) = serde_json::from_value::<SavedOptions>(value.clone()) {
            self.options = opts;
            self.recompare();
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn load_files(&mut self, left: &Path, right: &Path) -> Result<(), String> {
        let (a, b) = std::thread::scope(|scope| {
            let hb = scope.spawn(|| PictureImage::load(right));
            let a = PictureImage::load(left);
            (a, hb.join().unwrap())
        });
        let a = a?;
        let b = b?;
        self.left = Some(a);
        self.right = Some(b);
        self.left_path = left.to_string_lossy().into_owned();
        self.right_path = right.to_string_lossy().into_owned();
        self.remember_path(&self.left_path.clone());
        self.remember_path(&self.right_path.clone());
        self.options.left_transform = ImageTransform::default();
        self.options.right_transform = ImageTransform::default();
        self.options.compare.offset_x = 0;
        self.options.compare.offset_y = 0;
        self.pan = Vec2::ZERO;
        self.error = None;
        self.recompare();
        Ok(())
    }

    pub fn open_side(&mut self, left: bool, path: &Path, dialogs: &mut dyn DialogHost) {
        match PictureImage::load(path) {
            Ok(img) => {
                let path_str = path.to_string_lossy().into_owned();
                if left {
                    self.left = Some(img);
                    self.left_path = path_str.clone();
                    self.options.left_transform = ImageTransform::default();
                } else {
                    self.right = Some(img);
                    self.right_path = path_str.clone();
                    self.options.right_transform = ImageTransform::default();
                }
                self.remember_path(&path_str);
                self.active_left = left;
                self.error = None;
                self.recompare();
            }
            Err(error) => {
                self.error = Some(error.clone());
                dialogs.error(&error);
            }
        }
    }

    fn remember_path(&mut self, path: &str) {
        if !path.is_empty() && !self.recent_paths.iter().any(|p| p == path) {
            self.recent_paths.insert(0, path.to_owned());
            self.recent_paths.truncate(20);
        }
    }

    fn reload(&mut self, dialogs: &mut dyn DialogHost) {
        let left = self.left.as_ref().and_then(|i| i.path.clone());
        let right = self.right.as_ref().and_then(|i| i.path.clone());
        if let Some(path) = left {
            self.open_side(true, &path, dialogs);
        }
        if let Some(path) = right {
            self.open_side(false, &path, dialogs);
        }
    }

    fn swap_sides(&mut self) {
        std::mem::swap(&mut self.left, &mut self.right);
        std::mem::swap(&mut self.left_path, &mut self.right_path);
        std::mem::swap(
            &mut self.options.left_transform,
            &mut self.options.right_transform,
        );
        self.options.compare.offset_x = -self.options.compare.offset_x;
        self.options.compare.offset_y = -self.options.compare.offset_y;
        self.recompare();
    }

    fn pick(&mut self, left: bool, dialogs: &mut dyn DialogHost) {
        let current = PathBuf::from(if left {
            &self.left_path
        } else {
            &self.right_path
        });
        if let Some(path) = dialogs.file(
            false,
            left,
            (!current.as_os_str().is_empty()).then_some(current.as_path()),
        ) {
            self.open_side(left, &path, dialogs);
        }
    }

    fn recompare(&mut self) {
        let started = std::time::Instant::now();
        if self.left.is_none() && self.right.is_none() {
            self.comparison = None;
        } else {
            self.comparison = Some(compare_pictures(
                self.left.as_ref(),
                self.right.as_ref(),
                self.options.left_transform,
                self.options.right_transform,
                self.options.compare,
            ));
        }
        self.compare_time_ms = started.elapsed().as_millis();
        self.revision = self.revision.wrapping_add(1);
    }

    fn ensure_textures(&mut self, ctx: &Context) {
        if self.checker_tex.is_none() {
            let mut pixels = vec![Color32::WHITE; 16 * 16];
            for y in 0..16 {
                for x in 0..16 {
                    let dark = ((x / 8) + (y / 8)) % 2 == 1;
                    pixels[y * 16 + x] = if dark {
                        Color32::from_gray(204)
                    } else {
                        Color32::from_gray(244)
                    };
                }
            }
            let image = ColorImage {
                size: [16, 16],
                pixels,
            };
            let opts = TextureOptions {
                magnification: TextureFilter::Nearest,
                minification: TextureFilter::Nearest,
                wrap_mode: TextureWrapMode::Repeat,
                mipmap_mode: None,
            };
            self.checker_tex = Some(ctx.load_texture("pix_checkerboard", image, opts));
        }
        if self.textures_rev == self.revision {
            return;
        }
        self.textures_rev = self.revision;
        let opts = TextureOptions {
            magnification: TextureFilter::Nearest,
            minification: TextureFilter::Linear,
            wrap_mode: TextureWrapMode::ClampToEdge,
            mipmap_mode: None,
        };
        if let Some(cmp) = &self.comparison {
            self.left_tex = cmp
                .left_surface
                .as_ref()
                .map(|s| ctx.load_texture("pix_left", surface_to_color_image(s), opts));
            self.right_tex = cmp
                .right_surface
                .as_ref()
                .map(|s| ctx.load_texture("pix_right", surface_to_color_image(s), opts));
            self.diff_tex =
                Some(ctx.load_texture("pix_diff", surface_to_color_image(&cmp.diff_surface), opts));
        } else {
            self.left_tex = None;
            self.right_tex = None;
            self.diff_tex = None;
        }
    }

    fn active_transform_mut(&mut self) -> &mut ImageTransform {
        if self.active_left {
            &mut self.options.left_transform
        } else {
            &mut self.options.right_transform
        }
    }

    fn zoom_by(&mut self, factor: f32) {
        self.options.fit_to_window = false;
        self.options.zoom = (self.options.zoom * factor).clamp(0.05, 32.0);
    }

    fn rules_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        let mode_index = match self.options.compare.mode {
            CompareMode::Tolerance => 0,
            CompareMode::MismatchRange => 1,
            CompareMode::Blend => 2,
        };
        let mut fields = [
            DialogField::Choice(
                "common.comparison_mode".into(),
                mode_index,
                vec![
                    "picture.mode.tolerance".into(),
                    "picture.mode.range".into(),
                    "picture.mode.blend".into(),
                ],
            ),
            DialogField::Text(
                "picture.tolerance_0_255".into(),
                self.options.compare.tolerance.to_string(),
            ),
            DialogField::Check("picture.ghost".into(), self.options.compare.ghost),
            DialogField::Check(
                "table.ignore_unimportant".into(),
                self.options.compare.ignore_unimportant,
            ),
            DialogField::Check("picture.auto_scale".into(), self.options.compare.auto_scale),
        ];
        if dialogs.fields("common.comparison_rules", &mut fields) {
            if let DialogField::Choice(_, idx, _) = fields[0] {
                self.options.compare.mode = match idx {
                    1 => CompareMode::MismatchRange,
                    2 => CompareMode::Blend,
                    _ => CompareMode::Tolerance,
                };
            }
            if let DialogField::Text(_, ref text) = fields[1] {
                if let Ok(val) = text.trim().parse::<u8>() {
                    self.options.compare.tolerance = val;
                }
            }
            if let DialogField::Check(_, v) = fields[2] {
                self.options.compare.ghost = v;
            }
            if let DialogField::Check(_, v) = fields[3] {
                self.options.compare.ignore_unimportant = v;
            }
            if let DialogField::Check(_, v) = fields[4] {
                self.options.compare.auto_scale = v;
            }
            self.recompare();
        }
    }

    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> bool {
        let mut home = false;
        ui.spacing_mut().item_spacing = Vec2::ZERO;

        // Keyboard shortcuts
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
            self.reload(dialogs);
        }
        if ctx.input_mut(|i| {
            i.consume_key(Modifiers::CTRL, Key::Plus) || i.consume_key(Modifiers::CTRL, Key::Equals)
        }) {
            self.zoom_by(1.25);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Minus)) {
            self.zoom_by(0.8);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Num0)) {
            self.options.fit_to_window = false;
            self.options.zoom = 1.0;
            self.pan = Vec2::ZERO;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::B)) {
            self.toggle_blend();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F11)) {
            self.full_screen = !self.full_screen;
        }

        let has_active = if self.active_left {
            self.left.is_some()
        } else {
            self.right.is_some()
        };

        // Menu bar
        TopBottomPanel::top("pix_menu")
            .exact_height(chrome::MENU_BAND_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                chrome::menu_background(ui);
                chrome::menu_bar(ui, |ui| {
                    chrome::menu_button(ui, "menu.session", |ui| {
                        session_menu::prefix(ui, icons, true);
                        if chrome::menu_item(ui, icons, "menu.session.settings", true)
                            .clicked()
                        {
                            if dialogs.persist_settings() {
                                super::session_settings::request(ui.ctx());
                            } else {
                                self.rules_dialog(dialogs);
                            }
                            ui.close_menu();
                        }
                        session_menu::close(ui, icons);
                        ui.separator();
                        if chrome::menu_item(ui, icons, "menu.edit.swap_sides", true).clicked() {
                            self.swap_sides();
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.file.reload_files  F5",
                            self.left.is_some() || self.right.is_some(),
                        )
                        .clicked()
                        {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_item(ui, icons, "picture.report", true).clicked() {
                            self.report(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "picture.compare_info", true).clicked() {
                            self.open_info_modal(ctx);
                            ui.close_menu();
                        }
                        session_menu::suffix(
                            ui,
                            icons,
                            self.left.is_some() && self.right.is_some(),
                        );
                    });
                    chrome::menu_button(ui, "menu.file", |ui| {
                        if chrome::menu_item(ui, icons, "menu.file.open_left", true).clicked()
                        {
                            self.pick(true, dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "menu.file.open_right", true).clicked()
                        {
                            self.pick(false, dialogs);
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "menu.edit", |ui| {
                        if chrome::menu_item(ui, icons, "picture.rotate_left", has_active).clicked() {
                            self.active_transform_mut().rotate_left();
                            self.recompare();
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "picture.rotate_right", has_active).clicked() {
                            self.active_transform_mut().rotate_right();
                            self.recompare();
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "picture.flip_h", has_active)
                            .clicked()
                        {
                            let tf = self.active_transform_mut();
                            tf.flip_h = !tf.flip_h;
                            self.recompare();
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "picture.flip_v", has_active)
                            .clicked()
                        {
                            let tf = self.active_transform_mut();
                            tf.flip_v = !tf.flip_v;
                            self.recompare();
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_item(
                            ui,
                            icons,
                            "picture.reset_diff_offset",
                            self.options.compare.offset_x != 0
                                || self.options.compare.offset_y != 0,
                        )
                        .clicked()
                        {
                            self.reset_difference_offset();
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "menu.view", |ui| {
                        let modes = [
                            ("picture.mode.tolerance", CompareMode::Tolerance),
                            ("picture.mode.range", CompareMode::MismatchRange),
                            ("picture.mode.blend", CompareMode::Blend),
                        ];
                        for (label, mode) in modes {
                            if chrome::menu_radio_item(
                                ui,
                                icons,
                                label,
                                self.options.compare.mode == mode,
                                true,
                            )
                            .clicked()
                            {
                                self.options.compare.mode = mode;
                                self.recompare();
                                ui.close_menu();
                            }
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "picture.blend_toggle  Ctrl+B",
                            self.options.compare.mode == CompareMode::Blend,
                            true,
                        )
                        .clicked()
                        {
                            self.toggle_blend();
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "picture.ghost",
                            self.options.compare.ghost,
                            self.options.compare.mode == CompareMode::Tolerance,
                        )
                        .clicked()
                        {
                            self.options.compare.ghost = !self.options.compare.ghost;
                            self.recompare();
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "table.ignore_unimportant",
                            self.options.compare.ignore_unimportant,
                            true,
                        )
                        .clicked()
                        {
                            self.options.compare.ignore_unimportant =
                                !self.options.compare.ignore_unimportant;
                            self.recompare();
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "picture.auto_scale",
                            self.options.compare.auto_scale,
                            true,
                        )
                        .clicked()
                        {
                            self.options.compare.auto_scale = !self.options.compare.auto_scale;
                            self.recompare();
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "picture.compare_metadata",
                            self.options.show_tags,
                            true,
                        )
                        .clicked()
                        {
                            self.options.show_tags = !self.options.show_tags;
                            ui.close_menu();
                        }
                        ui.separator();
                        chrome::submenu_item(ui, icons, "picture.zoom_menu", |ui, icons| {
                            if chrome::menu_item(ui, icons, "picture.zoom_in  Ctrl++", true)
                                .clicked()
                            {
                                self.zoom_by(1.25);
                                ui.close_menu();
                            }
                            if chrome::menu_item(ui, icons, "picture.zoom_out  Ctrl+-", true)
                                .clicked()
                            {
                                self.zoom_by(0.8);
                                ui.close_menu();
                            }
                            if chrome::menu_item(
                                ui,
                                icons,
                                "picture.zoom_actual_label  Ctrl+0",
                                true,
                            )
                            .clicked()
                            {
                                self.options.fit_to_window = false;
                                self.options.zoom = 1.0;
                                self.pan = Vec2::ZERO;
                                ui.close_menu();
                            }
                            if chrome::menu_check_item(
                                ui,
                                icons,
                                "picture.zoom_fit_label",
                                self.options.fit_to_window,
                                true,
                            )
                            .clicked()
                            {
                                self.options.fit_to_window = true;
                                self.pan = Vec2::ZERO;
                                ui.close_menu();
                            }
                        });
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "picture.full_screen  F11",
                            self.full_screen,
                            true,
                        )
                        .clicked()
                        {
                            self.full_screen = !self.full_screen;
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "view.file_info",
                            self.show_file_info,
                            true,
                        )
                        .clicked()
                        {
                            self.show_file_info = !self.show_file_info;
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "view.toolbar",
                            self.show_toolbar,
                            true,
                        )
                        .clicked()
                        {
                            self.show_toolbar = !self.show_toolbar;
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "menu.tools", |ui| {
                        if chrome::menu_item(ui, icons, "menu.session.rules_ellipsis", true).clicked()
                        {
                            if dialogs.persist_settings() {
                                super::session_settings::request(ui.ctx());
                            } else {
                                self.rules_dialog(dialogs);
                            }
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "menu.tools.options", true).clicked() {
                            if dialogs.persist_settings() {
                                super::options_dialog::request(
                                    ui.ctx(),
                                    super::options_dialog::OptionsCategory::PictureCompare,
                                );
                            } else {
                                crate::i18n::configure(dialogs);
                            }
                            ui.close_menu();
                        }
                        session_menu::settings_tools(ui, icons, dialogs);
                    });
                    chrome::menu_button(ui, "menu.help", |ui| {
                        if chrome::menu_item(ui, icons, "menu.help.about", true).clicked() {
                            super::info_modals::open_about(ui.ctx());
                            ui.close_menu();
                        }
                    });
                });
            });

        session_tabs::show(ui, icons);

        // Standard BC5 single-row Picture Compare toolbar (`toolbar_spec::PICTURE_COMPARE`)
        let mode = self.options.compare.mode;
        let ignore_minor = self.options.compare.ignore_unimportant;
        let show_tags = self.options.show_tags;
        let has_any = self.left.is_some() || self.right.is_some();
        let state = |name: &str| match name {
            "SessionHome" | "SessionsCombo" | "Rules" | "Swap" => ToolState::enabled(true),
            "DiffTolerance" => ToolState::checked(mode == CompareMode::Tolerance),
            "DiffRange" => ToolState::checked(mode == CompareMode::MismatchRange),
            "DiffBlend" => ToolState::checked(mode == CompareMode::Blend),
            "InsigIgnored" => ToolState::checked(ignore_minor),
            "Reload" => ToolState::enabled(has_any),
            "CompareTags" => ToolState::checked(show_tags),
            "FileFormat" => ToolState::DISABLED,
            _ => ToolState::DISABLED,
        };
        let mut toolbar_event = None;
        if self.show_toolbar && !self.full_screen {
            let toolbar_height = toolbar::height(
                ui,
                toolbar_spec::PICTURE_COMPARE,
                ui.available_width(),
                &state,
            );
            TopBottomPanel::top("pix_toolbar")
                .exact_height(toolbar_height)
                .show_separator_line(false)
                .frame(Frame::none())
                .show_inside(ui, |ui| {
                    toolbar_event = toolbar::show(
                        ui,
                        icons,
                        toolbar_spec::PICTURE_COMPARE,
                        self.active_left,
                        &state,
                        &mut |_, _, _| {},
                    );
                });
        }
        if let Some(ToolEvent::Click(command)) = toolbar_event {
            match command {
                "SessionHome" => home = true,
                "DiffTolerance" => {
                    self.options.compare.mode = CompareMode::Tolerance;
                    self.recompare();
                }
                "DiffRange" => {
                    self.options.compare.mode = CompareMode::MismatchRange;
                    self.recompare();
                }
                "DiffBlend" => {
                    self.options.compare.mode = CompareMode::Blend;
                    self.recompare();
                }
                "InsigIgnored" => {
                    self.options.compare.ignore_unimportant =
                        !self.options.compare.ignore_unimportant;
                    self.recompare();
                }
                "Rules" => {
                    if dialogs.persist_settings() {
                        super::session_settings::request(ctx);
                    } else {
                        self.rules_dialog(dialogs);
                    }
                }
                "Swap" => self.swap_sides(),
                "Reload" => self.reload(dialogs),
                "CompareTags" => self.options.show_tags = !self.options.show_tags,
                _ => {}
            }
        }

        // Path bars (Left / Right)
        TopBottomPanel::top("pix_paths")
            .frame(
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(2.0, 2.0)),
            )
            .show_inside(ui, |ui| {
                let mut open_req: Option<(bool, PathBuf)> = None;
                let mut pick_req: Option<bool> = None;
                ui.columns(2, |cols| {
                    for (idx, col) in cols.iter_mut().enumerate() {
                        let left = idx == 0;
                        col.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let has_loaded = if left {
                                self.left.is_some()
                            } else {
                                self.right.is_some()
                            };
                            let path_str = if left {
                                &mut self.left_path
                            } else {
                                &mut self.right_path
                            };
                            let width = (ui.available_width() - 50.0).max(60.0);
                            let out = history_combo::show(
                                ui,
                                ("pix_path", idx),
                                path_str,
                                &self.recent_paths,
                                width,
                                "common.enter_filename_here",
                            );
                            if out.response.has_focus() {
                                self.active_left = left;
                            }
                            if let Some(selected) = out.selected {
                                *path_str = selected.clone();
                                open_req = Some((left, PathBuf::from(selected)));
                            } else if out.submitted && !path_str.is_empty() {
                                open_req = Some((left, PathBuf::from(path_str.as_str())));
                            }
                            let path_copy = path_str.clone();
                            let pending_load = !path_copy.trim().is_empty() && !has_loaded;
                            let (_, browse_act) = super::chrome::file_browse_buttons(
                                ui,
                                icons,
                                ("pix_browse", idx),
                                pending_load,
                                false,
                                false,
                                false,
                                has_loaded && !path_copy.trim().is_empty(),
                            );
                            match browse_act {
                                Some(super::chrome::BrowseMenuAction::LoadTyped) => {
                                    if !path_copy.trim().is_empty() {
                                        open_req = Some((left, PathBuf::from(path_copy)));
                                    }
                                }
                                Some(
                                    super::chrome::BrowseMenuAction::OpenFileSystem
                                    | super::chrome::BrowseMenuAction::OpenProfile,
                                ) => {
                                    pick_req = Some(left);
                                }
                                Some(super::chrome::BrowseMenuAction::OpenWithTextEdit) => {
                                    if !path_copy.trim().is_empty() {
                                        super::session_menu::request(
                                            ctx,
                                            super::session_menu::Request::EditText(PathBuf::from(
                                                &path_copy,
                                            )),
                                        );
                                    }
                                }
                                Some(super::chrome::BrowseMenuAction::OpenWithAssociated) => {
                                    if !path_copy.trim().is_empty() {
                                        if let Err(err) =
                                            dialogs.open_path(Path::new(&path_copy), false)
                                        {
                                            dialogs.error(&err);
                                        }
                                    }
                                }
                                Some(super::chrome::BrowseMenuAction::OpenClipboard) | None => {}
                            }
                        });
                    }
                });
                if let Some((left, path)) = open_req {
                    self.open_side(left, &path, dialogs);
                }
                if let Some(left) = pick_req {
                    self.pick(left, dialogs);
                }
            });

        // Status bar
        TopBottomPanel::bottom("pix_status")
            .exact_height(22.0)
            .frame(
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(6.0, 2.0)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    if let Some(error) = &self.error {
                        ui.colored_label(Color32::DARK_RED, error);
                    } else if let Some(cmp) = &self.comparison {
                        let s = cmp.stats;
                        ui.label(format!(
                            "{}: {} ({:.2}%)  |  {}: {}  |  {}: {} ({:.2}%)  |  {}×{}  |  {} ms",
                            tr("common.same"),
                            grouped(s.same_pixels),
                            s.same_percent(),
                            tr("picture.tolerated"),
                            grouped(s.tolerated_pixels),
                            tr("picture.different"),
                            grouped(s.different_pixels + s.left_only_pixels + s.right_only_pixels),
                            s.diff_percent(),
                            cmp.diff_surface.width,
                            cmp.diff_surface.height,
                            self.compare_time_ms,
                        ));
                    } else {
                        ui.label(tr(
                            "picture.drag_two_images_or_choose_files_above_to",
                        ));
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        let zoom_label = if self.options.fit_to_window {
                            tr("picture.fit")
                        } else {
                            format!("{:.0}%", self.options.zoom * 100.0)
                        };
                        ui.label(zoom_label);
                        if let Some(info) = self.hovered_pixel_summary() {
                            ui.separator();
                            ui.label(info);
                        }
                    });
                });
                super::chrome::status_bar_grip(ui.painter(), ui.max_rect());
            });

        // Optional Properties/Tags bottom drawer
        if self.options.show_tags {
            TopBottomPanel::bottom("pix_tags_panel")
                .resizable(true)
                .default_height(130.0)
                .min_height(80.0)
                .max_height(280.0)
                .frame(
                    Frame::none()
                        .fill(Color32::WHITE)
                        .inner_margin(Margin::symmetric(6.0, 4.0)),
                )
                .show_inside(ui, |ui| {
                    self.render_tags(ui);
                });
        }

        // Main image viewport area: Top (Left | Right) + Bottom (Difference)
        self.ensure_textures(ctx);
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::from_gray(180)))
            .show_inside(ui, |ui| {
                let avail = ui.available_rect_before_wrap();
                let split_h = 5.0;
                let view_bar_h = 28.0;
                let top_h = ((avail.height() - split_h - view_bar_h) * self.split_ratio).clamp(
                    80.0,
                    (avail.height() - split_h - view_bar_h - 80.0).max(80.0),
                );
                let half_w = (avail.width() - 3.0).max(40.0) / 2.0;

                let left_rect = Rect::from_min_size(avail.min, vec2(half_w, top_h));
                let right_rect = Rect::from_min_size(
                    pos2(avail.left() + half_w + 3.0, avail.top()),
                    vec2(half_w, top_h),
                );
                let splitter_rect = Rect::from_min_size(
                    pos2(avail.left(), avail.top() + top_h),
                    vec2(avail.width(), split_h),
                );
                let view_bar_rect = Rect::from_min_size(
                    pos2(avail.left(), splitter_rect.bottom()),
                    vec2(avail.width(), view_bar_h),
                );
                let diff_rect =
                    Rect::from_min_max(pos2(avail.left(), view_bar_rect.bottom()), avail.max);

                // Horizontal splitter between source pair and diff view
                let split_resp =
                    ui.interact(splitter_rect, ui.id().with("pix_h_split"), Sense::drag());
                if split_resp.dragged() {
                    let delta = ui.input(|i| i.pointer.delta().y);
                    let denom = (avail.height() - split_h - view_bar_h).max(1.0);
                    self.split_ratio = (self.split_ratio + delta / denom).clamp(0.18, 0.82);
                }
                split_resp.on_hover_cursor(CursorIcon::ResizeVertical);
                ui.painter()
                    .rect_filled(splitter_rect, 0.0, BcTheme::WINDOW_BG);
                ui.painter().line_segment(
                    [splitter_rect.left_top(), splitter_rect.right_top()],
                    Stroke::new(1.0_f32, Color32::from_gray(180)),
                );
                ui.painter().line_segment(
                    [splitter_rect.left_bottom(), splitter_rect.right_bottom()],
                    Stroke::new(1.0_f32, Color32::from_gray(210)),
                );

                // BC5 CenterPanel -> ViewPanel: Zoom + Tolerance/Blend + Transform/Offset strip
                ui.painter()
                    .rect_filled(view_bar_rect, 0.0, BcTheme::WINDOW_BG);
                ui.painter().line_segment(
                    [view_bar_rect.left_bottom(), view_bar_rect.right_bottom()],
                    Stroke::new(1.0_f32, Color32::from_gray(180)),
                );
                let mut bar_ui = ui.new_child(
                    UiBuilder::new()
                        .max_rect(view_bar_rect.shrink2(vec2(6.0, 2.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                bar_ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                if tool_btn(
                    &mut bar_ui,
                    icons,
                    "imgPixZoomActual",
                    "100%",
                    !self.options.fit_to_window && (self.options.zoom - 1.0).abs() < 0.01,
                    true,
                ) {
                    self.options.fit_to_window = false;
                    self.options.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }
                if tool_btn(
                    &mut bar_ui,
                    icons,
                    "imgPixZoomFit",
                    "picture.fit",
                    self.options.fit_to_window,
                    true,
                ) {
                    self.options.fit_to_window = true;
                    self.pan = Vec2::ZERO;
                }
                if icon_small_btn(&mut bar_ui, icons, "imgZoomOut", "picture.zoom_out", true) {
                    self.zoom_by(0.8);
                }
                if icon_small_btn(&mut bar_ui, icons, "imgZoomIn", "picture.zoom_in", true) {
                    self.zoom_by(1.25);
                }
                bar_ui.separator();
                match self.options.compare.mode {
                    CompareMode::Tolerance => {
                        if tool_btn(
                            &mut bar_ui,
                            icons,
                            "imgPixToleranceGhost",
                            "picture.ghost",
                            self.options.compare.ghost,
                            true,
                        ) {
                            self.options.compare.ghost = !self.options.compare.ghost;
                            self.recompare();
                        }
                        bar_ui.label(tr("common.tolerance"));
                        let mut tol = self.options.compare.tolerance;
                        if bar_ui
                            .add(Slider::new(&mut tol, 0..=255).show_value(true))
                            .changed()
                        {
                            self.options.compare.tolerance = tol;
                            self.recompare();
                        }
                    }
                    CompareMode::Blend => {
                        if tool_btn(
                            &mut bar_ui,
                            icons,
                            "imgPixBlendToggle",
                            "picture.toggle",
                            false,
                            true,
                        ) {
                            self.options.compare.blend_percent =
                                if self.options.compare.blend_percent < 50 {
                                    100
                                } else {
                                    0
                                };
                            self.recompare();
                        }
                        bar_ui.label(tr("picture.blend_mesclagem"));
                        let mut pct = self.options.compare.blend_percent;
                        if bar_ui
                            .add(Slider::new(&mut pct, 0..=100).suffix("%"))
                            .changed()
                        {
                            self.options.compare.blend_percent = pct;
                            self.recompare();
                        }
                    }
                    CompareMode::MismatchRange => {
                        bar_ui.label(tr("picture.mismatch_range"));
                    }
                }
                bar_ui.separator();
                if tool_btn(
                    &mut bar_ui,
                    icons,
                    "imgPixAutoScale",
                    "picture.auto_scale_short",
                    self.options.compare.auto_scale,
                    true,
                ) {
                    self.options.compare.auto_scale = !self.options.compare.auto_scale;
                    self.recompare();
                }
                if icon_small_btn(
                    &mut bar_ui,
                    icons,
                    "imgPixRotateLeft",
                    "picture.rotate_left_girar_esq",
                    has_active,
                ) {
                    self.active_transform_mut().rotate_left();
                    self.recompare();
                }
                if icon_small_btn(
                    &mut bar_ui,
                    icons,
                    "imgPixRotateRight",
                    "picture.rotate_right_girar_dir",
                    has_active,
                ) {
                    self.active_transform_mut().rotate_right();
                    self.recompare();
                }
                if icon_small_btn(
                    &mut bar_ui,
                    icons,
                    "imgPixFlipHorz",
                    "picture.flip_horiz",
                    has_active,
                ) {
                    let tf = self.active_transform_mut();
                    tf.flip_h = !tf.flip_h;
                    self.recompare();
                }
                if icon_small_btn(
                    &mut bar_ui,
                    icons,
                    "imgPixFlipVert",
                    "picture.flip_vert",
                    has_active,
                ) {
                    let tf = self.active_transform_mut();
                    tf.flip_v = !tf.flip_v;
                    self.recompare();
                }
                bar_ui.separator();
                bar_ui.label("ΔX:");
                let mut ox = self.options.compare.offset_x;
                if bar_ui
                    .add(DragValue::new(&mut ox).range(-4096..=4096))
                    .changed()
                {
                    self.options.compare.offset_x = ox;
                    self.recompare();
                }
                bar_ui.label("ΔY:");
                let mut oy = self.options.compare.offset_y;
                if bar_ui
                    .add(DragValue::new(&mut oy).range(-4096..=4096))
                    .changed()
                {
                    self.options.compare.offset_y = oy;
                    self.recompare();
                }
                if (self.options.compare.offset_x != 0 || self.options.compare.offset_y != 0)
                    && bar_ui.small_button(tr("picture.reset")).clicked()
                {
                    self.options.compare.offset_x = 0;
                    self.options.compare.offset_y = 0;
                    self.recompare();
                }

                // Vertical divider between left & right image panes
                let v_div =
                    Rect::from_min_size(pos2(left_rect.right(), left_rect.top()), vec2(3.0, top_h));
                ui.painter().rect_filled(v_div, 0.0, BcTheme::WINDOW_BG);

                let (canvas_w, canvas_h, left_origin, right_origin) = match &self.comparison {
                    Some(c) => (
                        c.diff_surface.width as f32,
                        c.diff_surface.height as f32,
                        c.left_origin,
                        c.right_origin,
                    ),
                    None => (1.0, 1.0, (0, 0), (0, 0)),
                };

                let fit_to_window = self.options.fit_to_window;
                let zoom = self.options.zoom;
                let base_scale = |rect: Rect| -> f32 {
                    if fit_to_window {
                        let sx = (rect.width() - 8.0).max(1.0) / canvas_w.max(1.0);
                        let sy = (rect.height() - 8.0).max(1.0) / canvas_h.max(1.0);
                        sx.min(sy).clamp(0.05, 16.0)
                    } else {
                        zoom
                    }
                };

                // Clone handles (cheap Arc clones) so we can borrow &mut self cleanly.
                let left_tex = self.left_tex.clone();
                let right_tex = self.right_tex.clone();
                let diff_tex = self.diff_tex.clone();
                let checker_tex = self.checker_tex.clone();
                let left_badge = self.side_caption(true);
                let right_badge = self.side_caption(false);
                let diff_badge = match self.options.compare.mode {
                    CompareMode::Tolerance => format!(
                        "{} ({}{})",
                        tr("picture.tolerance"),
                        self.options.compare.tolerance,
                        if self.options.compare.ghost {
                            ", Ghost"
                        } else {
                            ""
                        }
                    ),
                    CompareMode::MismatchRange => tr("picture.mismatch_range"),
                    CompareMode::Blend => {
                        format!(
                            "{} ({}%)",
                            tr("picture.blend"),
                            self.options.compare.blend_percent
                        )
                    }
                };

                let mut new_hover: Option<(i32, i32)> = None;
                let mut offset_delta = (0i32, 0i32);
                let mut pix_ctx: Option<PictureCtxCmd> = None;

                if let Some(cmd) = self.paint_image_pane(
                    ui,
                    icons,
                    left_rect,
                    "pix_pane_left",
                    left_tex.as_ref(),
                    checker_tex.as_ref(),
                    left_origin,
                    (canvas_w, canvas_h),
                    base_scale(left_rect),
                    &left_badge,
                    self.active_left,
                    false,
                    &mut new_hover,
                    &mut offset_delta,
                    Some(true),
                ) {
                    pix_ctx = Some(cmd);
                }
                if let Some(cmd) = self.paint_image_pane(
                    ui,
                    icons,
                    right_rect,
                    "pix_pane_right",
                    right_tex.as_ref(),
                    checker_tex.as_ref(),
                    right_origin,
                    (canvas_w, canvas_h),
                    base_scale(right_rect),
                    &right_badge,
                    !self.active_left,
                    false,
                    &mut new_hover,
                    &mut offset_delta,
                    Some(false),
                ) {
                    pix_ctx = Some(cmd);
                }
                if let Some(cmd) = self.paint_image_pane(
                    ui,
                    icons,
                    diff_rect,
                    "pix_pane_diff",
                    diff_tex.as_ref(),
                    checker_tex.as_ref(),
                    (0, 0),
                    (canvas_w, canvas_h),
                    base_scale(diff_rect),
                    &diff_badge,
                    false,
                    true,
                    &mut new_hover,
                    &mut offset_delta,
                    None,
                ) {
                    pix_ctx = Some(cmd);
                }

                self.hover_pixel = new_hover;
                if offset_delta != (0, 0) {
                    self.options.compare.offset_x =
                        (self.options.compare.offset_x + offset_delta.0).clamp(-4096, 4096);
                    self.options.compare.offset_y =
                        (self.options.compare.offset_y + offset_delta.1).clamp(-4096, 4096);
                    self.recompare();
                }
                if let Some(cmd) = pix_ctx {
                    let side_path = if self.active_left {
                        self.left.as_ref().and_then(|i| i.path.clone())
                    } else {
                        self.right.as_ref().and_then(|i| i.path.clone())
                    };
                    match cmd {
                        PictureCtxCmd::ZoomIn => self.zoom_by(1.25),
                        PictureCtxCmd::ZoomOut => self.zoom_by(0.8),
                        PictureCtxCmd::ZoomActual => {
                            self.options.fit_to_window = false;
                            self.options.zoom = 1.0;
                            self.pan = Vec2::ZERO;
                        }
                        PictureCtxCmd::ZoomFit => {
                            self.options.fit_to_window = true;
                            self.pan = Vec2::ZERO;
                        }
                        PictureCtxCmd::Mode(mode) => {
                            self.options.compare.mode = mode;
                            self.recompare();
                        }
                        PictureCtxCmd::ToggleGhost => {
                            self.options.compare.ghost = !self.options.compare.ghost;
                            self.recompare();
                        }
                        PictureCtxCmd::ToggleAutoScale => {
                            self.options.compare.auto_scale = !self.options.compare.auto_scale;
                            self.recompare();
                        }
                        PictureCtxCmd::ToggleProperties => {
                            self.options.show_tags = !self.options.show_tags;
                        }
                        PictureCtxCmd::RotateLeft => {
                            if self.active_left {
                                self.options.left_transform.rotate_left();
                            } else {
                                self.options.right_transform.rotate_left();
                            }
                            self.recompare();
                        }
                        PictureCtxCmd::RotateRight => {
                            if self.active_left {
                                self.options.left_transform.rotate_right();
                            } else {
                                self.options.right_transform.rotate_right();
                            }
                            self.recompare();
                        }
                        PictureCtxCmd::FlipH => {
                            if self.active_left {
                                self.options.left_transform.flip_h =
                                    !self.options.left_transform.flip_h;
                            } else {
                                self.options.right_transform.flip_h =
                                    !self.options.right_transform.flip_h;
                            }
                            self.recompare();
                        }
                        PictureCtxCmd::FlipV => {
                            if self.active_left {
                                self.options.left_transform.flip_v =
                                    !self.options.left_transform.flip_v;
                            } else {
                                self.options.right_transform.flip_v =
                                    !self.options.right_transform.flip_v;
                            }
                            self.recompare();
                        }
                        PictureCtxCmd::CopyFilename => {
                            if let Some(p) = side_path {
                                ctx.copy_text(p.display().to_string());
                            }
                        }
                        PictureCtxCmd::OpenHex => {
                            super::session_menu::request(
                                ctx,
                                super::session_menu::Request::CompareUsing(
                                    crate::history::SessionKind::Hex,
                                ),
                            );
                        }
                        PictureCtxCmd::AssocApp => {
                            if let Some(p) = side_path {
                                if let Err(err) = dialogs.open_path(&p, false) {
                                    dialogs.error(&err);
                                }
                            }
                        }
                        PictureCtxCmd::Explore(select) => {
                            if let Some(p) = side_path {
                                let target = if select {
                                    p
                                } else {
                                    p.parent().unwrap_or(&p).to_path_buf()
                                };
                                if let Err(err) = dialogs.open_path(&target, select) {
                                    dialogs.error(&err);
                                }
                            }
                        }
                    }
                }
            });

        home
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_image_pane(
        &mut self,
        ui: &mut Ui,
        icons: &mut IconManager,
        rect: Rect,
        id_salt: &str,
        tex: Option<&TextureHandle>,
        checker: Option<&TextureHandle>,
        origin_px: (i32, i32),
        canvas_size: (f32, f32),
        scale: f32,
        caption: &str,
        active_border: bool,
        allow_shift_offset: bool,
        hover_out: &mut Option<(i32, i32)>,
        offset_delta: &mut (i32, i32),
        pane_side: Option<bool>,
    ) -> Option<PictureCtxCmd> {
        let resp = ui.interact(rect, ui.id().with(id_salt), Sense::click_and_drag());
        if pane_side == Some(true) {
            ui.ctx().data_mut(|d| {
                d.insert_temp(Id::new("capture_menu_rect").with("pix_pane_left"), rect);
            });
        }
        if resp.clicked() || resp.secondary_clicked() {
            if let Some(left) = pane_side {
                self.active_left = left;
            }
        }
        let mut ctx_cmd: Option<PictureCtxCmd> = None;
        let has_side_path = if self.active_left {
            self.left.as_ref().and_then(|i| i.path.as_ref()).is_some()
        } else {
            self.right.as_ref().and_then(|i| i.path.as_ref()).is_some()
        };
        let both_paths = self.left.as_ref().and_then(|i| i.path.as_ref()).is_some()
            && self.right.as_ref().and_then(|i| i.path.as_ref()).is_some();
        let mode_now = self.options.compare.mode;
        let ghost_now = self.options.compare.ghost;
        let auto_scale_now = self.options.compare.auto_scale;
        let props_now = self.options.show_tags;
        resp.context_menu(|ui| {
            ui.set_min_width(210.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.spacing_mut().button_padding = vec2(4.0, 2.0);
            ui.spacing_mut().interact_size.y = 20.0;
            if super::chrome::menu_item(ui, icons, "picture.zoom_in", true).clicked() {
                ctx_cmd = Some(PictureCtxCmd::ZoomIn);
                ui.close_menu();
            }
            if super::chrome::menu_item(ui, icons, "picture.zoom_out", true).clicked() {
                ctx_cmd = Some(PictureCtxCmd::ZoomOut);
                ui.close_menu();
            }
            if super::chrome::menu_item(ui, icons, "picture.zoom_actual", true).clicked() {
                ctx_cmd = Some(PictureCtxCmd::ZoomActual);
                ui.close_menu();
            }
            if super::chrome::menu_item(ui, icons, "picture.zoom_fit", true).clicked() {
                ctx_cmd = Some(PictureCtxCmd::ZoomFit);
                ui.close_menu();
            }
            ui.separator();
            for (lbl, mode) in [
                ("picture.mode.tolerance", CompareMode::Tolerance),
                ("picture.mode.range", CompareMode::MismatchRange),
                ("picture.mode.blend", CompareMode::Blend),
            ] {
                if super::chrome::menu_radio_item(ui, icons, lbl, mode_now == mode, true).clicked()
                {
                    ctx_cmd = Some(PictureCtxCmd::Mode(mode));
                    ui.close_menu();
                }
            }
            if super::chrome::menu_check_item(ui, icons, "picture.ghost", ghost_now, true).clicked()
            {
                ctx_cmd = Some(PictureCtxCmd::ToggleGhost);
                ui.close_menu();
            }
            if super::chrome::menu_check_item(ui, icons, "picture.auto_scale", auto_scale_now, true)
                .clicked()
            {
                ctx_cmd = Some(PictureCtxCmd::ToggleAutoScale);
                ui.close_menu();
            }
            if super::chrome::menu_check_item(ui, icons, "picture.show_properties", props_now, true)
                .clicked()
            {
                ctx_cmd = Some(PictureCtxCmd::ToggleProperties);
                ui.close_menu();
            }
            ui.separator();
            if super::chrome::menu_item(ui, icons, "picture.rotate_left", true).clicked() {
                ctx_cmd = Some(PictureCtxCmd::RotateLeft);
                ui.close_menu();
            }
            if super::chrome::menu_item(ui, icons, "picture.rotate_right", true).clicked() {
                ctx_cmd = Some(PictureCtxCmd::RotateRight);
                ui.close_menu();
            }
            if super::chrome::menu_item(ui, icons, "picture.flip_h", true).clicked() {
                ctx_cmd = Some(PictureCtxCmd::FlipH);
                ui.close_menu();
            }
            if super::chrome::menu_item(ui, icons, "picture.flip_v", true).clicked() {
                ctx_cmd = Some(PictureCtxCmd::FlipV);
                ui.close_menu();
            }
            ui.separator();
            if super::chrome::menu_item(ui, icons, "folder.ctx.copy_filename", has_side_path)
                .clicked()
            {
                ctx_cmd = Some(PictureCtxCmd::CopyFilename);
                ui.close_menu();
            }
            super::chrome::submenu_item(ui, icons, "menu.actions.open_with", |ui, icons| {
                if super::chrome::menu_item(ui, icons, "session.hex_compare", both_paths).clicked()
                {
                    ctx_cmd = Some(PictureCtxCmd::OpenHex);
                    ui.close_menu();
                }
                ui.separator();
                if super::chrome::menu_item(ui, icons, "folder.ctx.associated_app", has_side_path)
                    .clicked()
                {
                    ctx_cmd = Some(PictureCtxCmd::AssocApp);
                    ui.close_menu();
                }
            });
            super::chrome::submenu_item(ui, icons, "table.explorer", |ui, icons| {
                if super::chrome::menu_item(
                    ui,
                    icons,
                    "version.select_in_explorer",
                    has_side_path,
                )
                .clicked()
                {
                    ctx_cmd = Some(PictureCtxCmd::Explore(true));
                    ui.close_menu();
                }
                if super::chrome::menu_item(ui, icons, "version.open_folder", has_side_path)
                    .clicked()
                {
                    ctx_cmd = Some(PictureCtxCmd::Explore(false));
                    ui.close_menu();
                }
            });
        });
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let factor = if scroll > 0.0 { 1.15 } else { 1.0 / 1.15 };
                self.options.fit_to_window = false;
                self.options.zoom = (scale * factor).clamp(0.05, 32.0);
            }
        }
        if resp.dragged() {
            let delta = ui.input(|i| i.pointer.delta());
            if allow_shift_offset && ui.input(|i| i.modifiers.shift) {
                offset_delta.0 += (delta.x / scale.max(0.1)).round() as i32;
                offset_delta.1 += (delta.y / scale.max(0.1)).round() as i32;
            } else {
                self.pan += delta;
            }
        }

        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(226));

        let canvas_screen_size = vec2(canvas_size.0 * scale, canvas_size.1 * scale);
        let canvas_top_left = rect.center() - canvas_screen_size * 0.5 + self.pan;

        if let Some(tex) = tex {
            let img_size = tex.size_vec2();
            let img_min =
                canvas_top_left + vec2(origin_px.0 as f32 * scale, origin_px.1 as f32 * scale);
            let img_rect = Rect::from_min_size(img_min, img_size * scale);

            if let Some(chk) = checker {
                let uv = Rect::from_min_max(
                    Pos2::ZERO,
                    pos2(
                        (img_rect.width() / 16.0).max(1.0),
                        (img_rect.height() / 16.0).max(1.0),
                    ),
                );
                painter.image(chk.id(), img_rect, uv, Color32::WHITE);
            }
            painter.image(
                tex.id(),
                img_rect,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            painter.rect_stroke(img_rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(120)));
        }

        // Update hovered canvas pixel coordinate
        if let Some(pointer) = resp.hover_pos() {
            let px = ((pointer.x - canvas_top_left.x) / scale).floor() as i32;
            let py = ((pointer.y - canvas_top_left.y) / scale).floor() as i32;
            if px >= 0 && py >= 0 && (px as f32) < canvas_size.0 && (py as f32) < canvas_size.1 {
                *hover_out = Some((px, py));
            }
        }

        // Mirrored shadow crosshair across all 3 panes (`TPixShadowCursor` in BC5!)
        if let Some((hx, hy)) = self.hover_pixel {
            let cx = canvas_top_left.x + (hx as f32 + 0.5) * scale;
            let cy = canvas_top_left.y + (hy as f32 + 0.5) * scale;
            if rect.contains(pos2(cx, cy)) {
                let stroke =
                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 120, 215, 190));
                painter.line_segment([pos2(cx - 8.0, cy), pos2(cx + 8.0, cy)], stroke);
                painter.line_segment([pos2(cx, cy - 8.0), pos2(cx, cy + 8.0)], stroke);
            }
        }

        // Subtle caption badge in top-left of each pane
        if !caption.is_empty() {
            let galley = painter.layout_no_wrap(
                caption.to_owned(),
                FontId::proportional(11.0),
                Color32::from_gray(30),
            );
            let badge =
                Rect::from_min_size(rect.min + vec2(4.0, 4.0), galley.size() + vec2(8.0, 4.0));
            painter.rect_filled(
                badge,
                2.0,
                Color32::from_rgba_unmultiplied(245, 245, 245, 215),
            );
            painter.rect_stroke(badge, 2.0, Stroke::new(1.0_f32, Color32::from_gray(180)));
            painter.galley(badge.min + vec2(4.0, 2.0), galley, Color32::BLACK);
        }

        if active_border {
            painter.rect_stroke(
                rect.shrink(0.5),
                0.0,
                Stroke::new(1.5_f32, Color32::from_rgb(0, 120, 212)),
            );
        }
        ctx_cmd
    }

    fn side_caption(&self, left: bool) -> String {
        let (img, tf, scale) = match (left, &self.comparison) {
            (true, Some(c)) => (&self.left, self.options.left_transform, c.left_scale_pct),
            (false, Some(c)) => (&self.right, self.options.right_transform, c.right_scale_pct),
            (true, None) => (&self.left, self.options.left_transform, 100),
            (false, None) => (&self.right, self.options.right_transform, 100),
        };
        let Some(img) = img else {
            return String::new();
        };
        let mut s = format!(
            "{}  {}×{}  {} bpp",
            img.format_name, img.width, img.height, img.bits_per_pixel
        );
        if scale != 100 {
            s.push_str(&format!("  ({scale}%)"));
        }
        if tf.rotation != 0 {
            s.push_str(&format!("  {}°", tf.rotation as u16 * 90));
        }
        if tf.flip_h {
            s.push_str("  FlipH");
        }
        if tf.flip_v {
            s.push_str("  FlipV");
        }
        s
    }

    fn hovered_pixel_summary(&self) -> Option<String> {
        let (hx, hy) = self.hover_pixel?;
        let cmp = self.comparison.as_ref()?;
        let lp = cmp.left_surface.as_ref().and_then(|s| {
            let x = hx - cmp.left_origin.0;
            let y = hy - cmp.left_origin.1;
            (x >= 0 && y >= 0)
                .then(|| s.pixel(x as u32, y as u32))
                .flatten()
        });
        let rp = cmp.right_surface.as_ref().and_then(|s| {
            let x = hx - cmp.right_origin.0;
            let y = hy - cmp.right_origin.1;
            (x >= 0 && y >= 0)
                .then(|| s.pixel(x as u32, y as u32))
                .flatten()
        });
        let fmt = |p: Option<[u8; 4]>| match p {
            Some([r, g, b, a]) => format!("({r},{g},{b},{a})"),
            None => "—".to_owned(),
        };
        Some(format!(
            "({hx}, {hy})  {}: {}  {}: {}",
            tr("picture.left_short"),
            fmt(lp),
            tr("picture.right_short"),
            fmt(rp)
        ))
    }

    fn render_tags(&self, ui: &mut Ui) {
        let rows: Vec<(&str, String, String)> = vec![
            (
                "common.format",
                self.left
                    .as_ref()
                    .map_or("-".into(), |i| i.format_name.clone()),
                self.right
                    .as_ref()
                    .map_or("-".into(), |i| i.format_name.clone()),
            ),
            (
                "picture.dimensions",
                self.left
                    .as_ref()
                    .map_or("-".into(), |i| format!("{} × {} px", i.width, i.height)),
                self.right
                    .as_ref()
                    .map_or("-".into(), |i| format!("{} × {} px", i.width, i.height)),
            ),
            (
                "picture.bit_depth",
                self.left.as_ref().map_or("-".into(), |i| {
                    format!(
                        "{} bpp{}",
                        i.bits_per_pixel,
                        if i.has_alpha { " (Alpha)" } else { "" }
                    )
                }),
                self.right.as_ref().map_or("-".into(), |i| {
                    format!(
                        "{} bpp{}",
                        i.bits_per_pixel,
                        if i.has_alpha { " (Alpha)" } else { "" }
                    )
                }),
            ),
            (
                "picture.resolution_dpi",
                self.left
                    .as_ref()
                    .and_then(|i| i.dpi)
                    .map_or("-".into(), |(x, y)| format!("{x:.0} × {y:.0}")),
                self.right
                    .as_ref()
                    .and_then(|i| i.dpi)
                    .map_or("-".into(), |(x, y)| format!("{x:.0} × {y:.0}")),
            ),
            (
                "common.size",
                self.left
                    .as_ref()
                    .map_or("-".into(), |i| format!("{} bytes", grouped(i.file_size))),
                self.right
                    .as_ref()
                    .map_or("-".into(), |i| format!("{} bytes", grouped(i.file_size))),
            ),
            (
                "status.modified",
                format_modified(self.left.as_ref().and_then(|i| i.modified)),
                format_modified(self.right.as_ref().and_then(|i| i.modified)),
            ),
            (
                "SHA-256",
                self.left
                    .as_ref()
                    .map_or("-".into(), |i| hex_prefix(&i.sha256)),
                self.right
                    .as_ref()
                    .map_or("-".into(), |i| hex_prefix(&i.sha256)),
            ),
        ];

        ScrollArea::vertical().show(ui, |ui| {
            Grid::new("pix_tags_grid")
                .num_columns(3)
                .striped(true)
                .spacing(vec2(16.0, 2.0))
                .show(ui, |ui| {
                    ui.strong(tr("picture.property"));
                    ui.strong(tr("common.left"));
                    ui.strong(tr("common.right"));
                    ui.end_row();
                    for (label, lv, rv) in rows {
                        let diff = lv != rv;
                        let color = if diff { Color32::RED } else { Color32::BLACK };
                        ui.label(tr(label));
                        ui.colored_label(color, lv);
                        ui.colored_label(color, rv);
                        ui.end_row();
                    }
                });
        });
    }
}

fn surface_to_color_image(s: &Surface) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([s.width as usize, s.height as usize], &s.rgba)
}

fn tool_btn(
    ui: &mut Ui,
    icons: &mut IconManager,
    icon_name: &str,
    label: &str,
    active: bool,
    enabled: bool,
) -> bool {
    let icon = icons.get_icon_with_state(ui.ctx(), icon_name, enabled);
    let text = tr(label);
    let btn = match icon {
        Some(tex) => Button::image_and_text((tex.id(), vec2(16.0, 16.0)), text),
        None => Button::new(text),
    }
    .selected(active)
    .frame(active);
    ui.add_enabled(enabled, btn).clicked()
}

fn icon_small_btn(
    ui: &mut Ui,
    icons: &mut IconManager,
    icon_name: &str,
    hint: &str,
    enabled: bool,
) -> bool {
    match icons.get_icon_with_state(ui.ctx(), icon_name, enabled) {
        Some(tex) => ui
            .add_enabled(
                enabled,
                ImageButton::new((tex.id(), vec2(16.0, 16.0))).frame(false),
            )
            .on_hover_text(tr(hint))
            .clicked(),
        None => ui
            .add_enabled(enabled, Button::new("..."))
            .on_hover_text(tr(hint))
            .clicked(),
    }
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let sep = if crate::i18n::language().starts_with("pt") {
        '.'
    } else {
        ','
    };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(sep);
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn format_modified(time: Option<std::time::SystemTime>) -> String {
    time.map_or_else(
        || "-".into(),
        |t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%d/%m/%Y %H:%M:%S").to_string()
        },
    )
}

fn hex_prefix(hash: &[u8; 32]) -> String {
    let mut s = String::with_capacity(16);
    for b in &hash[..8] {
        use std::fmt::Write;
        let _ = write!(&mut s, "{b:02x}");
    }
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::TestDialogs;

    #[test]
    fn picture_compare_screen_loads_compares_and_renders_all_modes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/home_icons/100/HomeImages");
        let a = root.join("imgHomePixCompareSession.png");
        let b = root.join("imgHomeTextCompareSession.png");
        let mut screen = PictureCompareScreen::new();
        screen.load_files(&a, &b).unwrap();
        assert!(screen.comparison.as_ref().unwrap().stats.different_pixels > 0);

        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut dialogs = TestDialogs::default();
        for mode in [
            CompareMode::Tolerance,
            CompareMode::MismatchRange,
            CompareMode::Blend,
        ] {
            screen.options.compare.mode = mode;
            screen.options.show_tags = mode == CompareMode::Tolerance;
            screen.recompare();
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1024.0, 700.0))),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    screen.ui(ctx, ui, &mut icons, &mut dialogs);
                });
            });
        }
        assert!(dialogs.errors.is_empty());
    }
}
