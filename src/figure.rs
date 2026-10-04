//! Placing posed mannequins on the storyboard canvas.
//!
//! Every character is rendered by the rust-pose-studio line-art renderer with its
//! own orthographic camera: the camera yaw follows the character's facing and the
//! pitch follows the shot's camera angle. The resulting drawing is positioned so
//! that the ground point under the pelvis lands on the actor's canvas position,
//! at a scale of `ppm` canvas pixels per metre.

use crate::mannequin::body::{Body, handle_point, snap_to_floor};
use crate::mannequin::math::{Vec3, v3};
use crate::mannequin::render::{Camera, DrawList, Frame2, Rgba, Style, View, draw_body_frame};
use crate::mannequin::skeleton::{JOINT_COUNT, Joint, Pose, Proportions, Skeleton, forward};
use crate::model::{Actor, Project, Shot};

/// Outline width relative to the canvas height (a little bolder than rust-pose-studio).
pub const LINE_WIDTH: f32 = 0.0042;

/// Everything needed to draw one figure; doubles as a cache key in the GUI.
#[derive(Clone, Debug, PartialEq)]
pub struct FigureSpec {
    pub actor_id: String,
    /// Light "end of movement" figure without fill.
    pub ghost: bool,
    pub props: Proportions,
    pub rot: [[f32; 3]; JOINT_COUNT],
    pub lift: f32,
    pub facing: f32,
    pub pitch: f32,
    /// Ground point in canvas px.
    pub x: f32,
    pub y: f32,
    /// Canvas pixels per metre.
    pub ppm: f32,
    /// Cast colour (ghost lines / highlights).
    pub color: Rgba,
    pub highlight: Option<Joint>,
}

/// A built figure: the line-art drawing list and its view -> canvas frame.
#[derive(Clone, Debug)]
pub struct FigureDraw {
    pub list: DrawList,
    pub frame: Frame2,
}

impl FigureSpec {
    /// Main figure of an actor in a shot.
    pub fn of_actor(p: &Project, shot: &Shot, a: &Actor) -> FigureSpec {
        let cast = p.cast_of(a);
        FigureSpec {
            actor_id: a.id.clone(),
            ghost: false,
            props: cast.proportions(),
            rot: a.pose.rotations(),
            lift: a.pose.lift,
            facing: a.facing,
            pitch: shot.camera.angle.pitch(),
            x: a.x,
            y: a.y,
            ppm: p.base_ppm() * a.scale,
            color: cast.color.rgba(255),
            highlight: None,
        }
    }

    /// Ghost figure at the end of the actor's movement path (if any).
    pub fn ghost_of_actor(p: &Project, shot: &Shot, a: &Actor) -> Option<FigureSpec> {
        let m = &a.movement;
        if !m.is_active() || !m.ghost {
            return None;
        }
        let mut f = FigureSpec::of_actor(p, shot, a);
        let [ex, ey] = a.end_point();
        f.ghost = true;
        f.x = ex;
        f.y = ey;
        f.ppm *= end_scale_factor(p, shot, a);
        if let Some(fc) = m.end_facing {
            f.facing = fc;
        } else if let Some(dir) = travel_facing(a) {
            // Walking characters naturally face their direction of travel at the end.
            if a.pose.preset == "walk" || a.pose.preset == "run" {
                f.facing = dir;
            }
        }
        if !m.end_pose.is_empty()
            && let Some((pose, lift)) = crate::mannequin::actions::library_pose(&f.skeleton(), &m.end_pose)
        {
            f.rot = pose.rot;
            f.lift = lift;
        }
        Some(f)
    }

    pub fn skeleton(&self) -> Skeleton {
        Skeleton::new(self.props)
    }

    /// The pose grounded on the floor (plus lift), root above the ground point.
    pub fn pose(&self, skel: &Skeleton) -> Pose {
        let mut pose = Pose { root: v3(0.0, skel.standing_pelvis_height, 0.0), rot: self.rot };
        snap_to_floor(skel, &mut pose);
        pose.root.y += self.lift.max(0.0);
        pose
    }

