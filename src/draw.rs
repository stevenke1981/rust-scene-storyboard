//! Backend-independent storyboard drawing.
//!
//! A shot is turned into a list of [`Item`]s in canvas pixel coordinates: 2D vector
//! primitives (background, props, motion arrows, labels, speech bubbles, header)
//! and line-art figures ([`FigureSpec`]). The GUI paints them with egui and the
//! exporter rasterises them with tiny-skia, so the editor and the exported draft
//! PNG look the same (the approach of whitebox-video-storyboard).

use crate::figure::{FigureSpec, end_scale_factor};
use crate::model::{Actor, InOut, Project, Prop, PropKind, Shot, TimeOfDay};
use crate::raster::Fonts;

pub type Rgba = [u8; 4];

pub const INK: Rgba = [24, 24, 28, 255];
pub const PAPER: Rgba = [255, 255, 255, 255];
pub const GUIDE: Rgba = [205, 208, 214, 255];
pub const LABEL_GRAY: Rgba = [96, 98, 104, 255];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke2 {
    pub width: f32,
    pub color: Rgba,
}

pub fn ink(width: f32) -> Option<Stroke2> {
    Some(Stroke2 { width, color: INK })
}

/// A 2D primitive in canvas pixels. Filled polygons are convex.
#[derive(Clone, Debug, PartialEq)]
pub enum P {
    Poly {
        pts: Vec<[f32; 2]>,
        closed: bool,
        fill: Option<Rgba>,
        stroke: Option<Stroke2>,
        dash: bool,
    },
    Ellipse {
        c: [f32; 2],
        r: [f32; 2],
        fill: Option<Rgba>,
        stroke: Option<Stroke2>,
    },
    /// Single-line text; `anchor` is the fraction of the text box placed at `pos`
    /// (`[0, 0]` = top-left, `[0.5, 1]` = bottom-centre). `bg` draws a rounded badge.
    Text {
        pos: [f32; 2],
        text: String,
        size: f32,
        color: Rgba,
        anchor: [f32; 2],
        bg: Option<Rgba>,
    },
}

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Item {
    Prim(P),
    Figure(FigureSpec),
}

/// What to include in a draft.
#[derive(Clone, Debug, PartialEq)]
pub struct DraftOptions {
    pub labels: bool,
    pub motion: bool,
    pub dialogue: bool,
    pub header: bool,
    pub ghosts: bool,
    pub prop_labels: bool,
}

impl Default for DraftOptions {
    fn default() -> Self {
        DraftOptions { labels: true, motion: true, dialogue: true, header: true, ghosts: true, prop_labels: true }
    }
}

fn poly(pts: Vec<[f32; 2]>, fill: Option<Rgba>, stroke: Option<Stroke2>) -> P {
    P::Poly { pts, closed: true, fill, stroke, dash: false }
}

fn line(pts: Vec<[f32; 2]>, stroke: Stroke2) -> P {
    P::Poly { pts, closed: false, fill: None, stroke: Some(stroke), dash: false }
}

fn rect(x: f32, y: f32, w: f32, h: f32, fill: Option<Rgba>, stroke: Option<Stroke2>) -> P {
    poly(vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]], fill, stroke)
}

fn circle(c: [f32; 2], r: f32, fill: Option<Rgba>, stroke: Option<Stroke2>) -> P {
    P::Ellipse { c, r: [r, r], fill, stroke }
}

pub fn text(pos: [f32; 2], s: impl Into<String>, size: f32, color: Rgba, anchor: [f32; 2]) -> P {
    P::Text { pos, text: s.into(), size, color, anchor, bg: None }
}

/// Rounded rectangle as a convex polygon.
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<[f32; 2]> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pts = Vec::with_capacity(28);
    let corners =
        [(x + w - r, y + r, -90.0f32), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, 90.0), (x + r, y + r, 180.0)];
    for (cx, cy, a0) in corners {
        for k in 0..=6 {
            let a = (a0 + 15.0 * k as f32).to_radians();
            pts.push([cx + r * a.cos(), cy + r * a.sin()]);
        }
    }
    pts
}

/// Catmull-Rom smoothing through the points.
pub fn smooth_path(pts: &[[f32; 2]], steps: usize) -> Vec<[f32; 2]> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::new();
    for i in 0..pts.len() - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(pts.len() - 1)];
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * (2.0 * b
                    + (-a + c) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push([f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]);
        }
    }
    out.push(*pts.last().unwrap());
    out
}

fn polyline_length(pts: &[[f32; 2]]) -> f32 {
    pts.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum()
}

/// Point at a fraction of a polyline's length.
pub fn point_along(pts: &[[f32; 2]], frac: f32) -> [f32; 2] {
    let total = polyline_length(pts);
    let mut target = total * frac.clamp(0.0, 1.0);
    for w in pts.windows(2) {
        let l = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
        if l >= target && l > 0.0 {
            let t = target / l;
            return [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t];
        }
        target -= l;
    }
    *pts.last().unwrap_or(&[0.0, 0.0])
}

