//! AnyCompare tabbed Session Settings / Rules modal
//! (`TDIRSESSION*`, `TTEXTSESSION*`, `TPIXSESSION*`).
use super::{icons::IconManager, theme::BcTheme};
use crate::{diff::AlignMethod, i18n::tr, picture::CompareMode};
use egui::*;

#[derive(Clone, Debug, PartialEq)]
pub struct FolderRulesDraft {
    pub compare_content: bool,
    pub content_mode: super::folder_ops::ContentCompareMode,
    pub tree: bool,
    pub show_log: bool,
    pub filter: String,
    pub suppress_filters: bool,
    pub only_diffs: bool,
    pub only_same: bool,
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
            Self::Folder(_) => &["options.criteria", "common.filters", "options.display"],
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
        self.draft = Some(rules);
        self.active_tab = 0;
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

        let win_w = (screen.width() - 48.0).clamp(460.0, 560.0);
        let win_h = (screen.height() - 48.0).clamp(320.0, 410.0);
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
                                let w = (ui.fonts(|f| {
                                    f.layout_no_wrap(
                                        label.clone(),
                                        FontId::proportional(12.0),
                                        Color32::BLACK,
                                    )
                                    .size()
                                    .x
                                }) + 22.0)
                                    .max(76.0);
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
                                ui.spacing_mut().item_spacing = vec2(6.0, 8.0);
                                match rules {
                                    SessionRules::Folder(f) => match self.active_tab {
                                        0 => {
                                            ui.label(
                                                RichText::new(tr("options.file_comparison_criteria"))
                                                    .strong(),
                                            );
                                            ui.checkbox(
                                                &mut f.compare_content,
                                                tr("folder.compare_file_contents"),
                                            );
                                            ui.add_enabled_ui(f.compare_content, |ui| {
                                                ui.indent("folder_content_mode_indent", |ui| {
                                                    for mode in super::folder_ops::ContentCompareMode::ALL {
                                                        ui.radio_value(
                                                            &mut f.content_mode,
                                                            mode,
                                                            tr(mode.label_key()),
                                                        );
                                                    }
                                                });
                                            });
                                            ui.label(
                                                RichText::new(tr(
                                                    "options.file_size_and_timestamp_are_always_compared",
                                                ))
                                                .size(11.0)
                                                .color(Color32::from_gray(100)),
                                            );
                                        }
                                        1 => {
                                            ui.horizontal(|ui| {
                                                ui.label(tr("options.name_filter"));
                                                ui.add(
                                                    TextEdit::singleline(&mut f.filter)
                                                        .desired_width(240.0),
                                                );
                                            });
                                            ui.checkbox(
                                                &mut f.suppress_filters,
                                                tr("options.temporarily_suppress_filters"),
                                            );
                                            ui.separator();
                                            ui.label(RichText::new(tr("options.display_filter")).strong());
                                            let mut mode = if f.only_diffs {
                                                1
                                            } else if f.only_same {
                                                2
                                            } else {
                                                0
                                            };
                                            ui.radio_value(&mut mode, 0, tr("common.all"));
                                            ui.radio_value(&mut mode, 1, tr("report.only_differences"));
                                            ui.radio_value(&mut mode, 2, tr("common.same"));
                                            f.only_diffs = mode == 1;
                                            f.only_same = mode == 2;
                                        }
                                        _ => {
                                            ui.checkbox(&mut f.tree, tr("common.show_folder_structure"));
                                            ui.checkbox(&mut f.show_log, tr("common.show_log"));
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

                // Footer buttons
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(12.0, 7.0))
                    .show(ui, |ui| {
                        ui.set_width(total.width() - 24.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if ui
                                .add(Button::new(tr("common.cancel")).min_size(vec2(82.0, 24.0)))
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
                compare_content: false,
                content_mode: super::super::folder_ops::ContentCompareMode::Binary,
                tree: true,
                show_log: true,
                filter: "*.*".into(),
                suppress_filters: false,
                only_diffs: false,
                only_same: false,
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
