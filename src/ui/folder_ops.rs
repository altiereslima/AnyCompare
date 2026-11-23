//! AnyCompare Folder Operations Engine & Modal Dialogs
//! (`TOpCompareDialog`, `TOpCompareResultsDialog`, `TOpCopyDialog`, `TOpMoveDialog`,
//! `TOpCopyToDialog`, `TOpMoveToDialog`, `TOpDeleteDialog`, `TOpRenameDialog`,
//! `TOpDOSAttribDialog`, `TOpTouchDialog`, `TOpExcludeDialog`, `TOpNewFolderDialog`).

use super::{dialogs::DialogHost, icons::IconManager, theme::BcTheme};
use crate::{
    diff::{DiffEngine, DiffRules, DiffStatus},
    folder_source::{self, EntryMeta},
    i18n::{tr, tr_fmt},
};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use egui::*;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

/// AnyCompare's three content-comparison algorithms (`TOpCompareDialog` /
/// `TDirSessionComparisonFrame`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ContentCompareMode {
    Crc32,
    #[default]
    Binary,
    RulesBased,
}

impl ContentCompareMode {
    pub const ALL: [Self; 3] = [Self::Crc32, Self::Binary, Self::RulesBased];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Crc32 => "folder.op.compare_crc",
            Self::Binary => "folder.op.compare_binary",
            Self::RulesBased => "folder.op.compare_rules",
        }
    }

    pub fn as_name(self) -> &'static str {
        match self {
            Self::Crc32 => "crc",
            Self::Binary => "binary",
            Self::RulesBased => "rules",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "crc" | "crc32" => Some(Self::Crc32),
            "binary" => Some(Self::Binary),
            "rules" | "rules_based" => Some(Self::RulesBased),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileContentVerdict {
    Same {
        detail: String,
        left_crc: Option<u32>,
        right_crc: Option<u32>,
    },
    Minor {
        detail: String,
    },
    Different {
        detail: String,
        left_crc: Option<u32>,
        right_crc: Option<u32>,
    },
    Error(String),
}

fn crc32_of_entry(
    root: &Path,
    relative: &Path,
    meta: &EntryMeta,
    cancel: &AtomicBool,
) -> Result<u32, String> {
    if let Some(crc) = meta.crc32 {
        return Ok(crc);
    }
    if folder_source::is_snapshot_path(root) {
        return Err(tr("folder.snapshot_no_content"));
    }
    if !folder_source::is_writable(root) {
        let bytes = folder_source::read_entry_bytes(root, relative)?;
        return Ok(crc32fast::hash(&bytes));
    }
    let path = crate::file_ops::resolve(root, relative)?;
    let mut file = fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(tr("common.canceled"));
        }
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize())
}

fn read_source_bytes(root: &Path, relative: &Path) -> Result<Vec<u8>, String> {
    if folder_source::is_snapshot_path(root) {
        return Err(tr("folder.snapshot_no_content"));
    }
    if !folder_source::is_writable(root) {
        return folder_source::read_entry_bytes(root, relative);
    }
    let path = crate::file_ops::resolve(root, relative)?;
    fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}

fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return false; // UTF-16 BOM
    }
    let sample = &bytes[..bytes.len().min(8192)];
    sample.contains(&0)
}

