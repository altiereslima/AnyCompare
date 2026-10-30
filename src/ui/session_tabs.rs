//! Shared session tabs, placed between the view's menu and its toolbar.
use super::icons::IconManager;
use egui::*;

pub const HEIGHT: f32 = 25.0;
const TAB_WIDTH: f32 = 286.0;

#[derive(Clone)]
pub struct Tab {
    pub title: String,
    pub icon: &'static str,
}

#[derive(Clone, Default)]
struct Strip {
    tabs: Vec<Tab>,
    active: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Activate(usize),
    Close(usize),
    CloseHome,
}

fn model_id() -> Id {
    Id::new("anycompare_session_tabs")
}

pub fn set(ctx: &Context, tabs: Vec<Tab>, active: Option<usize>) {
    ctx.data_mut(|data| data.insert_temp(model_id(), Strip { tabs, active }));
}

pub fn visible(ctx: &Context) -> bool {
    ctx.data(|data| data.get_temp::<Strip>(model_id()))
        .is_some_and(|strip| {
            strip.tabs.len() > 1 || strip.active.is_none() && !strip.tabs.is_empty()
        })
}

pub fn take_action(ctx: &Context) -> Option<Action> {
    ctx.data_mut(|data| {
        let id = model_id().with("action");
        let action = data.get_temp::<Action>(id);
        data.remove::<Action>(id);
        action
    })
}

