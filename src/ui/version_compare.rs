use super::{
    chrome,
    dialogs::{DialogField, DialogHost},
    history_combo,
    icons::IconManager,
    session_menu, session_tabs,
    theme::BcTheme,
    toolbar::{self, ToolEvent, ToolState},
    toolbar_spec,
};
use crate::{
    i18n::tr,
    pe_version::{Image, Node},
    table::Filter,
    table_columns::Change,
};
use egui::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

mod property_context;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Options {
    filter: Filter,
    ignore_minor: bool,
    ignored: Vec<String>,
    closed: BTreeSet<String>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            filter: Filter::All,
            ignore_minor: false,
            ignored: vec!["additional/sha256".into()],
            closed: BTreeSet::from(["additional".into()]),
        }
    }
}
struct Row {
    key: String,
    depth: usize,
    names: [Option<String>; 2],
    values: [Option<String>; 2],
    group: bool,
    change: Change,
}
fn flatten<'a>(
    nodes: &'a [Node],
    prefix: &str,
    depth: usize,
    result: &mut Vec<(String, usize, &'a Node)>,
) {
    for node in nodes {
        let key = if prefix.is_empty() {
            node.key.clone()
        } else {
            format!("{prefix}/{}", node.key)
        };
        result.push((key.clone(), depth, node));
        flatten(&node.children, &key, depth + 1, result);
    }
}
pub struct VersionCompareScreen {
    kind: crate::history::SessionKind,
    pub left_path: String,
    pub right_path: String,
    left: Option<Image>,
    right: Option<Image>,
    rows: Vec<Row>,
    options: Options,
    selected: usize,
    active_side: usize,
    jump: bool,
    error: Option<String>,
    show_toolbar: bool,
    show_file_info: bool,
    show_text_details: bool,
    show_hex_details: bool,
    visible_whitespace: bool,
    over_under: bool,
    font_size: f32,
    base_key: String,
    base_key_display: String,
    load_seconds: f32,
    find_query: String,
    undo_stack: Vec<(Option<Image>, Option<Image>)>,
    redo_stack: Vec<(Option<Image>, Option<Image>)>,
}
impl VersionCompareScreen {
    pub fn record_paths(&self) -> Option<(PathBuf, PathBuf)> {
        Some((
            self.left.as_ref()?.path.clone()?,
            self.right.as_ref()?.path.clone()?,
        ))
    }
    pub fn set_filter(&mut self, filter: Filter) {
        self.options.filter = filter;
    }
    pub fn new() -> Self {
        Self {
            kind: crate::history::SessionKind::Version,
            left_path: String::new(),
            right_path: String::new(),
            left: None,
            right: None,
            rows: Vec::new(),
            options: Options::default(),
            selected: 0,
            active_side: 0,
            jump: false,
            error: None,
            show_toolbar: true,
            show_file_info: true,
            show_text_details: true,
            show_hex_details: false,
            visible_whitespace: false,
            over_under: false,
            font_size: 12.0,
            base_key: String::new(),
            base_key_display: String::new(),
            load_seconds: 0.01,
            find_query: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push((self.left.clone(), self.right.clone()));
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some((l, r)) = self.undo_stack.pop() {
            self.redo_stack.push((self.left.clone(), self.right.clone()));
            self.left = l;
            self.right = r;
            self.compare();
        }
    }

    pub fn redo(&mut self) {
        if let Some((l, r)) = self.redo_stack.pop() {
            self.undo_stack.push((self.left.clone(), self.right.clone()));
            self.left = l;
            self.right = r;
            self.compare();
        }
    }

    pub fn copy_to_other_side(&mut self) {
        if self.kind != crate::history::SessionKind::Registry {
            return;
        }
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        let from_side = self.active_side;
        if row.names[from_side].is_none() {
            return;
        }
        let key_path = row.key.clone();
        let src_image = if from_side == 0 {
            self.left.as_ref()
        } else {
            self.right.as_ref()
        };
        let Some(src_node) = src_image.and_then(|img| find_node_by_key(&img.nodes, &key_path)).cloned() else {
            return;
        };
        self.push_undo();
        let dst_image = if from_side == 0 {
            &mut self.right
        } else {
            &mut self.left
        };
        if let Some(dst) = dst_image {
            upsert_node_by_key(&mut dst.nodes, &key_path, src_node, &self.rows);
            self.compare();
        }
    }

    pub fn delete_selected(&mut self) {
        if self.kind != crate::history::SessionKind::Registry {
            return;
        }
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        let side = self.active_side;
        if row.names[side].is_none() {
            return;
        }
        let key_path = row.key.clone();
        self.push_undo();
        let target = if side == 0 {
            &mut self.left
        } else {
            &mut self.right
        };
        if let Some(img) = target {
            remove_node_by_key(&mut img.nodes, &key_path);
            self.compare();
        }
    }

    pub fn rename_selected(&mut self, dialogs: &mut dyn DialogHost) {
        if self.kind != crate::history::SessionKind::Registry {
            return;
        }
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        let side = self.active_side;
        let Some(cur_name) = row.names[side].clone() else {
            return;
        };
        let is_group = row.group;
        let key_path = row.key.clone();
        let mut fields = vec![DialogField::Text(tr("common.name"), cur_name.clone())];
        if dialogs.fields("common.rename", &mut fields) {
            if let Some(DialogField::Text(_, new_name)) = fields.into_iter().next() {
                let trimmed = new_name.trim().to_owned();
                if !trimmed.is_empty() && trimmed != cur_name {
                    self.push_undo();
                    let target = if side == 0 {
                        &mut self.left
                    } else {
                        &mut self.right
                    };
                    if let Some(img) = target {
                        if let Some(node) = find_node_by_key_mut(&mut img.nodes, &key_path) {
                            node.name = trimmed.clone();
                            node.key = if is_group {
                                trimmed.to_lowercase()
                            } else {
                                format!("value:{}", trimmed.to_lowercase())
                            };
                        }
                        self.compare();
                    }
                }
            }
        }
    }

    pub fn new_key_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        if self.kind != crate::history::SessionKind::Registry {
            return;
        }
        let mut fields = vec![DialogField::Text(tr("common.name"), "NewKey".into())];
        if !dialogs.fields("version.new_key", &mut fields) {
            return;
        }
        let Some(DialogField::Text(_, name)) = fields.into_iter().next() else {
            return;
        };
        let trimmed = name.trim().to_owned();
        if trimmed.is_empty() {
            return;
        }
        let parent_key = self
            .rows
            .get(self.selected)
            .map(|r| {
                if r.group {
                    r.key.clone()
                } else {
                    r.key.rsplit_once('/').map(|(p, _)| p.to_owned()).unwrap_or_default()
                }
            })
            .unwrap_or_else(|| self.base_key.clone());
        let full_key = if parent_key.is_empty() {
            trimmed.to_lowercase()
        } else {
            format!("{parent_key}/{}", trimmed.to_lowercase())
        };
        let new_node = Node {
            key: trimmed.to_lowercase(),
            name: trimmed,
            value: String::new(),
            bytes: None,
            children: Vec::new(),
        };
        self.push_undo();
        let target = if self.active_side == 0 {
            &mut self.left
        } else {
            &mut self.right
        };
        if let Some(img) = target {
            upsert_node_by_key(&mut img.nodes, &full_key, new_node, &self.rows);
            self.compare();
        }
    }

