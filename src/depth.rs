//! Grayscale depth maps from the scene geometry (ControlNet / MiDaS convention:
//! near = white, far = black; values are relative inverse depth / disparity).
//!
//! - Ground plane: disparity of a flat floor seen from a level camera is linear in
//!   the distance below the horizon, so the floor is a vertical gradient from black
//!   at the horizon to near-white at the bottom edge (interiors keep a dark back wall).
//! - Props (walls, buildings, furniture…): solid silhouettes of all their shapes at
//!   the disparity of their ground line (front-parallel cards).
//! - Characters: the mannequin's triangle meshes are z-buffered, so limbs and torso
//!   are solid, rounded volumes; their relief is added to the disparity of the
//!   figure's ground point.
//!
//! Everything is painted back to front (sorted by ground y), like the clean frames.

use tiny_skia::{FillRule, LineCap, LineJoin, Mask, PathBuilder, Rect, Stroke, Transform};

use crate::draw::{P, prop_prims};
use crate::figure::FigureSpec;
use crate::model::{InOut, Project, Shot};
use crate::raster::path_of;

/// Disparity of the back wall / far end of an interior.
pub const INTERIOR_WALL: f32 = 0.08;
/// Disparity of the floor at the bottom edge; the headroom up to 1.0 keeps the
/// relief of bodies standing right at the front.
pub const NEAR_FLOOR: f32 = 0.9;
/// Exaggeration of the body relief so the roundness stays visible.
pub const RELIEF: f32 = 1.5;

/// A depth map with values in 0 (far) ..= 1 (near).
#[derive(Clone, Debug, PartialEq)]
pub struct DepthMap {
    pub w: u32,
    pub h: u32,
    pub data: Vec<f32>,
}

impl DepthMap {
    pub fn get(&self, x: u32, y: u32) -> f32 {
        self.data[(y * self.w + x) as usize]
    }

    /// Crop `[x, y, w, h]` (as produced by `video_ai::crop_rect`).
    pub fn crop(&self, r: [u32; 4]) -> DepthMap {
        let [x, y, w, h] = r;
        if [x, y, w, h] == [0, 0, self.w, self.h] {
            return self.clone();
        }
        let mut data = Vec::with_capacity((w * h) as usize);
        for yy in y..y + h {
            let row = (yy * self.w + x) as usize;
            data.extend_from_slice(&self.data[row..row + w as usize]);
        }
        DepthMap { w, h, data }
    }

    /// 8-bit gray values.
    pub fn gray8(&self) -> Vec<u8> {
        self.data.iter().map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect()
    }

    /// Grayscale PNG, 8 or 16 bit.
    pub fn to_png(&self, sixteen: bool) -> Result<Vec<u8>, String> {
        let buf: Vec<u8> = if sixteen {
            self.data.iter().flat_map(|&v| ((v.clamp(0.0, 1.0) * 65535.0).round() as u16).to_be_bytes()).collect()
        } else {
            self.gray8()
        };
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.w, self.h);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(if sixteen { png::BitDepth::Sixteen } else { png::BitDepth::Eight });
            enc.set_compression(png::Compression::Fast);
            let err = |e: png::EncodingError| format!("PNG 編碼失敗：{e}");
            let mut w = enc.write_header().map_err(err)?;
            w.write_image_data(&buf).map_err(err)?;
            w.finish().map_err(err)?;
        }
        Ok(out)
    }
}

struct Geo {
    hz: f32,
    h: f32,
    base: f32,
}

impl Geo {
    fn new(p: &Project, shot: &Shot) -> Geo {
        let h = p.canvas.height as f32;
        let base = if shot.setting.in_out == InOut::Interior { INTERIOR_WALL } else { 0.0 };
        Geo { hz: (shot.horizon * h).clamp(-h, h - 1.0), h, base }
    }

    /// Disparity of the ground at canvas row `y`.
    fn ground(&self, y: f32) -> f32 {
        let t = ((y - self.hz) / (self.h - self.hz).max(1.0)).clamp(0.0, 1.0);
        self.base + (NEAR_FLOOR - self.base) * t
    }

    /// Disparity of something standing on the ground at row `y` (never quite black).
    fn standing(&self, y: f32) -> f32 {
        self.ground(y).max(self.base + 0.03)
    }
}

#[allow(clippy::large_enum_variant)]
enum Layer {
    Card(Vec<P>, f32),
    Figure(FigureSpec),
}

/// Render the depth map of `shot` with figures at their start (`end = false`) or
/// movement end positions, over the full canvas.
pub fn render_depth(p: &Project, shot: &Shot, end: bool) -> DepthMap {
    let (w, h) = (p.canvas.width, p.canvas.height);
    let g = Geo::new(p, shot);
    let mut data = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let yy = y as f32 + 0.5;
        let v = if yy < g.hz { g.base } else { g.ground(yy) };
        data.extend(std::iter::repeat_n(v, w as usize));
    }
    let mut dm = DepthMap { w, h, data };

    let u = h as f32 / 1080.0;
    let pitch = shot.camera.angle.pitch();
    let mut layers: Vec<(f32, Layer)> = vec![];
    for pr in &shot.props {
        let prims: Vec<P> = prop_prims(pr, pitch, u).into_iter().filter(|q| !matches!(q, P::Text { .. })).collect();
        layers.push((pr.y, Layer::Card(prims, g.standing(pr.y))));
    }
    for a in &shot.actors {
        let f = end
            .then(|| FigureSpec::end_of_actor(p, shot, a))
            .flatten()
            .unwrap_or_else(|| FigureSpec::of_actor(p, shot, a));
        layers.push((f.y, Layer::Figure(f)));
    }
    layers.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, l) in layers {
        match l {
            Layer::Card(prims, v) => paint_card(&mut dm, &prims, v),
            Layer::Figure(f) => paint_figure(&mut dm, &f, &g),
        }
    }
    // keep the relief of very near bodies instead of clipping them to white
    let mx = dm.data.iter().copied().fold(0.0f32, f32::max);
    if mx > 1.0 {
        dm.data.iter_mut().for_each(|v| *v /= mx);
    }
    dm
}