/// Visible depth of horizontal surfaces for a viewing pitch (0 = edge-on).
pub fn top_factor(pitch_deg: f32) -> f32 {
    (pitch_deg.to_radians().sin() * 1.6 + 0.25).clamp(0.12, 1.0)
}

/// Circled number ①②… for motion labels.
pub fn circled(n: usize) -> String {
    const C: [char; 20] =
        ['①', '②', '③', '④', '⑤', '⑥', '⑦', '⑧', '⑨', '⑩', '⑪', '⑫', '⑬', '⑭', '⑮', '⑯', '⑰', '⑱', '⑲', '⑳'];
    C.get(n.wrapping_sub(1)).map(|c| c.to_string()).unwrap_or_else(|| format!("({n})"))
}

// ---------------------------------------------------------------- props

/// A 3/4 box inside the bounding rectangle whose bottom-centre is (cx, by).
/// `d` is the depth offset of the back face (dx, dy with dy <= 0).
fn box3(out: &mut Vec<P>, cx: f32, by: f32, w: f32, h: f32, d: [f32; 2], lw: f32) -> [f32; 4] {
    let fw = (w - d[0].abs()).max(1.0);
    let fh = (h - d[1].abs()).max(1.0);
    let x0 = cx - w / 2.0 + if d[0] < 0.0 { d[0].abs() } else { 0.0 };
    let (x1, y0, y1) = (x0 + fw, by - fh, by);
    let f = Some(PAPER);
    // side face
    let (sx, sx2) = if d[0] >= 0.0 { (x1, x1 + d[0]) } else { (x0, x0 + d[0]) };
    out.push(poly(vec![[sx, y0], [sx2, y0 + d[1]], [sx2, y1 + d[1]], [sx, y1]], f, ink(lw)));
    // top face
    out.push(poly(vec![[x0, y0], [x1, y0], [x1 + d[0], y0 + d[1]], [x0 + d[0], y0 + d[1]]], f, ink(lw)));
    // front face
    out.push(rect(x0, y0, fw, fh, f, ink(lw)));
    [x0, y0, x1, y1]
}

fn leaf(base: [f32; 2], angle_deg: f32, len: f32, wid: f32) -> Vec<[f32; 2]> {
    let (s, c) = angle_deg.to_radians().sin_cos();
    let dir = [c, -s];
    let nrm = [s, c];
    let mut pts = vec![];
    for k in 0..=8 {
        let t = k as f32 / 8.0;
        let wv = (t * std::f32::consts::PI).sin() * wid * 0.5;
        pts.push([base[0] + dir[0] * len * t + nrm[0] * wv, base[1] + dir[1] * len * t + nrm[1] * wv]);
    }
    for k in (1..8).rev() {
        let t = k as f32 / 8.0;
        let wv = (t * std::f32::consts::PI).sin() * wid * 0.5;
        pts.push([base[0] + dir[0] * len * t - nrm[0] * wv, base[1] + dir[1] * len * t - nrm[1] * wv]);
    }
    pts
}