    pub fn camera(&self) -> Camera {
        Camera {
            yaw: -self.facing,
            pitch: self.pitch,
            distance: 10.0,
            target: [0.0, 0.9, 0.0],
            fov: 20.0,
            perspective: false,
        }
    }

    pub fn view(&self) -> View {
        View::new(&self.camera())
    }

    /// View units -> canvas px, putting the ground point at (x, y).
    pub fn frame(&self, view: &View) -> Frame2 {
        let s = self.ppm * view.ortho_half;
        let g = view.project(Vec3::ZERO);
        Frame2 { ox: self.x - g.x * s, oy: self.y + g.y * s, scale: s }
    }

    /// Canvas position of a world point.
    pub fn to_canvas(&self, view: &View, frame: &Frame2, p: Vec3) -> [f32; 2] {
        let q = view.project(p);
        frame.to_px([q.x, q.y])
    }

    /// Build the line-art drawing. `canvas_h` is the canvas height (line widths are
    /// relative to it) and `res` the raster pixels per canvas pixel for the fill mask.
    pub fn build(&self, res: f32) -> FigureDraw {
        let skel = self.skeleton();
        let pose = self.pose(&skel);
        let body = Body::new(&skel, &pose);
        let view = self.view();
        let frame = self.frame(&view);
        let style = Style {
            highlight: if self.ghost { None } else { self.highlight },
            line_width: LINE_WIDTH,
            fill: !self.ghost,
            ..Style::default()
        };
        let mut list = draw_body_frame(&body, &view, &style, &frame, res.clamp(0.1, 4.0));
        if self.ghost {
            let c = self.color;
            let ghost =
                [(c[0] as u16 / 2 + 110) as u8, (c[1] as u16 / 2 + 110) as u8, (c[2] as u16 / 2 + 110) as u8, 200];
            for p in &mut list.prims {
                if let Some(s) = &mut p.stroke {
                    s.color = ghost;
                    s.width *= 0.8;
                }
            }
        }
        FigureDraw { list, frame }
    }

    /// Joint handle positions in canvas px with their view depth (for GUI editing).
    pub fn handles(&self) -> Vec<(Joint, [f32; 2], f32)> {
        let skel = self.skeleton();
        let pose = self.pose(&skel);
        let fk = forward(&skel, &pose);
        let view = self.view();
        let frame = self.frame(&view);
        Joint::ALL
            .iter()
            .map(|&j| {
                let q = view.project(handle_point(&skel, &fk, j));
                (j, frame.to_px([q.x, q.y]), q.depth)
            })
            .collect()
    }

    /// Approximate canvas bounds `[x0, y0, x1, y1]` of the figure (from joint positions).
    pub fn bounds(&self) -> [f32; 4] {
        let skel = self.skeleton();
        let pose = self.pose(&skel);
        let fk = forward(&skel, &pose);
        let view = self.view();
        let frame = self.frame(&view);
        let mut b = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
        for j in Joint::ALL {
            for p in [fk.pos(j), fk.tip(j)] {
                let q = self.to_canvas(&view, &frame, p);
                b = [b[0].min(q[0]), b[1].min(q[1]), b[2].max(q[0]), b[3].max(q[1])];
            }
        }
        // Body volumes extend beyond the joints (head, hips, feet).
        let pad = 0.13 * self.ppm * skel.scale();
        let g = self.to_canvas(&view, &frame, Vec3::ZERO);
        [b[0] - pad, b[1] - pad, b[2] + pad, b[3].max(g[1]) + pad * 0.3]
    }

    /// Top-of-head point in canvas px (for name labels and speech bubbles).
    pub fn head_top(&self) -> [f32; 2] {
        let skel = self.skeleton();
        let pose = self.pose(&skel);
        let fk = forward(&skel, &pose);
        let view = self.view();
        let frame = self.frame(&view);
        let h = fk.tip(Joint::Head);
        let c = self.to_canvas(&view, &frame, h);
        let b = self.bounds();
        [c[0], c[1].min(b[1] + 0.13 * self.ppm * skel.scale())]
    }

