//! AnyCompare Folder Sync session UI (`Sincronizar Pastas`).

use super::dialogs::{DialogField, DialogHost};
use super::folder_ops::{
    self, CompareResultRow, CompareResultsState, CompareRowStatus, ContentCompareMode,
    FileContentVerdict, FolderOpConfirmed, FolderOpModal, SelectionStats,
};
use super::toolbar::{ToolEvent, ToolState};
use crate::folder_source::EntryMeta;
use crate::folder_sync::{
    build_sync_plan, cycle_entry_action, execute_sync, format_time, recompute_actions, summarize,
    timestamp_now_hms, SyncAction, SyncEntry, SyncEntryStatus, SyncExecReport, SyncOptions,
    SyncPreset,
};
use crate::history::{RecentSession, SessionKind};
use crate::ui::{icons::IconManager, theme::BcTheme};
use egui::*;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
};

const ROW_HEIGHT: f32 = 18.0;
const ORPHAN_COLOR: Color32 = Color32::from_rgb(128, 0, 255);
const CONFLICT_BG: Color32 = Color32::from_rgb(255, 232, 232);
const COPY_RIGHT_BG: Color32 = Color32::from_rgb(236, 245, 255);
const COPY_LEFT_BG: Color32 = Color32::from_rgb(245, 238, 255);
const DELETE_BG: Color32 = Color32::from_rgb(255, 240, 240);

pub enum FolderSyncAction {
    None,
    Home,
    CompareFiles(PathBuf, PathBuf),
}

enum SyncJobResult {
    Plan(Result<Vec<SyncEntry>, String>),
    Executed(Result<SyncExecReport, String>),
}

pub struct FolderSyncScreen {
    pub left_path: String,
    pub right_path: String,
    pub options: SyncOptions,
    pub entries: Vec<SyncEntry>,
    pub collapsed: BTreeSet<String>,
    pub selected: BTreeSet<usize>,
    pub show_only_actions: bool,
    pub show_log: bool,
    pub show_toolbar: bool,
    pub log_lines: Vec<String>,
    pub active_left: bool,
    pending_rx: Option<mpsc::Receiver<SyncJobResult>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    running_exec: bool,
    last_error: Option<String>,
    content_mode: ContentCompareMode,
    compare_modal: Option<FolderOpModal>,
}

impl Default for FolderSyncScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl FolderSyncScreen {
    pub fn new() -> Self {
        Self {
            left_path: String::new(),
            right_path: String::new(),
            options: SyncOptions::default(),
            entries: Vec::new(),
            collapsed: BTreeSet::new(),
            selected: BTreeSet::new(),
            show_only_actions: false,
            show_log: true,
            show_toolbar: true,
            log_lines: Vec::new(),
            active_left: true,
            pending_rx: None,
            cancel_flag: None,
            running_exec: false,
            last_error: None,
            content_mode: ContentCompareMode::Binary,
            compare_modal: None,
        }
    }

