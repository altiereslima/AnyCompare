//! AnyCompare tabbed Session Settings / Rules modal
//! (`TDIRSESSION*`, `TTEXTSESSION*`, `TPIXSESSION*`).
use super::{icons::IconManager, theme::BcTheme};
use crate::{diff::AlignMethod, i18n::tr, picture::CompareMode};
use egui::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderScopeChoice {
    ThisViewOnly,
    ThisSession,
    UpdateDefaults,
}

impl FolderScopeChoice {
    pub const ALL: [Self; 3] = [Self::ThisViewOnly, Self::ThisSession, Self::UpdateDefaults];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::ThisViewOnly => "options.scope_this_view",
            Self::ThisSession => "options.scope_this_session",
            Self::UpdateDefaults => "options.scope_update_defaults",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderHandlingChoice {
    ShowAll,
    CompareStructure,
    CompareFilesOnly,
    Flatten,
}

impl FolderHandlingChoice {
    pub const ALL: [Self; 4] = [
        Self::ShowAll,
        Self::CompareStructure,
        Self::CompareFilesOnly,
        Self::Flatten,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::ShowAll => "toolbar.always_show_folders",
            Self::CompareStructure => "toolbar.compare_files_and_folder_structure",
            Self::CompareFilesOnly => "toolbar.only_compare_files",
            Self::Flatten => "toolbar.ignore_folder_structure",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FolderRulesDraft {
    // Specs
    pub left_path: String,
    pub right_path: String,
    pub description: String,
    // Comparison — Quick tests
    pub compare_size: bool,
    pub compare_timestamp: bool,
    pub timestamp_tolerance_secs: u64,
    pub ignore_dst: bool,
    pub ignore_timezone: bool,
    pub compare_filename_case: bool,
    pub align_different_extensions: bool,
    pub align_unicode_normalization: bool,
    // Comparison — Attributes & Unix metadata
    pub compare_attr_archive: bool,
    pub compare_attr_system: bool,
    pub compare_attr_hidden: bool,
    pub compare_attr_readonly: bool,
    pub compare_unix_permissions: bool,
    pub compare_unix_owner: bool,
    pub compare_unix_group: bool,
    // Comparison — Requires opening files
    pub compare_content: bool,
    pub content_mode: super::folder_ops::ContentCompareMode,
    pub compare_versions: bool,
    pub skip_if_quick_same: bool,
    pub override_quick_results: bool,
    // Handling
    pub dir_handling: FolderHandlingChoice,
    pub archives_as_folders: bool,
    pub follow_symlinks: bool,
    // Name Filters
    pub filter: String,
    pub exclude_patterns: String,
    pub suppress_filters: bool,
    // Other Filters
    pub only_diffs: bool,
    pub only_same: bool,
    pub exclude_protected_os: bool,
    pub min_size_kb: u64,
    // Misc
    pub show_log: bool,
    pub show_folder_sizes: bool,
    pub auto_expand_subfolders: bool,
    // Footer scope
    pub scope: FolderScopeChoice,
}

impl FolderRulesDraft {
    pub fn attributes_summary(&self) -> String {
        let mut s = String::new();
        if self.compare_attr_archive {
            s.push('A');
        }
        if self.compare_attr_system {
            s.push('S');
        }
        if self.compare_attr_hidden {
            s.push('H');
        }
        if self.compare_attr_readonly {
            s.push('R');
        }
        if s.is_empty() {
            "--------------".to_owned()
        } else {
            s
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRulesDraft {
    pub ignore_case: bool,
    pub ignore_whitespace: bool,
    pub align_method: AlignMethod,
    pub use_closeness: bool,
    pub never_align_diffs: bool,
    pub skew_tolerance: usize,
    pub editing: bool,
    pub line_numbers: bool,
    pub context_lines: bool,
    pub show_details: bool,
    pub show_thumbnail: bool,
    pub show_whitespace: bool,
    pub word_wrap: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PictureRulesDraft {
    pub mode: CompareMode,
    pub tolerance: u8,
    pub blend_percent: u8,
    pub ghost: bool,
    pub ignore_unimportant: bool,
    pub auto_scale: bool,
    pub offset_x: i32,
    pub offset_y: i32,
    pub fit_to_window: bool,
    pub show_tags: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionRules {
    Folder(FolderRulesDraft),
    Text(TextRulesDraft),
    Picture(PictureRulesDraft),
}

impl SessionRules {
    fn title(&self) -> &'static str {
        match self {
            Self::Folder(_) => "options.session_settings_folder_compare",
            Self::Text(_) => "options.session_settings_text_compare",
            Self::Picture(_) => "options.session_settings_picture_compare",
        }
    }

    fn tabs(&self) -> &'static [&'static str] {
        match self {
            Self::Folder(_) => &[
                "options.specs",
                "options.comparison",
                "options.handling",
                "options.name_filters_tab",
                "options.other_filters_tab",
                "options.misc_tab",
            ],
            Self::Text(_) => &["options.importance", "options.alignment", "options.display"],
            Self::Picture(_) => &["options.comparison", "options.alignment_and_display"],
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Self::Folder(_) => "imgDirCompareSession",
            Self::Text(_) => "imgTextCompareSession",
            Self::Picture(_) => "imgPixCompareSession",
        }
    }
}

fn request_id() -> Id {
    Id::new("anycompare_session_rules_request")
}

/// Signals the top-level app to open the tabbed BC5 Session Settings dialog
/// seeded with the active session's current rules.
pub fn request(ctx: &Context) {
    ctx.data_mut(|d| d.insert_temp(request_id(), true));
}

pub fn take_request(ctx: &Context) -> bool {
    ctx.data_mut(|d| {
        let requested = d.get_temp::<bool>(request_id()).unwrap_or(false);
        d.remove::<bool>(request_id());
        requested
    })
}

#[derive(Default)]
pub struct SessionSettingsDialog {
    draft: Option<SessionRules>,
    active_tab: usize,
}

impl SessionSettingsDialog {
    pub fn is_open(&self) -> bool {
        self.draft.is_some()
    }

    pub fn open(&mut self, rules: SessionRules) {
        self.active_tab = match &rules {
            SessionRules::Folder(_) => 1, // Land on "Comparison" tab by default
            _ => 0,
        };
        self.draft = Some(rules);
    }

    pub fn show(&mut self, ctx: &Context, icons: &mut IconManager) -> Option<SessionRules> {
        let rules = self.draft.as_mut()?;
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.draft = None;
            return None;
        }

        let screen = ctx.screen_rect();
        Area::new(Id::new("bc5_rules_backdrop"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(45));
            });

        let is_folder = matches!(rules, SessionRules::Folder(_));
        let win_w = if is_folder {
            (screen.width() - 48.0).clamp(580.0, 640.0)
        } else {
            (screen.width() - 48.0).clamp(460.0, 560.0)
        };
        let win_h = if is_folder {
            (screen.height() - 48.0).clamp(440.0, 476.0)
        } else {
            (screen.height() - 48.0).clamp(320.0, 410.0)
        };
        let title = format!("{} - AnyCompare", tr(rules.title()));
        let icon_name = rules.icon();
        let tabs = rules.tabs();
        self.active_tab = self.active_tab.min(tabs.len().saturating_sub(1));

        let mut accepted = false;
        let mut cancelled = false;

        Window::new(title)
            .id(Id::new("bc5_session_settings_window"))
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(vec2(win_w, win_h))
            .frame(
                Frame::window(&ctx.style())
                    .fill(BcTheme::WINDOW_BG)
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(145)))
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin::same(0.0)),
            )
            .show(ctx, |ui| {
                let total = ui.available_rect_before_wrap();
                let footer_h = 40.0;
                let tab_h = 30.0;
                let body_h = (total.height() - footer_h - tab_h).max(120.0);

                // BC5 Tab strip
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin {
                        left: 10.0,
                        right: 10.0,
                        top: 6.0,
                        bottom: 0.0,
                    })
                    .show(ui, |ui| {
                        ui.set_width(total.width() - 20.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if let Some(tex) = icons.get_icon(ui.ctx(), icon_name) {
                                ui.image((tex.id(), vec2(16.0, 16.0)));
                                ui.add_space(6.0);
                            }
                            for (i, &tab_label) in tabs.iter().enumerate() {
                                let selected = self.active_tab == i;
                                let label = tr(tab_label);
                                let pad = if tabs.len() >= 5 { 14.0 } else { 22.0 };
                                let min_w = if tabs.len() >= 5 { 54.0 } else { 76.0 };
                                let w = (ui.fonts(|f| {
                                    f.layout_no_wrap(
                                        label.clone(),
                                        FontId::proportional(12.0),
                                        Color32::BLACK,
                                    )
                                    .size()
                                    .x
                                }) + pad)
                                    .max(min_w);
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
                                let fill = if selected {
                                    Color32::WHITE
                                } else if resp.hovered() {
                                    Color32::from_rgb(232, 238, 246)
                                } else {
                                    Color32::from_gray(232)
                                };
                                ui.painter().rect_filled(rect, 2.0, fill);
                                let stroke = Stroke::new(1.0_f32, Color32::from_gray(190));
                                ui.painter().line_segment([rect.left_bottom(), rect.left_top()], stroke);
                                ui.painter().line_segment([rect.left_top(), rect.right_top()], stroke);
                                ui.painter().line_segment([rect.right_top(), rect.right_bottom()], stroke);
                                if !selected {
                                    ui.painter().line_segment(
                                        [rect.left_bottom(), rect.right_bottom()],
                                        stroke,
                                    );
                                }
                                ui.painter().text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    label,
                                    FontId::proportional(12.0),
                                    Color32::BLACK,
                                );
                                if resp.clicked() {
                                    self.active_tab = i;
                                }
                            }
                        });
                    });

