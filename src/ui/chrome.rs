use super::icons::IconManager;
use egui::*;

pub const MENU_HEIGHT: f32 = 25.0;

pub fn menu_button<R>(
    ui: &mut Ui,
    label: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let caption = crate::i18n::tr(label);
    let response = ui.menu_button(caption.clone(), content);
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            Id::new("capture_menu_rect").with(label),
            response.response.rect,
        );
        data.insert_temp(
            Id::new("capture_menu_rect").with(caption.as_str()),
            response.response.rect,
        );
    });
    super::audit::record_menu_button(ui.ctx(), label, &caption, response.response.rect);
    response
}

/// Menu bar laid out like the original at 100% (text-loaded.png): first item 2 px from the
/// left, item width = caption advance + 12 with the caption 6 px in, baseline 18 px below
/// the top of the 27 px band.
pub fn menu_bar<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    menu::bar(ui, |ui| {
        ui.spacing_mut().button_padding = vec2(6.0, MENU_TEXT_TOP);
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(2.0);
        add_contents(ui)
    })
}

/// Space above the 16 px text box of a menu bar caption in BC5's 25 px menu band.
const MENU_TEXT_TOP: f32 = 4.0;

/// Shared menu gutter, original action icons and right-aligned shortcuts.
pub fn menu_item(ui: &mut Ui, icons: &mut IconManager, label: &str, enabled: bool) -> Response {
    menu_item_styled(ui, icons, label, enabled, MenuToggle::None, false)
}

/// Bold default action menu item (matching BC5's bold top action like "Abrir Pasta" / "Abrir").
pub fn menu_item_default(
    ui: &mut Ui,
    icons: &mut IconManager,
    label: &str,
    enabled: bool,
) -> Response {
    menu_item_styled(ui, icons, label, enabled, MenuToggle::None, true)
}

/// Checkbox-style BC5 menu item with aligned 16 px gutter, depressed blue check frame
/// (matching native BC5), and audit instrumentation.
pub fn menu_check_item(
    ui: &mut Ui,
    icons: &mut IconManager,
    label: &str,
    checked: bool,
    enabled: bool,
) -> Response {
    menu_item_styled(ui, icons, label, enabled, MenuToggle::Check(checked), false)
}

/// Radio-style BC5 menu item with aligned 16 px gutter, depressed blue indicator
/// (or radio dot when no icon is defined), and audit instrumentation.
pub fn menu_radio_item(
    ui: &mut Ui,
    icons: &mut IconManager,
    label: &str,
    selected: bool,
    enabled: bool,
) -> Response {
    menu_item_styled(
        ui,
        icons,
        label,
        enabled,
        MenuToggle::Radio(selected),
        false,
    )
}