/// Line-art primitives of a prop (bounding box: bottom-centre (x, y), size w × h).
pub fn prop_prims(prop: &Prop, pitch: f32, u: f32) -> Vec<P> {
    let mut o = Vec::new();
    let (x, y, w, h) = (prop.x, prop.y - prop.elevation, prop.w.max(4.0), prop.h.max(4.0));
    let dir = if prop.flip { -1.0 } else { 1.0 };
    let k = top_factor(pitch);
    let lw = 2.2 * u;
    let thin = Stroke2 { width: 1.4 * u, color: INK };
    let f = Some(PAPER);
    let depth = |dw: f32| [dir * dw * 0.45, -dw * 0.55 * k];
    match prop.kind {
        PropKind::Box | PropKind::Custom => {
            let d = depth(w * 0.3);
            let r = box3(&mut o, x, y, w, h, d, lw);
            if prop.kind == PropKind::Box {
                // tape line across the top and front
                let mx = (r[0] + r[2]) / 2.0;
                o.push(line(vec![[mx, r[1]], [mx + d[0], r[1] + d[1]]], thin));
                o.push(line(vec![[mx, r[1]], [mx, r[1] + (r[3] - r[1]) * 0.3]], thin));
            } else {
                o.push(text([(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0], "?", 26.0 * u, LABEL_GRAY, [0.5, 0.5]));
            }
        }
        PropKind::Chair => {
            let d = depth(w * 0.7);
            let seat_y = y - h * 0.47;
            let fw = w - d[0].abs();
            let x0 = x - w / 2.0 + if d[0] < 0.0 { d[0].abs() } else { 0.0 };
            // back legs and backrest
            for bx in [x0 + d[0], x0 + fw + d[0]] {
                o.push(line(vec![[bx, seat_y + d[1]], [bx, y + d[1]]], Stroke2 { width: lw, color: INK }));
            }
            let top = y - h;
            o.push(rect(x0 + d[0], top, fw, (seat_y + d[1]) - top - h * 0.02, f, ink(lw)));
            o.push(line(vec![[x0 + d[0] + fw * 0.15, top + h * 0.12], [x0 + d[0] + fw * 0.85, top + h * 0.12]], thin));
            // seat
            box3(&mut o, x, seat_y + h * 0.05, w, h * 0.05 + d[1].abs(), d, lw);
            for fx in [x0 + lw, x0 + fw - lw] {
                o.push(line(vec![[fx, seat_y + h * 0.05], [fx, y]], Stroke2 { width: lw, color: INK }));
            }
        }
        PropKind::Table | PropKind::Bench => {
            let d = depth(if prop.kind == PropKind::Table { w * 0.45 } else { w * 0.18 });
            let top_y = y - h * if prop.kind == PropKind::Table { 1.0 } else { 0.52 };
            let fw = w - d[0].abs();
            let x0 = x - w / 2.0 + if d[0] < 0.0 { d[0].abs() } else { 0.0 };
            let slab = h * 0.07;
            let legs_top = top_y + slab - d[1].abs() * 0.0;
            for bx in [x0 + d[0] + w * 0.05, x0 + fw + d[0] - w * 0.05] {
                o.push(line(vec![[bx, legs_top + d[1]], [bx, y + d[1]]], Stroke2 { width: lw, color: INK }));
            }
            if prop.kind == PropKind::Bench {
                // backrest slats
                let bt = y - h;
                o.push(rect(x0 + d[0], bt, fw, h * 0.12, f, ink(lw)));
                o.push(rect(x0 + d[0], bt + h * 0.2, fw, h * 0.12, f, ink(lw)));
                for bx in [x0 + d[0] + fw * 0.1, x0 + d[0] + fw * 0.9] {
                    o.push(line(vec![[bx, bt + h * 0.32], [bx, top_y + d[1]]], Stroke2 { width: lw, color: INK }));
                }
            }
            box3(&mut o, x, top_y + slab + d[1].abs(), w, slab + d[1].abs(), d, lw);
            for fx in [x0 + w * 0.05, x0 + fw - w * 0.05] {
                o.push(line(vec![[fx, top_y + slab + d[1].abs()], [fx, y]], Stroke2 { width: lw, color: INK }));
            }
        }
        PropKind::Sofa => {
            let d = depth(w * 0.3);
            let fw = w - d[0].abs();
            let x0 = x - w / 2.0 + if d[0] < 0.0 { d[0].abs() } else { 0.0 };
            // backrest
            o.push(rect(x0 + d[0] * 0.8, y - h, fw, h * 0.62, f, ink(lw)));
            // seat
            box3(&mut o, x, y - h * 0.12, w, h * 0.33 + d[1].abs(), d, lw);
            o.push(rect(x0, y - h * 0.12, fw, h * 0.12, f, ink(lw)));
            o.push(line(vec![[x0 + fw / 2.0, y - h * 0.45], [x0 + fw / 2.0, y - h * 0.12]], thin));
            // armrests
            for ax in [x0, x0 + fw - w * 0.1] {
                o.push(rect(ax, y - h * 0.62, w * 0.1, h * 0.5, f, ink(lw)));
            }
        }
        PropKind::Bed => {
            let d = depth(w * 0.35);
            let fw = w - d[0].abs();
            let x0 = x - w / 2.0 + if d[0] < 0.0 { d[0].abs() } else { 0.0 };
            let head_x = if dir > 0.0 { x0 } else { x0 + fw - w * 0.06 };
            o.push(rect(head_x + d[0], y - h + d[1].abs() * 0.0, w * 0.06, h + d[1], f, ink(lw)));
            box3(&mut o, x, y, w, h * 0.5 + d[1].abs(), d, lw);
            let pil_x = if dir > 0.0 { x0 + w * 0.15 } else { x0 + fw - w * 0.15 };
            o.push(P::Ellipse {
                c: [pil_x + d[0] * 0.5, y - h * 0.5 + d[1] * 0.5],
                r: [w * 0.09, h * 0.08 + d[1].abs() * 0.25],
                fill: f,
                stroke: ink(lw),
            });
            o.push(rect(head_x, y - h * 0.95, w * 0.06, h * 0.95, f, ink(lw)));
        }
        PropKind::Door => {
            o.push(rect(x - w / 2.0, y - h, w, h, Some([250, 250, 250, 255]), ink(lw * 1.2)));
            o.push(rect(x - w / 2.0 + w * 0.08, y - h + h * 0.05, w * 0.84, h * 0.95, f, ink(lw)));
            o.push(rect(x - w * 0.3, y - h * 0.88, w * 0.6, h * 0.32, None, Some(thin)));
            o.push(rect(x - w * 0.3, y - h * 0.48, w * 0.6, h * 0.38, None, Some(thin)));
            let kx = x + dir * w * 0.33;
            o.push(circle([kx, y - h * 0.5], w * 0.035, f, ink(lw)));
        }
        PropKind::Window => {
            o.push(rect(x - w / 2.0, y - h, w, h, Some([246, 249, 252, 255]), ink(lw * 1.2)));
            o.push(rect(x - w / 2.0 + w * 0.05, y - h + h * 0.05, w * 0.9, h * 0.9, None, Some(thin)));
            o.push(line(vec![[x, y - h * 0.95], [x, y - h * 0.05]], Stroke2 { width: lw, color: INK }));
            o.push(line(
                vec![[x - w * 0.45, y - h * 0.5], [x + w * 0.45, y - h * 0.5]],
                Stroke2 { width: lw, color: INK },
            ));
            o.push(rect(x - w * 0.56, y, w * 1.12, h * 0.05, f, ink(lw)));
            // glass glint
            o.push(line(
                vec![[x - w * 0.36, y - h * 0.62], [x - w * 0.22, y - h * 0.82]],
                Stroke2 { width: u, color: GUIDE },
            ));
        }
        PropKind::Shelf => {
            let d = depth(w * 0.3);
            let r = box3(&mut o, x, y, w, h, d, lw);
            let n = 5;
            let sh = (r[3] - r[1]) / n as f32;
            for i in 1..n {
                let yy = r[1] + sh * i as f32;
                o.push(line(vec![[r[0], yy], [r[2], yy]], Stroke2 { width: lw * 0.8, color: INK }));
            }
            for i in 0..n {
                let yy = r[1] + sh * i as f32;
                let mut bx = r[0] + (r[2] - r[0]) * 0.06;
                let mut seed = (i * 7 + 3) as f32;
                while bx < r[2] - (r[2] - r[0]) * 0.12 {
                    seed = (seed * 13.7 + 5.3) % 10.0;
                    let bw = (r[2] - r[0]) * (0.05 + seed * 0.006);
                    let bh = sh * (0.55 + seed * 0.03);
                    o.push(rect(bx, yy + sh - bh, bw, bh - lw * 0.4, None, Some(thin)));
                    bx += bw + (r[2] - r[0]) * 0.01;
                }
            }
        }
        PropKind::Counter => {
            let d = depth(w * 0.2);
            let r = box3(&mut o, x, y, w, h, d, lw);
            o.push(line(
                vec![[r[0] - w * 0.01, r[1] + h * 0.08], [r[2] + w * 0.01, r[1] + h * 0.08]],
                Stroke2 { width: lw, color: INK },
            ));
            for i in 1..4 {
                let xx = r[0] + (r[2] - r[0]) * i as f32 / 4.0;
                o.push(line(vec![[xx, r[1] + h * 0.15], [xx, r[3] - h * 0.05]], thin));
            }
        }
        PropKind::Screen => {
            let sh = h * 0.62;
            o.push(rect(x - w / 2.0, y - h, w, sh, f, ink(lw * 1.2)));
            o.push(rect(
                x - w / 2.0 + w * 0.04,
                y - h + sh * 0.06,
                w * 0.92,
                sh * 0.88,
                Some([238, 240, 244, 255]),
                Some(thin),
            ));
            o.push(line(vec![[x, y - h + sh], [x, y - h * 0.08]], Stroke2 { width: lw * 1.5, color: INK }));
            o.push(P::Ellipse { c: [x, y - h * 0.04], r: [w * 0.18, h * 0.04], fill: f, stroke: ink(lw) });
        }
        PropKind::Lamp => {
            o.push(P::Ellipse { c: [x, y - h * 0.02], r: [w * 0.35, h * 0.025], fill: f, stroke: ink(lw) });
            o.push(line(vec![[x, y - h * 0.03], [x, y - h * 0.8]], Stroke2 { width: lw * 1.4, color: INK }));
            o.push(poly(
                vec![
                    [x - w * 0.2, y - h],
                    [x + w * 0.2, y - h],
                    [x + w * 0.5, y - h * 0.8],
                    [x - w * 0.5, y - h * 0.8],
                ],
                f,
                ink(lw),
            ));
            for dx in [-0.6f32, 0.0, 0.6] {
                o.push(P::Poly {
                    pts: vec![[x + dx * w * 0.5, y - h * 0.77], [x + dx * w * 1.0, y - h * 0.62]],
                    closed: false,
                    fill: None,
                    stroke: Some(Stroke2 { width: u, color: GUIDE }),
                    dash: true,
                });
            }
        }
        PropKind::Plant => {
            o.push(poly(
                vec![[x - w * 0.3, y - h * 0.35], [x + w * 0.3, y - h * 0.35], [x + w * 0.22, y], [x - w * 0.22, y]],
                f,
                ink(lw),
            ));
            o.push(rect(x - w * 0.34, y - h * 0.4, w * 0.68, h * 0.06, f, ink(lw)));
            for (ang, len) in
                [(150.0, 0.62), (120.0, 0.7), (95.0, 0.65), (70.0, 0.7), (35.0, 0.6), (105.0, 0.5), (60.0, 0.45)]
            {
                o.push(poly(leaf([x, y - h * 0.4], ang, h * len, w * 0.22), f, ink(lw * 0.8)));
            }
        }
        PropKind::Tree => {
            o.push(poly(
                vec![[x - w * 0.06, y - h * 0.55], [x + w * 0.06, y - h * 0.55], [x + w * 0.09, y], [x - w * 0.09, y]],
                f,
                ink(lw),
            ));
            o.push(line(vec![[x, y - h * 0.4], [x + w * 0.18, y - h * 0.58]], Stroke2 { width: lw * 1.5, color: INK }));
            let cy = y - h * 0.7;
            let r = w * 0.27;
            for (dx, dy, rr) in [
                (-0.55, 0.25, 0.8),
                (0.55, 0.25, 0.8),
                (-0.35, -0.35, 0.9),
                (0.35, -0.35, 0.9),
                (0.0, -0.75, 0.85),
                (0.0, 0.15, 1.0),
            ] {
                o.push(circle([x + dx * r * 1.4, cy + dy * r], r * rr, f, ink(lw)));
            }
        }
        PropKind::Car => {
            let (x0, x1) = (x - w / 2.0, x + w / 2.0);
            let wr = h * 0.2;
            let body_top = y - h * 0.55;
            let front = if dir > 0.0 { x1 } else { x0 };
            let back = if dir > 0.0 { x0 } else { x1 };
            o.push(poly(
                vec![
                    [back, y - h * 0.18],
                    [back, body_top + h * 0.05],
                    [back + dir * w * 0.05, body_top],
                    [front - dir * w * 0.08, body_top + h * 0.03],
                    [front, body_top + h * 0.15],
                    [front, y - h * 0.18],
                ],
                f,
                ink(lw),
            ));
            let cb = body_top;
            o.push(poly(
                vec![
                    [back + dir * w * 0.15, cb],
                    [back + dir * w * 0.27, y - h],
                    [back + dir * w * 0.62, y - h],
                    [back + dir * w * 0.78, cb],
                ],
                f,
                ink(lw),
            ));
            let wy0 = y - h * 0.93;
            o.push(poly(
                vec![
                    [back + dir * w * 0.2, cb - h * 0.04],
                    [back + dir * w * 0.29, wy0],
                    [back + dir * w * 0.43, wy0],
                    [back + dir * w * 0.43, cb - h * 0.04],
                ],
                Some([244, 246, 250, 255]),
                Some(thin),
            ));
            o.push(poly(
                vec![
                    [back + dir * w * 0.47, cb - h * 0.04],
                    [back + dir * w * 0.47, wy0],
                    [back + dir * w * 0.6, wy0],
                    [back + dir * w * 0.72, cb - h * 0.04],
                ],
                Some([244, 246, 250, 255]),
                Some(thin),
            ));
            o.push(rect(x0, y - h * 0.22, w, h * 0.08, f, ink(lw)));
            for wx in [back + dir * w * 0.2, back + dir * w * 0.8] {
                o.push(circle([wx, y - wr], wr, f, ink(lw * 1.3)));
                o.push(circle([wx, y - wr], wr * 0.45, f, ink(lw * 0.8)));
            }
            o.push(circle([front - dir * w * 0.03, body_top + h * 0.2], h * 0.04, f, ink(lw * 0.8)));
        }
        PropKind::Sign => {
            for px in [x - w * 0.3, x + w * 0.3] {
                o.push(line(vec![[px, y], [px, y - h * 0.6]], Stroke2 { width: lw * 1.4, color: INK }));
            }
            o.push(rect(x - w / 2.0, y - h, w, h * 0.42, f, ink(lw * 1.2)));
            if !prop.label.trim().is_empty() {
                o.push(text(
                    [x, y - h * 0.79],
                    prop.label.trim(),
                    (h * 0.13).min(w * 0.18).max(10.0 * u),
                    INK,
                    [0.5, 0.5],
                ));
            }
        }
        PropKind::Stairs => {
            let n = 6;
            let (sw, sh) = (w / n as f32, h / n as f32);
            for i in 0..n {
                let (rx, rw) = if dir > 0.0 {
                    (x - w / 2.0 + sw * i as f32, w - sw * i as f32)
                } else {
                    (x - w / 2.0, w - sw * i as f32)
                };
                o.push(rect(rx, y - sh * (i + 1) as f32, rw, sh, f, ink(lw)));
            }
        }
        PropKind::Building => {
            o.push(rect(x - w / 2.0, y - h, w, h, f, ink(lw * 1.3)));
            o.push(rect(x - w / 2.0 - w * 0.02, y - h - h * 0.03, w * 1.04, h * 0.03, f, ink(lw)));
            let (cols, rows) = (5, 6);
            for r in 0..rows {
                for c in 0..cols {
                    let wx = x - w / 2.0 + w * (0.08 + c as f32 * 0.18);
                    let wy = y - h + h * (0.06 + r as f32 * 0.13);
                    o.push(rect(wx, wy, w * 0.11, h * 0.08, Some([246, 248, 251, 255]), Some(thin)));
                }
            }
        }
        PropKind::Wall => {
            o.push(rect(x - w / 2.0, y - h, w, h, Some([247, 247, 245, 255]), ink(lw)));
            o.push(line(vec![[x - w / 2.0, y - h * 0.06], [x + w / 2.0, y - h * 0.06]], thin));
        }
        PropKind::Cup => {
            o.push(P::Ellipse {
                c: [x + dir * w * 0.42, y - h * 0.55],
                r: [w * 0.22, h * 0.22],
                fill: None,
                stroke: ink(lw),
            });
            o.push(poly(
                vec![[x - w * 0.4, y - h], [x + w * 0.4, y - h], [x + w * 0.32, y], [x - w * 0.32, y]],
                f,
                ink(lw),
            ));
            o.push(P::Ellipse { c: [x, y - h], r: [w * 0.4, h * 0.08 * (k + 0.3)], fill: f, stroke: ink(lw * 0.8) });
        }
        PropKind::Bag => {
            o.push(P::Poly {
                pts: (0..=12)
                    .map(|i| {
                        let a = std::f32::consts::PI * i as f32 / 12.0;
                        [x - a.cos() * w * 0.28, y - h * 0.6 - a.sin() * h * 0.38]
                    })
                    .collect(),
                closed: false,
                fill: None,
                stroke: ink(lw * 1.2),
                dash: false,
            });
            o.push(poly(rounded_rect(x - w / 2.0, y - h * 0.65, w, h * 0.65, w * 0.08), f, ink(lw)));
            o.push(line(vec![[x - w / 2.0, y - h * 0.45], [x + w / 2.0, y - h * 0.45]], thin));
        }
        PropKind::Ball => {
            let r = w.min(h) / 2.0;
            let c = [x, y - r];
            o.push(circle(c, r, f, ink(lw)));
            o.push(P::Poly {
                pts: (0..=12)
                    .map(|i| {
                        let a = -1.2 + 2.4 * i as f32 / 12.0;
                        [c[0] + r * 0.45 * a.cos() - r * 0.2, c[1] + r * 0.95 * a.sin()]
                    })
                    .collect(),
                closed: false,
                fill: None,
                stroke: Some(thin),
                dash: false,
            });
        }
    }
    o
}

// ---------------------------------------------------------------- shot

/// Background: paper, floor tint, horizon / floor line and perspective floor grid.
fn background(p: &Project, shot: &Shot, u: f32) -> Vec<P> {
    let (w, h) = (p.canvas.width as f32, p.canvas.height as f32);
    let hz = (shot.horizon * h).clamp(0.0, h);
    let night = matches!(shot.setting.time_of_day, TimeOfDay::Night | TimeOfDay::LateNight);
    let sky: Rgba = if night { [236, 238, 246, 255] } else { PAPER };
    let floor: Rgba = match shot.setting.in_out {
        InOut::Interior => [246, 245, 242, 255],
        InOut::Exterior => [244, 246, 241, 255],
    };
    let mut o = vec![rect(0.0, 0.0, w, h, Some(sky), None), rect(0.0, hz, w, h - hz, Some(floor), None)];
    if shot.floor_grid && hz < h - 2.0 {
        let vp = [w / 2.0, hz];
        let g = Stroke2 { width: 1.0 * u, color: [222, 224, 228, 255] };
        let n = 14;
        for i in -n..=n {
            let bx = w / 2.0 + i as f32 * w / (n as f32 * 0.5);
            o.push(line(vec![vp, [bx, h]], g));
        }
        // depth lines with perspective spacing
        let mut t = 0.04f32;
        while t < 1.0 {
            let yy = hz + (h - hz) * t;
            o.push(line(vec![[0.0, yy], [w, yy]], g));
            t = t * 1.6 + 0.02;
        }
    }
    o.push(line(vec![[0.0, hz], [w, hz]], Stroke2 { width: 1.6 * u, color: [180, 184, 190, 255] }));
    o
}

/// Speech bubble with wrapped text above `tip` (top of a head).
fn bubble(out: &mut Vec<P>, tip: [f32; 2], s: &str, color: Rgba, u: f32, canvas_w: f32) {
    let fonts = Fonts::get();
    let size = 26.0 * u;
    let max_w = 420.0 * u;
    let lines = fonts.wrap(s, size, max_w);
    let tw = lines.iter().map(|l| fonts.measure(l, size)).fold(0.0, f32::max);
    let lh = size * 1.3;
    let pad = 14.0 * u;
    let bw = tw + pad * 2.0;
    let bh = lh * lines.len() as f32 + pad * 1.4;
    let bx = (tip[0] + 30.0 * u).clamp(4.0, (canvas_w - bw - 4.0).max(4.0));
    let by = (tip[1] - bh - 46.0 * u).max(4.0);
    let st = Some(Stroke2 { width: 2.0 * u, color });
    let base_x = (tip[0] + 12.0 * u).clamp(bx + 16.0 * u, bx + bw - 16.0 * u);
    out.push(poly(
        vec![
            [base_x - 12.0 * u, by + bh - 2.0],
            [base_x + 14.0 * u, by + bh - 2.0],
            [tip[0] + 4.0 * u, tip[1] - 12.0 * u],
        ],
        Some(PAPER),
        st,
    ));
    out.push(poly(rounded_rect(bx, by, bw, bh, 18.0 * u), Some(PAPER), st));
    // cover the tail's base line
    out.push(line(
        vec![[base_x - 10.0 * u, by + bh - 1.0 * u], [base_x + 12.0 * u, by + bh - 1.0 * u]],
        Stroke2 { width: 3.0 * u, color: PAPER },
    ));
    for (i, l) in lines.iter().enumerate() {
        out.push(text(
            [bx + pad, by + pad * 0.7 + lh * i as f32 + (lh - size) / 2.0],
            l.clone(),
            size,
            INK,
            [0.0, 0.0],
        ));
    }
}

fn arrow_head(tip: [f32; 2], from: [f32; 2], size: f32, color: Rgba) -> P {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (ux, uy) = (dx / l, dy / l);
    let (nx, ny) = (-uy, ux);
    poly(
        vec![
            [tip[0] + ux * size * 0.3, tip[1] + uy * size * 0.3],
            [tip[0] - ux * size + nx * size * 0.55, tip[1] - uy * size + ny * size * 0.55],
            [tip[0] - ux * size * 0.7, tip[1] - uy * size * 0.7],
            [tip[0] - ux * size - nx * size * 0.55, tip[1] - uy * size - ny * size * 0.55],
        ],
        Some(color),
        None,
    )
}

/// Motion path with arrow and a numbered label.
fn motion(out: &mut Vec<P>, p: &Project, shot: &Shot, a: &Actor, n: usize, u: f32) {
    let m = &a.movement;
    if !m.is_active() {
        return;
    }
    let cast = p.cast_of(a);
    let col = cast.color.rgba(255);
    let pts = a.path_points();
    let path = smooth_path(&pts, 14);
    let w = 4.0 * u;
    // white halo for legibility over line art
    out.push(P::Poly {
        pts: path.clone(),
        closed: false,
        fill: None,
        stroke: Some(Stroke2 { width: w + 4.0 * u, color: [255, 255, 255, 170] }),
        dash: false,
    });
    out.push(P::Poly {
        pts: path.clone(),
        closed: false,
        fill: None,
        stroke: Some(Stroke2 { width: w, color: col }),
        dash: true,
    });
    let tip = *path.last().unwrap();
    let from = point_along(&path, 0.97);
    let from = if (from[0] - tip[0]).abs() + (from[1] - tip[1]).abs() < 1.0 { pts[pts.len() - 2] } else { from };
    out.push(arrow_head(tip, from, 26.0 * u, col));
    out.push(circle(pts[0], 7.0 * u, Some(col), Some(Stroke2 { width: 2.0 * u, color: PAPER })));
    for wp in &pts[1..pts.len() - 1] {
        out.push(circle(*wp, 5.0 * u, Some(PAPER), Some(Stroke2 { width: 2.0 * u, color: col })));
    }
    let mid = point_along(&path, 0.5);
    let label = format!("{} {}{} {:.1}–{:.1}s", circled(n), cast.name, m.style.zh(), m.start, m.end);
    let _ = shot;
    out.push(P::Text {
        pos: [mid[0], mid[1] - 12.0 * u],
        text: label,
        size: 22.0 * u,
        color: PAPER,
        anchor: [0.5, 1.0],
        bg: Some(col),
    });
}

/// Draft items of a shot in drawing order.
pub fn shot_items(p: &Project, shot: &Shot, opts: &DraftOptions) -> Vec<Item> {
    let (cw, ch) = (p.canvas.width as f32, p.canvas.height as f32);
    let u = ch / 1080.0;
    let pitch = shot.camera.angle.pitch();
    let mut items: Vec<Item> = background(p, shot, u).into_iter().map(Item::Prim).collect();

    // Depth-sorted scene content: props and figures by ground y (far first).
    #[allow(clippy::large_enum_variant)]
    enum Z<'a> {
        Prop(&'a Prop),
        Fig(FigureSpec, f32),
    }
    let mut z: Vec<(f32, Z)> = Vec::new();
    for pr in &shot.props {
        z.push((pr.y, Z::Prop(pr)));
    }
    for a in &shot.actors {
        let f = FigureSpec::of_actor(p, shot, a);
        z.push((a.y, Z::Fig(f, a.scale)));
        if opts.ghosts
            && let Some(g) = FigureSpec::ghost_of_actor(p, shot, a)
        {
            let s = a.scale * end_scale_factor(p, shot, a);
            z.push((g.y - 0.01, Z::Fig(g, s)));
        }
    }
    z.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, it) in z {
        match it {
            Z::Prop(pr) => items.extend(prop_prims(pr, pitch, u).into_iter().map(Item::Prim)),
            Z::Fig(f, s) => {
                let rx = 0.28 * p.base_ppm() * s;
                let ry = rx * (0.12 + 0.5 * top_factor(pitch)) * 0.5;
                let a = if f.ghost { 14 } else { 30 };
                let shrink = 1.0 / (1.0 + f.lift * 2.0);
                items.push(Item::Prim(P::Ellipse {
                    c: [f.x, f.y],
                    r: [rx * shrink, ry * shrink],
                    fill: Some([0, 0, 0, a]),
                    stroke: None,
                }));
                items.push(Item::Figure(f));
            }
        }
    }

    let mut over: Vec<P> = Vec::new();
    if opts.motion {
        let mut n = 0;
        for a in &shot.actors {
            if a.movement.is_active() {
                n += 1;
                motion(&mut over, p, shot, a, n, u);
            }
        }
    }
    if opts.prop_labels {
        for pr in &shot.props {
            if pr.kind == PropKind::Sign {
                continue;
            }
            over.push(text([pr.x, pr.y - pr.elevation + 6.0 * u], pr.display_name(), 18.0 * u, LABEL_GRAY, [0.5, 0.0]));
        }
    }
    for a in &shot.actors {
        let f = FigureSpec::of_actor(p, shot, a);
        let top = f.head_top();
        let cast = p.cast_of(a);
        if opts.dialogue && !a.dialogue.trim().is_empty() {
            bubble(&mut over, top, a.dialogue.trim(), cast.color.rgba(255), u, cw);
        }
        if opts.labels {
            over.push(P::Text {
                pos: [top[0], top[1] - 10.0 * u],
                text: cast.name.clone(),
                size: 24.0 * u,
                color: PAPER,
                anchor: [0.5, 1.0],
                bg: Some(cast.color.rgba(255)),
            });
        }
    }
    if opts.header {
        let idx = p.shots.iter().position(|s| s.id == shot.id).unwrap_or(0) + 1;
        let size = 26.0 * u;
        over.push(P::Text {
            pos: [16.0 * u, 14.0 * u],
            text: format!("鏡頭 {idx:02} · {}", shot.name),
            size,
            color: PAPER,
            anchor: [0.0, 0.0],
            bg: Some([30, 32, 38, 230]),
        });
        over.push(P::Text {
            pos: [cw - 16.0 * u, 14.0 * u],
            text: format!(
                "{} {} · {} · {} · {:.1}s",
                shot.camera.size.zh(),
                shot.camera.size.en(),
                shot.camera.angle.zh(),
                shot.camera.movement.zh(),
                shot.duration
            ),
            size: size * 0.85,
            color: INK,
            anchor: [1.0, 0.0],
            bg: Some([255, 255, 255, 225]),
        });
        let loc = if shot.setting.location.trim().is_empty() {
            "（未設定地點）"
        } else {
            shot.setting.location.trim()
        };
        over.push(P::Text {
            pos: [16.0 * u, ch - 14.0 * u],
            text: format!("{} · {} · {}", shot.setting.in_out.zh(), loc, shot.setting.time_of_day.zh()),
            size: size * 0.85,
            color: INK,
            anchor: [0.0, 1.0],
            bg: Some([255, 255, 255, 225]),
        });
    }
    items.extend(over.into_iter().map(Item::Prim));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_path_keeps_endpoints() {
        let pts = [[0.0, 0.0], [100.0, 50.0], [200.0, 0.0]];
        let s = smooth_path(&pts, 10);
        assert_eq!(s.first(), Some(&[0.0, 0.0]));
        assert_eq!(s.last(), Some(&[200.0, 0.0]));
        assert!(s.len() > 10);
        let m = point_along(&[[0.0, 0.0], [10.0, 0.0]], 0.5);
        assert!((m[0] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn every_prop_kind_draws() {
        for &kind in PropKind::ALL {
            for flip in [false, true] {
                let pr = Prop { kind, flip, label: "測試".into(), ..Prop::default() };
                let prims = prop_prims(&pr, 4.0, 1.0);
                assert!(!prims.is_empty(), "{kind:?}");
                for p in prims {
                    if let P::Poly { pts, .. } = p {
                        assert!(pts.iter().all(|q| q[0].is_finite() && q[1].is_finite()), "{kind:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn sample_shot_has_figures_motion_and_labels() {
        let p = crate::sample::sample_project();
        let items = shot_items(&p, &p.shots[0], &DraftOptions::default());
        let figs = items.iter().filter(|i| matches!(i, Item::Figure(f) if !f.ghost)).count();
        assert_eq!(figs, p.shots[0].actors.len());
        let texts: Vec<&str> = items
            .iter()
            .filter_map(|i| if let Item::Prim(P::Text { text, .. }) = i { Some(text.as_str()) } else { None })
            .collect();
        assert!(texts.iter().any(|t| t.starts_with("鏡頭 01")));
        assert!(texts.iter().any(|t| t.starts_with('①')));
    }
}
