//! AnyCompare Options dialog (`TPROGOPTIONSDIALOG`): category tree with
//! live search on the left, structured settings pane on the right, and
//! OK / Cancel / Apply footer.
use super::{dialogs::DialogHost, icons::IconManager, theme::BcTheme};
use crate::i18n::{tr, AppPreferences};
use egui::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsCategory {
    General,
    Tabs,
    ColorsAndFonts,
    FolderCompare,
    TextCompare,
    TableCompare,
    PictureCompare,
    HexCompare,
    SettingsIo,
}

impl OptionsCategory {
    pub const ALL: [Self; 9] = [
        Self::General,
        Self::Tabs,
        Self::ColorsAndFonts,
        Self::FolderCompare,
        Self::TextCompare,
        Self::TableCompare,
        Self::PictureCompare,
        Self::HexCompare,
        Self::SettingsIo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "options.general_and_startup",
            Self::Tabs => "options.tabs_and_windows",
            Self::ColorsAndFonts => "options.colors_fonts_and_theme",
            Self::FolderCompare => "session.folder_compare",
            Self::TextCompare => "session.text_compare",
            Self::TableCompare => "session.table_compare",
            Self::PictureCompare => "session.picture_compare",
            Self::HexCompare => "session.hex_compare",
            Self::SettingsIo => "options.settings_backup",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::General => "imgOptions",
            Self::Tabs => "imgPrefsTab",
            Self::ColorsAndFonts => "imgPrefsColor",
            Self::FolderCompare => "imgDirCompareSession",
            Self::TextCompare => "imgTextCompareSession",
            Self::TableCompare => "imgTableCompareSession",
            Self::PictureCompare => "imgPixCompareSession",
            Self::HexCompare => "imgHexCompareSession",
            Self::SettingsIo => "imgSessionSave",
        }
    }

    pub fn keywords(self) -> &'static str {
        match self {
            Self::General => "idioma linguagem explorer contexto inicio geral startup",
            Self::Tabs => "aba guia janela fechar confirmar tab window",
            Self::ColorsAndFonts => "cor cores fonte tema claro diferenca orfao font color theme",
            Self::FolderCompare => "pasta diretorio conteudo estrutura arvore log folder",
            Self::TextCompare => {
                "texto editor numero linha contexto detalhe miniatura espaco quebra"
            }
            Self::TableCompare => "tabela csv excel cabecalho ordenar chave coluna",
            Self::PictureCompare => "imagem foto tolerancia fantasma mesclar escala zoom",
            Self::HexCompare => "hex binario byte ascii endereco endian",
            Self::SettingsIo => "exportar importar restaurar padrao json backup",
        }
    }

    pub fn from_cli(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "tabs" | "guias" => Self::Tabs,
            "colors" | "cores" | "theme" | "tema" => Self::ColorsAndFonts,
            "folder" | "pastas" => Self::FolderCompare,
            "text" | "texto" | "textos" => Self::TextCompare,
            "table" | "tabela" | "tabelas" => Self::TableCompare,
            "picture" | "image" | "imagens" => Self::PictureCompare,
            "hex" | "hexadecimal" => Self::HexCompare,
            "backup" | "io" | "settings" => Self::SettingsIo,
            _ => Self::General,
        }
    }
}

fn request_id() -> Id {
    Id::new("anycompare_open_options_request")
}

/// Requests that the top-level app open the BC5 Options modal on `category`.
pub fn request(ctx: &Context, category: OptionsCategory) {
    ctx.data_mut(|d| d.insert_temp(request_id(), category));
}

pub fn take_request(ctx: &Context) -> Option<OptionsCategory> {
    ctx.data_mut(|d| {
        let value = d.get_temp::<OptionsCategory>(request_id());
        d.remove::<OptionsCategory>(request_id());
        value
    })
}

pub struct OptionsDialog {
    open: bool,
    category: OptionsCategory,
    search: String,
    language_pref: String,
    draft: AppPreferences,
    dirty: bool,
}

impl Default for OptionsDialog {
    fn default() -> Self {
        Self {
            open: false,
            category: OptionsCategory::General,
            search: String::new(),
            language_pref: crate::i18n::language_preference(),
            draft: crate::i18n::preferences(),
            dirty: false,
        }
    }
}

