//! Lightweight frame-level UI layout auditor active only when `--dump-layout`
//! is passed to developer captures. Zero allocations in normal operation.

use egui::{Context, Id, Rect};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct RectBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl From<Rect> for RectBox {
    fn from(r: Rect) -> Self {
        Self {
            x: (r.min.x * 10.0).round() / 10.0,
            y: (r.min.y * 10.0).round() / 10.0,
            w: (r.width() * 10.0).round() / 10.0,
            h: (r.height() * 10.0).round() / 10.0,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MenuButtonEntry {
    pub label: String,
    pub caption: String,
    pub rect: RectBox,
}

#[derive(Clone, Debug, Serialize)]
pub struct MenuItemEntry {
    pub label: String,
    pub caption: String,
    pub shortcut: String,
    pub icon: Option<String>,
    pub enabled: bool,
    pub rect: RectBox,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolEntry {
    pub name: String,
    pub short: String,
    pub kind: String,
    pub enabled: bool,
    pub checked: bool,
    pub rect: RectBox,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FrameAudit {
    pub viewport: Option<RectBox>,
    pub menu_buttons: Vec<MenuButtonEntry>,
    pub menu_items: Vec<MenuItemEntry>,
    pub tools: Vec<ToolEntry>,
}

fn enabled_id() -> Id {
    Id::new("anycompare_audit_enabled")
}

fn frame_id() -> Id {
    Id::new("anycompare_audit_frame")
}

pub fn set_enabled(ctx: &Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(enabled_id(), enabled));
}

pub fn is_enabled(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(enabled_id()).unwrap_or(false))
}

pub fn begin_frame(ctx: &Context) {
    if !is_enabled(ctx) {
        return;
    }
    let viewport = Some(RectBox::from(ctx.screen_rect()));
    ctx.data_mut(|d| {
        d.insert_temp(
            frame_id(),
            FrameAudit {
                viewport,
                ..Default::default()
            },
        );
    });
}

pub fn record_menu_button(ctx: &Context, label: &str, caption: &str, rect: Rect) {
    if !is_enabled(ctx) {
        return;
    }
    ctx.data_mut(|d| {
        let log = d.get_temp_mut_or_default::<FrameAudit>(frame_id());
        log.menu_buttons.push(MenuButtonEntry {
            label: label.to_owned(),
            caption: caption.to_owned(),
            rect: rect.into(),
        });
    });
}

pub fn record_menu_item(
    ctx: &Context,
    label: &str,
    caption: &str,
    shortcut: &str,
    icon: Option<&str>,
    enabled: bool,
    rect: Rect,
) {
    if !is_enabled(ctx) {
        return;
    }
    ctx.data_mut(|d| {
        let log = d.get_temp_mut_or_default::<FrameAudit>(frame_id());
        log.menu_items.push(MenuItemEntry {
            label: label.to_owned(),
            caption: caption.to_owned(),
            shortcut: shortcut.to_owned(),
            icon: icon.map(str::to_owned),
            enabled,
            rect: rect.into(),
        });
    });
}

pub fn record_tool(
    ctx: &Context,
    name: &str,
    short: &str,
    kind: &str,
    enabled: bool,
    checked: bool,
    rect: Rect,
) {
    if !is_enabled(ctx) {
        return;
    }
    ctx.data_mut(|d| {
        let log = d.get_temp_mut_or_default::<FrameAudit>(frame_id());
        log.tools.push(ToolEntry {
            name: name.to_owned(),
            short: short.to_owned(),
            kind: kind.to_owned(),
            enabled,
            checked,
            rect: rect.into(),
        });
    });
}

pub fn snapshot(ctx: &Context) -> FrameAudit {
    ctx.data(|d| d.get_temp::<FrameAudit>(frame_id()).unwrap_or_default())
}
