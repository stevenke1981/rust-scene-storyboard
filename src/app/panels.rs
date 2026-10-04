//! Side panels: project / shot list / cast (left) and the inspector (right).

use super::canvas::joint_zh;
use super::paint::{View, paint_figure, paint_items};
use super::{ACCENT, App, Sel, Thumb};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, StrokeKind, vec2};
use rss::draw::{DraftOptions, shot_items};
use rss::mannequin::actions::{ACTIONS, pose_label};
use rss::mannequin::render::Frame2;
use rss::mannequin::skeleton::{BodyType, Joint, Pose, Skeleton};
use rss::model::*;

/// Combo box over a `labeled_enum!` type. Evaluates to `true` when changed.
macro_rules! enum_combo {
    ($ui:expr, $id:expr, $v:expr, $t:ty) => {{
        let mut changed = false;
        let cur: $t = *$v;
        egui::ComboBox::from_id_salt($id).selected_text(cur.label()).width(210.0).show_ui($ui, |ui| {
            for k in <$t>::ALL {
                if ui.selectable_value($v, *k, k.label()).changed() {
                    changed = true;
                }
            }
        });
        changed
    }};
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong().color(ACCENT));
}

fn multiline(ui: &mut egui::Ui, v: &mut String, rows: usize, hint: &str) -> egui::Response {
    ui.add(egui::TextEdit::multiline(v).desired_rows(rows).desired_width(f32::INFINITY).hint_text(hint))
}

fn single(ui: &mut egui::Ui, v: &mut String, hint: &str) -> egui::Response {
    ui.add(egui::TextEdit::singleline(v).desired_width(f32::INFINITY).hint_text(hint))
}

/// Grid of pose thumbnails; returns the clicked pose key.
fn pose_grid(ui: &mut egui::Ui, id: &str, thumbs: &[Thumb], current: &str) -> Option<String> {
    let cols = ((ui.available_width() + 4.0) / 76.0).floor().max(2.0) as usize;
    let cell = ((ui.available_width() - (cols as f32 - 1.0) * 4.0) / cols as f32).floor();
    let mut clicked = None;
    egui::Grid::new(id).spacing([4.0, 4.0]).show(ui, |ui| {
        for (i, t) in thumbs.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(cell, cell * 1.18), Sense::click());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, Color32::WHITE);
            let inner = rect.shrink(3.0);
            let fr = Frame2::fit(&t.list, inner.left(), inner.top(), inner.width(), inner.height() - 16.0, 0.04);
            let view = View { origin: egui::Pos2::ZERO, scale: 1.0 };
            paint_figure(&painter, &t.list, &fr, None, view, cell * 1.6);
            let name = t.label.lines().next().unwrap_or_default().to_string();
            painter.text(
                egui::pos2(rect.center().x, rect.bottom() - 3.0),
                egui::Align2::CENTER_BOTTOM,
                name,
                egui::FontId::proportional(11.0),
                Color32::from_gray(40),
            );
            let border = if t.key == current {
                Stroke::new(2.5, ACCENT)
            } else if resp.hovered() {
                Stroke::new(1.5, Color32::from_gray(160))
            } else {
                Stroke::new(1.0, Color32::from_gray(70))
            };
            painter.rect_stroke(rect, 4.0, border, StrokeKind::Inside);
            if resp.on_hover_text(t.label.replace('\n', " · ")).clicked() {
                clicked = Some(t.key.clone());
            }
            if (i + 1) % cols == 0 {
                ui.end_row();
            }
        }
    });
    clicked
}

impl App {
    // ------------------------------------------------------------------ left

