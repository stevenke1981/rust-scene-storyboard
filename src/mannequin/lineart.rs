//! Line-art renderer.
//!
//! Every body volume is tessellated into a parametric grid. A small software
//! z-buffer of all volumes provides visibility and the white body fill; the
//! contour generators of each volume (where the surface turns away from the
//! viewer) are extracted with marching squares in parameter space, smoothed,
//! and only their visible parts are kept. Because hidden contours are removed
//! with the z-buffer, overlapping volumes read as one continuous surface: the
//! outline follows their union, and interior lines appear only where one part
//! passes in front of another (thigh over calf, arm over torso, under the bust...).

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::mannequin::body::{Body, Volume};
use crate::mannequin::math::{Vec3, v3};
use crate::mannequin::render::View;

/// Grid columns around every volume.
const NU: usize = 48;
pub const EMPTY: u8 = 255;

#[derive(Clone, Copy, Debug)]
enum Geo {
    Ell { c: Vec3, ax: [Vec3; 3], inv: [Vec3; 3] },
    Cone { a: Vec3, ra: f32, b: Vec3, rb: f32, d: Vec3, e1: Vec3, e2: Vec3, phi: f32, flat: Option<(Vec3, f32)> },
}

fn sphere(c: Vec3, r: f32) -> Geo {
    let r = r.max(1e-5);
    let ax = [Vec3::X * r, Vec3::Y * r, Vec3::Z * r];
    let inv = [Vec3::X * (1.0 / r), Vec3::Y * (1.0 / r), Vec3::Z * (1.0 / r)];
    Geo::Ell { c, ax, inv }
}

impl Geo {
    fn new(v: &Volume) -> Geo {
        match *v {
            Volume::Ellipsoid { center, axes } => {
                let ax = [axes.x, axes.y, axes.z];
                let inv = ax.map(|a| a * (1.0 / a.dot(a).max(1e-12)));
                Geo::Ell { c: center, ax, inv }
            }
            Volume::Capsule { a, ra, b, rb, flat } => {
                let l = (b - a).length();
                if l < 1e-5 || (ra - rb).abs() >= l * 0.98 {
                    let (c, r) = if ra >= rb { (a, ra) } else { (b, rb) };
                    let Some((u, k)) = flat else { return sphere(c, r) };
                    let m = |e: Vec3| (e + u * ((k - 1.0) * u.dot(e))) * r.max(1e-5);
                    let ax = [m(Vec3::X), m(Vec3::Y), m(Vec3::Z)];
                    let inv = ax.map(|a| a * (1.0 / a.dot(a).max(1e-12)));
                    return Geo::Ell { c, ax, inv };
                }
                let d = (b - a) * (1.0 / l);
                let t = if d.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
                let e1 = d.cross(t).normalized();
                let e2 = d.cross(e1);
                Geo::Cone { a, ra, b, rb, d, e1, e2, phi: ((ra - rb) / l).asin(), flat }
            }
        }
    }

    /// Parameter values of the grid rows.
    fn rows(&self) -> Vec<f32> {
        match self {
            Geo::Ell { .. } => (0..=24).map(|k| k as f32 / 24.0).collect(),
            Geo::Cone { .. } => {
                let mut v: Vec<f32> = (0..=10).map(|k| k as f32 / 10.0).collect();
                v.extend((1..=3).map(|k| 1.0 + k as f32 / 3.0));
                v.extend((1..=10).map(|k| 2.0 + k as f32 / 10.0));
                v
            }
        }
    }

    /// Point and unit normal at parameter (u in [0, 1) around, v along).
    fn eval(&self, u: f32, v: f32) -> (Vec3, Vec3) {
        let (su, cu) = (TAU * u).sin_cos();
        match *self {
            Geo::Ell { c, ax, inv } => {
                let th = PI * v;
                let (st, ct) = th.sin_cos();
                let s = v3(st * cu, -ct, st * su);
                let p = c + ax[0] * s.x + ax[1] * s.y + ax[2] * s.z;
                let n = (inv[0] * s.x + inv[1] * s.y + inv[2] * s.z).normalized_or(Vec3::Y);
                (p, n)
            }
            Geo::Cone { a, ra, b, rb, d, e1, e2, phi, flat } => {
                let radial = e1 * cu + e2 * su;
                let dir = |th: f32| {
                    let (s, c) = th.sin_cos();
                    radial * c + d * s
                };
                let (p, n) = if v <= 1.0 {
                    let n = dir(-FRAC_PI_2 + (phi + FRAC_PI_2) * v);
                    (a + n * ra, n)
                } else if v <= 2.0 {
                    let n = dir(phi);
                    ((a + n * ra).lerp(b + n * rb, v - 1.0), n)
                } else {
                    let n = dir(phi + (FRAC_PI_2 - phi) * (v - 2.0));
                    (b + n * rb, n)
                };
                match flat {
                    None => (p, n),
                    Some((u, k)) => {
                        let q = p - a;
                        let p2 = a + q + u * ((k - 1.0) * u.dot(q));
                        let n2 = (n + u * ((1.0 / k - 1.0) * u.dot(n))).normalized_or(n);
                        (p2, n2)
                    }
                }
            }
        }
    }
}