                // White tabbed content card
                Frame::none()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(195)))
                    .outer_margin(Margin::symmetric(10.0, 0.0))
                    .inner_margin(Margin::same(14.0))
                    .show(ui, |ui| {
                        ui.set_width(total.width() - 48.0);
                        ui.set_min_height(body_h - 28.0);
                        ui.visuals_mut().widgets.inactive.bg_stroke =
                            Stroke::new(1.0_f32, Color32::from_gray(165));
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::from_gray(244);
                        ui.visuals_mut().widgets.hovered.bg_stroke =
                            Stroke::new(1.0_f32, Color32::from_rgb(0, 120, 212));
                        ScrollArea::vertical()
                            .id_salt("session_rules_body")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = if is_folder {
                                    vec2(6.0, 5.0)
                                } else {
                                    vec2(6.0, 8.0)
                                };
                                match rules {
                                    SessionRules::Folder(f) => match self.active_tab {
                                        // 0: Specs (Especificações)
                                        0 => {
                                            ui.label(
                                                RichText::new(tr("options.left_base_folder"))
                                                    .strong(),
                                            );
                                            ui.add(
                                                TextEdit::singleline(&mut f.left_path)
                                                    .desired_width(f32::INFINITY),
                                            );
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(tr("options.right_base_folder"))
                                                    .strong(),
                                            );
                                            ui.add(
                                                TextEdit::singleline(&mut f.right_path)
                                                    .desired_width(f32::INFINITY),
                                            );
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(tr("options.session_description"))
                                                    .strong(),
                                            );
                                            ui.add(
                                                TextEdit::multiline(&mut f.description)
                                                    .desired_rows(4)
                                                    .desired_width(f32::INFINITY),
                                            );
                                        }
                                        // 1: Comparison (Comparação) — default tab
                                        1 => {
                                            let avail_w = ui.available_width();
                                            let right_col_w = 200.0_f32.min(avail_w * 0.36);
                                            let left_col_w = (avail_w - right_col_w - 14.0).max(260.0);
                                            ui.horizontal_top(|ui| {
                                                // Left column: Quick tests
                                                ui.allocate_ui_with_layout(
                                                    vec2(left_col_w, 0.0),
                                                    Layout::top_down(Align::LEFT),
                                                    |ui| {
                                                        ui.spacing_mut().item_spacing.y = 4.0;
                                                        ui.label(
                                                            RichText::new(tr(
                                                                "options.quick_tests",
                                                            ))
                                                            .strong(),
                                                        );
                                                        ui.checkbox(
                                                            &mut f.compare_size,
                                                            tr("options.compare_file_size"),
                                                        );
                                                        ui.checkbox(
                                                            &mut f.compare_timestamp,
                                                            tr("options.compare_timestamps"),
                                                        );
                                                        ui.add_enabled_ui(
                                                            f.compare_timestamp,
                                                            |ui| {
                                                                ui.indent(
                                                                    "folder_ts_indent",
                                                                    |ui| {
                                                                        ui.spacing_mut()
                                                                            .item_spacing
                                                                            .y = 4.0;
                                                                        ui.horizontal(|ui| {
                                                                            let mut secs = f
                                                                                .timestamp_tolerance_secs
                                                                                .min(86_400)
                                                                                as u32;
                                                                            if ui
                                                                                .add(
                                                                                    DragValue::new(
                                                                                        &mut secs,
                                                                                    )
                                                                                    .range(
                                                                                        0..=86_400,
                                                                                    ),
                                                                                )
                                                                                .changed()
                                                                            {
                                                                                f.timestamp_tolerance_secs =
                                                                                    secs as u64;
                                                                            }
                                                                            ui.label(tr(
                                                                                "options.second_tolerance",
                                                                            ));
                                                                        });
                                                                        ui.checkbox(
                                                                            &mut f.ignore_dst,
                                                                            tr("options.ignore_dst_diff"),
                                                                        );
                                                                        ui.checkbox(
                                                                            &mut f.ignore_timezone,
                                                                            tr("options.ignore_timezone_diff"),
                                                                        );
                                                                    },
                                                                );
                                                            },
                                                        );
                                                    },
                                                );
                                                ui.add_space(10.0);
                                                // Right column: File attributes & Unix metadata
                                                ui.allocate_ui_with_layout(
                                                    vec2(right_col_w, 0.0),
                                                    Layout::top_down(Align::LEFT),
                                                    |ui| {
                                                        ui.spacing_mut().item_spacing.y = 4.0;
                                                        ui.label(tr(
                                                            "options.compare_file_attributes",
                                                        ));
                                                        let summary = f.attributes_summary();
                                                        ComboBox::from_id_salt("folder_attr_combo")
                                                            .selected_text(summary)
                                                            .width((right_col_w - 8.0).max(130.0))
                                                            .show_ui(ui, |ui| {
                                                                ui.checkbox(
                                                                    &mut f.compare_attr_archive,
                                                                    "Archive (A)",
                                                                );
                                                                ui.checkbox(
                                                                    &mut f.compare_attr_system,
                                                                    "System (S)",
                                                                );
                                                                ui.checkbox(
                                                                    &mut f.compare_attr_hidden,
                                                                    "Hidden (H)",
                                                                );
                                                                ui.checkbox(
                                                                    &mut f.compare_attr_readonly,
                                                                    "Read-only (R)",
                                                                );
                                                            });
                                                        ui.add_space(6.0);
                                                        ui.label(tr(
                                                            "options.compare_unix_metadata",
                                                        ));
                                                        ui.checkbox(
                                                            &mut f.compare_unix_permissions,
                                                            tr("options.compare_permissions"),
                                                        );
                                                        ui.checkbox(
                                                            &mut f.compare_unix_owner,
                                                            tr("options.compare_owner"),
                                                        );
                                                        ui.checkbox(
                                                            &mut f.compare_unix_group,
                                                            tr("options.compare_group"),
                                                        );
                                                    },
                                                );
                                            });
                                            // Filename alignment quick tests (full width, zero overlap)
                                            ui.checkbox(
                                                &mut f.compare_filename_case,
                                                tr("options.compare_filename_case"),
                                            );
                                            ui.checkbox(
                                                &mut f.align_different_extensions,
                                                tr("options.align_different_extensions"),
                                            );
                                            ui.checkbox(
                                                &mut f.align_unicode_normalization,
                                                tr("options.align_unicode_normalization"),
                                            );

                                            ui.add_space(2.0);
                                            ui.separator();
                                            ui.add_space(2.0);

                                            // Bottom section: Requires opening files
                                            ui.label(
                                                RichText::new(tr("options.requires_opening_files"))
                                                    .strong(),
                                            );
                                            ui.horizontal(|ui| {
                                                ui.allocate_ui_with_layout(
                                                    vec2(left_col_w, 0.0),
                                                    Layout::left_to_right(Align::Center),
                                                    |ui| {
                                                        ui.set_min_width(left_col_w);
                                                        ui.checkbox(
                                                            &mut f.compare_content,
                                                            tr("options.compare_contents_colon"),
                                                        );
                                                    },
                                                );
                                                ui.add_space(10.0);
                                                ui.checkbox(
                                                    &mut f.compare_versions,
                                                    tr("options.compare_versions"),
                                                );
                                            });
                                            ui.add_enabled_ui(f.compare_content, |ui| {
                                                ui.indent("folder_content_mode_indent", |ui| {
                                                    ui.spacing_mut().item_spacing.y = 3.0;
                                                    for mode in
                                                        super::folder_ops::ContentCompareMode::ALL
                                                    {
                                                        ui.radio_value(
                                                            &mut f.content_mode,
                                                            mode,
                                                            tr(mode.label_key()),
                                                        );
                                                    }
                                                    ui.checkbox(
                                                        &mut f.skip_if_quick_same,
                                                        tr("options.skip_if_quick_same"),
                                                    );
                                                });
                                            });
                                            ui.add_enabled_ui(
                                                f.compare_content || f.compare_versions,
                                                |ui| {
                                                    ui.checkbox(
                                                        &mut f.override_quick_results,
                                                        tr("options.override_quick_results"),
                                                    );
                                                },
                                            );
                                        }
                                        // 2: Handling (Tratamento)
                                        2 => {
                                            ui.label(
                                                RichText::new(tr("options.folder_handling"))
                                                    .strong(),
                                            );
                                            for choice in FolderHandlingChoice::ALL {
                                                ui.radio_value(
                                                    &mut f.dir_handling,
                                                    choice,
                                                    tr(choice.label_key()),
                                                );
                                            }
                                            ui.separator();
                                            ui.label(
                                                RichText::new(tr("options.archive_handling"))
                                                    .strong(),
                                            );
                                            ui.radio_value(
                                                &mut f.archives_as_folders,
                                                false,
                                                tr("options.archives_as_files"),
                                            );
                                            ui.radio_value(
                                                &mut f.archives_as_folders,
                                                true,
                                                tr("options.archives_as_folders_always"),
                                            );
                                            ui.separator();
                                            ui.checkbox(
                                                &mut f.follow_symlinks,
                                                tr("options.follow_symlinks"),
                                            );
                                        }
                                        // 3: Name Filters (Nomear Filtros)
                                        3 => {
                                            ui.label(
                                                RichText::new(tr("options.include_files")).strong(),
                                            );
                                            ui.add(
                                                TextEdit::singleline(&mut f.filter)
                                                    .desired_width(f32::INFINITY),
                                            );
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(tr("options.exclude_files")).strong(),
                                            );
                                            ui.add(
                                                TextEdit::singleline(&mut f.exclude_patterns)
                                                    .desired_width(f32::INFINITY),
                                            );
                                            ui.label(
                                                RichText::new(tr("options.exclude_patterns_hint"))
                                                    .size(11.0)
                                                    .color(Color32::from_gray(100)),
                                            );
                                            ui.add_space(6.0);
                                            ui.checkbox(
                                                &mut f.suppress_filters,
                                                tr("options.temporarily_suppress_filters"),
                                            );
                                        }
                                        // 4: Other Filters (Outros Filtros)
                                        4 => {
                                            ui.label(
                                                RichText::new(tr("options.display_filter"))
                                                    .strong(),
                                            );
                                            let mut mode = if f.only_diffs {
                                                1
                                            } else if f.only_same {
                                                2
                                            } else {
                                                0
                                            };
                                            ui.radio_value(&mut mode, 0, tr("common.all"));
                                            ui.radio_value(
                                                &mut mode,
                                                1,
                                                tr("report.only_differences"),
                                            );
                                            ui.radio_value(&mut mode, 2, tr("common.same"));
                                            f.only_diffs = mode == 1;
                                            f.only_same = mode == 2;
                                            ui.separator();
                                            ui.checkbox(
                                                &mut f.exclude_protected_os,
                                                tr("options.exclude_protected_os"),
                                            );
                                            ui.horizontal(|ui| {
                                                ui.label(tr("options.size_filter_min_kb"));
                                                let mut kb = f.min_size_kb.min(10_000_000) as u32;
                                                if ui
                                                    .add(
                                                        DragValue::new(&mut kb)
                                                            .range(0..=10_000_000),
                                                    )
                                                    .changed()
                                                {
                                                    f.min_size_kb = kb as u64;
                                                }
                                            });
                                        }
                                        // 5: Misc (Diversos)
                                        _ => {
                                            ui.checkbox(&mut f.show_log, tr("common.show_log"));
                                            ui.checkbox(
                                                &mut f.show_folder_sizes,
                                                tr("options.show_folder_sizes"),
                                            );
                                            ui.checkbox(
                                                &mut f.auto_expand_subfolders,
                                                tr("options.auto_expand_subfolders"),
                                            );
                                        }
                                    },
                                    SessionRules::Text(t) => match self.active_tab {
                                        0 => {
                                            ui.label(
                                                RichText::new(tr("options.unimportant_differences"))
                                                    .strong(),
                                            );
                                            ui.checkbox(
                                                &mut t.ignore_case,
                                                tr("common.ignore_character_case"),
                                            );
                                            ui.checkbox(
                                                &mut t.ignore_whitespace,
                                                tr("common.ignore_whitespace"),
                                            );
                                        }
                                        1 => {
                                            ui.horizontal(|ui| {
                                                ui.label(tr("options.alignment_method"));
                                                ComboBox::from_id_salt("rules_align_method")
                                                    .selected_text(tr(t.align_method.label()))
                                                    .width(220.0)
                                                    .show_ui(ui, |ui| {
                                                        for m in AlignMethod::ALL {
                                                            ui.selectable_value(
                                                                &mut t.align_method,
                                                                m,
                                                                tr(m.label()),
                                                            );
                                                        }
                                                    });
                                            });
                                            ui.checkbox(
                                                &mut t.use_closeness,
                                                tr("options.align_similar_lines_closeness"),
                                            );
                                            ui.checkbox(
                                                &mut t.never_align_diffs,
                                                tr("options.never_align_mismatches"),
                                            );
                                            ui.horizontal(|ui| {
                                                ui.label(tr("options.skew_tolerance_lines"));
                                                let mut skew = t.skew_tolerance as u32;
                                                if ui
                                                    .add(
                                                        DragValue::new(&mut skew)
                                                            .range(1..=50_000),
                                                    )
                                                    .changed()
                                                {
                                                    t.skew_tolerance = skew as usize;
                                                }
                                            });
                                        }
                                        _ => {
                                            ui.checkbox(&mut t.editing, tr("common.allow_text_editing"));
                                            ui.checkbox(
                                                &mut t.line_numbers,
                                                tr("common.show_line_numbers"),
                                            );
                                            ui.checkbox(
                                                &mut t.context_lines,
                                                tr("common.show_difference_context"),
                                            );
                                            ui.checkbox(
                                                &mut t.show_details,
                                                tr("common.show_text_details"),
                                            );
                                            ui.checkbox(&mut t.show_thumbnail, tr("common.show_thumbnail"));
                                            ui.checkbox(
                                                &mut t.show_whitespace,
                                                tr("common.show_spaces_and_tabs"),
                                            );
                                            ui.checkbox(
                                                &mut t.word_wrap,
                                                tr("text.word_wrap"),
                                            );
                                        }
                                    },
                                    SessionRules::Picture(p) => match self.active_tab {
                                        0 => {
                                            let modes = [
                                                ("picture.mode.tolerance", CompareMode::Tolerance),
                                                (
                                                    "picture.mode.range",
                                                    CompareMode::MismatchRange,
                                                ),
                                                ("picture.mode.blend", CompareMode::Blend),
                                            ];
                                            ui.horizontal(|ui| {
                                                ui.label(tr("common.comparison_mode"));
                                                let current = modes
                                                    .iter()
                                                    .find(|(_, m)| *m == p.mode)
                                                    .map_or("picture.mode.tolerance", |(l, _)| *l);
                                                ComboBox::from_id_salt("rules_pix_mode")
                                                    .selected_text(tr(current))
                                                    .width(230.0)
                                                    .show_ui(ui, |ui| {
                                                        for (l, m) in modes {
                                                            ui.selectable_value(
                                                                &mut p.mode,
                                                                m,
                                                                tr(l),
                                                            );
                                                        }
                                                    });
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label(tr("common.tolerance"));
                                                ui.add(Slider::new(&mut p.tolerance, 0..=255));
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label(tr("picture.blend_mesclagem"));
                                                ui.add(
                                                    Slider::new(&mut p.blend_percent, 0..=100)
                                                        .suffix("%"),
                                                );
                                            });
                                            ui.checkbox(&mut p.ghost, tr("picture.ghost"));
                                            ui.checkbox(
                                                &mut p.ignore_unimportant,
                                                tr("table.ignore_unimportant"),
                                            );
                                        }
                                        _ => {
                                            ui.checkbox(
                                                &mut p.auto_scale,
                                                tr("picture.auto_scale"),
                                            );
                                            ui.checkbox(
                                                &mut p.fit_to_window,
                                                tr("picture.zoom_fit"),
                                            );
                                            ui.checkbox(
                                                &mut p.show_tags,
                                                tr("picture.show_properties"),
                                            );
                                            ui.horizontal(|ui| {
                                                ui.label("ΔX:");
                                                ui.add(
                                                    DragValue::new(&mut p.offset_x)
                                                        .range(-4096..=4096),
                                                );
                                                ui.label("ΔY:");
                                                ui.add(
                                                    DragValue::new(&mut p.offset_y)
                                                        .range(-4096..=4096),
                                                );
                                                if ui.button(tr("picture.reset")).clicked() {
                                                    p.offset_x = 0;
                                                    p.offset_y = 0;
                                                }
                                            });
                                        }
                                    },
                                }
                            });
                    });

                // Footer buttons (with scope combo on the left for Folder Compare)
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(12.0, 7.0))
                    .show(ui, |ui| {
                        ui.set_width(total.width() - 24.0);
                        ui.horizontal(|ui| {
                            if let SessionRules::Folder(f) = rules {
                                ComboBox::from_id_salt("folder_rules_scope")
                                    .selected_text(tr(f.scope.label_key()))
                                    .width(230.0)
                                    .show_ui(ui, |ui| {
                                        for sc in FolderScopeChoice::ALL {
                                            ui.selectable_value(
                                                &mut f.scope,
                                                sc,
                                                tr(sc.label_key()),
                                            );
                                        }
                                    });
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                if ui
                                    .add(
                                        Button::new(tr("common.cancel")).min_size(vec2(82.0, 24.0)),
                                    )
                                    .clicked()
                                {
                                    cancelled = true;
                                }
                                if ui
                                    .add(
                                        Button::new(RichText::new(tr("common.ok")).strong())
                                            .fill(Color32::from_rgb(225, 240, 255))
                                            .stroke(Stroke::new(
                                                1.0_f32,
                                                Color32::from_rgb(0, 120, 212),
                                            ))
                                            .min_size(vec2(82.0, 24.0)),
                                    )
                                    .clicked()
                                {
                                    accepted = true;
                                }
                            });
                        });
                    });
            });

        if accepted {
            self.draft.take()
        } else if cancelled {
            self.draft = None;
            None
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_settings_renders_all_session_variants_and_tabs() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let samples = [
            SessionRules::Folder(FolderRulesDraft {
                left_path: String::new(),
                right_path: String::new(),
                description: String::new(),
                compare_size: true,
                compare_timestamp: true,
                timestamp_tolerance_secs: 2,
                ignore_dst: false,
                ignore_timezone: false,
                compare_filename_case: false,
                align_different_extensions: false,
                align_unicode_normalization: true,
                compare_attr_archive: false,
                compare_attr_system: false,
                compare_attr_hidden: false,
                compare_attr_readonly: false,
                compare_unix_permissions: false,
                compare_unix_owner: false,
                compare_unix_group: false,
                compare_content: false,
                content_mode: super::super::folder_ops::ContentCompareMode::Binary,
                compare_versions: false,
                skip_if_quick_same: false,
                override_quick_results: true,
                dir_handling: FolderHandlingChoice::CompareStructure,
                archives_as_folders: false,
                follow_symlinks: false,
                filter: "*.*".into(),
                exclude_patterns: String::new(),
                suppress_filters: false,
                only_diffs: false,
                only_same: false,
                exclude_protected_os: false,
                min_size_kb: 0,
                show_log: true,
                show_folder_sizes: true,
                auto_expand_subfolders: false,
                scope: FolderScopeChoice::ThisViewOnly,
            }),
            SessionRules::Text(TextRulesDraft {
                ignore_case: false,
                ignore_whitespace: false,
                align_method: AlignMethod::Standard,
                use_closeness: true,
                never_align_diffs: false,
                skew_tolerance: 2000,
                editing: true,
                line_numbers: false,
                context_lines: false,
                show_details: true,
                show_thumbnail: true,
                show_whitespace: false,
                word_wrap: false,
            }),
            SessionRules::Picture(PictureRulesDraft {
                mode: CompareMode::Tolerance,
                tolerance: 0,
                blend_percent: 50,
                ghost: true,
                ignore_unimportant: false,
                auto_scale: false,
                offset_x: 0,
                offset_y: 0,
                fit_to_window: true,
                show_tags: false,
            }),
        ];
        for sample in samples {
            let tab_count = sample.tabs().len();
            let mut dialog = SessionSettingsDialog::default();
            dialog.open(sample);
            for tab in 0..tab_count {
                dialog.active_tab = tab;
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        dialog.show(ctx, &mut icons);
                    },
                );
            }
            assert!(dialog.is_open());
        }
    }
}