fn normalize_eols(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Compares a paired file entry using CRC32, Binary byte-for-byte, or Rules-based
/// semantic comparison.
#[allow(clippy::too_many_arguments)]
pub fn compare_file_pair(
    left_root: &Path,
    left_rel: &Path,
    left_meta: &EntryMeta,
    right_root: &Path,
    right_rel: &Path,
    right_meta: &EntryMeta,
    mode: ContentCompareMode,
    cancel: &AtomicBool,
) -> FileContentVerdict {
    if cancel.load(Ordering::Relaxed) {
        return FileContentVerdict::Error(tr("common.canceled"));
    }
    match mode {
        ContentCompareMode::Crc32 => {
            let l_crc = match crc32_of_entry(left_root, left_rel, left_meta, cancel) {
                Ok(c) => c,
                Err(e) => return FileContentVerdict::Error(e),
            };
            let r_crc = match crc32_of_entry(right_root, right_rel, right_meta, cancel) {
                Ok(c) => c,
                Err(e) => return FileContentVerdict::Error(e),
            };
            if left_meta.len() == right_meta.len() && l_crc == r_crc {
                FileContentVerdict::Same {
                    detail: format!("CRC32 {:08X}", l_crc),
                    left_crc: Some(l_crc),
                    right_crc: Some(r_crc),
                }
            } else {
                FileContentVerdict::Different {
                    detail: format!("CRC32 {:08X} ≠ {:08X}", l_crc, r_crc),
                    left_crc: Some(l_crc),
                    right_crc: Some(r_crc),
                }
            }
        }
        ContentCompareMode::Binary => {
            if left_meta.len() != right_meta.len() {
                return FileContentVerdict::Different {
                    detail: format!("{} ≠ {}", format_bytes(left_meta.len()), format_bytes(right_meta.len())),
                    left_crc: left_meta.crc32,
                    right_crc: right_meta.crc32,
                };
            }
            match folder_source::same_entry_content(
                left_root,
                left_rel,
                left_meta,
                right_root,
                right_rel,
                right_meta,
                cancel,
            ) {
                Ok(true) => FileContentVerdict::Same {
                    detail: format_bytes(left_meta.len()),
                    left_crc: left_meta.crc32,
                    right_crc: right_meta.crc32,
                },
                Ok(false) => FileContentVerdict::Different {
                    detail: tr("folder.op.different"),
                    left_crc: left_meta.crc32,
                    right_crc: right_meta.crc32,
                },
                Err(e) => FileContentVerdict::Error(e.to_string()),
            }
        }
        ContentCompareMode::RulesBased => {
            // Fast path: identical sizes + identical binary content is always Same.
            if left_meta.len() == right_meta.len()
                && !folder_source::is_snapshot_path(left_root)
                && !folder_source::is_snapshot_path(right_root)
            {
                if let Ok(true) = folder_source::same_entry_content(
                    left_root,
                    left_rel,
                    left_meta,
                    right_root,
                    right_rel,
                    right_meta,
                    cancel,
                ) {
                    return FileContentVerdict::Same {
                        detail: tr("common.binary_same"),
                        left_crc: left_meta.crc32,
                        right_crc: right_meta.crc32,
                    };
                }
            }
            let left_bytes = match read_source_bytes(left_root, left_rel) {
                Ok(b) => b,
                Err(e) => return FileContentVerdict::Error(e),
            };
            let right_bytes = match read_source_bytes(right_root, right_rel) {
                Ok(b) => b,
                Err(e) => return FileContentVerdict::Error(e),
            };
            if left_bytes == right_bytes {
                return FileContentVerdict::Same {
                    detail: tr("common.binary_same"),
                    left_crc: left_meta.crc32,
                    right_crc: right_meta.crc32,
                };
            }

            // 1. Convertible documents (PDF, DOCX, DOC, RTF, ODT, PPTX, ODP, EPUB)
            if crate::doc_convert::is_convertible(left_rel)
                && crate::doc_convert::is_convertible(right_rel)
            {
                if let (Ok(ld), Ok(rd)) = (
                    crate::doc_convert::convert_bytes(left_rel, &left_bytes),
                    crate::doc_convert::convert_bytes(right_rel, &right_bytes),
                ) {
                    return compare_normalized_texts(&ld.text, &rd.text, tr(ld.format_label));
                }
            }

            // 2. Picture files: compare decoded RGBA pixels
            if crate::picture::is_picture_path(left_rel)
                && crate::picture::is_picture_path(right_rel)
            {
                if let (Ok(lp), Ok(rp)) = (
                    folder_source::materialize_file(left_root, left_rel),
                    folder_source::materialize_file(right_root, right_rel),
                ) {
                    if let (Ok(li), Ok(ri)) = (
                        crate::picture::PictureImage::load(&lp),
                        crate::picture::PictureImage::load(&rp),
                    ) {
                        if li.width == ri.width && li.height == ri.height && li.rgba == ri.rgba {
                            return FileContentVerdict::Same {
                                detail: format!("{}×{} px", li.width, li.height),
                                left_crc: None,
                                right_crc: None,
                            };
                        }
                        return FileContentVerdict::Different {
                            detail: format!(
                                "{}×{} vs {}×{}",
                                li.width, li.height, ri.width, ri.height
                            ),
                            left_crc: None,
                            right_crc: None,
                        };
                    }
                }
            }

            // 3. PE executables / DLLs: compare parsed version properties
            if crate::pe_version::supported(left_rel) && crate::pe_version::supported(right_rel) {
                if let (Ok(lv), Ok(rv)) = (
                    crate::pe_version::Image::parse(&left_bytes),
                    crate::pe_version::Image::parse(&right_bytes),
                ) {
                    let same_nodes = lv.nodes.len() == rv.nodes.len()
                        && lv
                            .nodes
                            .iter()
                            .zip(&rv.nodes)
                            .all(|(a, b)| pe_nodes_equal(a, b));
                    return if same_nodes {
                        FileContentVerdict::Same {
                            detail: "PE".into(),
                            left_crc: None,
                            right_crc: None,
                        }
                    } else {
                        FileContentVerdict::Different {
                            detail: "PE".into(),
                            left_crc: None,
                            right_crc: None,
                        }
                    };
                }
            }

            // 4. Plain text / source / config / CSV / JSON / XML: decode & apply text rules
            if !looks_binary(&left_bytes) && !looks_binary(&right_bytes) {
                if let (Ok((lt, _)), Ok((rt, _))) = (
                    crate::document::Document::decode(&left_bytes),
                    crate::document::Document::decode(&right_bytes),
                ) {
                    return compare_normalized_texts(&lt, &rt, tr("doc.format.plain_text"));
                }
            }

            // 5. Fallback for opaque binary formats
            FileContentVerdict::Different {
                detail: format!(
                    "{} vs {}",
                    format_bytes(left_bytes.len() as u64),
                    format_bytes(right_bytes.len() as u64)
                ),
                left_crc: left_meta.crc32,
                right_crc: right_meta.crc32,
            }
        }
    }
}

fn pe_nodes_equal(a: &crate::pe_version::Node, b: &crate::pe_version::Node) -> bool {
    a.key == b.key
        && a.same(b)
        && a.children.len() == b.children.len()
        && a.children
            .iter()
            .zip(&b.children)
            .all(|(ca, cb)| pe_nodes_equal(ca, cb))
}

fn compare_normalized_texts(left: &str, right: &str, format_tag: String) -> FileContentVerdict {
    let ln = normalize_eols(left);
    let rn = normalize_eols(right);
    if ln == rn {
        // Raw bytes differed (e.g. CRLF vs LF or UTF-8 BOM vs UTF-8), text content is identical.
        return FileContentVerdict::Same {
            detail: format_tag,
            left_crc: None,
            right_crc: None,
        };
    }
    let rules = DiffRules {
        ignore_whitespace: true,
        ignore_case: false,
        ..DiffRules::default()
    };
    let diff = DiffEngine::compare_rules(&ln, &rn, rules);
    let diff_lines = diff
        .lines
        .iter()
        .filter(|l| {
            matches!(
                l.status,
                DiffStatus::Different | DiffStatus::LeftOrphan | DiffStatus::RightOrphan
            )
        })
        .count();
    if diff_lines == 0 {
        return FileContentVerdict::Minor { detail: format_tag };
    }
    FileContentVerdict::Different {
        detail: format!("{format_tag} ({diff_lines})"),
        left_crc: None,
        right_crc: None,
    }
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut suffix = "KB";
    for unit in ["KB", "MB", "GB", "TB"] {
        value /= 1024.0;
        suffix = unit;
        if value < 1024.0 {
            break;
        }
    }
    let number = if value >= 100.0 {
        format!("{value:.0}")
    } else {
        let s = format!("{value:.1}");
        s.strip_suffix(".0").unwrap_or(&s).to_owned()
    };
    let number = if crate::i18n::language().starts_with("pt") {
        number.replace('.', ",")
    } else {
        number
    };
    format!("{number} {suffix}")
}

/// Aggregated selection counts across both panes (including descendants of collapsed folders).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionStats {
    pub left_files: usize,
    pub left_dirs: usize,
    pub left_bytes: u64,
    pub right_files: usize,
    pub right_dirs: usize,
    pub right_bytes: u64,
    pub hidden_items: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FolderStructureMode {
    #[default]
    KeepRelative,
    KeepBase,
    Flatten,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenameMode {
    #[default]
    Dos,
    Regex,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttribTriState {
    #[default]
    Unchanged,
    Set,
    Clear,
}

impl AttribTriState {
    fn cycle(self) -> Self {
        match self {
            Self::Unchanged => Self::Set,
            Self::Set => Self::Clear,
            Self::Clear => Self::Unchanged,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DosAttributesSpec {
    pub read_only: AttribTriState,
    pub hidden: AttribTriState,
    pub archive: AttribTriState,
    pub system: AttribTriState,
    pub recursive: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TouchMode {
    CopyOtherSide,
    #[default]
    SetDateTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareRowStatus {
    Same,
    Minor,
    Different,
    LeftOrphan,
    RightOrphan,
    Error,
}

#[derive(Clone, Debug)]
pub struct CompareResultRow {
    pub relative: PathBuf,
    pub right_relative: PathBuf,
    pub status: CompareRowStatus,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct CompareResultsState {
    pub mode: ContentCompareMode,
    pub elapsed_ms: u128,
    pub same: usize,
    pub minor: usize,
    pub different: usize,
    pub left_orphans: usize,
    pub right_orphans: usize,
    pub errors: usize,
    pub rows: Vec<CompareResultRow>,
    /// Expand state per category: `[Different, Minor, Same, LeftOrphan, RightOrphan, Error]`.
    /// Collapsed by default to match BC5 `TOpCompareResultsDialog`.
    pub expanded: [bool; 6],
    /// Selected tree row: `(category_index, optional row index inside self.rows)`.
    pub selected: Option<(usize, Option<usize>)>,
}

impl CompareResultsState {
    pub fn total_items(&self) -> usize {
        self.same
            + self.minor
            + self.different
            + self.left_orphans
            + self.right_orphans
            + self.errors
    }

    pub fn format_elapsed(&self) -> String {
        let secs = self.elapsed_ms as f64 / 1000.0;
        let s = format!("{secs:.2} s");
        if crate::i18n::language().starts_with("pt") {
            s.replace('.', ",")
        } else {
            s
        }
    }

    pub fn completion_log_line(&self) -> String {
        let total = format_thousands(self.total_items());
        let elapsed = self.format_elapsed();
        tr_fmt(
            "folder.op.results_completed_log",
            &[("count", &total), ("elapsed", &elapsed)],
        )
    }

    pub fn ensure_default_selection(&mut self) {
        if self.selected.is_some() {
            return;
        }
        let counts = [
            self.different,
            self.minor,
            self.same,
            self.left_orphans,
            self.right_orphans,
            self.errors,
        ];
        if let Some((idx, _)) = counts.iter().enumerate().find(|(_, &c)| c > 0) {
            self.selected = Some((idx, None));
        }
    }
}

fn format_thousands(n: usize) -> String {
    let s = n.to_string();
    let sep = if crate::i18n::language().starts_with("pt")
        || crate::i18n::language().starts_with("de")
        || crate::i18n::language().starts_with("es")
        || crate::i18n::language().starts_with("it")
    {
        '.'
    } else {
        ','
    };
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(sep);
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// Interactive BC5 modal dialogs for Folder Compare operations.
pub enum FolderOpModal {
    CompareContents {
        stats: SelectionStats,
        mode: ContentCompareMode,
        show_results: bool,
        dont_show_again: bool,
        include_hidden: bool,
    },
    CompareResults(CompareResultsState),
    CopyToSide {
        stats: SelectionStats,
        to_right: bool,
        overwrite_count: usize,
        dont_show_again: bool,
    },
    MoveToSide {
        stats: SelectionStats,
        to_right: bool,
        overwrite_count: usize,
        dont_show_again: bool,
    },
    CopyToFolder {
        stats: SelectionStats,
        source_left: bool,
        target_dir: String,
        structure: FolderStructureMode,
    },
    MoveToFolder {
        stats: SelectionStats,
        source_left: bool,
        target_dir: String,
        structure: FolderStructureMode,
    },
    Delete {
        stats: SelectionStats,
        delete_left: bool,
        delete_right: bool,
        use_recycle_bin: bool,
    },
    Rename {
        old_mask: String,
        new_mask: String,
        mode: RenameMode,
        sample_names: Vec<String>,
        single_item: bool,
    },
    Attributes {
        stats: SelectionStats,
        left_side: bool,
        spec: DosAttributesSpec,
    },
    Touch {
        stats: SelectionStats,
        left_side: bool,
        mode: TouchMode,
        datetime_text: String,
        recursive: bool,
        can_copy_other: bool,
    },
    Exclude {
        names: Vec<String>,
        relative_paths: Vec<String>,
        only_this_folder: bool,
    },
    NewFolder {
        name: String,
        create_left: bool,
        create_right: bool,
        left_writable: bool,
        right_writable: bool,
    },
}

/// Confirmed action returned by [`FolderOpModal::show`].
pub enum FolderOpConfirmed {
    CompareContents {
        mode: ContentCompareMode,
        show_results: bool,
        dont_show_again: bool,
        include_hidden: bool,
    },
    CopyToSide {
        to_right: bool,
        dont_show_again: bool,
    },
    MoveToSide {
        to_right: bool,
        dont_show_again: bool,
    },
    CopyToFolder {
        source_left: bool,
        target_dir: PathBuf,
        structure: FolderStructureMode,
    },
    MoveToFolder {
        source_left: bool,
        target_dir: PathBuf,
        structure: FolderStructureMode,
    },
    Delete {
        delete_left: bool,
        delete_right: bool,
        use_recycle_bin: bool,
    },
    Rename {
        old_mask: String,
        new_mask: String,
        mode: RenameMode,
    },
    Attributes {
        left_side: bool,
        spec: DosAttributesSpec,
    },
    Touch {
        left_side: bool,
        mode: TouchMode,
        datetime_text: String,
        recursive: bool,
    },
    Exclude {
        patterns: Vec<String>,
    },
    NewFolder {
        name: String,
        create_left: bool,
        create_right: bool,
    },
    OpenResultRow {
        left_rel: PathBuf,
        right_rel: PathBuf,
        status: CompareRowStatus,
    },
}

/// Graphic center emblem matching BC5's `TOpBaseDialog` dual-pane schematic.
#[derive(Clone, Copy)]
enum SchematicSymbol {
    Compare,
    CopyRight,
    CopyLeft,
    MoveRight,
    MoveLeft,
    Delete,
    FolderOut,
}

fn paint_selection_panel(ui: &mut Ui, stats: SelectionStats, symbol: SchematicSymbol) {
    ui.label(RichText::new(tr("folder.op.selection_colon")).size(12.0));
    ui.add_space(6.0);
    let avail = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(vec2(avail, 74.0), Sense::hover());
    let box_w = 104.0;
    let box_h = 68.0;
    let center_box = Rect::from_center_size(row_rect.center(), vec2(box_w, box_h));

    // Left stats column
    let left_col = Rect::from_min_max(
        pos2(row_rect.left() + 8.0, row_rect.top() + 8.0),
        pos2(center_box.left() - 10.0, row_rect.bottom()),
    );
    paint_side_stats(
        ui,
        left_col,
        stats.left_files,
        stats.left_dirs,
        stats.left_bytes,
    );

    // Right stats column
    let right_col = Rect::from_min_max(
        pos2(center_box.right() + 14.0, row_rect.top() + 8.0),
        pos2(row_rect.right() - 4.0, row_rect.bottom()),
    );
    paint_side_stats(
        ui,
        right_col,
        stats.right_files,
        stats.right_dirs,
        stats.right_bytes,
    );

    // Iconic BC5 two-pane mini window schematic (matching Screenshot 2)
    let p = ui.painter();
    p.rect_filled(center_box, 3.0, Color32::WHITE);
    let title_bar = Rect::from_min_size(center_box.min, vec2(box_w, 9.0));
    p.rect_filled(title_bar, Rounding { nw: 3.0, ne: 3.0, sw: 0.0, se: 0.0 }, Color32::from_rgb(167, 193, 224));
    p.hline(
        center_box.x_range(),
        title_bar.bottom(),
        Stroke::new(1.0_f32, Color32::from_gray(45)),
    );
    let footer_y = center_box.bottom() - 5.0;
    p.hline(
        center_box.x_range(),
        footer_y,
        Stroke::new(1.0_f32, Color32::from_gray(45)),
    );
    let mid_x = center_box.center().x;
    let body_cy = (title_bar.bottom() + footer_y) * 0.5;
    p.vline(
        mid_x,
        title_bar.bottom()..=(body_cy - 11.0),
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    p.vline(
        mid_x,
        (body_cy + 11.0)..=footer_y,
        Stroke::new(1.0_f32, Color32::from_gray(55)),
    );
    p.rect_stroke(center_box, 3.0, Stroke::new(1.0_f32, Color32::from_gray(35)));

    let (glyph, color) = match symbol {
        SchematicSymbol::Compare => ("=?", Color32::BLACK),
        SchematicSymbol::CopyRight => ("→", Color32::from_rgb(200, 140, 0)),
        SchematicSymbol::CopyLeft => ("←", Color32::from_rgb(200, 140, 0)),
        SchematicSymbol::MoveRight => ("⇒", Color32::from_rgb(195, 45, 45)),
        SchematicSymbol::MoveLeft => ("⇐", Color32::from_rgb(195, 45, 45)),
        SchematicSymbol::Delete => ("×", Color32::from_rgb(200, 30, 30)),
        SchematicSymbol::FolderOut => ("↗", Color32::from_rgb(40, 110, 190)),
    };
    p.text(
        pos2(mid_x, body_cy),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(15.0),
        color,
    );
}

fn paint_side_stats(ui: &mut Ui, rect: Rect, files: usize, dirs: usize, bytes: u64) {
    let lines = [
        tr_fmt("folder.op.files_count", &[("count", &files.to_string())]),
        tr_fmt("folder.op.folders_count", &[("count", &dirs.to_string())]),
        format_bytes(bytes),
    ];
    for (idx, line) in lines.iter().enumerate() {
        ui.painter().text(
            pos2(rect.left(), rect.top() + idx as f32 * 18.0),
            Align2::LEFT_TOP,
            line,
            FontId::proportional(12.0),
            Color32::BLACK,
        );
    }
}

fn paint_primary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        Button::new(RichText::new(label).size(12.0).color(if enabled {
            Color32::BLACK
        } else {
            Color32::GRAY
        }))
        .min_size(vec2(86.0, 25.0))
        .fill(Color32::from_rgb(250, 251, 253))
        .stroke(Stroke::new(1.4_f32, Color32::from_rgb(0, 103, 192))),
    )
}

fn paint_secondary_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        Button::new(RichText::new(label).size(12.0).color(Color32::BLACK))
            .min_size(vec2(86.0, 25.0))
            .fill(Color32::from_rgb(251, 251, 251))
            .stroke(Stroke::new(1.0_f32, Color32::from_gray(195))),
    )
}

/// Fixed-height right-aligned button strip; avoids egui's `with_layout(right_to_left)`
/// expanding vertically to the full viewport height inside an auto-sized modal Window.
fn action_buttons_row(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 28.0),
        Layout::right_to_left(Align::Center),
        add_contents,
    );
}

fn tristate_checkbox(ui: &mut Ui, state: &mut AttribTriState, label: &str) {
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::click());
        let stroke = Stroke::new(
            1.0_f32,
            if resp.hovered() {
                Color32::from_rgb(0, 103, 192)
            } else {
                Color32::from_gray(120)
            },
        );
        let fill = match *state {
            AttribTriState::Set => Color32::from_rgb(0, 103, 192),
            AttribTriState::Unchanged => Color32::from_gray(236),
            AttribTriState::Clear => Color32::WHITE,
        };
        ui.painter().rect_filled(rect, 2.0, fill);
        ui.painter().rect_stroke(rect, 2.0, stroke);
        match *state {
            AttribTriState::Set => {
                let c = rect.center();
                let s = Stroke::new(1.7_f32, Color32::WHITE);
                ui.painter()
                    .line_segment([c + vec2(-3.5, 0.2), c + vec2(-1.0, 2.8)], s);
                ui.painter()
                    .line_segment([c + vec2(-1.0, 2.8), c + vec2(3.8, -2.8)], s);
            }
            AttribTriState::Unchanged => {
                ui.painter()
                    .rect_filled(rect.shrink(3.5), 1.0, Color32::from_gray(105));
            }
            AttribTriState::Clear => {}
        }
        let lbl = ui.add(Label::new(RichText::new(label).size(12.0)).sense(Sense::click()));
        if resp.clicked() || lbl.clicked() {
            *state = state.cycle();
        }
    });
}

impl FolderOpModal {
    /// Renders the active modal dialog. Returns `Ok(Some(confirmed))` when accepted,
    /// `Ok(None)` while open, or `Err(())` when cancelled/closed.
    pub fn show(
        &mut self,
        ctx: &Context,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> Result<Option<FolderOpConfirmed>, ()> {
        let esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let enter = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        if esc {
            return Err(());
        }

        // Transparent modal click-catcher (BC5/Win32 dialogs do not dim the parent window).
        let screen = ctx.screen_rect();
        Area::new(Id::new("bc5_folder_op_backdrop"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let _ = ui.allocate_exact_size(screen.size(), Sense::click());
            });

        let (title_key, width): (&str, f32) = match self {
            Self::CompareContents { .. } => ("folder.op.compare_title", 440.0),
            Self::CompareResults(res) => (
                match res.mode {
                    ContentCompareMode::Binary => "folder.op.results_title_binary",
                    ContentCompareMode::Crc32 => "folder.op.results_title_crc",
                    ContentCompareMode::RulesBased => "folder.op.results_title_rules",
                },
                // BC5 TOpCompareResultsDialog default width widened slightly so full localized
                // status bar text fits comfortably without clipping before the resize grip.
                412.0,
            ),
            Self::CopyToSide { .. } => ("menu.actions.copy_to_right", 430.0),
            Self::MoveToSide { .. } => ("folder.ctx.move_to_side_ellipsis", 430.0),
            Self::CopyToFolder { .. } => ("folder.ctx.copy_to_folder_ellipsis", 460.0),
            Self::MoveToFolder { .. } => ("folder.ctx.move_to_folder_ellipsis", 460.0),
            Self::Delete { .. } => ("common.delete", 430.0),
            Self::Rename { .. } => ("common.rename", 460.0),
            Self::Attributes { .. } => ("folder.op.attrib_title", 430.0),
            Self::Touch { .. } => ("folder.op.touch_title", 450.0),
            Self::Exclude { .. } => ("folder.op.exclude_title", 430.0),
            Self::NewFolder { .. } => ("folder.op.new_folder_title", 400.0),
        };
        let title = match self {
            Self::CopyToSide { to_right, .. } => tr(if *to_right {
                "menu.actions.copy_to_right"
            } else {
                "menu.actions.copy_to_left"
            }),
            Self::MoveToSide { .. } => tr("folder.ctx.move_to_side_ellipsis")
                .trim_end_matches("...")
                .to_owned(),
            Self::CopyToFolder { .. } => tr("folder.ctx.copy_to_folder_ellipsis")
                .trim_end_matches("...")
                .to_owned(),
            Self::MoveToFolder { .. } => tr("folder.ctx.move_to_folder_ellipsis")
                .trim_end_matches("...")
                .to_owned(),
            _ => tr(title_key),
        };

        let mut close_cancel = false;
        let mut confirmed: Option<FolderOpConfirmed> = None;

        let is_results_dialog = matches!(self, Self::CompareResults(_));
        let modal_w = width.min(screen.width() - 32.0);
        let mut win = Window::new(title.clone())
            .id(Id::new(if is_results_dialog {
                "bc5_folder_compare_results_modal"
            } else {
                "bc5_folder_op_modal"
            }))
            .title_bar(false)
            .order(Order::Foreground)
            .collapsible(false)
            .frame(
                Frame::none()
                    .fill(if is_results_dialog {
                        Color32::from_rgb(240, 240, 240)
                    } else {
                        BcTheme::WINDOW_BG
                    })
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(168)))
                    .rounding(Rounding::same(6.0))
                    .shadow(epaint::Shadow {
                        offset: vec2(0.0, 4.0),
                        blur: 14.0,
                        spread: 0.0,
                        color: Color32::from_black_alpha(48),
                    })
                    .inner_margin(Margin::ZERO),
            );
        win = if is_results_dialog {
            // Exact BC5 TOpCompareResultsDialog DFM: ClientWidth 377 x ClientHeight 343 (+28 titlebar)
            win.resizable(true)
                .default_size(vec2(modal_w, 371.0_f32.min(screen.height() - 48.0)))
                .min_size(vec2(260.0, 180.0))
                .default_pos(pos2(
                    (screen.center().x - modal_w * 0.5).max(16.0),
                    (screen.center().y - 185.0).max(24.0),
                ))
        } else {
            win.resizable(false)
                .auto_sized()
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        };
        win.show(ctx, |ui| {
            if !is_results_dialog {
                ui.set_width(modal_w);
            }
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            // Native Windows-style dialog title bar: white fill, 12px left-aligned title, × close button.
            let title_h = 28.0;
            let (tb_rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), title_h), Sense::hover());
            let close_w = 34.0;
            let close_rect = Rect::from_min_max(
                pos2(tb_rect.right() - close_w, tb_rect.top()),
                tb_rect.max,
            );
            let close_resp = ui.interact(
                close_rect,
                ui.id().with("bc5_dlg_close"),
                Sense::click(),
            );
            {
                let p = ui.painter();
                p.rect_filled(tb_rect, Rounding { nw: 6.0, ne: 6.0, sw: 0.0, se: 0.0 }, Color32::WHITE);
                let title_clip = Rect::from_min_max(
                    pos2(tb_rect.left() + 10.0, tb_rect.top()),
                    pos2(close_rect.left() - 6.0, tb_rect.bottom()),
                );
                p.with_clip_rect(title_clip).text(
                    pos2(title_clip.left(), tb_rect.center().y),
                    Align2::LEFT_CENTER,
                    &title,
                    FontId::proportional(12.0),
                    Color32::from_gray(35),
                );
                let x_color = if close_resp.hovered() {
                    p.rect_filled(
                        close_rect,
                        Rounding { nw: 0.0, ne: 6.0, sw: 0.0, se: 0.0 },
                        Color32::from_rgb(232, 17, 35),
                    );
                    Color32::WHITE
                } else {
                    Color32::from_gray(65)
                };
                let c = close_rect.center();
                let xs = Stroke::new(1.1_f32, x_color);
                p.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], xs);
                p.line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], xs);
                p.hline(
                    tb_rect.x_range(),
                    tb_rect.bottom(),
                    Stroke::new(1.0_f32, Color32::from_gray(225)),
                );
            }
            if close_resp.clicked() {
                close_cancel = true;
            }

            let body_margin = if is_results_dialog {
                Margin::ZERO
            } else {
                Margin::same(14.0)
            };
            Frame::none()
                .inner_margin(body_margin)
                .show(ui, |ui| {
                    if !is_results_dialog {
                        ui.set_width(modal_w - 28.0);
                    }
                    ui.spacing_mut().item_spacing = if is_results_dialog {
                        vec2(0.0, 0.0)
                    } else {
                        vec2(8.0, 6.0)
                    };
                    ui.visuals_mut().widgets.inactive.bg_stroke =
                        Stroke::new(1.0_f32, Color32::from_gray(150));
                    ui.visuals_mut().widgets.hovered.bg_stroke =
                        Stroke::new(1.0_f32, Color32::from_rgb(0, 103, 192));

                    let is_move_to_folder = matches!(self, Self::MoveToFolder { .. });
                    match self {
                    Self::CompareContents {
                        stats,
                        mode,
                        show_results,
                        dont_show_again,
                        include_hidden,
                    } => {
                        paint_selection_panel(ui, *stats, SchematicSymbol::Compare);
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(RichText::new(tr("folder.op.action_colon")).size(12.0));
                        ui.label(RichText::new(tr("folder.op.compare_desc")).size(12.0));
                        ui.add_space(4.0);
                        for opt in ContentCompareMode::ALL {
                            ui.radio_value(mode, opt, RichText::new(tr(opt.label_key())).size(12.0));
                        }
                        ui.add_space(4.0);
                        ui.checkbox(
                            show_results,
                            RichText::new(tr("folder.op.show_results_dialog")).size(12.0),
                        );
                        if stats.hidden_items > 0 {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(tr("folder.op.filtered_warning"))
                                    .size(11.0)
                                    .color(Color32::from_rgb(130, 80, 0)),
                            );
                            ui.checkbox(
                                include_hidden,
                                RichText::new(tr("folder.op.include_hidden")).size(12.0),
                            );
                        }
                        ui.add_space(8.0);
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("folder.op.start"), true).clicked()
                                || enter
                            {
                                confirmed = Some(FolderOpConfirmed::CompareContents {
                                    mode: *mode,
                                    show_results: *show_results,
                                    dont_show_again: *dont_show_again,
                                    include_hidden: *include_hidden,
                                });
                            }
                        });
                        ui.add_space(4.0);
                        ui.separator();
                        ui.checkbox(
                            dont_show_again,
                            RichText::new(tr("folder.op.dont_show_again")).size(12.0),
                        );
                    }
                    Self::CompareResults(results) => {
                        results.ensure_default_selection();
                        let is_binary = results.mode == ContentCompareMode::Binary;
                        let categories: [(usize, CompareRowStatus, usize, &str); 6] = [
                            (0, CompareRowStatus::Different, results.different, "folder.op.results_diff_files"),
                            (1, CompareRowStatus::Minor, results.minor, "folder.op.results_minor_files"),
                            (2, CompareRowStatus::Same, results.same, "folder.op.results_same_files"),
                            (3, CompareRowStatus::LeftOrphan, results.left_orphans, "folder.op.results_left_orphan_files"),
                            (4, CompareRowStatus::RightOrphan, results.right_orphans, "folder.op.results_right_orphan_files"),
                            (5, CompareRowStatus::Error, results.errors, "folder.op.results_error_files"),
                        ];
                        #[derive(Clone, Copy)]
                        enum TreeItem {
                            Category {
                                cat_idx: usize,
                                status: CompareRowStatus,
                                count: usize,
                                key: &'static str,
                            },
                            File {
                                cat_idx: usize,
                                status: CompareRowStatus,
                                row_idx: usize,
                                is_last: bool,
                            },
                        }
                        let mut visible: Vec<TreeItem> = Vec::new();
                        for (cat_idx, status, count, key) in categories {
                            if count == 0 {
                                continue;
                            }
                            visible.push(TreeItem::Category {
                                cat_idx,
                                status,
                                count,
                                key,
                            });
                            if results.expanded[cat_idx] {
                                let matching: Vec<usize> = results
                                    .rows
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, r)| (r.status == status).then_some(i))
                                    .collect();
                                let last_pos = matching.len().saturating_sub(1);
                                for (k, row_idx) in matching.into_iter().enumerate() {
                                    visible.push(TreeItem::File {
                                        cat_idx,
                                        status,
                                        row_idx,
                                        is_last: k == last_pos,
                                    });
                                }
                            }
                        }

                        let orphan_purple = Color32::from_rgb(128, 0, 255);
                        let sel_fill = Color32::from_rgb(204, 232, 255);
                        let sel_stroke = Stroke::new(1.0_f32, Color32::from_rgb(153, 209, 255));
                        let dot_color = Color32::from_gray(172);
                        let row_h = 18.0;
                        let icon_w = 28.0;
                        let icon_h = 16.0;
                        let status_bar_h = 22.0;
                        let tree_h = (ui.available_height() - status_bar_h).max(120.0);

                        let status_icon = |icons: &mut IconManager,
                                           ctx: &Context,
                                           status: CompareRowStatus|
                         -> Option<TextureHandle> {
                            match status {
                                CompareRowStatus::Different => icons.get_icon(
                                    ctx,
                                    if is_binary {
                                        "imgContentDifferentBinary"
                                    } else {
                                        "imgContentDifferent"
                                    },
                                ),
                                CompareRowStatus::Minor => {
                                    icons.get_icon(ctx, "imgContentSimilar")
                                }
                                CompareRowStatus::Same => icons.get_icon(
                                    ctx,
                                    if is_binary {
                                        "imgContentSameBinary"
                                    } else {
                                        "imgContentSame"
                                    },
                                ),
                                CompareRowStatus::LeftOrphan => icons
                                    .content_model(ctx, "imgContentSpotLeft", orphan_purple)
                                    .or_else(|| icons.get_icon(ctx, "imgContentSpotLeft")),
                                CompareRowStatus::RightOrphan => icons
                                    .content_model(ctx, "imgContentSpotRight", orphan_purple)
                                    .or_else(|| icons.get_icon(ctx, "imgContentSpotRight")),
                                CompareRowStatus::Error => icons.get_icon(ctx, "imgContentError"),
                            }
                        };

                        fn draw_dotted_vline(p: &Painter, x: f32, y0: f32, y1: f32, color: Color32) {
                            let xi = x.round();
                            let mut y = y0.round();
                            if (y as i32) % 2 != 0 {
                                y += 1.0;
                            }
                            while y <= y1 {
                                p.rect_filled(
                                    Rect::from_min_size(pos2(xi, y), vec2(1.0, 1.0)),
                                    0.0,
                                    color,
                                );
                                y += 2.0;
                            }
                        }

                        fn draw_dotted_hline(p: &Painter, x0: f32, x1: f32, y: f32, color: Color32) {
                            let yi = y.round();
                            let mut x = x0.round();
                            if (x as i32) % 2 != 0 {
                                x += 1.0;
                            }
                            while x <= x1 {
                                p.rect_filled(
                                    Rect::from_min_size(pos2(x, yi), vec2(1.0, 1.0)),
                                    0.0,
                                    color,
                                );
                                x += 2.0;
                            }
                        }

                        fn layout_truncated(
                            p: &Painter,
                            text: &str,
                            font: FontId,
                            color: Color32,
                            max_w: f32,
                        ) -> std::sync::Arc<Galley> {
                            let full = p.layout_no_wrap(text.to_owned(), font.clone(), color);
                            if full.size().x <= max_w || max_w <= 20.0 {
                                return full;
                            }
                            let chars: Vec<char> = text.chars().collect();
                            let (mut lo, mut hi) = (0usize, chars.len());
                            while lo < hi {
                                let mid = (lo + hi).div_ceil(2);
                                let mut cand: String = chars[..mid].iter().collect();
                                cand.push_str("...");
                                let w = p.layout_no_wrap(cand, font.clone(), color).size().x;
                                if w <= max_w {
                                    lo = mid;
                                } else {
                                    hi = mid - 1;
                                }
                            }
                            let mut out: String = chars[..lo].iter().collect();
                            out.push_str("...");
                            p.layout_no_wrap(out, font, color)
                        }

                        Frame::none()
                            .fill(Color32::WHITE)
                            .inner_margin(Margin::symmetric(2.0, 2.0))
                            .show(ui, |ui| {
                                ui.set_min_size(vec2(ui.available_width(), tree_h - 4.0));
                                ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                                ScrollArea::vertical()
                                    .id_salt("compare_results_tree")
                                    .max_height(tree_h - 4.0)
                                    .auto_shrink([false, false])
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                    )
                                    .show_rows(ui, row_h, visible.len().max(1), |ui, range| {
                                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                                        if visible.is_empty() {
                                            let label = tr_fmt(
                                                "folder.op.results_same_files",
                                                &[("count", "0")],
                                            );
                                            ui.label(
                                                RichText::new(label)
                                                    .size(12.0)
                                                    .color(Color32::from_gray(110)),
                                            );
                                            return;
                                        }
                                        let font = FontId::proportional(12.0);
                                        for idx in range {
                                            let item = visible[idx];
                                            match item {
                                                TreeItem::Category {
                                                    cat_idx,
                                                    status,
                                                    count,
                                                    key,
                                                } => {
                                                    let is_sel =
                                                        results.selected == Some((cat_idx, None));
                                                    let expanded = results.expanded[cat_idx];
                                                    let (rect, resp) = ui.allocate_exact_size(
                                                        vec2(ui.available_width(), row_h),
                                                        Sense::click(),
                                                    );
                                                    let label = tr_fmt(
                                                        key,
                                                        &[("count", &count.to_string())],
                                                    );
                                                    let icon_tex =
                                                        status_icon(icons, ui.ctx(), status);
                                                    let p = ui.painter();
                                                    let chev_rect = Rect::from_min_size(
                                                        pos2(rect.left() + 3.0, rect.top() + 1.0),
                                                        vec2(13.0, row_h - 2.0),
                                                    );
                                                    let c = chev_rect.center();
                                                    if expanded {
                                                        // Crisp BC5/Windows thin downward chevron (v)
                                                        let s = Stroke::new(
                                                            1.1_f32,
                                                            Color32::from_gray(75),
                                                        );
                                                        p.line_segment(
                                                            [
                                                                pos2(c.x - 3.5, c.y - 1.5),
                                                                pos2(c.x, c.y + 2.0),
                                                            ],
                                                            s,
                                                        );
                                                        p.line_segment(
                                                            [
                                                                pos2(c.x, c.y + 2.0),
                                                                pos2(c.x + 3.5, c.y - 1.5),
                                                            ],
                                                            s,
                                                        );
                                                    } else {
                                                        let s = Stroke::new(
                                                            1.1_f32,
                                                            Color32::from_gray(115),
                                                        );
                                                        p.line_segment(
                                                            [
                                                                pos2(c.x - 1.5, c.y - 3.5),
                                                                pos2(c.x + 2.0, c.y),
                                                            ],
                                                            s,
                                                        );
                                                        p.line_segment(
                                                            [
                                                                pos2(c.x + 2.0, c.y),
                                                                pos2(c.x - 1.5, c.y + 3.5),
                                                            ],
                                                            s,
                                                        );
                                                    }
                                                    let icon_x = rect.left() + 18.0;
                                                    if let Some(tex) = icon_tex {
                                                        let ir = Rect::from_min_size(
                                                            pos2(
                                                                icon_x,
                                                                rect.center().y - icon_h * 0.5,
                                                            ),
                                                            vec2(icon_w, icon_h),
                                                        );
                                                        p.image(
                                                            tex.id(),
                                                            ir,
                                                            Rect::from_min_max(
                                                                pos2(0.0, 0.0),
                                                                pos2(1.0, 1.0),
                                                            ),
                                                            Color32::WHITE,
                                                        );
                                                    }
                                                    let text_x = icon_x + icon_w + 3.0;
                                                    let max_w =
                                                        (rect.right() - text_x - 6.0).max(24.0);
                                                    let galley = layout_truncated(
                                                        p,
                                                        &label,
                                                        font.clone(),
                                                        Color32::BLACK,
                                                        max_w,
                                                    );
                                                    if is_sel {
                                                        let hl = Rect::from_min_max(
                                                            pos2(text_x - 2.0, rect.top() + 1.0),
                                                            pos2(
                                                                (text_x + galley.size().x + 3.0)
                                                                    .min(rect.right() - 2.0),
                                                                rect.bottom() - 1.0,
                                                            ),
                                                        );
                                                        p.rect_filled(hl, 0.0, sel_fill);
                                                        p.rect_stroke(hl, 0.0, sel_stroke);
                                                    }
                                                    p.galley(
                                                        pos2(
                                                            text_x,
                                                            rect.center().y
                                                                - galley.size().y * 0.5,
                                                        ),
                                                        galley,
                                                        Color32::BLACK,
                                                    );
                                                    if resp.double_clicked() {
                                                        results.expanded[cat_idx] =
                                                            !results.expanded[cat_idx];
                                                        results.selected = Some((cat_idx, None));
                                                    } else if resp.clicked() {
                                                        results.selected = Some((cat_idx, None));
                                                        if let Some(pos) =
                                                            resp.interact_pointer_pos()
                                                        {
                                                            if pos.x <= icon_x {
                                                                results.expanded[cat_idx] =
                                                                    !results.expanded[cat_idx];
                                                            }
                                                        }
                                                    }
                                                }
                                                TreeItem::File {
                                                    cat_idx,
                                                    status,
                                                    row_idx,
                                                    is_last,
                                                } => {
                                                    let row = &results.rows[row_idx];
                                                    let is_sel = results.selected
                                                        == Some((cat_idx, Some(row_idx)));
                                                    let (rect, resp) = ui.allocate_exact_size(
                                                        vec2(ui.available_width(), row_h),
                                                        Sense::click(),
                                                    );
                                                    let icon_tex =
                                                        status_icon(icons, ui.ctx(), status);
                                                    let p = ui.painter();
                                                    // Exact BC5 Win32 TreeView connector lines + native 28x16 child icon
                                                    let tree_x = rect.left() + 26.0;
                                                    let child_icon_x = rect.left() + 36.0;
                                                    let v_end = if is_last {
                                                        rect.center().y
                                                    } else {
                                                        rect.bottom()
                                                    };
                                                    draw_dotted_vline(
                                                        p,
                                                        tree_x,
                                                        rect.top(),
                                                        v_end,
                                                        dot_color,
                                                    );
                                                    draw_dotted_hline(
                                                        p,
                                                        tree_x,
                                                        child_icon_x - 2.0,
                                                        rect.center().y,
                                                        dot_color,
                                                    );
                                                    if let Some(tex) = icon_tex {
                                                        let ir = Rect::from_min_size(
                                                            pos2(
                                                                child_icon_x,
                                                                rect.center().y - icon_h * 0.5,
                                                            ),
                                                            vec2(icon_w, icon_h),
                                                        );
                                                        p.image(
                                                            tex.id(),
                                                            ir,
                                                            Rect::from_min_max(
                                                                pos2(0.0, 0.0),
                                                                pos2(1.0, 1.0),
                                                            ),
                                                            Color32::WHITE,
                                                        );
                                                    }
                                                    let text_x = child_icon_x + icon_w + 3.0;
                                                    let text = if row.status == CompareRowStatus::Error
                                                        && !row.detail.is_empty()
                                                    {
                                                        format!(
                                                            "{} ({})",
                                                            row.relative.display(),
                                                            row.detail
                                                        )
                                                    } else {
                                                        row.relative.display().to_string()
                                                    };
                                                    let max_w =
                                                        (rect.right() - text_x - 6.0).max(24.0);
                                                    let galley = layout_truncated(
                                                        p,
                                                        &text,
                                                        font.clone(),
                                                        Color32::BLACK,
                                                        max_w,
                                                    );
                                                    if is_sel {
                                                        let hl = Rect::from_min_max(
                                                            pos2(text_x - 2.0, rect.top() + 1.0),
                                                            pos2(
                                                                (text_x + galley.size().x + 3.0)
                                                                    .min(rect.right() - 2.0),
                                                                rect.bottom() - 1.0,
                                                            ),
                                                        );
                                                        p.rect_filled(hl, 0.0, sel_fill);
                                                        p.rect_stroke(hl, 0.0, sel_stroke);
                                                    }
                                                    p.galley(
                                                        pos2(
                                                            text_x,
                                                            rect.center().y
                                                                - galley.size().y * 0.5,
                                                        ),
                                                        galley,
                                                        Color32::BLACK,
                                                    );
                                                    if resp.clicked() {
                                                        results.selected =
                                                            Some((cat_idx, Some(row_idx)));
                                                    }
                                                    if resp.double_clicked() {
                                                        results.selected =
                                                            Some((cat_idx, Some(row_idx)));
                                                        confirmed =
                                                            Some(FolderOpConfirmed::OpenResultRow {
                                                                left_rel: row.relative.clone(),
                                                                right_rel: row
                                                                    .right_relative
                                                                    .clone(),
                                                                status: row.status,
                                                            });
                                                    }
                                                }
                                            }
                                        }
                                    });
                            });
                        // Native BC5/Win32 bottom status bar with subtle corner grip
                        let (sb_rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), status_bar_h),
                            Sense::hover(),
                        );
                        let p = ui.painter();
                        p.rect_filled(sb_rect, 0.0, Color32::from_rgb(240, 240, 240));
                        p.hline(
                            sb_rect.x_range(),
                            sb_rect.top(),
                            Stroke::new(1.0_f32, Color32::from_gray(205)),
                        );
                        let sb_clip = Rect::from_min_max(
                            pos2(sb_rect.left() + 6.0, sb_rect.top()),
                            pos2(sb_rect.right() - 18.0, sb_rect.bottom()),
                        );
                        p.with_clip_rect(sb_clip).text(
                            pos2(sb_clip.left(), sb_rect.center().y),
                            Align2::LEFT_CENTER,
                            tr("folder.op.results_double_click_hint"),
                            FontId::proportional(11.5),
                            Color32::from_gray(30),
                        );
                        if enter {
                            if let Some((_, Some(row_idx))) = results.selected {
                                if let Some(row) = results.rows.get(row_idx) {
                                    confirmed = Some(FolderOpConfirmed::OpenResultRow {
                                        left_rel: row.relative.clone(),
                                        right_rel: row.right_relative.clone(),
                                        status: row.status,
                                    });
                                }
                            } else {
                                close_cancel = true;
                            }
                        }
                    }
                    Self::CopyToSide {
                        stats,
                        to_right,
                        overwrite_count,
                        dont_show_again,
                    } => {
                        let sym = if *to_right {
                            SchematicSymbol::CopyRight
                        } else {
                            SchematicSymbol::CopyLeft
                        };
                        paint_selection_panel(ui, *stats, sym);
                        ui.separator();
                        ui.label(RichText::new(tr("folder.op.action_colon")).size(12.0));
                        ui.label(RichText::new(tr("folder.op.copy_desc")).size(12.0));
                        if *overwrite_count > 0 {
                            ui.label(
                                RichText::new(format!(
                                    "{} {}",
                                    overwrite_count,
                                    tr("folder.op.different").to_lowercase()
                                ))
                                .size(11.5)
                                .color(Color32::from_rgb(155, 65, 0)),
                            );
                        }
                        ui.add_space(6.0);
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("folder.op.start"), true).clicked()
                                || enter
                            {
                                confirmed = Some(FolderOpConfirmed::CopyToSide {
                                    to_right: *to_right,
                                    dont_show_again: *dont_show_again,
                                });
                            }
                        });
                        ui.separator();
                        ui.checkbox(
                            dont_show_again,
                            RichText::new(tr("folder.op.dont_show_again")).size(12.0),
                        );
                    }
                    Self::MoveToSide {
                        stats,
                        to_right,
                        overwrite_count,
                        dont_show_again,
                    } => {
                        let sym = if *to_right {
                            SchematicSymbol::MoveRight
                        } else {
                            SchematicSymbol::MoveLeft
                        };
                        paint_selection_panel(ui, *stats, sym);
                        ui.separator();
                        ui.label(RichText::new(tr("folder.op.action_colon")).size(12.0));
                        ui.label(RichText::new(tr("folder.op.move_desc")).size(12.0));
                        if *overwrite_count > 0 {
                            ui.label(
                                RichText::new(format!(
                                    "{} {}",
                                    overwrite_count,
                                    tr("folder.op.different").to_lowercase()
                                ))
                                .size(11.5)
                                .color(Color32::from_rgb(155, 65, 0)),
                            );
                        }
                        ui.add_space(6.0);
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("folder.op.start"), true).clicked()
                                || enter
                            {
                                confirmed = Some(FolderOpConfirmed::MoveToSide {
                                    to_right: *to_right,
                                    dont_show_again: *dont_show_again,
                                });
                            }
                        });
                        ui.separator();
                        ui.checkbox(
                            dont_show_again,
                            RichText::new(tr("folder.op.dont_show_again")).size(12.0),
                        );
                    }
                    Self::CopyToFolder {
                        stats,
                        source_left,
                        target_dir,
                        structure,
                    }
                    | Self::MoveToFolder {
                        stats,
                        source_left,
                        target_dir,
                        structure,
                    } => {
                        let is_move = is_move_to_folder;
                        paint_selection_panel(ui, *stats, SchematicSymbol::FolderOut);
                        ui.separator();
                        ui.label(RichText::new(tr("folder.op.action_colon")).size(12.0));
                        ui.label(
                            RichText::new(tr(if is_move {
                                "folder.op.move_to_desc"
                            } else {
                                "folder.op.copy_to_desc"
                            }))
                            .size(12.0),
                        );
                        ui.horizontal(|ui| {
                            ui.add(
                                TextEdit::singleline(target_dir)
                                    .desired_width(ui.available_width() - 36.0),
                            );
                            if ui.button("...").clicked() {
                                if let Some(picked) =
                                    dialogs.folder(*source_left, Path::new(target_dir.as_str()))
                                {
                                    *target_dir = picked.to_string_lossy().into_owned();
                                }
                            }
                        });
                        ui.add_space(4.0);
                        ui.label(RichText::new(tr("folder.op.folder_structure")).size(12.0));
                        ui.radio_value(
                            structure,
                            FolderStructureMode::KeepRelative,
                            RichText::new(tr("folder.op.keep_relative")).size(12.0),
                        );
                        ui.radio_value(
                            structure,
                            FolderStructureMode::KeepBase,
                            RichText::new(tr("folder.op.keep_base")).size(12.0),
                        );
                        ui.radio_value(
                            structure,
                            FolderStructureMode::Flatten,
                            RichText::new(tr("folder.op.flatten_structure")).size(12.0),
                        );
                        ui.add_space(6.0);
                        let valid = !target_dir.trim().is_empty();
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("folder.op.start"), valid).clicked()
                                || (enter && valid)
                            {
                                let dst = PathBuf::from(target_dir.trim());
                                confirmed = Some(if is_move {
                                    FolderOpConfirmed::MoveToFolder {
                                        source_left: *source_left,
                                        target_dir: dst,
                                        structure: *structure,
                                    }
                                } else {
                                    FolderOpConfirmed::CopyToFolder {
                                        source_left: *source_left,
                                        target_dir: dst,
                                        structure: *structure,
                                    }
                                });
                            }
                        });
                    }
                    Self::Delete {
                        stats,
                        delete_left,
                        delete_right,
                        use_recycle_bin,
                    } => {
                        paint_selection_panel(ui, *stats, SchematicSymbol::Delete);
                        ui.horizontal(|ui| {
                            ui.add_enabled(
                                stats.left_files + stats.left_dirs > 0,
                                Checkbox::new(delete_left, tr("folder.op.left_side")),
                            );
                            ui.add_space(16.0);
                            ui.add_enabled(
                                stats.right_files + stats.right_dirs > 0,
                                Checkbox::new(delete_right, tr("folder.op.right_side")),
                            );
                        });
                        ui.separator();
                        ui.label(RichText::new(tr("folder.op.action_colon")).size(12.0));
                        ui.label(RichText::new(tr("folder.op.delete_desc")).size(12.0));
                        ui.checkbox(
                            use_recycle_bin,
                            RichText::new(tr("folder.op.use_recycle_bin")).size(12.0),
                        );
                        ui.add_space(6.0);
                        let valid = (*delete_left && stats.left_files + stats.left_dirs > 0)
                            || (*delete_right && stats.right_files + stats.right_dirs > 0);
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("common.delete"), valid).clicked()
                                || (enter && valid)
                            {
                                confirmed = Some(FolderOpConfirmed::Delete {
                                    delete_left: *delete_left,
                                    delete_right: *delete_right,
                                    use_recycle_bin: *use_recycle_bin,
                                });
                            }
                        });
                    }
                    Self::Rename {
                        old_mask,
                        new_mask,
                        mode,
                        sample_names,
                        single_item,
                    } => {
                        if !*single_item {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(tr("folder.op.rename_old_mask")).size(12.0));
                                ui.add(
                                    TextEdit::singleline(old_mask)
                                        .desired_width(ui.available_width()),
                                );
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr(if *single_item {
                                "folder.new_name"
                            } else {
                                "folder.op.rename_new_mask"
                            }))
                            .size(12.0));
                            ui.add(
                                TextEdit::singleline(new_mask).desired_width(ui.available_width()),
                            );
                        });
                        if !*single_item {
                            ui.horizontal(|ui| {
                                ui.radio_value(
                                    mode,
                                    RenameMode::Dos,
                                    RichText::new(tr("folder.op.rename_dos")).size(12.0),
                                );
                                ui.radio_value(
                                    mode,
                                    RenameMode::Regex,
                                    RichText::new(tr("folder.op.rename_regex")).size(12.0),
                                );
                            });
                            ui.label(RichText::new(tr("folder.op.preview")).size(11.5).strong());
                            Frame::none()
                                .fill(Color32::WHITE)
                                .stroke(Stroke::new(1.0_f32, Color32::from_gray(195)))
                                .inner_margin(Margin::same(6.0))
                                .show(ui, |ui| {
                                    ScrollArea::vertical()
                                        .id_salt("rename_preview")
                                        .max_height(110.0)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            for name in sample_names.iter().take(20) {
                                                let next = apply_rename_mask(
                                                    name, old_mask, new_mask, *mode,
                                                )
                                                .unwrap_or_else(|_| name.clone());
                                                ui.label(
                                                    RichText::new(format!("{name}  →  {next}"))
                                                        .size(11.5),
                                                );
                                            }
                                        });
                                });
                        }
                        ui.add_space(6.0);
                        let valid = !new_mask.trim().is_empty();
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("common.ok"), valid).clicked()
                                || (enter && valid)
                            {
                                confirmed = Some(FolderOpConfirmed::Rename {
                                    old_mask: old_mask.clone(),
                                    new_mask: new_mask.clone(),
                                    mode: *mode,
                                });
                            }
                        });
                    }
                    Self::Attributes {
                        stats,
                        left_side,
                        spec,
                    } => {
                        paint_selection_panel(ui, *stats, SchematicSymbol::Compare);
                        ui.separator();
                        ui.label(RichText::new(tr("folder.op.action_colon")).size(12.0));
                        ui.label(RichText::new(tr("folder.op.attrib_desc")).size(11.5));
                        ui.add_space(4.0);
                        tristate_checkbox(ui, &mut spec.read_only, &tr("folder.op.attrib_readonly"));
                        tristate_checkbox(ui, &mut spec.hidden, &tr("folder.op.attrib_hidden"));
                        tristate_checkbox(ui, &mut spec.archive, &tr("folder.op.attrib_archive"));
                        tristate_checkbox(ui, &mut spec.system, &tr("folder.op.attrib_system"));
                        ui.add_space(4.0);
                        ui.checkbox(
                            &mut spec.recursive,
                            RichText::new(tr("folder.op.include_folder_contents")).size(12.0),
                        );
                        ui.add_space(6.0);
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("common.ok"), true).clicked() || enter {
                                confirmed = Some(FolderOpConfirmed::Attributes {
                                    left_side: *left_side,
                                    spec: *spec,
                                });
                            }
                        });
                    }
                    Self::Touch {
                        stats,
                        left_side,
                        mode,
                        datetime_text,
                        recursive,
                        can_copy_other,
                    } => {
                        paint_selection_panel(ui, *stats, SchematicSymbol::Compare);
                        ui.separator();
                        ui.label(RichText::new(tr("folder.op.action_colon")).size(12.0));
                        ui.checkbox(
                            recursive,
                            RichText::new(tr("folder.op.include_folder_contents")).size(12.0),
                        );
                        ui.add_enabled(
                            *can_copy_other,
                            RadioButton::new(
                                *mode == TouchMode::CopyOtherSide,
                                RichText::new(tr("folder.op.touch_copy_other")).size(12.0),
                            ),
                        )
                        .clicked()
                        .then(|| *mode = TouchMode::CopyOtherSide);
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                mode,
                                TouchMode::SetDateTime,
                                RichText::new(tr("folder.op.touch_set_time")).size(12.0),
                            );
                            ui.add_enabled(
                                *mode == TouchMode::SetDateTime,
                                TextEdit::singleline(datetime_text).desired_width(145.0),
                            );
                        });
                        if *mode == TouchMode::SetDateTime
                            && ui.button(tr("folder.op.touch_now")).clicked()
                        {
                            *datetime_text = Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
                        }
                        ui.add_space(6.0);
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("common.ok"), true).clicked() || enter {
                                confirmed = Some(FolderOpConfirmed::Touch {
                                    left_side: *left_side,
                                    mode: *mode,
                                    datetime_text: datetime_text.clone(),
                                    recursive: *recursive,
                                });
                            }
                        });
                    }
                    Self::Exclude {
                        names,
                        relative_paths,
                        only_this_folder,
                    } => {
                        ui.label(RichText::new(tr("folder.op.exclude_selected")).strong().size(12.0));
                        ui.checkbox(
                            only_this_folder,
                            RichText::new(tr("folder.op.exclude_only_this_folder")).size(12.0),
                        );
                        let patterns: Vec<String> = if *only_this_folder {
                            relative_paths.clone()
                        } else {
                            names.clone()
                        };
                        Frame::none()
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0_f32, Color32::from_gray(195)))
                            .inner_margin(Margin::same(6.0))
                            .show(ui, |ui| {
                                ScrollArea::vertical()
                                    .id_salt("exclude_preview")
                                    .max_height(110.0)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        for pat in patterns.iter().take(24) {
                                            ui.label(RichText::new(format!("-{pat}")).size(11.5));
                                        }
                                    });
                            });
                        ui.add_space(6.0);
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("common.ok"), !patterns.is_empty())
                                .clicked()
                                || (enter && !patterns.is_empty())
                            {
                                confirmed = Some(FolderOpConfirmed::Exclude { patterns });
                            }
                        });
                    }
                    Self::NewFolder {
                        name,
                        create_left,
                        create_right,
                        left_writable,
                        right_writable,
                    } => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr("folder.folder_name")).size(12.0));
                            ui.add(TextEdit::singleline(name).desired_width(ui.available_width()));
                        });
                        ui.label(RichText::new(tr("folder.op.create_in")).size(12.0));
                        ui.horizontal(|ui| {
                            ui.add_enabled(
                                *left_writable,
                                Checkbox::new(create_left, tr("folder.op.left_side")),
                            );
                            ui.add_space(12.0);
                            ui.add_enabled(
                                *right_writable,
                                Checkbox::new(create_right, tr("folder.op.right_side")),
                            );
                        });
                        ui.add_space(6.0);
                        let valid = !name.trim().is_empty() && (*create_left || *create_right);
                        action_buttons_row(ui, |ui| {
                            if paint_secondary_button(ui, &tr("common.cancel")).clicked() {
                                close_cancel = true;
                            }
                            if paint_primary_button(ui, &tr("common.ok"), valid).clicked()
                                || (enter && valid)
                            {
                                confirmed = Some(FolderOpConfirmed::NewFolder {
                                    name: name.trim().to_owned(),
                                    create_left: *create_left,
                                    create_right: *create_right,
                                });
                            }
                        });
                    }
                }
                });
            });

        if close_cancel {
            Err(())
        } else {
            Ok(confirmed)
        }
    }
}