    pub fn new_value_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        if self.kind != crate::history::SessionKind::Registry {
            return;
        }
        let types = vec![
            "REG_SZ".into(),
            "REG_DWORD".into(),
            "REG_QWORD".into(),
            "REG_BINARY".into(),
            "REG_EXPAND_SZ".into(),
            "REG_MULTI_SZ".into(),
        ];
        let mut fields = vec![
            DialogField::Text(tr("common.name"), "NewValue".into()),
            DialogField::Choice(tr("table.type").trim_end_matches(':').to_owned(), 0, types.clone()),
            DialogField::Text(tr("version.data"), String::new()),
        ];
        if !dialogs.fields("version.new_value", &mut fields) {
            return;
        }
        let mut it = fields.into_iter();
        let Some(DialogField::Text(_, name)) = it.next() else { return };
        let Some(DialogField::Choice(_, type_idx, _)) = it.next() else { return };
        let Some(DialogField::Text(_, data)) = it.next() else { return };
        let trimmed = name.trim().to_owned();
        if trimmed.is_empty() {
            return;
        }
        let reg_type = types.get(type_idx).map(String::as_str).unwrap_or("REG_SZ");
        let formatted_val = format_registry_edit_value(reg_type, data.trim());
        let parent_key = self
            .rows
            .get(self.selected)
            .map(|r| {
                if r.group {
                    r.key.clone()
                } else {
                    r.key.rsplit_once('/').map(|(p, _)| p.to_owned()).unwrap_or_default()
                }
            })
            .unwrap_or_else(|| self.base_key.clone());
        let leaf_key = format!("value:{}", trimmed.to_lowercase());
        let full_key = if parent_key.is_empty() {
            leaf_key.clone()
        } else {
            format!("{parent_key}/{leaf_key}")
        };
        let new_node = Node {
            key: leaf_key,
            name: trimmed,
            value: formatted_val.clone(),
            bytes: Some(std::sync::Arc::from(formatted_val.into_bytes())),
            children: Vec::new(),
        };
        self.push_undo();
        let target = if self.active_side == 0 {
            &mut self.left
        } else {
            &mut self.right
        };
        if let Some(img) = target {
            upsert_node_by_key(&mut img.nodes, &full_key, new_node, &self.rows);
            self.compare();
        }
    }

    pub fn modify_selected(&mut self, dialogs: &mut dyn DialogHost) {
        if self.kind != crate::history::SessionKind::Registry {
            return;
        }
        let Some(row) = self.rows.get(self.selected).filter(|r| !r.group) else {
            return;
        };
        let side = self.active_side;
        let Some(cur_val) = row.values[side].clone() else {
            return;
        };
        let (cur_type, cur_data) = split_registry_value(&cur_val);
        let cur_type_owned = cur_type.to_owned();
        let mut fields = vec![DialogField::Text(tr("version.data"), cur_data.to_owned())];
        if dialogs.fields("common.modify_ellipsis", &mut fields) {
            if let Some(DialogField::Text(_, new_data)) = fields.into_iter().next() {
                let formatted = format_registry_edit_value(&cur_type_owned, new_data.trim());
                let key_path = row.key.clone();
                self.push_undo();
                let target = if side == 0 {
                    &mut self.left
                } else {
                    &mut self.right
                };
                if let Some(img) = target {
                    if let Some(node) = find_node_by_key_mut(&mut img.nodes, &key_path) {
                        node.value = formatted.clone();
                        node.bytes = Some(std::sync::Arc::from(formatted.into_bytes()));
                    }
                    self.compare();
                }
            }
        }
    }

    pub fn find_dialog(&mut self, dialogs: &mut dyn DialogHost) {
        let mut fields = vec![DialogField::Text(
            tr("common.find_what"),
            self.find_query.clone(),
        )];
        if dialogs.fields("menu.search.find", &mut fields) {
            if let Some(DialogField::Text(_, q)) = fields.into_iter().next() {
                self.find_query = q;
                self.find_step(true);
            }
        }
    }

    pub fn find_step(&mut self, forward: bool) {
        let q = self.find_query.trim().to_lowercase();
        if q.is_empty() {
            return;
        }
        let vis = self.visible();
        if vis.is_empty() {
            return;
        }
        let cur_pos = vis.iter().position(|&i| i == self.selected).unwrap_or(0);
        for step in 1..=vis.len() {
            let v_idx = if forward {
                (cur_pos + step) % vis.len()
            } else {
                (cur_pos + vis.len() - (step % vis.len())) % vis.len()
            };
            let r_idx = vis[v_idx];
            let row = &self.rows[r_idx];
            let matches = row
                .names
                .iter()
                .chain(row.values.iter())
                .flatten()
                .any(|s| s.to_lowercase().contains(&q));
            if matches {
                self.selected = r_idx;
                self.jump = true;
                break;
            }
        }
    }

    fn compute_default_base_key(&self) -> (String, String) {
        if self.kind != crate::history::SessionKind::Registry {
            return (String::new(), String::new());
        }
        let mut a_cur = self
            .left
            .as_ref()
            .map(|i| i.nodes.as_slice())
            .unwrap_or(&[]);
        let mut b_cur = self
            .right
            .as_ref()
            .map(|i| i.nodes.as_slice())
            .unwrap_or(&[]);
        let mut key_parts = Vec::new();
        let mut display_parts = Vec::new();
        loop {
            let next = match (a_cur, b_cur) {
                ([a], [b])
                    if !a.key.starts_with("value:")
                        && !a.children.is_empty()
                        && a.key == b.key
                        && !b.children.is_empty() =>
                {
                    Some((
                        a.key.as_str(),
                        a.name.as_str(),
                        a.children.as_slice(),
                        b.children.as_slice(),
                    ))
                }
                ([a], []) if !a.key.starts_with("value:") && !a.children.is_empty() => {
                    Some((a.key.as_str(), a.name.as_str(), a.children.as_slice(), &[][..]))
                }
                ([], [b]) if !b.key.starts_with("value:") && !b.children.is_empty() => {
                    Some((b.key.as_str(), b.name.as_str(), &[][..], b.children.as_slice()))
                }
                _ => None,
            };
            let Some((k, name, na, nb)) = next else {
                break;
            };
            key_parts.push(k);
            display_parts.push(name);
            a_cur = na;
            b_cur = nb;
        }
        (key_parts.join("/"), display_parts.join("\\"))
    }

    pub fn up_one_level_all(&mut self) {
        if self.base_key.is_empty() {
            return;
        }
        if let Some((parent, _)) = self.base_key.rsplit_once('/') {
            self.base_key = parent.to_owned();
        } else {
            self.base_key.clear();
        }
        if let Some((parent, _)) = self.base_key_display.rsplit_once('\\') {
            self.base_key_display = parent.to_owned();
        } else {
            self.base_key_display.clear();
        }
        let vis = self.visible();
        if let Some(&first_vis) = vis.first() {
            self.selected = first_vis;
        }
    }

    pub fn enter_subkey(&mut self, row_index: usize) {
        if self.kind != crate::history::SessionKind::Registry {
            return;
        }
        let Some(row) = self.rows.get(row_index).filter(|r| r.group) else {
            return;
        };
        let target_key = row.key.clone();
        let mut display_parts = Vec::new();
        let mut prefix = String::new();
        for part in target_key.split('/') {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if let Some(anc) = self.rows.iter().find(|r| r.key == prefix) {
                if let Some(name) = anc.names[0].as_deref().or(anc.names[1].as_deref()) {
                    display_parts.push(name.to_owned());
                }
            }
        }
        self.base_key = target_key;
        self.base_key_display = display_parts.join("\\");
        let vis = self.visible();
        if let Some(&first_vis) = vis
            .iter()
            .find(|&&i| !self.rows[i].group)
            .or_else(|| vis.first())
        {
            self.selected = first_vis;
        }
    }

    pub fn is_selected_important(&self) -> bool {
        let Some(row) = self.rows.get(self.selected) else {
            return true;
        };
        !self
            .options
            .ignored
            .iter()
            .any(|ig| row.key == *ig || row.key.starts_with(&format!("{ig}/")))
    }

    pub fn toggle_selected_important(&mut self) {
        let Some(key) = self.rows.get(self.selected).map(|r| r.key.clone()) else {
            return;
        };
        if let Some(pos) = self.options.ignored.iter().position(|ig| *ig == key) {
            self.options.ignored.remove(pos);
        } else {
            self.options.ignored.push(key);
        }
        self.compare();
    }

    pub fn copy_key_name(&self, ctx: &Context) {
        if let Some(row) = self.rows.get(self.selected) {
            let name = row.names[self.active_side]
                .as_deref()
                .or(row.names[1 - self.active_side].as_deref())
                .unwrap_or(&row.key);
            ctx.copy_text(name.to_owned());
        }
    }

    fn report_title_key(&self) -> &'static str {
        match self.kind {
            crate::history::SessionKind::Registry => "registry.report",
            crate::history::SessionKind::Media => "media.report",
            _ => "version.report",
        }
    }

    fn info_title_key(&self) -> &'static str {
        match self.kind {
            crate::history::SessionKind::Registry => "registry.compare_info",
            crate::history::SessionKind::Media => "media.compare_info",
            _ => "version.compare_info",
        }
    }

    pub fn report(&self, dialogs: &mut dyn DialogHost) {
        let rows: Vec<crate::report::Row> = self
            .rows
            .iter()
            .map(|r| crate::report::Row {
                key: r.key.clone(),
                left: r.values[0].clone().unwrap_or_default(),
                right: r.values[1].clone().unwrap_or_default(),
                different: r.change != Change::Same,
            })
            .collect();
        let mut inputs = Vec::new();
        if !self.left_path.is_empty() {
            inputs.push(PathBuf::from(&self.left_path));
        }
        if !self.right_path.is_empty() {
            inputs.push(PathBuf::from(&self.right_path));
        }
        crate::report::export(
            self.report_title_key(),
            &self.left_path,
            &self.right_path,
            &rows,
            &inputs,
            dialogs,
        );
    }

    fn open_info_modal(&self, ctx: &Context) {
        let same = self
            .rows
            .iter()
            .filter(|r| !r.group && r.change == Change::Same)
            .count();
        let diff = self
            .rows
            .iter()
            .filter(|r| !r.group && r.change != Change::Same)
            .count();
        let rows = vec![
            (tr("folder.side_left"), self.left_path.clone()),
            (tr("folder.side_right"), self.right_path.clone()),
            (tr("common.same"), same.to_string()),
            (tr("common.differences"), diff.to_string()),
        ];
        super::info_modals::open_info(ctx, tr(self.info_title_key()), rows);
    }
    pub fn session_options(&self) -> serde_json::Value {
        serde_json::to_value(&self.options).unwrap_or_default()
    }
    pub fn for_kind(kind: crate::history::SessionKind) -> Self {
        let mut screen = Self::new();
        screen.kind = kind;
        if kind != crate::history::SessionKind::Version {
            screen.options.ignored = vec!["technical".into()];
            screen.options.closed = BTreeSet::from(["technical".into()]);
        }
        screen
    }
    pub fn kind(&self) -> crate::history::SessionKind {
        self.kind
    }
    pub fn active_left(&self) -> bool {
        self.active_side == 0
    }
    pub fn is_side_empty(&self, left: bool) -> bool {
        if left {
            self.left_path.is_empty() && self.left.is_none()
        } else {
            self.right_path.is_empty() && self.right.is_none()
        }
    }
    fn tools(&self) -> &'static [super::toolbar::ToolItem] {
        match self.kind {
            crate::history::SessionKind::Registry => toolbar_spec::REGISTRY_COMPARE,
            crate::history::SessionKind::Media => toolbar_spec::MP3_COMPARE,
            _ => toolbar_spec::VERSION_COMPARE,
        }
    }
    pub fn apply_session_options(&mut self, value: &serde_json::Value) {
        if let Ok(options) = serde_json::from_value(value.clone()) {
            self.options = options;
            self.compare();
        }
    }
    pub fn load_files(&mut self, left: &Path, right: &Path) -> Result<(), String> {
        let t0 = std::time::Instant::now();
        let kind = self.kind;
        let (a, b) = std::thread::scope(|scope| {
            let hb = scope.spawn(|| crate::property_sources::load(kind, right));
            let a = crate::property_sources::load(kind, left);
            (a, hb.join().unwrap())
        });
        let a = a?;
        let b = b?;
        self.left = Some(a);
        self.right = Some(b);
        self.left_path = left.to_string_lossy().into_owned();
        self.right_path = right.to_string_lossy().into_owned();
        self.error = None;
        self.load_seconds = t0.elapsed().as_secs_f32().max(0.01);
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.compare();
        (self.base_key, self.base_key_display) = self.compute_default_base_key();
        let vis = self.visible();
        if let Some(&first_vis) = vis.first() {
            self.selected = first_vis;
        }
        Ok(())
    }
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }
    pub fn refresh_source(&mut self, path: &Path, dialogs: &mut dyn DialogHost) {
        for side in [true, false] {
            let source = if side { &self.left } else { &self.right };
            if source
                .as_ref()
                .and_then(|s| s.path.as_deref())
                .is_some_and(|source| crate::document::same_file(source, path))
            {
                self.open_side(side, path, dialogs);
            }
        }
    }
    pub fn open_side(&mut self, left: bool, path: &Path, dialogs: &mut dyn DialogHost) {
        let t0 = std::time::Instant::now();
        match crate::property_sources::load(self.kind, path) {
            Ok(image) => {
                if left {
                    self.left = Some(image);
                    self.left_path = path.to_string_lossy().into_owned();
                } else {
                    self.right = Some(image);
                    self.right_path = path.to_string_lossy().into_owned();
                }
                self.error = None;
                self.load_seconds = t0.elapsed().as_secs_f32().max(0.01);
                self.compare();
                (self.base_key, self.base_key_display) = self.compute_default_base_key();
                let vis = self.visible();
                if let Some(&first_vis) = vis.first() {
                    self.selected = first_vis;
                }
            }
            Err(error) => dialogs.error(&error),
        }
    }
    fn compare(&mut self) {
        let mut a = Vec::new();
        let mut b = Vec::new();
        if let Some(image) = &self.left {
            flatten(&image.nodes, "", 0, &mut a);
        }
        if let Some(image) = &self.right {
            flatten(&image.nodes, "", 0, &mut b);
        }
        let right: BTreeMap<_, _> = b
            .iter()
            .map(|(key, depth, n)| (key.as_str(), (*depth, *n)))
            .collect();
        let left: BTreeSet<_> = a.iter().map(|(k, _, _)| k.as_str()).collect();
        let mut rows = Vec::new();
        for (key, depth, n) in &a {
            let other = right.get(key.as_str()).map(|(_, n)| *n);
            let change = if other.is_some_and(|other| n.same(other)) {
                Change::Same
            } else if self
                .options
                .ignored
                .iter()
                .any(|ignored| key == ignored || key.starts_with(&format!("{ignored}/")))
            {
                Change::Minor
            } else {
                Change::Important
            };
            rows.push(Row {
                key: key.clone(),
                depth: *depth,
                names: [Some(n.name.clone()), other.map(|n| n.name.clone())],
                values: [Some(n.value.clone()), other.map(|n| n.value.clone())],
                group: !n.children.is_empty(),
                change,
            });
        }
        for (key, depth, n) in &b {
            if !left.contains(key.as_str()) {
                rows.push(Row {
                    key: key.clone(),
                    depth: *depth,
                    names: [None, Some(n.name.clone())],
                    values: [None, Some(n.value.clone())],
                    group: !n.children.is_empty(),
                    change: Change::Important,
                });
            }
        }
        for i in (0..rows.len()).rev() {
            if rows[i].group {
                let prefix = format!("{}/", rows[i].key);
                rows[i].change = rows
                    .iter()
                    .filter(|r| r.key.starts_with(&prefix))
                    .map(|r| r.change)
                    .max()
                    .unwrap_or(rows[i].change);
            }
        }
        self.rows = rows;
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }
    fn visible(&self) -> Vec<usize> {
        let base_prefix = if self.kind == crate::history::SessionKind::Registry
            && !self.base_key.is_empty()
        {
            Some(format!("{}/", self.base_key))
        } else {
            None
        };
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                if let Some(prefix) = &base_prefix {
                    if !row.key.starts_with(prefix) {
                        return None;
                    }
                }
                let changed = if self.options.ignore_minor {
                    row.change == Change::Important
                } else {
                    row.change != Change::Same
                };
                let filter = match self.options.filter {
                    Filter::All => true,
                    Filter::Differences => changed,
                    Filter::Same => !changed,
                };
                (filter
                    && !self
                        .options
                        .closed
                        .iter()
                        .any(|key| row.key.starts_with(&format!("{key}/"))))
                .then_some(i)
            })
            .collect()
    }
    fn navigate(&mut self, forward: bool) {
        let different: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                (!row.group
                    && if self.options.ignore_minor {
                        row.change == Change::Important
                    } else {
                        row.change != Change::Same
                    })
                .then_some(i)
            })
            .collect();
        if different.is_empty() {
            return;
        }
        let index = if forward {
            different
                .iter()
                .copied()
                .find(|&i| i > self.selected)
                .unwrap_or(different[0])
        } else {
            different
                .iter()
                .rev()
                .copied()
                .find(|&i| i < self.selected)
                .unwrap_or(*different.last().unwrap())
        };
        self.selected = index;
        self.jump = true;
        let key = self.rows[index].key.clone();
        self.options
            .closed
            .retain(|closed| !key.starts_with(&format!("{closed}/")));
    }
    fn rules(&mut self, dialogs: &mut dyn DialogHost) {
        if self.kind != crate::history::SessionKind::Version {
            let keys: Vec<_> = self
                .rows
                .iter()
                .filter(|r| !r.group)
                .map(|r| {
                    (
                        r.key.clone(),
                        r.names[0]
                            .as_ref()
                            .or(r.names[1].as_ref())
                            .cloned()
                            .unwrap_or_default(),
                    )
                })
                .collect();
            let mut fields: Vec<_> = keys
                .iter()
                .map(|(key, name)| {
                    DialogField::Check(
                        property_name(self.kind, key, name),
                        !self.options.ignored.iter().any(|ignored| {
                            key == ignored || key.starts_with(&format!("{ignored}/"))
                        }),
                    )
                })
                .collect();
            if dialogs.fields("common.comparison_rules", &mut fields) {
                self.options.ignored.clear();
                for ((key, _), field) in keys.into_iter().zip(fields) {
                    self.options.ignored.retain(|k| k != &key);
                    if matches!(field, DialogField::Check(_, false)) {
                        self.options.ignored.push(key);
                    }
                }
                self.compare();
            }
            return;
        }
        let keys = [
            ("code/data", "version.compare_code_and_data"),
            ("pe/timestamp", "version.compare_creation_date"),
            ("pe/checksum", "version.compare_checksum"),
            ("pe/base", "version.compare_image_base"),
            ("additional/sha256", "version.compare_other_header_bytes"),
        ];
        let mut fields: Vec<_> = keys
            .iter()
            .map(|(key, label)| {
                DialogField::Check(tr(label), !self.options.ignored.iter().any(|k| k == key))
            })
            .collect();
        if dialogs.fields("common.comparison_rules", &mut fields) {
            for ((key, _), field) in keys.into_iter().zip(fields) {
                self.options.ignored.retain(|k| k != key);
                if matches!(field, DialogField::Check(_, false)) {
                    self.options.ignored.push(key.into());
                }
            }
            self.compare();
        }
    }
    fn pick(&mut self, left: bool, dialogs: &mut dyn DialogHost) {
        let path = PathBuf::from(if left {
            &self.left_path
        } else {
            &self.right_path
        });
        if let Some(path) = dialogs.file(false, left, Some(&path)) {
            self.open_side(left, &path, dialogs);
        }
    }
    pub fn ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) -> bool {
        let mut home = false;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        TopBottomPanel::top("version_menu")
            .exact_height(chrome::MENU_BAND_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                if self.kind == crate::history::SessionKind::Version {
                    chrome::menu_background(ui);
                } else {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::WHITE);
                }
                chrome::menu_bar(ui, |ui| {
                    chrome::menu_button(ui, "menu.session", |ui| {
                        session_menu::prefix(ui, icons, true);
                        session_menu::close(ui, icons);
                        ui.separator();
                        if self.kind == crate::history::SessionKind::Registry {
                            if chrome::menu_item(
                                ui,
                                icons,
                                "folder.up_one_level",
                                !self.base_key.is_empty(),
                            )
                            .clicked()
                            {
                                self.up_one_level_all();
                                ui.close_menu();
                            }
                            if chrome::menu_item(
                                ui,
                                icons,
                                "registry.recompare  F5",
                                self.left.is_some() && self.right.is_some(),
                            )
                            .clicked()
                            {
                                self.reload(dialogs);
                                ui.close_menu();
                            }
                        } else if chrome::menu_item(
                            ui,
                            icons,
                            "menu.actions.recompare  Shift+F5",
                            self.left.is_some() && self.right.is_some(),
                        )
                        .clicked()
                        {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                        if self.kind == crate::history::SessionKind::Media
                            && chrome::menu_item(
                                ui,
                                icons,
                                "toolbar.play",
                                self.left.is_some() || self.right.is_some(),
                            )
                            .clicked()
                        {
                            self.context_command(
                                property_context::Command::OpenDefault,
                                ctx,
                                dialogs,
                            );
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, self.report_title_key(), true).clicked() {
                            self.report(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, self.info_title_key(), true).clicked() {
                            self.open_info_modal(ctx);
                            ui.close_menu();
                        }
                        session_menu::suffix(
                            ui,
                            icons,
                            self.left.is_some() && self.right.is_some(),
                        );
                    });
                    chrome::menu_button(ui, "menu.file", |ui| {
                        for (label, left) in [
                            ("menu.file.open_left", true),
                            ("menu.file.open_right", false),
                        ] {
                            if chrome::menu_item(ui, icons, label, true).clicked() {
                                self.pick(left, dialogs);
                                ui.close_menu();
                            }
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.file.reload_files  F5",
                            self.left.is_some() && self.right.is_some(),
                        )
                        .clicked()
                        {
                            self.reload(dialogs);
                            ui.close_menu();
                        }
                        let active_loaded = if self.active_side == 0 {
                            self.left.is_some()
                        } else {
                            self.right.is_some()
                        };
                        ui.add_enabled_ui(active_loaded, |ui| {
                            chrome::submenu_item(ui, icons, "menu.actions.open_with", |ui, icons| {
                                if self.kind == crate::history::SessionKind::Registry
                                    && chrome::menu_item(ui, icons, "session.text_edit", true)
                                        .clicked()
                                {
                                    self.context_command(
                                        property_context::Command::EditText,
                                        ctx,
                                        dialogs,
                                    );
                                    ui.close_menu();
                                }
                                if chrome::menu_item(
                                    ui,
                                    icons,
                                    "table.default_application",
                                    true,
                                )
                                .clicked()
                                {
                                    self.context_command(
                                        property_context::Command::OpenDefault,
                                        ctx,
                                        dialogs,
                                    );
                                    ui.close_menu();
                                }
                            });
                            chrome::submenu_item(ui, icons, "table.explorer", |ui, icons| {
                                if chrome::menu_item(
                                    ui,
                                    icons,
                                    "version.select_in_explorer",
                                    true,
                                )
                                .clicked()
                                {
                                    self.context_command(
                                        property_context::Command::Explore(true),
                                        ctx,
                                        dialogs,
                                    );
                                    ui.close_menu();
                                }
                                if chrome::menu_item(ui, icons, "version.open_folder", true)
                                    .clicked()
                                {
                                    self.context_command(
                                        property_context::Command::Explore(false),
                                        ctx,
                                        dialogs,
                                    );
                                    ui.close_menu();
                                }
                            });
                        });
                        if self.kind == crate::history::SessionKind::Registry {
                            ui.separator();
                            if chrome::menu_item(
                                ui,
                                icons,
                                "registry.export",
                                !self.rows.is_empty(),
                            )
                            .clicked()
                            {
                                self.report(dialogs);
                                ui.close_menu();
                            }
                            if chrome::menu_item(
                                ui,
                                icons,
                                "registry.export_all",
                                !self.rows.is_empty(),
                            )
                            .clicked()
                            {
                                self.report(dialogs);
                                ui.close_menu();
                            }
                        }
                    });
                    chrome::menu_button(ui, "menu.edit", |ui| {
                        let sel_present = self
                            .rows
                            .get(self.selected)
                            .is_some_and(|r| r.names[self.active_side].is_some());
                        let sel_is_group = self
                            .rows
                            .get(self.selected)
                            .is_some_and(|r| r.group);
                        if self.kind == crate::history::SessionKind::Registry {
                            if chrome::menu_item(
                                ui,
                                icons,
                                "common.undo  Ctrl+Z",
                                !self.undo_stack.is_empty(),
                            )
                            .clicked()
                            {
                                self.undo();
                                ui.close_menu();
                            }
                            if chrome::menu_item(
                                ui,
                                icons,
                                "common.redo  Ctrl+Y",
                                !self.redo_stack.is_empty(),
                            )
                            .clicked()
                            {
                                self.redo();
                                ui.close_menu();
                            }
                            ui.separator();
                            let copy_side_label = if self.active_side == 0 {
                                "menu.actions.copy_right_short  Ctrl+R"
                            } else {
                                "menu.actions.copy_left_short  Ctrl+L"
                            };
                            if chrome::menu_item(ui, icons, copy_side_label, sel_present).clicked()
                            {
                                self.copy_to_other_side();
                                ui.close_menu();
                            }
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "version.copy_value  Ctrl+C",
                            !self.rows.is_empty(),
                        )
                        .clicked()
                        {
                            self.copy(ctx);
                            ui.close_menu();
                        }
                        if self.kind == crate::history::SessionKind::Registry {
                            if chrome::menu_item(
                                ui,
                                icons,
                                "registry.copy_key_name",
                                !self.rows.is_empty(),
                            )
                            .clicked()
                            {
                                self.copy_key_name(ctx);
                                ui.close_menu();
                            }
                            ui.separator();
                            if chrome::menu_item(ui, icons, "common.delete  Del", sel_present)
                                .clicked()
                            {
                                self.delete_selected();
                                ui.close_menu();
                            }
                            if chrome::menu_item(ui, icons, "common.rename  F2", sel_present)
                                .clicked()
                            {
                                self.rename_selected(dialogs);
                                ui.close_menu();
                            }
                            if chrome::menu_item(ui, icons, "version.new_key", sel_present)
                                .clicked()
                            {
                                self.new_key_dialog(dialogs);
                                ui.close_menu();
                            }
                            if chrome::menu_item(ui, icons, "version.new_value  Ins", sel_present)
                                .clicked()
                            {
                                self.new_value_dialog(dialogs);
                                ui.close_menu();
                            }
                            if chrome::menu_item(
                                ui,
                                icons,
                                "common.modify_ellipsis",
                                sel_present && !sel_is_group,
                            )
                            .clicked()
                            {
                                self.modify_selected(dialogs);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "version.item_is_important",
                            self.is_selected_important(),
                            !self.rows.is_empty(),
                        )
                        .clicked()
                        {
                            self.toggle_selected_important();
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.edit.select_all  Ctrl+A",
                            !self.rows.is_empty(),
                        )
                        .clicked()
                        {
                            if let Some(&first_vis) = self.visible().first() {
                                self.selected = first_vis;
                            }
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "menu.search", |ui| {
                        for (label, next) in
                            [("toolbar.next_diff", true), ("toolbar.prev_diff", false)]
                        {
                            if chrome::menu_item(ui, icons, label, true).clicked() {
                                self.navigate(next);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.search.find  Ctrl+F",
                            !self.rows.is_empty(),
                        )
                        .clicked()
                        {
                            self.find_dialog(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.search.find_next  F3",
                            !self.find_query.is_empty(),
                        )
                        .clicked()
                        {
                            self.find_step(true);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "menu.search.find_prev  Shift+F3",
                            !self.find_query.is_empty(),
                        )
                        .clicked()
                        {
                            self.find_step(false);
                            ui.close_menu();
                        }
                    });
                    chrome::menu_button(ui, "menu.view", |ui| {
                        for (label, filter) in [
                            ("common.all", Filter::All),
                            ("common.differences", Filter::Differences),
                            ("common.same", Filter::Same),
                        ] {
                            if chrome::menu_radio_item(
                                ui,
                                icons,
                                label,
                                self.options.filter == filter,
                                true,
                            )
                            .clicked()
                            {
                                self.options.filter = filter;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "table.ignore_unimportant",
                            self.options.ignore_minor,
                            true,
                        )
                        .clicked()
                        {
                            self.options.ignore_minor = !self.options.ignore_minor;
                            ui.close_menu();
                        }
                        ui.separator();
                        let has_groups = self.rows.iter().any(|r| r.group);
                        if chrome::menu_item(ui, icons, "toolbar.expand_all", has_groups).clicked() {
                            self.options.closed.clear();
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "toolbar.collapse_all", has_groups).clicked() {
                            self.options.closed = self
                                .rows
                                .iter()
                                .filter(|r| r.group)
                                .map(|r| r.key.clone())
                                .collect();
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_item(
                            ui,
                            icons,
                            "view.increase_font_size  Ctrl++",
                            self.font_size < 24.0,
                        )
                        .clicked()
                        {
                            self.font_size = (self.font_size + 1.0).clamp(9.0, 24.0);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "view.decrease_font_size  Ctrl+-",
                            self.font_size > 9.0,
                        )
                        .clicked()
                        {
                            self.font_size = (self.font_size - 1.0).clamp(9.0, 24.0);
                            ui.close_menu();
                        }
                        if chrome::menu_item(
                            ui,
                            icons,
                            "view.reset_font_size  Ctrl+0",
                            (self.font_size - 12.0).abs() > 0.1,
                        )
                        .clicked()
                        {
                            self.font_size = 12.0;
                            ui.close_menu();
                        }
                        ui.separator();
                        if chrome::menu_radio_item(
                            ui,
                            icons,
                            "view.side_by_side_layout",
                            !self.over_under,
                            true,
                        )
                        .clicked()
                        {
                            self.over_under = false;
                            ui.close_menu();
                        }
                        if chrome::menu_radio_item(
                            ui,
                            icons,
                            "view.over_under_layout",
                            self.over_under,
                            true,
                        )
                        .clicked()
                        {
                            self.over_under = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if self.kind == crate::history::SessionKind::Registry
                            && chrome::menu_check_item(
                                ui,
                                icons,
                                "text.visible_whitespace",
                                self.visible_whitespace,
                                true,
                            )
                            .clicked()
                        {
                            self.visible_whitespace = !self.visible_whitespace;
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "text.show_details",
                            self.show_text_details,
                            true,
                        )
                        .clicked()
                        {
                            self.show_text_details = !self.show_text_details;
                            ui.close_menu();
                        }
                        if self.kind == crate::history::SessionKind::Registry
                            && chrome::menu_check_item(
                                ui,
                                icons,
                                "text.hex_details",
                                self.show_hex_details,
                                true,
                            )
                            .clicked()
                        {
                            self.show_hex_details = !self.show_hex_details;
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
                            ui,
                            icons,
                            "view.file_info",
                            self.show_file_info,
                            true,
                        )
                        .clicked()
                        {
                            self.show_file_info = !self.show_file_info;
                            ui.close_menu();
                        }
                        if chrome::menu_check_item(
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
                    chrome::menu_button(ui, "menu.tools", |ui| {
                        if chrome::menu_item(ui, icons, "menu.session.rules_ellipsis", true).clicked()
                        {
                            self.rules(dialogs);
                            ui.close_menu();
                        }
                        if chrome::menu_item(ui, icons, "menu.tools.options", true).clicked() {
                            if dialogs.persist_settings() {
                                super::options_dialog::request(
                                    ui.ctx(),
                                    super::options_dialog::OptionsCategory::General,
                                );
                            } else {
                                crate::i18n::configure(dialogs);
                            }
                            ui.close_menu();
                        }
                        session_menu::settings_tools(ui, icons, dialogs);
                    });
                    chrome::menu_button(ui, "menu.help", |ui| {
                        if chrome::menu_item(ui, icons, "menu.help.about", true).clicked() {
                            super::info_modals::open_about(ui.ctx());
                            ui.close_menu();
                        }
                    });
                });
            });
        session_tabs::show(ui, icons);
        let has_groups = self.rows.iter().any(|r| r.group);
        let can_copy = self
            .rows
            .get(self.selected)
            .and_then(|r| r.values[self.active_side].as_ref())
            .is_some();
        let state = |name: &str| match name {
            "FavCompareAll" => ToolState::checked(self.options.filter == Filter::All),
            "FavDiff" => ToolState::checked(self.options.filter == Filter::Differences),
            "FavSame" => ToolState::checked(self.options.filter == Filter::Same),
            "InsigIgnored" => ToolState::checked(self.options.ignore_minor),
            "SessionHome" | "SessionsCombo" | "Rules" | "Swap" => ToolState::enabled(true),
            "CopySide" => ToolState::enabled(can_copy),
            "ExpandAll" | "CollapseAll" => ToolState::enabled(has_groups),
            "Reload" => ToolState::enabled(self.left.is_some() && self.right.is_some()),
            "Play" | "Play2" => ToolState::enabled(self.left.is_some() || self.right.is_some()),
            "DiffItemNext" | "DiffItemPrev" => {
                ToolState::enabled(self.rows.iter().any(|r| r.change != Change::Same))
            }
            _ => ToolState::DISABLED,
        };
        let mut event = None;
        if self.show_toolbar {
            let height = toolbar::height(ui, self.tools(), ui.available_width(), &state);
            TopBottomPanel::top("version_toolbar")
                .exact_height(height)
                .show_separator_line(false)
                .frame(Frame::none())
                .show_inside(ui, |ui| {
                    event =
                        toolbar::show(ui, icons, self.tools(), true, &state, &mut |_, _, _| {});
                });
        }
        if let Some(ToolEvent::Click(command)) = event {
            match command {
                "SessionHome" => home = true,
                "FavCompareAll" => self.options.filter = Filter::All,
                "FavDiff" => self.options.filter = Filter::Differences,
                "FavSame" => self.options.filter = Filter::Same,
                "InsigIgnored" => self.options.ignore_minor = !self.options.ignore_minor,
                "Rules" => self.rules(dialogs),
                "CopySide" => {
                    if self.kind == crate::history::SessionKind::Registry {
                        self.copy_to_other_side();
                    } else {
                        self.copy(ctx);
                    }
                }
                "Play" | "Play2" => {
                    self.context_command(property_context::Command::OpenDefault, ctx, dialogs);
                }
                "ExpandAll" => self.options.closed.clear(),
                "CollapseAll" => {
                    self.options.closed = self
                        .rows
                        .iter()
                        .filter(|r| r.group)
                        .map(|r| r.key.clone())
                        .collect();
                }
                "DiffItemNext" => self.navigate(true),
                "DiffItemPrev" => self.navigate(false),
                "Reload" => self.reload(dialogs),
                "Swap" => {
                    std::mem::swap(&mut self.left, &mut self.right);
                    std::mem::swap(&mut self.left_path, &mut self.right_path);
                    self.compare();
                }
                _ => {}
            }
        }
        if !ctx.wants_keyboard_input() && !super::session_picker::is_open(ctx) {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
                self.reload(dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::C)) {
                self.copy(ctx);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O)) {
                self.pick(self.active_side == 0, dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F7)) {
                self.navigate(true);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
                self.find_dialog(dialogs);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F3)) {
                self.find_step(true);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F3)) {
                self.find_step(false);
            }
            if self.kind == crate::history::SessionKind::Registry {
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
                    self.undo();
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y)) {
                    self.redo();
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::R)) {
                    self.active_side = 0;
                    self.copy_to_other_side();
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
                    self.active_side = 1;
                    self.copy_to_other_side();
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
                    self.delete_selected();
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
                    self.rename_selected(dialogs);
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Insert)) {
                    self.new_value_dialog(dialogs);
                }
            }
        }
        let is_registry = self.kind == crate::history::SessionKind::Registry;
        let center_gap = if is_registry { 22.0 } else { 6.0 };
        let mut go_up_one_level = false;
        ui.horizontal(|ui| {
            let width = ((ui.available_width() - center_gap) / 2.0).max(80.0);
            for left in [true, false] {
                ui.allocate_ui_with_layout(
                    vec2(width, 23.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(width);
                        ui.set_max_width(width);
                        ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let raw_path = if left {
                            self.left_path.clone()
                        } else {
                            self.right_path.clone()
                        };
                        let trail_buttons_w = if is_registry { 70.0 } else { 50.0 };
                        let combo_w = (width - trail_buttons_w).max(30.0);
                        let full_for_display = if is_registry
                            && !self.base_key_display.is_empty()
                            && !raw_path.is_empty()
                        {
                            format!("{raw_path}\\{}", self.base_key_display)
                        } else {
                            raw_path.clone()
                        };
                        let mut display_path = if !full_for_display.is_empty() {
                            elide_registry_path(
                                ui,
                                &full_for_display,
                                (combo_w - 26.0).max(40.0),
                            )
                        } else {
                            full_for_display
                        };
                        let before_display = display_path.clone();
                        let response = history_combo::show(
                            ui,
                            ("version_path", left),
                            &mut display_path,
                            &[],
                            combo_w,
                            "common.enter_filename_here",
                        );
                        if let Some(selected) = response.selected {
                            self.open_side(left, Path::new(&selected), dialogs);
                        } else if response.submitted {
                            let target = if display_path != before_display {
                                display_path
                            } else {
                                raw_path.clone()
                            };
                            self.open_side(left, Path::new(&target), dialogs);
                        }
                        let can_up = is_registry && !self.base_key.is_empty();
                        let (up_clicked, browse_act) = chrome::file_browse_buttons(
                            ui,
                            icons,
                            ("ver_browse", left),
                            false,
                            can_up,
                            is_registry,
                            false,
                            !raw_path.is_empty(),
                        );
                        if up_clicked {
                            go_up_one_level = true;
                        }
                        match browse_act {
                            Some(
                                chrome::BrowseMenuAction::OpenFileSystem
                                | chrome::BrowseMenuAction::OpenProfile,
                            ) => {
                                self.pick(left, dialogs);
                            }
                            Some(chrome::BrowseMenuAction::OpenWithTextEdit) => {
                                if !raw_path.is_empty() {
                                    session_menu::request(
                                        ctx,
                                        session_menu::Request::EditText(PathBuf::from(&raw_path)),
                                    );
                                }
                            }
                            Some(chrome::BrowseMenuAction::OpenWithAssociated) => {
                                if !raw_path.is_empty() {
                                    if let Err(err) = dialogs.open_path(Path::new(&raw_path), false)
                                    {
                                        dialogs.error(&err);
                                    }
                                }
                            }
                            Some(
                                chrome::BrowseMenuAction::LoadTyped
                                | chrome::BrowseMenuAction::OpenClipboard,
                            )
                            | None => {}
                        }
                    },
                );
                if left {
                    if is_registry {
                        ui.allocate_ui_with_layout(
                            vec2(center_gap, 23.0),
                            Layout::centered_and_justified(Direction::LeftToRight),
                            |ui| {
                                let can_up = !self.base_key.is_empty();
                                if let Some(icon) =
                                    icons.get_icon_with_state(ui.ctx(), "imgUpOneLevelAll", can_up)
                                {
                                    if ui
                                        .add_enabled(
                                            can_up,
                                            ImageButton::new((icon.id(), Vec2::splat(16.0)))
                                                .frame(false),
                                        )
                                        .on_hover_text(tr("folder.up_one_level_on_both_sides"))
                                        .clicked()
                                    {
                                        go_up_one_level = true;
                                    }
                                }
                            },
                        );
                    } else {
                        ui.add_space(center_gap);
                    }
                }
            }
        });
        if go_up_one_level {
            self.up_one_level_all();
        }
        let pane_area_w = (ui.available_width() - 6.0).max(80.0);
        let width = pane_area_w / 2.0;
        let type_col_w = if is_registry {
            (width * 0.22).clamp(68.0, 120.0).floor()
        } else {
            0.0
        };
        let name_col_w = if is_registry {
            (width * 0.46)
                .clamp(124.0, (width - type_col_w - 66.0).max(92.0))
                .floor()
        } else {
            (width * 0.36).clamp(180.0, 280.0).floor()
        };
        if self.show_file_info {
            ui.horizontal(|ui| {
                for (side_idx, image) in [&self.left, &self.right].into_iter().enumerate() {
                    let (rect, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 0.0, Color32::from_rgb(245, 245, 245));
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(1.0_f32, Color32::from_gray(208)),
                    );
                    let meta = image.as_ref().map_or(String::new(), Image::metadata_label);
                    ui.painter()
                        .with_clip_rect(rect.shrink2(vec2(4.0, 0.0)))
                        .text(
                            rect.left_center() + vec2(6.0, 0.0),
                            Align2::LEFT_CENTER,
                            meta,
                            FontId::proportional(11.5),
                            Color32::BLACK,
                        );
                    if side_idx == 0 {
                        ui.add_space(6.0);
                    }
                }
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(Color32::RED, error);
        }
        let visible = self.visible();
        let type_header_label = tr("table.type").trim_end_matches(':').to_owned();
        let data_header_label = tr("version.data");
        ui.horizontal(|ui| {
            let hdr_border = Stroke::new(1.0_f32, Color32::from_gray(208));
            for side_idx in 0..2 {
                let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_rgb(246, 247, 249));
                ui.painter().hline(rect.x_range(), rect.top(), hdr_border);
                ui.painter()
                    .hline(rect.x_range(), rect.bottom(), hdr_border);
                ui.painter()
                    .vline(rect.left() + name_col_w, rect.y_range(), hdr_border);
                paint(
                    ui,
                    Rect::from_min_size(rect.min + vec2(4.0, 0.0), vec2(name_col_w - 6.0, 22.0)),
                    &tr("common.name"),
                    Color32::BLACK,
                );
                if is_registry {
                    ui.painter().vline(
                        rect.left() + name_col_w + type_col_w,
                        rect.y_range(),
                        hdr_border,
                    );
                    paint(
                        ui,
                        Rect::from_min_size(
                            rect.min + vec2(name_col_w + 4.0, 0.0),
                            vec2(type_col_w - 6.0, 22.0),
                        ),
                        &type_header_label,
                        Color32::BLACK,
                    );
                    paint(
                        ui,
                        Rect::from_min_max(
                            rect.min + vec2(name_col_w + type_col_w + 4.0, 0.0),
                            rect.max,
                        ),
                        &data_header_label,
                        Color32::BLACK,
                    );
                } else {
                    paint(
                        ui,
                        Rect::from_min_max(rect.min + vec2(name_col_w + 4.0, 0.0), rect.max),
                        &tr("common.value"),
                        Color32::BLACK,
                    );
                }
                if side_idx == 0 {
                    ui.add_space(6.0);
                }
            }
        });
        let is_media = self.kind == crate::history::SessionKind::Media;
        let media_bar_h = if is_media { 24.0 } else { 0.0 };
        let details_h = if self.show_text_details || self.show_hex_details {
            38.0
        } else {
            0.0
        };
        let status_h = 21.0;
        let height = (ui.available_height() - media_bar_h - details_h - status_h).max(40.0);
        let row_area_origin = ui.cursor().min;
        let background = Rect::from_min_size(row_area_origin, vec2(width * 2.0 + 6.0, height));
        ui.painter().rect_filled(background, 0.0, Color32::WHITE);
        let splitter_rect = Rect::from_min_size(
            background.min + vec2(width, 0.0),
            vec2(6.0, height),
        );
        ui.painter()
            .rect_filled(splitter_rect, 0.0, BcTheme::WINDOW_BG);
        ui.painter().vline(
            splitter_rect.left(),
            splitter_rect.y_range(),
            Stroke::new(1.0_f32, Color32::from_gray(195)),
        );
        ui.painter().vline(
            splitter_rect.right(),
            splitter_rect.y_range(),
            Stroke::new(1.0_f32, Color32::from_gray(195)),
        );

        let mut toggle = None;
        let mut enter_subkey = None;
        let mut modify_on_dblclick = false;
        let mut context_command = None;
        let first_leaf = visible
            .iter()
            .copied()
            .find(|&idx| !self.rows[idx].group)
            .or_else(|| self.rows.iter().position(|row| !row.group));
        let base_depth = if is_registry && !self.base_key.is_empty() {
            self.base_key.split('/').count()
        } else {
            0
        };
        let row_h = 18.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ScrollArea::both()
                    .id_salt("version_tree")
                    .auto_shrink([false, false])
                    .max_height(height)
                    .show_rows(ui, row_h, visible.len(), |ui, range| {
                        for visual in range {
                            let index = visible[visual];
                            let row = &self.rows[index];
                            let (rect, response) = ui.allocate_exact_size(
                                vec2(width * 2.0 + 6.0, row_h),
                                Sense::hover(),
                            );
                            if index == self.selected && self.jump {
                                response.scroll_to_me(Some(Align::Center));
                            }
                            let display_depth = row.depth.saturating_sub(base_depth);
                            let indent = display_depth as f32 * 16.0;
                            let both_present = row.names[0].is_some() && row.names[1].is_some();
                            let (l_type, _) =
                                split_registry_value(row.values[0].as_deref().unwrap_or(""));
                            let (r_type, _) =
                                split_registry_value(row.values[1].as_deref().unwrap_or(""));
                            for side in 0..2 {
                                let pane = Rect::from_min_size(
                                    rect.min + vec2(side as f32 * (width + 6.0), 0.0),
                                    vec2(width, row_h),
                                );
                                let cell = ui.interact(
                                    pane,
                                    Id::new("property_row").with((index, side)),
                                    Sense::click(),
                                );
                                if cell.clicked() || cell.secondary_clicked() {
                                    self.selected = index;
                                    self.active_side = side;
                                    if cell.clicked() && row.group {
                                        toggle = Some(row.key.clone());
                                    }
                                }
                                if cell.double_clicked() && is_registry {
                                    if row.group {
                                        enter_subkey = Some(index);
                                    } else if row.names[side].is_some() {
                                        self.selected = index;
                                        self.active_side = side;
                                        modify_on_dblclick = true;
                                    }
                                }
                                #[cfg(test)]
                                ui.data_mut(|d| {
                                    d.insert_temp(
                                        Id::new("property_test_row").with((index, side)),
                                        pane,
                                    )
                                });
                                if first_leaf == Some(index) {
                                    let label = if side == 0 {
                                        "Menu de propriedades"
                                    } else {
                                        "Menu de propriedades direito"
                                    };
                                    ui.data_mut(|d| {
                                        d.insert_temp(
                                            Id::new("capture_menu_rect").with(label),
                                            pane,
                                        )
                                    });
                                }
                                cell.context_menu(|ui| {
                                    context_command = property_context::menu(
                                        ui,
                                        icons,
                                        row,
                                        property_context::Target {
                                            kind: self.kind,
                                            side,
                                            important: !self.options.ignored.iter().any(
                                                |ignored| {
                                                    row.key == *ignored
                                                        || row
                                                            .key
                                                            .starts_with(&format!("{ignored}/"))
                                                },
                                            ),
                                            loaded: if side == 0 {
                                                self.left.is_some()
                                            } else {
                                                self.right.is_some()
                                            },
                                        },
                                    )
                                    .map(|command| (index, side, command));
                                });
                                let is_important_diff = row.change == Change::Important;
                                let is_minor_diff =
                                    row.change == Change::Minor && !self.options.ignore_minor;
                                let bg = if row.group && both_present {
                                    Color32::WHITE
                                } else if index == self.selected && side == self.active_side {
                                    Color32::from_rgb(194, 224, 255)
                                } else if index == self.selected {
                                    if is_important_diff {
                                        Color32::from_rgb(252, 230, 230)
                                    } else {
                                        Color32::from_rgb(225, 238, 250)
                                    }
                                } else if is_important_diff {
                                    Color32::from_rgb(253, 232, 232)
                                } else if is_minor_diff {
                                    Color32::from_rgb(240, 244, 252)
                                } else {
                                    Color32::WHITE
                                };
                                ui.painter().rect_filled(pane, 0.0, bg);
                                if index == self.selected {
                                    let border_col = if row.group && both_present {
                                        Color32::from_gray(175)
                                    } else if side == self.active_side {
                                        Color32::from_rgb(132, 172, 221)
                                    } else if is_important_diff {
                                        Color32::from_rgb(224, 198, 198)
                                    } else {
                                        Color32::from_rgb(185, 208, 232)
                                    };
                                    ui.painter().rect_stroke(
                                        pane,
                                        0.0,
                                        Stroke::new(1.0_f32, border_col),
                                    );
                                }
                                let color = match row.change {
                                    Change::Same => Color32::BLACK,
                                    Change::Minor if self.options.ignore_minor => Color32::BLACK,
                                    Change::Minor => BcTheme::MINOR_TEXT,
                                    Change::Important => BcTheme::DIFF_TEXT,
                                };
                                let name = row.names[side]
                                    .as_ref()
                                    .map(|name| property_name(self.kind, &row.key, name))
                                    .unwrap_or_default();
                                if is_registry {
                                    if !name.is_empty() {
                                        let cy = pane.center().y.floor();
                                        if row.group {
                                            let collapsed = self.options.closed.contains(&row.key);
                                            let box_rect = Rect::from_min_size(
                                                pos2(pane.left() + indent + 4.0, cy - 4.0),
                                                vec2(9.0, 9.0),
                                            );
                                            ui.painter()
                                                .rect_filled(box_rect, 0.0, Color32::WHITE);
                                            ui.painter().rect_stroke(
                                                box_rect,
                                                0.0,
                                                Stroke::new(1.0_f32, Color32::from_gray(140)),
                                            );
                                            ui.painter().hline(
                                                (box_rect.left() + 2.0)..=(box_rect.right() - 2.0),
                                                box_rect.center().y,
                                                Stroke::new(1.0_f32, Color32::from_gray(45)),
                                            );
                                            if collapsed {
                                                ui.painter().vline(
                                                    box_rect.center().x,
                                                    (box_rect.top() + 2.0)
                                                        ..=(box_rect.bottom() - 2.0),
                                                    Stroke::new(1.0_f32, Color32::from_gray(45)),
                                                );
                                            }
                                            let folder_icon = if collapsed {
                                                "imgFolderClosed"
                                            } else {
                                                "imgFolderOpened"
                                            };
                                            let folder_tint = if !both_present {
                                                Color32::from_rgb(138, 12, 238)
                                            } else if is_important_diff {
                                                Color32::from_rgb(230, 50, 50)
                                            } else {
                                                Color32::from_rgb(238, 189, 75)
                                            };
                                            if let Some(tex) = icons.folder_model(
                                                ui.ctx(),
                                                folder_icon,
                                                folder_tint,
                                            ) {
                                                ui.painter().image(
                                                    tex.id(),
                                                    Rect::from_min_size(
                                                        pos2(
                                                            pane.left() + indent + 15.0,
                                                            pane.top() + 1.0,
                                                        ),
                                                        vec2(16.0, 16.0),
                                                    ),
                                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                    Color32::WHITE,
                                                );
                                            }
                                        } else {
                                            ui.painter().hline(
                                                (pane.left() + indent + 4.0)
                                                    ..=(pane.left() + indent + 9.0),
                                                cy,
                                                Stroke::new(1.0_f32, Color32::from_gray(155)),
                                            );
                                            let (reg_type, _) = split_registry_value(
                                                row.values[side].as_deref().unwrap_or(""),
                                            );
                                            let icon_name = if reg_type.starts_with("REG_DWORD")
                                                || reg_type.starts_with("REG_QWORD")
                                                || reg_type.starts_with("REG_BINARY")
                                                || reg_type.starts_with("REG_NONE")
                                                || reg_type.starts_with("REG_RESOURCE")
                                            {
                                                "imgRegUInt"
                                            } else {
                                                "imgRegStr"
                                            };
                                            if let Some(tex) = icons.get_icon(ui.ctx(), icon_name) {
                                                ui.painter().image(
                                                    tex.id(),
                                                    Rect::from_min_size(
                                                        pos2(
                                                            pane.left() + indent + 11.0,
                                                            pane.top() + 1.0,
                                                        ),
                                                        vec2(16.0, 16.0),
                                                    ),
                                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                    Color32::WHITE,
                                                );
                                            }
                                        }
                                    }
                                    let text_x = indent + if row.group { 33.0 } else { 28.0 };
                                    let name_color = if both_present && !row.group {
                                        Color32::BLACK
                                    } else {
                                        color
                                    };
                                    paint(
                                        ui,
                                        Rect::from_min_max(
                                            pane.min + vec2(text_x, 0.0),
                                            pane.min + vec2(name_col_w - 2.0, row_h),
                                        ),
                                        &name,
                                        name_color,
                                    );
                                    if !row.group {
                                        let (reg_type, reg_data) = split_registry_value(
                                            row.values[side].as_deref().unwrap_or(""),
                                        );
                                        let type_color = if both_present && l_type == r_type {
                                            Color32::BLACK
                                        } else {
                                            color
                                        };
                                        paint(
                                            ui,
                                            Rect::from_min_max(
                                                pane.min + vec2(name_col_w + 4.0, 0.0),
                                                pane.min
                                                    + vec2(name_col_w + type_col_w - 2.0, row_h),
                                            ),
                                            reg_type,
                                            type_color,
                                        );
                                        paint(
                                            ui,
                                            Rect::from_min_max(
                                                pane.min
                                                    + vec2(name_col_w + type_col_w + 4.0, 0.0),
                                                pane.max,
                                            ),
                                            reg_data,
                                            color,
                                        );
                                    }
                                } else {
                                    let name_color = if both_present {
                                        Color32::BLACK
                                    } else {
                                        color
                                    };
                                    if row.group && !name.is_empty() {
                                        let collapsed = self.options.closed.contains(&row.key);
                                        let cx = (pane.left() + indent + 8.0).floor() + 0.5;
                                        let cy = pane.center().y.floor() + 0.5;
                                        let stroke = Stroke::new(
                                            1.25_f32,
                                            if name_color == Color32::BLACK {
                                                if collapsed {
                                                    Color32::from_gray(115)
                                                } else {
                                                    Color32::from_gray(45)
                                                }
                                            } else {
                                                name_color
                                            },
                                        );
                                        let pts = if collapsed {
                                            [
                                                pos2(cx - 2.0, cy - 3.5),
                                                pos2(cx + 1.5, cy),
                                                pos2(cx - 2.0, cy + 3.5),
                                            ]
                                        } else {
                                            [
                                                pos2(cx - 3.5, cy - 1.5),
                                                pos2(cx, cy + 2.0),
                                                pos2(cx + 3.5, cy - 1.5),
                                            ]
                                        };
                                        ui.painter().line_segment([pts[0], pts[1]], stroke);
                                        ui.painter().line_segment([pts[1], pts[2]], stroke);
                                    } else if !row.group && display_depth > 0 && !name.is_empty() {
                                        let lx = (pane.left() + indent - 8.0).floor() + 0.5;
                                        let cy = pane.center().y.floor() + 0.5;
                                        let tree_stroke =
                                            Stroke::new(1.0_f32, Color32::from_gray(180));
                                        ui.painter().line_segment(
                                            [pos2(lx, pane.top()), pos2(lx, cy)],
                                            tree_stroke,
                                        );
                                        ui.painter().line_segment(
                                            [pos2(lx, cy), pos2(lx + 7.0, cy)],
                                            tree_stroke,
                                        );
                                    }
                                    let text_x = indent + if row.group { 16.0 } else { 4.0 };
                                    paint(
                                        ui,
                                        Rect::from_min_max(
                                            pane.min + vec2(text_x, 0.0),
                                            pane.min + vec2(name_col_w - 2.0, row_h),
                                        ),
                                        &name,
                                        name_color,
                                    );
                                    paint(
                                        ui,
                                        Rect::from_min_max(
                                            pane.min + vec2(name_col_w + 4.0, 0.0),
                                            pane.max,
                                        ),
                                        &property_value(
                                            self.kind,
                                            &row.key,
                                            row.values[side].as_deref().unwrap_or(""),
                                        ),
                                        color,
                                    );
                                }
                                if index != self.selected {
                                    ui.painter().line_segment(
                                        [pane.left_bottom(), pane.right_bottom()],
                                        Stroke::new(1.0_f32, Color32::from_gray(238)),
                                    );
                                }
                            }
                        }
                    });
            });
        });
        self.jump = false;
        if let Some((index, side, command)) = context_command {
            self.selected = index;
            self.active_side = side;
            self.context_command(command, ctx, dialogs);
        }
        if modify_on_dblclick {
            self.modify_selected(dialogs);
        } else if let Some(index) = enter_subkey {
            self.enter_subkey(index);
        } else if let Some(key) = toggle {
            if !self.options.closed.remove(&key) {
                self.options.closed.insert(key);
            }
        }
        if is_media {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for side in 0..2 {
                    let (bar_rect, _) =
                        ui.allocate_exact_size(vec2(width, media_bar_h), Sense::hover());
                    ui.painter()
                        .rect_filled(bar_rect, 0.0, Color32::from_rgb(245, 245, 245));
                    ui.painter().hline(
                        bar_rect.x_range(),
                        bar_rect.top(),
                        Stroke::new(1.0_f32, Color32::from_gray(208)),
                    );
                    let has_media = if side == 0 {
                        self.left.is_some()
                    } else {
                        self.right.is_some()
                    };
                    let play_color = if has_media {
                        Color32::from_rgb(18, 145, 24)
                    } else {
                        Color32::from_gray(170)
                    };
                    let pc = pos2(bar_rect.left() + 11.0, bar_rect.center().y);
                    ui.painter().add(Shape::convex_polygon(
                        vec![
                            pc + vec2(-3.5, -4.5),
                            pc + vec2(4.5, 0.0),
                            pc + vec2(-3.5, 4.5),
                        ],
                        play_color,
                        Stroke::NONE,
                    ));
                    let track_rect = Rect::from_min_max(
                        pos2(bar_rect.left() + 26.0, bar_rect.center().y - 5.0),
                        pos2(bar_rect.right() - 6.0, bar_rect.center().y + 5.0),
                    );
                    ui.painter()
                        .rect_filled(track_rect, 0.0, Color32::from_rgb(233, 233, 233));
                    ui.painter().rect_stroke(
                        track_rect,
                        0.0,
                        Stroke::new(1.0_f32, Color32::from_gray(205)),
                    );
                    let thumb_rect = Rect::from_min_size(
                        pos2(track_rect.left(), bar_rect.center().y - 7.0),
                        vec2(8.0, 14.0),
                    );
                    ui.painter()
                        .rect_filled(thumb_rect, 1.0, Color32::from_gray(198));
                    ui.painter().rect_stroke(
                        thumb_rect,
                        1.0,
                        Stroke::new(1.0_f32, Color32::from_gray(172)),
                    );
                    if side == 0 {
                        let (sep, _) =
                            ui.allocate_exact_size(vec2(6.0, media_bar_h), Sense::hover());
                        ui.painter().rect_filled(sep, 0.0, BcTheme::WINDOW_BG);
                        ui.painter().vline(
                            sep.left(),
                            sep.y_range(),
                            Stroke::new(1.0_f32, Color32::from_gray(195)),
                        );
                        ui.painter().vline(
                            sep.right(),
                            sep.y_range(),
                            Stroke::new(1.0_f32, Color32::from_gray(195)),
                        );
                    }
                }
            });
        }
        // Bottom 2-Row Value Comparison DetailsPanel (TRegView/TMediaView/TVerView.DetailsPanel)
        if details_h > 0.0 {
            let (details_rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), details_h), Sense::hover());
            ui.painter().rect_filled(details_rect, 0.0, Color32::WHITE);
            ui.painter().rect_stroke(
                details_rect,
                0.0,
                Stroke::new(1.0_f32, Color32::from_gray(180)),
            );
            if let Some(sel_row) = self.rows.get(self.selected) {
                let sel_important = !sel_row.group && sel_row.change == Change::Important;
                let sel_minor = !sel_row.group
                    && sel_row.change == Change::Minor
                    && !self.options.ignore_minor;
                let detail_bg = if sel_important {
                    Color32::from_rgb(253, 230, 230)
                } else if sel_minor {
                    Color32::from_rgb(240, 244, 252)
                } else {
                    Color32::WHITE
                };
                let detail_fg = if sel_important {
                    BcTheme::DIFF_TEXT
                } else if sel_minor {
                    BcTheme::MINOR_TEXT
                } else {
                    Color32::BLACK
                };
                let inner = details_rect.shrink(1.0);
                let half_h = inner.height() / 2.0;
                if !sel_row.group {
                    for side in 0..2 {
                        let band = Rect::from_min_size(
                            pos2(inner.left(), inner.top() + side as f32 * half_h),
                            vec2(inner.width(), half_h),
                        );
                        let raw_val = if is_registry {
                            let (_, data) = split_registry_value(
                                sel_row.values[side].as_deref().unwrap_or(""),
                            );
                            data.to_owned()
                        } else {
                            property_value(
                                self.kind,
                                &sel_row.key,
                                sel_row.values[side].as_deref().unwrap_or(""),
                            )
                        };
                        let val_str = if self.show_hex_details && !raw_val.is_empty() {
                            raw_val
                                .as_bytes()
                                .iter()
                                .take(64)
                                .map(|b| format!("{b:02X}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        } else if self.visible_whitespace {
                            raw_val.replace(' ', "·").replace('\t', "→")
                        } else {
                            raw_val
                        };
                        if !val_str.is_empty() || sel_important || sel_minor {
                            ui.painter().rect_filled(band, 0.0, detail_bg);
                        }
                        paint(ui, band, &val_str, detail_fg);
                    }
                }
                ui.painter().hline(
                    inner.x_range(),
                    inner.top() + half_h,
                    Stroke::new(1.0_f32, Color32::from_gray(208)),
                );
            }
        }

        // Bottom 3-Section BC5 StatusBar (TRegView.StatusBar: 225 | 300 | remaining)
        let changed = self
            .rows
            .iter()
            .filter(|r| {
                !r.group
                    && if self.options.ignore_minor {
                        r.change == Change::Important
                    } else {
                        r.change != Change::Same
                    }
            })
            .count();
        let has_important = self
            .rows
            .iter()
            .any(|r| !r.group && r.change == Change::Important);
        let has_minor = self
            .rows
            .iter()
            .any(|r| !r.group && r.change == Change::Minor);
        let same_binary = self
            .left
            .as_ref()
            .zip(self.right.as_ref())
            .is_some_and(|(a, b)| a.sha256 == b.sha256);
        let (status_rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), status_h), Sense::hover());
        ui.painter()
            .rect_filled(status_rect, 0.0, BcTheme::WINDOW_BG);
        ui.painter().hline(
            status_rect.x_range(),
            status_rect.top() + 0.5,
            Stroke::new(1.0_f32, Color32::from_gray(208)),
        );
        let status_icon = if same_binary {
            "imgContentSameBinary"
        } else if changed > 0 {
            "status_diff"
        } else {
            "status_same"
        };
        if let Some(tex) = icons.get_icon(ui.ctx(), status_icon) {
            ui.painter().image(
                tex.id(),
                Rect::from_center_size(
                    status_rect.left_center() + vec2(10.0, 0.0),
                    vec2(14.0, 14.0),
                ),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let status_text = if same_binary {
            tr("common.binary_same")
        } else if changed == 0 {
            tr("common.same")
        } else if has_important {
            tr("table.important_differences")
        } else if has_minor {
            tr("table.unimportant_differences")
        } else {
            tr("common.differences")
        };
        let col1_x = (status_rect.left() + 225.0).min(status_rect.right());
        ui.painter()
            .with_clip_rect(Rect::from_min_max(status_rect.min, pos2(col1_x - 2.0, status_rect.bottom())))
            .text(
                status_rect.left_center() + vec2(22.0, 0.0),
                Align2::LEFT_CENTER,
                status_text,
                FontId::proportional(11.5),
                Color32::BLACK,
            );
        let sel_status = self.rows.get(self.selected).map_or(String::new(), |r| {
            if r.group {
                String::new()
            } else {
                match r.change {
                    Change::Important => tr("table.important_difference"),
                    Change::Minor if !self.options.ignore_minor => {
                        tr("table.unimportant_difference")
                    }
                    _ => tr("common.same"),
                }
            }
        });
        let col2_x = (col1_x + 300.0).min(status_rect.right());
        if col1_x < status_rect.right() - 20.0 {
            ui.painter().vline(
                col1_x,
                (status_rect.top() + 2.0)..=(status_rect.bottom() - 2.0),
                Stroke::new(1.0_f32, Color32::from_gray(208)),
            );
            ui.painter()
                .with_clip_rect(Rect::from_min_max(
                    pos2(col1_x + 4.0, status_rect.top()),
                    pos2(col2_x - 2.0, status_rect.bottom()),
                ))
                .text(
                    pos2(col1_x + 6.0, status_rect.center().y),
                    Align2::LEFT_CENTER,
                    sel_status,
                    FontId::proportional(11.5),
                    Color32::BLACK,
                );
        }
        if col2_x < status_rect.right() - 20.0 {
            ui.painter().vline(
                col2_x,
                (status_rect.top() + 2.0)..=(status_rect.bottom() - 2.0),
                Stroke::new(1.0_f32, Color32::from_gray(208)),
            );
            let secs_str = if crate::i18n::language().starts_with("en") {
                format!("{:.2}", self.load_seconds)
            } else {
                format!("{:.2}", self.load_seconds).replace('.', ",")
            };
            let load_label = format!(
                "{} {} {}",
                tr("table.load_time"),
                secs_str,
                tr("common.seconds")
            );
            ui.painter()
                .with_clip_rect(Rect::from_min_max(
                    pos2(col2_x + 4.0, status_rect.top()),
                    pos2(status_rect.right() - 14.0, status_rect.bottom()),
                ))
                .text(
                    pos2(col2_x + 6.0, status_rect.center().y),
                    Align2::LEFT_CENTER,
                    load_label,
                    FontId::proportional(11.5),
                    Color32::BLACK,
                );
        }
        chrome::status_bar_grip(ui.painter(), status_rect);
        home
    }
    fn reload(&mut self, dialogs: &mut dyn DialogHost) {
        let a = PathBuf::from(&self.left_path);
        let b = PathBuf::from(&self.right_path);
        if let Err(error) = self.load_files(&a, &b) {
            dialogs.error(&error);
        }
    }
    fn copy(&self, ctx: &Context) {
        if let Some(row) = self.rows.get(self.selected) {
            if let Some(value) = &row.values[self.active_side] {
                ctx.copy_text(property_value(self.kind, &row.key, value));
            }
        }
    }
}
fn split_registry_value(val: &str) -> (&str, &str) {
    val.split_once(": ").unwrap_or(("", val))
}

