//! egui backend for draft items: 2D prims ([`rss::draw::P`]) and line-art figures
//! (rust-pose-studio drawing lists with a fill-mask texture), plus a figure cache.

use eframe::egui::{self, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use rss::draw::{Item, P, Rgba};
use rss::figure::{FigureDraw, FigureSpec};
use rss::mannequin::render::{DrawList, Frame2, Prim};
use std::collections::HashMap;

/// Canvas (target pixels) → screen transform.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub origin: Pos2,
    pub scale: f32,
}

impl View {
    pub fn pt(&self, p: [f32; 2]) -> Pos2 {
        pos2(self.origin.x + p[0] * self.scale, self.origin.y + p[1] * self.scale)
    }
    pub fn rect(&self, b: [f32; 4]) -> Rect {
        Rect::from_min_max(self.pt([b[0], b[1]]), self.pt([b[2], b[3]]))
    }
    pub fn to_canvas(self, p: Pos2) -> [f32; 2] {
        [(p.x - self.origin.x) / self.scale, (p.y - self.origin.y) / self.scale]
    }
    /// Figure frame (view units → canvas px) composed with this view.
    pub fn frame(&self, f: &Frame2) -> Frame2 {
        Frame2 {
            ox: self.origin.x + f.ox * self.scale,
            oy: self.origin.y + f.oy * self.scale,
            scale: f.scale * self.scale,
        }
    }
}

pub fn c32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// A built figure with its uploaded fill texture.
pub struct CachedFig {
    pub draw: FigureDraw,
    pub tex: Option<egui::TextureHandle>,
    used: u64,
}

/// Figures are re-rendered (line art + hidden lines + fill mask) only when their
/// spec or the raster resolution changes.
#[derive(Default)]
pub struct FigCache {
    map: HashMap<String, CachedFig>,
    frame: u64,
}

fn quantize(res: f32) -> f32 {
    ((res * 16.0).round() / 16.0).clamp(1.0 / 16.0, 4.0)
}

pub fn fig_key(spec: &FigureSpec, res: f32) -> String {
    format!("{:.4}|{spec:?}", quantize(res))
}

impl FigCache {
    /// Drop entries that were not used in the last couple of frames.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        let f = self.frame;
        self.map.retain(|_, c| f - c.used <= 3);
    }

    pub fn get(&mut self, ctx: &egui::Context, spec: &FigureSpec, res: f32) -> (&CachedFig, String) {
        let key = fig_key(spec, res);
        let frame = self.frame;
        let e = self.map.entry(key.clone()).or_insert_with(|| {
            let draw = spec.build(quantize(res));
            let tex = draw.list.mask.as_ref().map(|m| {
                let img = egui::ColorImage::from_rgba_unmultiplied([m.w, m.h], &m.rgba());
                ctx.load_texture("figure-mask", img, egui::TextureOptions::LINEAR)
            });
            CachedFig { draw, tex, used: frame }
        });
        e.used = frame;
        (e, key)
    }

    pub fn peek(&self, key: &str) -> Option<&CachedFig> {
        self.map.get(key)
    }
}

fn fig_prims(shapes: &mut Vec<Shape>, prims: &[Prim], frame: &Frame2, frame_h: f32) {
    for prim in prims {
        if prim.pts.len() < 2 {
            continue;
        }
        let pts: Vec<Pos2> = prim
            .pts
            .iter()
            .map(|p| {
                let q = frame.to_px(*p);
                pos2(q[0], q[1])
            })
            .collect();
        let stroke =
            prim.stroke.map(|s| Stroke::new(frame.stroke_px(s.width, frame_h), c32(s.color))).unwrap_or(Stroke::NONE);
        if prim.closed {
            shapes.push(Shape::convex_polygon(pts, prim.fill.map(c32).unwrap_or(Color32::TRANSPARENT), stroke));
        } else {
            shapes.push(Shape::line(pts, stroke));
        }
    }
}

/// Paint a figure drawing list mapped by `view`.
pub fn paint_figure(
    painter: &Painter,
    list: &DrawList,
    frame: &Frame2,
    tex: Option<&egui::TextureHandle>,
    view: View,
    canvas_h: f32,
) {
    let f = view.frame(frame);
    let fh = canvas_h * view.scale;
    let mut shapes = vec![];
    fig_prims(&mut shapes, &list.under, &f, fh);
    painter.extend(shapes);
    if let (Some(m), Some(tex)) = (&list.mask, tex) {
        let a = f.to_px([m.min[0], m.max[1]]);
        let b = f.to_px([m.max[0], m.min[1]]);
        let rect = Rect::from_min_max(pos2(a[0], a[1]), pos2(b[0], b[1]));
        painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    }
    let mut shapes = vec![];
    fig_prims(&mut shapes, &list.prims, &f, fh);
    painter.extend(shapes);
}