    pub(super) fn left_panel(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("專案 Project").strong()).default_open(false).show(ui, |ui| {
            ui.label("片名");
            single(ui, &mut self.project.title, "片名 / 專案名稱");
            ui.label("故事大綱");
            multiline(ui, &mut self.project.synopsis, 3, "一句話故事 / 大綱");
            ui.label("畫面比例");
            let cur = self.project.canvas.preset.clone();
            let label = ASPECT_PRESETS
                .iter()
                .find(|p| p.0 == cur)
                .map(|p| p.1.to_string())
                .unwrap_or(format!("{}×{}", self.project.canvas.width, self.project.canvas.height));
            egui::ComboBox::from_id_salt("aspect").selected_text(label).show_ui(ui, |ui| {
                for (key, label, w, h) in ASPECT_PRESETS {
                    if ui.selectable_label(cur == key, label).clicked() && cur != key {
                        self.project.set_canvas(w, h, key);
                        self.checkpoint();
                    }
                }
            });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading("分鏡 Shots");
            ui.label(
                RichText::new(format!("{} 個 · {:.1}s", self.project.shots.len(), self.project.total_duration()))
                    .weak(),
            );
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("＋ 新增").on_hover_text("在目前鏡頭後新增空白鏡頭（沿用場景設定）").clicked()
            {
                self.add_shot(false);
            }
            if ui.button("🗐 複製").on_hover_text("複製目前鏡頭（含角色與道具）").clicked() {
                self.add_shot(true);
            }
            if ui
                .add_enabled(self.project.shots.len() > 1, egui::Button::new("🗑"))
                .on_hover_text("刪除目前鏡頭")
                .clicked()
            {
                self.delete_shot();
            }
            if ui.button("⬆").on_hover_text("上移").clicked() {
                self.move_shot(-1);
            }
            if ui.button("⬇").on_hover_text("下移").clicked() {
                self.move_shot(1);
            }
        });
        ui.add_space(2.0);
        let (cw, ch) = (self.project.canvas.width as f32, self.project.canvas.height as f32);
        let w = ui.available_width() - 4.0;
        let th = w * ch / cw;
        let opts = DraftOptions {
            labels: false,
            motion: true,
            dialogue: false,
            header: false,
            ghosts: false,
            prop_labels: false,
        };
        let starts = self.project.shot_starts();
        let ctx = ui.ctx().clone();
        let mut clicked = None;
        #[allow(clippy::needless_range_loop)]
        for i in 0..self.project.shots.len() {
            let (rect, resp) = ui.allocate_exact_size(vec2(w, th + 20.0), Sense::click());
            let img = egui::Rect::from_min_size(rect.min, vec2(w, th));
            let painter = ui.painter_at(img);
            painter.rect_filled(img, 3.0, Color32::WHITE);
            let items = shot_items(&self.project, &self.project.shots[i], &opts);
            let view = View { origin: img.min, scale: w / cw };
            paint_items(&painter, &ctx, &mut self.figs, &items, view, ch, 1000.0);
            let selected = i == self.shot;
            let border = if selected {
                Stroke::new(2.5, ACCENT)
            } else if resp.hovered() {
                Stroke::new(1.5, Color32::from_gray(160))
            } else {
                Stroke::new(1.0, Color32::from_gray(70))
            };
            ui.painter().rect_stroke(img, 3.0, border, StrokeKind::Outside);
            let s = &self.project.shots[i];
            ui.painter().text(
                egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0),
                egui::Align2::LEFT_BOTTOM,
                format!("#{} {}", i + 1, s.name),
                egui::FontId::proportional(13.0),
                if selected { ACCENT } else { Color32::from_gray(210) },
            );
            ui.painter().text(
                egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("{:.1}s @{:.1}", s.duration, starts[i]),
                egui::FontId::proportional(11.0),
                Color32::from_gray(140),
            );
            if resp.clicked() {
                clicked = Some(i);
            }
            ui.add_space(4.0);
        }
        if let Some(i) = clicked {
            self.select_shot(i);
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.heading("角色表 Cast");
            if ui.small_button("＋ 新增").clicked() {
                let id = self.add_cast();
                self.cast_edit = Some(id);
            }
        });
        let mut remove = None;
        for ci in 0..self.project.cast.len() {
            let c = self.project.cast[ci].clone();
            let editing = self.cast_edit.as_deref() == Some(c.id.as_str());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                ui.painter().circle_filled(
                    r.center(),
                    6.0,
                    Color32::from_rgb(c.color.0[0], c.color.0[1], c.color.0[2]),
                );
                let used = self
                    .project
                    .shots
                    .iter()
                    .map(|s| s.actors.iter().filter(|a| a.cast == c.id).count())
                    .sum::<usize>();
                if ui
                    .selectable_label(editing, format!("{}  ({}, {:.2}m)", c.name, c.body_type.zh(), c.height))
                    .on_hover_text(format!("{} · 出場 {used} 次\n{}", c.id, c.description))
                    .clicked()
                {
                    self.cast_edit = if editing { None } else { Some(c.id.clone()) };
                }
            });
            if editing {
                let mut del = false;
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    let m = &mut self.project.cast[ci];
                    egui::Grid::new(("cast", ci)).num_columns(2).show(ui, |ui| {
                        ui.label("名稱");
                        ui.text_edit_singleline(&mut m.name);
                        ui.end_row();
                        ui.label("顏色");
                        ui.color_edit_button_srgb(&mut m.color.0);
                        ui.end_row();
                        ui.label("身高");
                        ui.add(egui::DragValue::new(&mut m.height).range(1.0..=2.4).speed(0.01).suffix(" m"));
                        ui.end_row();
                        ui.label("體型");
                        egui::ComboBox::from_id_salt(("bt", ci)).selected_text(m.body_type.zh()).show_ui(ui, |ui| {
                            for b in BodyType::ALL {
                                ui.selectable_value(&mut m.body_type, b, format!("{} {}", b.zh(), b.label()));
                            }
                        });
                        ui.end_row();
                        ui.label("頭身");
                        ui.add(egui::Slider::new(&mut m.head_size, 0.6..=1.6).text("頭部大小"));
                        ui.end_row();
                    });
                    ui.label("角色描述");
                    multiline(ui, &mut m.description, 2, "年齡、外型、個性、服裝…");
                    if ui.small_button("🗑 刪除此角色").clicked() {
                        del = true;
                    }
                });
                if del {
                    remove = Some(ci);
                }
            }
        }
        if let Some(ci) = remove {
            let id = self.project.cast.remove(ci).id;
            for s in &mut self.project.shots {
                s.actors.retain(|a| a.cast != id);
            }
            self.cast_edit = None;
            self.checkpoint();
        }
    }

    // ------------------------------------------------------------------ right

    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        match self.sel.clone() {
            Sel::Actor(id) => self.actor_inspector(ui, &id),
            Sel::Prop(id) => self.prop_inspector(ui, &id),
            Sel::None => {}
        }
        let open_default = self.sel == Sel::None;
        ui.separator();
        egui::CollapsingHeader::new(
            RichText::new(format!("🎬 鏡頭 #{} 設定（場景 / 鏡頭 / 旁白）", self.shot + 1)).strong(),
        )
        .id_salt(("shot_settings", open_default))
        .default_open(open_default)
        .show(ui, |ui| self.shot_inspector(ui));
    }

    fn shot_inspector(&mut self, ui: &mut egui::Ui) {
        let shot = &mut self.project.shots[self.shot];
        egui::Grid::new("shot_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("鏡頭名稱");
            ui.text_edit_singleline(&mut shot.name);
            ui.end_row();
            ui.label("長度");
            ui.add(egui::DragValue::new(&mut shot.duration).range(0.1..=600.0).speed(0.1).suffix(" 秒"));
            ui.end_row();
        });
        section(ui, "場景 Scene");
        egui::Grid::new("scene_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("地點");
            ui.add(egui::TextEdit::singleline(&mut shot.setting.location).hint_text("例：街角咖啡店門口"));
            ui.end_row();
            ui.label("內 / 外景");
            enum_combo!(ui, "inout", &mut shot.setting.in_out, InOut);
            ui.end_row();
            ui.label("時間");
            enum_combo!(ui, "tod", &mut shot.setting.time_of_day, TimeOfDay);
            ui.end_row();
            ui.label("天氣 / 光線");
            ui.add(egui::TextEdit::singleline(&mut shot.setting.weather).hint_text("例：晴朗、夕陽斜照"));
            ui.end_row();
        });
        ui.label("環境描述");
        multiline(ui, &mut shot.setting.environment, 3, "背景、氣氛、聲音、陳設…");
        section(ui, "鏡頭 Camera");
        egui::Grid::new("cam_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("景別");
            enum_combo!(ui, "size", &mut shot.camera.size, ShotSize);
            ui.end_row();
            ui.label("角度");
            enum_combo!(ui, "angle", &mut shot.camera.angle, CameraAngle);
            ui.end_row();
            ui.label("運鏡");
            enum_combo!(ui, "move", &mut shot.camera.movement, CameraMove);
            ui.end_row();
            ui.label("地平線");
            ui.add(egui::Slider::new(&mut shot.horizon, 0.05..=0.95).text("畫面高度比例"));
            ui.end_row();
            ui.label("地面格線");
            ui.checkbox(&mut shot.floor_grid, "顯示透視格線");
            ui.end_row();
        });
        ui.label("鏡頭備註");
        single(ui, &mut shot.camera.notes, "鏡頭焦段、構圖說明…");
        section(ui, "旁白 / 敘述 Narration");
        multiline(ui, &mut shot.narration, 3, "旁白（會寫入 Markdown / HTML；留空則自動產生敘述）");
        ui.checkbox(&mut shot.narration_box, "在草稿圖左上角顯示旁白框（漫畫說明框）");
        ui.label("導演備註");
        multiline(ui, &mut shot.notes, 2, "其他備註");
        ui.add_space(4.0);
        if ui.button("📝 預覽本鏡頭的自動敘述").clicked() {
            self.show_text = true;
        }
    }

    fn actor_inspector(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(ai) = self.actor_index(id) else { return };
        let cast_list: Vec<(String, String)> =
            self.project.cast.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
        let cast = self.project.cast_of(&self.cur().actors[ai]);
        let depth_scale = self.project.depth_scale(self.cur(), self.cur().actors[ai].y);
        let canvas_w = self.project.canvas.width as f32;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            ui.painter().circle_filled(
                r.center(),
                7.0,
                Color32::from_rgb(cast.color.0[0], cast.color.0[1], cast.color.0[2]),
            );
            ui.heading(format!("角色：{}", cast.name));
            ui.label(RichText::new(id).weak().small());
        });
        let mut pose_click: Option<String> = None;
        let mut mirror = false;
        let mut reset = false;
        {
            let a = &mut self.project.shots[self.shot].actors[ai];
            egui::Grid::new("actor_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("演員");
                egui::ComboBox::from_id_salt("cast_pick").selected_text(cast.name.clone()).show_ui(ui, |ui| {
                    for (cid, name) in &cast_list {
                        ui.selectable_value(&mut a.cast, cid.clone(), name);
                    }
                });
                ui.end_row();
                ui.label("位置 X / Y");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut a.x).speed(2.0).prefix("x "));
                    ui.add(egui::DragValue::new(&mut a.y).speed(2.0).prefix("y "));
                });
                ui.end_row();
                ui.label("朝向");
                ui.add(egui::Slider::new(&mut a.facing, -180.0..=180.0).suffix("°").step_by(5.0));
                ui.end_row();
                ui.label("");
                ui.horizontal_wrapped(|ui| {
                    for (v, l) in [
                        (-90.0, "←左"),
                        (-45.0, "↙左前"),
                        (0.0, "正面"),
                        (45.0, "右前↘"),
                        (90.0, "右→"),
                        (180.0, "背面"),
                    ] {
                        if ui.small_button(l).clicked() {
                            a.facing = v;
                        }
                    }
                });
                ui.end_row();
                ui.label("大小");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut a.scale).range(0.05..=8.0).speed(0.01).prefix("× "));
                    if ui
                        .small_button("依景深")
                        .on_hover_text(format!("依地面位置的透視比例 × {depth_scale:.2}"))
                        .clicked()
                    {
                        a.scale = depth_scale;
                    }
                });
                ui.end_row();
            });

            section(ui, "姿勢 Pose");
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(pose_label(&a.pose.preset)).strong());
                if a.pose.edited {
                    ui.label(RichText::new("（已手動調整）").color(ACCENT).small());
                }
            });
            ui.horizontal(|ui| {
                if ui.button("⇋ 左右鏡像").clicked() {
                    mirror = true;
                }
                if ui.button("↺ 重設姿勢").on_hover_text("回到姿勢庫的原始姿勢").clicked() {
                    reset = true;
                }
                ui.label("離地");
                ui.add(egui::DragValue::new(&mut a.pose.lift).range(0.0..=2.0).speed(0.01).suffix(" m"));
            });
            let cur_key = a.pose.preset.clone();
            if let Some(k) = pose_grid(ui, "action_grid", &self.action_thumbs, &cur_key) {
                pose_click = Some(k);
            }
            egui::CollapsingHeader::new("地面姿勢（rust-pose-studio 25 種）").show(ui, |ui| {
                if let Some(k) = pose_grid(ui, "floor_grid", &self.floor_thumbs, &cur_key) {
                    pose_click = Some(k);
                }
            });
        }
        if let Some(k) = pose_click {
            self.apply_pose(id, &k);
        }
        if mirror || reset {
            let props = cast.proportions();
            let skel = Skeleton::new(props);
            let a = &mut self.project.shots[self.shot].actors[ai];
            if reset {
                let key = a.pose.preset.clone();
                if let Some((p, lift)) = rss::mannequin::actions::library_pose(&skel, &key) {
                    a.pose = PoseData::from_pose(&key, &p, lift);
                }
            } else {
                let mut p = Pose::rest(&skel);
                p.rot = a.pose.rotations();
                let m = p.mirrored();
                for j in Joint::ALL {
                    a.pose.set_joint(j, m.get(j));
                }
                a.pose.edited = true;
            }
            self.checkpoint();
        }

        // joint fine-tuning
        egui::CollapsingHeader::new("關節微調 Joint angles").default_open(self.sel_joint.is_some()).show(ui, |ui| {
            let mut sj = self.sel_joint.unwrap_or(Joint::Head);
            egui::ComboBox::from_id_salt("joint_pick").selected_text(joint_zh(sj)).show_ui(ui, |ui| {
                for j in Joint::ALL {
                    ui.selectable_value(&mut sj, j, joint_zh(j));
                }
            });
            self.sel_joint = Some(sj);
            let a = &mut self.project.shots[self.shot].actors[ai];
            let mut e = a.pose.rotations()[sj.index()];
            let mut changed = false;
            for (i, axis) in ["X 前後彎", "Y 扭轉", "Z 側彎"].iter().enumerate() {
                changed |= ui.add(egui::Slider::new(&mut e[i], -180.0..=180.0).suffix("°").text(*axis)).changed();
            }
            if changed {
                a.pose.set_joint(sj, e);
                a.pose.edited = true;
            }
            ui.label(RichText::new("提示：也可直接在畫布上拖曳關節點；綠色菱形為手腕 / 腳踝 IK。").small().weak());
        });

        let a = &mut self.project.shots[self.shot].actors[ai];
        section(ui, "表演 Acting");
        ui.label("動作描述");
        multiline(ui, &mut a.action, 2, "例：推門走進店裡，四處張望");
        ui.label("表情 / 情緒");
        single(ui, &mut a.expression, "例：好奇、微笑");
        ui.label("對白");
        multiline(ui, &mut a.dialogue, 2, "台詞（草稿圖顯示為漫畫對白框，可在畫布上拖曳）");
        ui.horizontal_wrapped(|ui| {
            ui.label("對白框");
            for &st in BubbleStyle::ALL {
                let icon = match st {
                    BubbleStyle::Speech => "💬",
                    BubbleStyle::Thought => "☁",
                    BubbleStyle::Shout => "💥",
                    BubbleStyle::Whisper => "┄",
                    BubbleStyle::Narration => "□",
                };
                ui.selectable_value(&mut a.bubble.style, st, format!("{icon} {}", st.zh()))
                    .on_hover_text(st.shape_zh());
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut a.bubble.vertical, "直書").on_hover_text("由上而下、由右至左排列（中文直排）");
            ui.label("字級");
            ui.add(egui::DragValue::new(&mut a.bubble.text_scale).range(0.3..=4.0).speed(0.02).prefix("× "));
            ui.label(if a.bubble.vertical { "每行高" } else { "每行寬" });
            ui.add(egui::DragValue::new(&mut a.bubble.wrap).range(0.0..=4000.0).speed(4.0).suffix(" px"))
                .on_hover_text("0 = 自動");
        });
        ui.horizontal(|ui| {
            ui.label("位置偏移");
            ui.add(egui::DragValue::new(&mut a.bubble.offset[0]).speed(2.0).prefix("x "));
            ui.add(egui::DragValue::new(&mut a.bubble.offset[1]).speed(2.0).prefix("y "));
            if ui.small_button("↺ 自動位置").clicked() {
                a.bubble.offset = [0.0, 0.0];
            }
        });

        section(ui, "走位 Movement");
        let was = a.movement.enabled;
        ui.checkbox(&mut a.movement.enabled, "這個鏡頭中會移動（畫出走位箭頭）");
        if a.movement.enabled && !was && a.movement.path.is_empty() {
            let dx = if a.x > canvas_w * 0.6 { -350.0 } else { 350.0 };
            a.movement.path.push([a.x + dx, a.y]);
        }
        if a.movement.enabled {
            egui::Grid::new("move_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("移動方式");
                enum_combo!(ui, "mstyle", &mut a.movement.style, MoveStyle);
                ui.end_row();
                ui.label("時間");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut a.movement.start).range(0.0..=600.0).speed(0.1).suffix(" s"));
                    ui.label("→");
                    ui.add(egui::DragValue::new(&mut a.movement.end).range(0.0..=600.0).speed(0.1).suffix(" s"));
                });
                ui.end_row();
                ui.label("終點姿勢");
                let cur = if a.movement.end_pose.is_empty() {
                    "（與起點相同）".to_string()
                } else {
                    pose_label(&a.movement.end_pose)
                };
                egui::ComboBox::from_id_salt("end_pose").selected_text(cur).width(210.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut a.movement.end_pose, String::new(), "（與起點相同）");
                    for p in ACTIONS.iter() {
                        ui.selectable_value(&mut a.movement.end_pose, p.key.to_string(), format!("{} {}", p.zh, p.en));
                    }
                    for i in 1..=25 {
                        let k = format!("floor:{i}");
                        let l = pose_label(&k);
                        ui.selectable_value(&mut a.movement.end_pose, k, l);
                    }
                });
                ui.end_row();
                ui.label("終點朝向");
                ui.horizontal(|ui| {
                    let mut has = a.movement.end_facing.is_some();
                    if ui.checkbox(&mut has, "指定").changed() {
                        a.movement.end_facing = if has { Some(a.facing) } else { None };
                    }
                    if let Some(f) = &mut a.movement.end_facing {
                        ui.add(egui::Slider::new(f, -180.0..=180.0).suffix("°").step_by(5.0));
                    } else {
                        ui.label(RichText::new("自動（走 / 跑時面向前進方向）").weak().small());
                    }
                });
                ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut a.movement.ghost, "終點殘影");
                ui.checkbox(&mut a.movement.depth_scale, "終點依景深縮放");
            });
            ui.label("移動描述");
            multiline(ui, &mut a.movement.description, 2, "例：穿過店內走到櫃台前（留空則自動描述方向與距離）");
            ui.label(RichText::new("路徑點（canvas px）· 在畫布上拖曳，Shift+點擊加入").small().weak());
            let mut remove = None;
            let n = a.movement.path.len();
            for (i, q) in a.movement.path.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let last = i + 1 == n;
                    ui.label(if last { "終點".to_string() } else { format!("{}", i + 1) });
                    ui.add(egui::DragValue::new(&mut q[0]).speed(2.0).prefix("x "));
                    ui.add(egui::DragValue::new(&mut q[1]).speed(2.0).prefix("y "));
                    if ui.small_button("✖").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                a.movement.path.remove(i);
                if a.movement.path.is_empty() {
                    a.movement.enabled = false;
                }
            }
            ui.horizontal(|ui| {
                if ui.button("＋ 中途點").on_hover_text("在終點前插入一個中途點").clicked() {
                    let pts = a.path_points();
                    let n = pts.len();
                    if n >= 2 {
                        let (p0, p1) = (pts[n - 2], pts[n - 1]);
                        let mid = [(p0[0] + p1[0]) / 2.0, (p0[1] + p1[1]) / 2.0 - 40.0];
                        let at = a.movement.path.len() - 1;
                        a.movement.path.insert(at, mid);
                    }
                }
                if ui.button("清除路徑").clicked() {
                    a.movement.path.clear();
                    a.movement.enabled = false;
                }
            });
        }
    }

    fn prop_inspector(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(pi) = self.prop_index(id) else { return };
        let base_ppm = self.project.base_ppm();
        let ds = {
            let shot = self.cur();
            self.project.depth_scale(shot, shot.props[pi].y)
        };
        let p = &mut self.project.shots[self.shot].props[pi];
        ui.heading(format!("道具：{}", p.display_name()));
        ui.label(RichText::new(id).weak().small());
        egui::Grid::new("prop_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("種類");
            enum_combo!(ui, "pkind", &mut p.kind, PropKind);
            ui.end_row();
            ui.label("名稱");
            ui.add(egui::TextEdit::singleline(&mut p.label).hint_text(p.kind.zh()));
            ui.end_row();
            ui.label("位置 X / Y");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut p.x).speed(2.0).prefix("x "));
                ui.add(egui::DragValue::new(&mut p.y).speed(2.0).prefix("y "));
            });
            ui.end_row();
            ui.label("寬 / 高");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut p.w).range(4.0..=8000.0).speed(2.0).prefix("w "));
                ui.add(egui::DragValue::new(&mut p.h).range(4.0..=8000.0).speed(2.0).prefix("h "));
            });
            ui.end_row();
            ui.label("");
            if ui.small_button("依景深重設大小").on_hover_text("依種類的實際尺寸與地面位置的透視比例").clicked()
            {
                let (w, h) = p.kind.default_size_m();
                p.w = w * base_ppm * ds;
                p.h = h * base_ppm * ds;
            }
            ui.end_row();
            ui.label("離地高度");
            ui.add(egui::DragValue::new(&mut p.elevation).range(0.0..=4000.0).speed(1.0).suffix(" px"))
                .on_hover_text("例：桌上的杯子 = 桌面高度");
            ui.end_row();
            ui.label("方向");
            ui.checkbox(&mut p.flip, "左右翻轉");
            ui.end_row();
        });
        ui.label("備註");
        multiline(ui, &mut p.notes, 2, "材質、顏色、用途…");
    }
}