/// Applies DOS wildcard (`*` / `?`) or Regular Expression rename masks to a single filename.
pub fn apply_rename_mask(
    name: &str,
    old_mask: &str,
    new_mask: &str,
    mode: RenameMode,
) -> Result<String, String> {
    let old_mask = old_mask.trim();
    let new_mask = new_mask.trim();
    if new_mask.is_empty() {
        return Ok(name.to_owned());
    }
    match mode {
        RenameMode::Regex => {
            if old_mask.is_empty() {
                return Ok(new_mask.to_owned());
            }
            let re = regex::Regex::new(old_mask).map_err(|e| e.to_string())?;
            Ok(re.replace_all(name, new_mask).into_owned())
        }
        RenameMode::Dos => {
            if !old_mask.contains(['*', '?']) && !new_mask.contains(['*', '?']) {
                return Ok(new_mask.to_owned());
            }
            // Convert DOS wildcard mask into regex capture groups for each `*` and `?`.
            let mut pattern = String::from("^");
            for ch in old_mask.chars() {
                match ch {
                    '*' => pattern.push_str("(.*)"),
                    '?' => pattern.push_str("(.)"),
                    other => pattern.push_str(&regex::escape(&other.to_string())),
                }
            }
            pattern.push('$');
            let re = regex::RegexBuilder::new(&pattern)
                .case_insensitive(cfg!(windows))
                .build()
                .map_err(|e| e.to_string())?;
            let Some(caps) = re.captures(name) else {
                return Ok(name.to_owned());
            };
            let mut out = String::new();
            let mut group = 1usize;
            for ch in new_mask.chars() {
                match ch {
                    '*' | '?' => {
                        if let Some(m) = caps.get(group) {
                            out.push_str(m.as_str());
                        }
                        group += 1;
                    }
                    other => out.push(other),
                }
            }
            Ok(out)
        }
    }
}

