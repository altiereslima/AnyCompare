//! Text editor chrome recovered from TUiEditor / ViewText.pas and measured on
//! docs/reference/screenshots/text-loaded.png at 100% (24/10/2026).
use super::{icons::IconManager, theme::BcTheme};
use egui::*;

pub const STATUS_HEIGHT: f32 = 18.0;
pub const WINDOW_STATUS_HEIGHT: f32 = 19.0;
pub const BAR_WIDTH: f32 = 17.0;
pub const DETAILS_HEIGHT: f32 = 52.0;
pub const SPLITTER_WIDTH: f32 = 3.0;

#[derive(Clone, Copy)]
pub enum Axis {
    Horizontal,
    HorizontalWithArrows,
    Vertical,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum NavigationMode {
    Differences,
    #[default]
    Sections,
    Find,
}

fn pixel(ui: &Ui, rect: Rect, color: Color32) {
    ui.painter()
        .rect_filled(ui.painter().round_rect_to_pixels(rect), 0.0, color);
}

/// The editor's lowered frame: dark top/left, white right/bottom. The
/// scrollbar also has a white edge against the text viewport. Paint last,
/// outside the scrolling content, so long lines cannot cover these edges.
pub fn editor_edges(ui: &Ui, frame: Rect, viewport: Rect) {
    let shadow = Color32::from_gray(160);
    for (rect, color) in [
        (
            Rect::from_min_size(frame.min, vec2(frame.width() - 1.0, 1.0)),
            shadow,
        ),
        (
            Rect::from_min_size(frame.min, vec2(1.0, frame.height())),
            shadow,
        ),
        (
            Rect::from_min_size(
                frame.right_top() - vec2(1.0, 0.0),
                vec2(1.0, frame.height()),
            ),
            Color32::WHITE,
        ),
        (
            Rect::from_min_size(
                frame.left_bottom() - vec2(0.0, 1.0),
                vec2(frame.width(), 1.0),
            ),
            Color32::WHITE,
        ),
        (
            Rect::from_min_size(viewport.right_top(), vec2(1.0, viewport.height())),
            Color32::WHITE,
        ),
    ] {
        pixel(ui, rect, color);
    }
}

pub fn separator(ui: &Ui, x: f32, top: f32, height: f32) {
    pixel(
        ui,
        Rect::from_min_size(pos2(x, top), vec2(1.0, height)),
        Color32::from_gray(215),
    );
}

pub fn label(ui: &Ui, rect: Rect, text: &str) {
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.text(
        painter.round_pos_to_pixels(rect.left_center() + vec2(2.0, 0.0)),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.0),
        Color32::BLACK,
    );
}

/// Fixed editor fields are 100 / 133 / 133 physical logical pixels at 100%.
/// The remaining part of this same row is the horizontal scrollbar.
pub fn pane_status(ui: &Ui, rect: Rect, position: &str, format: &str, modified: bool) -> Rect {
    pixel(ui, rect, BcTheme::WINDOW_BG);
    pixel(
        ui,
        Rect::from_min_size(rect.min, vec2(rect.width(), 1.0)),
        Color32::from_gray(215),
    );
    let mut x = rect.left();
    for (width, text) in [
        (100.0, position.to_owned()),
        (133.0, format.to_owned()),
        (
            133.0,
            if modified {
                crate::i18n::tr("Modificado")
            } else {
                String::new()
            },
        ),
    ] {
        let right = (x + width).min(rect.right());
        if right > x {
            label(
                ui,
                Rect::from_min_max(pos2(x, rect.top()), pos2(right, rect.bottom())),
                &text,
            );
        }
        x = right;
        if x < rect.right() {
            separator(ui, x, rect.top() + 1.0, rect.height() - 2.0);
        }
    }
    Rect::from_min_max(
        pos2((x + 1.0).min(rect.right()), rect.top() + 1.0),
        rect.right_bottom(),
    )
}