pub fn show(ui: &mut Ui, icons: &mut IconManager) {
    if !visible(ui.ctx()) {
        return;
    }
    let strip = ui
        .ctx()
        .data(|data| data.get_temp::<Strip>(model_id()))
        .unwrap();
    let mut action = None;
    TopBottomPanel::top("session_tabs")
        .exact_height(HEIGHT)
        .show_separator_line(false)
        .frame(Frame::none())
        .show_inside(ui, |ui| {
            super::chrome::gradient(
                ui,
                ui.max_rect(),
                Color32::from_rgb(215, 222, 229),
                Color32::from_rgb(198, 207, 216),
            );
            let active = strip.active.unwrap_or(usize::MAX);
            let changed = ui
                .ctx()
                .data(|data| data.get_temp::<usize>(model_id().with("active")))
                != Some(active);
            ui.ctx()
                .data_mut(|data| data.insert_temp(model_id().with("active"), active));
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ScrollArea::horizontal()
                .id_salt("session_tab_scroll")
                .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        let home = Tab {
                            title: crate::i18n::tr("Página Inicial"),
                            icon: "imgSessionHome",
                        };
                        for (index, tab) in strip
                            .tabs
                            .iter()
                            .chain(strip.active.is_none().then_some(&home))
                            .enumerate()
                        {
                            let selected = strip.active == Some(index)
                                || strip.active.is_none() && index == strip.tabs.len();
                            let (slot, _) =
                                ui.allocate_exact_size(vec2(TAB_WIDTH, HEIGHT), Sense::hover());
                            let rect = Rect::from_min_max(
                                slot.min + vec2(0.0, if selected { 0.0 } else { 3.0 }),
                                slot.max,
                            );
                            let close_rect = Rect::from_center_size(
                                pos2(rect.right() - 11.5, slot.top() + 11.5),
                                vec2(11.0, 11.0),
                            );
                            let body = Rect::from_min_max(
                                rect.min,
                                pos2(close_rect.left() - 4.0, rect.bottom()),
                            );
                            let response =
                                ui.interact(body, model_id().with(("tab", index)), Sense::click());
                            let close = ui.interact(
                                close_rect.expand(3.0),
                                model_id().with(("close", index)),
                                Sense::click(),
                            );
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(model_id().with(("rect", index)), rect);
                                data.insert_temp(
                                    model_id().with(("close_rect", index)),
                                    close_rect,
                                );
                            });
                            let painter =
                                ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                            let (top, bottom) = if selected {
                                (
                                    Color32::from_rgb(245, 248, 250),
                                    Color32::from_rgb(237, 241, 244),
                                )
                            } else {
                                (
                                    Color32::from_rgb(214, 221, 228),
                                    Color32::from_rgb(198, 207, 217),
                                )
                            };
                            let tab_ui = ui.new_child(UiBuilder::new().max_rect(rect));
                            super::chrome::gradient(&tab_ui, rect, top, bottom);
                            let border = Color32::from_rgb(167, 151, 122);
                            for line in [
                                [rect.left_bottom(), rect.left_top() + vec2(0.0, 2.0)],
                                [
                                    rect.left_top() + vec2(0.0, 2.0),
                                    rect.left_top() + vec2(2.0, 0.0),
                                ],
                                [
                                    rect.left_top() + vec2(2.0, 0.0),
                                    rect.right_top() - vec2(2.0, 0.0),
                                ],
                                [
                                    rect.right_top() - vec2(2.0, 0.0),
                                    rect.right_top() + vec2(0.0, 2.0),
                                ],
                                [rect.right_top() + vec2(0.0, 2.0), rect.right_bottom()],
                            ] {
                                painter.line_segment(line, Stroke::new(1.0_f32, border));
                            }
                            if !selected {
                                painter.hline(
                                    rect.x_range(),
                                    rect.bottom() - 0.5,
                                    Stroke::new(1.0_f32, border),
                                );
                            }
                            if let Some(icon) = icons.get_icon(ui.ctx(), tab.icon) {
                                painter.image(
                                    icon.id(),
                                    Rect::from_min_size(
                                        slot.min + vec2(4.0, 5.0),
                                        vec2(16.0, 16.0),
                                    ),
                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }
                            let text_clip = Rect::from_min_max(
                                rect.min + vec2(23.0, 0.0),
                                pos2(close_rect.left() - 5.0, rect.bottom()),
                            );
                            painter.with_clip_rect(text_clip).text(
                                pos2(text_clip.left(), slot.top() + 14.0),
                                Align2::LEFT_CENTER,
                                &tab.title,
                                FontId::proportional(12.0),
                                Color32::BLACK,
                            );
                            if selected || close.hovered() {
                                painter.rect_filled(close_rect, 2.0, Color32::from_gray(150));
                            }
                            let color = if selected || close.hovered() {
                                Color32::from_gray(220)
                            } else {
                                Color32::from_gray(140)
                            };
                            for (a, b) in [
                                (vec2(-2.5, -2.5), vec2(2.5, 2.5)),
                                (vec2(-2.5, 2.5), vec2(2.5, -2.5)),
                            ] {
                                painter.line_segment(
                                    [close_rect.center() + a, close_rect.center() + b],
                                    Stroke::new(1.0_f32, color),
                                );
                            }
                            if selected && changed {
                                ui.scroll_to_rect(slot, Some(Align::Center));
                            }
                            if response.clicked() && index < strip.tabs.len() {
                                action = Some(Action::Activate(index));
                            }
                            if close.clicked() || response.clicked_by(PointerButton::Middle) {
                                action = Some(if index < strip.tabs.len() {
                                    Action::Close(index)
                                } else {
                                    Action::CloseHome
                                });
                            }
                            response.on_hover_text(&tab.title);
                            close.on_hover_text(crate::i18n::tr("Fechar sessão"));
                        }
                    });
                });
        });
    if let Some(action) = action {
        ui.ctx()
            .data_mut(|data| data.insert_temp(model_id().with("action"), action));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Vec<Tab> {
        vec![
            Tab {
                title: "left <--> right".into(),
                icon: "imgDirCompareSession",
            },
            Tab {
                title: "Readme.txt".into(),
                icon: "imgTextCompareSession",
            },
        ]
    }

    fn render(ctx: &Context, icons: &mut IconManager, events: Vec<Event>) {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(958.0, 720.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default()
                    .frame(Frame::none())
                    .show(ctx, |ui| {
                        TopBottomPanel::top("menu")
                            .exact_height(super::super::chrome::MENU_BAND_HEIGHT)
                            .frame(Frame::none())
                            .show_inside(ui, |_| {});
                        show(ui, icons);
                    });
            },
        );
    }

    #[test]
    fn tab_strip_follows_the_menu_and_keeps_close_clicks_separate() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        set(&ctx, tabs(), Some(1));
        render(&ctx, &mut icons, Vec::new());
        let active = ctx
            .data(|d| d.get_temp::<Rect>(model_id().with(("rect", 1usize))))
            .unwrap();
        assert_eq!(active.top(), super::super::chrome::MENU_BAND_HEIGHT);
        assert_eq!(active.height(), HEIGHT);
        assert_eq!(active.width(), TAB_WIDTH);
        let close = ctx
            .data(|d| d.get_temp::<Rect>(model_id().with(("close_rect", 1usize))))
            .unwrap()
            .center();
        for pressed in [true, false] {
            render(
                &ctx,
                &mut icons,
                vec![
                    Event::PointerMoved(close),
                    Event::PointerButton {
                        pos: close,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(take_action(&ctx), Some(Action::Close(1)));
        assert_eq!(take_action(&ctx), None);
        let first = ctx
            .data(|d| d.get_temp::<Rect>(model_id().with(("rect", 0usize))))
            .unwrap()
            .center();
        for pressed in [true, false] {
            render(
                &ctx,
                &mut icons,
                vec![
                    Event::PointerMoved(first),
                    Event::PointerButton {
                        pos: first,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(take_action(&ctx), Some(Action::Activate(0)));
    }

    #[test]
    fn home_is_only_a_tab_when_its_view_is_open() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        set(&ctx, vec![tabs().remove(0)], Some(0));
        assert!(!visible(&ctx));
        set(&ctx, vec![tabs().remove(0)], None);
        assert!(visible(&ctx));
        render(&ctx, &mut icons, Vec::new());
        let close = ctx
            .data(|d| d.get_temp::<Rect>(model_id().with(("close_rect", 1usize))))
            .unwrap()
            .center();
        for pressed in [true, false] {
            render(
                &ctx,
                &mut icons,
                vec![
                    Event::PointerMoved(close),
                    Event::PointerButton {
                        pos: close,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(take_action(&ctx), Some(Action::CloseHome));
    }
}