/// Parses user-entered date/time strings (`DD/MM/YYYY HH:MM:SS` or `YYYY-MM-DD HH:MM:SS`).
pub fn parse_touch_datetime(input: &str) -> Result<SystemTime, String> {
    let s = input.trim();
    for fmt in [
        "%d/%m/%Y %H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%d/%m/%Y %H:%M",
        "%Y-%m-%d %H:%M",
        "%d/%m/%Y",
        "%Y-%m-%d",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            if let Some(dt) = Local.from_local_datetime(&naive).earliest() {
                return Ok(SystemTime::from(dt));
            }
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(s, fmt) {
            if let Some(naive) = date.and_hms_opt(0, 0, 0) {
                if let Some(dt) = Local.from_local_datetime(&naive).earliest() {
                    return Ok(SystemTime::from(dt));
                }
            }
        }
    }
    Err(format!("Formato de data/hora inválido: {s}"))
}

pub fn format_datetime(time: SystemTime) -> String {
    let dt: DateTime<Local> = time.into();
    dt.format("%d/%m/%Y %H:%M:%S").to_string()
}

/// Applies modification timestamp to a file (or directory on Windows).
pub fn apply_touch_time(path: &Path, time: SystemTime) -> Result<(), String> {
    if path.is_file() {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        file.set_modified(time)
            .map_err(|e| format!("{}: {e}", path.display()))
    } else {
        #[cfg(windows)]
        {
            set_windows_mtime(path, time)
        }
        #[cfg(not(windows))]
        {
            let _ = (path, time);
            Ok(())
        }
    }
}

