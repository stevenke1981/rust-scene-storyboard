//! Built-in preset poses.
//!
//! The 25 presets are original pose data inspired by common figure-drawing
//! reference pose types (kneeling, sitting, lying, crouching, all-fours...).
//! They are authored with a small builder that works in world space: the torso
//! is oriented with "up/forward" vectors, limbs are placed with two-bone IK
//! targets or bone directions, and the result is snapped onto the floor.
//! Because the presets are generated from the current skeleton they adapt to
//! the chosen body proportions.

use crate::mannequin::body::{Body, snap_to_floor};
use crate::mannequin::math::{Mat3, Vec3, v3};
use crate::mannequin::posing::{ik_limb, orient, set_limb, set_world_rot};
use crate::mannequin::skeleton::{Joint, Pose, Side, Skeleton, forward};

#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    /// Suggested viewing angles for thumbnails / contact sheets.
    pub yaw: f32,
    pub pitch: f32,
    pub pose: Pose,
}

pub(crate) fn d(x: f32, y: f32, z: f32) -> Vec3 {
    v3(x, y, z).normalized()
}

/// `v` turned by `deg` degrees about the vertical axis (+Z towards +X).
pub(crate) fn ry(deg: f32, v: Vec3) -> Vec3 {
    Mat3::rot_y(deg.to_radians()).mul_vec(v)
}

/// World-space pose builder.
#[derive(Clone)]
pub(crate) struct B<'a> {
    pub(crate) s: &'a Skeleton,
    pub(crate) k: f32,
    pub(crate) pose: Pose,
}

impl<'a> B<'a> {
    pub(crate) fn new(s: &'a Skeleton) -> Self {
        B { s, k: s.scale(), pose: Pose::rest(s) }
    }
    pub(crate) fn root(&mut self, x: f32, y: f32, z: f32) -> &mut Self {
        self.pose.root = v3(x, y, z) * self.k;
        self
    }
    pub(crate) fn orient(&mut self, j: Joint, up: Vec3, fwd: Vec3) -> &mut Self {
        orient(self.s, &mut self.pose, j, up, fwd);
        self
    }
    /// Pelvis, waist and chest "up" directions sharing one facing direction.
    pub(crate) fn spine(&mut self, pelvis: Vec3, waist: Vec3, chest: Vec3, fwd: Vec3) -> &mut Self {
        self.orient(Joint::Pelvis, pelvis, fwd).orient(Joint::Waist, waist, fwd).orient(Joint::Chest, chest, fwd)
    }
    /// Head orientation; the neck takes the in-between direction.
    pub(crate) fn head(&mut self, up: Vec3, fwd: Vec3) -> &mut Self {
        let fk = forward(self.s, &self.pose);
        let c = fk.rot(Joint::Chest);
        let nu = (c.y + up.normalized()).normalized();
        let nf = (c.z + fwd.normalized()).normalized();
        self.orient(Joint::Neck, nu, nf).orient(Joint::Head, up, fwd)
    }
    pub(crate) fn pos(&self, j: Joint) -> Vec3 {
        forward(self.s, &self.pose).pos(j)
    }
    /// A point given in joint-local coordinates (reference-figure metres).
    pub(crate) fn at(&self, j: Joint, x: f32, y: f32, z: f32) -> Vec3 {
        let fk = forward(self.s, &self.pose);
        fk.pos(j) + fk.rot(j).mul_vec(v3(x, y, z) * self.k)
    }
    /// `base + offset` with the offset in reference-figure metres.
    pub(crate) fn off(&self, base: Vec3, x: f32, y: f32, z: f32) -> Vec3 {
        base + v3(x, y, z) * self.k
    }
    pub(crate) fn limb(&mut self, upper: Joint, du: Vec3, dl: Vec3) -> &mut Self {
        set_limb(self.s, &mut self.pose, upper, du, dl);
        self
    }
    pub(crate) fn ik(&mut self, hinge: Joint, target: Vec3, pole: Vec3) -> &mut Self {
        ik_limb(self.s, &mut self.pose, hinge, target, Some(pole));
        self
    }
    /// Foot: toes along `toe`, top of the foot towards `up`.
    pub(crate) fn foot(&mut self, j: Joint, toe: Vec3, up: Vec3) -> &mut Self {
        let z = toe.normalized();
        let y = up.reject(z).normalized_or(Vec3::Y);
        set_world_rot(self.s, &mut self.pose, j, Mat3::from_cols(y.cross(z), y, z));
        self
    }
    /// Hand: fingers along `finger`, palm facing `palm`.
    pub(crate) fn hand(&mut self, j: Joint, finger: Vec3, palm: Vec3) -> &mut Self {
        let y = -finger.normalized();
        // The back of the left hand is local +X, of the right hand local -X.
        let back = if j.side() == Side::Right { palm } else { -palm };
        let x = back.reject(y).normalized_or(Vec3::X);
        set_world_rot(self.s, &mut self.pose, j, Mat3::from_cols(x, y, x.cross(y)));
        self
    }
    /// Try `steps` values of a parameter in `[lo, hi]` and apply the one whose
    /// `apply` result (an error measure) is smallest.
    pub(crate) fn best(&mut self, lo: f32, hi: f32, steps: usize, apply: impl Fn(&mut B<'a>, f32) -> f32) -> &mut Self {
        let mut best = (f32::INFINITY, lo);
        for i in 0..=steps {
            let t = lo + (hi - lo) * i as f32 / steps as f32;
            let mut trial = self.clone();
            let err = apply(&mut trial, t);
            if err < best.0 {
                best = (err, t);
            }
        }
        apply(self, best.1);
        self
    }
    /// Lowest point of the given body parts.
    pub(crate) fn min_y_of(&self, joints: &[Joint]) -> f32 {
        let body = Body::new(self.s, &self.pose);
        body.parts.iter().filter(|p| joints.contains(&p.joint)).map(|p| p.volume.min_y()).fold(f32::INFINITY, f32::min)
    }
    pub(crate) fn done(&mut self) -> Pose {
        let mut p = self.pose.clone();
        snap_to_floor(self.s, &mut p);
        p
    }
}

use Joint::*;

pub(crate) const SIDES: [(f32, Joint, Joint, Joint); 2] = [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)];
pub(crate) const ARMS: [(f32, Joint, Joint, Joint); 2] =
    [(1.0, UpperArmL, ForearmL, HandL), (-1.0, UpperArmR, ForearmR, HandR)];