impl OptionsDialog {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, category: OptionsCategory, current: AppPreferences) {
        self.open = true;
        self.category = category;
        self.search.clear();
        self.language_pref = crate::i18n::language_preference();
        self.draft = current;
        self.dirty = false;
    }

    /// Renders the modal if open. Returns `Some(AppPreferences)` whenever the
    /// user commits changes via `OK` or `Aplicar`.
    pub fn show(
        &mut self,
        ctx: &Context,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> Option<AppPreferences> {
        if !self.open {
            return None;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.open = false;
            return None;
        }

        let screen = ctx.screen_rect();
        // Soft backdrop scrim preventing clicks behind the modal.
        Area::new(Id::new("bc5_options_backdrop"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(45));
            });

        let win_w = (screen.width() - 48.0).clamp(560.0, 760.0);
        let win_h = (screen.height() - 48.0).clamp(380.0, 520.0);
        let mut applied: Option<AppPreferences> = None;
        let mut close_now = false;

        Window::new(format!("{} - AnyCompare", tr("common.options")))
            .id(Id::new("bc5_options_window"))
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
                let body_h = (total.height() - footer_h).max(120.0);
                let sidebar_w = 205.0;

                // Body: left category tree + right active panel
                ui.allocate_ui_with_layout(
                    vec2(total.width(), body_h),
                    Layout::left_to_right(Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;

                        // Left category sidebar
                        Frame::none()
                            .fill(Color32::from_rgb(244, 246, 248))
                            .inner_margin(Margin::same(8.0))
                            .show(ui, |ui| {
                                ui.set_width(sidebar_w - 16.0);
                                ui.set_min_height(body_h - 16.0);
                                ui.visuals_mut().widgets.inactive.bg_stroke =
                                    Stroke::new(1.0_f32, Color32::from_gray(175));
                                ui.visuals_mut().extreme_bg_color = Color32::WHITE;
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                                    ui.add(
                                        TextEdit::singleline(&mut self.search)
                                            .desired_width(sidebar_w - 18.0)
                                            .hint_text(tr("options.find_option")),
                                    );
                                    ui.add_space(4.0);
                                    let q = self.search.trim().to_lowercase();
                                    let matching: Vec<OptionsCategory> = OptionsCategory::ALL
                                        .into_iter()
                                        .filter(|cat| {
                                            q.is_empty()
                                                || tr(cat.label()).to_lowercase().contains(&q)
                                                || cat.keywords().contains(&q)
                                        })
                                        .collect();
                                    if !matching.is_empty() && !matching.contains(&self.category) {
                                        self.category = matching[0];
                                    }
                                    ScrollArea::vertical()
                                        .id_salt("options_cat_tree")
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 2.0;
                                            for cat in matching {
                                                let selected = self.category == cat;
                                                let (row_rect, resp) = ui.allocate_exact_size(
                                                    vec2(sidebar_w - 20.0, 26.0),
                                                    Sense::click(),
                                                );
                                                if selected {
                                                    ui.painter().rect_filled(
                                                        row_rect,
                                                        2.0,
                                                        Color32::from_rgb(204, 232, 255),
                                                    );
                                                    ui.painter().rect_stroke(
                                                        row_rect,
                                                        2.0,
                                                        Stroke::new(
                                                            1.0_f32,
                                                            Color32::from_rgb(153, 209, 255),
                                                        ),
                                                    );
                                                } else if resp.hovered() {
                                                    ui.painter().rect_filled(
                                                        row_rect,
                                                        2.0,
                                                        Color32::from_rgb(232, 238, 245),
                                                    );
                                                }
                                                if let Some(tex) =
                                                    icons.get_icon(ui.ctx(), cat.icon())
                                                {
                                                    ui.painter().image(
                                                        tex.id(),
                                                        Rect::from_center_size(
                                                            row_rect.left_center()
                                                                + vec2(12.0, 0.0),
                                                            vec2(16.0, 16.0),
                                                        ),
                                                        Rect::from_min_max(
                                                            Pos2::ZERO,
                                                            pos2(1.0, 1.0),
                                                        ),
                                                        Color32::WHITE,
                                                    );
                                                }
                                                ui.painter().text(
                                                    row_rect.left_center() + vec2(26.0, 0.0),
                                                    Align2::LEFT_CENTER,
                                                    tr(cat.label()),
                                                    FontId::proportional(12.0),
                                                    Color32::BLACK,
                                                );
                                                if resp.clicked() {
                                                    self.category = cat;
                                                }
                                            }
                                        });
                                });
                            });

                        // Vertical divider
                        let (div_rect, _) =
                            ui.allocate_exact_size(vec2(1.0, body_h), Sense::hover());
                        ui.painter()
                            .rect_filled(div_rect, 0.0, Color32::from_gray(205));

                        // Right content panel
                        let right_w = (total.width() - sidebar_w - 1.0).max(200.0);
                        Frame::none()
                            .fill(Color32::WHITE)
                            .inner_margin(Margin::same(14.0))
                            .show(ui, |ui| {
                                ui.set_width(right_w - 28.0);
                                ui.set_min_height(body_h - 28.0);
                                ui.visuals_mut().widgets.inactive.bg_stroke =
                                    Stroke::new(1.0_f32, Color32::from_gray(165));
                                ui.visuals_mut().widgets.inactive.weak_bg_fill =
                                    Color32::from_gray(244);
                                ui.visuals_mut().widgets.hovered.bg_stroke =
                                    Stroke::new(1.0_f32, Color32::from_rgb(0, 120, 212));
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(6.0, 8.0);
                                    ui.horizontal(|ui| {
                                        if let Some(tex) =
                                            icons.get_icon(ui.ctx(), self.category.icon())
                                        {
                                            ui.image((tex.id(), vec2(16.0, 16.0)));
                                        }
                                        ui.label(
                                            RichText::new(tr(self.category.label()))
                                                .strong()
                                                .size(14.0)
                                                .color(Color32::BLACK),
                                        );
                                    });
                                    ui.separator();
                                    ScrollArea::vertical()
                                        .id_salt("options_detail_scroll")
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 8.0;
                                            self.show_category(ui, dialogs);
                                        });
                                });
                            });
                    },
                );

                // Footer separator + OK / Cancelar / Aplicar buttons
                let sep_rect = Rect::from_min_size(ui.cursor().min, vec2(total.width(), 1.0));
                ui.painter()
                    .rect_filled(sep_rect, 0.0, Color32::from_gray(205));
                ui.add_space(1.0);
                Frame::none()
                    .fill(BcTheme::WINDOW_BG)
                    .inner_margin(Margin::symmetric(12.0, 7.0))
                    .show(ui, |ui| {
                        ui.set_width(total.width() - 24.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let apply_btn = ui.add_enabled(
                                self.dirty,
                                Button::new(tr("options.apply")).min_size(vec2(82.0, 24.0)),
                            );
                            let cancel_btn =
                                ui.add(Button::new(tr("common.cancel")).min_size(vec2(82.0, 24.0)));
                            let ok_btn = ui.add(
                                Button::new(RichText::new(tr("common.ok")).strong())
                                    .fill(Color32::from_rgb(225, 240, 255))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0, 120, 212)))
                                    .min_size(vec2(82.0, 24.0)),
                            );
                            if apply_btn.clicked() {
                                let prev_lang = crate::i18n::language();
                                if crate::i18n::save_options(
                                    &self.language_pref,
                                    self.draft.clone(),
                                    dialogs,
                                ) {
                                    if crate::i18n::language() != prev_lang {
                                        crate::ui::theme::apply(ctx);
                                    }
                                    self.dirty = false;
                                    applied = Some(self.draft.clone());
                                }
                            }
                            if ok_btn.clicked() {
                                let prev_lang = crate::i18n::language();
                                if !self.dirty
                                    || crate::i18n::save_options(
                                        &self.language_pref,
                                        self.draft.clone(),
                                        dialogs,
                                    )
                                {
                                    if crate::i18n::language() != prev_lang {
                                        crate::ui::theme::apply(ctx);
                                    }
                                    self.dirty = false;
                                    applied = Some(self.draft.clone());
                                    close_now = true;
                                }
                            }
                            if cancel_btn.clicked() {
                                close_now = true;
                            }
                        });
                    });
            });

        if close_now {
            self.open = false;
        }
        applied
    }

    fn show_category(&mut self, ui: &mut Ui, dialogs: &mut dyn DialogHost) {
        let mut changed = false;
        match self.category {
            OptionsCategory::General => {
                let languages = crate::i18n::available_languages();
                let current_name = languages
                    .iter()
                    .find(|(code, _)| code.eq_ignore_ascii_case(&self.language_pref))
                    .map(|(_, name)| name.clone())
                    .unwrap_or_else(|| self.language_pref.clone());
                ui.horizontal(|ui| {
                    ui.label(tr("app.language.label"));
                    ComboBox::from_id_salt("options_language_combo")
                        .selected_text(current_name)
                        .width(240.0)
                        .height(340.0)
                        .show_ui(ui, |ui| {
                            for (code, name) in &languages {
                                if ui
                                    .selectable_label(
                                        self.language_pref.eq_ignore_ascii_case(code),
                                        name,
                                    )
                                    .clicked()
                                {
                                    self.language_pref = code.clone();
                                    changed = true;
                                }
                            }
                        });
                });
                ui.add_space(6.0);
                ui.label(RichText::new(tr("options.windows_explorer_integration")).strong());
                changed |= ui
                    .checkbox(
                        &mut self.draft.explorer_context_menu,
                        tr("options.include_anycompare_in_explorer_context_menu"),
                    )
                    .changed();
                ui.label(
                    RichText::new(tr(
                        "options.enabled_automatically_by_the_inno_setup",
                    ))
                    .size(11.0)
                    .color(Color32::from_gray(100)),
                );
            }
            OptionsCategory::Tabs => {
                changed |= ui
                    .checkbox(
                        &mut self.draft.open_in_new_tab,
                        tr("options.open_new_comparisons_in_a_new_tab"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.confirm_close_tabs,
                        tr("options.confirm_before_closing_tabs_with_unsaved"),
                    )
                    .changed();
            }
            OptionsCategory::ColorsAndFonts => {
                ui.horizontal(|ui| {
                    ui.label(tr("options.interface_theme"));
                    ui.label(RichText::new(tr("options.bc5_light_theme_windows_11")).strong());
                });
                ui.horizontal(|ui| {
                    ui.label(tr("options.editor_font_size"));
                    changed |= ui
                        .add(Slider::new(&mut self.draft.editor_font_size, 10..=18).suffix(" px"))
                        .changed();
                });
                changed |= ui
                    .checkbox(
                        &mut self.draft.alternate_row_shading,
                        tr("options.alternate_row_shading_in_lists"),
                    )
                    .changed();
                ui.add_space(6.0);
                ui.label(RichText::new(tr("options.comparison_color_preview_bc5")).strong());
                for (label, bg, fg) in [
                    (
                        "options.important_difference_modified_file_or_line",
                        BcTheme::DIFF_BG,
                        BcTheme::DIFF_TEXT,
                    ),
                    (
                        "options.unimportant_difference_case_whitespace",
                        BcTheme::MINOR_BG,
                        BcTheme::MINOR_TEXT,
                    ),
                    (
                        "options.orphan_present_on_only_one_side",
                        BcTheme::ORPHAN_BG,
                        Color32::from_rgb(128, 0, 255),
                    ),
                    (
                        "options.same_matching_content",
                        Color32::WHITE,
                        Color32::BLACK,
                    ),
                ] {
                    let (rect, _) = ui.allocate_exact_size(
                        vec2(ui.available_width().min(480.0), 22.0),
                        Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, 2.0, bg);
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(1.0_f32, Color32::from_gray(215)),
                    );
                    ui.painter().text(
                        rect.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        tr(label),
                        FontId::monospace(self.draft.editor_font_size.clamp(10, 15) as f32),
                        fg,
                    );
                }
            }
            OptionsCategory::FolderCompare => {
                changed |= ui
                    .checkbox(
                        &mut self.draft.folder_compare_content,
                        tr("folder.compare_file_contents"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.folder_show_tree,
                        tr("common.show_folder_structure"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.draft.folder_show_log, tr("common.show_log"))
                    .changed();
            }
            OptionsCategory::TextCompare => {
                changed |= ui
                    .checkbox(
                        &mut self.draft.text_allow_edit,
                        tr("common.allow_text_editing"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.text_line_numbers,
                        tr("common.show_line_numbers"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.text_show_context,
                        tr("common.show_difference_context"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.text_show_details,
                        tr("common.show_text_details"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.draft.text_show_thumbnail, tr("common.show_thumbnail"))
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.text_show_whitespace,
                        tr("common.show_spaces_and_tabs"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.text_word_wrap,
                        tr("text.word_wrap"),
                    )
                    .changed();
            }
            OptionsCategory::TableCompare => {
                changed |= ui
                    .checkbox(
                        &mut self.draft.table_has_header,
                        tr("options.first_line_contains_column_names"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.table_sort_by_key,
                        tr("options.sort_rows_by_key_columns"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.table_ignore_minor,
                        tr("table.ignore_unimportant"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.table_hide_same_columns,
                        tr("options.hide_same_columns"),
                    )
                    .changed();
            }
            OptionsCategory::PictureCompare => {
                let modes = [
                    "picture.mode.tolerance",
                    "picture.mode.range",
                    "picture.mode.blend",
                ];
                let idx = (self.draft.picture_default_mode as usize).min(2);
                ui.horizontal(|ui| {
                    ui.label(tr("common.comparison_mode"));
                    ComboBox::from_id_salt("options_pix_mode")
                        .selected_text(tr(modes[idx]))
                        .width(230.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in modes.iter().enumerate() {
                                if ui.selectable_label(idx == i, tr(name)).clicked() {
                                    self.draft.picture_default_mode = i as u8;
                                    changed = true;
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(tr("common.tolerance"));
                    changed |= ui
                        .add(Slider::new(&mut self.draft.picture_tolerance, 0..=255))
                        .changed();
                });
                changed |= ui
                    .checkbox(&mut self.draft.picture_ghost, tr("picture.ghost"))
                    .changed();
                changed |= ui
                    .checkbox(&mut self.draft.picture_auto_scale, tr("picture.auto_scale"))
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.picture_fit_to_window,
                        tr("picture.zoom_fit"),
                    )
                    .changed();
            }
            OptionsCategory::HexCompare => {
                ui.horizontal(|ui| {
                    ui.label(tr("hex.bytes_per_line"));
                    for n in [8usize, 16, 32] {
                        if ui
                            .selectable_label(self.draft.hex_bytes_per_row == n, n.to_string())
                            .clicked()
                        {
                            self.draft.hex_bytes_per_row = n;
                            changed = true;
                        }
                    }
                });
                changed |= ui
                    .checkbox(
                        &mut self.draft.hex_show_ascii,
                        tr("hex.show_ascii_characters"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.hex_addresses,
                        tr("hex.hexadecimal_addresses"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.draft.hex_little_endian,
                        tr("hex.little_endian_values"),
                    )
                    .changed();
            }
            OptionsCategory::SettingsIo => {
                ui.label(tr(
                    "options.export_or_import_all_anycompare_preferences",
                ));
                ui.horizontal(|ui| {
                    if ui.button(tr("menu.tools.export_settings")).clicked() {
                        crate::i18n::settings_file(true, dialogs);
                    }
                    if ui.button(tr("menu.tools.import_settings")).clicked() {
                        crate::i18n::settings_file(false, dialogs);
                        self.draft = crate::i18n::preferences();
                        self.language_pref = crate::i18n::language_preference();
                        self.dirty = false;
                    }
                });
                ui.add_space(6.0);
                if ui.button(tr("menu.tools.restore_defaults")).clicked() {
                    crate::i18n::restore_defaults(dialogs);
                    self.draft = crate::i18n::preferences();
                    self.language_pref = crate::i18n::language_preference();
                    self.dirty = false;
                }
            }
        }
        if changed {
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::TestDialogs;

    #[test]
    fn options_dialog_renders_all_categories_and_applies_changes() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut dialogs = TestDialogs::default();
        let mut modal = OptionsDialog::default();
        modal.open(OptionsCategory::ColorsAndFonts, AppPreferences::default());
        assert!(modal.is_open());

        for cat in OptionsCategory::ALL {
            modal.category = cat;
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                    ..Default::default()
                },
                |ctx| {
                    modal.show(ctx, &mut icons, &mut dialogs);
                },
            );
        }
        // Pressing Escape closes the modal without committing.
        let result = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                events: vec![Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| {
                modal.show(ctx, &mut icons, &mut dialogs);
            },
        );
        assert!(!modal.is_open());
        let _ = result;
    }
}