    fn selected_indices_with_descendants(&self) -> Vec<usize> {
        let sel_dirs: Vec<PathBuf> = self
            .selected
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .filter(|e| e.is_dir)
            .map(|e| e.relative.clone())
            .collect();
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                (self.selected.contains(&i)
                    || sel_dirs.iter().any(|d| e.relative.starts_with(d) && e.relative != *d))
                .then_some(i)
            })
            .collect()
    }

    fn sync_selection_stats(&self) -> SelectionStats {
        let mut stats = SelectionStats::default();
        for idx in self.selected_indices_with_descendants() {
            let e = &self.entries[idx];
            if let Some(m) = &e.left {
                if e.is_dir {
                    stats.left_dirs += 1;
                } else {
                    stats.left_files += 1;
                    stats.left_bytes = stats.left_bytes.saturating_add(m.size);
                }
            }
            if let Some(m) = &e.right {
                if e.is_dir {
                    stats.right_dirs += 1;
                } else {
                    stats.right_files += 1;
                    stats.right_bytes = stats.right_bytes.saturating_add(m.size);
                }
            }
        }
        stats
    }

    fn run_sync_content_compare(&mut self, mode: ContentCompareMode, show_results: bool) {
        self.content_mode = mode;
        let left_root = PathBuf::from(self.left_path.trim());
        let right_root = PathBuf::from(self.right_path.trim());
        let cancel = AtomicBool::new(false);
        let started = std::time::Instant::now();
        let indices = self.selected_indices_with_descendants();
        let mut rows = Vec::new();
        let (mut same, mut minor, mut different, mut left_orphans, mut right_orphans, mut errors) =
            (0, 0, 0, 0, 0, 0);
        for idx in indices {
            let e = &mut self.entries[idx];
            if e.is_dir {
                continue;
            }
            match (&e.left, &e.right) {
                (Some(lm), Some(rm)) => {
                    let l_meta = EntryMeta {
                        is_dir: false,
                        size: lm.size,
                        modified: None,
                        crc32: None,
                        ..EntryMeta::default()
                    };
                    let r_meta = EntryMeta {
                        is_dir: false,
                        size: rm.size,
                        modified: None,
                        crc32: None,
                        ..EntryMeta::default()
                    };
                    match folder_ops::compare_file_pair(
                        &left_root,
                        &e.relative,
                        &l_meta,
                        &right_root,
                        &e.relative,
                        &r_meta,
                        mode,
                        &cancel,
                    ) {
                        FileContentVerdict::Same { detail, .. } => {
                            same += 1;
                            e.status = SyncEntryStatus::Same;
                            e.action = SyncAction::Skip;
                            rows.push(CompareResultRow {
                                relative: e.relative.clone(),
                                right_relative: e.relative.clone(),
                                status: CompareRowStatus::Same,
                                detail,
                            });
                        }
                        FileContentVerdict::Minor { detail } => {
                            minor += 1;
                            rows.push(CompareResultRow {
                                relative: e.relative.clone(),
                                right_relative: e.relative.clone(),
                                status: CompareRowStatus::Minor,
                                detail,
                            });
                        }
                        FileContentVerdict::Different { detail, .. } => {
                            different += 1;
                            rows.push(CompareResultRow {
                                relative: e.relative.clone(),
                                right_relative: e.relative.clone(),
                                status: CompareRowStatus::Different,
                                detail,
                            });
                        }
                        FileContentVerdict::Error(err) => {
                            errors += 1;
                            rows.push(CompareResultRow {
                                relative: e.relative.clone(),
                                right_relative: e.relative.clone(),
                                status: CompareRowStatus::Error,
                                detail: err,
                            });
                        }
                    }
                }
                (Some(_), None) => {
                    left_orphans += 1;
                    rows.push(CompareResultRow {
                        relative: e.relative.clone(),
                        right_relative: PathBuf::new(),
                        status: CompareRowStatus::LeftOrphan,
                        detail: String::new(),
                    });
                }
                _ => {
                    right_orphans += 1;
                    rows.push(CompareResultRow {
                        relative: e.relative.clone(),
                        right_relative: e.relative.clone(),
                        status: CompareRowStatus::RightOrphan,
                        detail: String::new(),
                    });
                }
            }
        }
        if show_results {
            self.compare_modal = Some(FolderOpModal::CompareResults(CompareResultsState {
                mode,
                elapsed_ms: started.elapsed().as_millis(),
                same,
                minor,
                different,
                left_orphans,
                right_orphans,
                errors,
                rows,
                expanded: [false; 6],
                selected: None,
            }));
        }
    }

    pub fn load_folders(&mut self, left: &Path, right: &Path) {
        self.left_path = left.to_string_lossy().into_owned();
        self.right_path = right.to_string_lossy().into_owned();
        self.start_scan();
    }

    pub fn has_pair(&self) -> bool {
        !self.left_path.trim().is_empty() && !self.right_path.trim().is_empty()
    }

    pub fn is_busy(&self) -> bool {
        self.pending_rx.is_some()
    }

    pub fn stop_pending(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Synchronous scan helper used by CLI capture & tests.
    pub fn scan_sync(&mut self) -> Result<(), String> {
        self.stop_pending();
        self.pending_rx = None;
        self.cancel_flag = None;
        self.running_exec = false;
        let l = PathBuf::from(self.left_path.trim());
        let r = PathBuf::from(self.right_path.trim());
        let cancel = AtomicBool::new(false);
        let plan = build_sync_plan(&l, &r, &self.options, &cancel)?;
        self.entries = plan;
        self.selected.clear();
        self.last_error = None;
        let s = summarize(&self.entries);
        self.log_lines.push(format!(
            "{} {} ({}: {} ->, {} <-, {} X, {} !)",
            timestamp_now_hms(),
            crate::i18n::tr("sync.sync_preview_updated"),
            crate::i18n::tr(self.options.preset.label()),
            s.copy_right,
            s.copy_left,
            s.delete_left + s.delete_right,
            s.conflicts,
        ));
        Ok(())
    }

    pub fn start_scan(&mut self) {
        if !self.has_pair() {
            return;
        }
        self.stop_pending();
        let left = PathBuf::from(self.left_path.trim());
        let right = PathBuf::from(self.right_path.trim());
        let opts = self.options.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_bg = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel();
        self.pending_rx = Some(rx);
        self.cancel_flag = Some(cancel);
        self.running_exec = false;
        self.last_error = None;
        thread::spawn(move || {
            let res = build_sync_plan(&left, &right, &opts, &cancel_bg);
            let _ = tx.send(SyncJobResult::Plan(res));
        });
    }

    fn start_execute(&mut self, dialogs: &mut dyn DialogHost) {
        if !self.has_pair() || self.is_busy() {
            return;
        }
        let s = summarize(&self.entries);
        if s.total_operations() == 0 {
            dialogs.info(&crate::i18n::tr(
                "sync.there_are_no_pending_changes_to_sync",
            ));
            return;
        }
        let mirror_warn = if self.options.preset.is_mirror() {
            format!("\n⚠ {}", crate::i18n::tr("sync.mirror_mode_will_delete_orphan_files_at_the"))
        } else {
            String::new()
        };
        let confirm_msg = format!(
            "{} ({}){mirror_warn}\n\n• {}: {}\n• {}: {}\n• {}: {}\n• {}: {}{}",
            crate::i18n::tr("sync.run_sync_now"),
            crate::i18n::tr(self.options.preset.label()),
            crate::i18n::tr("menu.actions.copy_right_short"),
            s.copy_right,
            crate::i18n::tr("menu.actions.copy_left_short"),
            s.copy_left,
            crate::i18n::tr("sync.delete_left"),
            s.delete_left,
            crate::i18n::tr("sync.delete_right"),
            s.delete_right,
            if s.conflicts > 0 {
                format!(
                    "\n• {}: {} ({})",
                    crate::i18n::tr("common.conflicts"),
                    s.conflicts,
                    crate::i18n::tr("sync.will_be_skipped")
                )
            } else {
                String::new()
            }
        );
        if !dialogs.confirm(&confirm_msg) {
            return;
        }

        let left = PathBuf::from(self.left_path.trim());
        let right = PathBuf::from(self.right_path.trim());
        let plan = self.entries.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_bg = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel();
        self.pending_rx = Some(rx);
        self.cancel_flag = Some(cancel);
        self.running_exec = true;
        thread::spawn(move || {
            let res = execute_sync(&left, &right, &plan, &cancel_bg);
            let _ = tx.send(SyncJobResult::Executed(res));
        });
    }

    fn poll_background(&mut self, ctx: &Context, dialogs: &mut dyn DialogHost) {
        let Some(rx) = &self.pending_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(SyncJobResult::Plan(res)) => {
                self.pending_rx = None;
                self.cancel_flag = None;
                self.running_exec = false;
                match res {
                    Ok(plan) => {
                        self.entries = plan;
                        self.selected.clear();
                        self.last_error = None;
                        let s = summarize(&self.entries);
                        self.log_lines.push(format!(
                            "{} {} ({}: {} ->, {} <-, {} X, {} !)",
                            timestamp_now_hms(),
                            crate::i18n::tr("sync.sync_preview_updated"),
                            crate::i18n::tr(self.options.preset.label()),
                            s.copy_right,
                            s.copy_left,
                            s.delete_left + s.delete_right,
                            s.conflicts,
                        ));
                    }
                    Err(err) if err == crate::i18n::tr("common.canceled") => {
                        self.log_lines.push(format!(
                            "{} {}",
                            timestamp_now_hms(),
                            crate::i18n::tr("common.operation_canceled")
                        ));
                    }
                    Err(err) => {
                        self.last_error = Some(err.clone());
                        self.log_lines
                            .push(format!("{} {}: {err}", timestamp_now_hms(), crate::i18n::tr("merge.error")));
                    }
                }
            }
            Ok(SyncJobResult::Executed(res)) => {
                self.pending_rx = None;
                self.cancel_flag = None;
                self.running_exec = false;
                match res {
                    Ok(report) => {
                        let ts = timestamp_now_hms();
                        for line in report.log.iter().take(200) {
                            self.log_lines.push(format!("{ts} {line}"));
                        }
                        for err in &report.errors {
                            self.log_lines
                                .push(format!("{ts} {}: {err}", crate::i18n::tr("merge.error")));
                        }
                        self.log_lines.push(format!(
                            "{ts} {} ({} ->, {} <-, {} X, {} {})",
                            crate::i18n::tr("sync.sync_complete"),
                            report.copied_right,
                            report.copied_left,
                            report.deleted_left + report.deleted_right,
                            report.errors.len(),
                            crate::i18n::tr("sync.errors"),
                        ));
                        if !report.errors.is_empty() {
                            dialogs.error(&format!(
                                "{}\n{}",
                                crate::i18n::tr("sync.some_items_failed_to_sync"),
                                report.errors.iter().take(5).cloned().collect::<Vec<_>>().join("\n")
                            ));
                        }
                        self.start_scan();
                    }
                    Err(err) => {
                        dialogs.error(&err);
                        self.last_error = Some(err);
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending_rx = None;
                self.cancel_flag = None;
                self.running_exec = false;
            }
        }
    }

    pub fn swap_sides(&mut self) {
        std::mem::swap(&mut self.left_path, &mut self.right_path);
        self.options.preset = self.options.preset.swapped();
        if self.has_pair() {
            self.start_scan();
        }
    }

    pub fn saved_options(&self) -> serde_json::Value {
        serde_json::json!({
            "preset": format!("{:?}", self.options.preset),
            "compare_content": self.options.compare_content,
            "include_empty_dirs": self.options.include_empty_dirs,
            "name_filter": self.options.name_filter,
        })
    }

    pub fn apply_saved_options(&mut self, value: &serde_json::Value) {
        if let Some(p) = value["preset"].as_str() {
            self.options.preset = match p {
                "UpdateLeft" => SyncPreset::UpdateLeft,
                "UpdateBoth" => SyncPreset::UpdateBoth,
                "MirrorRight" => SyncPreset::MirrorRight,
                "MirrorLeft" => SyncPreset::MirrorLeft,
                _ => SyncPreset::UpdateRight,
            };
        }
        if let Some(v) = value["compare_content"].as_bool() {
            self.options.compare_content = v;
        }
        if let Some(v) = value["include_empty_dirs"].as_bool() {
            self.options.include_empty_dirs = v;
        }
        if let Some(v) = value["name_filter"].as_str() {
            self.options.name_filter = v.to_owned();
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for (idx, entry) in self.entries.iter().enumerate() {
            if self.show_only_actions && entry.action == SyncAction::None {
                continue;
            }
            let mut hidden = false;
            let mut ancestor = entry.relative.parent();
            while let Some(p) = ancestor {
                if !p.as_os_str().is_empty()
                    && self
                        .collapsed
                        .contains(&p.to_string_lossy().to_lowercase())
                {
                    hidden = true;
                    break;
                }
                ancestor = p.parent();
            }
            if !hidden {
                out.push(idx);
            }
        }
        out
    }

    fn open_rules_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        let presets: Vec<String> = SyncPreset::ALL
            .iter()
            .map(|p| crate::i18n::tr(p.label()))
            .collect();
        let current_preset = SyncPreset::ALL
            .iter()
            .position(|p| *p == self.options.preset)
            .unwrap_or(0);
        let mut fields = [
            DialogField::Choice(
                crate::i18n::tr("sync.sync_mode"),
                current_preset,
                presets,
            ),
            DialogField::Check(
                crate::i18n::tr("common.compare_file_contents_binary"),
                self.options.compare_content,
            ),
            DialogField::Check(
                crate::i18n::tr("sync.include_empty_folders"),
                self.options.include_empty_dirs,
            ),
            DialogField::Text(
                crate::i18n::tr("common.filters_colon"),
                self.options.name_filter.clone(),
            ),
        ];
        if dialogs.fields(&crate::i18n::tr("common.session_settings"), &mut fields) {
            if let DialogField::Choice(_, idx, _) = &fields[0] {
                if let Some(&preset) = SyncPreset::ALL.get(*idx) {
                    self.options.preset = preset;
                }
            }
            if let DialogField::Check(_, v) = fields[1] {
                self.options.compare_content = v;
            }
            if let DialogField::Check(_, v) = fields[2] {
                self.options.include_empty_dirs = v;
            }
            if let DialogField::Text(_, ref f) = fields[3] {
                self.options.name_filter = f.clone();
            }
            if self.has_pair() {
                self.start_scan();
            }
        }
    }

    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> FolderSyncAction {
        self.poll_background(ctx, dialogs);
        let mut action = FolderSyncAction::None;
        let pending = self.is_busy();
        if self.compare_modal.is_none() && !ctx.wants_keyboard_input() {
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
                self.selected = self.visible_indices().into_iter().collect();
            }
            if !pending && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
                self.start_scan();
            }
        }
        let summary = summarize(&self.entries);
        let can_sync = self.has_pair() && !pending && summary.total_operations() > 0;

        // 1. Menu Bar
        TopBottomPanel::top("folder_sync_menu")
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
                        if super::chrome::menu_item(ui, icons, "menu.actions.compare_refresh  F5", self.has_pair() && !pending)
                            .clicked()
                        {
                            self.start_scan();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.compare_base_folders",
                            self.has_pair(),
                        )
                        .clicked()
                        {
                            super::session_menu::request(
                                ui.ctx(),
                                super::session_menu::Request::CompareBaseFolders(
                                    PathBuf::from(&self.left_path),
                                    PathBuf::from(&self.right_path),
                                ),
                            );
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.sync_info", true).clicked() {
                            let pending_cnt = self
                                .entries
                                .iter()
                                .filter(|e| e.action != SyncAction::Skip)
                                .count();
                            let rows = vec![
                                (crate::i18n::tr("folder.side_left"), self.left_path.clone()),
                                (crate::i18n::tr("folder.side_right"), self.right_path.clone()),
                                (crate::i18n::tr(self.options.preset.label()), pending_cnt.to_string()),
                            ];
                            super::info_modals::open_info(
                                ui.ctx(),
                                crate::i18n::tr("folder.sync_info"),
                                rows,
                            );
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "sync.clear_session", true).clicked() {
                            self.left_path.clear();
                            self.right_path.clear();
                            self.entries.clear();
                            self.selected.clear();
                            ui.close_menu();
                        }
                        super::session_menu::suffix(ui, icons, self.has_pair());
                    });
                    super::chrome::menu_button(ui, "menu.actions", |ui| {
                        if super::chrome::menu_item(ui, icons, "sync.run_sync", can_sync)
                            .clicked()
                        {
                            self.start_execute(dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        for preset in SyncPreset::ALL {
                            if super::chrome::menu_radio_item(
                                ui,
                                icons,
                                preset.label(),
                                self.options.preset == preset,
                                true,
                            )
                            .clicked()
                            {
                                self.options.preset = preset;
                                recompute_actions(&mut self.entries, &self.options);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "sync.copy_left_to_right",
                            !self.selected.is_empty(),
                        )
                        .clicked()
                        {
                            for &idx in &self.selected {
                                if let Some(e) = self.entries.get_mut(idx) {
                                    if e.left.is_some() {
                                        e.action = SyncAction::CopyRight;
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "sync.copy_right_to_left",
                            !self.selected.is_empty(),
                        )
                        .clicked()
                        {
                            for &idx in &self.selected {
                                if let Some(e) = self.entries.get_mut(idx) {
                                    if e.right.is_some() {
                                        e.action = SyncAction::CopyLeft;
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "sync.leave_alone",
                            !self.selected.is_empty(),
                        )
                        .clicked()
                        {
                            for &idx in &self.selected {
                                if let Some(e) = self.entries.get_mut(idx) {
                                    e.action = SyncAction::Skip;
                                }
                            }
                            ui.close_menu();
                        }
                        let sel_dir_rel = self
                            .selected
                            .iter()
                            .filter_map(|&i| self.entries.get(i))
                            .find(|e| e.is_dir)
                            .map(|e| e.relative.clone());
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.set_as_base_folder",
                            sel_dir_rel.is_some(),
                        )
                        .clicked()
                        {
                            if let Some(rel) = &sel_dir_rel {
                                if self.active_left {
                                    self.left_path = Path::new(&self.left_path)
                                        .join(rel)
                                        .to_string_lossy()
                                        .into_owned();
                                } else {
                                    self.right_path = Path::new(&self.right_path)
                                        .join(rel)
                                        .to_string_lossy()
                                        .into_owned();
                                }
                                self.start_scan();
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.set_base_other_side",
                            sel_dir_rel.is_some(),
                        )
                        .clicked()
                        {
                            if let Some(rel) = &sel_dir_rel {
                                if self.active_left {
                                    self.right_path = Path::new(&self.left_path)
                                        .join(rel)
                                        .to_string_lossy()
                                        .into_owned();
                                } else {
                                    self.left_path = Path::new(&self.right_path)
                                        .join(rel)
                                        .to_string_lossy()
                                        .into_owned();
                                }
                                self.start_scan();
                            }
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.edit", |ui| {
                        if super::chrome::menu_item(ui, icons, "menu.edit.select_all  Ctrl+A", true)
                            .clicked()
                        {
                            self.selected = self.visible_indices().into_iter().collect();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "common.expand_all_title", true).clicked() {
                            self.collapsed.clear();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "common.collapse_all_title", true).clicked() {
                            self.collapsed = self
                                .entries
                                .iter()
                                .filter(|e| e.is_dir)
                                .map(|e| e.relative.to_string_lossy().to_lowercase())
                                .collect();
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.full_refresh  Ctrl+F5",
                            self.has_pair() && !pending,
                        )
                        .clicked()
                        {
                            self.start_scan();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "sync.prev_conflict", !self.entries.is_empty())
                            .clicked()
                        {
                            let cur = self.selected.iter().next().copied().unwrap_or(0);
                            if let Some(prev) = (0..cur)
                                .rev()
                                .find(|&i| self.entries[i].action == SyncAction::Conflict)
                            {
                                self.selected.clear();
                                self.selected.insert(prev);
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.find_next_filename  F3",
                            !self.entries.is_empty(),
                        )
                        .clicked()
                        {
                            let vis = self.visible_indices();
                            if let Some(&first) = vis.first() {
                                let cur = self.selected.iter().next().copied().unwrap_or(first);
                                let next = vis
                                    .iter()
                                    .copied()
                                    .find(|&i| i > cur)
                                    .unwrap_or(first);
                                self.selected.clear();
                                self.selected.insert(next);
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "folder.find_prev_filename  Shift+F3",
                            !self.entries.is_empty(),
                        )
                        .clicked()
                        {
                            let vis = self.visible_indices();
                            if let Some(&last) = vis.last() {
                                let cur = self.selected.iter().next().copied().unwrap_or(last);
                                let prev = vis
                                    .iter()
                                    .rev()
                                    .copied()
                                    .find(|&i| i < cur)
                                    .unwrap_or(last);
                                self.selected.clear();
                                self.selected.insert(prev);
                            }
                            ui.close_menu();
                        }
                    });
                    super::chrome::menu_button(ui, "menu.view", |ui| {
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "sync.only_pending_actions",
                            self.show_only_actions,
                            true,
                        )
                        .clicked()
                        {
                            self.show_only_actions = !self.show_only_actions;
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "sync.suppress_filters",
                            self.options.suppress_filters,
                            true,
                        )
                        .clicked()
                        {
                            self.options.suppress_filters = !self.options.suppress_filters;
                            if self.has_pair() {
                                self.start_scan();
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_check_item(
                            ui,
                            icons,
                            "merge.show_operation_log",
                            self.show_log,
                            true,
                        )
                        .clicked()
                        {
                            self.show_log = !self.show_log;
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(ui, icons, "folder.legend", true).clicked() {
                            super::info_modals::open_legend(ui.ctx());
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
                                super::options_dialog::OptionsCategory::FolderCompare,
                            );
                            ui.close_menu();
                        }
                        super::session_menu::settings_tools(ui, icons, dialogs);
                    });
                    super::chrome::menu_button(ui, "menu.help", |ui| {
                        if super::chrome::menu_item(ui, icons, "folder.legend", true).clicked() {
                            super::info_modals::open_legend(ui.ctx());
                            ui.close_menu();
                        }
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

        // 3. BC5 FOLDER_SYNC Toolbar
        let suppress = self.options.suppress_filters;
        let tool_state = |name: &str| match name {
            "SessionHome" | "SessionsCombo" | "ExpandAll" | "CollapseAll" | "SelectAll" => {
                ToolState::enabled(true)
            }
            "Refresh" => ToolState::enabled(self.has_pair() && !pending),
            "Stop" => ToolState::enabled(pending),
            "SuppressFilters" => ToolState::checked(suppress),
            "InsigIgnored" => ToolState::checked(!self.options.compare_content),
            _ => ToolState::DISABLED,
        };
        let items = super::toolbar_spec::FOLDER_SYNC;
        let mut tool_event = None;
        if self.show_toolbar {
            let toolbar_height =
                super::toolbar::height(ui, items, ui.available_width(), &tool_state);
            TopBottomPanel::top("folder_sync_toolbar")
                .exact_height(toolbar_height)
                .show_separator_line(false)
                .frame(Frame::none())
                .show_inside(ui, |ui| {
                    tool_event = super::toolbar::show(
                        ui,
                        icons,
                        items,
                        self.active_left,
                        &tool_state,
                        &mut |_, _, _| {},
                    );
                });
        }
        match tool_event {
            Some(ToolEvent::Click("SessionHome")) => action = FolderSyncAction::Home,
            Some(ToolEvent::Click("InsigIgnored")) => {
                self.options.compare_content = !self.options.compare_content;
                if self.has_pair() {
                    self.start_scan();
                }
            }
            Some(ToolEvent::Click("ExpandAll")) => self.collapsed.clear(),
            Some(ToolEvent::Click("CollapseAll")) => {
                self.collapsed = self
                    .entries
                    .iter()
                    .filter(|e| e.is_dir)
                    .map(|e| e.relative.to_string_lossy().to_lowercase())
                    .collect();
            }
            Some(ToolEvent::Click("SelectAll")) => {
                self.selected = self.visible_indices().into_iter().collect();
            }
            Some(ToolEvent::Click("Refresh")) => self.start_scan(),
            Some(ToolEvent::Click("Stop")) => self.stop_pending(),
            Some(ToolEvent::Click("SuppressFilters")) => {
                self.options.suppress_filters = !self.options.suppress_filters;
                if self.has_pair() {
                    self.start_scan();
                }
            }
            _ => {}
        }

        // Keyboard shortcuts
        if ctx.input(|i| i.key_pressed(Key::F5)) && self.has_pair() && !pending {
            self.start_scan();
        }

        // 4. BC5 Folder Sync Control Header (Left Path | Preset + Sync Now | Right Path)
        TopBottomPanel::top("folder_sync_controls")
            .show_separator_line(true)
            .frame(
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(8.0, 6.0)),
            )
            .show_inside(ui, |ui| {
                ui.visuals_mut().widgets.inactive.bg_stroke =
                    Stroke::new(1.0_f32, Color32::from_gray(155));
                ui.visuals_mut().extreme_bg_color = Color32::WHITE;
                let total_w = ui.available_width();
                let center_w = 220.0_f32.min(total_w * 0.34).max(160.0);
                let side_w = ((total_w - center_w - 16.0) / 2.0).max(120.0);

                ui.horizontal(|ui| {
                    // Left Folder Path + Browse
                    ui.allocate_ui_with_layout(
                        vec2(side_w, 24.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            let history =
                                super::session_picker::paths(ctx, crate::history::SessionKind::FolderSync);
                            let field_w = (side_w - 30.0).max(60.0);
                            let out = super::history_combo::show_active(
                                ui,
                                "sync_left_path",
                                &mut self.left_path,
                                &history,
                                field_w,
                                &crate::i18n::tr("sync.left_folder"),
                                self.active_left,
                            );
                            if out.response.clicked() || out.response.gained_focus() {
                                self.active_left = true;
                            }
                            if let Some(picked) = out.selected {
                                self.left_path = picked;
                                self.start_scan();
                            } else if out.submitted {
                                self.start_scan();
                            }
                            if icon_btn(ui, icons, "imgBrowse", "sync.select_left_folder").clicked() {
                                if let Some(dir) = dialogs.folder(true, Path::new(&self.left_path)) {
                                    self.left_path = dir.to_string_lossy().into_owned();
                                    self.start_scan();
                                }
                            }
                        },
                    );

                    // Center Preset Combo + Swap
                    ui.allocate_ui_with_layout(
                        vec2(center_w, 24.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            if let Some(tex) = icons.get_icon(ctx, self.options.preset.icon()) {
                                ui.image((tex.id(), vec2(16.0, 16.0)));
                            }
                            ComboBox::from_id_salt("sync_preset_combo")
                                .width((center_w - 50.0).max(110.0))
                                .selected_text(crate::i18n::tr(self.options.preset.label()))
                                .show_ui(ui, |ui| {
                                    for preset in SyncPreset::ALL {
                                        if ui
                                            .selectable_label(
                                                self.options.preset == preset,
                                                crate::i18n::tr(preset.label()),
                                            )
                                            .clicked()
                                        {
                                            self.options.preset = preset;
                                            recompute_actions(&mut self.entries, &self.options);
                                        }
                                    }
                                });
                            if icon_btn(ui, icons, "imgSwap", "toolbar.swap_sides").clicked() {
                                self.swap_sides();
                            }
                        },
                    );

                    // Right Folder Path + Browse
                    ui.allocate_ui_with_layout(
                        vec2(side_w, 24.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            let history =
                                super::session_picker::paths(ctx, crate::history::SessionKind::FolderSync);
                            let field_w = (side_w - 30.0).max(60.0);
                            let out = super::history_combo::show_active(
                                ui,
                                "sync_right_path",
                                &mut self.right_path,
                                &history,
                                field_w,
                                &crate::i18n::tr("sync.right_folder"),
                                !self.active_left,
                            );
                            if out.response.clicked() || out.response.gained_focus() {
                                self.active_left = false;
                            }
                            if let Some(picked) = out.selected {
                                self.right_path = picked;
                                self.start_scan();
                            } else if out.submitted {
                                self.start_scan();
                            }
                            if icon_btn(ui, icons, "imgBrowse", "sync.select_right_folder").clicked() {
                                if let Some(dir) = dialogs.folder(false, Path::new(&self.right_path)) {
                                    self.right_path = dir.to_string_lossy().into_owned();
                                    self.start_scan();
                                }
                            }
                        },
                    );
                });

                ui.add_space(4.0);
                // Second strip: Description + Filter + Action counters + Sync Now button
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(crate::i18n::tr(self.options.preset.description()))
                            .size(11.0)
                            .color(Color32::from_gray(75)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let sync_btn = ui.add_enabled(
                            can_sync,
                            Button::new(
                                RichText::new(format!(
                                    "{} ({})",
                                    crate::i18n::tr("sync.now"),
                                    summary.total_operations()
                                ))
                                .strong(),
                            )
                            .fill(if can_sync {
                                Color32::from_rgb(220, 238, 255)
                            } else {
                                Color32::from_gray(232)
                            })
                            .stroke(Stroke::new(
                                1.0_f32,
                                if can_sync {
                                    Color32::from_rgb(0, 120, 212)
                                } else {
                                    Color32::from_gray(185)
                                },
                            ))
                            .min_size(vec2(140.0, 24.0)),
                        );
                        if sync_btn.clicked() {
                            self.start_execute(dialogs);
                        }

                        ui.add_space(6.0);
                        ui.checkbox(
                            &mut self.show_only_actions,
                            crate::i18n::tr("sync.actions_only"),
                        );
                        ui.add_space(8.0);
                        let filter_edit = ui.add(
                            TextEdit::singleline(&mut self.options.name_filter)
                                .desired_width(92.0)
                                .hint_text("*.*"),
                        );
                        if filter_edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            self.start_scan();
                        }
                        ui.label(crate::i18n::tr("common.filters_colon"));
                    });
                });
            });

        // 5. Status Bar (bottom-most)
        TopBottomPanel::bottom("folder_sync_status")
            .exact_height(22.0)
            .frame(
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(8.0, 3.0)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if pending {
                        let msg = if self.running_exec {
                            crate::i18n::tr("sync.syncing_files")
                        } else {
                            crate::i18n::tr("merge.comparing_folders")
                        };
                        ui.label(RichText::new(msg).color(Color32::from_rgb(0, 102, 180)));
                    } else if let Some(err) = &self.last_error {
                        ui.label(RichText::new(err).color(Color32::DARK_RED));
                    } else {
                        ui.label(format!(
                            "→ {}  ·  ← {}  ·  ✕ {}  ·  ! {}  ·  {} {}",
                            summary.copy_right,
                            summary.copy_left,
                            summary.delete_left + summary.delete_right,
                            summary.conflicts,
                            summary.unchanged,
                            crate::i18n::tr("merge.same"),
                        ));
                    }
                });
                super::chrome::status_bar_grip(ui.painter(), ui.max_rect());
            });

        // 6. Optional Log Panel
        if self.show_log {
            TopBottomPanel::bottom("folder_sync_log")
                .resizable(true)
                .default_height(84.0)
                .min_height(46.0)
                .max_height(220.0)
                .frame(
                    Frame::none()
                        .fill(Color32::from_rgb(227, 231, 237))
                        .stroke(Stroke::new(1.0_f32, Color32::from_gray(172)))
                        .inner_margin(Margin::symmetric(6.0, 3.0)),
                )
                .show_inside(ui, |ui| {
                    ScrollArea::vertical()
                        .id_salt("folder_sync_log_scroll")
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.log_lines.is_empty() {
                                ui.label(
                                    RichText::new(crate::i18n::tr(
                                        "sync.select_left_and_right_folders_to_preview_the",
                                    ))
                                    .size(11.0)
                                    .color(Color32::from_gray(110)),
                                );
                            } else {
                                for line in &self.log_lines {
                                    ui.label(
                                        RichText::new(line)
                                            .font(FontId::new(11.5, FontFamily::Name("Table Mono".into())))
                                            .color(Color32::from_gray(45)),
                                    );
                                }
                            }
                        });
                });
        }

        // 7. Central Preview Tree (Left | Center Action Column | Right)
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::WHITE))
            .show_inside(ui, |ui| {
                let rect = ui.max_rect();
                let center_col_w = 40.0;
                let side_w = ((rect.width() - center_col_w) / 2.0).max(120.0);

                // Column Headers
                let header_rect = Rect::from_min_size(rect.min, vec2(rect.width(), 22.0));
                super::chrome::gradient(
                    ui,
                    header_rect,
                    Color32::from_gray(248),
                    Color32::from_gray(232),
                );
                ui.painter().hline(
                    header_rect.x_range(),
                    header_rect.bottom() - 0.5,
                    Stroke::new(1.0_f32, Color32::from_gray(185)),
                );
                draw_side_header(
                    ui,
                    Rect::from_min_size(header_rect.min, vec2(side_w, 22.0)),
                );
                // Center action header
                let ch = Rect::from_min_size(
                    pos2(header_rect.left() + side_w, header_rect.top()),
                    vec2(center_col_w, 22.0),
                );
                ui.painter().vline(
                    ch.left(),
                    rect.y_range(),
                    Stroke::new(1.0_f32, Color32::from_gray(205)),
                );
                ui.painter().vline(
                    ch.right(),
                    rect.y_range(),
                    Stroke::new(1.0_f32, Color32::from_gray(205)),
                );
                ui.painter().text(
                    ch.center(),
                    Align2::CENTER_CENTER,
                    "⇄",
                    FontId::proportional(12.0),
                    Color32::from_gray(80),
                );
                draw_side_header(
                    ui,
                    Rect::from_min_size(
                        pos2(header_rect.left() + side_w + center_col_w, header_rect.top()),
                        vec2(side_w, 22.0),
                    ),
                );

                // Rows
                let body_rect = Rect::from_min_max(
                    pos2(rect.left(), header_rect.bottom()),
                    rect.max,
                );
                let visible = self.visible_indices();
                let mut clicked_compare: Option<(PathBuf, PathBuf)> = None;
                let mut cycle_idx: Option<usize> = None;
                let mut toggle_dir: Option<String> = None;
                let mut set_action: Option<(usize, SyncAction)> = None;

                let mut child_ui = ui.new_child(UiBuilder::new().max_rect(body_rect));
                ScrollArea::vertical()
                    .id_salt("folder_sync_rows")
                    .auto_shrink([false, false])
                    .show_rows(&mut child_ui, ROW_HEIGHT, visible.len(), |ui, range| {
                        for vis_pos in range {
                            let idx = visible[vis_pos];
                            let entry = &self.entries[idx];
                            let (row_rect, row_resp) = ui.allocate_exact_size(
                                vec2(rect.width(), ROW_HEIGHT),
                                Sense::click(),
                            );
                            let selected = self.selected.contains(&idx);
                            let bg = if selected {
                                Color32::from_rgb(204, 232, 255)
                            } else {
                                match entry.action {
                                    SyncAction::CopyRight => COPY_RIGHT_BG,
                                    SyncAction::CopyLeft => COPY_LEFT_BG,
                                    SyncAction::DeleteLeft | SyncAction::DeleteRight => DELETE_BG,
                                    SyncAction::Conflict => CONFLICT_BG,
                                    _ if vis_pos % 2 == 1 => Color32::from_gray(249),
                                    _ => Color32::WHITE,
                                }
                            };
                            ui.painter().rect_filled(row_rect, 0.0, bg);

                            let left_rect =
                                Rect::from_min_size(row_rect.min, vec2(side_w, ROW_HEIGHT));
                            let mid_rect = Rect::from_min_size(
                                pos2(row_rect.left() + side_w, row_rect.top()),
                                vec2(center_col_w, ROW_HEIGHT),
                            );
                            let right_rect = Rect::from_min_size(
                                pos2(row_rect.left() + side_w + center_col_w, row_rect.top()),
                                vec2(side_w, ROW_HEIGHT),
                            );

                            let name = entry
                                .relative
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let indent = entry.depth as f32 * 16.0;
                            let collapsed = self
                                .collapsed
                                .contains(&entry.relative.to_string_lossy().to_lowercase());

                            let left_color = sync_text_color(entry, true);
                            let right_color = sync_text_color(entry, false);
                            let strike_left = entry.action == SyncAction::DeleteLeft;
                            let strike_right = entry.action == SyncAction::DeleteRight;

                            let folder_tint = match entry.status {
                                SyncEntryStatus::LeftOnly | SyncEntryStatus::RightOnly => {
                                    ORPHAN_COLOR
                                }
                                SyncEntryStatus::Same => Color32::from_rgb(235, 198, 92),
                                _ => Color32::from_rgb(218, 60, 60),
                            };
                            if let Some(lm) = &entry.left {
                                draw_side_cell(
                                    ui,
                                    icons,
                                    left_rect,
                                    indent,
                                    &name,
                                    lm.is_dir,
                                    collapsed,
                                    lm.size,
                                    lm.modified_secs,
                                    left_color,
                                    folder_tint,
                                    strike_left,
                                );
                            }
                            if let Some(rm) = &entry.right {
                                draw_side_cell(
                                    ui,
                                    icons,
                                    right_rect,
                                    indent,
                                    &name,
                                    rm.is_dir,
                                    collapsed,
                                    rm.size,
                                    rm.modified_secs,
                                    right_color,
                                    folder_tint,
                                    strike_right,
                                );
                            }

                            // Center sync action icon (clickable to override action!)
                            if let Some(tex) = icons.get_icon(ctx, entry.action.icon()) {
                                let icon_r =
                                    Rect::from_center_size(mid_rect.center(), vec2(16.0, 16.0));
                                ui.painter().image(
                                    tex.id(),
                                    icon_r,
                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }
                            let mid_tip = match &entry.error {
                                Some(err) => format!("{}: {err}", crate::i18n::tr("merge.error")),
                                None => format!(
                                    "{} ({})",
                                    crate::i18n::tr(entry.action.label()),
                                    crate::i18n::tr("sync.click_to_change_action")
                                ),
                            };
                            let mid_resp = ui
                                .interact(
                                    mid_rect,
                                    ui.id().with(("sync_mid", idx)),
                                    Sense::click(),
                                )
                                .on_hover_text(mid_tip);
                            if mid_resp.clicked() {
                                cycle_idx = Some(idx);
                            }

                            if row_resp.clicked() {
                                if let Some(pos) = row_resp.interact_pointer_pos() {
                                    self.active_left = pos.x < mid_rect.center().x;
                                    if entry.is_dir
                                        && ((pos.x >= left_rect.left() + indent
                                            && pos.x <= left_rect.left() + indent + 16.0)
                                            || (pos.x >= right_rect.left() + indent
                                                && pos.x <= right_rect.left() + indent + 16.0))
                                    {
                                        toggle_dir = Some(
                                            entry.relative.to_string_lossy().to_lowercase(),
                                        );
                                    }
                                }
                                if !ui.input(|i| i.modifiers.ctrl) {
                                    self.selected.clear();
                                }
                                self.selected.insert(idx);
                            }
                            if row_resp.double_clicked() {
                                if entry.is_dir {
                                    toggle_dir =
                                        Some(entry.relative.to_string_lossy().to_lowercase());
                                } else if entry.left.is_some() && entry.right.is_some() {
                                    clicked_compare = Some((
                                        PathBuf::from(self.left_path.trim()).join(&entry.relative),
                                        PathBuf::from(self.right_path.trim()).join(&entry.relative),
                                    ));
                                }
                            }
                            if row_resp.secondary_clicked() {
                                if let Some(pos) = row_resp.interact_pointer_pos() {
                                    self.active_left = pos.x < mid_rect.center().x;
                                }
                                if !self.selected.contains(&idx) {
                                    self.selected.clear();
                                    self.selected.insert(idx);
                                }
                            }
                            let mut open_with_kind: Option<SessionKind> = None;
                            let mut open_compare_modal = false;
                            let mut copy_paths_now = false;
                            let mut open_subfolders_cmd: Option<bool> = None;
                            let mut refresh_now = false;
                            let mut explore_cmd: Option<bool> = None;
                            let mut assoc_app_now = false;
                            let mut bulk_action: Option<SyncAction> = None;
                            let row_key = entry.relative.to_string_lossy().to_lowercase();
                            let row_collapsed = self.collapsed.contains(&row_key);
                            row_resp.context_menu(|ui| {
                                ui.set_min_width(215.0);
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.spacing_mut().button_padding = vec2(4.0, 2.0);
                                ui.spacing_mut().interact_size.y = 20.0;
                                if entry.is_dir {
                                    let lbl = if row_collapsed {
                                        "folder.ctx.open_folder"
                                    } else {
                                        "folder.ctx.close_folder"
                                    };
                                    if super::chrome::menu_item_default(ui, icons, lbl, true)
                                        .clicked()
                                    {
                                        toggle_dir = Some(row_key.clone());
                                        ui.close_menu();
                                    }
                                    if super::chrome::menu_item(
                                        ui,
                                        icons,
                                        "folder.ctx.open_subfolders",
                                        true,
                                    )
                                    .clicked()
                                    {
                                        open_subfolders_cmd = Some(true);
                                        ui.close_menu();
                                    }
                                    if super::chrome::menu_item(
                                        ui,
                                        icons,
                                        "folder.ctx.close_subfolders",
                                        true,
                                    )
                                    .clicked()
                                    {
                                        open_subfolders_cmd = Some(false);
                                        ui.close_menu();
                                    }
                                } else if super::chrome::menu_item_default(
                                    ui,
                                    icons,
                                    "folder.ctx.open_file",
                                    entry.left.is_some() && entry.right.is_some(),
                                )
                                .clicked()
                                {
                                    clicked_compare = Some((
                                        PathBuf::from(self.left_path.trim()).join(&entry.relative),
                                        PathBuf::from(self.right_path.trim()).join(&entry.relative),
                                    ));
                                    ui.close_menu();
                                }
                                super::chrome::submenu_item(
                                    ui,
                                    icons,
                                    "menu.actions.open_with",
                                    |ui, icons| {
                                        for (lbl, kind) in [
                                            ("session.text_compare", SessionKind::Text),
                                            ("session.table_compare", SessionKind::Table),
                                            ("session.hex_compare", SessionKind::Hex),
                                            ("session.picture_compare", SessionKind::Picture),
                                            ("session.version_compare", SessionKind::Version),
                                            ("session.media_compare", SessionKind::Media),
                                            ("session.registry_compare", SessionKind::Registry),
                                            ("session.text_edit", SessionKind::TextEdit),
                                        ] {
                                            if super::chrome::menu_item(
                                                ui,
                                                icons,
                                                lbl,
                                                !entry.is_dir,
                                            )
                                            .clicked()
                                            {
                                                open_with_kind = Some(kind);
                                                ui.close_menu();
                                            }
                                        }
                                        ui.separator();
                                        if super::chrome::menu_item(
                                            ui,
                                            icons,
                                            "folder.ctx.associated_app",
                                            true,
                                        )
                                        .clicked()
                                        {
                                            assoc_app_now = true;
                                            ui.close_menu();
                                        }
                                    },
                                );
                                ui.separator();
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "folder.ctx.compare_contents_ellipsis",
                                    !pending,
                                )
                                .clicked()
                                {
                                    open_compare_modal = true;
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "menu.actions.copy_right_short",
                                    entry.left.is_some() || self.selected.len() > 1,
                                )
                                .clicked()
                                {
                                    bulk_action = Some(SyncAction::CopyRight);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "menu.actions.copy_left_short",
                                    entry.right.is_some() || self.selected.len() > 1,
                                )
                                .clicked()
                                {
                                    bulk_action = Some(SyncAction::CopyLeft);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "sync.delete_left",
                                    (entry.left.is_some() && entry.right.is_none())
                                        || self.selected.len() > 1,
                                )
                                .clicked()
                                {
                                    bulk_action = Some(SyncAction::DeleteLeft);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "sync.delete_right",
                                    (entry.right.is_some() && entry.left.is_none())
                                        || self.selected.len() > 1,
                                )
                                .clicked()
                                {
                                    bulk_action = Some(SyncAction::DeleteRight);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(ui, icons, "common.ignore", true)
                                    .clicked()
                                {
                                    bulk_action = Some(SyncAction::Skip);
                                    ui.close_menu();
                                }
                                ui.separator();
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "folder.ctx.copy_filename",
                                    true,
                                )
                                .clicked()
                                {
                                    copy_paths_now = true;
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "menu.actions.compare_refresh  F5",
                                    !pending,
                                )
                                .clicked()
                                {
                                    refresh_now = true;
                                    ui.close_menu();
                                }
                                ui.separator();
                                super::chrome::submenu_item(
                                    ui,
                                    icons,
                                    "table.explorer",
                                    |ui, icons| {
                                        if super::chrome::menu_item(
                                            ui,
                                            icons,
                                            "version.select_in_explorer",
                                            true,
                                        )
                                        .clicked()
                                        {
                                            explore_cmd = Some(true);
                                            ui.close_menu();
                                        }
                                        if super::chrome::menu_item(
                                            ui,
                                            icons,
                                            "version.open_folder",
                                            true,
                                        )
                                        .clicked()
                                        {
                                            explore_cmd = Some(false);
                                            ui.close_menu();
                                        }
                                    },
                                );
                            });
                            if let Some(act) = bulk_action {
                                set_action = Some((idx, act));
                            }
                            if let Some(open) = open_subfolders_cmd {
                                let base_rel = entry.relative.clone();
                                for e in &self.entries {
                                    if e.is_dir && e.relative.starts_with(&base_rel) {
                                        let k = e.relative.to_string_lossy().to_lowercase();
                                        if open {
                                            self.collapsed.remove(&k);
                                        } else {
                                            self.collapsed.insert(k);
                                        }
                                    }
                                }
                            }
                            if let Some(kind) = open_with_kind {
                                let lp = PathBuf::from(self.left_path.trim()).join(&entry.relative);
                                let rp = PathBuf::from(self.right_path.trim()).join(&entry.relative);
                                if kind == SessionKind::TextEdit {
                                    let p = if self.active_left && lp.is_file() { lp } else { rp };
                                    if p.is_file() {
                                        super::session_menu::request(
                                            ctx,
                                            super::session_menu::Request::EditText(p),
                                        );
                                    }
                                } else {
                                    super::session_menu::request(
                                        ctx,
                                        super::session_menu::Request::Recent(RecentSession::new(
                                            kind, lp, rp,
                                        )),
                                    );
                                }
                            }
                            if open_compare_modal {
                                self.compare_modal = Some(FolderOpModal::CompareContents {
                                    stats: self.sync_selection_stats(),
                                    mode: self.content_mode,
                                    show_results: true,
                                    dont_show_again: false,
                                    include_hidden: false,
                                });
                            }
                            if copy_paths_now {
                                let root = if self.active_left {
                                    PathBuf::from(self.left_path.trim())
                                } else {
                                    PathBuf::from(self.right_path.trim())
                                };
                                let lines: Vec<String> = self
                                    .selected
                                    .iter()
                                    .filter_map(|&i| self.entries.get(i))
                                    .map(|e| root.join(&e.relative).display().to_string())
                                    .collect();
                                ctx.copy_text(lines.join("\n"));
                            }
                            if assoc_app_now {
                                let root = if self.active_left {
                                    PathBuf::from(self.left_path.trim())
                                } else {
                                    PathBuf::from(self.right_path.trim())
                                };
                                let p = root.join(&entry.relative);
                                if let Err(err) = dialogs.open_path(&p, false) {
                                    dialogs.error(&err);
                                }
                            }
                            if let Some(select) = explore_cmd {
                                let root = if self.active_left {
                                    PathBuf::from(self.left_path.trim())
                                } else {
                                    PathBuf::from(self.right_path.trim())
                                };
                                let p = root.join(&entry.relative);
                                let target = if select || p.is_dir() {
                                    p
                                } else {
                                    p.parent().unwrap_or(&root).to_path_buf()
                                };
                                if let Err(err) = dialogs.open_path(&target, select) {
                                    dialogs.error(&err);
                                }
                            }
                            if refresh_now {
                                self.start_scan();
                            }
                        }
                    });

                if let Some(idx) = cycle_idx {
                    if let Some(e) = self.entries.get_mut(idx) {
                        cycle_entry_action(e);
                    }
                }
                if let Some((idx, act)) = set_action {
                    let targets: Vec<usize> = if self.selected.contains(&idx) && self.selected.len() > 1
                    {
                        self.selected.iter().copied().collect()
                    } else {
                        vec![idx]
                    };
                    for i in targets {
                        if let Some(e) = self.entries.get_mut(i) {
                            let valid = match act {
                                SyncAction::CopyRight => e.left.is_some(),
                                SyncAction::CopyLeft => e.right.is_some(),
                                SyncAction::DeleteLeft => e.left.is_some() && e.right.is_none(),
                                SyncAction::DeleteRight => e.right.is_some() && e.left.is_none(),
                                SyncAction::Skip | SyncAction::None | SyncAction::Conflict => true,
                            };
                            if valid {
                                e.action = act;
                            }
                        }
                    }
                }
                if let Some(key) = toggle_dir {
                    if !self.collapsed.remove(&key) {
                        self.collapsed.insert(key);
                    }
                }
                if let Some((l, r)) = clicked_compare {
                    action = FolderSyncAction::CompareFiles(l, r);
                }
            });

        if let Some(mut modal) = self.compare_modal.take() {
            match modal.show(ctx, icons, dialogs) {
                Ok(Some(FolderOpConfirmed::CompareContents {
                    mode, show_results, ..
                })) => self.run_sync_content_compare(mode, show_results),
                Ok(Some(FolderOpConfirmed::OpenResultRow {
                    left_rel,
                    right_rel,
                    status,
                })) => {
                    let lr = PathBuf::from(self.left_path.trim());
                    let rr = PathBuf::from(self.right_path.trim());
                    match status {
                        CompareRowStatus::LeftOrphan => {
                            super::session_menu::request(
                                ctx,
                                super::session_menu::Request::EditText(lr.join(&left_rel)),
                            );
                        }
                        CompareRowStatus::RightOrphan => {
                            super::session_menu::request(
                                ctx,
                                super::session_menu::Request::EditText(rr.join(&right_rel)),
                            );
                        }
                        _ => {
                            action = FolderSyncAction::CompareFiles(
                                lr.join(&left_rel),
                                rr.join(&right_rel),
                            );
                        }
                    }
                    self.compare_modal = Some(modal);
                }
                Ok(Some(_)) => {}
                Ok(None) => self.compare_modal = Some(modal),
                Err(()) => {}
            }
        }

        action
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

fn sync_text_color(entry: &SyncEntry, left: bool) -> Color32 {
    if entry.is_dir {
        return Color32::BLACK;
    }
    match entry.status {
        SyncEntryStatus::LeftOnly | SyncEntryStatus::RightOnly => ORPHAN_COLOR,
        SyncEntryStatus::LeftNewer => {
            if left {
                Color32::RED
            } else {
                Color32::from_gray(120)
            }
        }
        SyncEntryStatus::RightNewer => {
            if left {
                Color32::from_gray(120)
            } else {
                Color32::RED
            }
        }
        SyncEntryStatus::DifferentSameTime
        | SyncEntryStatus::TypeMismatch
        | SyncEntryStatus::Error => Color32::RED,
        SyncEntryStatus::Same => Color32::BLACK,
    }
}

fn draw_side_header(ui: &Ui, rect: Rect) {
    let date_x = (rect.right() - 108.0).max(rect.left() + 60.0);
    let size_x = (date_x - 74.0).max(rect.left() + 40.0);
    let font = FontId::proportional(11.5);
    ui.painter().text(
        pos2(rect.left() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        crate::i18n::tr("common.name"),
        font.clone(),
        Color32::BLACK,
    );
    ui.painter().text(
        pos2(date_x - 6.0, rect.center().y),
        Align2::RIGHT_CENTER,
        crate::i18n::tr("common.size"),
        font.clone(),
        Color32::BLACK,
    );
    ui.painter().text(
        pos2(date_x + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        crate::i18n::tr("status.modified"),
        font,
        Color32::BLACK,
    );
    let _ = size_x;
}

#[allow(clippy::too_many_arguments)]
fn draw_side_cell(
    ui: &Ui,
    icons: &mut IconManager,
    rect: Rect,
    indent: f32,
    name: &str,
    is_dir: bool,
    collapsed: bool,
    size: u64,
    modified_secs: Option<u64>,
    color: Color32,
    folder_tint: Color32,
    strikethrough: bool,
) {
    let date_x = (rect.right() - 108.0).max(rect.left() + 60.0);
    let size_x = (date_x - 74.0).max(rect.left() + 40.0);
    let name_clip = Rect::from_min_max(rect.min, pos2(size_x - 4.0, rect.bottom()));
    let painter = ui.painter().with_clip_rect(name_clip);

    let mut x = rect.left() + 4.0 + indent;
    if is_dir {
        painter.text(
            pos2(x + 4.0, rect.center().y),
            Align2::CENTER_CENTER,
            if collapsed { "▸" } else { "▾" },
            FontId::proportional(10.0),
            Color32::from_gray(90),
        );
    }
    x += 12.0;
    if is_dir {
        let layers = if collapsed {
            ["imgFolderTop0", "imgFolderBottom0"]
        } else {
            ["imgFolderTop1", "imgFolderBottom1"]
        };
        for layer in layers {
            if let Some(tex) = icons.folder_model(ui.ctx(), layer, folder_tint) {
                painter.image(
                    tex.id(),
                    Rect::from_center_size(pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0)),
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
    } else {
        draw_file_glyph(&painter, pos2(x + 2.0, rect.center().y - 6.5), color);
    }
    x += 20.0;
    let galley = painter.layout_no_wrap(name.to_owned(), FontId::proportional(12.0), color);
    let text_pos = pos2(x, rect.center().y - galley.size().y / 2.0);
    let text_w = galley.size().x;
    painter.galley(text_pos, galley, color);
    if strikethrough {
        painter.hline(
            x..=(x + text_w),
            rect.center().y,
            Stroke::new(1.0_f32, Color32::from_rgb(200, 30, 30)),
        );
    }

    if !is_dir {
        ui.painter().text(
            pos2(date_x - 6.0, rect.center().y),
            Align2::RIGHT_CENTER,
            format_size(size),
            FontId::proportional(11.5),
            Color32::from_gray(60),
        );
    }
    if modified_secs.is_some() {
        ui.painter().text(
            pos2(date_x + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            format_time(modified_secs),
            FontId::proportional(11.5),
            Color32::from_gray(60),
        );
    }
}

fn draw_file_glyph(painter: &Painter, min: Pos2, text_color: Color32) {
    let doc = Rect::from_min_size(min, vec2(11.0, 13.0));
    let outline = if text_color == Color32::BLACK {
        Color32::from_gray(110)
    } else {
        text_color
    };
    let fold = 3.5;
    let pts = vec![
        doc.left_top(),
        pos2(doc.right() - fold, doc.top()),
        pos2(doc.right(), doc.top() + fold),
        doc.right_bottom(),
        doc.left_bottom(),
    ];
    painter.add(Shape::convex_polygon(
        pts,
        Color32::WHITE,
        Stroke::new(1.0_f32, outline),
    ));
    painter.line_segment(
        [
            pos2(doc.right() - fold, doc.top()),
            pos2(doc.right() - fold, doc.top() + fold),
        ],
        Stroke::new(1.0_f32, outline),
    );
    painter.line_segment(
        [
            pos2(doc.right() - fold, doc.top() + fold),
            pos2(doc.right(), doc.top() + fold),
        ],
        Stroke::new(1.0_f32, outline),
    );
    let line_col = if text_color == Color32::BLACK {
        Color32::from_gray(155)
    } else {
        text_color
    };
    for (dy, rw) in [(6.0, 6.0), (8.5, 6.5), (11.0, 4.5)] {
        painter.line_segment(
            [
                pos2(doc.left() + 2.0, doc.top() + dy),
                pos2(doc.left() + 2.0 + rw, doc.top() + dy),
            ],
            Stroke::new(1.0_f32, line_col),
        );
    }
}

fn format_size(bytes: u64) -> String {
    let digits = bytes.to_string();
    let sep = if crate::i18n::language().starts_with("pt") {
        '.'
    } else {
        ','
    };
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}
