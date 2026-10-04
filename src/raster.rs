//! Offscreen software rasteriser (tiny-skia + ab_glyph) for draft PNG export.
//! Needs no GPU or display. Text uses the bundled Noto Sans CJK TC subset with a
//! system font fallback (from whitebox-video-storyboard); figures are drawn with
//! the mask + polyline scheme of rust-pose-studio.

use std::sync::OnceLock;

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont, point};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, StrokeDash, Transform};

use crate::draw::{Item, P, Rgba, Stroke2};
use crate::figure::FigureDraw;
use crate::mannequin::render::{DrawList, Frame2, Mask, Prim};

/// Font chain: bundled CJK subset first, then an optional system font.
pub struct Fonts {
    chain: Vec<FontArc>,
}

const NO_LINE_START: &str = "，。、！？：；）」』】》〉,.!?:;)]}…ー～";

impl Fonts {
    pub fn get() -> &'static Fonts {
        static F: OnceLock<Fonts> = OnceLock::new();
        F.get_or_init(|| {
            let mut chain = vec![FontArc::try_from_slice(crate::fonts::cjk_font()).expect("bundled font")];
            if let Some((bytes, idx)) = crate::fonts::load_system_fallback()
                && let Ok(f) = ab_glyph::FontVec::try_from_vec_and_index(bytes, idx)
            {
                chain.push(FontArc::new(f));
            }
            Fonts { chain }
        })
    }

    fn pick(&self, c: char) -> (usize, GlyphId) {
        for (i, f) in self.chain.iter().enumerate() {
            let g = f.glyph_id(c);
            if g.0 != 0 {
                return (i, g);
            }
        }
        (0, self.chain[0].glyph_id(c))
    }

    fn advance(&self, c: char, size: f32) -> f32 {
        let (i, g) = self.pick(c);
        self.chain[i].as_scaled(PxScale::from(size)).h_advance(g)
    }

    pub fn measure(&self, s: &str, size: f32) -> f32 {
        s.chars().map(|c| self.advance(c, size)).sum()
    }

    fn ascent(&self, size: f32) -> f32 {
        self.chain[0].as_scaled(PxScale::from(size)).ascent()
    }

    fn descent(&self, size: f32) -> f32 {
        self.chain[0].as_scaled(PxScale::from(size)).descent()
    }

    /// Greedy line wrapping: break at spaces for Latin words, anywhere between CJK characters.
    pub fn wrap(&self, text: &str, size: f32, max_w: f32) -> Vec<String> {
        let mut lines = vec![];
        for para in text.split('\n') {
            let mut line = String::new();
            let mut w = 0.0;
            let mut tokens: Vec<String> = vec![];
            for c in para.chars() {
                let wordy = c.is_ascii_alphanumeric() || "'-_.,:;!?)\"".contains(c);
                match tokens.last_mut() {
                    Some(t)
                        if wordy
                            && t.chars().last().is_some_and(|l| l.is_ascii_alphanumeric() || "'-_".contains(l)) =>
                    {
                        t.push(c)
                    }
                    _ => tokens.push(c.to_string()),
                }
            }
            for t in tokens {
                let tw = self.measure(&t, size);
                let hang = t.chars().count() == 1 && NO_LINE_START.contains(t.as_str());
                if w + tw > max_w && !line.is_empty() && !hang {
                    lines.push(line.trim_end().to_string());
                    line = String::new();
                    w = 0.0;
                    if t == " " {
                        continue;
                    }
                }
                if tw > max_w && t.chars().count() > 1 {
                    for c in t.chars() {
                        let cw = self.advance(c, size);
                        if w + cw > max_w && !line.is_empty() {
                            lines.push(std::mem::take(&mut line));
                            w = 0.0;
                        }
                        line.push(c);
                        w += cw;
                    }
                } else {
                    line.push_str(&t);
                    w += tw;
                }
            }
            lines.push(line.trim_end().to_string());
        }
        lines
    }
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