/// Nested submenu entry inside a dropdown menu, indented by the exact same 16 px
/// icon gutter as [`menu_item`] so captions stay vertically aligned.
pub fn submenu_item<R>(
    ui: &mut Ui,
    icons: &mut IconManager,
    label: &str,
    content: impl FnOnce(&mut Ui, &mut IconManager) -> R,
) -> InnerResponse<Option<R>> {
    let caption = crate::i18n::tr(label);
    let icon = menu_icon(label);
    let texture = icon.and_then(|name| icons.get_icon_with_state(ui.ctx(), name, true));
    let indent = 16.0 + ui.spacing().icon_spacing;
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &caption,
        indent,
        TextFormat::simple(FontId::proportional(12.0), Color32::BLACK),
    );
    let response = ui.menu_button(job, |ui| content(ui, icons));
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            Id::new("capture_menu_rect").with(label),
            response.response.rect,
        );
        data.insert_temp(
            Id::new("capture_menu_rect").with(caption.as_str()),
            response.response.rect,
        );
    });
    if let Some(texture) = texture {
        let pad_x = ui.spacing().button_padding.x;
        let icon_rect = Rect::from_min_size(
            pos2(
                response.response.rect.left() + pad_x,
                response.response.rect.center().y - 8.0,
            ),
            vec2(16.0, 16.0),
        );
        ui.painter().image(
            texture.id(),
            icon_rect,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    super::audit::record_menu_item(
        ui.ctx(),
        label,
        &caption,
        "",
        icon,
        true,
        response.response.rect,
    );
    response
}

#[derive(Clone, Copy)]
enum MenuToggle {
    None,
    Check(bool),
    Radio(bool),
}

fn menu_item_styled(
    ui: &mut Ui,
    icons: &mut IconManager,
    label: &str,
    enabled: bool,
    toggle: MenuToggle,
    bold: bool,
) -> Response {
    let (key, shortcut) = label.rsplit_once("  ").unwrap_or((label, ""));
    let caption_owned = crate::i18n::tr(key);
    let caption = caption_owned.as_str();
    let icon = menu_icon(key);
    let icon_texture = icon.and_then(|name| icons.get_icon_with_state(ui.ctx(), name, enabled));
    let texture = icon_texture
        .clone()
        .unwrap_or_else(|| icons.menu_gutter(ui.ctx()));
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            let mut caption_rt = RichText::new(caption).color(if enabled {
                Color32::BLACK
            } else {
                Color32::GRAY
            });
            if bold {
                caption_rt = caption_rt.strong();
            }
            let mut button = Button::image_and_text((texture.id(), vec2(16.0, 16.0)), caption_rt);
            if !shortcut.is_empty() {
                button = button.shortcut_text(shortcut);
            }
            ui.add(button)
        })
        .inner;
    let filter_icon_name = match key {
        "toolbar.show_no_orphans" => Some("FavNoOrphan"),
        "toolbar.show_differences_but_no_orphans" => Some("FavDiffNoOrphan"),
        "toolbar.show_orphans" => Some("FavOrphan"),
        "toolbar.show_left_newer" => Some("FavLeftNewer"),
        "toolbar.show_right_newer" => Some("FavRightNewer"),
        "toolbar.show_left_newer_and_left_orphans" => Some("FavLeftNewerOrphan"),
        "toolbar.show_right_newer_and_right_orphans" => Some("FavRightNewerOrphan"),
        "toolbar.show_left_orphans" => Some("FavLeftOrphan"),
        "toolbar.show_right_orphans" => Some("FavRightOrphan"),
        _ => None,
    };
    let active = matches!(toggle, MenuToggle::Check(true) | MenuToggle::Radio(true));
    let pad_x = ui.spacing().button_padding.x;
    let icon_rect = Rect::from_min_size(
        pos2(response.rect.left() + pad_x, response.rect.center().y - 8.0),
        vec2(16.0, 16.0),
    );
    if active {
        let frame_rect = icon_rect.expand(1.5);
        let painter = ui.painter();
        painter.rect_filled(frame_rect, 1.0, Color32::from_rgb(204, 232, 255));
        painter.rect_stroke(
            frame_rect,
            1.0,
            Stroke::new(1.0_f32, Color32::from_rgb(153, 209, 255)),
        );
        if let Some(tex) = icon_texture {
            painter.image(
                tex.id(),
                icon_rect,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else if let Some(filter_name) = filter_icon_name {
            super::toolbar::paint_filter_icon(painter, icon_rect, filter_name, enabled);
        } else {
            let c = icon_rect.center();
            let fg = if enabled {
                Color32::from_gray(25)
            } else {
                Color32::GRAY
            };
            match toggle {
                MenuToggle::Check(true) => {
                    let stroke = Stroke::new(1.5_f32, fg);
                    painter.line_segment([c + vec2(-3.5, 0.2), c + vec2(-1.0, 2.8)], stroke);
                    painter.line_segment([c + vec2(-1.0, 2.8), c + vec2(3.8, -3.0)], stroke);
                }
                MenuToggle::Radio(true) => {
                    painter.circle_filled(c, 2.8, fg);
                }
                _ => {}
            }
        }
    } else if let Some(filter_name) = filter_icon_name {
        super::toolbar::paint_filter_icon(ui.painter(), icon_rect, filter_name, enabled);
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(Id::new("capture_menu_rect").with(key), response.rect);
        data.insert_temp(Id::new("capture_menu_rect").with(caption), response.rect);
    });
    super::audit::record_menu_item(
        ui.ctx(),
        label,
        caption,
        shortcut,
        icon,
        enabled,
        response.rect,
    );
    response
}