fn translucent(fill: &Option<[u8; 4]>, stroke_none: bool) -> bool {
    fill.is_some_and(|c| c[3] < 128) && stroke_none
}

fn paint_card(dm: &mut DepthMap, prims: &[P], v: f32) {
    let Some(mut mask) = Mask::new(dm.w, dm.h) else { return };
    let mut any = false;
    for q in prims {
        let (path, closed, stroke) = match q {
            // translucent fills are shadows / tints, not surfaces
            P::Poly { pts, closed, fill, stroke, .. } => {
                if translucent(fill, stroke.is_none()) {
                    continue;
                }
                (path_of(pts, *closed), *closed, stroke.map(|s| s.width))
            }
            P::Shape { pts, fill, stroke, .. } => {
                if translucent(fill, stroke.is_none()) {
                    continue;
                }
                (path_of(pts, true), true, stroke.map(|s| s.width))
            }
            P::Ellipse { c, r, fill, stroke } => {
                if translucent(fill, stroke.is_none()) {
                    continue;
                }
                let rect = Rect::from_xywh(c[0] - r[0], c[1] - r[1], 2.0 * r[0], 2.0 * r[1]);
                (rect.and_then(PathBuilder::from_oval), true, stroke.map(|s| s.width))
            }
            P::Text { .. } => continue,
        };
        let Some(path) = path else { continue };
        if closed {
            mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
            any = true;
        }
        if let Some(sw) = stroke {
            let st = Stroke {
                width: sw.max(1.0),
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            if let Some(sp) = path.stroke(&st, 1.0) {
                mask.fill_path(&sp, FillRule::Winding, true, Transform::identity());
                any = true;
            }
        }
    }
    if !any {
        return;
    }
    for (d, &c) in dm.data.iter_mut().zip(mask.data()) {
        if c > 0 {
            let a = c as f32 / 255.0;
            *d += (v - *d) * a;
        }
    }
}

fn paint_figure(dm: &mut DepthMap, f: &FigureSpec, g: &Geo) {
    let ss = 2;
    let Some(pt) = f.depth_patch(dm.w, dm.h, ss) else { return };
    let v0 = g.standing(f.y);
    // distance of the figure's ground point, taking the focal length as the canvas height
    let z0 = (g.h / f.ppm.max(1e-3)).max(0.5);
    let bw = pt.w * ss;
    for py in 0..pt.h {
        for px in 0..pt.w {
            let (mut n, mut sum) = (0usize, 0.0f32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let dz = pt.dz[(py * ss + sy) * bw + px * ss + sx];
                    if dz.is_finite() {
                        n += 1;
                        sum += v0 * z0 / (z0 + RELIEF * dz).max(0.1);
                    }
                }
            }
            if n == 0 {
                continue;
            }
            let (cx, cy) = (pt.x0 + px as i64, pt.y0 + py as i64);
            if cx < 0 || cy < 0 || cx >= dm.w as i64 || cy >= dm.h as i64 {
                continue;
            }
            let a = n as f32 / (ss * ss) as f32;
            let v = (sum / n as f32).max(0.0);
            let d = &mut dm.data[cy as usize * dm.w as usize + cx as usize];
            *d += (v - *d) * a;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::sample_project;

    #[test]
    fn ground_gradient_and_figures() {
        let p = sample_project();
        let shot = &p.shots[0];
        let dm = render_depth(&p, shot, false);
        assert_eq!((dm.w, dm.h), (p.canvas.width, p.canvas.height));
        // near = white at the bottom, far = black at the top of an exterior
        assert!(dm.get(5, dm.h - 1) > 0.85);
        assert!(dm.get(5, 2) < 0.05);
        // the walking character is a solid volume brighter than the floor behind it
        let a = &shot.actors[0];
        let f = FigureSpec::of_actor(&p, shot, a);
        let chest = dm.get(f.x as u32, (f.y - 1.3 * f.ppm) as u32);
        let floor_there = Geo::new(&p, shot).ground(f.y - 1.3 * f.ppm);
        assert!(chest > floor_there + 0.1, "chest {chest} floor {floor_there}");
        // body relief: values vary across the figure (rounded, not flat)
        let mut vals = vec![];
        for dx in -12..=12 {
            vals.push(dm.get((f.x + dx as f32 * 0.01 * f.ppm) as u32, (f.y - 1.3 * f.ppm) as u32));
        }
        let (mn, mx) = vals.iter().fold((1.0f32, 0.0f32), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(mx - mn > 0.003, "flat body {mn}..{mx}");
        // the end frame moves the figure
        assert_ne!(dm, render_depth(&p, shot, true));
    }

    #[test]
    fn png_8_and_16_bit() {
        let p = sample_project();
        let dm = render_depth(&p, &p.shots[1], false).crop([0, 0, 64, 32]);
        let a = dm.to_png(false).unwrap();
        let b = dm.to_png(true).unwrap();
        assert_eq!(&a[1..4], b"PNG");
        let dec = png::Decoder::new(std::io::Cursor::new(b)).read_info().unwrap();
        assert_eq!(dec.info().bit_depth, png::BitDepth::Sixteen);
        assert_eq!(dec.info().color_type, png::ColorType::Grayscale);
    }
}