fn format_registry_edit_value(reg_type: &str, raw_data: &str) -> String {
    if reg_type == "REG_DWORD" {
        if let Some(hex) = raw_data
            .strip_prefix("0x")
            .or_else(|| raw_data.strip_prefix("0X"))
        {
            let hex_part = hex.split_whitespace().next().unwrap_or(hex);
            if let Ok(n) = u32::from_str_radix(hex_part, 16) {
                return format!("REG_DWORD: 0x{n:08x} ({n})");
            }
        }
        if let Ok(n) = raw_data.parse::<u32>() {
            return format!("REG_DWORD: 0x{n:08x} ({n})");
        }
    }
    format!("{reg_type}: {raw_data}")
}

fn find_node_by_key<'a>(nodes: &'a [Node], key_path: &str) -> Option<&'a Node> {
    let mut cur = nodes;
    let parts: Vec<&str> = key_path.split('/').collect();
    for (idx, part) in parts.iter().enumerate() {
        let found = cur.iter().find(|n| n.key == *part)?;
        if idx + 1 == parts.len() {
            return Some(found);
        }
        cur = &found.children;
    }
    None
}

fn find_node_by_key_mut<'a>(nodes: &'a mut [Node], key_path: &str) -> Option<&'a mut Node> {
    let mut cur = nodes;
    let parts: Vec<&str> = key_path.split('/').collect();
    for (idx, part) in parts.iter().enumerate() {
        let pos = cur.iter().position(|n| n.key == *part)?;
        if idx + 1 == parts.len() {
            return Some(&mut cur[pos]);
        }
        cur = &mut cur[pos].children;
    }
    None
}