    /// Unproject a canvas point at a given view depth into the world.
    pub fn unproject(&self, px: [f32; 2], depth: f32) -> Vec3 {
        let view = self.view();
        let frame = self.frame(&view);
        let v = frame.from_px(px);
        view.unproject(v[0], v[1], depth)
    }
}

/// Size factor of the end figure relative to the start, from depth (y relative to
/// the horizon): characters walking towards the camera get bigger.
pub fn end_scale_factor(p: &Project, shot: &Shot, a: &Actor) -> f32 {
    if !a.movement.depth_scale {
        return 1.0;
    }
    let hz = shot.horizon * p.canvas.height as f32;
    let [_, ey] = a.end_point();
    let (d0, d1) = (a.y - hz, ey - hz);
    if d0 < 10.0 || d1 < 10.0 {
        return 1.0;
    }
    (d1 / d0).clamp(0.25, 4.0)
}

/// Facing (degrees, storyboard convention) of the overall travel direction.
pub fn travel_facing(a: &Actor) -> Option<f32> {
    let pts = a.path_points();
    let (s, e) = (pts.first()?, pts.last()?);
    let (dx, dy) = (e[0] - s[0], e[1] - s[1]);
    if dx.abs() < 1.0 && dy.abs() < 1.0 {
        return None;
    }
    // Screen right = 90, towards camera (down the screen) = 0, away = 180.
    Some(dx.atan2(dy).to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mannequin::actions::action_pose;

    fn spec(facing: f32) -> FigureSpec {
        let props = Proportions::default();
        let (pose, _) = action_pose(&Skeleton::new(props), "point").unwrap();
        FigureSpec {
            actor_id: "a".into(),
            ghost: false,
            props,
            rot: pose.rot,
            lift: 0.0,
            facing,
            pitch: 4.0,
            x: 500.0,
            y: 800.0,
            ppm: 300.0,
            color: [255, 0, 0, 255],
            highlight: None,
        }
    }

    #[test]
    fn ground_point_and_height() {
        let f = spec(0.0);
        let b = f.bounds();
        // Feet on the ground point, head about 1.7 m * ppm above.
        assert!((b[3] - 800.0).abs() < 25.0, "{b:?}");
        let h = b[3] - b[1];
        assert!(h > 300.0 * 1.5 && h < 300.0 * 2.0, "height {h}");
        assert!(b[0] < 500.0 && b[2] > 500.0);
    }

    #[test]
    fn facing_right_points_right() {
        // The "point" pose extends the right arm forward: facing screen right
        // the hand ends up right of the body, facing left it is on the left.
        for (facing, right) in [(90.0, true), (-90.0, false)] {
            let f = spec(facing);
            let hand = f.handles().into_iter().find(|h| h.0 == Joint::HandR).unwrap().1;
            assert_eq!(hand[0] > 500.0 + 100.0, right, "facing {facing}: hand {hand:?}");
            assert_eq!(hand[0] < 500.0 - 100.0, !right, "facing {facing}: hand {hand:?}");
        }
    }

    #[test]
    fn build_produces_lines_inside_bounds() {
        let f = spec(30.0);
        let d = f.build(1.0);
        assert!(d.list.prims.len() > 5);
        let b = f.bounds();
        for p in &d.list.prims {
            for q in &p.pts {
                let c = d.frame.to_px(*q);
                assert!(c[0] > b[0] - 40.0 && c[0] < b[2] + 40.0 && c[1] > b[1] - 40.0 && c[1] < b[3] + 40.0);
            }
        }
    }

    #[test]
    fn travel_direction() {
        let mut a = Actor::default();
        a.movement.enabled = true;
        a.movement.path = vec![[a.x + 300.0, a.y]];
        assert!((travel_facing(&a).unwrap() - 90.0).abs() < 1e-3);
        a.movement.path = vec![[a.x, a.y - 300.0]];
        assert!((travel_facing(&a).unwrap().abs() - 180.0).abs() < 1e-3);
    }
}