/// Signed "facing" value: positive where the surface faces the viewer.
fn facing(view: &View, p: Vec3, n: Vec3) -> f32 {
    if view.perspective { n.dot(view.eye - p) } else { -n.dot(view.fwd) }
}

struct Mesh {
    part: usize,
    geo: Geo,
    vs: Vec<f32>,
    /// Projected vertices (view x, view y, depth).
    scr: Vec<[f32; 3]>,
    g: Vec<f32>,
    pos: Vec<Vec3>,
    /// Bounding sphere.
    center: Vec3,
    radius: f32,
}

impl Mesh {
    fn rows(&self) -> usize {
        self.vs.len()
    }
}

/// A body prepared for drawing from one viewpoint.
pub struct Scene<'a> {
    body: &'a Body,
    view: View,
    meshes: Vec<Mesh>,
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Depth tolerance for visibility tests (world units).
    eps: f32,
}

/// Per-pixel buffers over a view-space rectangle.
pub struct Buffers {
    pub w: usize,
    pub h: usize,
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub depth: Vec<f32>,
    /// Index of the body part covering each pixel ([`EMPTY`] = background).
    pub part: Vec<u8>,
}

impl Buffers {
    fn new(w: usize, h: usize, min: [f32; 2], max: [f32; 2]) -> Buffers {
        Buffers { w, h, min, max, depth: vec![f32::INFINITY; w * h], part: vec![EMPTY; w * h] }
    }

    /// View units to pixel coordinates.
    pub fn to_px(&self, x: f32, y: f32) -> [f32; 2] {
        [
            (x - self.min[0]) / (self.max[0] - self.min[0]) * self.w as f32,
            (self.max[1] - y) / (self.max[1] - self.min[1]) * self.h as f32,
        ]
    }

    fn tri(&mut self, p: [[f32; 3]; 3], id: u8) {
        let edge =
            |a: [f32; 3], b: [f32; 3], c: [f32; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        let area = edge(p[0], p[1], [p[2][0], p[2][1]]);
        if area.abs() < 1e-9 || !area.is_finite() {
            return;
        }
        let inv = 1.0 / area;
        let x0 = p[0][0].min(p[1][0]).min(p[2][0]).floor().max(0.0) as i64;
        let x1 = (p[0][0].max(p[1][0]).max(p[2][0]).ceil() as i64).min(self.w as i64 - 1);
        let y0 = p[0][1].min(p[1][1]).min(p[2][1]).floor().max(0.0) as i64;
        let y1 = (p[0][1].max(p[1][1]).max(p[2][1]).ceil() as i64).min(self.h as i64 - 1);
        for y in y0..=y1 {
            let row = y as usize * self.w;
            for x in x0..=x1 {
                let c = [x as f32 + 0.5, y as f32 + 0.5];
                let w0 = edge(p[1], p[2], c) * inv;
                let w1 = edge(p[2], p[0], c) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                    continue;
                }
                let d = w0 * p[0][2] + w1 * p[1][2] + w2 * p[2][2];
                let i = row + x as usize;
                if d < self.depth[i] {
                    self.depth[i] = d;
                    self.part[i] = id;
                }
            }
        }
    }

    fn get(&self, x: i64, y: i64) -> (f32, u8) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            (f32::INFINITY, EMPTY)
        } else {
            let i = y as usize * self.w + x as usize;
            (self.depth[i], self.part[i])
        }
    }
}

/// A visible line in view units.
#[derive(Clone, Debug)]
pub struct Line {
    pub pts: Vec<[f32; 2]>,
    pub part: usize,
    /// Face guide line (drawn thinner).
    pub guide: bool,
}