#[cfg(windows)]
fn set_windows_mtime(path: &Path, time: SystemTime) -> Result<(), String> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_WRITE_ATTRIBUTES,
    };
    let dir = fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    dir.set_modified(time)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads current DOS attributes across paths and initializes tri-state flags.
pub fn inspect_dos_attributes(paths: &[PathBuf]) -> DosAttributesSpec {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileAttributesW, FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN,
            FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM, INVALID_FILE_ATTRIBUTES,
        };
        let mut all_mask = u32::MAX;
        let mut any_mask = 0u32;
        let mut count = 0usize;
        for path in paths {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: wide is NUL-terminated.
            let attr = unsafe { GetFileAttributesW(wide.as_ptr()) };
            if attr != INVALID_FILE_ATTRIBUTES {
                all_mask &= attr;
                any_mask |= attr;
                count += 1;
            }
        }
        if count == 0 {
            return DosAttributesSpec::default();
        }
        let state = |flag: u32| {
            if all_mask & flag != 0 {
                AttribTriState::Set
            } else if any_mask & flag == 0 {
                AttribTriState::Clear
            } else {
                AttribTriState::Unchanged
            }
        };
        DosAttributesSpec {
            read_only: state(FILE_ATTRIBUTE_READONLY),
            hidden: state(FILE_ATTRIBUTE_HIDDEN),
            archive: state(FILE_ATTRIBUTE_ARCHIVE),
            system: state(FILE_ATTRIBUTE_SYSTEM),
            recursive: true,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        DosAttributesSpec::default()
    }
}

