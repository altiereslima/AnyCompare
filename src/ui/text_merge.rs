//! AnyCompare 3-Way Text Merge session UI (`Mesclar Texto`).

use super::dialogs::{DialogField, DialogHost, SaveChoice};
use super::toolbar::{ToolEvent, ToolState};
use crate::diff::DiffRules;
use crate::document::Document;
use crate::merge3::{
    compute_three_way, visible_row_indices, MergeChoice, MergeFilter, MergeResult,
    MergeSectionKind, OutputSourceTag,
};
use crate::ui::{icons::IconManager, theme::BcTheme};
use egui::*;
use std::path::{Path, PathBuf};

const ROW_HEIGHT: f32 = 17.0;
const GUTTER_W: f32 = 54.0;
const THUMB_W: f32 = 16.0;

// BC5 3-way merge palette (light Windows 11 theme)
const LEFT_CHANGE_BG: Color32 = Color32::from_rgb(241, 232, 255);
const LEFT_CHANGE_TEXT: Color32 = Color32::from_rgb(106, 27, 154);
const RIGHT_CHANGE_BG: Color32 = Color32::from_rgb(227, 245, 252);
const RIGHT_CHANGE_TEXT: Color32 = Color32::from_rgb(0, 105, 148);
const BOTH_SAME_BG: Color32 = Color32::from_rgb(232, 248, 236);
const BOTH_SAME_TEXT: Color32 = Color32::from_rgb(24, 118, 48);
const CONFLICT_BG: Color32 = Color32::from_rgb(255, 227, 227);
const CONFLICT_TEXT: Color32 = Color32::from_rgb(205, 0, 0);

pub enum TextMergeAction {
    None,
    Home,
}

#[derive(Clone)]
struct MergeUndoEntry {
    section_idx: usize,
    previous_choice: MergeChoice,
}

pub struct TextMergeScreen {
    pub left_path: String,
    pub center_path: String,
    pub right_path: String,
    pub output_path: String,
    pub left_doc: Document,
    pub center_doc: Option<Document>,
    pub right_doc: Document,
    pub output_doc: Document,
    pub rules: DiffRules,
    pub ignore_unimportant: bool,
    pub result: MergeResult,
    pub filter: MergeFilter,
    pub show_context: bool,
    pub show_center: bool,
    pub full_edit_output: bool,
    pub output_free_text: String,
    pub active_section: usize,
    pub dirty: bool,
    undo_stack: Vec<MergeUndoEntry>,
    redo_stack: Vec<MergeUndoEntry>,
    pending_scroll_row: Option<usize>,
    editor_font_size: f32,
    show_toolbar: bool,
    show_file_info: bool,
    syntax_highlighting: bool,
    ignore_same_changes: bool,
}

impl Default for TextMergeScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl TextMergeScreen {
    pub fn new() -> Self {
        Self {
            left_path: String::new(),
            center_path: String::new(),
            right_path: String::new(),
            output_path: String::new(),
            left_doc: Document::default(),
            center_doc: None,
            right_doc: Document::default(),
            output_doc: Document::default(),
            rules: DiffRules::default(),
            ignore_unimportant: false,
            result: MergeResult::default(),
            filter: MergeFilter::All,
            show_context: true,
            show_center: true,
            full_edit_output: false,
            output_free_text: String::new(),
            active_section: 0,
            dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending_scroll_row: None,
            editor_font_size: 13.0,
            show_toolbar: true,
            show_file_info: true,
            syntax_highlighting: true,
            ignore_same_changes: false,
        }
    }

    pub fn set_editor_font_size(&mut self, px: f32) {
        self.editor_font_size = px.clamp(9.0, 24.0);
    }

    fn open_info_modal(&self, ctx: &Context) {
        use crate::i18n::tr;
        let rows = vec![
            (tr("folder.side_left"), self.left_path.clone()),
            (tr("view.center_pane"), self.center_path.clone()),
            (tr("folder.side_right"), self.right_path.clone()),
            (tr("merge.output"), self.output_path.clone()),
            (
                tr("common.differences"),
                self.result.changed_sections.to_string(),
            ),
            (
                tr("common.show_conflicts"),
                format!(
                    "{} ({})",
                    self.result.conflict_sections, self.result.unresolved_conflicts
                ),
            ),
        ];
        super::info_modals::open_info(ctx, tr("text.merge_info"), rows);
    }

    pub fn has_pair(&self) -> bool {
        !self.left_path.trim().is_empty() && !self.right_path.trim().is_empty()
    }

    pub fn load_files_silent(
        &mut self,
        left: &Path,
        center: Option<&Path>,
        right: &Path,
        output: Option<&Path>,
    ) -> Result<(), String> {
        self.left_path = left.to_string_lossy().into_owned();
        self.right_path = right.to_string_lossy().into_owned();
        self.center_path = center
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.output_path = output
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.reload_silent()
    }