impl<'a> Scene<'a> {
    pub fn new(body: &'a Body, view: &View) -> Scene<'a> {
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        let mut meshes = Vec::with_capacity(body.parts.len());
        for (k, part) in body.parts.iter().enumerate() {
            let geo = Geo::new(&part.volume);
            let vs = geo.rows();
            let mut scr = Vec::with_capacity(vs.len() * NU);
            let mut g = Vec::with_capacity(vs.len() * NU);
            let mut pos = Vec::with_capacity(vs.len() * NU);
            for &v in &vs {
                for i in 0..NU {
                    let (p, n) = geo.eval(i as f32 / NU as f32, v);
                    let q = view.project(p);
                    min = [min[0].min(q.x), min[1].min(q.y)];
                    max = [max[0].max(q.x), max[1].max(q.y)];
                    scr.push([q.x, q.y, q.depth]);
                    g.push(facing(view, p, n));
                    pos.push(p);
                }
            }
            let center = pos.iter().fold(Vec3::ZERO, |a, &p| a + p) * (1.0 / pos.len().max(1) as f32);
            let radius = pos.iter().map(|&p| (p - center).length()).fold(0.0, f32::max);
            meshes.push(Mesh { part: k, geo, vs, scr, g, pos, center, radius });
        }
        let (lo, hi) = body.bounds();
        let size = (hi - lo).length().max(0.3);
        Scene { body, view: *view, meshes, min, max, eps: 0.0025 * size }
    }

    /// Rasterise depth and part ids over the view rectangle `[min, max]` at `w x h` pixels.
    pub fn buffers(&self, min: [f32; 2], max: [f32; 2], w: usize, h: usize) -> Buffers {
        let mut buf = Buffers::new(w.max(1), h.max(1), min, max);
        for m in &self.meshes {
            let px: Vec<[f32; 3]> = m
                .scr
                .iter()
                .map(|q| {
                    let p = buf.to_px(q[0], q[1]);
                    [p[0], p[1], q[2]]
                })
                .collect();
            let id = m.part.min(254) as u8;
            for r in 0..m.rows() - 1 {
                for i in 0..NU {
                    let j = (i + 1) % NU;
                    let c = [r * NU + i, r * NU + j, (r + 1) * NU + j, (r + 1) * NU + i];
                    if c.iter().all(|&k| m.g[k] < 0.0) {
                        continue;
                    }
                    buf.tri([px[c[0]], px[c[1]], px[c[2]]], id);
                    buf.tri([px[c[0]], px[c[2]], px[c[3]]], id);
                }
            }
        }
        buf
    }

    /// Visible contour and guide lines, using `buf` for visibility.
    pub fn lines(&self, buf: &Buffers) -> Vec<Line> {
        let mut out = Vec::new();
        for m in &self.meshes {
            for (chain, closed) in contours(m) {
                // Project and smooth in (x, y, depth).
                let mut pts: Vec<[f32; 3]> = chain
                    .iter()
                    .map(|p| {
                        let q = self.view.project(*p);
                        [q.x, q.y, q.depth]
                    })
                    .collect();
                for _ in 0..2 {
                    pts = chaikin(&pts, closed);
                }
                if closed && let Some(&f) = pts.first() {
                    pts.push(f);
                }
                self.visible_runs(buf, &pts, m.part, false, false, &mut out);
            }
            if self.body.parts[m.part].is_head {
                self.head_guides(buf, m, &mut out);
            }
        }
        self.creases(buf, &mut out);
        out
    }

    /// Crease lines where two separate body parts meet at a clear angle (hip
    /// crease, buttock fold, inner thighs, armpits, the back of a bent knee...).
    fn creases(&self, buf: &Buffers, out: &mut Vec<Line>) {
        let parts = &self.body.parts;
        let h = self.eps * 0.2;
        let cos_min = 40f32.to_radians().cos();
        for (ia, a) in self.meshes.iter().enumerate() {
            for b in &self.meshes[ia + 1..] {
                let neck = |k: usize| parts[k].joint == crate::mannequin::skeleton::Joint::Neck;
                if crease_group(a.part, &parts[a.part]) == crease_group(b.part, &parts[b.part])
                    || neck(a.part)
                    || neck(b.part)
                    || (a.center - b.center).length() > a.radius + b.radius
                {
                    continue;
                }
                let vals: Vec<f32> = a.pos.iter().map(|&p| field(&b.geo, p)).collect();
                if !vals.iter().any(|&v| v < 0.0) || !vals.iter().any(|&v| v > 0.0) {
                    continue;
                }
                for (chain, closed) in iso_curves(a, &vals) {
                    // Keep the stretches where the two surfaces meet at an angle.
                    let sharp: Vec<bool> = chain
                        .iter()
                        .map(|&p| {
                            grad(&a.geo, p, h).dot(grad(&b.geo, p, h)) < cos_min
                                && self.crease_kept(&a.geo, &parts[a.part], p)
                                && self.crease_kept(&b.geo, &parts[b.part], p)
                        })
                        .collect();
                    let mut start = 0;
                    let n = chain.len();
                    while start < n {
                        if !sharp[start] {
                            start += 1;
                            continue;
                        }
                        let mut end = start;
                        while end < n && sharp[end] {
                            end += 1;
                        }
                        let mut pts: Vec<[f32; 3]> = chain[start..end]
                            .iter()
                            .map(|p| {
                                let q = self.view.project(*p);
                                [q.x, q.y, q.depth]
                            })
                            .collect();
                        let whole = closed && start == 0 && end == n;
                        for _ in 0..2 {
                            pts = chaikin(&pts, whole);
                        }
                        if whole && let Some(&f) = pts.first() {
                            pts.push(f);
                        }
                        self.visible_runs(buf, &pts, a.part, false, true, out);
                        start = end;
                    }
                }
            }
        }
    }

    /// Breasts and glutes blend smoothly into the torso at the top: only their
    /// lower creases are drawn.
    fn crease_kept(&self, g: &Geo, part: &crate::mannequin::body::Part, p: Vec3) -> bool {
        use crate::mannequin::skeleton::Joint::*;
        match (part.joint, g) {
            (Chest | Pelvis, Geo::Ell { c, ax, .. }) if !part.is_head => {
                let up = self.body.fk.rot(part.joint).y;
                let half = ax.iter().map(|a| a.dot(up).abs()).fold(0.0, f32::max);
                (p - *c).dot(up) < if part.joint == Chest { -0.1 } else { 0.25 } * half
            }
            _ => true,
        }
    }

    /// Split a projected polyline into visible runs.
    fn visible_runs(
        &self,
        buf: &Buffers,
        pts: &[[f32; 3]],
        part: usize,
        guide: bool,
        surface: bool,
        out: &mut Vec<Line>,
    ) {
        if pts.len() < 2 {
            return;
        }
        let closed = pts.len() > 3 && pts[0] == pts[pts.len() - 1];
        let px_len = |a: [f32; 2], b: [f32; 2]| {
            let a = buf.to_px(a[0], a[1]);
            let b = buf.to_px(b[0], b[1]);
            ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
        };
        // Sample densely (about one pixel apart) and classify.
        let mut runs: Vec<Vec<[f32; 2]>> = Vec::new();
        let mut cur: Vec<[f32; 2]> = Vec::new();
        let mut starts_at_begin = false;
        let mut ends_at_end = false;
        let n = pts.len();
        for k in 0..n - 1 {
            let (a, b) = (pts[k], pts[k + 1]);
            let steps = (px_len([a[0], a[1]], [b[0], b[1]]) / 1.0).ceil().clamp(1.0, 600.0) as usize;
            let last_seg = k + 2 == n;
            for i in 0..steps + usize::from(last_seg) {
                let t = i as f32 / steps as f32;
                let q = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
                let vis = if surface { self.surface_visible(buf, q) } else { self.contour_visible(buf, q) };
                if vis {
                    if cur.is_empty() && k == 0 && i == 0 {
                        starts_at_begin = true;
                    }
                    cur.push([q[0], q[1]]);
                    if last_seg && i == steps {
                        ends_at_end = true;
                    }
                } else if !cur.is_empty() {
                    runs.push(std::mem::take(&mut cur));
                }
            }
        }
        if !cur.is_empty() {
            runs.push(cur);
        }
        let mut whole_loop = false;
        if closed && starts_at_begin && ends_at_end {
            if runs.len() == 1 {
                whole_loop = true;
            } else if runs.len() > 1 {
                // Join the run crossing the loop's seam.
                let first = runs.remove(0);
                runs.last_mut().unwrap().extend(first);
            }
        }
        for mut run in runs {
            if !whole_loop {
                // Trim the ends a little so lines stop cleanly where they pass behind
                // another surface (no tiny spurs at junctions).
                trim_start(&mut run, 0.9, &px_len);
                run.reverse();
                trim_start(&mut run, 0.9, &px_len);
            }
            if run.len() < 2 {
                continue;
            }
            let len: f32 = run.windows(2).map(|w| px_len(w[0], w[1])).sum();
            if len > 2.0 {
                out.push(Line { pts: run, part, guide });
            }
        }
    }

    /// A contour point is visible when nothing lies in front of it: the farthest
    /// depth in its pixel neighbourhood must not be nearer than the point.
    fn contour_visible(&self, buf: &Buffers, q: [f32; 3]) -> bool {
        // The four pixels whose centres surround the point.
        let p = buf.to_px(q[0], q[1]);
        let (ix, iy) = ((p[0] - 0.5).floor() as i64, (p[1] - 0.5).floor() as i64);
        let mut far = f32::NEG_INFINITY;
        for dy in 0..=1 {
            for dx in 0..=1 {
                far = far.max(buf.get(ix + dx, iy + dy).0);
            }
        }
        far >= q[2] - self.eps
    }

    /// A point on a front-facing surface is visible when it is the nearest surface.
    fn surface_visible(&self, buf: &Buffers, q: [f32; 3]) -> bool {
        let p = buf.to_px(q[0], q[1]);
        let (d, _) = buf.get(p[0].floor() as i64, p[1].floor() as i64);
        d >= q[2] - self.eps * 4.0
    }

    /// Vertical centre line and eye line on the face.
    fn head_guides(&self, buf: &Buffers, m: &Mesh, out: &mut Vec<Line>) {
        let Geo::Ell { c, ax, inv } = m.geo else { return };
        let curve = |pts: &mut dyn Iterator<Item = Vec3>| -> Vec<Vec<[f32; 3]>> {
            let mut runs = vec![];
            let mut cur = vec![];
            for s in pts {
                let p = c + ax[0] * s.x + ax[1] * s.y + ax[2] * s.z;
                let n = (inv[0] * s.x + inv[1] * s.y + inv[2] * s.z).normalized_or(Vec3::Y);
                if facing(&self.view, p, n) > 0.0 {
                    let q = self.view.project(p);
                    cur.push([q.x, q.y, q.depth]);
                } else if !cur.is_empty() {
                    runs.push(std::mem::take(&mut cur));
                }
            }
            if !cur.is_empty() {
                runs.push(cur);
            }
            runs
        };
        // Centre line from below the chin round over the crown (front half).
        let mut meridian = (0..=60).map(|i| {
            let t = (-75.0 + 165.0 * i as f32 / 60.0).to_radians();
            v3(0.0, t.sin(), t.cos())
        });
        let lat: f32 = -0.16;
        let mut eye = (0..=60).map(move |i| {
            let t = (-100.0 + 200.0 * i as f32 / 60.0).to_radians();
            v3(lat.cos() * t.sin(), lat.sin(), lat.cos() * t.cos())
        });
        for run in curve(&mut meridian).into_iter().chain(curve(&mut eye)) {
            self.visible_runs(buf, &run, m.part, true, true, out);
        }
    }
}

/// Parts in the same group blend into each other without a crease line.
fn crease_group(k: usize, p: &crate::mannequin::body::Part) -> usize {
    use crate::mannequin::skeleton::Joint::*;
    match (p.joint, &p.volume) {
        (Head, _) => 1000,
        (Pelvis | Waist | Chest | Neck, Volume::Capsule { .. }) => 1001,
        // Glutes and breasts: one group each.
        (Pelvis | Chest, Volume::Ellipsoid { .. }) => 3000 + k,
        (j, _) => 2000 + j.index(),
    }
}

/// Signed distance-like field of a volume (negative inside).
fn field(g: &Geo, p: Vec3) -> f32 {
    match *g {
        Geo::Ell { c, ax, inv } => {
            let q = p - c;
            let s = v3(inv[0].dot(q), inv[1].dot(q), inv[2].dot(q));
            let r = (ax[0].length() * ax[1].length() * ax[2].length()).cbrt();
            (s.length() - 1.0) * r
        }
        Geo::Cone { a, ra, b, rb, flat, .. } => {
            let p = match flat {
                None => p,
                Some((u, k)) => {
                    let q = p - a;
                    a + q + u * ((1.0 / k - 1.0) * u.dot(q))
                }
            };
            round_cone(p, a, b, ra, rb)
        }
    }
}

/// Exact signed distance to a round cone (two spheres joined by their tangent cone).
fn round_cone(p: Vec3, a: Vec3, b: Vec3, r1: f32, r2: f32) -> f32 {
    let ba = b - a;
    let l2 = ba.dot(ba).max(1e-12);
    let rr = r1 - r2;
    let a2 = l2 - rr * rr;
    let il2 = 1.0 / l2;
    let pa = p - a;
    let y = pa.dot(ba);
    let z = y - l2;
    let xv = pa * l2 - ba * y;
    let x2 = xv.dot(xv);
    let y2 = y * y * l2;
    let z2 = z * z * l2;
    let k = rr.signum() * rr * rr * x2;
    if z.signum() * a2 * z2 > k {
        return (x2 + z2).sqrt() * il2 - r2;
    }
    if y.signum() * a2 * y2 < k {
        return (x2 + y2).sqrt() * il2 - r1;
    }
    ((x2 * a2 * il2).max(0.0).sqrt() + y * rr) * il2 - r1
}

/// Unit gradient of [`field`] (outward surface normal) by central differences.
fn grad(g: &Geo, p: Vec3, h: f32) -> Vec3 {
    let f = |d: Vec3| field(g, p + d) - field(g, p - d);
    v3(f(Vec3::X * h), f(Vec3::Y * h), f(Vec3::Z * h)).normalized_or(Vec3::Y)
}

/// Remove `amount` pixels of length from the start of a polyline.
fn trim_start(run: &mut Vec<[f32; 2]>, amount: f32, px_len: &dyn Fn([f32; 2], [f32; 2]) -> f32) {
    let mut left = amount;
    while run.len() >= 2 {
        let l = px_len(run[0], run[1]);
        if l > left {
            let t = left / l;
            run[0] = [run[0][0] + (run[1][0] - run[0][0]) * t, run[0][1] + (run[1][1] - run[0][1]) * t];
            return;
        }
        left -= l;
        run.remove(0);
    }
}

/// Chaikin corner cutting on (x, y, depth) points.
fn chaikin(p: &[[f32; 3]], closed: bool) -> Vec<[f32; 3]> {
    let n = p.len();
    if n < 3 {
        return p.to_vec();
    }
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| {
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
    };
    let mut out = Vec::with_capacity(n * 2 + 2);
    if closed {
        for i in 0..n {
            let a = p[i];
            let b = p[(i + 1) % n];
            out.push(mix(a, b, 0.25));
            out.push(mix(a, b, 0.75));
        }
    } else {
        out.push(p[0]);
        for i in 0..n - 1 {
            let a = p[i];
            let b = p[i + 1];
            if i > 0 {
                out.push(mix(a, b, 0.25));
            }
            if i + 2 < n {
                out.push(mix(a, b, 0.75));
            }
        }
        out.push(p[n - 1]);
    }
    out
}

/// Contour generator curves of one mesh as 3D polylines (`true` = closed loop).
fn contours(m: &Mesh) -> Vec<(Vec<Vec3>, bool)> {
    iso_curves(m, &m.g)
}

/// Zero crossings of a per-vertex value over a mesh's parameter grid.
fn iso_curves(m: &Mesh, vals: &[f32]) -> Vec<(Vec<Vec3>, bool)> {
    let rows = m.rows();
    let g = |r: usize, i: usize| vals[r * NU + (i % NU)];
    // Edge ids: horizontal (r, i)-(r, i+1) = 2 * (r * NU + i); vertical (r, i)-(r+1, i) = +1.
    let mut points: HashMap<usize, Vec3> = HashMap::new();
    let mut point = |id: usize| -> Vec3 {
        *points.entry(id).or_insert_with(|| {
            let cell = id / 2;
            let (r, i) = (cell / NU, cell % NU);
            let (r2, i2) = if id.is_multiple_of(2) { (r, i + 1) } else { (r + 1, i) };
            let (ga, gb) = (g(r, i), g(r2, i2));
            let t = if (ga - gb).abs() > 1e-12 { (ga / (ga - gb)).clamp(0.0, 1.0) } else { 0.5 };
            let u = (i as f32 + (i2 as f32 - i as f32) * t) / NU as f32;
            let v = m.vs[r] + (m.vs[r2] - m.vs[r]) * t;
            m.geo.eval(u, v).0
        })
    };
    let mut segs: Vec<(usize, usize)> = Vec::new();
    for r in 0..rows - 1 {
        for i in 0..NU {
            let c = [g(r, i), g(r, i + 1), g(r + 1, i + 1), g(r + 1, i)];
            let s: Vec<bool> = c.iter().map(|&x| x >= 0.0).collect();
            let e = [2 * (r * NU + i), 2 * (r * NU + (i + 1) % NU) + 1, 2 * ((r + 1) * NU + i), 2 * (r * NU + i) + 1];
            let cut = [s[0] != s[1], s[1] != s[2], s[3] != s[2], s[0] != s[3]];
            let ids: Vec<usize> = (0..4).filter(|&k| cut[k]).collect();
            match ids.len() {
                2 => segs.push((e[ids[0]], e[ids[1]])),
                4 => {
                    let center = c.iter().sum::<f32>() >= 0.0;
                    if center == s[0] {
                        segs.push((e[0], e[1]));
                        segs.push((e[2], e[3]));
                    } else {
                        segs.push((e[3], e[0]));
                        segs.push((e[1], e[2]));
                    }
                }
                _ => {}
            }
        }
    }
    // Chain segments through shared edges.
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
    for (k, &(a, b)) in segs.iter().enumerate() {
        adj.entry(a).or_default().push(k);
        adj.entry(b).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut chains = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (a, b) = segs[start];
        let mut fwd = vec![a, b];
        // Extend forwards from b, then backwards from a.
        for dir in 0..2 {
            loop {
                let end = *fwd.last().unwrap();
                let next = adj.get(&end).and_then(|v| v.iter().copied().find(|&k| !used[k]));
                let Some(k) = next else { break };
                used[k] = true;
                let (x, y) = segs[k];
                fwd.push(if x == end { y } else { x });
            }
            if dir == 0 {
                fwd.reverse();
            }
        }
        let closed = fwd.len() > 3 && fwd.first() == fwd.last();
        if closed {
            fwd.pop();
        }
        let pts: Vec<Vec3> = fwd.into_iter().map(&mut point).collect();
        chains.push((pts, closed));
    }
    chains
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mannequin::render::Camera;
    use crate::mannequin::skeleton::{Pose, Proportions, Skeleton};

    #[test]
    fn cone_parametrisation_is_continuous() {
        let g = Geo::new(&Volume::Capsule { a: Vec3::ZERO, ra: 0.1, b: v3(0.0, 0.5, 0.0), rb: 0.05, flat: None });
        for v in [1.0f32, 2.0] {
            let (p0, _) = g.eval(0.3, v - 1e-4);
            let (p1, _) = g.eval(0.3, v + 1e-4);
            assert!((p0 - p1).length() < 1e-3);
        }
        // Normals point outwards.
        for k in 0..30 {
            let (p, n) = g.eval(0.17, k as f32 / 10.0);
            let axis_pt = v3(0.0, p.y.clamp(0.0, 0.5), 0.0);
            assert!(n.dot(p - axis_pt) >= -1e-4);
        }
    }

    #[test]
    fn sphere_contour_is_one_closed_loop() {
        let skel = Skeleton::new(Proportions::default());
        let body = Body::new(&skel, &Pose::rest(&skel));
        let view = View::new(&Camera::default());
        let scene = Scene::new(&body, &view);
        let head = scene.meshes.iter().find(|m| body.parts[m.part].is_head).unwrap();
        let c = contours(head);
        assert_eq!(c.len(), 1);
        assert!(c[0].1, "closed");
        assert!(c[0].0.len() > 20);
    }

    #[test]
    fn rest_pose_has_visible_lines_and_fill() {
        let skel = Skeleton::new(Proportions::default());
        let body = Body::new(&skel, &Pose::rest(&skel));
        let view = View::new(&Camera { yaw: 0.0, pitch: 0.0, ..Camera::default() });
        let scene = Scene::new(&body, &view);
        let buf = scene.buffers(scene.min, scene.max, 200, 400);
        assert!(buf.part.iter().filter(|&&p| p != EMPTY).count() > 2000);
        let lines = scene.lines(&buf);
        assert!(lines.len() > 5);
        assert!(lines.iter().any(|l| l.guide));
        assert!(lines.iter().all(|l| l.pts.iter().all(|p| p[0].is_finite() && p[1].is_finite())));
    }
}