fn menu_icon(key: &str) -> Option<&'static str> {
    let key = key.split("  ").next().unwrap_or(key);
    Some(match key {
        "session.home" => "imgSessionHome",
        "menu.session.new" => "imgNewSessionFolder",
        "menu.session.open" => "imgSessionsCombo",
        "session.text_compare" => "imgTextCompareSession",
        "session.folder_compare" => "imgDirCompareSession",
        "session.hex_compare" | "session.hex_compare_short" => "imgHexCompareSession",
        "session.table_compare" => "imgTableCompareSession",
        "session.version_compare" => "imgVerCompareSession",
        "session.registry_compare" => "imgRegCompareSession",
        "session.media_compare" => "imgMP3CompareSession",
        "session.picture_compare" => "imgPixCompareSession",
        "session.folder_merge" => "imgDirMergeSession",
        "session.folder_sync" => "imgDirSyncSession",
        "session.text_merge" => "imgTextMergeSession",
        "menu.tools.options" | "common.options" => "imgOptions",
        "text.file_format_ellipsis" | "text.text_format_ellipsis" => "imgTextFormatEdit",
        "menu.tools.file_formats" => "imgFileFormatManager",
        "menu.session.rules_ellipsis" | "common.rules" => "imgRules",
        "menu.session.settings" => "imgSessionEdit",
        "text.report"
        | "folder.report"
        | "hex.report"
        | "table.report"
        | "picture.report"
        | "version.report"
        | "registry.report"
        | "media.report" => "imgReport",
        "menu.session.compare_info"
        | "folder.compare_info"
        | "folder.sync_info"
        | "folder.merge_info"
        | "text.compare_info"
        | "text.merge_info"
        | "table.compare_info"
        | "hex.compare_info"
        | "picture.compare_info"
        | "version.compare_info"
        | "registry.compare_info"
        | "media.compare_info" => "imgInfo",
        "menu.session.manage_workspaces" => "imgSessionsCombo",
        "folder.compare_files_only" => "imgDirCompareFilesOnly",
        "folder.full_refresh" => "imgFullRefresh",
        "folder.sync_base_folders" => "imgDirSyncSession",
        "folder.merge_base_folders" => "imgDirMergeSession",
        "folder.compare_base_folders" => "imgDirCompareSession",
        "folder.legend" => "imgInfo",
        "sync.copy_left_to_right" => "imgSyncCopyRight",
        "sync.copy_right_to_left" => "imgSyncCopyLeft",
        "sync.leave_alone" => "imgSyncCancel",
        "sync.run_sync" => "imgDirSyncSession",
        "view.side_by_side_layout" => "imgSideBySide",
        "view.over_under_layout" => "imgOverUnder",
        "view.increase_font_size" => "imgDisplayFontIncrease",
        "view.decrease_font_size" => "imgDisplayFontDecrease",
        "view.reset_font_size" => "imgDisplayFontReset",
        "view.center_pane" => "imgShowCenter",
        "text.ruler" => "imgShowRuler",
        "text.hex_details" => "imgShowHexDetails",
        "text.alignment_details" => "imgShowAlignDetails",
        "text.syntax_highlighting" => "imgShowSyntaxHighlighting",
        "text.delete_line" => "imgDelete",
        "picture.blend_toggle" => "imgPixBlendToggle",
        "picture.full_screen" => "imgFullScreen",
        "picture.compare_metadata" => "imgPixCompareTags",
        "picture.zoom_actual_label" => "imgPixZoomActual",
        "picture.zoom_fit_label" => "imgPixZoomFit",
        "folder.up_one_level" | "toolbar.up" => "imgUpOneLevel",
        "folder.up_one_level_on_both_sides" => "imgUpOneLevelAll",
        "toolbar.play" => "imgMP3Play",
        "text.visible_whitespace" => "imgShowWS",
        "version.new_key" => "imgNewFolder",
        "version.new_value" => "imgRegStr",
        "common.modify_ellipsis" => "imgFullEdit",
        "table.explorer" => "imgBrowse",
        "registry.recompare" => "imgRecompare",
        "registry.copy_key_name" => "imgCopy",
        "table.next_diff_cell" => "imgDown",
        "table.prev_diff_cell" => "imgUp",
        "table.copy_cells_right" => "imgCopyRight",
        "table.copy_cells_left" => "imgCopyLeft",
        "folder.find_next_filename" => "imgFindNext",
        "folder.find_prev_filename" => "imgFindPrev",
        "menu.help.about" => "imgHelpContents",
        "menu.session.save" => "imgSessionSave",
        "menu.session.save_as" => "imgSessionSaveAs",
        "menu.file.save" | "hex.save_bytes" | "common.save" => "imgSave",
        "menu.file.save_as"
        | "menu.file.save_title_as"
        | "menu.file.save_as_short"
        | "file.save_on_file_system_as"
        | "file.save_using_profile_as"
        | "registry.export"
        | "registry.export_all"
        | "hex.save_bytes_as" => "imgSaveAs",
        "menu.actions.copy_right_short" => "imgCopyRight",
        "menu.actions.copy_left_short" => "imgCopyLeft",
        "menu.file.open"
        | "menu.file.open_ellipsis"
        | "menu.file.open_title_ellipsis"
        | "menu.file.open_left"
        | "menu.file.open_right"
        | "file.open_on_file_system"
        | "file.open_using_profile"
        | "registry.open_on_file_system"
        | "registry.open_using_profile"
        | "menu.session.open_workspace" => "imgBrowse",
        "file.open_clipboard" => "imgPaste",
        "menu.actions.open_with" => "imgPrefsOpenWith",
        "menu.tools.save_snapshot" => "imgSaveSnapshot",
        "menu.tools.view_patch" => "imgTextPatchSession",
        "menu.session.locked" => "imgSessionLocked",
        "menu.edit.swap_sides" | "toolbar.swap_sides" => "imgSwap",
        "common.reload" | "menu.file.reload_files" | "menu.actions.compare_refresh" => "imgRefresh",
        "menu.actions.recompare" | "toolbar.recompare_files" => "imgRecompare",
        "common.undo" => "imgUndo",
        "common.redo" => "imgRedo",
        "common.cut" | "hex.cut_bytes" => "imgCut",
        "common.copy"
        | "version.copy_value"
        | "hex.copy_bytes"
        | "folder.copy_filenames"
        | "folder.ctx.copy_filename"
        | "toolbar.copy_filenames" => "imgCopy",
        "common.paste" | "hex.paste_bytes" => "imgPaste",
        "hex.delete_bytes"
        | "text.delete_lines"
        | "common.delete"
        | "folder.delete_recycle"
        | "folder.ctx.delete_ellipsis" => "imgDelete",
        "folder.ctx.compare_contents_ellipsis" => "imgCompare",
        "folder.ctx.copy_to_side_ellipsis" => "imgCopyLeftOrRight",
        "folder.ctx.move_to_side_ellipsis" => "imgMoveLeftOrRight",
        "folder.ctx.copy_to_folder_ellipsis" => "imgCopyTo",
        "folder.ctx.move_to_folder_ellipsis" => "imgMoveTo",
        "folder.ctx.touch_ellipsis" => "imgTouch",
        "folder.ctx.ignored" => "imgIgnored",
        "folder.ctx.refresh_selection" => "imgRefresh",
        "folder.ctx.synchronize" => "imgDirSyncSession",
        "menu.search.find" | "folder.find_name" | "hex.find_bytes" | "common.find" => "imgFind",
        "menu.search.find_next" | "toolbar.find_next" => "imgFindNext",
        "menu.search.find_prev" | "toolbar.find_prev" => "imgFindPrev",
        "menu.search.replace" | "hex.replace_bytes" | "common.replace" => "imgReplace",
        "menu.search.goto" => "imgGoTo",
        "menu.search.next_diff" | "toolbar.next_diff" => "imgDown",
        "menu.search.prev_diff" | "toolbar.prev_diff" => "imgUp",
        "menu.actions.copy_to_right"
        | "text.copy_diff_right"
        | "text.copy_line_right" => "imgCopyRight",
        "menu.actions.copy_to_left"
        | "text.copy_diff_left"
        | "text.copy_line_left" => "imgCopyLeft",
        "folder.expand_all" | "common.expand_all_title" | "toolbar.expand_all" => "imgExpandAll",
        "folder.collapse_all" | "common.collapse_all_title" | "toolbar.collapse_all" => {
            "imgCollapseAll"
        }
        "menu.edit.select_all" | "toolbar.select_all" => "imgSelectAll",
        "common.rename" | "folder.rename_ellipsis" => "imgRename",
        "folder.new_folder" | "folder.new_folder_ellipsis" => "imgNewFolder",
        "common.stop" => "imgStopRed",
        "session.text_edit" => "imgTextEditSession",
        "hex.edit_bytes"
        | "menu.tools.edit_text_file"
        | "session.text_edit_short"
        | "text.edit_texts" => "imgFullEdit",
        "menu.session.merge_files" => "imgTextMergeSession",
        "sync.now" => "imgDirSyncSession",
        "sync.preset.update_right" => "imgUpdateRight",
        "sync.preset.update_left" => "imgUpdateLeft",
        "sync.preset.update_both" => "imgDirSyncSession",
        "sync.preset.mirror_right" => "imgMirrorRight",
        "sync.preset.mirror_left" => "imgMirrorLeft",
        "merge.take_left" | "toolbar.take_left" => "imgFavorLeft",
        "merge.take_center" => "imgShowCenter",
        "merge.take_right" | "toolbar.take_right" => "imgFavorRight",
        "merge.next_conflict" | "toolbar.next_conflict_section" => "imgConflictSectionNext",
        "merge.prev_conflict" | "toolbar.previous_conflict_section" => "imgConflictSectionPrev",
        "merge.to_output" | "merge.copy_to_output" => "imgCopyToOutput",
        "merge.save_output" => "imgSave",
        "merge.save_output_as" => "imgSaveAs",
        "merge.show_center" => "imgShowCenter",
        "common.all" => "imgFavAll",
        "common.differences" | "report.only_differences" => "imgFavDiff",
        "common.same" => "imgFavSame",
        "view.ignore_unimportant" | "table.ignore_unimportant" => "imgInsigIgnored",
        "folder.show_tree" => "imgDirCompareStructure",
        "text.show_context" => "imgShowContext",
        "text.show_details" => "imgShowTextDetails",
        "text.thumbnail" => "imgShowThumbnail",
        "picture.mode.tolerance" => "imgPixDiffTolerance",
        "picture.mode.range" => "imgPixDiffRange",
        "picture.mode.blend" => "imgPixDiffBlend",
        "picture.ghost" => "imgPixToleranceGhost",
        "picture.auto_scale" => "imgPixAutoScale",
        "picture.show_properties" => "imgPixCompareTags",
        "picture.zoom_actual" => "imgPixZoomActual",
        "picture.zoom_fit" => "imgPixZoomFit",
        "picture.rotate_left" => "imgPixRotateLeft",
        "picture.rotate_right" => "imgPixRotateRight",
        "picture.flip_h" => "imgPixFlipHorz",
        "picture.flip_v" => "imgPixFlipVert",
        "picture.zoom_in" => "imgZoomIn",
        "picture.zoom_out" => "imgZoomOut",
        _ => return None,
    })
}