/// Applies DOS attributes to a file/directory path on Windows.
pub fn apply_dos_attributes(path: &Path, spec: DosAttributesSpec) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN,
            FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM,
            INVALID_FILE_ATTRIBUTES,
        };
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: wide is NUL-terminated.
        let current = unsafe { GetFileAttributesW(wide.as_ptr()) };
        if current == INVALID_FILE_ATTRIBUTES {
            return Err(format!(
                "{}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        let mut next = current;
        for (state, flag) in [
            (spec.read_only, FILE_ATTRIBUTE_READONLY),
            (spec.hidden, FILE_ATTRIBUTE_HIDDEN),
            (spec.archive, FILE_ATTRIBUTE_ARCHIVE),
            (spec.system, FILE_ATTRIBUTE_SYSTEM),
        ] {
            match state {
                AttribTriState::Set => next |= flag,
                AttribTriState::Clear => next &= !flag,
                AttribTriState::Unchanged => {}
            }
        }
        if next == 0 {
            next = FILE_ATTRIBUTE_NORMAL;
        }
        // SAFETY: wide is NUL-terminated.
        let ok = unsafe { SetFileAttributesW(wide.as_ptr(), next) };
        if ok == 0 {
            return Err(format!(
                "{}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .permissions();
        match spec.read_only {
            AttribTriState::Set => perms.set_readonly(true),
            AttribTriState::Clear => {
                let mode = perms.mode() | 0o200;
                perms.set_mode(mode);
            }
            AttribTriState::Unchanged => {}
        }
        fs::set_permissions(path, perms).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Computes the target path for Copy/Move to Folder respecting BC5's 3 structure modes.
pub fn resolve_external_target(
    target_root: &Path,
    source_root: &Path,
    relative: &Path,
    structure: FolderStructureMode,
) -> PathBuf {
    match structure {
        FolderStructureMode::KeepRelative => target_root.join(relative),
        FolderStructureMode::KeepBase => {
            let base = source_root
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_default();
            target_root.join(base).join(relative)
        }
        FolderStructureMode::Flatten => target_root.join(
            relative
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| relative.to_path_buf()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_binary_and_rules_modes_distinguish_line_endings_and_whitespace() {
        let root = std::env::temp_dir().join(format!(
            "anycompare-folder-ops-modes-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();

        // File 1: identical text with CRLF vs LF
        fs::write(left.join("eol.txt"), b"hello\r\nworld\r\n").unwrap();
        fs::write(right.join("eol.txt"), b"hello\nworld\n").unwrap();

        // File 2: whitespace-only change
        fs::write(left.join("ws.txt"), b"alpha   beta\n").unwrap();
        fs::write(right.join("ws.txt"), b"alpha beta\n").unwrap();

        // File 3: exact binary match
        fs::write(left.join("exact.bin"), b"\x00\x01\x02\x03").unwrap();
        fs::write(right.join("exact.bin"), b"\x00\x01\x02\x03").unwrap();

        let cancel = AtomicBool::new(false);
        let meta = |p: PathBuf| EntryMeta::from_fs(&fs::metadata(p).unwrap());

        // CRLF vs LF: Binary = Different, CRC = Different, Rules = Same!
        assert!(matches!(
            compare_file_pair(
                &left,
                Path::new("eol.txt"),
                &meta(left.join("eol.txt")),
                &right,
                Path::new("eol.txt"),
                &meta(right.join("eol.txt")),
                ContentCompareMode::Binary,
                &cancel,
            ),
            FileContentVerdict::Different { .. }
        ));
        assert!(matches!(
            compare_file_pair(
                &left,
                Path::new("eol.txt"),
                &meta(left.join("eol.txt")),
                &right,
                Path::new("eol.txt"),
                &meta(right.join("eol.txt")),
                ContentCompareMode::Crc32,
                &cancel,
            ),
            FileContentVerdict::Different { .. }
        ));
        assert!(matches!(
            compare_file_pair(
                &left,
                Path::new("eol.txt"),
                &meta(left.join("eol.txt")),
                &right,
                Path::new("eol.txt"),
                &meta(right.join("eol.txt")),
                ContentCompareMode::RulesBased,
                &cancel,
            ),
            FileContentVerdict::Same { .. }
        ));

        // Whitespace diff: Binary = Different, Rules = Minor!
        assert!(matches!(
            compare_file_pair(
                &left,
                Path::new("ws.txt"),
                &meta(left.join("ws.txt")),
                &right,
                Path::new("ws.txt"),
                &meta(right.join("ws.txt")),
                ContentCompareMode::RulesBased,
                &cancel,
            ),
            FileContentVerdict::Minor { .. }
        ));

        // Exact binary: CRC = Same with matching CRC32 hex!
        let crc_res = compare_file_pair(
            &left,
            Path::new("exact.bin"),
            &meta(left.join("exact.bin")),
            &right,
            Path::new("exact.bin"),
            &meta(right.join("exact.bin")),
            ContentCompareMode::Crc32,
            &cancel,
        );
        assert!(matches!(
            crc_res,
            FileContentVerdict::Same {
                left_crc: Some(l),
                right_crc: Some(r),
                ..
            } if l == r
        ));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dos_and_regex_batch_rename_masks_work() {
        assert_eq!(
            apply_rename_mask("report.txt", "*.txt", "*_bak.txt", RenameMode::Dos).unwrap(),
            "report_bak.txt"
        );
        assert_eq!(
            apply_rename_mask("01_alpha.log", r"^(\d+)_(.+)\.log$", "${2}_${1}.txt", RenameMode::Regex)
                .unwrap(),
            "alpha_01.txt"
        );
    }
}
