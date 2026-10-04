//! Camera, projection and the backend-independent mannequin drawing list.
//!
//! The posed body is turned into line art by [`crate::mannequin::lineart`]: visible contour
//! polylines plus a per-pixel fill mask. Together with optional floor decoration
//! this forms a [`DrawList`] that egui draws on screen and tiny-skia rasterises
//! for PNG export, so the exported images match the viewport exactly.

use serde::{Deserialize, Serialize};

use crate::mannequin::body::{Body, Volume};
use crate::mannequin::lineart::{EMPTY, Scene};
use crate::mannequin::math::{Mat3, Vec3, v3};
use crate::mannequin::skeleton::Joint;

pub type Rgba = [u8; 4];

pub const INK: Rgba = [20, 20, 22, 255];
pub const PAPER: Rgba = [255, 255, 255, 255];
pub const ACCENT: Rgba = [240, 128, 20, 255];

/// Orbit camera around a target point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    /// Rotation around the vertical axis in degrees (0 = looking at the figure's front).
    pub yaw: f32,
    /// Elevation in degrees (positive = looking down from above).
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
    /// Vertical field of view in degrees (perspective) / framing reference (orthographic).
    pub fov: f32,
    pub perspective: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { yaw: 30.0, pitch: 12.0, distance: 4.2, target: [0.0, 0.85, 0.0], fov: 30.0, perspective: true }
    }
}

/// A camera resolved into a view basis.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
    pub focal: f32,
    pub ortho_half: f32,
    pub perspective: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Projected {
    pub x: f32,
    pub y: f32,
    /// Distance along the view direction (larger = further away).
    pub depth: f32,
    /// World-units to view-units scale at this point.
    pub scale: f32,
}