/// Vertical gradient interpolated per physical pixel row in sRGB, as GDI paints it (an
/// egui mesh would interpolate in linear space and darken the middle rows).
pub fn gradient(ui: &Ui, rect: Rect, top: Color32, bottom: Color32) {
    let ppp = ui.ctx().pixels_per_point();
    let first = (rect.top() * ppp).round() as i32;
    let last = (rect.bottom() * ppp).round() as i32;
    let rows = (last - first).max(1);
    let lerp = |a: u8, b: u8, t: f32| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    for row in first..last {
        let t = if rows > 1 {
            (row - first) as f32 / (rows - 1) as f32
        } else {
            0.0
        };
        let color = Color32::from_rgb(
            lerp(top.r(), bottom.r(), t),
            lerp(top.g(), bottom.g(), t),
            lerp(top.b(), bottom.b(), t),
        );
        ui.painter().rect_filled(
            Rect::from_min_max(
                pos2(rect.left(), row as f32 / ppp),
                pos2(rect.right(), (row + 1) as f32 / ppp),
            ),
            0.0,
            color,
        );
    }
}

pub fn menu_background(ui: &Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0_f32, Color32::from_gray(224)),
    );
}

/// Height of the BC5 menu band (measured 25 px on BC5.2.7); the toolbar band starts right below it.
pub const MENU_BAND_HEIGHT: f32 = 25.0;

