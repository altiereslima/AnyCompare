//! AnyCompare 3-Way Folder Merge session UI (`Combinar Pastas`).

use super::dialogs::{DialogField, DialogHost};
use super::folder_ops::{
    self, CompareResultRow, CompareResultsState, CompareRowStatus, ContentCompareMode,
    FileContentVerdict, FolderOpConfirmed, FolderOpModal, SelectionStats,
};
use super::toolbar::{ToolEvent, ToolState};
use crate::folder_merge::{
    build_folder_merge_plan, check_merge_output_safety, execute_folder_merge,
    output_matches_input_side, summarize, FolderMergeAction, FolderMergeEntry,
    FolderMergeExecReport, FolderMergeOptions, FolderMergeStatus,
};
use crate::folder_source::EntryMeta;
use crate::folder_sync::{format_time, timestamp_now_hms};
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
const LEFT_CHANGE_BG: Color32 = Color32::from_rgb(241, 232, 255);
const LEFT_CHANGE_TEXT: Color32 = Color32::from_rgb(106, 27, 154);
const RIGHT_CHANGE_BG: Color32 = Color32::from_rgb(227, 245, 252);
const RIGHT_CHANGE_TEXT: Color32 = Color32::from_rgb(0, 105, 148);
const BOTH_SAME_BG: Color32 = Color32::from_rgb(232, 248, 236);
const BOTH_SAME_TEXT: Color32 = Color32::from_rgb(24, 118, 48);
const CONFLICT_BG: Color32 = Color32::from_rgb(255, 227, 227);
const CONFLICT_TEXT: Color32 = Color32::from_rgb(205, 0, 0);