/// One thin Windows-style scrollbar. The caller owns the scroll offset, so the
/// same wheel, keyboard, thumb and arrow actions all update the actual viewport.
pub fn scroll_bar(
    ui: &Ui,
    id: Id,
    rect: Rect,
    axis: Axis,
    content: f32,
    viewport: f32,
    offset: &mut f32,
) -> Response {
    let arrows = matches!(axis, Axis::HorizontalWithArrows);
    let vertical = matches!(axis, Axis::Vertical);
    let length = if vertical {
        rect.height()
    } else {
        rect.width()
    };
    let coord = |p: Pos2| if vertical { p.y } else { p.x };
    let start = coord(rect.min);
    let maximum = (content - viewport).max(0.0);
    *offset = offset.clamp(0.0, maximum);
    pixel(ui, rect, BcTheme::WINDOW_BG);
    let end = if arrows {
        17.0_f32.min(length / 2.0)
    } else {
        0.0
    };
    let track_len = (length - end * 2.0).max(0.0);
    let thumb_len = if content > 0.0 {
        (track_len * (viewport / content).min(1.0))
            .max(18.0)
            .min(track_len)
    } else {
        track_len
    };
    let travel = (track_len - thumb_len).max(0.0);
    let thumb_at = start
        + end
        + if maximum > 0.0 {
            *offset / maximum * travel
        } else {
            0.0
        };
    let thumb = if vertical {
        Rect::from_min_size(pos2(rect.center().x - 1.5, thumb_at), vec2(3.0, thumb_len))
    } else {
        Rect::from_min_size(pos2(thumb_at, rect.center().y - 1.5), vec2(thumb_len, 3.0))
    };
    let response = ui.interact(rect, id, Sense::click_and_drag());
    if response.hovered() && maximum > 0.0 {
        let delta = ui.input_mut(|input| {
            let d = if vertical {
                input.smooth_scroll_delta.y
            } else {
                input.smooth_scroll_delta.x
            };
            if vertical {
                input.smooth_scroll_delta.y = 0.0;
            } else {
                input.smooth_scroll_delta.x = 0.0;
            }
            d
        });
        if delta != 0.0 {
            *offset = (*offset - delta).clamp(0.0, maximum);
            ui.ctx().request_repaint();
        }
    }
    if maximum > 0.0 && travel > 0.0 {
        if response.drag_started() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let p = coord(pointer);
                let grab = if p >= thumb_at && p <= thumb_at + thumb_len {
                    p - thumb_at
                } else {
                    thumb_len / 2.0
                };
                ui.ctx().data_mut(|d| d.insert_temp(id.with("grab"), grab));
            }
        }
        if (response.dragged() || response.clicked()) && response.interact_pointer_pos().is_some() {
            let p = coord(response.interact_pointer_pos().unwrap());
            let grab = ui
                .ctx()
                .data(|d| d.get_temp::<f32>(id.with("grab")))
                .unwrap_or(thumb_len / 2.0);
            if p < start + end {
                *offset = (*offset - 32.0).max(0.0);
            } else if p >= start + length - end {
                *offset = (*offset + 32.0).min(maximum);
            } else {
                *offset = ((p - start - end - grab) / travel).clamp(0.0, 1.0) * maximum;
            }
            ui.ctx().request_repaint();
        }
        pixel(
            ui,
            thumb,
            Color32::from_gray(if response.hovered() { 100 } else { 133 }),
        );
    }
    if arrows && length >= 18.0 {
        for (at, direction) in [(start + end / 2.0, -1.0), (start + length - end / 2.0, 1.0)] {
            let center = if vertical {
                pos2(rect.center().x, at)
            } else {
                pos2(at, rect.center().y)
            };
            let points = if vertical {
                vec![
                    center + vec2(-2.5, -direction),
                    center + vec2(0.0, 1.5 * direction),
                    center + vec2(2.5, -direction),
                ]
            } else {
                vec![
                    center + vec2(-direction, -2.5),
                    center + vec2(1.5 * direction, 0.0),
                    center + vec2(-direction, 2.5),
                ]
            };
            ui.painter().add(Shape::line(
                points,
                Stroke::new(1.0_f32, Color32::from_gray(153)),
            ));
        }
    }
    response
}

/// Native NavImages are 7 x 9 at 100%; preserve their image scale and pixel grid.
pub fn navigation(
    ui: &Ui,
    icons: &mut IconManager,
    rect: Rect,
    mode: &mut NavigationMode,
) -> Option<i8> {
    pixel(ui, rect, BcTheme::WINDOW_BG);
    let mut chosen = None;
    for (i, (name, hint, value)) in [
        ("imgNavPrev", "Seção Anterior", -1),
        ("imgNavPick", "Escolher modo de navegação", 0),
        ("imgNavNext", "Próxima Seção", 1),
    ]
    .into_iter()
    .enumerate()
    {
        let hit = Rect::from_min_size(
            rect.min + vec2(0.0, 18.0 * i as f32),
            vec2(rect.width(), 18.0),
        );
        let response = ui.interact(hit, ui.id().with(("nav", i)), Sense::click());
        if response.hovered() {
            pixel(ui, hit.shrink(1.0), Color32::from_rgb(229, 243, 255));
        }
        if let Some(texture) = icons.get_icon(ui.ctx(), name) {
            let size = texture.size_vec2() / ui.ctx().pixels_per_point();
            let min = ui.painter().round_pos_to_pixels(hit.center() - size / 2.0);
            ui.painter().image(
                texture.id(),
                Rect::from_min_size(min, size),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let popup = ui.id().with("nav_picker");
        if response
            .clone()
            .on_hover_text(crate::i18n::tr(hint))
            .clicked()
        {
            if value == 0 {
                ui.memory_mut(|m| m.toggle_popup(popup));
            } else {
                chosen = Some(value);
            }
        }
        if value == 0 {
            popup::popup_below_widget(
                ui,
                popup,
                &response,
                PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    for (label, kind) in [
                        ("Navegar pela diferença", Some(NavigationMode::Differences)),
                        (
                            "Navegar pela seção de diferença",
                            Some(NavigationMode::Sections),
                        ),
                        ("Navegar por substituição", None),
                        ("Navegar por Editar", None),
                        ("Navegar por Localizar", Some(NavigationMode::Find)),
                    ] {
                        let response = ui.add_enabled(
                            kind.is_some(),
                            Button::new(crate::i18n::tr(label)).selected(kind == Some(*mode)),
                        );
                        if response.clicked() {
                            *mode = kind.unwrap();
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                },
            );
        }
    }
    chosen
}

/// Outlined detail-margin arrows measured from the original editor, not a
/// platform-dependent Unicode font fallback.
pub fn detail_arrow(ui: &Ui, rect: Rect, right: bool) {
    let center = ui.painter().round_pos_to_pixels(rect.center());
    let sign = if right { 1.0 } else { -1.0 };
    let points = [
        (-5.0, -2.0),
        (0.0, -2.0),
        (0.0, -4.0),
        (5.0, 0.0),
        (0.0, 4.0),
        (0.0, 2.0),
        (-5.0, 2.0),
        (-5.0, -2.0),
    ];
    ui.painter().add(Shape::line(
        points
            .into_iter()
            .map(|(x, y)| center + vec2(x * sign, y))
            .collect(),
        Stroke::new(1.0_f32, Color32::from_rgb(112, 128, 137)),
    ));
}
