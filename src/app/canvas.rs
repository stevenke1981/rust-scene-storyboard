//! Central storyboard canvas: draft preview, selection, dragging characters / props /
//! movement paths, and posing joints with handles (IK for wrists and ankles).

use super::paint::{View, paint_items};
use super::{ACCENT, App, Drag, Sel};
use eframe::egui::{self, Color32, CursorIcon, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use rss::draw::{Item, P, prop_prims, shot_items};
use rss::figure::FigureSpec;
use rss::mannequin::posing::{aim_joint, ik_limb};
use rss::mannequin::skeleton::Joint;
use rss::model::{Project, Prop, Shot};

const HANDLE_R: f32 = 5.5;
const PICK_R: f32 = 9.0;
const IK_HANDLE: Color32 = Color32::from_rgb(40, 170, 90);
const FK_HANDLE: Color32 = Color32::from_rgb(40, 120, 220);
const PELVIS_HANDLE: Color32 = Color32::from_rgb(150, 60, 200);

/// Traditional Chinese joint names.
pub fn joint_zh(j: Joint) -> &'static str {
    use Joint::*;
    match j {
        Pelvis => "骨盆（整體位置）",
        Waist => "腰",
        Chest => "胸",
        Neck => "脖子",
        Head => "頭",
        UpperArmL => "左上臂",
        ForearmL => "左前臂（手腕 IK）",
        HandL => "左手",
        UpperArmR => "右上臂",
        ForearmR => "右前臂（手腕 IK）",
        HandR => "右手",
        ThighL => "左大腿",
        ShinL => "左小腿（腳踝 IK）",
        FootL => "左腳",
        ThighR => "右大腿",
        ShinR => "右小腿（腳踝 IK）",
        FootR => "右腳",
    }
}

/// Canvas bounds `[x0, y0, x1, y1]` of a prop drawing (labels excluded).
pub fn prop_bounds(project: &Project, shot: &Shot, p: &Prop) -> [f32; 4] {
    let u = project.canvas.height as f32 / 1080.0;
    let mut b = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    let mut add = |q: [f32; 2]| b = [b[0].min(q[0]), b[1].min(q[1]), b[2].max(q[0]), b[3].max(q[1])];
    for prim in prop_prims(p, shot.camera.angle.pitch(), u) {
        match prim {
            P::Poly { pts, .. } | P::Shape { pts, .. } => pts.iter().for_each(|q| add(*q)),
            P::Ellipse { c, r, .. } => {
                add([c[0] - r[0], c[1] - r[1]]);
                add([c[0] + r[0], c[1] + r[1]]);
            }
            P::Text { .. } => {}
        }
    }
    if !b[0].is_finite() {
        return [p.x - p.w / 2.0, p.y - p.h, p.x + p.w / 2.0, p.y];
    }
    b
}

enum Hit {
    Joint(Joint, f32, [f32; 2]),
    PathPoint(usize),
    PropResize,
    Bubble(String),
    Actor(String),
    Prop(String),
}