fn remove_node_by_key(nodes: &mut Vec<Node>, key_path: &str) {
    let parts: Vec<&str> = key_path.split('/').collect();
    fn helper(cur: &mut Vec<Node>, parts: &[&str]) {
        let Some((&first, rest)) = parts.split_first() else {
            return;
        };
        if rest.is_empty() {
            cur.retain(|n| n.key != first);
        } else if let Some(parent) = cur.iter_mut().find(|n| n.key == first) {
            helper(&mut parent.children, rest);
        }
    }
    helper(nodes, &parts);
}

fn upsert_node_by_key(nodes: &mut Vec<Node>, key_path: &str, new_node: Node, rows: &[Row]) {
    let parts: Vec<&str> = key_path.split('/').collect();
    fn helper(
        cur: &mut Vec<Node>,
        parts: &[&str],
        prefix: &str,
        new_node: Node,
        rows: &[Row],
    ) {
        let Some((&first, rest)) = parts.split_first() else {
            return;
        };
        let full = if prefix.is_empty() {
            first.to_owned()
        } else {
            format!("{prefix}/{first}")
        };
        if rest.is_empty() {
            if let Some(existing) = cur.iter_mut().find(|n| n.key == first) {
                *existing = new_node;
            } else {
                cur.push(new_node);
            }
        } else {
            let pos = if let Some(pos) = cur.iter().position(|n| n.key == first) {
                pos
            } else {
                let display_name = rows
                    .iter()
                    .find(|r| r.key == full)
                    .and_then(|r| r.names[0].clone().or_else(|| r.names[1].clone()))
                    .unwrap_or_else(|| first.to_owned());
                cur.push(Node {
                    key: first.to_owned(),
                    name: display_name,
                    value: String::new(),
                    bytes: None,
                    children: Vec::new(),
                });
                cur.len() - 1
            };
            helper(&mut cur[pos].children, rest, &full, new_node, rows);
        }
    }
    helper(nodes, &parts, "", new_node, rows);
}