/// AnyCompare light Windows 11 toolbar surface (#F0F0F0 with a crisp
/// bottom hairline separating it from the path/spec bar below).
pub fn toolbar_background(ui: &Ui) {
    let rect = ui.max_rect();
    ui.painter()
        .rect_filled(rect, 0.0, super::theme::BcTheme::WINDOW_BG);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0_f32, Color32::from_gray(216)),
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowseMenuAction {
    LoadTyped,
    OpenFileSystem,
    OpenProfile,
    OpenClipboard,
    OpenWithTextEdit,
    OpenWithAssociated,
}

pub fn read_system_clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok())
}

/// Paints the classic BC5 6-dot triangular resize grip at the bottom-right
/// corner of a status bar (`1+2+3` dots with 1px light shadow).
pub fn status_bar_grip(painter: &Painter, status_rect: Rect) {
    let gx = status_rect.right() - 4.0;
    let gy = status_rect.bottom() - 4.0;
    for (col, row) in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (0, 2)] {
        let px = gx - (col as f32) * 3.0;
        let py = gy - (row as f32) * 3.0;
        painter.rect_filled(
            Rect::from_min_size(pos2(px + 1.0, py + 1.0), vec2(1.5, 1.5)),
            0.0,
            Color32::WHITE,
        );
        painter.rect_filled(
            Rect::from_min_size(pos2(px, py), vec2(1.5, 1.5)),
            0.0,
            Color32::from_gray(165),
        );
    }
}