/// Source-over blend of one pixel with coverage `cov` (0..1).
fn blend(pm: &mut Pixmap, x: i32, y: i32, c: Rgba, cov: f32) {
    if x < 0 || y < 0 || x >= pm.width() as i32 || y >= pm.height() as i32 {
        return;
    }
    let a = (c[3] as f32 / 255.0) * cov.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let idx = (y as u32 * pm.width() + x as u32) as usize * 4;
    let data = pm.data_mut();
    for k in 0..3 {
        let src = c[k] as f32 * a;
        data[idx + k] = (src + data[idx + k] as f32 * (1.0 - a)).round().min(255.0) as u8;
    }
    let da = data[idx + 3] as f32 / 255.0;
    data[idx + 3] = ((a + da * (1.0 - a)) * 255.0).round() as u8;
    for k in 0..3 {
        data[idx + k] = data[idx + k].min(data[idx + 3]);
    }
}

/// Draw one line of text with its left edge at `x` and baseline at `baseline`.
pub fn draw_line(pm: &mut Pixmap, s: &str, size: f32, x: f32, baseline: f32, c: Rgba) {
    let fonts = Fonts::get();
    let mut pen = x;
    for ch in s.chars() {
        let (i, gid) = fonts.pick(ch);
        let font = &fonts.chain[i];
        let scale = PxScale::from(size);
        let g = gid.with_scale_and_position(scale, point(pen, baseline));
        if let Some(og) = font.outline_glyph(g) {
            let b = og.px_bounds();
            og.draw(|gx, gy, cov| blend(pm, b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32, c, cov));
        }
        pen += font.as_scaled(scale).h_advance(gid);
    }
}

/// Text box (x, y, w, h) of an anchored label (shared with the egui painter).
pub fn text_box(pos: [f32; 2], s: &str, size: f32, anchor: [f32; 2], badge: bool) -> [f32; 4] {
    let fonts = Fonts::get();
    let pad = if badge { size * 0.35 } else { 0.0 };
    let w = fonts.measure(s, size) + pad * 2.0;
    let h = size * 1.25 + if badge { size * 0.2 } else { 0.0 };
    [pos[0] - anchor[0] * w, pos[1] - anchor[1] * h, w, h]
}

fn path_of(pts: &[[f32; 2]], closed: bool) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for (i, q) in pts.iter().enumerate() {
        if i == 0 { pb.move_to(q[0], q[1]) } else { pb.line_to(q[0], q[1]) }
    }
    if closed {
        pb.close();
    }
    pb.finish()
}