fn contains(b: [f32; 4], p: [f32; 2], pad: f32) -> bool {
    p[0] >= b[0] - pad && p[0] <= b[2] + pad && p[1] >= b[1] - pad && p[1] <= b[3] + pad
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

impl App {
    fn view_for(&self, avail: Rect) -> View {
        let (cw, ch) = (self.project.canvas.width as f32, self.project.canvas.height as f32);
        let m = 24.0;
        let fit = ((avail.width() - 2.0 * m) / cw).min((avail.height() - 2.0 * m) / ch).max(0.01);
        let scale = fit * self.zoom;
        let size = vec2(cw * scale, ch * scale);
        View { origin: avail.center() - size / 2.0 + self.pan, scale }
    }

    fn sel_actor_spec(&self) -> Option<FigureSpec> {
        let Sel::Actor(id) = &self.sel else { return None };
        let a = self.cur().actor(id)?;
        Some(FigureSpec::of_actor(&self.project, self.cur(), a))
    }

    /// What is under canvas point `p` (canvas px); `r` = pick radius in canvas px.
    fn hit(&self, p: [f32; 2], r: f32) -> Option<Hit> {
        let shot = self.cur();
        if let Sel::Actor(id) = &self.sel
            && let Some(a) = shot.actor(id)
        {
            if self.settings.show_handles
                && let Some(spec) = self.sel_actor_spec()
            {
                let best = spec
                    .handles()
                    .into_iter()
                    .filter(|(_, h, _)| dist(*h, p) <= r)
                    .min_by(|a, b| (dist(a.1, p) + a.2).total_cmp(&(dist(b.1, p) + b.2)));
                if let Some((j, h, d)) = best {
                    return Some(Hit::Joint(j, d, h));
                }
            }
            if a.movement.enabled
                && let Some(i) = a.movement.path.iter().rposition(|q| dist(*q, p) <= r)
            {
                return Some(Hit::PathPoint(i));
            }
        }
        if let Sel::Prop(id) = &self.sel
            && let Some(pr) = shot.props.iter().find(|x| &x.id == id)
        {
            let b = prop_bounds(&self.project, shot, pr);
            if dist([b[2], b[1]], p) <= r * 1.2 {
                return Some(Hit::PropResize);
            }
        }
        // Dialogue balloons are drawn above everything else.
        if self.export_opts.dialogue {
            for a in shot.actors.iter().rev() {
                if let Some(b) = rss::bubble::actor_bubble(&self.project, shot, a)
                    && contains(b.rect, p, 0.0)
                {
                    return Some(Hit::Bubble(a.id.clone()));
                }
            }
        }
        // Scene content, front (larger y) first.
        let mut z: Vec<(f32, Hit)> = shot.actors.iter().map(|a| (a.y, Hit::Actor(a.id.clone()))).collect();
        z.extend(shot.props.iter().map(|x| (x.y, Hit::Prop(x.id.clone()))));
        z.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, h) in z {
            match &h {
                Hit::Actor(id) => {
                    let Some(a) = shot.actor(id) else { continue };
                    let cached = self.pick_keys.iter().find(|(aid, _)| aid == id).and_then(|(_, k)| self.figs.peek(k));
                    let inside = match cached {
                        Some(c) => {
                            let v = c.draw.frame.from_px(p);
                            c.draw.list.pick(v[0], v[1]).is_some()
                        }
                        None => contains(FigureSpec::of_actor(&self.project, shot, a).bounds(), p, 0.0),
                    };
                    if inside {
                        return Some(h);
                    }
                }
                Hit::Prop(id) => {
                    let Some(pr) = shot.props.iter().find(|x| &x.id == id) else { continue };
                    if contains(prop_bounds(&self.project, shot, pr), p, r * 0.3) {
                        return Some(h);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn drag_joint(&mut self, id: &str, joint: Joint, depth: f32, target_px: [f32; 2]) {
        let Some(i) = self.actor_index(id) else { return };
        let spec = FigureSpec::of_actor(&self.project, self.cur(), &self.cur().actors[i]);
        let skel = spec.skeleton();
        let mut pose = spec.pose(&skel);
        let target = spec.unproject(target_px, depth);
        if joint.is_hinge() {
            ik_limb(&skel, &mut pose, joint, target, None);
        } else {
            aim_joint(&skel, &mut pose, joint, target);
        }
        if !pose.is_finite() {
            return;
        }
        let a = &mut self.cur_mut().actors[i];
        for j in Joint::ALL {
            a.pose.set_joint(j, pose.get(j));
        }
        a.pose.edited = true;
    }

    fn depth_ratio(&self, y0: f32, y1: f32) -> Option<f32> {
        let hz = self.cur().horizon * self.project.canvas.height as f32;
        if !self.settings.auto_depth || y0 < hz + 8.0 || y1 < hz + 8.0 {
            return None;
        }
        Some((y1 - hz) / (y0 - hz))
    }

    fn apply_drag(&mut self, p: [f32; 2], delta: egui::Vec2) {
        let Some(drag) = self.drag.clone() else { return };
        match drag {
            Drag::Actor { id, start, p0, scale0, path0 } => {
                let (dx, dy) = (p[0] - p0[0], p[1] - p0[1]);
                let ratio = self.depth_ratio(start[1], start[1] + dy);
                if let Some(a) = self.actor_mut(&id) {
                    a.x = start[0] + dx;
                    a.y = start[1] + dy;
                    if let Some(r) = ratio {
                        a.scale = (scale0 * r).clamp(0.05, 8.0);
                    }
                    for (q, q0) in a.movement.path.iter_mut().zip(&path0) {
                        *q = [q0[0] + dx, q0[1] + dy];
                    }
                }
            }
            Drag::Prop { id, start, p0, size0 } => {
                let (dx, dy) = (p[0] - p0[0], p[1] - p0[1]);
                let ratio = self.depth_ratio(start[1], start[1] + dy);
                if let Some(pr) = self.prop_mut(&id) {
                    pr.x = start[0] + dx;
                    pr.y = start[1] + dy;
                    if let Some(r) = ratio {
                        pr.w = (size0[0] * r).max(4.0);
                        pr.h = (size0[1] * r).max(4.0);
                    }
                }
            }
            Drag::PropResize { id, size0, p0 } => {
                let (dx, dy) = (p[0] - p0[0], p[1] - p0[1]);
                if let Some(pr) = self.prop_mut(&id) {
                    pr.w = (size0[0] + 2.0 * dx).max(8.0);
                    pr.h = (size0[1] - dy).max(8.0);
                }
            }
            Drag::Bubble { id, off0, p0 } => {
                if let Some(a) = self.actor_mut(&id) {
                    a.bubble.offset = [off0[0] + p[0] - p0[0], off0[1] + p[1] - p0[1]];
                }
            }
            Drag::PathPoint { id, idx } => {
                if let Some(a) = self.actor_mut(&id)
                    && let Some(q) = a.movement.path.get_mut(idx)
                {
                    *q = p;
                }
            }
            Drag::Joint { id, joint, depth, offset } => {
                if joint == Joint::Pelvis {
                    return;
                }
                self.drag_joint(&id, joint, depth, [p[0] + offset[0], p[1] + offset[1]]);
            }
            Drag::Pan => self.pan += delta,
        }
    }

    pub(super) fn canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let view = self.view_for(rect);
        let (cw, ch) = (self.project.canvas.width as f32, self.project.canvas.height as f32);
        let pick_r = PICK_R / view.scale;
        let shift = ui.input(|i| i.modifiers.shift);

        // ---------------------------------------------------------------- interaction
        let pointer = resp.hover_pos().or(resp.interact_pointer_pos());
        let hovered =
            if self.drag.is_none() { pointer.and_then(|p| self.hit(view.to_canvas(p), pick_r)) } else { None };

        if resp.drag_started_by(egui::PointerButton::Primary) {
            let start = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()).unwrap_or(rect.center());
            let sp = view.to_canvas(start);
            self.checkpoint();
            self.drag = match self.hit(sp, pick_r) {
                Some(Hit::Joint(j, depth, h)) => {
                    let Sel::Actor(id) = self.sel.clone() else { unreachable!() };
                    self.sel_joint = Some(j);
                    if j == Joint::Pelvis {
                        self.actor_drag(&id, sp)
                    } else {
                        Some(Drag::Joint { id, joint: j, depth, offset: [h[0] - sp[0], h[1] - sp[1]] })
                    }
                }
                Some(Hit::PathPoint(idx)) => match &self.sel {
                    Sel::Actor(id) => Some(Drag::PathPoint { id: id.clone(), idx }),
                    _ => None,
                },
                Some(Hit::PropResize) => match self.sel.clone() {
                    Sel::Prop(id) => self.cur().props.iter().find(|x| x.id == id).map(|pr| Drag::PropResize {
                        id,
                        size0: [pr.w, pr.h],
                        p0: sp,
                    }),
                    _ => None,
                },
                Some(Hit::Actor(id)) => {
                    if self.sel != Sel::Actor(id.clone()) {
                        self.sel_joint = None;
                    }
                    self.sel = Sel::Actor(id.clone());
                    self.actor_drag(&id, sp)
                }
                Some(Hit::Bubble(id)) => {
                    if self.sel != Sel::Actor(id.clone()) {
                        self.sel_joint = None;
                    }
                    self.sel = Sel::Actor(id.clone());
                    self.cur().actor(&id).map(|a| Drag::Bubble { id: id.clone(), off0: a.bubble.offset, p0: sp })
                }
                Some(Hit::Prop(id)) => {
                    self.sel = Sel::Prop(id.clone());
                    self.cur().props.iter().find(|x| x.id == id).map(|pr| Drag::Prop {
                        id: id.clone(),
                        start: [pr.x, pr.y],
                        p0: sp,
                        size0: [pr.w, pr.h],
                    })
                }
                None => Some(Drag::Pan),
            };
        } else if resp.drag_started_by(egui::PointerButton::Secondary)
            || resp.drag_started_by(egui::PointerButton::Middle)
        {
            self.drag = Some(Drag::Pan);
        }

        if self.drag.is_some() {
            let delta = ui.input(|i| i.pointer.delta());
            if let Some(p) = resp.interact_pointer_pos().or(pointer) {
                self.apply_drag(view.to_canvas(p), delta);
            }
            if !ui.input(|i| i.pointer.any_down()) || resp.drag_stopped() {
                self.drag = None;
            }
        }

        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let cp = view.to_canvas(p);
            let hit = self.hit(cp, pick_r);
            match (hit, shift, self.sel.clone()) {
                (None, true, Sel::Actor(id)) => {
                    if let Some(a) = self.actor_mut(&id) {
                        if !a.movement.enabled {
                            a.movement.enabled = true;
                            a.movement.path.clear();
                        }
                        a.movement.path.push(cp);
                    }
                    self.checkpoint();
                    self.set_status(true, "已加入走位路徑點（最後一點為終點）");
                }
                (Some(Hit::Joint(j, ..)), _, _) => self.sel_joint = Some(j),
                (Some(Hit::Actor(id)) | Some(Hit::Bubble(id)), _, _) => {
                    if self.sel != Sel::Actor(id.clone()) {
                        self.sel_joint = None;
                    }
                    self.sel = Sel::Actor(id);
                }
                (Some(Hit::Prop(id)), _, _) => self.sel = Sel::Prop(id),
                (Some(_), _, _) => {}
                (None, _, _) => {
                    self.sel = Sel::None;
                    self.sel_joint = None;
                }
            }
        }
        if resp.double_clicked()
            && let Some(p) = resp.interact_pointer_pos()
            && let Some(Hit::PathPoint(i)) = self.hit(view.to_canvas(p), pick_r)
            && let Sel::Actor(id) = self.sel.clone()
            && let Some(a) = self.actor_mut(&id)
        {
            a.movement.path.remove(i);
            if a.movement.path.is_empty() {
                a.movement.enabled = false;
            }
            self.checkpoint();
        }

        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(ptr) = pointer
            {
                let c = view.to_canvas(ptr);
                self.zoom = (self.zoom * (scroll * 0.0015).exp()).clamp(0.25, 4.0);
                let v2 = self.view_for(rect);
                // keep the canvas point under the pointer fixed
                self.pan += ptr - v2.pt(c);
            }
        }

        // ---------------------------------------------------------------- drawing
        let view = self.view_for(rect);
        let painter = ui.painter_at(rect);
        let crect = view.rect([0.0, 0.0, cw, ch]);
        painter.rect_filled(crect.translate(vec2(3.0, 4.0)), 2.0, Color32::from_black_alpha(90));
        painter.rect_filled(crect, 0.0, Color32::WHITE);
        let cp = painter.with_clip_rect(crect.intersect(rect));

        let mut items = shot_items(&self.project, self.cur(), &self.export_opts.draft());
        if let (Sel::Actor(id), Some(j)) = (&self.sel, self.sel_joint) {
            for it in &mut items {
                if let Item::Figure(f) = it
                    && &f.actor_id == id
                    && !f.ghost
                {
                    f.highlight = Some(j);
                }
            }
        }
        let ctx = ui.ctx().clone();
        self.pick_keys = paint_items(&cp, &ctx, &mut self.figs, &items, view, ch, 2.0);

        self.overlays(&painter, view, &hovered);

        // hints
        let shot = self.cur();
        painter.text(
            rect.left_top() + vec2(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "#{} {} · {} · {} · {:.1}s",
                self.shot + 1,
                shot.name,
                shot.camera.size.zh(),
                shot.camera.angle.zh(),
                shot.duration
            ),
            egui::FontId::proportional(13.0),
            Color32::from_gray(170),
        );
        let hint = match (&hovered, &self.sel) {
            (Some(Hit::Joint(j, ..)), _) => format!("關節：{}", joint_zh(*j)),
            (Some(Hit::PathPoint(i)), _) => format!("走位點 {}（拖曳移動，雙擊刪除）", i + 1),
            (Some(Hit::PropResize), _) => "拖曳調整道具大小".into(),
            (Some(Hit::Bubble(_)), _) => "拖曳移動對白框（尾巴會自動指向說話者）".into(),
            (_, Sel::Actor(_)) => "拖曳關節調整姿勢 · Shift+點擊空白處加入走位點 · Delete 刪除".into(),
            (_, Sel::Prop(_)) => "拖曳移動道具 · 右上角方塊調整大小".into(),
            _ => "點選角色或道具；右鍵拖曳平移、滾輪縮放".into(),
        };
        painter.text(
            rect.left_bottom() + vec2(10.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            hint,
            egui::FontId::proportional(12.5),
            Color32::from_gray(150),
        );

        if self.drag.is_some() {
            ctx.set_cursor_icon(CursorIcon::Grabbing);
        } else {
            match hovered {
                Some(Hit::PropResize) => ctx.set_cursor_icon(CursorIcon::ResizeNeSw),
                Some(Hit::Bubble(_)) => ctx.set_cursor_icon(CursorIcon::Move),
                Some(_) => ctx.set_cursor_icon(CursorIcon::PointingHand),
                None => {}
            }
        }
    }

    fn actor_drag(&self, id: &str, sp: [f32; 2]) -> Option<Drag> {
        let a = self.cur().actor(id)?;
        Some(Drag::Actor {
            id: id.to_string(),
            start: [a.x, a.y],
            p0: sp,
            scale0: a.scale,
            path0: a.movement.path.clone(),
        })
    }

    fn overlays(&self, painter: &egui::Painter, view: View, hovered: &Option<Hit>) {
        let shot = self.cur();
        match &self.sel {
            Sel::Actor(id) => {
                let Some(a) = shot.actor(id) else { return };
                let spec = FigureSpec::of_actor(&self.project, shot, a);
                let r = view.rect(spec.bounds());
                painter.rect_stroke(r, 3.0, Stroke::new(1.2, ACCENT.gamma_multiply(0.8)), StrokeKind::Outside);
                if self.export_opts.dialogue
                    && let Some(b) = rss::bubble::actor_bubble(&self.project, shot, a)
                {
                    let br = view.rect(b.rect).expand(3.0);
                    let pts = vec![br.left_top(), br.right_top(), br.right_bottom(), br.left_bottom(), br.left_top()];
                    painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.3, ACCENT), 6.0, 4.0));
                }
                // movement path handles
                if a.movement.enabled {
                    for (i, q) in a.movement.path.iter().enumerate() {
                        let p = view.pt(*q);
                        let hov = matches!(hovered, Some(Hit::PathPoint(k)) if *k == i);
                        let last = i + 1 == a.movement.path.len();
                        let rr = if hov { 7.5 } else { 6.0 };
                        painter.circle(p, rr, if last { ACCENT } else { Color32::WHITE }, Stroke::new(2.0, ACCENT));
                        painter.text(
                            p + vec2(9.0, -9.0),
                            egui::Align2::LEFT_BOTTOM,
                            if last { "終點".to_string() } else { format!("{}", i + 1) },
                            egui::FontId::proportional(12.0),
                            ACCENT,
                        );
                    }
                }
                if self.settings.show_handles {
                    let mut hs = spec.handles();
                    hs.sort_by(|a, b| b.2.total_cmp(&a.2));
                    for (j, q, _) in hs {
                        let p = view.pt(q);
                        let hov = matches!(hovered, Some(Hit::Joint(h, ..)) if *h == j);
                        let sel = self.sel_joint == Some(j);
                        let r = if hov || sel { HANDLE_R + 2.0 } else { HANDLE_R };
                        let base = if j == Joint::Pelvis {
                            PELVIS_HANDLE
                        } else if j.is_hinge() {
                            IK_HANDLE
                        } else {
                            FK_HANDLE
                        };
                        let fill = if sel { ACCENT } else { base.gamma_multiply(if hov { 1.0 } else { 0.85 }) };
                        if j.is_hinge() {
                            let pts = vec![
                                p + vec2(0.0, -r - 1.0),
                                p + vec2(r + 1.0, 0.0),
                                p + vec2(0.0, r + 1.0),
                                p + vec2(-r - 1.0, 0.0),
                            ];
                            painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(1.5, Color32::WHITE)));
                        } else {
                            painter.circle(p, r, fill, Stroke::new(1.5, Color32::WHITE));
                        }
                    }
                }
            }
            Sel::Prop(id) => {
                let Some(pr) = shot.props.iter().find(|x| &x.id == id) else { return };
                let b = prop_bounds(&self.project, shot, pr);
                let r = view.rect(b);
                painter.rect_stroke(r, 2.0, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
                let h = pos2(r.right(), r.top());
                let hov = matches!(hovered, Some(Hit::PropResize));
                let s = if hov { 6.0 } else { 5.0 };
                painter.rect_filled(Rect::from_center_size(h, vec2(s * 2.0, s * 2.0)), 1.0, ACCENT);
                painter.circle_filled(view.pt([pr.x, pr.y]), 3.5, ACCENT);
            }
            Sel::None => {}
        }
    }
}