pub enum FolderMergeUiAction {
    None,
    Home,
    OpenTextMerge {
        left: PathBuf,
        center: Option<PathBuf>,
        right: PathBuf,
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayFilter {
    All,
    Changed,
    Conflicts,
    LeftChanges,
    RightChanges,
    Mergeable,
    Unchanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirMode {
    Structure,
    Flatten,
    FilesOnly,
    ShowAll,
}

enum MergeJobResult {
    Plan(Result<Vec<FolderMergeEntry>, String>),
    Executed(Result<FolderMergeExecReport, String>),
}

pub struct FolderMergeScreen {
    pub left_path: String,
    pub center_path: String,
    pub right_path: String,
    pub output_path: String,
    pub options: FolderMergeOptions,
    pub entries: Vec<FolderMergeEntry>,
    pub collapsed: BTreeSet<String>,
    pub selected: BTreeSet<usize>,
    filter: DisplayFilter,
    dir_mode: DirMode,
    include_unchanged_in_output: bool,
    show_log: bool,
    show_toolbar: bool,
    show_center_pane: bool,
    ignore_same_changes: bool,
    log_lines: Vec<String>,
    pending_rx: Option<mpsc::Receiver<MergeJobResult>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    running_exec: bool,
    last_error: Option<String>,
    content_mode: ContentCompareMode,
    compare_modal: Option<FolderOpModal>,
}

impl Default for FolderMergeScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl FolderMergeScreen {
    pub fn new() -> Self {
        Self {
            left_path: String::new(),
            center_path: String::new(),
            right_path: String::new(),
            output_path: String::new(),
            options: FolderMergeOptions::default(),
            entries: Vec::new(),
            collapsed: BTreeSet::new(),
            selected: BTreeSet::new(),
            filter: DisplayFilter::All,
            dir_mode: DirMode::Structure,
            include_unchanged_in_output: true,
            show_log: true,
            show_toolbar: true,
            show_center_pane: true,
            ignore_same_changes: false,
            log_lines: Vec::new(),
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

    fn merge_selection_stats(&self) -> SelectionStats {
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

    fn run_merge_content_compare(&mut self, mode: ContentCompareMode, show_results: bool) {
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
            let e = &self.entries[idx];
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
                    };
                    let r_meta = EntryMeta {
                        is_dir: false,
                        size: rm.size,
                        modified: None,
                        crc32: None,
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

    pub fn load_folders(
        &mut self,
        left: &Path,
        center: Option<&Path>,
        right: &Path,
        output: Option<&Path>,
    ) {
        self.left_path = left.to_string_lossy().into_owned();
        self.center_path = center
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.right_path = right.to_string_lossy().into_owned();
        self.output_path = output
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
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
        let left = PathBuf::from(self.left_path.trim());
        let center = (!self.center_path.trim().is_empty())
            .then(|| PathBuf::from(self.center_path.trim()));
        let right = PathBuf::from(self.right_path.trim());
        let cancel = AtomicBool::new(false);
        let plan = build_folder_merge_plan(
            &left,
            center.as_deref(),
            &right,
            &self.options,
            &cancel,
        )?;
        self.entries = plan;
        self.selected.clear();
        self.last_error = None;
        let s = summarize(&self.entries);
        self.log_lines.push(format!(
            "{} {} ({} {}, {} {}, {} {})",
            timestamp_now_hms(),
            crate::i18n::tr("merge.merge_preview_updated"),
            s.mergeable,
            crate::i18n::tr("merge.mergeable_mesclaveis"),
            s.conflicts,
            crate::i18n::tr("merge.conflicts"),
            s.unchanged,
            crate::i18n::tr("merge.same"),
        ));
        Ok(())
    }

    pub fn start_scan(&mut self) {
        if !self.has_pair() {
            return;
        }
        self.stop_pending();
        let left = PathBuf::from(self.left_path.trim());
        let center = (!self.center_path.trim().is_empty())
            .then(|| PathBuf::from(self.center_path.trim()));
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
            let res = build_folder_merge_plan(&left, center.as_deref(), &right, &opts, &cancel_bg);
            let _ = tx.send(MergeJobResult::Plan(res));
        });
    }

    fn start_merge_execute(&mut self, only_selected: bool, dialogs: &mut dyn DialogHost) {
        if !self.has_pair() || self.is_busy() {
            return;
        }
        if self.output_path.trim().is_empty() {
            if let Some(dir) = dialogs.folder(false, Path::new(&self.left_path)) {
                self.output_path = dir.to_string_lossy().into_owned();
            } else {
                return;
            }
        }
        let left = PathBuf::from(self.left_path.trim());
        let center = (!self.center_path.trim().is_empty())
            .then(|| PathBuf::from(self.center_path.trim()));
        let right = PathBuf::from(self.right_path.trim());
        let output = PathBuf::from(self.output_path.trim());
        if let Err(err) = check_merge_output_safety(&left, center.as_deref(), &right, &output) {
            dialogs.error(&err);
            return;
        }
        let target_entries: Vec<FolderMergeEntry> = if only_selected && !self.selected.is_empty() {
            self.selected
                .iter()
                .filter_map(|&i| self.entries.get(i).cloned())
                .collect()
        } else {
            self.entries.clone()
        };
        let s = summarize(&target_entries);
        let delete_count = target_entries
            .iter()
            .filter(|e| e.action == FolderMergeAction::DeleteOutput)
            .count();
        let copy_count = s.mergeable.saturating_sub(delete_count);
        let in_place_tag =
            output_matches_input_side(&left, center.as_deref(), &right, &output).map(|side_key| {
                let side_label = crate::i18n::tr(side_key);
                format!(" [{}]", side_label.trim_end_matches(':').trim())
            });
        let mut msg = format!(
            "{}\n{}{}\n\n• {}: {}\n• {}: {}",
            crate::i18n::tr("merge.merge_folders_to_output"),
            self.output_path,
            in_place_tag.as_deref().unwrap_or(""),
            crate::i18n::tr("merge.copy_to_output"),
            copy_count,
            crate::i18n::tr("merge.delete_from_output"),
            delete_count,
        );
        if s.conflicts > 0 {
            msg.push_str(&format!(
                "\n• {}: {}",
                crate::i18n::tr("common.conflicts"),
                s.conflicts
            ));
        }
        if !dialogs.confirm(&msg) {
            return;
        }

        let include_unchanged = self.include_unchanged_in_output && !only_selected;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_bg = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel();
        self.pending_rx = Some(rx);
        self.cancel_flag = Some(cancel);
        self.running_exec = true;
        thread::spawn(move || {
            let res = execute_folder_merge(
                &left,
                center.as_deref(),
                &right,
                &output,
                &target_entries,
                include_unchanged,
                &cancel_bg,
            );
            let _ = tx.send(MergeJobResult::Executed(res));
        });
    }

    fn poll_background(&mut self, ctx: &Context, dialogs: &mut dyn DialogHost) {
        let Some(rx) = &self.pending_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(MergeJobResult::Plan(res)) => {
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
                            "{} {} ({} {}, {} {}, {} {})",
                            timestamp_now_hms(),
                            crate::i18n::tr("merge.merge_preview_updated"),
                            s.mergeable,
                            crate::i18n::tr("merge.mergeable_mesclaveis"),
                            s.conflicts,
                            crate::i18n::tr("merge.conflicts"),
                            s.unchanged,
                            crate::i18n::tr("merge.same"),
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
            Ok(MergeJobResult::Executed(res)) => {
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
                            "{ts} {} (L:{}, R:{}, C:{}, X:{}, !:{})",
                            crate::i18n::tr("merge.merge_complete"),
                            report.copied_from_left,
                            report.copied_from_right,
                            report.copied_from_center,
                            report.deleted_output,
                            report.skipped_conflicts,
                        ));
                        if !report.errors.is_empty() {
                            dialogs.error(&format!(
                                "{}\n{}",
                                crate::i18n::tr("merge.some_items_failed_to_merge"),
                                report.errors.iter().take(5).cloned().collect::<Vec<_>>().join("\n")
                            ));
                        }
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
        if self.has_pair() {
            self.start_scan();
        }
    }

    pub fn saved_options(&self) -> serde_json::Value {
        serde_json::json!({
            "compare_content": self.options.compare_content,
            "name_filter": self.options.name_filter,
            "include_unchanged": self.include_unchanged_in_output,
        })
    }

    pub fn apply_saved_options(&mut self, value: &serde_json::Value) {
        if let Some(v) = value["compare_content"].as_bool() {
            self.options.compare_content = v;
        }
        if let Some(v) = value["name_filter"].as_str() {
            self.options.name_filter = v.to_owned();
        }
        if let Some(v) = value["include_unchanged"].as_bool() {
            self.include_unchanged_in_output = v;
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for (idx, entry) in self.entries.iter().enumerate() {
            if matches!(self.dir_mode, DirMode::Flatten | DirMode::FilesOnly) && entry.is_dir {
                continue;
            }
            let keep = if entry.is_dir && self.dir_mode == DirMode::ShowAll {
                true
            } else {
                match self.filter {
                    DisplayFilter::All => true,
                    DisplayFilter::Changed => entry.status != FolderMergeStatus::Unchanged,
                    DisplayFilter::Conflicts => entry.status == FolderMergeStatus::Conflict,
                    DisplayFilter::LeftChanges => matches!(
                        entry.status,
                        FolderMergeStatus::LeftChanged
                            | FolderMergeStatus::BothSame
                            | FolderMergeStatus::Conflict
                    ),
                    DisplayFilter::RightChanges => matches!(
                        entry.status,
                        FolderMergeStatus::RightChanged
                            | FolderMergeStatus::BothSame
                            | FolderMergeStatus::Conflict
                    ),
                    DisplayFilter::Mergeable => matches!(
                        entry.status,
                        FolderMergeStatus::LeftChanged
                            | FolderMergeStatus::RightChanged
                            | FolderMergeStatus::BothSame
                    ),
                    DisplayFilter::Unchanged => entry.status == FolderMergeStatus::Unchanged,
                }
            };
            if !keep {
                continue;
            }
            if !matches!(self.dir_mode, DirMode::Flatten) {
                let mut hidden = false;
                let mut anc = entry.relative.parent();
                while let Some(p) = anc {
                    if !p.as_os_str().is_empty()
                        && self.collapsed.contains(&p.to_string_lossy().to_lowercase())
                    {
                        hidden = true;
                        break;
                    }
                    anc = p.parent();
                }
                if hidden {
                    continue;
                }
            }
            out.push(idx);
        }
        out
    }

    fn open_rules_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        let mut fields = [
            DialogField::Check(
                crate::i18n::tr("common.compare_file_contents_binary"),
                self.options.compare_content,
            ),
            DialogField::Check(
                crate::i18n::tr("merge.also_copy_unchanged_files_to_output"),
                self.include_unchanged_in_output,
            ),
            DialogField::Text(
                crate::i18n::tr("common.filters_colon"),
                self.options.name_filter.clone(),
            ),
        ];
        if dialogs.fields(&crate::i18n::tr("common.session_settings"), &mut fields) {
            if let DialogField::Check(_, v) = fields[0] {
                self.options.compare_content = v;
            }
            if let DialogField::Check(_, v) = fields[1] {
                self.include_unchanged_in_output = v;
            }
            if let DialogField::Text(_, ref f) = fields[2] {
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
    ) -> FolderMergeUiAction {
        self.poll_background(ctx, dialogs);
        let mut action = FolderMergeUiAction::None;
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
        let can_merge = self.has_pair() && !pending && !self.entries.is_empty();

        // 1. Menu Bar
        TopBottomPanel::top("folder_merge_menu")
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
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "menu.actions.compare_refresh  F5",
                            self.has_pair() && !pending,
                        )
                        .clicked()
                        {
                            self.start_scan();
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(ui, icons, "folder.merge_info", true).clicked() {
                            let rows = vec![
                                (crate::i18n::tr("folder.side_left"), self.left_path.clone()),
                                (crate::i18n::tr("view.center_pane"), self.center_path.clone()),
                                (crate::i18n::tr("folder.side_right"), self.right_path.clone()),
                                (crate::i18n::tr("merge.output"), self.output_path.clone()),
                                (
                                    crate::i18n::tr("common.show_conflicts"),
                                    summary.conflicts.to_string(),
                                ),
                            ];
                            super::info_modals::open_info(
                                ui.ctx(),
                                crate::i18n::tr("folder.merge_info"),
                                rows,
                            );
                            ui.close_menu();
                        }
                        super::session_menu::suffix(ui, icons, self.has_pair());
                    });
                    super::chrome::menu_button(ui, "menu.actions", |ui| {
                        if super::chrome::menu_item(ui, icons, "merge.to_output", can_merge)
                            .clicked()
                        {
                            self.start_merge_execute(false, dialogs);
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.copy_to_output",
                            can_merge && !self.selected.is_empty(),
                        )
                        .clicked()
                        {
                            self.start_merge_execute(true, dialogs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.take_left",
                            !self.selected.is_empty(),
                        )
                        .clicked()
                        {
                            for &i in &self.selected {
                                if let Some(e) = self.entries.get_mut(i) {
                                    if e.left.is_some() {
                                        e.action = FolderMergeAction::TakeLeft;
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.take_center",
                            !self.selected.is_empty() && !self.center_path.trim().is_empty(),
                        )
                        .clicked()
                        {
                            for &i in &self.selected {
                                if let Some(e) = self.entries.get_mut(i) {
                                    if e.center.is_some() {
                                        e.action = FolderMergeAction::TakeCenter;
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                        if super::chrome::menu_item(
                            ui,
                            icons,
                            "merge.take_right",
                            !self.selected.is_empty(),
                        )
                        .clicked()
                        {
                            for &i in &self.selected {
                                if let Some(e) = self.entries.get_mut(i) {
                                    if e.right.is_some() {
                                        e.action = FolderMergeAction::TakeRight;
                                    }
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
                                self.left_path = Path::new(&self.left_path)
                                    .join(rel)
                                    .to_string_lossy()
                                    .into_owned();
                                self.right_path = Path::new(&self.right_path)
                                    .join(rel)
                                    .to_string_lossy()
                                    .into_owned();
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
                        super::chrome::submenu_item(ui, icons, "folder.select_newer", |ui, icons| {
                            for (lbl, l, r) in [
                                ("folder.side_left", true, false),
                                ("folder.side_right", false, true),
                                ("folder.side_both", true, true),
                            ] {
                                if super::chrome::menu_item(ui, icons, lbl, true).clicked() {
                                    self.selected = self
                                        .visible_indices()
                                        .into_iter()
                                        .filter(|&i| {
                                            let e = &self.entries[i];
                                            let lt = e.left.as_ref().and_then(|m| m.modified_secs);
                                            let rt = e.right.as_ref().and_then(|m| m.modified_secs);
                                            !e.is_dir
                                                && ((l && lt > rt) || (r && rt > lt))
                                        })
                                        .collect();
                                    ui.close_menu();
                                }
                            }
                        });
                        super::chrome::submenu_item(ui, icons, "folder.select_orphans", |ui, icons| {
                            for (lbl, l, r) in [
                                ("folder.side_left", true, false),
                                ("folder.side_right", false, true),
                                ("folder.side_both", true, true),
                            ] {
                                if super::chrome::menu_item(ui, icons, lbl, true).clicked() {
                                    self.selected = self
                                        .visible_indices()
                                        .into_iter()
                                        .filter(|&i| {
                                            let e = &self.entries[i];
                                            (l && e.left.is_some() && e.right.is_none())
                                                || (r && e.right.is_some() && e.left.is_none())
                                        })
                                        .collect();
                                    ui.close_menu();
                                }
                            }
                        });
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
                        for (lbl, f) in [
                            ("common.all", DisplayFilter::All),
                            ("common.show_changes", DisplayFilter::Changed),
                            ("common.show_conflicts", DisplayFilter::Conflicts),
                            ("common.show_left_changes", DisplayFilter::LeftChanges),
                            ("common.show_right_changes", DisplayFilter::RightChanges),
                            ("common.show_mergeable", DisplayFilter::Mergeable),
                            ("common.same", DisplayFilter::Unchanged),
                        ] {
                            if super::chrome::menu_radio_item(
                                ui,
                                icons,
                                lbl,
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
                            "folder.compare_files_only",
                            self.dir_mode == DirMode::FilesOnly,
                            true,
                        )
                        .clicked()
                        {
                            self.dir_mode = if self.dir_mode == DirMode::FilesOnly {
                                DirMode::Structure
                            } else {
                                DirMode::FilesOnly
                            };
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
                            "view.center_pane",
                            self.show_center_pane,
                            true,
                        )
                        .clicked()
                        {
                            self.show_center_pane = !self.show_center_pane;
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

        // 3. Exact BC5 FOLDER_MERGE Toolbar
        let tool_state = |name: &str| match name {
            "SessionHome" | "SessionsCombo" | "Rules" | "SelectAll" | "SelectAllFiles"
            | "Swap" | "NameFilter" => ToolState::enabled(true),
            "ExpandAll" | "CollapseAll" => {
                ToolState::enabled(!matches!(self.dir_mode, DirMode::Flatten))
            }
            "Refresh" => ToolState::enabled(self.has_pair() && !pending),
            "Stop" => ToolState::enabled(pending),
            "FavMergeAll" => ToolState::checked(self.filter == DisplayFilter::All),
            "FavChanged" => ToolState::checked(self.filter == DisplayFilter::Changed),
            "FavConflict" => ToolState::checked(self.filter == DisplayFilter::Conflicts),
            "FavLeft" => ToolState::checked(self.filter == DisplayFilter::LeftChanges),
            "FavRight" => ToolState::checked(self.filter == DisplayFilter::RightChanges),
            "FavMergeable" => ToolState::checked(self.filter == DisplayFilter::Mergeable),
            "FavUnchanged" => ToolState::checked(self.filter == DisplayFilter::Unchanged),
            "DirShowAll" => ToolState::checked(self.dir_mode == DirMode::ShowAll),
            "DirCompareStructure" => ToolState::checked(self.dir_mode == DirMode::Structure),
            "DirCompareFilesOnly" => ToolState::checked(self.dir_mode == DirMode::FilesOnly),
            "DirFlatten" => ToolState::checked(self.dir_mode == DirMode::Flatten),
            "InsigIgnored" => ToolState::checked(!self.options.compare_content),
            "SuppressFilters" => ToolState::checked(self.options.suppress_filters),
            "Merge" => ToolState::enabled(can_merge),
            "CopyToOutput" | "CopySide" => ToolState::enabled(can_merge && !self.selected.is_empty()),
            _ => ToolState::DISABLED,
        };
        let items = super::toolbar_spec::FOLDER_MERGE;
        let mut tool_event = None;
        let mut filter_text = self.options.name_filter.clone();
        let mut filter_submitted = false;
        if self.show_toolbar {
            let toolbar_height =
                super::toolbar::height(ui, items, ui.available_width(), &tool_state);
            TopBottomPanel::top("folder_merge_toolbar")
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
                        &mut |ui, _item, rect| {
                            let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                            let out = super::history_combo::show_sized(
                                &mut child,
                                "fmerge_name_filter",
                                &mut filter_text,
                                &["*.*".to_owned()],
                                rect.size(),
                                "",
                            );
                            if out.submitted || out.selected.is_some() {
                                if let Some(sel) = out.selected {
                                    filter_text = sel;
                                }
                                filter_submitted = true;
                            }
                        },
                    );
                });
        }
        if filter_text != self.options.name_filter {
            self.options.name_filter = filter_text;
        }
        if filter_submitted && self.has_pair() {
            self.start_scan();
        }
        match tool_event {
            Some(ToolEvent::Click("SessionHome")) => action = FolderMergeUiAction::Home,
            Some(ToolEvent::Click("FavMergeAll")) => self.filter = DisplayFilter::All,
            Some(ToolEvent::Click("FavChanged")) => self.filter = DisplayFilter::Changed,
            Some(ToolEvent::Click("FavConflict")) => self.filter = DisplayFilter::Conflicts,
            Some(ToolEvent::Click("FavLeft")) => self.filter = DisplayFilter::LeftChanges,
            Some(ToolEvent::Click("FavRight")) => self.filter = DisplayFilter::RightChanges,
            Some(ToolEvent::Click("FavMergeable")) => self.filter = DisplayFilter::Mergeable,
            Some(ToolEvent::Click("FavUnchanged")) => self.filter = DisplayFilter::Unchanged,
            Some(ToolEvent::Click("DirShowAll")) => self.dir_mode = DirMode::ShowAll,
            Some(ToolEvent::Click("DirCompareStructure")) => self.dir_mode = DirMode::Structure,
            Some(ToolEvent::Click("DirCompareFilesOnly")) => self.dir_mode = DirMode::FilesOnly,
            Some(ToolEvent::Click("DirFlatten")) => self.dir_mode = DirMode::Flatten,
            Some(ToolEvent::Click("InsigIgnored")) => {
                self.options.compare_content = !self.options.compare_content;
                if self.has_pair() {
                    self.start_scan();
                }
            }
            Some(ToolEvent::Click("Rules")) | Some(ToolEvent::Click("NameFilter")) => {
                self.open_rules_dialog(dialogs);
            }
            Some(ToolEvent::Click("Merge")) => self.start_merge_execute(false, dialogs),
            Some(ToolEvent::Click("CopyToOutput")) | Some(ToolEvent::Click("CopySide")) => {
                self.start_merge_execute(true, dialogs);
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
            Some(ToolEvent::Click("SelectAllFiles")) => {
                self.selected = self
                    .visible_indices()
                    .into_iter()
                    .filter(|&i| !self.entries[i].is_dir)
                    .collect();
            }
            Some(ToolEvent::Click("Refresh")) => self.start_scan(),
            Some(ToolEvent::Click("Swap")) => self.swap_sides(),
            Some(ToolEvent::Click("Stop")) => self.stop_pending(),
            Some(ToolEvent::Click("SuppressFilters")) => {
                self.options.suppress_filters = !self.options.suppress_filters;
                if self.has_pair() {
                    self.start_scan();
                }
            }
            _ => {}
        }

        if ctx.input(|i| i.key_pressed(Key::F5)) && self.has_pair() && !pending {
            self.start_scan();
        }

        // 4. 3+1 Path Bar (Left | Center | Right + Output target)
        TopBottomPanel::top("folder_merge_paths")
            .show_separator_line(true)
            .frame(
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(6.0, 5.0)),
            )
            .show_inside(ui, |ui| {
                ui.visuals_mut().widgets.inactive.bg_stroke =
                    Stroke::new(1.0_f32, Color32::from_gray(155));
                ui.visuals_mut().extreme_bg_color = Color32::WHITE;
                ui.spacing_mut().item_spacing.x = 6.0;
                let total_w = ui.available_width();
                let col_w = ((total_w - 16.0) / 3.0).max(120.0);
                let mut need_scan = false;

                ui.horizontal(|ui| {
                    for (slot, label, color) in [
                        (0usize, "merge.left", LEFT_CHANGE_TEXT),
                        (1usize, "merge.center", Color32::from_gray(70)),
                        (2usize, "merge.right", RIGHT_CHANGE_TEXT),
                    ] {
                        ui.allocate_ui_with_layout(
                            vec2(col_w, 22.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(crate::i18n::tr(label))
                                        .size(11.5)
                                        .strong()
                                        .color(color),
                                );
                                let path_ref = match slot {
                                    0 => &mut self.left_path,
                                    1 => &mut self.center_path,
                                    _ => &mut self.right_path,
                                };
                                let fw = (ui.available_width() - 32.0).max(50.0);
                                let resp = ui.add(TextEdit::singleline(path_ref).desired_width(fw));
                                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                    need_scan = true;
                                }
                                if icon_btn(ui, icons, "imgBrowse", "merge.select_folder").clicked() {
                                    if let Some(dir) =
                                        dialogs.folder(slot == 0, Path::new(path_ref.as_str()))
                                    {
                                        *path_ref = dir.to_string_lossy().into_owned();
                                        need_scan = true;
                                    }
                                }
                            },
                        );
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if let Some(tex) = icons.get_icon(ctx, "imgDirMergeOutput") {
                        ui.image((tex.id(), vec2(15.0, 15.0)));
                    }
                    ui.label(RichText::new(crate::i18n::tr("merge.output")).strong());
                    let out_w = (ui.available_width() - 380.0).clamp(140.0, 480.0);
                    ui.add(
                        TextEdit::singleline(&mut self.output_path)
                            .desired_width(out_w)
                            .hint_text(crate::i18n::tr("merge.merge_output_folder")),
                    );
                    if icon_btn(ui, icons, "imgBrowse", "merge.select_output_folder").clicked() {
                        if let Some(dir) = dialogs.folder(false, Path::new(&self.output_path)) {
                            self.output_path = dir.to_string_lossy().into_owned();
                        }
                    }
                    ui.checkbox(
                        &mut self.include_unchanged_in_output,
                        crate::i18n::tr("merge.include_same"),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(2.0);
                        if ui
                            .add_enabled(
                                can_merge,
                                Button::new(
                                    RichText::new(format!(
                                        "{} ({})",
                                        crate::i18n::tr("merge.to_output"),
                                        summary.mergeable
                                    ))
                                    .strong(),
                                )
                                .fill(if can_merge {
                                    Color32::from_rgb(220, 238, 255)
                                } else {
                                    Color32::from_gray(232)
                                })
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if can_merge {
                                        Color32::from_rgb(0, 120, 212)
                                    } else {
                                        Color32::from_gray(185)
                                    },
                                ))
                                .min_size(vec2(148.0, 23.0)),
                            )
                            .clicked()
                        {
                            self.start_merge_execute(false, dialogs);
                        }
                    });
                });

                if need_scan && self.has_pair() {
                    self.start_scan();
                }
            });

        // 5. Status Bar
        TopBottomPanel::bottom("folder_merge_status")
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
                            crate::i18n::tr("merge.merging_folders")
                        } else {
                            crate::i18n::tr("merge.comparing_folders")
                        };
                        ui.label(RichText::new(msg).color(Color32::from_rgb(0, 102, 180)));
                    } else if let Some(err) = &self.last_error {
                        ui.label(RichText::new(err).color(Color32::DARK_RED));
                    } else {
                        ui.label(format!(
                            "{}: {}  ·  {}: {}  ·  {}: {}  ·  {}: {}  ·  {}: {}",
                            crate::i18n::tr("common.conflicts"),
                            summary.conflicts,
                            crate::i18n::tr("common.left"),
                            summary.left_changes,
                            crate::i18n::tr("common.right"),
                            summary.right_changes,
                            crate::i18n::tr("merge.mergeable"),
                            summary.mergeable,
                            crate::i18n::tr("common.same"),
                            summary.unchanged,
                        ));
                    }
                });
                super::chrome::status_bar_grip(ui.painter(), ui.max_rect());
            });

        // 6. Optional Log Panel
        if self.show_log {
            TopBottomPanel::bottom("folder_merge_log")
                .resizable(true)
                .default_height(78.0)
                .min_height(44.0)
                .max_height(200.0)
                .frame(
                    Frame::none()
                        .fill(Color32::from_rgb(227, 231, 237))
                        .stroke(Stroke::new(1.0_f32, Color32::from_gray(172)))
                        .inner_margin(Margin::symmetric(6.0, 3.0)),
                )
                .show_inside(ui, |ui| {
                    ScrollArea::vertical()
                        .id_salt("folder_merge_log_scroll")
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.log_lines.is_empty() {
                                ui.label(
                                    RichText::new(crate::i18n::tr(
                                        "merge.select_left_center_optional_and_right",
                                    ))
                                    .size(11.0)
                                    .color(Color32::from_gray(110)),
                                );
                            } else {
                                for line in &self.log_lines {
                                    ui.label(
                                        RichText::new(line)
                                            .font(FontId::new(
                                                11.5,
                                                FontFamily::Name("Table Mono".into()),
                                            ))
                                            .color(Color32::from_gray(45)),
                                    );
                                }
                            }
                        });
                });
        }

        // 7. Central 3-Pane Tree (Left | Center | Right + Status Action Strip)
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::WHITE))
            .show_inside(ui, |ui| {
                let rect = ui.max_rect();
                let status_w = 34.0;
                let pane_w = ((rect.width() - status_w) / 3.0).max(110.0);

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
                let col_x = [
                    rect.left(),
                    rect.left() + pane_w,
                    rect.left() + pane_w * 2.0,
                    rect.left() + pane_w * 3.0,
                ];
                for &x in &col_x[..3] {
                    let hr = Rect::from_min_size(pos2(x, header_rect.top()), vec2(pane_w, 22.0));
                    draw_merge_header(ui, hr);
                    ui.painter().vline(
                        hr.right(),
                        rect.y_range(),
                        Stroke::new(1.0_f32, Color32::from_gray(195)),
                    );
                }
                ui.painter().text(
                    pos2(col_x[3] + status_w / 2.0, header_rect.center().y),
                    Align2::CENTER_CENTER,
                    "⇄",
                    FontId::proportional(11.5),
                    Color32::from_gray(75),
                );

                // Rows
                let body_rect =
                    Rect::from_min_max(pos2(rect.left(), header_rect.bottom()), rect.max);
                let visible = self.visible_indices();
                let mut toggle_dir: Option<String> = None;
                let mut open_merge_row: Option<usize> = None;
                let mut set_action: Option<(usize, FolderMergeAction)> = None;

                let mut child = ui.new_child(UiBuilder::new().max_rect(body_rect));
                ScrollArea::vertical()
                    .id_salt("folder_merge_rows")
                    .auto_shrink([false, false])
                    .show_rows(&mut child, ROW_HEIGHT, visible.len(), |ui, range| {
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
                                match entry.status {
                                    FolderMergeStatus::LeftChanged => LEFT_CHANGE_BG,
                                    FolderMergeStatus::RightChanged => RIGHT_CHANGE_BG,
                                    FolderMergeStatus::BothSame => BOTH_SAME_BG,
                                    FolderMergeStatus::Conflict | FolderMergeStatus::Error => {
                                        CONFLICT_BG
                                    }
                                    FolderMergeStatus::Unchanged if vis_pos % 2 == 1 => {
                                        Color32::from_gray(249)
                                    }
                                    FolderMergeStatus::Unchanged => Color32::WHITE,
                                }
                            };
                            ui.painter().rect_filled(row_rect, 0.0, bg);

                            let name = if self.dir_mode == DirMode::Flatten {
                                entry.relative.to_string_lossy().into_owned()
                            } else {
                                entry
                                    .relative
                                    .file_name()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            };
                            let indent = if self.dir_mode == DirMode::Flatten {
                                0.0
                            } else {
                                entry.depth as f32 * 14.0
                            };
                            let collapsed = self
                                .collapsed
                                .contains(&entry.relative.to_string_lossy().to_lowercase());

                            let sides = [
                                (0usize, entry.left.as_ref()),
                                (1usize, entry.center.as_ref()),
                                (2usize, entry.right.as_ref()),
                            ];
                            for (s_idx, meta_opt) in sides {
                                let cell = Rect::from_min_size(
                                    pos2(col_x[s_idx], row_rect.top()),
                                    vec2(pane_w, ROW_HEIGHT),
                                );
                                if let Some(m) = meta_opt {
                                    let col = match entry.status {
                                        FolderMergeStatus::Conflict | FolderMergeStatus::Error => {
                                            CONFLICT_TEXT
                                        }
                                        FolderMergeStatus::LeftChanged if s_idx == 0 => {
                                            LEFT_CHANGE_TEXT
                                        }
                                        FolderMergeStatus::RightChanged if s_idx == 2 => {
                                            RIGHT_CHANGE_TEXT
                                        }
                                        FolderMergeStatus::BothSame => BOTH_SAME_TEXT,
                                        _ => Color32::BLACK,
                                    };
                                    draw_merge_cell(
                                        ui,
                                        icons,
                                        cell,
                                        indent,
                                        &name,
                                        m.is_dir,
                                        collapsed,
                                        m.size,
                                        m.modified_secs,
                                        col,
                                    );
                                }
                            }

                            // Rightmost status/action icon
                            let act_rect = Rect::from_min_size(
                                pos2(col_x[3], row_rect.top()),
                                vec2(status_w, ROW_HEIGHT),
                            );
                            if let Some(tex) = icons.get_icon(ctx, entry.action.icon()) {
                                ui.painter().image(
                                    tex.id(),
                                    Rect::from_center_size(act_rect.center(), vec2(16.0, 16.0)),
                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }
                            let act_tip = match &entry.error {
                                Some(err) => format!("{}: {err}", crate::i18n::tr("merge.error")),
                                None => crate::i18n::tr(entry.action.label()),
                            };
                            let act_resp = ui
                                .interact(
                                    act_rect,
                                    ui.id().with(("fmerge_act", idx)),
                                    Sense::click(),
                                )
                                .on_hover_text(act_tip);
                            if act_resp.clicked() {
                                let next = next_merge_action(entry);
                                set_action = Some((idx, next));
                            }

                            if row_resp.clicked() {
                                if !ui.input(|i| i.modifiers.ctrl) {
                                    self.selected.clear();
                                }
                                self.selected.insert(idx);
                            }
                            if row_resp.double_clicked() {
                                if entry.is_dir {
                                    toggle_dir =
                                        Some(entry.relative.to_string_lossy().to_lowercase());
                                } else {
                                    open_merge_row = Some(idx);
                                }
                            }
                            if row_resp.secondary_clicked() && !self.selected.contains(&idx) {
                                self.selected.clear();
                                self.selected.insert(idx);
                            }
                            let mut open_with_kind: Option<SessionKind> = None;
                            let mut open_compare_modal = false;
                            let mut copy_paths_now = false;
                            let mut open_subfolders_cmd: Option<bool> = None;
                            let mut refresh_now = false;
                            let mut explore_cmd: Option<bool> = None;
                            let mut assoc_app_now = false;
                            let mut bulk_action: Option<FolderMergeAction> = None;
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
                                    "merge.open_in_text_merge",
                                    entry.left.is_some() && entry.right.is_some(),
                                )
                                .clicked()
                                {
                                    open_merge_row = Some(idx);
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
                                    "merge.take_left",
                                    entry.left.is_some() || self.selected.len() > 1,
                                )
                                .clicked()
                                {
                                    bulk_action = Some(FolderMergeAction::TakeLeft);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "merge.take_center",
                                    entry.center.is_some() || self.selected.len() > 1,
                                )
                                .clicked()
                                {
                                    bulk_action = Some(FolderMergeAction::TakeCenter);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "merge.take_right",
                                    entry.right.is_some() || self.selected.len() > 1,
                                )
                                .clicked()
                                {
                                    bulk_action = Some(FolderMergeAction::TakeRight);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(
                                    ui,
                                    icons,
                                    "merge.delete_from_output",
                                    true,
                                )
                                .clicked()
                                {
                                    bulk_action = Some(FolderMergeAction::DeleteOutput);
                                    ui.close_menu();
                                }
                                if super::chrome::menu_item(ui, icons, "common.ignore", true)
                                    .clicked()
                                {
                                    bulk_action = Some(FolderMergeAction::Skip);
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
                                    let p = if lp.is_file() { lp } else { rp };
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
                                    stats: self.merge_selection_stats(),
                                    mode: self.content_mode,
                                    show_results: true,
                                    dont_show_again: false,
                                    include_hidden: false,
                                });
                            }
                            if copy_paths_now {
                                let root = PathBuf::from(self.left_path.trim());
                                let lines: Vec<String> = self
                                    .selected
                                    .iter()
                                    .filter_map(|&i| self.entries.get(i))
                                    .map(|e| root.join(&e.relative).display().to_string())
                                    .collect();
                                ctx.copy_text(lines.join("\n"));
                            }
                            if assoc_app_now {
                                let root = PathBuf::from(self.left_path.trim());
                                let p = root.join(&entry.relative);
                                if let Err(err) = dialogs.open_path(&p, false) {
                                    dialogs.error(&err);
                                }
                            }
                            if let Some(select) = explore_cmd {
                                let root = PathBuf::from(self.left_path.trim());
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
                                FolderMergeAction::TakeLeft => e.left.is_some(),
                                FolderMergeAction::TakeCenter => e.center.is_some(),
                                FolderMergeAction::TakeRight => e.right.is_some(),
                                FolderMergeAction::DeleteOutput
                                | FolderMergeAction::Skip
                                | FolderMergeAction::None
                                | FolderMergeAction::Conflict => true,
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
                if let Some(idx) = open_merge_row {
                    if let Some(e) = self.entries.get(idx) {
                        let left = PathBuf::from(self.left_path.trim()).join(&e.relative);
                        let right = PathBuf::from(self.right_path.trim()).join(&e.relative);
                        let center = (!self.center_path.trim().is_empty())
                            .then(|| PathBuf::from(self.center_path.trim()).join(&e.relative))
                            .filter(|p| p.is_file());
                        let output = (!self.output_path.trim().is_empty())
                            .then(|| PathBuf::from(self.output_path.trim()).join(&e.relative));
                        if left.is_file() && right.is_file() {
                            action = FolderMergeUiAction::OpenTextMerge {
                                left,
                                center,
                                right,
                                output,
                            };
                        }
                    }
                }
            });

        if let Some(mut modal) = self.compare_modal.take() {
            match modal.show(ctx, icons, dialogs) {
                Ok(Some(FolderOpConfirmed::CompareContents {
                    mode, show_results, ..
                })) => self.run_merge_content_compare(mode, show_results),
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
                            let c = if self.center_path.trim().is_empty() {
                                None
                            } else {
                                Some(PathBuf::from(self.center_path.trim()).join(&left_rel))
                            };
                            let o = if self.output_path.trim().is_empty() {
                                None
                            } else {
                                Some(PathBuf::from(self.output_path.trim()).join(&left_rel))
                            };
                            action = FolderMergeUiAction::OpenTextMerge {
                                left: lr.join(&left_rel),
                                center: c,
                                right: rr.join(&right_rel),
                                output: o,
                            };
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

fn next_merge_action(entry: &FolderMergeEntry) -> FolderMergeAction {
    let mut choices = Vec::new();
    if entry.left.is_some() {
        choices.push(FolderMergeAction::TakeLeft);
    }
    if entry.right.is_some() {
        choices.push(FolderMergeAction::TakeRight);
    }
    if entry.center.is_some() {
        choices.push(FolderMergeAction::TakeCenter);
    }
    choices.push(FolderMergeAction::DeleteOutput);
    choices.push(FolderMergeAction::Skip);
    if let Some(pos) = choices.iter().position(|c| *c == entry.action) {
        choices[(pos + 1) % choices.len()]
    } else {
        choices.first().copied().unwrap_or(FolderMergeAction::Skip)
    }
}

fn draw_merge_header(ui: &Ui, rect: Rect) {
    let show_date = rect.width() > 260.0;
    let date_x = if show_date {
        rect.right() - 96.0
    } else {
        rect.right()
    };
    let font = FontId::proportional(11.0);
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
    if show_date {
        ui.painter().text(
            pos2(date_x + 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            crate::i18n::tr("status.modified"),
            font,
            Color32::BLACK,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_merge_cell(
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
) {
    let show_date = rect.width() > 260.0;
    let date_x = if show_date {
        rect.right() - 96.0
    } else {
        rect.right()
    };
    let size_x = (date_x - 62.0).max(rect.left() + 40.0);
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
        let folder_tint = if color == Color32::BLACK {
            Color32::from_rgb(235, 198, 92)
        } else {
            color
        };
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
        let doc = Rect::from_min_size(pos2(x + 2.0, rect.center().y - 6.5), vec2(11.0, 13.0));
        let outline = if color == Color32::BLACK {
            Color32::from_gray(110)
        } else {
            color
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
        let line_col = if color == Color32::BLACK {
            Color32::from_gray(155)
        } else {
            color
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
    x += 20.0;
    painter.text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(11.5),
        color,
    );
    if !is_dir {
        ui.painter().text(
            pos2(date_x - 6.0, rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{size}"),
            FontId::proportional(11.0),
            Color32::from_gray(65),
        );
    }
    if show_date && modified_secs.is_some() {
        ui.painter().text(
            pos2(date_x + 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            format_time(modified_secs),
            FontId::proportional(11.0),
            Color32::from_gray(65),
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