    pub fn reload_silent(&mut self) -> Result<(), String> {
        if !self.left_path.trim().is_empty() {
            self.left_doc = Document::load(Path::new(self.left_path.trim()))
                .map_err(|err| format!("{}: {err}", self.left_path))?;
        }
        if !self.center_path.trim().is_empty() {
            self.center_doc = Some(
                Document::load(Path::new(self.center_path.trim()))
                    .map_err(|err| format!("{}: {err}", self.center_path))?,
            );
        } else {
            self.center_doc = None;
        }
        if !self.right_path.trim().is_empty() {
            self.right_doc = Document::load(Path::new(self.right_path.trim()))
                .map_err(|err| format!("{}: {err}", self.right_path))?;
        }
        self.output_doc = self.left_doc.clone();
        self.output_doc.path = if self.output_path.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.output_path.trim()))
        };
        self.recompute();
        self.dirty = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        Ok(())
    }

    pub fn reload(&mut self, dialogs: &mut dyn DialogHost) {
        if let Err(err) = self.reload_silent() {
            dialogs.error(&err);
        }
    }

    pub fn recompute(&mut self) {
        let center_text = self.center_doc.as_ref().map(|d| d.text.as_str());
        self.result = compute_three_way(
            &self.left_doc.text,
            center_text,
            &self.right_doc.text,
            self.rules,
            self.ignore_unimportant,
        );
        self.active_section = self
            .result
            .sections
            .iter()
            .position(|s| s.is_conflict())
            .or_else(|| self.result.sections.iter().position(|s| s.is_changed()))
            .unwrap_or(0);
        self.sync_free_text();
    }

    fn sync_free_text(&mut self) {
        self.output_free_text = self.result.assemble_output_text("\n");
    }

    fn set_section_choice(&mut self, sec_idx: usize, choice: MergeChoice) {
        if let Some(sec) = self.result.sections.get_mut(sec_idx) {
            if sec.choice == choice {
                return;
            }
            let prev = std::mem::replace(&mut sec.choice, choice);
            self.undo_stack.push(MergeUndoEntry {
                section_idx: sec_idx,
                previous_choice: prev,
            });
            self.redo_stack.clear();
            self.result.rebuild_views();
            self.sync_free_text();
            self.dirty = true;
            self.full_edit_output = false;
        }
    }

    fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop() {
            if let Some(sec) = self.result.sections.get_mut(entry.section_idx) {
                let current = std::mem::replace(&mut sec.choice, entry.previous_choice);
                self.redo_stack.push(MergeUndoEntry {
                    section_idx: entry.section_idx,
                    previous_choice: current,
                });
                self.result.rebuild_views();
                self.sync_free_text();
                self.dirty = true;
            }
        }
    }

    fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop() {
            if let Some(sec) = self.result.sections.get_mut(entry.section_idx) {
                let current = std::mem::replace(&mut sec.choice, entry.previous_choice);
                self.undo_stack.push(MergeUndoEntry {
                    section_idx: entry.section_idx,
                    previous_choice: current,
                });
                self.result.rebuild_views();
                self.sync_free_text();
                self.dirty = true;
            }
        }
    }

    fn jump_section(&mut self, forward: bool, conflicts_only: bool) {
        let n = self.result.sections.len();
        if n == 0 {
            return;
        }
        let matches = |i: usize| {
            let s = &self.result.sections[i];
            if conflicts_only {
                s.is_conflict()
            } else {
                s.is_changed()
            }
        };
        let target = if forward {
            ((self.active_section + 1)..n)
                .chain(0..=self.active_section)
                .find(|&i| matches(i))
        } else {
            (0..self.active_section)
                .rev()
                .chain((self.active_section..n).rev())
                .find(|&i| matches(i))
        };
        if let Some(sec_idx) = target {
            self.active_section = sec_idx;
            if let Some(row_pos) = self
                .result
                .rows
                .iter()
                .position(|r| r.section_idx == sec_idx)
            {
                self.pending_scroll_row = Some(row_pos);
            }
        }
    }

    pub fn save_output(&mut self, save_as: bool, dialogs: &mut dyn DialogHost) -> bool {
        if self.result.unresolved_conflicts > 0
            && !self.full_edit_output
            && !dialogs.confirm(&crate::i18n::tr(
                "merge.there_are_still_unresolved_conflicts_with",
            ))
        {
            return false;
        }
        let target = if !save_as && !self.output_path.trim().is_empty() {
            PathBuf::from(self.output_path.trim())
        } else {
            let default_name = Path::new(&self.left_path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "merged.txt".into());
            let Some(picked) = dialogs.named_file(
                &crate::i18n::tr("merge.save_output"),
                true,
                self.output_doc.path.as_deref(),
                &default_name,
            ) else {
                return false;
            };
            self.output_path = picked.to_string_lossy().into_owned();
            picked
        };

        let text = if self.full_edit_output {
            self.output_free_text.clone()
        } else {
            self.result.assemble_output_text("\n")
        };
        self.output_doc.text = text;
        match self.output_doc.save(&target) {
            Ok(()) => {
                self.dirty = false;
                true
            }
            Err(err) => {
                dialogs.error(&format!("{}: {err}", target.display()));
                false
            }
        }
    }

    pub fn confirm_saved(&mut self, dialogs: &mut dyn DialogHost) -> bool {
        if !self.dirty {
            return true;
        }
        let path = (!self.output_path.trim().is_empty()).then(|| PathBuf::from(&self.output_path));
        match dialogs.confirm_save_file(path.as_deref()) {
            SaveChoice::Save => self.save_output(false, dialogs),
            SaveChoice::Discard => true,
            SaveChoice::Cancel => false,
        }
    }

    pub fn swap_sides(&mut self) {
        std::mem::swap(&mut self.left_path, &mut self.right_path);
        std::mem::swap(&mut self.left_doc, &mut self.right_doc);
        self.recompute();
    }

    pub fn saved_options(&self) -> serde_json::Value {
        serde_json::json!({
            "show_center": self.show_center,
            "show_context": self.show_context,
            "ignore_case": self.rules.ignore_case,
            "ignore_whitespace": self.rules.ignore_whitespace,
        })
    }

    pub fn apply_saved_options(&mut self, value: &serde_json::Value) {
        if let Some(v) = value["show_center"].as_bool() {
            self.show_center = v;
        }
        if let Some(v) = value["show_context"].as_bool() {
            self.show_context = v;
        }
        if let Some(v) = value["ignore_case"].as_bool() {
            self.rules.ignore_case = v;
        }
        if let Some(v) = value["ignore_whitespace"].as_bool() {
            self.rules.ignore_whitespace = v;
        }
    }

    fn open_rules_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        let mut fields = [
            DialogField::Check(
                crate::i18n::tr("common.ignore_character_case"),
                self.rules.ignore_case,
            ),
            DialogField::Check(
                crate::i18n::tr("common.ignore_whitespace"),
                self.rules.ignore_whitespace,
            ),
            DialogField::Check(
                crate::i18n::tr("view.ignore_unimportant"),
                self.ignore_unimportant,
            ),
            DialogField::Check(crate::i18n::tr("merge.show_center"), self.show_center),
        ];
        if dialogs.fields(&crate::i18n::tr("common.session_settings"), &mut fields) {
            if let DialogField::Check(_, v) = fields[0] {
                self.rules.ignore_case = v;
            }
            if let DialogField::Check(_, v) = fields[1] {
                self.rules.ignore_whitespace = v;
            }
            if let DialogField::Check(_, v) = fields[2] {
                self.ignore_unimportant = v;
            }
            if let DialogField::Check(_, v) = fields[3] {
                self.show_center = v;
            }
            self.recompute();
        }
    }

    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> TextMergeAction {
        let mut action = TextMergeAction::None;

        // Global shortcuts
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S)) {
            self.save_output(false, dialogs);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) && !self.full_edit_output {
            self.undo();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y)) && !self.full_edit_output {
            self.redo();
        }
        if ctx.input(|i| i.key_pressed(Key::F5)) && self.has_pair() {
            self.reload(dialogs);
        }
        if ctx.input_mut(|i| {
            i.consume_key(Modifiers::CTRL, Key::Plus)
                || i.consume_key(Modifiers::CTRL, Key::Equals)
        }) {
            self.set_editor_font_size(self.editor_font_size + 1.0);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Minus)) {
            self.set_editor_font_size(self.editor_font_size - 1.0);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Num0)) {
            self.set_editor_font_size(13.0);
        }

        let active_sec_kind = self
            .result
            .sections
            .get(self.active_section)
            .map(|s| s.kind);
        let active_choice = self
            .result
            .sections
            .get(self.active_section)
            .map(|s| s.choice.clone());
        let has_section = active_sec_kind.is_some();

        // 1. Menu Bar
        TopBottomPanel::top("text_merge_menu")
            .exact_height(super::chrome::MENU_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                super::chrome::menu_bar(ui, |ui| {
                    super::chrome::menu_button(ui, "menu.session", |ui| {
                        super::session_menu::prefix(ui, icons, self.has_pair());
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "menu.session.settings", true)
                            .clicked()
                        {
                            self.open_rules_dialog(dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "toolbar.swap_sides", self.has_pair())
                            .clicked()
                        {
                            self.swap_sides();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "menu.file.reload_files  F5", self.has_pair())
                            .clicked()
                        {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "text.merge_info", true).clicked() {
                            self.open_info_modal(ctx);
                            ui.close_menu();
                        }
                        super::session_menu::suffix(ui, icons, self.has_pair());
                    });
                    super::chrome::menu_button(ui, "menu.file", |ui| {
                        if super::chrome::menu_item(ui, icons, "merge.save_output  Ctrl+S", true)
                            .clicked()
                        {
                            self.save_output(false, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "merge.save_output_as", true)
                            .clicked()
                        {
                            self.save_output(true, dialogs);
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.edit", |ui| {
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "common.undo  Ctrl+Z",
                            !self.undo_stack.is_empty(),
                        )
                        .clicked()
                        {
                            self.undo();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "common.redo  Ctrl+Y",
                            !self.redo_stack.is_empty(),
                        )
                        .clicked()
                        {
                            self.redo();
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "merge.take_left", has_section)
                            .clicked()
                        {
                            self.set_section_choice(self.active_section, MergeChoice::TakeLeft);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.take_center",
                            has_section && self.center_doc.is_some(),
                        )
                        .clicked()
                        {
                            self.set_section_choice(self.active_section, MergeChoice::TakeCenter);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "merge.take_right", has_section)
                            .clicked()
                        {
                            self.set_section_choice(self.active_section, MergeChoice::TakeRight);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.resolve_all_conflicts_using_left",
                            self.result.unresolved_conflicts > 0,
                        )
                        .clicked()
                        {
                            let indices: Vec<usize> = self
                                .result
                                .sections
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| s.is_unresolved_conflict())
                                .map(|(i, _)| i)
                                .collect();
                            for idx in indices {
                                self.set_section_choice(idx, MergeChoice::TakeLeft);
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.resolve_all_conflicts_using_right",
                            self.result.unresolved_conflicts > 0,
                        )
                        .clicked()
                        {
                            let indices: Vec<usize> = self
                                .result
                                .sections
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| s.is_unresolved_conflict())
                                .map(|(i, _)| i)
                                .collect();
                            for idx in indices {
                                self.set_section_choice(idx, MergeChoice::TakeRight);
                            }
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.search", |ui| {
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.next_conflict",
                            self.result.conflict_sections > 0,
                        )
                        .clicked()
                        {
                            self.jump_section(true, true);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.prev_conflict",
                            self.result.conflict_sections > 0,
                        )
                        .clicked()
                        {
                            self.jump_section(false, true);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "toolbar.next_diff",
                            self.result.changed_sections > 0,
                        )
                        .clicked()
                        {
                            self.jump_section(true, false);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "toolbar.prev_diff",
                            self.result.changed_sections > 0,
                        )
                        .clicked()
                        {
                            self.jump_section(false, false);
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.view", |ui| {
                        for (label, f) in [
                            ("common.all", MergeFilter::All),
                            ("common.show_changes", MergeFilter::Changed),
                            ("common.show_conflicts", MergeFilter::Conflicts),
                            ("common.show_left_changes", MergeFilter::LeftChanges),
                            ("common.show_right_changes", MergeFilter::RightChanges),
                            ("common.show_mergeable", MergeFilter::Mergeable),
                            ("common.same", MergeFilter::Unchanged),
                        ] {
                            if super::chrome::menu_radio_item(
                                ui,
                                icons,
                                label,
                                self.filter == f,
                                true,
                            )
                            .clicked()
                            {
                                self.filter = f;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "merge.show_center",
                            self.show_center,
                            true,
                        )
                        .clicked()
                        {
                            self.show_center = !self.show_center;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.show_context",
                            self.show_context,
                            true,
                        )
                        .clicked()
                        {
                            self.show_context = !self.show_context;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "view.ignore_same_changes",
                            self.ignore_same_changes,
                            true,
                        )
                        .clicked()
                        {
                            self.ignore_same_changes = !self.ignore_same_changes;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "text.syntax_highlighting",
                            self.syntax_highlighting,
                            true,
                        )
                        .clicked()
                        {
                            self.syntax_highlighting = !self.syntax_highlighting;
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "view.increase_font_size  Ctrl++",
                            self.editor_font_size < 24.0,
                        )
                        .clicked()
                        {
                            self.set_editor_font_size(self.editor_font_size + 1.0);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "view.decrease_font_size  Ctrl+-",
                            self.editor_font_size > 9.0,
                        )
                        .clicked()
                        {
                            self.set_editor_font_size(self.editor_font_size - 1.0);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "view.reset_font_size  Ctrl+0",
                            (self.editor_font_size - 13.0).abs() > 0.1,
                        )
                        .clicked()
                        {
                            self.set_editor_font_size(13.0);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_check_item(
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
                        if super::chrome::menu_check_item(
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
                    super::chrome::menu_button(ui, "menu.tools", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.tools.options", true).clicked() {
                            super::options_dialog::request(
                                ui.ctx(),
                                super::options_dialog::OptionsCategory::TextCompare,
                            );
                            ui.close_menu();
                        }
                        super::session_menu::settings_tools(ui, icons, dialogs);
                    });
                    super::chrome::menu_button(ui, "menu.help", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.help.about", true)
                            .clicked()
                        {
                            super::info_modals::open_about(ui.ctx());
                            ui.close_menu();
                        }
                    });
                });
            });

        // 2. Shared Session Tabs
        super::session_tabs::show(ui, icons);

        // 3. Exact BC5 TEXT_MERGE Toolbar
        let tool_state = |name: &str| match name {
            "SessionHome" | "SessionsCombo" | "Rules" | "Swap" => ToolState::enabled(true),
            "Reload" => ToolState::enabled(self.has_pair()),
            "FavMergeAll" => ToolState::checked(self.filter == MergeFilter::All),
            "FavChanged" => ToolState::checked(self.filter == MergeFilter::Changed),
            "FavConflict" => ToolState::checked(self.filter == MergeFilter::Conflicts),
            "FavLeft" => ToolState::checked(self.filter == MergeFilter::LeftChanges),
            "FavRight" => ToolState::checked(self.filter == MergeFilter::RightChanges),
            "FavMergeable" => ToolState::checked(self.filter == MergeFilter::Mergeable),
            "FavUnchanged" => ToolState::checked(self.filter == MergeFilter::Unchanged),
            "ShowContext" => ToolState::checked(self.show_context),
            "InsigIgnored" => ToolState::checked(self.ignore_unimportant),
            "Conflict" => ToolState {
                enabled: has_section,
                checked: active_choice == Some(MergeChoice::ConflictUnresolved),
            },
            "TakeLeft" => ToolState {
                enabled: has_section,
                checked: active_choice == Some(MergeChoice::TakeLeft),
            },
            "TakeCenter" => ToolState {
                enabled: has_section && self.center_doc.is_some(),
                checked: active_choice == Some(MergeChoice::TakeCenter),
            },
            "TakeRight" => ToolState {
                enabled: has_section,
                checked: active_choice == Some(MergeChoice::TakeRight),
            },
            "FullEdit" => ToolState::checked(self.full_edit_output),
            "ConflictSectionNext" | "ConflictSectionPrev" => {
                ToolState::enabled(self.result.conflict_sections > 0)
            }
            "DiffSectionNext" | "DiffSectionPrev" => {
                ToolState::enabled(self.result.changed_sections > 0)
            }
            _ => ToolState::DISABLED,
        };
        let items = super::toolbar_spec::TEXT_MERGE;
        let mut tool_event = None;
        if self.show_toolbar {
            let toolbar_height =
                super::toolbar::height(ui, items, ui.available_width(), &tool_state);
            TopBottomPanel::top("text_merge_toolbar")
                .exact_height(toolbar_height)
                .show_separator_line(false)
                .frame(Frame::none())
                .show_inside(ui, |ui| {
                    tool_event = super::toolbar::show(
                        ui,
                        icons,
                        items,
                        true,
                        &tool_state,
                        &mut |_, _, _| {},
                    );
                });
        }

        match tool_event {
            Some(ToolEvent::Click("SessionHome")) => action = TextMergeAction::Home,
            Some(ToolEvent::Click("FavMergeAll")) => self.filter = MergeFilter::All,
            Some(ToolEvent::Click("FavChanged")) => self.filter = MergeFilter::Changed,
            Some(ToolEvent::Click("FavConflict")) => self.filter = MergeFilter::Conflicts,
            Some(ToolEvent::Click("FavLeft")) => self.filter = MergeFilter::LeftChanges,
            Some(ToolEvent::Click("FavRight")) => self.filter = MergeFilter::RightChanges,
            Some(ToolEvent::Click("FavMergeable")) => self.filter = MergeFilter::Mergeable,
            Some(ToolEvent::Click("FavUnchanged")) => self.filter = MergeFilter::Unchanged,
            Some(ToolEvent::Click("ShowContext")) => self.show_context = !self.show_context,
            Some(ToolEvent::Click("InsigIgnored")) => {
                self.ignore_unimportant = !self.ignore_unimportant;
                self.recompute();
            }
            Some(ToolEvent::Click("Rules")) => self.open_rules_dialog(dialogs),
            Some(ToolEvent::Click("Conflict")) => {
                self.set_section_choice(self.active_section, MergeChoice::ConflictUnresolved);
            }
            Some(ToolEvent::Click("TakeLeft")) => {
                self.set_section_choice(self.active_section, MergeChoice::TakeLeft);
            }
            Some(ToolEvent::Click("TakeCenter")) => {
                self.set_section_choice(self.active_section, MergeChoice::TakeCenter);
            }
            Some(ToolEvent::Click("TakeRight")) => {
                self.set_section_choice(self.active_section, MergeChoice::TakeRight);
            }
            Some(ToolEvent::Click("FullEdit")) => {
                self.full_edit_output = !self.full_edit_output;
                if self.full_edit_output {
                    self.sync_free_text();
                }
            }
            Some(ToolEvent::Click("ConflictSectionNext")) => self.jump_section(true, true),
            Some(ToolEvent::Click("ConflictSectionPrev")) => self.jump_section(false, true),
            Some(ToolEvent::Click("DiffSectionNext")) => self.jump_section(true, false),
            Some(ToolEvent::Click("DiffSectionPrev")) => self.jump_section(false, false),
            Some(ToolEvent::Click("Swap")) => self.swap_sides(),
            Some(ToolEvent::Click("Reload")) => self.reload(dialogs),
            _ => {}
        }

        // 4. Status Bar
        TopBottomPanel::bottom("text_merge_status")
            .exact_height(22.0)
            .frame(
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(8.0, 3.0)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let conflict_col = if self.result.unresolved_conflicts > 0 {
                        CONFLICT_TEXT
                    } else {
                        BOTH_SAME_TEXT
                    };
                    ui.label(
                        RichText::new(format!(
                            "{}: {}  |  {}: {}  |  {}: {}  |  {}: {}  |  {}: {}",
                            crate::i18n::tr("common.conflicts"),
                            self.result.unresolved_conflicts,
                            crate::i18n::tr("common.left"),
                            self.result.left_changes,
                            crate::i18n::tr("common.right"),
                            self.result.right_changes,
                            crate::i18n::tr("merge.both"),
                            self.result.both_same,
                            crate::i18n::tr("common.output"),
                            self.result.output_rows.len(),
                        ))
                        .size(11.5)
                        .color(conflict_col),
                    );
                });
                super::chrome::status_bar_grip(ui.painter(), ui.max_rect());
            });

        // 5. Bottom Output Pane (BC5 signature bottom merged-output window)
        let avail_h = ui.available_height();
        TopBottomPanel::bottom("text_merge_output_pane")
            .resizable(true)
            .default_height((avail_h * 0.42).clamp(140.0, 340.0))
            .min_height(110.0)
            .frame(
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(168))),
            )
            .show_inside(ui, |ui| {
                // Output header bar: Output path + Save + Quick Resolution buttons
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(6.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(tex) = icons.get_icon(ctx, "imgOutputWindow") {
                                ui.image((tex.id(), vec2(15.0, 15.0)));
                            }
                            ui.label(RichText::new(crate::i18n::tr("merge.output")).strong());
                            let path_w = (ui.available_width() - 420.0).clamp(140.0, 420.0);
                            ui.add(
                                TextEdit::singleline(&mut self.output_path)
                                    .desired_width(path_w)
                                    .hint_text(crate::i18n::tr("merge.merged_output_file")),
                            );
                            if icon_btn(ui, icons, "imgBrowse", "merge.select_output_file")
                                .clicked()
                            {
                                if let Some(p) = dialogs.named_file(
                                    &crate::i18n::tr("merge.save_output"),
                                    true,
                                    self.output_doc.path.as_deref(),
                                    "merged.txt",
                                ) {
                                    self.output_path = p.to_string_lossy().into_owned();
                                }
                            }
                            if ui
                                .add(
                                    Button::new(format!(
                                        "{}{}",
                                        if self.dirty { "* " } else { "" },
                                        crate::i18n::tr("merge.save_output")
                                    ))
                                    .fill(Color32::from_rgb(225, 240, 255))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0, 120, 212))),
                                )
                                .clicked()
                            {
                                self.save_output(false, dialogs);
                            }

                            ui.separator();
                            if ui
                                .add_enabled(has_section, Button::new(crate::i18n::tr("common.left")))
                                .clicked()
                            {
                                self.set_section_choice(self.active_section, MergeChoice::TakeLeft);
                            }
                            if self.center_doc.is_some()
                                && ui
                                    .add_enabled(
                                        has_section,
                                        Button::new(crate::i18n::tr("common.center")),
                                    )
                                    .clicked()
                            {
                                self.set_section_choice(
                                    self.active_section,
                                    MergeChoice::TakeCenter,
                                );
                            }
                            if ui
                                .add_enabled(has_section, Button::new(crate::i18n::tr("common.right")))
                                .clicked()
                            {
                                self.set_section_choice(
                                    self.active_section,
                                    MergeChoice::TakeRight,
                                );
                            }
                            if ui
                                .add_enabled(has_section, Button::new("E+D"))
                                .on_hover_text(crate::i18n::tr("merge.take_left_then_right"))
                                .clicked()
                            {
                                self.set_section_choice(
                                    self.active_section,
                                    MergeChoice::TakeLeftThenRight,
                                );
                            }
                            if ui
                                .add_enabled(has_section, Button::new("D+E"))
                                .on_hover_text(crate::i18n::tr("merge.take_right_then_left"))
                                .clicked()
                            {
                                self.set_section_choice(
                                    self.active_section,
                                    MergeChoice::TakeRightThenLeft,
                                );
                            }
                        });
                    });

                // Output Content Area
                Frame::none()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(190)))
                    .show(ui, |ui| {
                        let mono = FontId::new(
                            self.editor_font_size,
                            FontFamily::Name("Text Mono".into()),
                        );
                        if self.full_edit_output {
                            ScrollArea::both()
                                .id_salt("text_merge_free_edit")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let edit = TextEdit::multiline(&mut self.output_free_text)
                                        .font(mono)
                                        .desired_width(f32::INFINITY)
                                        .frame(false);
                                    if ui.add(edit).changed() {
                                        self.dirty = true;
                                    }
                                });
                        } else {
                            let total_rows = self.result.output_rows.len();
                            ScrollArea::both()
                                .id_salt("text_merge_output_rows")
                                .auto_shrink([false, false])
                                .show_rows(ui, ROW_HEIGHT, total_rows, |ui, range| {
                                    let avail_w = ui.available_width().max(400.0);
                                    for r_idx in range {
                                        let row = &self.result.output_rows[r_idx];
                                        let (rect, resp) = ui.allocate_exact_size(
                                            vec2(avail_w, ROW_HEIGHT),
                                            Sense::click(),
                                        );
                                        let is_active = row.section_idx == self.active_section;
                                        let (bg, fg, tag_txt, tag_col) = match row.choice_tag {
                                            OutputSourceTag::Same => {
                                                (Color32::WHITE, Color32::BLACK, "", Color32::GRAY)
                                            }
                                            OutputSourceTag::Left => (
                                                LEFT_CHANGE_BG,
                                                LEFT_CHANGE_TEXT,
                                                "E",
                                                LEFT_CHANGE_TEXT,
                                            ),
                                            OutputSourceTag::Right => (
                                                RIGHT_CHANGE_BG,
                                                RIGHT_CHANGE_TEXT,
                                                "D",
                                                RIGHT_CHANGE_TEXT,
                                            ),
                                            OutputSourceTag::Center => (
                                                Color32::from_gray(242),
                                                Color32::from_gray(70),
                                                "C",
                                                Color32::from_gray(90),
                                            ),
                                            OutputSourceTag::Both => (
                                                BOTH_SAME_BG,
                                                BOTH_SAME_TEXT,
                                                "✓",
                                                BOTH_SAME_TEXT,
                                            ),
                                            OutputSourceTag::Conflict => (
                                                CONFLICT_BG,
                                                CONFLICT_TEXT,
                                                "!",
                                                CONFLICT_TEXT,
                                            ),
                                        };
                                        ui.painter().rect_filled(rect, 0.0, bg);
                                        let gutter = Rect::from_min_size(
                                            rect.min,
                                            vec2(GUTTER_W, ROW_HEIGHT),
                                        );
                                        ui.painter()
                                            .rect_filled(gutter, 0.0, BcTheme::GUTTER_BG);
                                        ui.painter().vline(
                                            gutter.right(),
                                            rect.y_range(),
                                            Stroke::new(1.0_f32, Color32::from_gray(205)),
                                        );
                                        if is_active {
                                            ui.painter().rect_filled(
                                                Rect::from_min_size(rect.min, vec2(3.0, ROW_HEIGHT)),
                                                0.0,
                                                Color32::from_rgb(0, 120, 212),
                                            );
                                        }
                                        if !tag_txt.is_empty() {
                                            ui.painter().text(
                                                pos2(gutter.left() + 10.0, gutter.center().y),
                                                Align2::CENTER_CENTER,
                                                tag_txt,
                                                FontId::proportional(11.0),
                                                tag_col,
                                            );
                                        }
                                        ui.painter().text(
                                            pos2(gutter.right() - 5.0, gutter.center().y),
                                            Align2::RIGHT_CENTER,
                                            row.output_line_no.to_string(),
                                            FontId::proportional(11.0),
                                            BcTheme::GUTTER_TEXT,
                                        );
                                        ui.painter().text(
                                            pos2(gutter.right() + 6.0, rect.center().y),
                                            Align2::LEFT_CENTER,
                                            &row.text,
                                            mono.clone(),
                                            fg,
                                        );
                                        if resp.clicked() {
                                            self.active_section = row.section_idx;
                                        }
                                    }
                                });
                        }
                    });
            });

        // 6. Central Top 3-Pane View (Thumbnail + Left | Center | Right)
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::WHITE))
            .show_inside(ui, |ui| {
                let rect = ui.max_rect();
                let pane_count = if self.show_center { 3 } else { 2 };
                let content_w = (rect.width() - THUMB_W).max(240.0);
                let pane_w = content_w / pane_count as f32;

                // Top Path Header Strip (Left | Center | Right)
                let path_h = 28.0;
                let path_rect = Rect::from_min_size(
                    pos2(rect.left() + THUMB_W, rect.top()),
                    vec2(content_w, path_h),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min, vec2(rect.width(), path_h)),
                    0.0,
                    BcTheme::WINDOW_BG,
                );
                ui.painter().hline(
                    rect.x_range(),
                    path_rect.bottom() - 0.5,
                    Stroke::new(1.0_f32, Color32::from_gray(172)),
                );

                let mut need_reload = false;
                let mut pane_rects = Vec::new();
                for p_idx in 0..pane_count {
                    let r = Rect::from_min_size(
                        pos2(path_rect.left() + p_idx as f32 * pane_w, path_rect.top()),
                        vec2(pane_w, path_h),
                    );
                    pane_rects.push(r);
                }

                // Render path pickers for each pane
                let labels_and_slots: Vec<(&str, usize)> = if self.show_center {
                    vec![("merge.left", 0), ("merge.center", 1), ("merge.right", 2)]
                } else {
                    vec![("merge.left", 0), ("merge.right", 2)]
                };
                for (col, (lbl, slot)) in labels_and_slots.into_iter().enumerate() {
                    let pr = pane_rects[col];
                    let mut child = ui.new_child(
                        UiBuilder::new().max_rect(pr.shrink2(vec2(4.0, 3.0))),
                    );
                    let mut open_clipboard_slot: Option<usize> = None;
                    child.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let tag_color = match slot {
                            0 => LEFT_CHANGE_TEXT,
                            1 => Color32::from_gray(75),
                            _ => RIGHT_CHANGE_TEXT,
                        };
                        ui.label(
                            RichText::new(crate::i18n::tr(lbl))
                                .size(11.5)
                                .strong()
                                .color(tag_color),
                        );
                        let (path_ref, loaded_path) = match slot {
                            0 => (&mut self.left_path, self.left_doc.path.clone()),
                            1 => (
                                &mut self.center_path,
                                self.center_doc.as_ref().and_then(|d| d.path.clone()),
                            ),
                            _ => (&mut self.right_path, self.right_doc.path.clone()),
                        };
                        let fw = (ui.available_width() - 52.0).max(50.0);
                        let resp = ui.add(TextEdit::singleline(path_ref).desired_width(fw));
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            need_reload = true;
                        }
                        let typed = path_ref.clone();
                        let pending_load = !typed.trim().is_empty()
                            && loaded_path.as_deref().map(|p| p.to_string_lossy())
                                != Some(std::borrow::Cow::Borrowed(typed.as_str()))
                            && typed != crate::i18n::tr("file.open_clipboard");
                        let (_, browse_act) = super::chrome::file_browse_buttons(
                            ui,
                            icons,
                            ("text_merge_browse", slot),
                            pending_load,
                            false,
                            false,
                            true,
                            loaded_path.is_some(),
                        );
                        match browse_act {
                            Some(super::chrome::BrowseMenuAction::LoadTyped) => {
                                if !typed.trim().is_empty() {
                                    need_reload = true;
                                }
                            }
                            Some(
                                super::chrome::BrowseMenuAction::OpenFileSystem
                                | super::chrome::BrowseMenuAction::OpenProfile,
                            ) => {
                                let cur = (!path_ref.trim().is_empty())
                                    .then(|| PathBuf::from(path_ref.as_str()));
                                if let Some(p) = dialogs.file(false, slot == 0, cur.as_deref()) {
                                    *path_ref = p.to_string_lossy().into_owned();
                                    need_reload = true;
                                }
                            }
                            Some(super::chrome::BrowseMenuAction::OpenClipboard) => {
                                open_clipboard_slot = Some(slot);
                            }
                            Some(super::chrome::BrowseMenuAction::OpenWithTextEdit) => {
                                if let Some(p) = loaded_path.clone() {
                                    super::session_menu::request(
                                        ctx,
                                        super::session_menu::Request::EditText(p),
                                    );
                                }
                            }
                            Some(super::chrome::BrowseMenuAction::OpenWithAssociated) => {
                                if let Some(p) = loaded_path.clone() {
                                    if let Err(err) = dialogs.open_path(&p, false) {
                                        dialogs.error(&err);
                                    }
                                }
                            }
                            None => {}
                        }
                    });
                    if let Some(cb_slot) = open_clipboard_slot {
                        if let Some(text) = super::chrome::read_system_clipboard_text() {
                            let doc = Document::from_clipboard_text(text);
                            let label = crate::i18n::tr("file.open_clipboard");
                            match cb_slot {
                                0 => {
                                    self.left_doc = doc;
                                    self.left_path = label;
                                }
                                1 => {
                                    self.center_doc = Some(doc);
                                    self.center_path = label;
                                }
                                _ => {
                                    self.right_doc = doc;
                                    self.right_path = label;
                                }
                            }
                            self.recompute();
                        }
                    }
                    if col + 1 < pane_count {
                        ui.painter().vline(
                            pr.right(),
                            rect.y_range(),
                            Stroke::new(1.0_f32, Color32::from_gray(172)),
                        );
                    }
                }
                if need_reload {
                    self.reload(dialogs);
                }

                // Thumbnail bar on the far left
                let body_top = path_rect.bottom();
                let body_h = (rect.bottom() - body_top).max(40.0);
                let thumb_rect = Rect::from_min_size(pos2(rect.left(), body_top), vec2(THUMB_W, body_h));
                ui.painter()
                    .rect_filled(thumb_rect, 0.0, Color32::from_gray(242));
                ui.painter().vline(
                    thumb_rect.right(),
                    thumb_rect.y_range(),
                    Stroke::new(1.0_f32, Color32::from_gray(180)),
                );
                let total_all = self.result.rows.len().max(1);
                for (i, row) in self.result.rows.iter().enumerate() {
                    if row.kind == MergeSectionKind::Unchanged {
                        continue;
                    }
                    let y = thumb_rect.top() + (i as f32 / total_all as f32) * body_h;
                    let col = match row.kind {
                        MergeSectionKind::Conflict => CONFLICT_TEXT,
                        MergeSectionKind::LeftOnly => LEFT_CHANGE_TEXT,
                        MergeSectionKind::RightOnly => RIGHT_CHANGE_TEXT,
                        MergeSectionKind::BothSame => BOTH_SAME_TEXT,
                        MergeSectionKind::Unchanged => Color32::TRANSPARENT,
                    };
                    ui.painter().rect_filled(
                        Rect::from_min_size(pos2(thumb_rect.left() + 2.0, y), vec2(THUMB_W - 4.0, 2.5)),
                        0.0,
                        col,
                    );
                }

                // Synchronized 3-pane rows
                let visible = visible_row_indices(&self.result, self.filter, self.show_context, 2);
                let body_rect = Rect::from_min_size(
                    pos2(rect.left() + THUMB_W, body_top),
                    vec2(content_w, body_h),
                );
                let mono = FontId::new(self.editor_font_size, FontFamily::Name("Text Mono".into()));
                let mut pick_choice: Option<(usize, MergeChoice)> = None;

                let mut scroll = ScrollArea::vertical()
                    .id_salt("text_merge_top_rows")
                    .auto_shrink([false, false]);
                if let Some(target_row) = self.pending_scroll_row.take() {
                    if let Some(vis_pos) = visible.iter().position(|&r| r >= target_row) {
                        scroll = scroll.vertical_scroll_offset(
                            (vis_pos.saturating_sub(2) as f32) * ROW_HEIGHT,
                        );
                    }
                }

                let mut child = ui.new_child(UiBuilder::new().max_rect(body_rect));
                let mut merge_jump_conflict: Option<bool> = None;
                let mut merge_copy_text: Option<String> = None;
                let mut merge_explore: Option<(usize, bool)> = None;
                let show_center = self.show_center;
                let has_conflicts = self.result.conflict_sections > 0;
                let has_left_path = !self.left_path.trim().is_empty();
                let has_right_path = !self.right_path.trim().is_empty();
                scroll.show_rows(&mut child, ROW_HEIGHT, visible.len(), |ui, range| {
                    for vis_pos in range {
                        let row_idx = visible[vis_pos];
                        let row = &self.result.rows[row_idx];
                        let (row_rect, row_resp) =
                            ui.allocate_exact_size(vec2(content_w, ROW_HEIGHT), Sense::click());
                        let is_active = row.section_idx == self.active_section;
                        let is_sec_first = row_idx == 0
                            || self.result.rows[row_idx - 1].section_idx != row.section_idx;

                        let slots: &[(usize, Option<usize>, &str)] = if self.show_center {
                            &[
                                (0, row.left_line_no, &row.left_text),
                                (1, row.center_line_no, &row.center_text),
                                (2, row.right_line_no, &row.right_text),
                            ]
                        } else {
                            &[
                                (0, row.left_line_no, &row.left_text),
                                (2, row.right_line_no, &row.right_text),
                            ]
                        };

                        for (col_idx, &(slot, line_no, text)) in slots.iter().enumerate() {
                            let cell = Rect::from_min_size(
                                pos2(row_rect.left() + col_idx as f32 * pane_w, row_rect.top()),
                                vec2(pane_w, ROW_HEIGHT),
                            );
                            let (bg, fg) = merge_cell_colors(row.kind, slot, line_no.is_none());
                            ui.painter().rect_filled(cell, 0.0, bg);
                            if line_no.is_none() && row.kind != MergeSectionKind::Unchanged {
                                draw_diagonal_hatch(ui.painter(), cell);
                            }

                            // Gutter
                            let gutter = Rect::from_min_size(cell.min, vec2(GUTTER_W, ROW_HEIGHT));
                            ui.painter().rect_filled(gutter, 0.0, BcTheme::GUTTER_BG);
                            ui.painter().vline(
                                gutter.right(),
                                cell.y_range(),
                                Stroke::new(1.0_f32, Color32::from_gray(205)),
                            );
                            if is_active {
                                ui.painter().rect_filled(
                                    Rect::from_min_size(cell.min, vec2(3.0, ROW_HEIGHT)),
                                    0.0,
                                    Color32::from_rgb(0, 120, 212),
                                );
                            }
                            if let Some(n) = line_no {
                                ui.painter().text(
                                    pos2(gutter.right() - 5.0, gutter.center().y),
                                    Align2::RIGHT_CENTER,
                                    n.to_string(),
                                    FontId::proportional(11.0),
                                    BcTheme::GUTTER_TEXT,
                                );
                            }

                            // Clickable Take-Arrow in gutter on first row of every changed section
                            if is_sec_first && row.kind != MergeSectionKind::Unchanged {
                                let arrow_r = Rect::from_min_size(
                                    pos2(gutter.left() + 4.0, gutter.top() + 1.0),
                                    vec2(15.0, ROW_HEIGHT - 2.0),
                                );
                                let arrow_resp = ui
                                    .interact(
                                        arrow_r,
                                        ui.id().with(("tm_take", row.section_idx, slot)),
                                        Sense::click(),
                                    )
                                    .on_hover_text(crate::i18n::tr(match slot {
                                        0 => "merge.take_left",
                                        1 => "merge.take_center",
                                        _ => "merge.take_right",
                                    }));
                                let arrow_col = if arrow_resp.hovered() {
                                    Color32::from_rgb(0, 102, 204)
                                } else if row.unresolved {
                                    CONFLICT_TEXT
                                } else {
                                    Color32::from_gray(90)
                                };
                                ui.painter().text(
                                    arrow_r.center(),
                                    Align2::CENTER_CENTER,
                                    if row.unresolved && slot == 1 { "!" } else { "↓" },
                                    FontId::proportional(12.0),
                                    arrow_col,
                                );
                                if arrow_resp.clicked() {
                                    let choice = match slot {
                                        0 => MergeChoice::TakeLeft,
                                        1 => MergeChoice::TakeCenter,
                                        _ => MergeChoice::TakeRight,
                                    };
                                    pick_choice = Some((row.section_idx, choice));
                                }
                            }

                            // Clipped line text
                            let text_clip = Rect::from_min_max(
                                pos2(gutter.right() + 4.0, cell.top()),
                                pos2(cell.right() - 2.0, cell.bottom()),
                            );
                            ui.painter().with_clip_rect(text_clip).text(
                                pos2(gutter.right() + 6.0, cell.center().y),
                                Align2::LEFT_CENTER,
                                text,
                                mono.clone(),
                                fg,
                            );

                            if col_idx + 1 < slots.len() {
                                ui.painter().vline(
                                    cell.right(),
                                    cell.y_range(),
                                    Stroke::new(1.0_f32, Color32::from_gray(172)),
                                );
                            }
                        }

                        if row_resp.clicked() || row_resp.secondary_clicked() {
                            self.active_section = row.section_idx;
                        }
                        let sec_idx = row.section_idx;
                        let sec_changed = row.kind != MergeSectionKind::Unchanged;
                        let clicked_slot = row_resp
                            .interact_pointer_pos()
                            .map(|p| {
                                let col = ((p.x - row_rect.left()) / pane_w.max(1.0)).floor() as usize;
                                slots.get(col).map_or(0, |s| s.0)
                            })
                            .unwrap_or(0);
                        let line_for_copy = match clicked_slot {
                            0 => row.left_text.clone(),
                            1 => row.center_text.clone(),
                            _ => row.right_text.clone(),
                        };
                        row_resp.context_menu(|ui| {
                            ui.set_min_width(205.0);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.spacing_mut().button_padding = vec2(4.0, 2.0);
                            ui.spacing_mut().interact_size.y = 20.0;
                            if super::chrome::menu_item(ui, icons, "merge.take_left", sec_changed)
                                .clicked()
                            {
                                pick_choice = Some((sec_idx, MergeChoice::TakeLeft));
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "merge.take_center",
                                sec_changed && show_center,
                            )
                            .clicked()
                            {
                                pick_choice = Some((sec_idx, MergeChoice::TakeCenter));
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(ui, icons, "merge.take_right", sec_changed)
                                .clicked()
                            {
                                pick_choice = Some((sec_idx, MergeChoice::TakeRight));
                                ui.close_menu();
                            }
                            ui.separator();
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "common.copy  Ctrl+C",
                                !line_for_copy.is_empty(),
                            )
                            .clicked()
                            {
                                merge_copy_text = Some(line_for_copy.clone());
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "merge.next_conflict",
                                has_conflicts,
                            )
                            .clicked()
                            {
                                merge_jump_conflict = Some(true);
                                ui.close_menu();
                            }
                            if super::chrome::menu_item(
                                ui,
                                icons,
                                "merge.prev_conflict",
                                has_conflicts,
                            )
                            .clicked()
                            {
                                merge_jump_conflict = Some(false);
                                ui.close_menu();
                            }
                            ui.separator();
                            let side_has_path = match clicked_slot {
                                0 => has_left_path,
                                2 => has_right_path,
                                _ => has_left_path || has_right_path,
                            };
                            super::chrome::submenu_item(ui, icons, "table.explorer", |ui, icons| {
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "version.select_in_explorer",
                                    side_has_path,
                                )
                                .clicked()
                                {
                                    merge_explore = Some((clicked_slot, true));
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "version.open_folder",
                                    side_has_path,
                                )
                                .clicked()
                                {
                                    merge_explore = Some((clicked_slot, false));
                                    ui.close_menu();
                                }
                            });
                        });
                    }
                });

                if let Some((sec_idx, choice)) = pick_choice {
                    self.active_section = sec_idx;
                    self.set_section_choice(sec_idx, choice);
                }
                if let Some(forward) = merge_jump_conflict {
                    self.jump_section(forward, true);
                }
                if let Some(txt) = merge_copy_text {
                    ctx.copy_text(txt);
                }
                if let Some((slot, select)) = merge_explore {
                    let raw = match slot {
                        0 => self.left_path.trim(),
                        1 if !self.center_path.trim().is_empty() => self.center_path.trim(),
                        2 => self.right_path.trim(),
                        _ => self.left_path.trim(),
                    };
                    if !raw.is_empty() {
                        let p = PathBuf::from(raw);
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
            });

        action
    }
}