fn elide_registry_path(ui: &Ui, full: &str, max_width: f32) -> String {
    let measure = |s: &str| -> f32 {
        ui.fonts(|f| {
            f.layout_no_wrap(s.to_owned(), FontId::proportional(12.0), Color32::BLACK)
                .size()
                .x
        })
    };
    if measure(full) <= max_width {
        return full.to_owned();
    }
    let parts: Vec<&str> = full.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    if parts.len() <= 2 {
        return full.to_owned();
    }
    let prefix = format!("{}\\...", parts[0]);
    let mut kept = Vec::new();
    for &seg in parts[1..].iter().rev() {
        let mut candidate_parts = kept.clone();
        candidate_parts.push(seg);
        candidate_parts.reverse();
        let candidate = format!("{prefix}\\{}", candidate_parts.join("\\"));
        if measure(&candidate) <= max_width || kept.is_empty() {
            kept.push(seg);
        } else {
            break;
        }
    }
    kept.reverse();
    format!("{prefix}\\{}", kept.join("\\"))
}

// Stored field names are locale-independent keys. Registry identifiers are user
// data and must never be run through the application's translation catalog.
fn property_name(kind: crate::history::SessionKind, key: &str, name: &str) -> String {
    if kind == crate::history::SessionKind::Registry && !key.ends_with("/value:") {
        name.to_owned()
    } else {
        tr(name)
    }
}
fn property_value(kind: crate::history::SessionKind, key: &str, value: &str) -> String {
    if kind == crate::history::SessionKind::Media && key == "audio/data" {
        if let Some(number) = value
            .strip_suffix(" bytes")
            .and_then(|n| n.parse::<usize>().ok())
        {
            return format!("{} bytes", crate::property_sources::display_number(number));
        }
    }
    value.to_owned()
}

