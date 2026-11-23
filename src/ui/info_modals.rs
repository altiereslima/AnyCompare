use super::{icons::IconManager, theme::BcTheme};
use crate::i18n::tr;
use egui::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActiveModal {
    Info {
        title: String,
        rows: Vec<(String, String)>,
    },
    Legend,
    About,
}

fn modal_id() -> Id {
    Id::new("anycompare_active_info_modal")
}

pub fn open_info(ctx: &Context, title: impl Into<String>, rows: Vec<(String, String)>) {
    ctx.data_mut(|d| {
        d.insert_temp(
            modal_id(),
            ActiveModal::Info {
                title: title.into(),
                rows,
            },
        );
    });
}

pub fn open_legend(ctx: &Context) {
    ctx.data_mut(|d| d.insert_temp(modal_id(), ActiveModal::Legend));
}

pub fn open_about(ctx: &Context) {
    ctx.data_mut(|d| d.insert_temp(modal_id(), ActiveModal::About));
}

pub fn close(ctx: &Context) {
    ctx.data_mut(|d| d.remove::<ActiveModal>(modal_id()));
}

pub fn active(ctx: &Context) -> Option<ActiveModal> {
    ctx.data(|d| d.get_temp::<ActiveModal>(modal_id()))
}

pub fn show(ctx: &Context, icons: &mut IconManager) {
    let Some(modal) = active(ctx) else {
        return;
    };

    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        close(ctx);
        return;
    }

    let screen = ctx.screen_rect();
    Area::new(Id::new("bc5_info_modal_backdrop"))
        .order(Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let _ = ui.allocate_exact_size(screen.size(), Sense::click());
        });

    let (title, width): (String, f32) = match &modal {
        ActiveModal::Info { title, .. } => (title.clone(), 440.0),
        ActiveModal::Legend => (tr("folder.legend"), 430.0),
        ActiveModal::About => (
            tr("menu.help.about").trim_end_matches("...").to_owned(),
            400.0,
        ),
    };

    let modal_w = width.min(screen.width() - 32.0);
    let mut should_close = false;

    Window::new(title.clone())
        .id(Id::new("bc5_info_modal_window"))
        .title_bar(false)
        .order(Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .auto_sized()
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(
            Frame::none()
                .fill(BcTheme::WINDOW_BG)
                .stroke(Stroke::new(1.0_f32, Color32::from_gray(168)))
                .rounding(Rounding::same(6.0))
                .shadow(epaint::Shadow {
                    offset: vec2(0.0, 4.0),
                    blur: 14.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(48),
                })
                .inner_margin(Margin::ZERO),
        )
        .show(ctx, |ui| {
            ui.set_width(modal_w);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

            // Native Windows 11 title bar matching folder_ops.rs
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
                ui.id().with("bc5_info_dlg_close"),
                Sense::click(),
            );
            {
                let p = ui.painter();
                p.rect_filled(
                    tb_rect,
                    Rounding {
                        nw: 6.0,
                        ne: 6.0,
                        sw: 0.0,
                        se: 0.0,
                    },
                    Color32::WHITE,
                );
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
                        Rounding {
                            nw: 0.0,
                            ne: 6.0,
                            sw: 0.0,
                            se: 0.0,
                        },
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
                should_close = true;
            }

            Frame::none().inner_margin(Margin::same(14.0)).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                match &modal {
                    ActiveModal::Info { rows, .. } => {
                        Frame::none()
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0_f32, Color32::from_gray(205)))
                            .inner_margin(Margin::same(10.0))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                Grid::new("bc5_info_modal_grid")
                                    .num_columns(2)
                                    .spacing(vec2(14.0, 6.0))
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for (label, value) in rows {
                                            ui.label(
                                                RichText::new(label)
                                                    .size(12.0)
                                                    .strong()
                                                    .color(Color32::from_gray(55)),
                                            );
                                            ui.add(
                                                Label::new(
                                                    RichText::new(value)
                                                        .size(12.0)
                                                        .color(Color32::from_gray(20)),
                                                )
                                                .wrap(),
                                            );
                                            ui.end_row();
                                        }
                                    });
                            });
                    }
                    ActiveModal::Legend => {
                        Frame::none()
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0_f32, Color32::from_gray(205)))
                            .inner_margin(Margin::same(10.0))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let rows: [(&str, Option<Color32>, Color32, String); 6] = [
                                    (
                                        "imgFolderClosed",
                                        Some(Color32::from_rgb(210, 32, 32)),
                                        Color32::from_rgb(210, 32, 32),
                                        tr("common.differences"),
                                    ),
                                    (
                                        "imgFolderClosed",
                                        Some(Color32::from_rgb(128, 0, 255)),
                                        Color32::from_rgb(128, 0, 255),
                                        tr("folder.orphans"),
                                    ),
                                    (
                                        "imgFolderClosed",
                                        Some(Color32::from_rgb(30, 100, 210)),
                                        Color32::from_rgb(30, 100, 210),
                                        tr("options.unimportant_differences"),
                                    ),
                                    (
                                        "imgFolderClosed",
                                        Some(Color32::from_rgb(140, 140, 140)),
                                        Color32::from_gray(95),
                                        tr("folder.ctx.ignored"),
                                    ),
                                    (
                                        "imgFolderClosed",
                                        Some(Color32::from_rgb(205, 165, 55)),
                                        Color32::from_gray(25),
                                        tr("common.same"),
                                    ),
                                    (
                                        "imgContentDiff",
                                        None,
                                        Color32::from_rgb(210, 32, 32),
                                        tr("table.important_difference"),
                                    ),
                                ];
                                for (icon_name, tint, text_color, desc) in rows {
                                    ui.horizontal(|ui| {
                                        let (swatch_rect, _) = ui.allocate_exact_size(
                                            vec2(28.0, 18.0),
                                            Sense::hover(),
                                        );
                                        if let Some(c) = tint {
                                            if let Some(tex) =
                                                icons.folder_model(ui.ctx(), icon_name, c)
                                            {
                                                ui.painter().image(
                                                    tex.id(),
                                                    Rect::from_center_size(
                                                        swatch_rect.center(),
                                                        vec2(16.0, 16.0),
                                                    ),
                                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                    Color32::WHITE,
                                                );
                                            }
                                        } else if let Some(tex) =
                                            icons.get_icon(ui.ctx(), icon_name)
                                        {
                                            ui.painter().image(
                                                tex.id(),
                                                Rect::from_center_size(
                                                    swatch_rect.center(),
                                                    vec2(28.0, 16.0),
                                                ),
                                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                Color32::WHITE,
                                            );
                                        }
                                        ui.label(
                                            RichText::new(desc).size(12.0).color(text_color),
                                        );
                                    });
                                }
                            });
                    }
                    ActiveModal::About => {
                        ui.horizontal(|ui| {
                            if let Some(tex) =
                                icons.get_icon(ui.ctx(), "imgDirCompareSession")
                            {
                                ui.image((tex.id(), vec2(32.0, 32.0)));
                            }
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "AnyCompare {}",
                                        env!("CARGO_PKG_VERSION")
                                    ))
                                    .size(15.0)
                                    .strong()
                                    .color(Color32::from_gray(20)),
                                );
                                ui.label(
                                    RichText::new(crate::i18n::tr_fmt(
                                        "dialog.anycompare_file_and_folder_comparison",
                                        &[("", env!("CARGO_PKG_VERSION"))],
                                    ))
                                    .size(12.0)
                                    .color(Color32::from_gray(65)),
                                );
                            });
                        });
                    }
                }

                ui.add_space(6.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_sized(vec2(82.0, 24.0), Button::new(tr("common.close")))
                        .clicked()
                    {
                        should_close = true;
                    }
                });
            });
        });

    if should_close {
        close(ctx);
    }
}