fn merge_cell_colors(kind: MergeSectionKind, slot: usize, missing: bool) -> (Color32, Color32) {
    if missing && kind != MergeSectionKind::Unchanged {
        return (Color32::from_gray(246), Color32::GRAY);
    }
    match kind {
        MergeSectionKind::Unchanged => (Color32::WHITE, Color32::BLACK),
        MergeSectionKind::LeftOnly => {
            if slot == 0 {
                (LEFT_CHANGE_BG, LEFT_CHANGE_TEXT)
            } else {
                (Color32::WHITE, Color32::from_gray(95))
            }
        }
        MergeSectionKind::RightOnly => {
            if slot == 2 {
                (RIGHT_CHANGE_BG, RIGHT_CHANGE_TEXT)
            } else {
                (Color32::WHITE, Color32::from_gray(95))
            }
        }
        MergeSectionKind::BothSame => (BOTH_SAME_BG, BOTH_SAME_TEXT),
        MergeSectionKind::Conflict => {
            if slot == 1 {
                (Color32::from_rgb(252, 240, 240), Color32::from_gray(70))
            } else {
                (CONFLICT_BG, CONFLICT_TEXT)
            }
        }
    }
}

fn draw_diagonal_hatch(painter: &Painter, rect: Rect) {
    let clipped = painter.with_clip_rect(rect);
    let step = 8.0_f32;
    let stroke = Stroke::new(1.0_f32, Color32::from_gray(212));
    let k_min = ((rect.left() + rect.top()) / step).floor() as i32;
    let k_max = ((rect.right() + rect.bottom()) / step).ceil() as i32;
    for k in k_min..=k_max {
        let s = k as f32 * step;
        clipped.line_segment(
            [pos2(s - rect.bottom(), rect.bottom()), pos2(s - rect.top(), rect.top())],
            stroke,
        );
    }
}

fn icon_btn(ui: &mut Ui, icons: &mut IconManager, name: &str, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 21.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, Color32::from_rgb(229, 243, 255));
        ui.painter()
            .rect_stroke(rect, 2.0, Stroke::new(1.0_f32, Color32::from_rgb(204, 232, 255)));
    }
    if let Some(tex) = icons.get_icon(ui.ctx(), name) {
        ui.painter().image(
            tex.id(),
            Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    resp.on_hover_text(crate::i18n::tr(tip))
}