impl B<'_> {
    /// Lowest point of the whole figure.
    pub(crate) fn floor(&self) -> f32 {
        Body::new(self.s, &self.pose).min_y()
    }
    /// Folded leg: the thigh aims at `knee` and the shin at `ankle`, both given
    /// relative to the root's floor point (reference metres).
    pub(crate) fn kneel(&mut self, th: Joint, sh: Joint, knee: Vec3, ankle: Vec3) -> &mut Self {
        let _ = sh;
        let base = v3(self.pose.root.x, 0.0, self.pose.root.z);
        let hip = self.pos(th);
        let du = (base + knee * self.k - hip).normalized();
        let knee_at = hip + du * (self.s.length(th));
        let dl = (base + ankle * self.k - knee_at).normalized_or(-du);
        self.limb(th, du, dl)
    }
    /// Place a wrist `y` above the current floor at an (x, z) offset from the shoulder.
    pub(crate) fn hand_floor(&mut self, fa: Joint, dx: f32, dz: f32, pole: Vec3) -> &mut Self {
        let sh = self.pos(fa.parent().unwrap());
        let y = self.floor() + 0.03 * self.k;
        self.ik(fa, v3(sh.x + dx * self.k, y, sh.z + dz * self.k), pole)
    }
    /// Distance between the shoulder and the floor point the hand should reach.
    pub(crate) fn reach_err(&self, fa: Joint, dx: f32, dz: f32, len: f32) -> f32 {
        let sh = self.pos(fa.parent().unwrap());
        let t = v3(sh.x + dx * self.k, self.floor() + 0.03 * self.k, sh.z + dz * self.k);
        ((t - sh).length() - len * self.k).abs()
    }
}