fn stroke_path(pm: &mut Pixmap, path: &tiny_skia::Path, s: Stroke2, dash: bool) {
    let mut st = Stroke { width: s.width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    if dash {
        st.dash = StrokeDash::new(vec![s.width * 3.0, s.width * 2.2], 0.0);
    }
    pm.stroke_path(path, &paint(s.color), &st, Transform::identity(), None);
}

/// Rasterise one 2D primitive.
pub fn draw_prim(pm: &mut Pixmap, p: &P) {
    match p {
        P::Poly { pts, closed, fill, stroke, dash } => {
            if pts.len() < 2 {
                return;
            }
            let Some(path) = path_of(pts, *closed) else { return };
            if *closed && let Some(f) = fill {
                pm.fill_path(&path, &paint(*f), FillRule::Winding, Transform::identity(), None);
            }
            if let Some(s) = stroke {
                stroke_path(pm, &path, *s, *dash);
            }
        }
        P::Ellipse { c, r, fill, stroke } => {
            let Some(rect) =
                tiny_skia::Rect::from_xywh(c[0] - r[0], c[1] - r[1], (r[0] * 2.0).max(0.1), (r[1] * 2.0).max(0.1))
            else {
                return;
            };
            let Some(path) = PathBuilder::from_oval(rect) else { return };
            if let Some(f) = fill {
                pm.fill_path(&path, &paint(*f), FillRule::Winding, Transform::identity(), None);
            }
            if let Some(s) = stroke {
                stroke_path(pm, &path, *s, false);
            }
        }
        P::Text { pos, text, size, color, anchor, bg } => {
            let b = text_box(*pos, text, *size, *anchor, bg.is_some());
            let fonts = Fonts::get();
            if let Some(bgc) = bg
                && let Some(path) = path_of(&crate::draw::rounded_rect(b[0], b[1], b[2], b[3], size * 0.3), true)
            {
                pm.fill_path(&path, &paint(*bgc), FillRule::Winding, Transform::identity(), None);
            }
            let pad = if bg.is_some() { size * 0.35 } else { 0.0 };
            // vertically centre the em box inside the text box
            let asc = fonts.ascent(*size);
            let desc = fonts.descent(*size);
            let base = b[1] + b[3] / 2.0 + (asc + desc) / 2.0;
            draw_line(pm, text, *size, b[0] + pad, base, *color);
        }
    }
}

/// Composite a figure's body fill mask (its view rectangle mapped through `frame`).
fn draw_mask(pixmap: &mut Pixmap, mask: &Mask, frame: &Frame2) {
    let Some(mut img) = Pixmap::new(mask.w as u32, mask.h as u32) else { return };
    let rgba = mask.rgba();
    for (dst, src) in img.pixels_mut().iter_mut().zip(rgba.as_chunks::<4>().0) {
        *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
    }
    let a = frame.to_px([mask.min[0], mask.max[1]]);
    let b = frame.to_px([mask.max[0], mask.min[1]]);
    let sx = (b[0] - a[0]) / mask.w as f32;
    let sy = (b[1] - a[1]) / mask.h as f32;
    let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
    pixmap.draw_pixmap(0, 0, img.as_ref(), &paint, Transform::from_row(sx, 0.0, 0.0, sy, a[0], a[1]), None);
}

fn draw_fig_prims(pixmap: &mut Pixmap, prims: &[Prim], frame: &Frame2, frame_h: f32) {
    for prim in prims {
        if prim.pts.len() < 2 {
            continue;
        }
        let pts: Vec<[f32; 2]> = prim.pts.iter().map(|q| frame.to_px(*q)).collect();
        let Some(path) = path_of(&pts, prim.closed) else { continue };
        if prim.closed
            && let Some(f) = prim.fill
        {
            pixmap.fill_path(&path, &paint(f), FillRule::Winding, Transform::identity(), None);
        }
        if let Some(s) = prim.stroke {
            let stroke = Stroke {
                width: frame.stroke_px(s.width, frame_h),
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint(s.color), &stroke, Transform::identity(), None);
        }
    }
}

/// Rasterise a figure's drawing list. `frame_h` = canvas height (line widths are relative to it).
pub fn draw_figure_list(pixmap: &mut Pixmap, list: &DrawList, frame: &Frame2, frame_h: f32) {
    draw_fig_prims(pixmap, &list.under, frame, frame_h);
    if let Some(mask) = &list.mask {
        draw_mask(pixmap, mask, frame);
    }
    draw_fig_prims(pixmap, &list.prims, frame, frame_h);
}

/// Rasterise draft items into a new canvas-sized pixmap.
pub fn render_items(items: &[Item], width: u32, height: u32) -> Result<Pixmap, String> {
    let mut pm = Pixmap::new(width, height).ok_or("invalid image size")?;
    pm.fill(tiny_skia::Color::WHITE);
    for it in items {
        match it {
            Item::Prim(p) => draw_prim(&mut pm, p),
            Item::Figure(f) => {
                let FigureDraw { list, frame } = f.build(1.0);
                draw_figure_list(&mut pm, &list, &frame, height as f32);
            }
        }
    }
    Ok(pm)
}

/// Encode a pixmap as PNG (demultiplying alpha).
pub fn encode_png(pm: &Pixmap) -> Result<Vec<u8>, String> {
    let mut rgba = Vec::with_capacity(pm.data().len());
    for p in pm.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let img = image::RgbaImage::from_raw(pm.width(), pm.height(), rgba).ok_or("bad image buffer size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| format!("PNG 編碼失敗：{e}"))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_cjk_and_latin() {
        let f = Fonts::get();
        let lines = f.wrap("這是一段很長的中文對白，需要自動換行。Hello world", 20.0, 120.0);
        assert!(lines.len() >= 3, "{lines:?}");
        assert!(lines.iter().all(|l| f.measure(l, 20.0) <= 140.0));
    }

    #[test]
    fn text_draws_pixels() {
        let mut pm = Pixmap::new(200, 60).unwrap();
        pm.fill(tiny_skia::Color::WHITE);
        draw_prim(
            &mut pm,
            &P::Text {
                pos: [10.0, 10.0],
                text: "分鏡 Storyboard".into(),
                size: 24.0,
                color: [0, 0, 0, 255],
                anchor: [0.0, 0.0],
                bg: None,
            },
        );
        let dark = pm.pixels().iter().filter(|p| p.red() < 128).count();
        assert!(dark > 50, "{dark}");
    }
}