fn closed_loop(mut v: Vec<Pos2>) -> Vec<Pos2> {
    if let Some(&f) = v.first() {
        v.push(f);
    }
    v
}

/// Paint one 2D primitive. Texts smaller than `min_text` screen px are skipped.
pub fn paint_prim(painter: &Painter, p: &P, view: View, min_text: f32) {
    let s = view.scale;
    match p {
        P::Poly { pts, closed, fill, stroke, dash } => {
            if pts.len() < 2 {
                return;
            }
            let v: Vec<Pos2> = pts.iter().map(|q| view.pt(*q)).collect();
            let st = stroke.map(|k| Stroke::new((k.width * s).max(0.6), c32(k.color))).unwrap_or(Stroke::NONE);
            if *closed && fill.is_some() {
                painter.add(Shape::convex_polygon(
                    v.clone(),
                    fill.map(c32).unwrap_or(Color32::TRANSPARENT),
                    if *dash { Stroke::NONE } else { st },
                ));
                if !*dash {
                    return;
                }
            }
            if st.width <= 0.0 || st.color == Color32::TRANSPARENT {
                return;
            }
            let v = if *closed { closed_loop(v) } else { v };
            if *dash {
                let w = st.width.max(1.0);
                painter.extend(Shape::dashed_line(&v, st, w * 3.0, w * 2.2));
            } else {
                painter.add(Shape::line(v, st));
            }
        }
        P::Shape { pts, center, fill, stroke, dash } => {
            if pts.len() < 3 {
                return;
            }
            let v: Vec<Pos2> = pts.iter().map(|q| view.pt(*q)).collect();
            if let Some(f) = fill {
                // star-shaped polygon: triangle fan around the centre
                let mut mesh = egui::Mesh::default();
                let col = c32(*f);
                mesh.colored_vertex(view.pt(*center), col);
                for p in &v {
                    mesh.colored_vertex(*p, col);
                }
                let n = v.len() as u32;
                for i in 0..n {
                    mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
                }
                painter.add(Shape::mesh(mesh));
            }
            if let Some(k) = stroke {
                let st = Stroke::new((k.width * s).max(0.6), c32(k.color));
                let v = closed_loop(v);
                if *dash {
                    let w = st.width.max(1.0);
                    painter.extend(Shape::dashed_line(&v, st, w * 3.0, w * 2.2));
                } else {
                    painter.add(Shape::line(v, st));
                }
            }
        }
        P::Ellipse { c, r, fill, stroke } => {
            let c = view.pt(*c);
            let r = vec2(r[0] * s, r[1] * s);
            if let Some(f) = fill {
                painter.add(Shape::ellipse_filled(c, r, c32(*f)));
            }
            if let Some(k) = stroke {
                painter.add(Shape::ellipse_stroke(c, r, Stroke::new((k.width * s).max(0.6), c32(k.color))));
            }
        }
        P::Text { pos, text, size, color, anchor, bg } => {
            let px = size * s;
            if px < min_text {
                return;
            }
            let b = rss::raster::text_box(*pos, text, *size, *anchor, bg.is_some());
            let rect = view.rect([b[0], b[1], b[0] + b[2], b[1] + b[3]]);
            if let Some(bgc) = bg {
                painter.rect_filled(rect, size * 0.3 * s, c32(*bgc));
            }
            let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(px), c32(*color));
            let pos = pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0);
            painter.galley(pos, galley, c32(*color));
        }
    }
}

/// Paint draft items. Returns the cache key of every main (non-ghost) figure by actor id.
pub fn paint_items(
    painter: &Painter,
    ctx: &egui::Context,
    cache: &mut FigCache,
    items: &[Item],
    view: View,
    canvas_h: f32,
    min_text: f32,
) -> Vec<(String, String)> {
    let res = view.scale * ctx.pixels_per_point();
    let mut keys = vec![];
    for it in items {
        match it {
            Item::Prim(p) => paint_prim(painter, p, view, min_text),
            Item::Figure(f) => {
                let (c, key) = cache.get(ctx, f, res);
                paint_figure(painter, &c.draw.list, &c.draw.frame, c.tex.as_ref(), view, canvas_h);
                if !f.ghost {
                    keys.push((f.actor_id.clone(), key));
                }
            }
        }
    }
    keys
}