/// All built-in presets for the given skeleton.
pub fn presets(s: &Skeleton) -> Vec<Preset> {
    let mut v = Vec::with_capacity(25);
    let mut add = |name: &'static str, yaw: f32, pitch: f32, pose: Pose| v.push(Preset { name, yaw, pitch, pose });

    // 1. Sitting back on the heels, back arched, looking up (seen from behind).
    {
        let mut b = B::new(s);
        b.root(0.0, 0.30, 0.0).spine(d(0.0, 1.0, 0.9), d(0.0, 1.0, 0.5), d(0.0, 1.0, 0.12), Vec3::Z);
        b.head(d(0.0, 0.85, -0.55), d(0.0, 0.55, 0.85));
        for (sx, th, sh, ft) in SIDES {
            b.kneel(th, sh, v3(sx * 0.17, 0.05, 0.42), v3(sx * 0.13, 0.065, -0.10));
            b.foot(ft, d(-sx * 0.15, -0.3, -1.0), d(0.0, -1.0, -0.3));
        }
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, sx * 0.02, 0.16, d(0.0, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.1, -0.2, 1.0), -Vec3::Y);
        }
        add("Sitting on heels, arching back", -122.0, 15.0, b.done());
    }

    // 2. Kneeling on all fours, torso raised, looking ahead (seen from behind).
    {
        let mut b = B::new(s);
        b.root(0.0, 0.45, 0.0);
        b.best(0.0, 0.9, 90, |b, t| {
            b.spine(d(0.0, t * 0.8, 1.0), d(0.0, t * 0.9, 1.0), d(0.0, t, 1.0), -Vec3::Y);
            for (sx, th, _, ft) in SIDES {
                b.limb(th, d(sx * 0.15, -1.0, 0.42), d(-sx * 0.03, 0.18, -1.0));
                b.foot(ft, d(0.0, -1.0, 0.3), d(0.0, 0.3, 1.0));
            }
            b.reach_err(ForearmL, 0.03, 0.16, 0.50)
        });
        b.head(d(0.0, 1.0, 0.6), d(0.0, -0.15, 1.0));
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, sx * 0.03, 0.16, d(0.0, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.15, 0.0, 1.0), -Vec3::Y);
        }
        add("Kneeling on all fours, torso raised", -155.0, 12.0, b.done());
    }

    // 3. Sitting back on the heels, leaning forward onto the hands in front of the knees.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.30, 0.0);
        b.best(0.0, 1.2, 120, |b, t| {
            b.spine(d(0.0, 1.0, 0.4 + t * 0.4), d(0.0, 1.0, 0.1 + t * 0.6), d(0.0, 1.0, t * 0.7), Vec3::Z);
            for (sx, th, sh, ft) in SIDES {
                b.kneel(th, sh, v3(sx * 0.12, 0.05, 0.42), v3(sx * 0.16, 0.065, -0.08));
                b.foot(ft, d(sx * 0.3, -0.3, -1.0), d(0.0, -1.0, -0.3));
            }
            b.reach_err(ForearmL, -0.07, 0.30, 0.53) + 0.01 * t
        });
        b.head(d(0.0, 1.0, 0.1), d(0.1, -0.2, 1.0));
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, -sx * 0.07, 0.30, d(sx * 0.3, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.15, -0.1, 1.0), -Vec3::Y);
        }
        add("Sitting on heels, hands in front of knees", 45.0, 10.0, b.done());
    }

    // 4. Crawling on hands and knees, head lowered, one foot raised.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.34, 0.0).spine(d(0.0, 0.0, 1.0), d(0.0, 0.08, 1.0), d(0.0, 0.02, 1.0), -Vec3::Y);
        b.head(d(0.0, 0.15, 1.0), d(0.0, -1.0, 0.3));
        b.limb(ThighL, d(0.1, -0.77, -0.64), d(0.0, -0.05, -1.0));
        b.limb(ThighR, d(-0.1, -0.8, -0.55), d(0.0, 0.55, -0.83));
        b.foot(FootL, d(0.0, -0.15, -1.0), -Vec3::Y).foot(FootR, d(0.0, 0.7, -0.6), d(0.0, -0.6, -0.7));
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, sx * 0.08, 0.18, d(sx * 0.8, 0.0, -0.6));
            b.hand(ha, d(-sx * 0.15, 0.0, 1.0), -Vec3::Y);
        }
        add("Crawling, head lowered", -32.0, 10.0, b.done());
    }

    // 5. Lying on the stomach on the elbows, chin in the hands, lower legs up.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), -Vec3::Y);
        b.orient(Waist, d(-1.0, 0.32, 0.0), -Vec3::Y).orient(Chest, d(-0.8, 0.6, 0.0), -Vec3::Y);
        b.head(d(-0.25, 1.0, 0.0), d(-1.0, -0.1, 0.0));
        for (sz, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let chin = b.at(Head, sz * 0.05, -0.02, 0.07);
            b.ik(fa, chin, d(0.0, -1.0, sz * 0.3));
            b.hand(ha, d(0.2, 1.0, -sz * 0.2), d(-1.0, 0.0, 0.0));
        }
        b.limb(ThighL, d(1.0, -0.1, 0.05), d(-0.5, 1.0, -0.2)).limb(ThighR, d(1.0, -0.1, -0.05), d(-0.6, 1.0, 0.18));
        b.foot(FootL, d(-0.2, 1.0, -0.2), d(1.0, 0.3, 0.0)).foot(FootR, d(-0.3, 1.0, 0.2), d(1.0, 0.3, 0.0));
        add("Lying on stomach, chin in hands", -158.0, 8.0, b.done());
    }

    // 6. Lying on the right side, propped on the forearm, top hand on the thigh.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.165, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), Vec3::Z);
        let k = b.k;
        let upper_dir = d(0.1, -1.0, 0.15);
        b.best(10.0, 85.0, 150, |b, a| {
            let a = a.to_radians();
            b.orient(Waist, d(-1.0, 0.8 * a.sin(), 0.0), Vec3::Z).orient(Chest, d(-a.cos(), a.sin(), 0.0), Vec3::Z);
            (b.pos(UpperArmR).y + upper_dir.y * b.s.length(UpperArmR) - 0.045 * k).abs()
        });
        b.head(d(-0.1, 1.0, 0.0), d(0.1, -0.1, 1.0));
        b.limb(UpperArmR, upper_dir, d(0.65, -0.02, 1.0));
        b.hand(HandR, d(0.65, 0.0, 1.0), -Vec3::Y);
        b.limb(ThighR, d(0.7, -0.02, 0.7), d(0.6, -0.02, -0.8));
        b.limb(ThighL, d(0.75, -0.12, 0.8), d(0.7, -0.06, -0.6));
        b.foot(FootR, d(0.25, 0.0, 1.0), d(-1.0, 0.0, 0.0)).foot(FootL, d(0.3, -0.1, 1.0), d(-1.0, 0.0, 0.0));
        let knee = b.pos(ShinL);
        b.ik(ForearmL, b.off(knee, -0.10, 0.06, 0.03), d(0.0, 0.3, -1.0));
        b.hand(HandL, d(1.0, -0.2, 0.1), d(0.0, -1.0, 0.0));
        add("Side-lying, propped on forearm", 12.0, 24.0, b.done());
    }

    // 7. Side-saddle sitting, legs folded to the right, hand on the knee.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.06, 1.0, 0.0), d(0.16, 1.0, 0.02), d(0.08, 1.0, 0.02), d(0.15, 0.0, 1.0));
        b.head(d(-0.12, 1.0, 0.0), d(-0.2, -0.1, 1.0));
        b.best(-0.3, 0.3, 60, |b, t| {
            b.limb(ThighL, d(0.1, t, 1.0), d(-1.0, -0.02, -0.25));
            b.limb(ThighR, d(-0.4, t + 0.04, 0.9), d(-0.5, -0.02, -1.0));
            (b.min_y_of(&[Pelvis]) - b.min_y_of(&[ThighL, ShinL, ShinR, ThighR])).abs()
        });
        b.foot(FootL, d(-1.0, 0.0, -0.4), d(0.0, 0.3, -1.0)).foot(FootR, d(-0.6, 0.0, -1.0), d(0.0, 0.3, -1.0));
        b.hand_floor(ForearmL, 0.22, -0.04, d(0.0, 0.0, -1.0));
        b.hand(HandL, d(0.6, -0.1, -0.2), d(0.0, -1.0, 0.0));
        let knee = b.pos(ShinL);
        b.ik(ForearmR, b.off(knee, -0.04, 0.09, -0.02), d(-1.0, -0.5, -0.2));
        b.hand(HandR, d(0.3, -0.5, 0.6), d(0.0, -1.0, 0.0));
        add("Side-saddle sitting, hand on knee", 20.0, 10.0, b.done());
    }

    // 8. On all fours, hips high, looking ahead (seen from behind the side).
    {
        let mut b = B::new(s);
        b.root(0.0, 0.50, 0.0).spine(d(0.0, -0.05, 1.0), d(0.0, -0.12, 1.0), d(0.0, -0.05, 1.0), -Vec3::Y);
        b.head(d(0.0, 1.0, 0.8), d(0.0, -0.2, 1.0));
        for (sx, th, sh, ft) in SIDES {
            let hip = b.pos(th);
            b.ik(sh, v3(hip.x + sx * 0.02 * b.k, 0.06 * b.k, hip.z - 0.38 * b.k), d(0.0, -1.0, 1.0));
            b.foot(ft, d(0.0, -0.25, -1.0), d(0.0, -1.0, 0.25));
        }
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, sx * 0.03, 0.10, d(0.0, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.1, -0.1, 1.0), -Vec3::Y);
        }
        add("All fours, looking ahead", -138.0, 8.0, b.done());
    }

    // 9. Wide kneel sitting between the heels, arms straight down in front.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.15, 0.0);
        b.best(0.0, 0.8, 80, |b, t| {
            b.spine(d(0.0, 1.0, 0.1 + t * 0.2), d(0.0, 1.0, 0.05 + t * 0.4), d(0.0, 1.0, t), Vec3::Z);
            for (sx, th, sh, ft) in SIDES {
                b.kneel(th, sh, v3(sx * 0.26, 0.05, 0.34), v3(sx * 0.20, 0.05, -0.08));
                b.foot(ft, d(sx * 0.2, -0.2, -1.0), d(0.0, -1.0, 0.2));
            }
            b.reach_err(ForearmL, -0.10, 0.18, 0.51) + 0.02 * t
        });
        b.head(d(0.0, 1.0, 0.12), d(0.0, -0.35, 1.0));
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, -sx * 0.10, 0.18, d(sx * 0.4, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.15, -0.1, 1.0), -Vec3::Y);
        }
        add("Wide kneel, hands between knees", 0.0, 6.0, b.done());
    }

    // 10. Sitting with one knee up, forearm on the knee, leaning on the other hand.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.0, 1.0, -0.15), d(0.08, 1.0, -0.05), d(0.05, 1.0, 0.0), d(-0.15, 0.0, 1.0));
        b.head(d(-0.08, 1.0, 0.0), d(-0.3, -0.1, 1.0));
        b.ik(ShinR, b.off(Vec3::ZERO, -0.14, 0.075, 0.42), d(-0.2, 1.0, 0.3));
        b.foot(FootR, d(-0.1, 0.0, 1.0), Vec3::Y);
        b.kneel(ThighL, ShinL, v3(0.36, 0.06, 0.30), v3(-0.10, 0.05, 0.34));
        b.foot(FootL, d(-1.0, 0.0, 0.1), d(0.0, 0.3, 1.0));
        let knee = b.pos(ShinR);
        b.ik(ForearmR, b.off(knee, 0.06, -0.06, 0.10), d(-1.0, 0.4, 0.0));
        b.hand(HandR, d(0.4, -1.0, 0.2), d(0.0, 0.0, -1.0));
        b.hand_floor(ForearmL, 0.16, -0.16, d(0.0, 0.0, 1.0));
        b.hand(HandL, d(0.5, 0.0, -0.6), -Vec3::Y);
        add("Sitting, forearm on raised knee", 22.0, 10.0, b.done());
    }

    // 11. All fours, hips high, head low (seen from the front side).
    {
        let mut b = B::new(s);
        b.root(0.0, 0.52, 0.0).spine(d(0.0, -0.22, 1.0), d(0.0, -0.3, 1.0), d(0.0, -0.18, 1.0), -Vec3::Y);
        b.head(d(0.0, -0.05, 1.0), d(0.0, -1.0, -0.05));
        b.limb(ThighR, d(-0.05, -1.0, -0.05), d(0.0, -0.08, -1.0));
        b.limb(ThighL, d(0.05, -1.0, 0.05), d(0.0, 0.55, -0.85));
        b.foot(FootR, d(0.0, -0.15, -1.0), -Vec3::Y).foot(FootL, d(0.0, 0.5, -0.85), d(0.0, -0.85, -0.5));
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, sx * 0.03, 0.12, d(0.0, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.2, 0.0, 1.0), -Vec3::Y);
        }
        add("All fours, head low", -50.0, 14.0, b.done());
    }

    // 12. Side-saddle sitting, one hand behind the head, leaning on the other.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.08, 1.0, 0.0), d(0.2, 1.0, 0.0), d(0.1, 1.0, -0.04), d(0.2, 0.0, 1.0));
        b.head(d(-0.15, 1.0, 0.05), d(-0.15, -0.1, 1.0));
        b.best(-0.3, 0.3, 60, |b, t| {
            b.limb(ThighL, d(0.0, t, 1.0), d(-1.0, -0.02, -0.15));
            b.limb(ThighR, d(-0.5, t + 0.04, 0.85), d(-0.3, -0.02, -1.0));
            (b.min_y_of(&[Pelvis]) - b.min_y_of(&[ThighL, ShinL, ShinR, ThighR])).abs()
        });
        b.foot(FootL, d(-1.0, 0.0, -0.2), d(0.0, 0.3, -1.0)).foot(FootR, d(-0.4, 0.0, -1.0), d(0.0, 0.3, -1.0));
        b.hand_floor(ForearmL, 0.24, -0.02, d(0.0, 0.0, -1.0));
        b.hand(HandL, d(0.6, -0.1, -0.2), d(0.0, -1.0, 0.0));
        let nape = b.at(Head, -0.02, 0.06, -0.10);
        b.ik(ForearmR, nape, d(-0.6, 1.0, 0.1));
        b.hand(HandR, d(0.6, 0.3, -0.4), d(0.0, 0.0, 1.0));
        add("Side-saddle, hand behind head", 22.0, 10.0, b.done());
    }

    // 13. Lying on the stomach, chin in the hands, lower legs up, crossed.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), -Vec3::Y);
        b.orient(Waist, d(-1.0, 0.25, 0.0), -Vec3::Y).orient(Chest, d(-0.85, 0.5, 0.0), -Vec3::Y);
        b.head(d(-0.2, 1.0, 0.0), d(-1.0, -0.05, 0.1));
        for (sz, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let chin = b.at(Head, sz * 0.06, -0.04, 0.06);
            b.ik(fa, chin, d(0.0, -1.0, sz * 0.5));
            b.hand(ha, d(0.0, 1.0, -sz * 0.3), d(-1.0, 0.0, 0.0));
        }
        b.limb(ThighL, d(1.0, -0.08, 0.06), d(-0.08, 1.0, -0.22)).limb(
            ThighR,
            d(1.0, -0.08, -0.06),
            d(-0.12, 1.0, 0.22),
        );
        b.foot(FootL, d(0.1, 1.0, -0.2), d(1.0, -0.1, 0.0)).foot(FootR, d(0.05, 1.0, 0.2), d(1.0, -0.1, 0.0));
        add("Lying on stomach, feet crossed up", -142.0, 12.0, b.done());
    }

    // 14. Sitting on the heels with the knees turned aside, hand on the floor.
    {
        let mut b = B::new(s);
        let a = -68.0;
        b.root(0.0, 0.34, 0.0).orient(Pelvis, d(0.0, 1.0, 0.0), ry(a, Vec3::Z));
        b.orient(Waist, d(0.08, 1.0, 0.0), ry(a * 0.5, Vec3::Z)).orient(Chest, d(0.04, 1.0, -0.03), ry(0.0, Vec3::Z));
        b.head(d(-0.05, 1.0, 0.0), ry(-12.0, d(0.0, -0.12, 1.0)));
        for (sx, th, sh, ft) in SIDES {
            b.kneel(th, sh, ry(a, v3(sx * 0.11, 0.05, 0.38)), ry(a, v3(sx * 0.07 + 0.08, 0.065, -0.05)));
            b.foot(ft, ry(a, d(0.0, -0.3, -1.0)), ry(a, d(0.0, -1.0, -0.3)));
        }
        b.hand_floor(ForearmL, 0.16, 0.0, d(0.0, 0.0, -1.0));
        b.hand(HandL, d(0.5, 0.0, 0.5), -Vec3::Y);
        let knee = b.pos(ShinR);
        let hip = b.pos(ThighR);
        b.ik(ForearmR, hip.lerp(knee, 0.65) + v3(0.0, 0.07 * b.k, 0.0), d(-1.0, -0.2, -0.5));
        b.hand(HandR, ry(a, d(0.0, -0.3, 1.0)), -Vec3::Y);
        add("Sitting on heels, knees turned aside", 25.0, 10.0, b.done());
    }

    // 15. Child's pose with the arms stretched forward (seen from behind, above).
    {
        let mut b = B::new(s);
        b.root(0.0, 0.40, 0.0);
        b.spine(d(0.0, -0.3, 1.0), d(0.0, -0.45, 1.0), d(0.0, -0.3, 1.0), -Vec3::Y);
        for (sx, th, _, ft) in SIDES {
            b.limb(th, d(sx * 0.15, -1.0, 0.7), d(0.0, 0.0, -1.0));
            b.foot(ft, d(0.0, -0.2, -1.0), d(0.0, -1.0, 0.2));
        }
        b.head(d(0.0, -0.2, 1.0), d(0.0, -1.0, -0.2));
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, sx * 0.04, 0.48, d(0.0, 1.0, 0.0));
            b.hand(ha, d(0.0, 0.0, 1.0), -Vec3::Y);
        }
        add("Child's pose, arms forward", -140.0, 32.0, b.done());
    }

    // 16. Mermaid sitting, leaning forward onto both hands, head bowed.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.14, 0.0);
        b.best(0.2, 1.2, 80, |b, t| {
            b.spine(d(0.05, 1.0, 0.2 + t * 0.3), d(0.0, 1.0, 0.1 + t * 0.6), d(-0.05, 1.0, t), d(-0.15, 0.0, 1.0));
            for (sx, th, sh, ft) in SIDES {
                b.kneel(th, sh, v3(sx * 0.10 - 0.08, 0.05, 0.38), v3(0.26 + sx * 0.05, 0.05, -0.02));
                b.foot(ft, d(0.4, -0.2, -1.0), d(0.0, -1.0, 0.0));
            }
            b.reach_err(ForearmL, -0.12, 0.30, 0.50)
        });
        b.head(d(0.0, 0.8, 0.6), d(-0.1, -0.8, 0.6));
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, -sx * 0.12 - 0.04, 0.30, d(sx * 0.4, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.25, -0.1, 1.0), -Vec3::Y);
        }
        add("Mermaid sit, leaning on hands", 40.0, 10.0, b.done());
    }

    // 17. Sitting with the legs to the side, looking aside (seen from behind).
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.1, 1.0, -0.05), d(0.12, 1.0, 0.0), d(0.02, 1.0, 0.0), d(0.1, 0.0, 1.0));
        b.head(d(-0.05, 1.0, 0.0), d(-1.0, -0.08, -0.1));
        b.best(-0.3, 0.3, 60, |b, t| {
            b.limb(ThighL, d(0.15, t, 1.0), d(-1.0, -0.02, -0.4));
            b.limb(ThighR, d(-0.55, t + 0.04, 0.8), d(-0.45, -0.02, -1.0));
            (b.min_y_of(&[Pelvis]) - b.min_y_of(&[ThighL, ShinL, ShinR, ThighR])).abs()
        });
        b.foot(FootL, d(-1.0, 0.0, -0.4), d(0.0, 0.3, -1.0)).foot(FootR, d(-0.5, 0.0, -1.0), d(0.0, 0.3, -1.0));
        b.hand_floor(ForearmL, 0.15, 0.10, d(0.0, 0.0, -1.0));
        b.hand(HandL, d(0.3, 0.0, 0.8), -Vec3::Y);
        let knee = b.pos(ShinL);
        b.ik(ForearmR, b.off(knee, -0.08, 0.08, -0.10), d(-1.0, -0.4, -0.2));
        b.hand(HandR, d(0.3, -0.5, 0.6), -Vec3::Y);
        add("Sitting with legs to the side, from behind", -155.0, 8.0, b.done());
    }

    // 18. Lying on the back, knees up, hands behind the head.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).spine(d(-1.0, 0.0, 0.0), d(-1.0, 0.03, 0.0), d(-1.0, 0.1, 0.0), Vec3::Y);
        b.head(d(-1.0, 0.25, 0.0), d(0.25, 1.0, 0.1));
        for (sz, th, sh, ft) in [(-1.0, ThighL, ShinL, FootL), (1.0, ThighR, ShinR, FootR)] {
            let hip = b.pos(th);
            b.ik(sh, v3(hip.x + 0.48 * b.k, 0.075 * b.k, hip.z + sz * 0.04 * b.k), d(0.0, 1.0, -sz * 0.1));
            b.foot(ft, d(1.0, 0.0, sz * 0.1), Vec3::Y);
        }
        for (sz, fa, ha) in [(-1.0, ForearmL, HandL), (1.0, ForearmR, HandR)] {
            let nape = b.at(Head, -sz * 0.03, 0.06, -0.11);
            b.ik(fa, nape, d(-0.3, 0.6, sz));
            b.hand(ha, d(0.0, -0.3, -sz), d(1.0, 0.0, 0.0));
        }
        add("Lying on back, hands behind head", 115.0, 25.0, b.done());
    }

    // 19. Kneeling with the chest down on the forearms, hips high.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.48, 0.0).spine(d(0.0, -0.3, 1.0), d(0.0, -0.6, 1.0), d(0.0, -0.45, 1.0), -Vec3::Y);
        b.head(d(0.3, 0.1, 1.0), d(0.6, -0.8, 0.1));
        for (sx, th, sh, ft) in SIDES {
            let hip = b.pos(th);
            b.ik(sh, v3(hip.x + sx * 0.04 * b.k, 0.15 * b.k, hip.z - 0.38 * b.k), d(0.0, -1.0, 1.0));
            b.foot(ft, d(0.0, -1.0, 0.2), d(0.0, 0.2, 1.0));
        }
        let floor = b.floor();
        for (sx, ua, fa, ha) in ARMS {
            let sh = b.pos(ua);
            let elbow = v3(sh.x + sx * 0.03 * b.k, floor + 0.04 * b.k, sh.z + 0.05 * b.k);
            let upper = (elbow - sh).normalized();
            b.limb(ua, upper, d(-sx * 0.6, 0.0, 1.0));
            let _ = fa;
            b.hand(ha, d(-sx * 0.6, 0.0, 1.0), -Vec3::Y);
        }
        add("Kneeling, chest down on forearms", -150.0, 20.0, b.done());
    }

    // 20. Sitting, hugging both knees, arms crossed over them.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.0, 1.0, -0.25), d(0.0, 1.0, 0.1), d(0.0, 1.0, 0.3), Vec3::Z);
        b.head(d(0.0, 1.0, 0.1), d(0.0, -0.15, 1.0));
        for (sx, _, sh, ft) in SIDES {
            b.ik(sh, b.off(Vec3::ZERO, sx * 0.09, 0.075, 0.36), d(sx * 0.1, 1.0, 0.3));
            b.foot(ft, d(0.0, 0.0, 1.0), Vec3::Y);
        }
        let mid = b.pos(ShinL).lerp(b.pos(ShinR), 0.5);
        for (sx, _, fa, ha) in ARMS {
            let elbow = b.off(mid, sx * 0.20, -0.06, 0.0);
            let wrist = b.off(mid, -sx * 0.10, -0.03 + sx * 0.02, 0.10);
            let sh = b.pos(fa.parent().unwrap());
            b.ik(fa, wrist, elbow - sh.lerp(wrist, 0.5));
            b.hand(ha, d(-sx, -0.3, -0.2), d(0.0, 0.0, -1.0));
        }
        add("Sitting, hugging both knees", 32.0, 8.0, b.done());
    }

    // 21. Lying on the front, propped on a forearm, head resting on the hand.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.11, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), d(0.0, -1.0, 0.45));
        b.orient(Waist, d(-1.0, 0.12, 0.0), d(0.0, -1.0, 0.4)).orient(Chest, d(-1.0, 0.38, 0.0), d(0.0, -1.0, 0.45));
        b.head(d(-0.6, 1.0, 0.45), d(-0.6, -0.2, 0.8));
        let floor = b.floor();
        let sh = b.pos(UpperArmL);
        let elbow = v3(sh.x - 0.12 * b.k, floor + 0.045 * b.k, sh.z + 0.06 * b.k);
        let cheek = b.at(Head, 0.07, 0.04, 0.04);
        b.limb(UpperArmL, (elbow - sh).normalized(), cheek - elbow);
        b.hand(HandL, (cheek - elbow).normalized(), d(1.0, 0.0, -0.5));
        b.hand_floor(ForearmR, 0.12, 0.34, d(-0.5, 1.0, 0.0));
        b.hand(HandR, d(0.5, 0.0, 1.0), -Vec3::Y);
        b.limb(ThighL, d(1.0, -0.3, 0.45), d(0.6, 0.05, -0.8));
        b.limb(ThighR, d(1.0, -0.3, 0.15), d(0.7, 0.0, -0.7));
        b.foot(FootL, d(0.5, 0.0, -0.8), d(0.0, -1.0, 0.0)).foot(FootR, d(0.6, 0.0, -0.7), d(0.0, -1.0, 0.0));
        add("Lying on front, head on hand", -38.0, 8.0, b.done());
    }

    // 22. All fours, looking back over the shoulder, one hand forward.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.48, 0.0).spine(d(0.0, 0.05, 1.0), d(0.0, 0.1, 1.0), d(-0.1, 0.25, 1.0), d(-0.25, -1.0, 0.0));
        b.head(d(-0.2, 1.0, 0.5), d(-1.0, -0.1, 0.1));
        for (sx, th, sh, ft) in SIDES {
            let hip = b.pos(th);
            b.ik(sh, v3(hip.x + sx * 0.03 * b.k, 0.055 * b.k, hip.z - 0.38 * b.k), d(0.0, -1.0, 1.0));
            b.foot(ft, d(0.0, -0.45, -1.0), d(0.0, -1.0, 0.45));
        }
        b.hand_floor(ForearmL, 0.03, 0.02, d(0.0, 0.0, -1.0));
        b.hand(HandL, d(-0.1, 0.0, 1.0), -Vec3::Y);
        b.hand_floor(ForearmR, -0.04, 0.24, d(0.0, 0.0, -1.0));
        b.hand(HandR, d(0.2, 0.0, 1.0), -Vec3::Y);
        add("All fours, looking back", -140.0, 10.0, b.done());
    }

    // 23. Frog kneel: knees wide apart, hands on the floor between them.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.17, 0.0);
        b.best(0.0, 0.9, 90, |b, t| {
            b.spine(d(0.0, 1.0, 0.1 + t * 0.3), d(0.0, 1.0, 0.05 + t * 0.4), d(0.0, 1.0, t), Vec3::Z);
            for (sx, th, sh, ft) in SIDES {
                b.kneel(th, sh, v3(sx * 0.42, 0.05, 0.22), v3(sx * 0.36, 0.05, -0.20));
                b.foot(ft, d(-sx * 0.5, -0.4, -1.0), d(0.0, -1.0, 0.3));
            }
            b.reach_err(ForearmL, -0.13, 0.15, 0.51) + 0.02 * t
        });
        b.head(d(0.0, 1.0, 0.1), d(0.0, -0.3, 1.0));
        for (sx, _, fa, ha) in ARMS {
            b.hand_floor(fa, -sx * 0.13, 0.15, d(sx * 0.3, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.25, -0.1, 1.0), -Vec3::Y);
        }
        add("Frog kneel, hands on floor", 0.0, 5.0, b.done());
    }

    // 24. Lying on the back, knees raised to one side, arm out on the floor.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), d(0.0, 0.85, 0.5));
        b.orient(Waist, d(-1.0, 0.0, 0.0), d(0.0, 0.9, 0.42)).orient(Chest, d(-1.0, 0.05, 0.0), d(0.0, 0.75, 0.65));
        b.head(d(-1.0, 0.1, 0.1), d(0.0, 0.45, 0.9));
        b.limb(ThighR, d(0.25, 1.0, 0.25), d(0.8, -0.55, 0.0));
        b.limb(ThighL, d(0.95, 0.35, -0.1), d(0.25, 0.25, 0.95));
        b.foot(FootR, d(0.8, -0.55, 0.0), d(0.55, 0.8, 0.0)).foot(FootL, d(0.4, 0.2, 0.9), d(-0.2, 1.0, 0.0));
        b.limb(UpperArmR, d(0.55, -0.2, 0.8), d(1.0, -0.08, 0.3));
        b.hand(HandR, d(1.0, -0.05, 0.25), -Vec3::Y);
        let belly = b.at(Waist, 0.02, 0.06, 0.14);
        b.ik(ForearmL, belly, d(0.0, 0.3, -1.0));
        b.hand(HandL, d(0.3, 0.0, 1.0), d(0.0, -1.0, -0.5));
        add("Lying on back, legs raised", 12.0, 18.0, b.done());
    }

    // 25. Sitting with the legs folded to the right, leaning on the left hand.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.15, 1.0, 0.0), d(0.18, 1.0, -0.02), d(0.06, 1.0, -0.05), d(0.3, 0.0, 1.0));
        b.head(d(-0.15, 1.0, 0.0), d(-0.35, -0.1, 1.0));
        b.best(-0.3, 0.3, 60, |b, t| {
            b.limb(ThighL, d(-0.55, t, 0.85), d(-0.8, -0.02, -0.6));
            b.limb(ThighR, d(-0.85, t + 0.04, 0.45), d(-0.3, -0.02, -0.95));
            (b.min_y_of(&[Pelvis]) - b.min_y_of(&[ThighL, ShinL, ShinR, ThighR])).abs()
        });
        b.foot(FootL, d(-0.8, 0.0, -0.5), d(0.0, 0.3, -1.0)).foot(FootR, d(-0.3, 0.0, -1.0), d(0.0, 0.3, -1.0));
        b.hand_floor(ForearmL, 0.30, -0.08, d(0.0, 0.0, -1.0));
        b.hand(HandL, d(0.7, -0.1, -0.3), d(0.0, -1.0, 0.0));
        let thigh = b.at(ThighL, 0.0, -0.25, 0.04);
        b.ik(ForearmR, thigh, d(-1.0, -0.3, -0.4));
        b.hand(HandR, d(0.5, -0.4, 0.5), d(0.0, -1.0, 0.0));
        add("Sitting, legs folded, leaning on hand", 25.0, 10.0, b.done());
    }

    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mannequin::body::Body;
    use crate::mannequin::skeleton::{BodyType, Proportions};

    #[test]
    fn twenty_five_valid_presets() {
        for props in [
            Proportions::default(),
            Proportions { height: 1.5, head_size: 1.3, body_type: BodyType::Curvy, ..Proportions::default() },
            Proportions {
                height: 1.95,
                shoulder_width: 1.3,
                hip_width: 0.8,
                body_type: BodyType::Slim,
                ..Proportions::default()
            },
        ] {
            let s = Skeleton::new(props);
            let list = presets(&s);
            assert_eq!(list.len(), 25);
            let mut names: Vec<_> = list.iter().map(|p| p.name).collect();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), 25, "names are unique");
            for p in &list {
                assert!(p.pose.is_finite(), "{} has NaNs", p.name);
                let body = Body::new(&s, &p.pose);
                // Resting on the floor: lowest point at y = 0, nothing below it.
                assert!(body.min_y().abs() < 1e-3, "{} min_y {}", p.name, body.min_y());
                for j in [Joint::HandL, Joint::HandR, Joint::FootL, Joint::FootR, Joint::Head] {
                    assert!(body.fk.tip(j).y > -0.02, "{} {:?} below the floor", p.name, j);
                    assert!(body.fk.pos(j).y > -0.02, "{} {:?} below the floor", p.name, j);
                }
                // Not floating: something touches the floor and the figure is compact.
                let (lo, hi) = body.bounds();
                assert!(hi.y < 2.4 && (hi - lo).length() < 3.0, "{} too large", p.name);
            }
        }
    }

    #[test]
    fn presets_are_distinct() {
        let s = Skeleton::new(Proportions::default());
        let list = presets(&s);
        for (i, a) in list.iter().enumerate() {
            for b in &list[i + 1..] {
                let diff: f32 =
                    a.pose.rot.iter().flatten().zip(b.pose.rot.iter().flatten()).map(|(x, y)| (x - y).abs()).sum();
                assert!(diff > 60.0, "{} and {} are too similar", a.name, b.name);
            }
        }
    }
}
