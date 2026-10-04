//! The egui editor: shots & cast (left), storyboard canvas (centre), inspector (right).
//!
//! Window chrome, fonts, theme, undo history, settings and the export dialog follow
//! whitebox-video-storyboard; the joint handles / IK posing follow rust-pose-studio.

mod canvas;
mod paint;
mod panels;
mod settings;

use eframe::egui::{self, Color32, RichText};
use rss::export::{ExportOptions, Formats, export_to, project_dir_name};
use rss::mannequin::actions::{ACTIONS, floor_pose_names, library_pose};
use rss::mannequin::body::Body;
use rss::mannequin::render::{DrawList, Framing, Style, View, draw_body, framing_camera};
use rss::mannequin::skeleton::{Joint, Skeleton};
use rss::model::{Actor, CAST_COLORS, CastMember, PoseData, Project, Prop, PropKind, Shot};
use settings::Settings;
use std::path::PathBuf;
use std::sync::Arc;

pub const ACCENT: Color32 = Color32::from_rgb(240, 128, 20);
pub const OK_GREEN: Color32 = Color32::from_rgb(90, 200, 120);
pub const ERR_RED: Color32 = Color32::from_rgb(240, 90, 90);

/// What is selected on the canvas.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Sel {
    #[default]
    None,
    Actor(String),
    Prop(String),
}

/// An active canvas drag.
#[derive(Clone, Debug)]
pub enum Drag {
    /// Move a character (and its movement path) by the pointer delta.
    Actor {
        id: String,
        start: [f32; 2],
        p0: [f32; 2],
        scale0: f32,
        path0: Vec<[f32; 2]>,
    },
    Prop {
        id: String,
        start: [f32; 2],
        p0: [f32; 2],
        size0: [f32; 2],
    },
    /// Resize a prop from its top-right corner handle.
    PropResize {
        id: String,
        size0: [f32; 2],
        p0: [f32; 2],
    },
    /// Move one point of a movement path (index into `movement.path`).
    PathPoint {
        id: String,
        idx: usize,
    },
    /// Pose a joint (IK for hands / feet, FK aim otherwise).
    Joint {
        id: String,
        joint: Joint,
        depth: f32,
        offset: [f32; 2],
    },
    Pan,
}

#[derive(Clone, Copy, Debug)]
pub enum PendingAction {
    New,
    Sample,
    Open,
}

/// Pose library thumbnail.
pub struct Thumb {
    pub key: String,
    pub label: String,
    pub list: DrawList,
}

pub struct App {
    pub project: Project,
    pub path: Option<PathBuf>,
    pub shot: usize,
    pub sel: Sel,
    pub sel_joint: Option<Joint>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    last_change: f64,
    force_checkpoint: bool,
    history_jump: bool,
    pub dirty: bool,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub drag: Option<Drag>,
    pub status: Option<(bool, String)>,
    pub export_opts: ExportOptions,
    pub settings: Settings,
    pub show_export: bool,
    pub export_base: String,
    pub last_export: Option<(PathBuf, usize)>,
    pub show_help: bool,
    pub show_about: bool,
    pub show_text: bool,
    pub confirm_discard: Option<PendingAction>,
    pub cast_edit: Option<String>,
    pub figs: paint::FigCache,
    /// Cache keys of the figures drawn on the canvas last frame (actor id → key), for picking.
    pub pick_keys: Vec<(String, String)>,
    pub action_thumbs: Vec<Thumb>,
    pub floor_thumbs: Vec<Thumb>,
}

fn setup_fonts(ctx: &egui::Context) {
    // eframe's `default_fonts` feature is off: the bundled CJK font covers Latin +
    // Traditional Chinese, egui's small icon font covers the toolbar symbols.
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert("noto_cjk_tc".into(), Arc::new(egui::FontData::from_static(rss::fonts::cjk_font())));
    fonts.font_data.insert(
        "emoji-icon-font".into(),
        Arc::new(
            egui::FontData::from_static(epaint_default_fonts::EMOJI_ICON)
                .tweak(egui::FontTweak { scale: 0.90, ..Default::default() }),
        ),
    );
    let mut chain = vec!["noto_cjk_tc".to_string(), "emoji-icon-font".to_string()];
    if let Some((bytes, index)) = rss::fonts::load_system_fallback() {
        let mut fd = egui::FontData::from_owned(bytes);
        fd.index = index;
        fonts.font_data.insert("system_cjk".into(), Arc::new(fd));
        chain.push("system_cjk".into());
    }
    fonts.families.insert(egui::FontFamily::Proportional, chain.clone());
    fonts.families.insert(egui::FontFamily::Monospace, chain);
    ctx.set_fonts(fonts);
}

fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(32, 34, 38);
    v.window_fill = Color32::from_rgb(40, 43, 48);
    v.extreme_bg_color = Color32::from_rgb(20, 21, 24);
    v.selection.bg_fill = Color32::from_rgb(150, 85, 25);
    v.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.slider_trailing_fill = true;
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
    });
}

fn thumb(skel: &Skeleton, key: &str, label: String) -> Option<Thumb> {
    let (pose, _) = library_pose(skel, key)?;
    let body = Body::new(skel, &pose);
    let cam = framing_camera(&body, 30.0, 8.0, false);
    let style = Style { line_width: 0.0055, fill: false, ..Style::default() };
    let list =
        draw_body(&body, &View::new(&cam), &style, Framing::Fit { margin: 0.05 }, [0.0, 0.0, 120.0, 120.0], 1.0).0;
    Some(Thumb { key: key.into(), label, list })
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<String>) -> App {
        setup_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);
        let settings = Settings::load();
        let skel = Skeleton::new(Default::default());
        let action_thumbs = ACTIONS.iter().filter_map(|a| thumb(&skel, a.key, format!("{}\n{}", a.zh, a.en))).collect();
        let floor_thumbs = floor_pose_names(&skel)
            .iter()
            .enumerate()
            .filter_map(|(i, n)| thumb(&skel, &format!("floor:{}", i + 1), format!("地面姿勢 {}\n{n}", i + 1)))
            .collect();
        let mut app = App {
            project: rss::sample::sample_project(),
            path: None,
            shot: 0,
            sel: Sel::None,
            sel_joint: None,
            undo: vec![],
            redo: vec![],
            last_change: -10.0,
            force_checkpoint: false,
            history_jump: false,
            dirty: false,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            drag: None,
            status: Some((true, "已載入範例專案「咖啡店的相遇」。點選角色可拖曳移動、拖曳關節調整姿勢。".into())),
            export_opts: settings.export.clone(),
            export_base: String::new(),
            settings,
            show_export: false,
            last_export: None,
            show_help: false,
            show_about: false,
            show_text: false,
            confirm_discard: None,
            cast_edit: None,
            figs: paint::FigCache::default(),
            pick_keys: vec![],
            action_thumbs,
            floor_thumbs,
        };
        match open {
            Some(p) => app.open_path(PathBuf::from(p)),
            // Start with the sample's first character selected so its pose tools are visible.
            None => app.sel = app.cur().actors.first().map(|a| Sel::Actor(a.id.clone())).unwrap_or_default(),
        }
        app
    }

    // ------------------------------------------------------------------ helpers

    pub fn cur(&self) -> &Shot {
        &self.project.shots[self.shot]
    }
    pub fn cur_mut(&mut self) -> &mut Shot {
        &mut self.project.shots[self.shot]
    }
    pub fn actor_index(&self, id: &str) -> Option<usize> {
        self.cur().actors.iter().position(|a| a.id == id)
    }
    pub fn prop_index(&self, id: &str) -> Option<usize> {
        self.cur().props.iter().position(|a| a.id == id)
    }
    pub fn actor_mut(&mut self, id: &str) -> Option<&mut Actor> {
        let i = self.actor_index(id)?;
        Some(&mut self.cur_mut().actors[i])
    }
    pub fn prop_mut(&mut self, id: &str) -> Option<&mut Prop> {
        let i = self.prop_index(id)?;
        Some(&mut self.cur_mut().props[i])
    }

    pub fn checkpoint(&mut self) {
        self.force_checkpoint = true;
    }

    pub fn set_status(&mut self, ok: bool, msg: impl Into<String>) {
        self.status = Some((ok, msg.into()));
    }

    pub fn undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, p));
            self.history_jump = true;
            self.last_change = -10.0;
            self.clamp();
        }
    }

    pub fn redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, p));
            self.history_jump = true;
            self.last_change = -10.0;
            self.clamp();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn clamp(&mut self) {
        if self.project.shots.is_empty() {
            self.project.shots.push(Shot::default());
        }
        self.shot = self.shot.min(self.project.shots.len() - 1);
        let valid = match &self.sel {
            Sel::None => true,
            Sel::Actor(id) => self.actor_index(id).is_some(),
            Sel::Prop(id) => self.prop_index(id).is_some(),
        };
        if !valid {
            self.sel = Sel::None;
        }
    }

    pub fn select_shot(&mut self, i: usize) {
        if i != self.shot {
            self.shot = i;
            self.sel = Sel::None;
            self.sel_joint = None;
        }
    }

    /// Add a character of `cast` (or a new cast member when `None`) in the middle of the floor.
    pub fn add_actor(&mut self, cast: Option<String>) {
        let cast = match cast {
            Some(c) => c,
            None => self.add_cast(),
        };
        let (cw, ch) = (self.project.canvas.width as f32, self.project.canvas.height as f32);
        let hz = self.cur().horizon * ch;
        let n = self.cur().actors.len() as f32;
        let x = cw * (0.35 + 0.15 * (n % 3.0));
        let y = (hz + (ch - hz) * 0.62).min(ch * 0.95);
        let id = self.cur().next_actor_id();
        let mut a = rss::sample::actor(&self.project, &id, &cast, "stand", x, y, 0.0, 1.0);
        a.scale = self.project.depth_scale(self.cur(), y);
        self.cur_mut().actors.push(a);
        self.sel = Sel::Actor(id.clone());
        self.sel_joint = None;
        self.checkpoint();
        let name = self.project.cast_member(&cast).map(|c| c.name.clone()).unwrap_or_default();
        self.set_status(true, format!("已新增角色 {name}（{id}）"));
    }

    /// New cast member with the next free colour; returns its id.
    pub fn add_cast(&mut self) -> String {
        let id = self.project.next_cast_id();
        let n = self.project.cast.len();
        self.project.cast.push(CastMember {
            id: id.clone(),
            name: format!("角色{}", n + 1),
            color: CAST_COLORS[n % CAST_COLORS.len()],
            ..CastMember::default()
        });
        self.checkpoint();
        id
    }

    pub fn add_prop(&mut self, kind: PropKind) {
        let (cw, ch) = (self.project.canvas.width as f32, self.project.canvas.height as f32);
        let hz = self.cur().horizon * ch;
        let y = if kind.is_set_piece() { hz + 4.0 } else { (hz + (ch - hz) * 0.5).min(ch * 0.95) };
        let id = self.cur().next_prop_id(kind);
        let scale = self.project.depth_scale(self.cur(), y.max(hz + 40.0));
        let mut p = rss::sample::prop(&self.project, &id, kind, cw * 0.5, y, scale);
        p.label = String::new();
        self.cur_mut().props.push(p);
        self.sel = Sel::Prop(id.clone());
        self.checkpoint();
        self.set_status(true, format!("已新增道具 {}（{id}）", kind.zh()));
    }

    pub fn delete_selected(&mut self) {
        match self.sel.clone() {
            Sel::Actor(id) => {
                if let Some(i) = self.actor_index(&id) {
                    self.cur_mut().actors.remove(i);
                    self.set_status(true, format!("已刪除角色 {id}"));
                }
            }
            Sel::Prop(id) => {
                if let Some(i) = self.prop_index(&id) {
                    self.cur_mut().props.remove(i);
                    self.set_status(true, format!("已刪除道具 {id}"));
                }
            }
            Sel::None => return,
        }
        self.sel = Sel::None;
        self.checkpoint();
    }

    pub fn duplicate_selected(&mut self) {
        match self.sel.clone() {
            Sel::Actor(id) => {
                if let Some(i) = self.actor_index(&id) {
                    let mut a = self.cur().actors[i].clone();
                    a.id = self.cur().next_actor_id();
                    a.x += 80.0;
                    for p in &mut a.movement.path {
                        p[0] += 80.0;
                    }
                    self.sel = Sel::Actor(a.id.clone());
                    self.cur_mut().actors.push(a);
                }
            }
            Sel::Prop(id) => {
                if let Some(i) = self.prop_index(&id) {
                    let mut p = self.cur().props[i].clone();
                    p.id = self.cur().next_prop_id(p.kind);
                    p.x += 60.0;
                    self.sel = Sel::Prop(p.id.clone());
                    self.cur_mut().props.push(p);
                }
            }
            Sel::None => return,
        }
        self.checkpoint();
    }

    pub fn nudge(&mut self, dx: f32, dy: f32) {
        match self.sel.clone() {
            Sel::Actor(id) => {
                if let Some(a) = self.actor_mut(&id) {
                    a.x += dx;
                    a.y += dy;
                }
            }
            Sel::Prop(id) => {
                if let Some(p) = self.prop_mut(&id) {
                    p.x += dx;
                    p.y += dy;
                }
            }
            Sel::None => {}
        }
    }

    pub fn add_shot(&mut self, duplicate: bool) {
        let mut s = if duplicate {
            self.cur().clone()
        } else {
            let c = self.cur();
            Shot { setting: c.setting.clone(), horizon: c.horizon, ..Shot::default() }
        };
        s.id = self.project.next_shot_id();
        let n = self.project.shots.len() + 1;
        s.name = if duplicate { format!("{}（複製）", s.name) } else { format!("鏡頭 {n}") };
        self.project.shots.insert(self.shot + 1, s);
        self.select_shot(self.shot + 1);
        self.checkpoint();
    }

    pub fn delete_shot(&mut self) {
        if self.project.shots.len() > 1 {
            self.project.shots.remove(self.shot);
            self.shot = self.shot.min(self.project.shots.len() - 1);
            self.sel = Sel::None;
            self.checkpoint();
        }
    }

    pub fn move_shot(&mut self, delta: i32) {
        let n = self.project.shots.len() as i32;
        let j = (self.shot as i32 + delta).clamp(0, n - 1) as usize;
        if j != self.shot {
            self.project.shots.swap(self.shot, j);
            self.shot = j;
            self.checkpoint();
        }
    }

    /// Apply a library pose to an actor (resets joint edits).
    pub fn apply_pose(&mut self, id: &str, key: &str) {
        let Some(i) = self.actor_index(id) else { return };
        let props = self.project.cast_of(&self.cur().actors[i]).proportions();
        if let Some((pose, lift)) = library_pose(&Skeleton::new(props), key) {
            self.cur_mut().actors[i].pose = PoseData::from_pose(key, &pose, lift);
            self.checkpoint();
        }
    }

    // ------------------------------------------------------------------ files

    fn discard_ok(&mut self, action: PendingAction) -> bool {
        if self.dirty {
            self.confirm_discard = Some(action);
            false
        } else {
            true
        }
    }

    pub fn do_action(&mut self, action: PendingAction, force: bool) {
        if !force && !self.discard_ok(action) {
            return;
        }
        match action {
            PendingAction::New => {
                let mut p = Project::default();
                p.cast.push(CastMember { id: "char_1".into(), name: "主角".into(), ..CastMember::default() });
                self.load_project(p, None);
                self.set_status(true, "新專案：用工具列「＋角色」「＋道具」開始編排");
            }
            PendingAction::Sample => {
                self.load_project(rss::sample::sample_project(), None);
                self.set_status(true, "已載入範例專案");
            }
            PendingAction::Open => {
                if let Some(p) = rfd::FileDialog::new().add_filter("分鏡專案 JSON", &["json"]).pick_file() {
                    self.open_path(p);
                }
            }
        }
    }

    fn load_project(&mut self, p: Project, path: Option<PathBuf>) {
        self.project = p;
        self.path = path;
        self.shot = 0;
        self.sel = Sel::None;
        self.sel_joint = None;
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        self.history_jump = true;
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match Project::load(&path) {
            Ok(p) => {
                self.load_project(p, Some(path.clone()));
                self.set_status(true, format!("已開啟 {}", path.display()));
            }
            Err(e) => self.set_status(false, format!("開啟失敗：{e}")),
        }
    }

    pub fn save(&mut self, save_as: bool) {
        let path = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("分鏡專案 JSON", &["json"])
                .set_file_name(format!("{}.json", rss::export::sanitize_name(&self.project.title)))
                .save_file(),
        };
        let Some(path) = path else { return };
        match self.project.save(&path) {
            Ok(()) => {
                self.path = Some(path.clone());
                self.dirty = false;
                self.set_status(true, format!("已儲存 {}", path.display()));
            }
            Err(e) => self.set_status(false, format!("儲存失敗：{e}")),
        }
    }

    pub fn export_dialog(&mut self) {
        if self.export_base.is_empty() {
            self.export_base = self.default_export_base().display().to_string();
        }
        self.show_export = true;
    }

    fn default_export_base(&self) -> PathBuf {
        if let Some(b) = self.settings.export_base.as_ref().filter(|b| b.is_dir()) {
            return b.clone();
        }
        if let Some(d) = self.path.as_ref().and_then(|p| p.parent()).filter(|d| d.is_dir()) {
            return d.to_path_buf();
        }
        if let Some(h) = settings::home_dir() {
            let docs = h.join("Documents");
            return if docs.is_dir() { docs } else { h };
        }
        PathBuf::from(".")
    }

    pub fn export_name(&self) -> String {
        match &self.path {
            Some(_) => project_dir_name(self.path.as_deref()),
            None => rss::export::sanitize_name(&self.project.title),
        }
    }

    pub fn do_export(&mut self) {
        let base = PathBuf::from(self.export_base.trim());
        match export_to(&self.project, &base, &self.export_name(), self.settings.export_subdir, &self.export_opts) {
            Ok(rep) => {
                self.set_status(true, format!("匯出完成：{} 個檔案 → {}", rep.files.len(), rep.dir.display()));
                self.last_export = Some((rep.dir.clone(), rep.files.len()));
                self.settings.export = self.export_opts.clone();
                self.settings.export_base = Some(base);
                self.settings.save();
            }
            Err(e) => self.set_status(false, format!("匯出失敗：{e}")),
        }
    }

    pub fn export_shot_png(&mut self) {
        let name = rss::describe::shot_png_name(self.shot, &self.cur().id);
        let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name(name).save_file() else {
            return;
        };
        let res = rss::export::render_shot(&self.project, self.shot, &self.export_opts)
            .and_then(|pm| rss::raster::encode_png(&pm))
            .and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string()));
        match res {
            Ok(()) => self.set_status(true, format!("已輸出 {}", path.display())),
            Err(e) => self.set_status(false, format!("輸出失敗：{e}")),
        }
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        if !self.show_export {
            return;
        }
        let mut open = true;
        let mut do_export = false;
        egui::Window::new("匯出分鏡草稿 Export")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("輸出格式").strong().color(ACCENT));
                egui::Grid::new("fmt_grid").num_columns(2).spacing([18.0, 4.0]).show(ui, |ui| {
                    for (n, (key, label, file)) in Formats::INFO.iter().enumerate() {
                        if let Some(b) = self.export_opts.formats.get_mut(key) {
                            ui.checkbox(b, *label).on_hover_text(*file);
                        }
                        if n % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.small_button("全選").clicked() {
                        self.export_opts.formats = Formats::default();
                    }
                    if ui.small_button("全不選").clicked() {
                        self.export_opts.formats = Formats::NONE;
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new("草稿圖內容").strong().color(ACCENT));
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.export_opts.labels, "角色名牌");
                    ui.checkbox(&mut self.export_opts.motion, "走位箭頭");
                    ui.checkbox(&mut self.export_opts.ghosts, "終點殘影");
                    ui.checkbox(&mut self.export_opts.dialogue, "對白泡泡");
                    ui.checkbox(&mut self.export_opts.prop_labels, "道具名稱");
                    ui.checkbox(&mut self.export_opts.header, "鏡頭資訊列");
                });
                ui.checkbox(&mut self.export_opts.embed_images, "HTML 內嵌圖片（單一檔案，可直接分享）");
                ui.add_space(4.0);
                ui.label(RichText::new("輸出位置").strong().color(ACCENT));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.export_base).desired_width(340.0));
                    if ui.button("選擇…").clicked() {
                        let mut dlg = rfd::FileDialog::new().set_title("選擇匯出位置");
                        if !self.export_base.is_empty() {
                            dlg = dlg.set_directory(&self.export_base);
                        }
                        if let Some(d) = dlg.pick_folder() {
                            self.export_base = d.display().to_string();
                        }
                    }
                });
                ui.checkbox(&mut self.settings.export_subdir, "建立「專案名_日期_時間」子資料夾（建議）");
                let target = if self.settings.export_subdir {
                    PathBuf::from(&self.export_base).join(format!("{}_YYYYMMDD_HHMMSS", self.export_name()))
                } else {
                    PathBuf::from(&self.export_base)
                };
                ui.label(RichText::new(format!("將輸出到：{}", target.display())).small().weak());
                ui.separator();
                ui.horizontal(|ui| {
                    let ok = self.export_opts.formats != Formats::NONE && !self.export_base.trim().is_empty();
                    if ui.add_enabled(ok, egui::Button::new(RichText::new("⬇ 匯出").strong())).clicked() {
                        do_export = true;
                    }
                    if !ok {
                        ui.label(RichText::new("請至少選一種格式並指定位置").color(ERR_RED).small());
                    }
                });
                if let Some((dir, n)) = self.last_export.clone() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("✔ 已匯出 {n} 個檔案到：")).color(OK_GREEN));
                    ui.label(RichText::new(dir.display().to_string()).monospace());
                    ui.horizontal(|ui| {
                        if ui.button("開啟資料夾").clicked() {
                            settings::open_folder(&dir);
                        }
                        if ui.button("複製路徑").clicked() {
                            ui.ctx().copy_text(dir.display().to_string());
                        }
                    });
                }
            });
        if do_export {
            self.do_export();
        }
        if !open {
            self.show_export = false;
            self.settings.export = self.export_opts.clone();
            self.settings.save();
        }
    }

    // ------------------------------------------------------------------ input

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        let sc = |m, k| KeyboardShortcut::new(m, k);
        let (redo_a, redo_b, save_as, save, open, export, dup) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Y)),
                i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::E)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::D)),
            )
        });
        let undo = ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Z)));
        if redo_a || redo_b {
            self.redo();
        } else if undo {
            self.undo();
        }
        if save_as {
            self.save(true);
        } else if save {
            self.save(false);
        }
        if open {
            self.do_action(PendingAction::Open, false);
        }
        if export {
            self.export_dialog();
        }
        if dup {
            self.duplicate_selected();
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (del, up, down, left, right, shift, pgup, pgdn, esc) = ctx.input(|i| {
            (
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.modifiers.shift,
                i.key_pressed(Key::PageUp),
                i.key_pressed(Key::PageDown),
                i.key_pressed(Key::Escape),
            )
        });
        if del {
            self.delete_selected();
        }
        let step = if shift { 10.0 } else { 1.0 };
        let (dx, dy) = ((right as i32 - left as i32) as f32 * step, (down as i32 - up as i32) as f32 * step);
        if dx != 0.0 || dy != 0.0 {
            self.nudge(dx, dy);
        }
        if pgup && self.shot > 0 {
            self.select_shot(self.shot - 1);
        }
        if pgdn && self.shot + 1 < self.project.shots.len() {
            self.select_shot(self.shot + 1);
        }
        if esc {
            self.sel = Sel::None;
            self.sel_joint = None;
        }
    }

    fn commit_history(&mut self, ctx: &egui::Context, before: Project) {
        if self.history_jump {
            self.history_jump = false;
            self.force_checkpoint = false;
            return;
        }
        if self.project != before {
            let now = ctx.input(|i| i.time);
            if self.force_checkpoint || now - self.last_change > 0.8 {
                self.undo.push(before);
                if self.undo.len() > 300 {
                    self.undo.remove(0);
                }
                self.redo.clear();
            }
            self.last_change = now;
            self.dirty = true;
        }
        self.force_checkpoint = false;
    }

    // ------------------------------------------------------------------ chrome

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("檔案", |ui| {
                if ui.button("新增專案").clicked() {
                    ui.close();
                    self.do_action(PendingAction::New, false);
                }
                if ui.button("載入範例專案").clicked() {
                    ui.close();
                    self.do_action(PendingAction::Sample, false);
                }
                if ui.button("開啟…            Ctrl+O").clicked() {
                    ui.close();
                    self.do_action(PendingAction::Open, false);
                }
                ui.separator();
                if ui.button("儲存              Ctrl+S").clicked() {
                    ui.close();
                    self.save(false);
                }
                if ui.button("另存新檔…   Ctrl+Shift+S").clicked() {
                    ui.close();
                    self.save(true);
                }
                ui.separator();
                if ui.button("匯出…（PNG 草稿 / Markdown / HTML / JSON）  Ctrl+E").clicked() {
                    ui.close();
                    self.export_dialog();
                }
                if ui.button("只輸出目前鏡頭 PNG…").clicked() {
                    ui.close();
                    self.export_shot_png();
                }
                ui.separator();
                if ui.button("結束").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("編輯", |ui| {
                if ui.add_enabled(self.can_undo(), egui::Button::new("復原    Ctrl+Z")).clicked() {
                    self.undo();
                    ui.close();
                }
                if ui.add_enabled(self.can_redo(), egui::Button::new("重做    Ctrl+Y")).clicked() {
                    self.redo();
                    ui.close();
                }
                ui.separator();
                let sel = self.sel != Sel::None;
                if ui.add_enabled(sel, egui::Button::new("複製選取    Ctrl+D")).clicked() {
                    self.duplicate_selected();
                    ui.close();
                }
                if ui.add_enabled(sel, egui::Button::new("刪除選取    Delete")).clicked() {
                    self.delete_selected();
                    ui.close();
                }
                ui.separator();
                if ui.button("新增鏡頭").clicked() {
                    self.add_shot(false);
                    ui.close();
                }
                if ui.button("複製目前鏡頭").clicked() {
                    self.add_shot(true);
                    ui.close();
                }
            });
            ui.menu_button("檢視", |ui| {
                ui.checkbox(&mut self.settings.show_handles, "顯示關節控制點");
                ui.checkbox(&mut self.settings.auto_depth, "拖曳時依景深自動縮放");
                ui.checkbox(&mut self.export_opts.labels, "角色名牌");
                ui.checkbox(&mut self.export_opts.motion, "走位箭頭");
                ui.checkbox(&mut self.export_opts.ghosts, "終點殘影");
                ui.checkbox(&mut self.export_opts.dialogue, "對白泡泡");
                ui.checkbox(&mut self.export_opts.prop_labels, "道具名稱");
                ui.checkbox(&mut self.export_opts.header, "鏡頭資訊列");
                ui.separator();
                ui.checkbox(&mut self.show_text, "鏡頭敘述預覽（Markdown）");
            });
            ui.menu_button("說明", |ui| {
                if ui.button("操作說明…").clicked() {
                    self.show_help = true;
                    ui.close();
                }
                if ui.button("關於…").clicked() {
                    self.show_about = true;
                    ui.close();
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(self.can_undo(), egui::Button::new("⟲ 復原")).clicked() {
                self.undo();
            }
            if ui.add_enabled(self.can_redo(), egui::Button::new("⟳ 重做")).clicked() {
                self.redo();
            }
            ui.separator();
            ui.menu_button("＋ 角色", |ui| {
                let cast: Vec<(String, String)> =
                    self.project.cast.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
                for (id, name) in cast {
                    if ui.button(format!("{name}（{id}）")).clicked() {
                        self.add_actor(Some(id));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("＋ 新角色…").clicked() {
                    self.add_actor(None);
                    ui.close();
                }
            })
            .response
            .on_hover_text("在目前鏡頭加入角色（線稿人偶）");
            ui.menu_button("＋ 道具 / 場景物件", |ui| {
                egui::Grid::new("prop_menu").num_columns(3).show(ui, |ui| {
                    for (n, k) in PropKind::ALL.iter().enumerate() {
                        if ui.button(format!("{} {}", k.zh(), k.en())).clicked() {
                            self.add_prop(*k);
                            ui.close();
                        }
                        if n % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            });
            let sel = self.sel != Sel::None;
            if ui.add_enabled(sel, egui::Button::new("🗐 複製")).clicked() {
                self.duplicate_selected();
            }
            if ui.add_enabled(sel, egui::Button::new("🗑 刪除")).clicked() {
                self.delete_selected();
            }
            ui.separator();
            ui.toggle_value(&mut self.settings.show_handles, "關節")
                .on_hover_text("顯示選取角色的關節控制點（拖曳調整姿勢）");
            ui.toggle_value(&mut self.settings.auto_depth, "景深縮放")
                .on_hover_text("拖曳角色 / 道具前後移動時，依透視自動放大縮小");
            ui.toggle_value(&mut self.export_opts.motion, "走位");
            ui.toggle_value(&mut self.export_opts.labels, "名牌");
            ui.toggle_value(&mut self.export_opts.dialogue, "對白");
            ui.separator();
            ui.label("縮放");
            ui.add(egui::Slider::new(&mut self.zoom, 0.25..=4.0).show_value(false).logarithmic(true));
            if ui.small_button(format!("{:.0}%", self.zoom * 100.0)).on_hover_text("重設為符合視窗").clicked() {
                self.zoom = 1.0;
                self.pan = egui::Vec2::ZERO;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("⬇ 匯出").strong().color(ACCENT))
                    .on_hover_text("Ctrl+E：輸出草稿 PNG、storyboard.md、storyboard.html、project.json")
                    .clicked()
                {
                    self.export_dialog();
                }
                if ui.button("💾 儲存").clicked() {
                    self.save(false);
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.status {
                Some((true, m)) => ui.label(RichText::new(m).color(OK_GREEN)),
                Some((false, m)) => ui.label(RichText::new(m).color(ERR_RED)),
                None => ui.label(RichText::new("就緒").weak()),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let file = self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "（未儲存）".into());
                ui.label(
                    RichText::new(format!(
                        "{}{} · {}×{} · {} 個鏡頭 · 共 {:.1}s",
                        if self.dirty { "● " } else { "" },
                        file,
                        self.project.canvas.width,
                        self.project.canvas.height,
                        self.project.shots.len(),
                        self.project.total_duration()
                    ))
                    .small()
                    .weak(),
                );
            });
        });
    }

    fn windows(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("操作說明").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("help").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                for (k, v) in [
                    ("點選角色 / 道具", "選取（右側面板編輯屬性）"),
                    ("拖曳角色身體 / 紫色骨盆點", "移動角色（走位路徑跟著平移）"),
                    ("拖曳藍色圓點", "旋轉該關節（FK）"),
                    ("拖曳綠色菱形（手腕、腳踝）", "IK：整條手臂 / 腿跟著移動"),
                    ("拖曳走位路徑點", "調整移動路徑；最後一點是終點"),
                    ("Shift + 點擊空白處", "為選取角色加入走位路徑點"),
                    ("拖曳道具右上角方塊", "調整道具大小"),
                    ("右鍵 / 中鍵拖曳、滾輪", "平移、縮放畫布"),
                    ("方向鍵 / Shift+方向鍵", "微調位置 1px / 10px"),
                    ("Delete", "刪除選取"),
                    ("Ctrl+D", "複製選取"),
                    ("PageUp / PageDown", "上一個 / 下一個鏡頭"),
                    ("Ctrl+Z / Ctrl+Y", "復原 / 重做"),
                    ("Ctrl+S / Ctrl+O", "儲存 / 開啟專案"),
                    ("Ctrl+E", "匯出（PNG 草稿、Markdown、HTML、JSON）"),
                    ("Esc", "取消選取"),
                ] {
                    ui.label(RichText::new(k).strong());
                    ui.label(v);
                    ui.end_row();
                }
            });
        });
        self.show_help = open;
        let mut open = self.show_about;
        egui::Window::new("關於").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.heading(format!("Rust Scene Storyboard {}", env!("CARGO_PKG_VERSION")));
            ui.label("影片分鏡工具：以線稿人偶設定角色姿勢、位置與走位，搭配道具與場景，輸出草稿圖與 Markdown / HTML 分鏡敘述。");
            ui.label("人偶與線稿渲染來自 rust-pose-studio；分鏡、匯出與介面架構延續 whitebox-video-storyboard。");
            ui.hyperlink("https://github.com/stevenke1981/rust-scene-storyboard");
            ui.label("© 2026 Ke Sheng Da — MIT License");
            ui.label("內附字型：Noto Sans CJK TC（子集），SIL Open Font License 1.1");
        });
        self.show_about = open;

        let mut open = self.show_text;
        egui::Window::new("鏡頭敘述預覽").open(&mut open).default_size([520.0, 520.0]).show(ctx, |ui| {
            let d = rss::describe::describe_shot(&self.project, self.shot);
            let text = rss::describe::narrative(&self.project, self.cur(), &d);
            ui.label(RichText::new(rss::describe::shot_summary(&self.project, self.shot)).strong());
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(text);
                ui.separator();
                let (sketch, legend) = rss::describe::ascii_sketch(&self.project, self.cur());
                ui.label(RichText::new(sketch).monospace().size(11.0));
                for (c, l) in legend {
                    ui.label(format!("{c} = {l}"));
                }
            });
        });
        self.show_text = open;

        if let Some(action) = self.confirm_discard {
            let mut close = false;
            egui::Modal::new(egui::Id::new("confirm_discard")).show(ctx, |ui| {
                ui.heading("尚未儲存的變更");
                ui.label("目前的專案有未儲存的變更，要放棄嗎？");
                ui.horizontal(|ui| {
                    if ui.button("放棄變更並繼續").clicked() {
                        close = true;
                        self.confirm_discard = None;
                        self.do_action(action, true);
                    }
                    if ui.button("取消").clicked() {
                        close = true;
                    }
                });
            });
            if close {
                self.confirm_discard = None;
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.clamp();
        self.figs.begin_frame();
        let before = self.project.clone();
        self.shortcuts(&ctx);

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("shots").resizable(true).default_size(250.0).min_size(210.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.left_panel(ui));
        });
        egui::Panel::right("inspector").resizable(true).default_size(360.0).min_size(300.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.inspector(ui));
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(22, 23, 26)).inner_margin(0))
            .show(ui, |ui| self.canvas(ui));
        self.windows(&ctx);
        self.export_window(&ctx);
        self.clamp();
        self.commit_history(&ctx, before);
    }
}