#[cfg(test)]
mod locale_tests {
    use super::*;
    use crate::history::SessionKind;

    #[test]
    #[ignore = "Locale changes require their own test process"]
    fn locale_switch_properties_hidden() {
        crate::i18n::initialize(Some("en"));
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/media/tests/fixtures/A.mp3");
        let mut screen = VersionCompareScreen::for_kind(SessionKind::Media);
        screen.load_files(&fixture, &fixture).unwrap();
        let data = screen.rows.iter().find(|r| r.key == "audio/data").unwrap();
        assert_eq!(
            data.names[0].as_deref(),
            Some("version.data"),
            "reader must preserve a language-independent key"
        );
        assert_eq!(
            property_name(screen.kind, &data.key, data.names[0].as_deref().unwrap()),
            "Data"
        );
        assert_eq!(
            property_value(screen.kind, &data.key, data.values[0].as_deref().unwrap()),
            "2,689 bytes"
        );
        // Switch the already loaded session. Never reopen or reread a personal file.
        crate::i18n::initialize(Some("pt-BR"));
        assert_eq!(
            property_name(screen.kind, &data.key, data.names[0].as_deref().unwrap()),
            "Dados"
        );
        assert_eq!(
            property_value(screen.kind, &data.key, data.values[0].as_deref().unwrap()),
            "2.689 bytes"
        );
        crate::i18n::initialize(Some("en"));
        for literal in ["Dados", "Áudio", "Título", "(Padrão)"] {
            assert_eq!(
                property_name(
                    SessionKind::Registry,
                    "hkey_current_user/software/test/value:nome",
                    literal
                ),
                literal
            );
        }
        assert_eq!(
            property_name(
                SessionKind::Registry,
                "hkey_current_user/software/test/value:",
                "version.default"
            ),
            "(Default)"
        );
        crate::i18n::initialize(Some("pt-BR"));
    }
}
fn paint(ui: &Ui, rect: Rect, text: &str, color: Color32) {
    if rect.width() <= 0.0 {
        return;
    }
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .text(
            rect.left_center() + vec2(4.0, 0.0),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(12.0),
            color,
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::TestDialogs;
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pe")
            .join(name)
    }
    #[test]
    fn version_tree_compares_values_and_preserves_previous_files_after_a_load_error() {
        let mut screen = VersionCompareScreen::new();
        screen
            .load_files(&fixture("pe32.bin"), &fixture("pe32-changed.bin"))
            .unwrap();
        let changes: Vec<_> = screen
            .rows
            .iter()
            .filter(|r| !r.group && r.change == Change::Important)
            .collect();
        assert_eq!(changes.len(), 1);
        assert!(changes[0].key.contains("FileDescription"));
        assert_eq!(
            screen
                .rows
                .iter()
                .find(|r| r.key == "code/data")
                .unwrap()
                .change,
            Change::Same
        );
        screen.navigate(true);
        assert!(screen.rows[screen.selected].key.contains("FileDescription"));
        assert!(screen.jump);
        let left = screen.left_path.clone();
        let count = screen.rows.len();
        let mut dialogs = TestDialogs::default();
        screen.open_side(true, &fixture("missing.dll"), &mut dialogs);
        assert_eq!(screen.left_path, left);
        assert_eq!(screen.rows.len(), count);
        assert_eq!(dialogs.errors.len(), 1);
        let saved = screen.session_options();
        let mut restored = VersionCompareScreen::new();
        restored.apply_session_options(&saved);
        assert_eq!(restored.options.ignored, screen.options.ignored);
    }
    #[test]
    fn version_tree_renders_at_small_and_large_sizes() {
        for size in [
            vec2(700.0, 450.0),
            vec2(1150.0, 720.0),
            vec2(1910.0, 1000.0),
        ] {
            let mut screen = VersionCompareScreen::new();
            screen
                .load_files(&fixture("pe32.bin"), &fixture("pe64.bin"))
                .unwrap();
            let ctx = Context::default();
            super::super::theme::apply(&ctx);
            let mut icons = IconManager::new();
            let mut dialogs = TestDialogs::default();
            for _ in 0..2 {
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| {
                            screen.ui(ctx, ui, &mut icons, &mut dialogs);
                        });
                    },
                );
            }
            assert!(dialogs.errors.is_empty());
        }
    }
    #[test]
    fn registry_edit_copy_delete_undo_redo_and_base_key_navigation() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry");
        let a = root.join("left.reg");
        let b = root.join("right.reg");
        let mut screen = VersionCompareScreen::for_kind(crate::history::SessionKind::Registry);
        screen.load_files(&a, &b).unwrap();
        assert_eq!(screen.base_key, "hkey_current_user/software/anycompare-test");
        assert_eq!(screen.base_key_display, "HKEY_CURRENT_USER\\Software\\AnyCompare-Test");
        let ativo_idx = screen
            .rows
            .iter()
            .position(|r| r.key.ends_with("/value:ativo"))
            .unwrap();
        assert_eq!(screen.rows[ativo_idx].change, Change::Important);
        screen.selected = ativo_idx;
        screen.active_side = 0;
        screen.copy_to_other_side();
        let ativo_after = screen
            .rows
            .iter()
            .find(|r| r.key.ends_with("/value:ativo"))
            .unwrap();
        assert_eq!(ativo_after.change, Change::Same);
        screen.undo();
        assert_eq!(
            screen
                .rows
                .iter()
                .find(|r| r.key.ends_with("/value:ativo"))
                .unwrap()
                .change,
            Change::Important
        );
        screen.redo();
        assert_eq!(
            screen
                .rows
                .iter()
                .find(|r| r.key.ends_with("/value:ativo"))
                .unwrap()
                .change,
            Change::Same
        );
        screen.up_one_level_all();
        assert_eq!(screen.base_key, "hkey_current_user/software");
        let group_idx = screen.visible()[0];
        assert!(screen.rows[group_idx].group);
        screen.enter_subkey(group_idx);
        assert_eq!(screen.base_key, "hkey_current_user/software/anycompare-test");
        screen.find_query = "Ativo".into();
        screen.find_step(true);
        assert!(screen.rows[screen.selected].key.ends_with("/value:ativo"));
    }
}