/// Renders the BC5 path bar `[load_spec] [imgBrowse | ▾]` (plus `[imgUpOneLevel]`
/// in Registry Compare) and its 4-item dropdown menu (`Open File on File System...`,
/// `Open File Using Profile...`, `Open Clipboard`, `Open With >`).
#[allow(clippy::too_many_arguments)]
pub fn file_browse_buttons(
    ui: &mut Ui,
    icons: &mut IconManager,
    id_salt: impl std::hash::Hash,
    pending_load: bool,
    can_up: bool,
    is_registry: bool,
    can_clipboard: bool,
    has_loaded_file: bool,
) -> (bool, Option<BrowseMenuAction>) {
    let mut up_clicked = false;
    let mut browse_action = None;
    let base_id = ui.make_persistent_id(id_salt);

    // 1. Small bent load_spec arrow (18x22) right after the path combo box
    let (load_rect, _) = ui.allocate_exact_size(vec2(18.0, 22.0), Sense::hover());
    let load_resp = ui.interact(
        load_rect,
        base_id.with("load_spec_btn"),
        if pending_load {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if pending_load && load_resp.hovered() {
        ui.painter()
            .rect_filled(load_rect, 1.0, Color32::from_rgb(229, 243, 255));
        ui.painter().rect_stroke(
            load_rect,
            1.0,
            Stroke::new(1.0_f32, Color32::from_rgb(204, 232, 255)),
        );
    }
    let points = [
        (-6.0, -5.0),
        (0.0, -5.0),
        (3.0, -2.0),
        (3.0, 1.0),
        (6.0, 1.0),
        (1.0, 6.0),
        (-4.0, 1.0),
        (-1.0, 1.0),
        (-1.0, -1.0),
        (-3.0, -3.0),
        (-6.0, -3.0),
    ];
    let lc = ui
        .painter()
        .round_pos_to_pixels(pos2(load_rect.center().x, load_rect.top() + 11.0));
    ui.painter().add(Shape::Path(egui::epaint::PathShape {
        points: points.into_iter().map(|(x, y)| lc + vec2(x, y)).collect(),
        closed: true,
        fill: if pending_load {
            Color32::from_rgb(247, 218, 126)
        } else {
            Color32::from_gray(232)
        },
        stroke: Stroke::new(
            1.0_f32,
            if pending_load {
                Color32::from_gray(128)
            } else {
                Color32::from_gray(190)
            },
        )
        .into(),
    }));
    if pending_load
        && load_resp
            .on_hover_text(crate::i18n::tr("picture.load_file"))
            .clicked()
    {
        browse_action = Some(BrowseMenuAction::LoadTyped);
    }

    // 2. [imgBrowse | ▾] split dropdown button (28x22: 18px icon face + 10px arrow)
    let (split_rect, _) = ui.allocate_exact_size(vec2(28.0, 22.0), Sense::hover());
    let face_rect = Rect::from_min_size(split_rect.min, vec2(18.0, 22.0));
    let arrow_rect = Rect::from_min_size(split_rect.min + vec2(18.0, 0.0), vec2(10.0, 22.0));
    let face_resp = ui.interact(face_rect, base_id.with("browse_face"), Sense::click());
    let arrow_resp = ui.interact(arrow_rect, base_id.with("browse_arrow"), Sense::click());
    let popup_id = base_id.with("browse_popup");
    let popup_open = ui.memory(|m| m.is_popup_open(popup_id));
    let hot = face_resp.hovered() || arrow_resp.hovered();

    if popup_open || hot {
        let fill = if popup_open {
            Color32::from_rgb(204, 232, 255)
        } else {
            Color32::from_rgb(229, 243, 255)
        };
        let border = if popup_open {
            Color32::from_rgb(153, 209, 255)
        } else {
            Color32::from_rgb(204, 232, 255)
        };
        ui.painter().rect_filled(split_rect, 1.0, fill);
        ui.painter()
            .rect_stroke(split_rect, 1.0, Stroke::new(1.0_f32, border));
        ui.painter().vline(
            face_rect.right(),
            (split_rect.top() + 1.0)..=(split_rect.bottom() - 1.0),
            Stroke::new(1.0_f32, border),
        );
    }
    if let Some(tex) = icons.get_icon(ui.ctx(), "imgBrowse") {
        ui.painter().image(
            tex.id(),
            Rect::from_center_size(face_rect.center(), vec2(16.0, 16.0)),
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    // 5x3 down arrow in arrow_rect
    let ac = arrow_rect.center();
    for row in 0..3 {
        let w = 5 - row * 2;
        let x0 = (ac.x - (w as f32) * 0.5).floor();
        let y = (ac.y - 1.0 + row as f32).floor();
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(x0, y), vec2(w as f32, 1.0)),
            0.0,
            Color32::from_gray(35),
        );
    }

    if face_resp
        .clone()
        .on_hover_text(crate::i18n::tr("menu.file.open_ellipsis"))
        .clicked()
    {
        browse_action = Some(BrowseMenuAction::OpenFileSystem);
    }
    if arrow_resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let anchor = face_resp.union(arrow_resp);
    popup::popup_below_widget(
        ui,
        popup_id,
        &anchor,
        PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(250.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let open_fs_key = if is_registry {
                "registry.open_on_file_system"
            } else {
                "file.open_on_file_system"
            };
            let open_prof_key = if is_registry {
                "registry.open_using_profile"
            } else {
                "file.open_using_profile"
            };
            if menu_item(ui, icons, open_fs_key, true).clicked() {
                browse_action = Some(BrowseMenuAction::OpenFileSystem);
                ui.memory_mut(|m| m.close_popup());
            }
            if menu_item(ui, icons, open_prof_key, true).clicked() {
                browse_action = Some(BrowseMenuAction::OpenProfile);
                ui.memory_mut(|m| m.close_popup());
            }
            if can_clipboard && menu_item(ui, icons, "file.open_clipboard", true).clicked() {
                browse_action = Some(BrowseMenuAction::OpenClipboard);
                ui.memory_mut(|m| m.close_popup());
            }
            ui.add_enabled_ui(has_loaded_file, |ui| {
                submenu_item(ui, icons, "menu.actions.open_with", |ui, icons| {
                    if menu_item(ui, icons, "session.text_edit", true).clicked() {
                        browse_action = Some(BrowseMenuAction::OpenWithTextEdit);
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if menu_item(ui, icons, "file.associated_application", true).clicked() {
                        browse_action = Some(BrowseMenuAction::OpenWithAssociated);
                        ui.memory_mut(|m| m.close_popup());
                    }
                });
            });
        },
    );

    // 3. In Registry Compare, also draw the Up One Level button right after [imgBrowse | ▾]
    if is_registry {
        let (up_rect, _) = ui.allocate_exact_size(vec2(18.0, 22.0), Sense::hover());
        let up_resp = ui.interact(
            up_rect,
            base_id.with("up_btn"),
            if can_up { Sense::click() } else { Sense::hover() },
        );
        if can_up && up_resp.hovered() {
            ui.painter()
                .rect_filled(up_rect, 1.0, Color32::from_rgb(229, 243, 255));
            ui.painter().rect_stroke(
                up_rect,
                1.0,
                Stroke::new(1.0_f32, Color32::from_rgb(204, 232, 255)),
            );
        }
        if let Some(tex) = icons.get_icon_with_state(ui.ctx(), "imgUpOneLevel", can_up) {
            ui.painter().image(
                tex.id(),
                Rect::from_center_size(up_rect.center(), vec2(16.0, 16.0)),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if can_up
            && up_resp
                .on_hover_text(crate::i18n::tr("folder.up_one_level"))
                .clicked()
        {
            up_clicked = true;
        }
    }

    (up_clicked, browse_action)
}