impl View {
    pub fn new(cam: &Camera) -> View {
        let yaw = cam.yaw.to_radians();
        let pitch = cam.pitch.clamp(-89.0, 89.0).to_radians();
        let target = Vec3::from_array(cam.target);
        let dir = v3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos());
        let eye = target + dir * cam.distance.max(0.1);
        let fwd = -dir;
        let right = fwd.cross(Vec3::Y).normalized_or(Vec3::X);
        let up = right.cross(fwd);
        let half = (cam.fov.clamp(5.0, 120.0).to_radians() * 0.5).tan();
        View {
            eye,
            right,
            up,
            fwd,
            focal: 1.0 / half,
            ortho_half: cam.distance.max(0.1) * half,
            perspective: cam.perspective,
        }
    }

    /// Project a world point into view units: the visible frame spans y in [-1, 1].
    pub fn project(&self, p: Vec3) -> Projected {
        let d = p - self.eye;
        let depth = d.dot(self.fwd);
        let scale = if self.perspective { self.focal / depth.max(0.05) } else { 1.0 / self.ortho_half };
        Projected { x: d.dot(self.right) * scale, y: d.dot(self.up) * scale, depth, scale }
    }

    /// Inverse projection at a given view depth.
    pub fn unproject(&self, x: f32, y: f32, depth: f32) -> Vec3 {
        let k = if self.perspective { depth.max(0.05) / self.focal } else { self.ortho_half };
        self.eye + self.right * (x * k) + self.up * (y * k) + self.fwd * depth
    }

    /// Unit vector from a world point towards the viewer.
    pub fn to_viewer(&self, p: Vec3) -> Vec3 {
        if self.perspective { (self.eye - p).normalized() } else { -self.fwd }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloorStyle {
    #[default]
    None,
    Shadow,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub floor: FloorStyle,
    pub highlight: Option<Joint>,
    /// Outline width as a fraction of the frame height.
    pub line_width: f32,
    /// Paper is transparent (affects the shadow colour only).
    pub transparent: bool,
    /// Produce the white body fill mask (needed over grids / transparent paper).
    pub fill: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style { floor: FloorStyle::None, highlight: None, line_width: 0.0036, transparent: false, fill: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// Width as a fraction of the frame height (view units / 2).
    pub width: f32,
    pub color: Rgba,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prim {
    /// Points in view units (+y up).
    pub pts: Vec<[f32; 2]>,
    pub fill: Option<Rgba>,
    pub stroke: Option<Stroke>,
    pub closed: bool,
    /// Body part this primitive belongs to (for picking).
    pub joint: Option<Joint>,
}

/// Per-pixel body coverage over a view-space rectangle (white paper fill + picking).
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub w: usize,
    pub h: usize,
    /// Joint index per pixel, [`MASK_EMPTY`] for background.
    pub joints: Vec<u8>,
    pub highlight: Option<Joint>,
}

pub const MASK_EMPTY: u8 = 255;
pub const HIGHLIGHT_FILL: Rgba = [255, 228, 196, 255];

impl Mask {
    pub fn joint_at(&self, x: f32, y: f32) -> Option<Joint> {
        let px = ((x - self.min[0]) / (self.max[0] - self.min[0]) * self.w as f32).floor();
        let py = ((self.max[1] - y) / (self.max[1] - self.min[1]) * self.h as f32).floor();
        if px < 0.0 || py < 0.0 || px >= self.w as f32 || py >= self.h as f32 {
            return None;
        }
        let j = self.joints[py as usize * self.w + px as usize];
        Joint::ALL.get(j as usize).copied()
    }

    /// RGBA colour of every pixel (unpremultiplied): white body, tinted highlight.
    pub fn rgba(&self) -> Vec<u8> {
        let hl = self.highlight.map(|j| j.index() as u8);
        let mut out = Vec::with_capacity(self.joints.len() * 4);
        for &j in &self.joints {
            let c = if j == MASK_EMPTY {
                [0, 0, 0, 0]
            } else if Some(j) == hl {
                HIGHLIGHT_FILL
            } else {
                PAPER
            };
            out.extend_from_slice(&c);
        }
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct DrawList {
    /// Floor decoration, drawn below the body.
    pub under: Vec<Prim>,
    /// Body fill.
    pub mask: Option<Mask>,
    /// Body lines, drawn on top.
    pub prims: Vec<Prim>,
    /// Bounds of the figure (excluding floor decoration) in view units.
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl DrawList {
    pub fn is_empty(&self) -> bool {
        self.prims.is_empty()
    }

    /// Body part under a view-space point.
    pub fn pick(&self, x: f32, y: f32) -> Option<Joint> {
        self.mask.as_ref()?.joint_at(x, y)
    }
}

/// Convex hull (Andrew's monotone chain), counter-clockwise.
pub fn convex_hull(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    pts.retain(|p| p[0].is_finite() && p[1].is_finite());
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-7 && (a[1] - b[1]).abs() < 1e-7);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lower: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Outline of a projected ellipsoid (weak perspective at its centre).
fn ellipse_outline(view: &View, center: Vec3, axes: &Mat3, n: usize) -> (Vec<[f32; 2]>, Projected) {
    let c = view.project(center);
    // 2x3 projection of the axes, then the 2x2 shape matrix A = P P^T.
    let px = [axes.x.dot(view.right), axes.y.dot(view.right), axes.z.dot(view.right)];
    let py = [axes.x.dot(view.up), axes.y.dot(view.up), axes.z.dot(view.up)];
    let a = px[0] * px[0] + px[1] * px[1] + px[2] * px[2];
    let b = px[0] * py[0] + px[1] * py[1] + px[2] * py[2];
    let d = py[0] * py[0] + py[1] * py[1] + py[2] * py[2];
    // Eigen decomposition of [[a, b], [b, d]].
    let tr = 0.5 * (a + d);
    let det = (0.25 * (a - d) * (a - d) + b * b).sqrt();
    let l1 = (tr + det).max(1e-12);
    let l2 = (tr - det).max(1e-12);
    let (ex, ey) = if b.abs() > 1e-9 {
        let v = [l1 - d, b];
        let n = (v[0] * v[0] + v[1] * v[1]).sqrt();
        (v[0] / n, v[1] / n)
    } else if a >= d {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let (r1, r2) = (l1.sqrt() * c.scale, l2.sqrt() * c.scale);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let (s, co) = t.sin_cos();
        pts.push([c.x + ex * r1 * co - ey * r2 * s, c.y + ey * r1 * co + ex * r2 * s]);
    }
    (pts, c)
}

/// How the figure is placed in the output rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Framing {
    /// The camera's own framing (view y in [-1, 1] fills the height).
    Camera,
    /// Fit the figure into the rectangle with a relative margin.
    Fit { margin: f32 },
}

/// Build the drawing list for a posed body shown in the output rectangle
/// `(x, y, w, h)` (output units, e.g. screen points or pixels). `res` is the number
/// of raster pixels per output unit used for visibility and the fill mask.
/// Returns the list and the frame that maps it into the rectangle.
pub fn draw_body(
    body: &Body,
    view: &View,
    style: &Style,
    framing: Framing,
    rect: [f32; 4],
    res: f32,
) -> (DrawList, Frame2) {
    let [x, y, w, h] = rect;
    let scene = Scene::new(body, view);
    let frame = match framing {
        Framing::Camera => Frame2::camera(x, y, w, h),
        Framing::Fit { margin } => Frame2::fit_bounds(scene.min, scene.max, x, y, w, h, margin),
    };
    let a = frame.from_px([x, y + h]);
    let b = frame.from_px([x + w, y]);
    let (pw, ph) = (((w * res).round() as usize).clamp(1, 8192), ((h * res).round() as usize).clamp(1, 8192));
    let buf = scene.buffers(a, b, pw, ph);

    let mut list = DrawList { min: scene.min, max: scene.max, ..DrawList::default() };
    let lw = style.line_width;
    let s = (body.fk.tip(Joint::Head).y - body.min_y()).max(0.5) / 1.7;
    match style.floor {
        FloorStyle::Grid => {
            push_grid(&mut list.under, body, view, lw);
        }
        FloorStyle::Shadow => push_shadows(&mut list.under, body, view, style, s),
        FloorStyle::None => {}
    }
    if style.fill {
        let joints = buf
            .part
            .iter()
            .map(|&p| if p == EMPTY { MASK_EMPTY } else { body.parts[p as usize].joint.index() as u8 })
            .collect();
        list.mask = Some(Mask { min: a, max: b, w: pw, h: ph, joints, highlight: style.highlight });
    }
    for line in scene.lines(&buf) {
        let joint = body.parts[line.part].joint;
        let hl = style.highlight == Some(joint);
        let width = if line.guide { lw * 0.7 } else { lw } * if hl { 1.25 } else { 1.0 };
        let color = if hl {
            ACCENT
        } else if line.guide {
            [60, 60, 64, 255]
        } else {
            INK
        };
        list.prims.push(Prim {
            pts: line.pts,
            fill: None,
            stroke: Some(Stroke { width, color }),
            closed: false,
            joint: Some(joint),
        });
    }
    (list, frame)
}

fn push_grid(out: &mut Vec<Prim>, body: &Body, view: &View, lw: f32) {
    let pelvis = body.fk.pos(Joint::Pelvis);
    let (cx, cz) = ((pelvis.x / 0.25).round() * 0.25, (pelvis.z / 0.25).round() * 0.25);
    let n = 8;
    let ext = n as f32 * 0.25;
    for i in -n..=n {
        let o = i as f32 * 0.25;
        let col: Rgba = if i == 0 { [178, 183, 192, 255] } else { [216, 219, 224, 255] };
        for (a, b) in [
            (v3(cx - ext, 0.0, cz + o), v3(cx + ext, 0.0, cz + o)),
            (v3(cx + o, 0.0, cz - ext), v3(cx + o, 0.0, cz + ext)),
        ] {
            let mut pts = Vec::new();
            for k in 0..=16 {
                let p = view.project(a.lerp(b, k as f32 / 16.0));
                if p.depth > 0.1 {
                    pts.push([p.x, p.y]);
                }
            }
            if pts.len() > 1 {
                out.push(Prim {
                    pts,
                    fill: None,
                    stroke: Some(Stroke { width: lw * 0.45, color: col }),
                    closed: false,
                    joint: None,
                });
            }
        }
    }
}

fn push_shadows(list: &mut Vec<Prim>, body: &Body, view: &View, style: &Style, s: f32) {
    let reach = 0.22 * s;
    for part in &body.parts {
        let h = part.volume.min_y();
        if h > reach {
            continue;
        }
        let k = (1.0 - (h / reach).max(0.0)).powf(1.5);
        let (center, axes) = match part.volume {
            Volume::Capsule { a, ra, b, rb, .. } => {
                let fa = v3(a.x, 0.0, a.z);
                let fb = v3(b.x, 0.0, b.z);
                let along = fb - fa;
                let len = along.length();
                let dir = along.normalized_or(Vec3::X);
                let side = Vec3::Y.cross(dir);
                let r = ra.max(rb) * 1.1;
                (fa.lerp(fb, 0.5), Mat3::from_cols(dir * (len * 0.5 + r), Vec3::Y * 0.001, side * r))
            }
            Volume::Ellipsoid { center, axes } => {
                let flat = |v: Vec3| v3(v.x, 0.0, v.z) * 1.1;
                (v3(center.x, 0.0, center.z), Mat3::from_cols(flat(axes.x), flat(axes.y), flat(axes.z)))
            }
        };
        // Collapse to a floor ellipse: use the two largest horizontal extents.
        let ex = crate::mannequin::body::extent(&axes, Vec3::X);
        let ez = crate::mannequin::body::extent(&axes, Vec3::Z);
        if ex < 1e-4 && ez < 1e-4 {
            continue;
        }
        let (pts, _) = ellipse_outline(view, center, &axes, 32);
        let alpha = (k * if style.transparent { 60.0 } else { 46.0 }) as u8;
        list.push(Prim { pts, fill: Some([60, 64, 72, alpha]), stroke: None, closed: true, joint: None });
    }
}

/// Build the drawing list for a body placed with an explicit `frame` (view units ->
/// output units). Used by the storyboard, where every character is positioned and
/// scaled independently on a shared canvas. The fill mask only covers the figure's
/// own bounds (plus a small margin) at `res` raster pixels per output unit.
/// Added for rust-scene-storyboard.
pub fn draw_body_frame(body: &Body, view: &View, style: &Style, frame: &Frame2, res: f32) -> DrawList {
    let scene = Scene::new(body, view);
    let pad = 0.02 * (scene.max[1] - scene.min[1]).abs().max(0.1);
    let a = [scene.min[0] - pad, scene.min[1] - pad];
    let b = [scene.max[0] + pad, scene.max[1] + pad];
    let w_out = (b[0] - a[0]) * frame.scale;
    let h_out = (b[1] - a[1]) * frame.scale;
    let pw = ((w_out * res).round() as usize).clamp(1, 4096);
    let ph = ((h_out * res).round() as usize).clamp(1, 4096);
    let buf = scene.buffers(a, b, pw, ph);
    let mut list = DrawList { min: scene.min, max: scene.max, ..DrawList::default() };
    let lw = style.line_width;
    if style.fill {
        let joints = buf
            .part
            .iter()
            .map(|&p| if p == EMPTY { MASK_EMPTY } else { body.parts[p as usize].joint.index() as u8 })
            .collect();
        list.mask = Some(Mask { min: a, max: b, w: pw, h: ph, joints, highlight: style.highlight });
    }
    for line in scene.lines(&buf) {
        let joint = body.parts[line.part].joint;
        let hl = style.highlight == Some(joint);
        let width = if line.guide { lw * 0.7 } else { lw } * if hl { 1.25 } else { 1.0 };
        let color = if hl {
            ACCENT
        } else if line.guide {
            [60, 60, 64, 255]
        } else {
            INK
        };
        list.prims.push(Prim {
            pts: line.pts,
            fill: None,
            stroke: Some(Stroke { width, color }),
            closed: false,
            joint: Some(joint),
        });
    }
    list
}

/// Maps view units to pixels: `px = ox + x * scale`, `py = oy - y * scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame2 {
    pub ox: f32,
    pub oy: f32,
    pub scale: f32,
}

impl Frame2 {
    /// The camera's own framing: view y in [-1, 1] fills the height.
    pub fn camera(x: f32, y: f32, w: f32, h: f32) -> Frame2 {
        Frame2 { ox: x + w * 0.5, oy: y + h * 0.5, scale: h * 0.5 }
    }

    /// Fit the figure bounds into the rectangle with a relative margin.
    pub fn fit(list: &DrawList, x: f32, y: f32, w: f32, h: f32, margin: f32) -> Frame2 {
        Frame2::fit_bounds(list.min, list.max, x, y, w, h, margin)
    }

    /// Fit view-space bounds into the rectangle with a relative margin.
    pub fn fit_bounds(min: [f32; 2], max: [f32; 2], x: f32, y: f32, w: f32, h: f32, margin: f32) -> Frame2 {
        let list = DrawList { min, max, ..DrawList::default() };
        let bw = (list.max[0] - list.min[0]).max(1e-3);
        let bh = (list.max[1] - list.min[1]).max(1e-3);
        if !bw.is_finite() || !bh.is_finite() {
            return Frame2::camera(x, y, w, h);
        }
        let scale = ((w * (1.0 - 2.0 * margin)) / bw).min((h * (1.0 - 2.0 * margin)) / bh);
        let cx = 0.5 * (list.min[0] + list.max[0]);
        let cy = 0.5 * (list.min[1] + list.max[1]);
        Frame2 { ox: x + w * 0.5 - cx * scale, oy: y + h * 0.5 + cy * scale, scale }
    }

    pub fn to_px(&self, p: [f32; 2]) -> [f32; 2] {
        [self.ox + p[0] * self.scale, self.oy - p[1] * self.scale]
    }

    pub fn from_px(&self, px: [f32; 2]) -> [f32; 2] {
        [(px[0] - self.ox) / self.scale, (self.oy - px[1]) / self.scale]
    }

    /// Stroke width in pixels for a relative stroke, given the output height.
    pub fn stroke_px(&self, rel: f32, frame_h: f32) -> f32 {
        // Strokes are relative to the frame height, not the zoom, so lines stay crisp.
        (rel * frame_h * 0.5 * 1.15).max(0.75)
    }
}

/// Camera that frames the body nicely from the given angles.
pub fn framing_camera(body: &Body, yaw: f32, pitch: f32, perspective: bool) -> Camera {
    let (lo, hi) = body.bounds();
    let c = lo.lerp(hi, 0.5);
    let size = (hi - lo).length().max(0.5);
    // A narrow field of view keeps the perspective distortion mild.
    let cam = Camera { yaw, pitch, distance: 1.0, target: c.to_array(), fov: 22.0, perspective };
    let half = (cam.fov.to_radians() * 0.5).tan();
    Camera { distance: (size * 0.75 / half).max(1.0), ..cam }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mannequin::skeleton::{Pose, Proportions, Skeleton};

    #[test]
    fn projection_round_trip() {
        for perspective in [true, false] {
            let cam = Camera { perspective, ..Camera::default() };
            let v = View::new(&cam);
            let p = v3(0.3, 1.2, -0.4);
            let q = v.project(p);
            let back = v.unproject(q.x, q.y, q.depth);
            assert!((back - p).length() < 1e-4);
        }
    }

    #[test]
    fn front_view_shows_left_side_on_the_right() {
        let cam = Camera { yaw: 0.0, pitch: 0.0, ..Camera::default() };
        let v = View::new(&cam);
        assert!(v.project(v3(0.5, 0.85, 0.0)).x > 0.0);
    }

    #[test]
    fn hull_of_square_points() {
        let h = convex_hull(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]]);
        assert_eq!(h.len(), 4);
        assert!(!h.contains(&[0.5, 0.5]));
    }

    #[test]
    fn draw_list_has_lines_fill_and_picks() {
        let skel = Skeleton::new(Proportions::default());
        let body = Body::new(&skel, &Pose::rest(&skel));
        let cam = Camera { yaw: 0.0, pitch: 0.0, ..Camera::default() };
        let view = View::new(&cam);
        let (list, frame) = draw_body(&body, &view, &Style::default(), Framing::Camera, [0.0, 0.0, 400.0, 400.0], 1.0);
        assert!(list.prims.len() > 5);
        assert!(list.prims.iter().all(|p| p.pts.iter().all(|q| q[0].is_finite() && q[1].is_finite())));
        let mask = list.mask.as_ref().unwrap();
        assert_eq!((mask.w, mask.h), (400, 400));
        let head = view.project(body.fk.pos(Joint::Head) + v3(0.0, 0.12, 0.0));
        assert_eq!(list.pick(head.x, head.y), Some(Joint::Head));
        let px = frame.to_px([head.x, head.y]);
        assert!(px[0] > 0.0 && px[0] < 400.0 && px[1] > 0.0 && px[1] < 400.0);
    }
}
