//! Pose editing operations: world-space joint orientation, limb aiming, two-bone
//! IK on the skeleton, FK dragging and random poses.

use crate::mannequin::body::{handle_point, snap_to_floor};
use crate::mannequin::ik::solve_two_bone;
use crate::mannequin::math::{Mat3, Vec3};
use crate::mannequin::skeleton::{Joint, Pose, Skeleton, forward};

/// Set the joint's world rotation (its local rotation is derived from the parent).
pub fn set_world_rot(skel: &Skeleton, pose: &mut Pose, j: Joint, world: Mat3) {
    let fk = forward(skel, pose);
    let local = fk.parent_rot(j).transpose().mul(&world.orthonormalized());
    pose.set(j, local.to_euler_deg());
}

/// Orient a joint so its local +Y points along `up` and local +Z towards `fwd` (world).
pub fn orient(skel: &Skeleton, pose: &mut Pose, j: Joint, up: Vec3, fwd: Vec3) {
    set_world_rot(skel, pose, j, Mat3::look(up, fwd));
}

fn is_arm(upper: Joint) -> bool {
    matches!(upper, Joint::UpperArmL | Joint::UpperArmR)
}

fn hinge_of(upper: Joint) -> Joint {
    match upper {
        Joint::UpperArmL => Joint::ForearmL,
        Joint::UpperArmR => Joint::ForearmR,
        Joint::ThighL => Joint::ShinL,
        Joint::ThighR => Joint::ShinR,
        other => panic!("{other:?} is not the upper joint of a limb"),
    }
}

/// Point a limb: the upper bone (upper arm / thigh) along `upper_dir` and the lower
/// bone (forearm / shin) along `lower_dir`, both world directions. Elbows only bend
/// forwards and knees backwards relative to the upper bone's frame, so the upper bone's
/// twist is chosen to make the hinge reach `lower_dir` exactly.
pub fn set_limb(skel: &Skeleton, pose: &mut Pose, upper: Joint, upper_dir: Vec3, lower_dir: Vec3) {
    let fk = forward(skel, pose);
    let du = upper_dir.normalized();
    let dl = lower_dir.normalized_or(du);
    let p = dl.reject(du);
    let arm = is_arm(upper);
    let y = -du;
    let z = if p.length() > 1e-3 {
        let n = p.normalized();
        if arm { n } else { -n }
    } else {
        // Straight limb: keep the current twist.
        fk.rot(upper).z
    };
    let world = Mat3::look(y, z);
    let local = fk.parent_rot(upper).transpose().mul(&world);
    pose.set(upper, local.to_euler_deg());
    let angle = du.dot(dl).clamp(-1.0, 1.0).acos().to_degrees();
    pose.set(hinge_of(upper), [if arm { -angle } else { angle }, 0.0, 0.0]);
}

/// Two-bone IK: move the end of the limb whose hinge is `hinge` (forearm / shin) to
/// `target`, keeping the elbow / knee bending the way it currently bends.
/// Returns whether the target was reachable.
pub fn ik_limb(skel: &Skeleton, pose: &mut Pose, hinge: Joint, target: Vec3, pole: Option<Vec3>) -> bool {
    let Some(upper) = hinge.parent() else { return false };
    let fk = forward(skel, pose);
    let root = fk.pos(upper);
    let mid = fk.pos(hinge);
    let end = fk.tip(hinge);
    let line = (end - root).normalized();
    let current_bulge = (mid - root).reject(line);
    let default_pole = if is_arm(upper) { -fk.rot(upper).z } else { fk.rot(upper).z };
    let pole = pole.unwrap_or(if current_bulge.length() > 1e-3 { current_bulge } else { default_pole });
    let sol = solve_two_bone(root, skel.length(upper), skel.length(hinge), target, pole);
    set_limb(skel, pose, upper, sol.mid - root, sol.end - sol.mid);
    sol.reached
}

/// FK drag: rotate joint `j` so that its handle moves towards `target` (world).
/// Hinge joints only rotate around their hinge axis.
pub fn aim_joint(skel: &Skeleton, pose: &mut Pose, j: Joint, target: Vec3) {
    let fk = forward(skel, pose);
    let origin = fk.pos(j);
    let handle = handle_point(skel, &fk, j);
    let from = handle - origin;
    let to = target - origin;
    if from.length() < 1e-5 || to.length() < 1e-5 {
        return;
    }
    if j.is_hinge() {
        let axis = fk.rot(j).x;
        let a = from.reject(axis);
        let b = to.reject(axis);
        if a.length() < 1e-5 || b.length() < 1e-5 {
            return;
        }
        let angle = a.cross(b).dot(axis).atan2(a.dot(b)).to_degrees();
        let mut e = pose.get(j);
        e[0] = crate::mannequin::math::wrap_deg(e[0] + angle);
        // Keep elbows / knees inside their natural range.
        e[0] = if is_arm(j.parent().unwrap_or(j)) { e[0].clamp(-165.0, 5.0) } else { e[0].clamp(-5.0, 165.0) };
        pose.set(j, e);
        return;
    }
    let r = Mat3::rotation_between(from, to);
    set_world_rot(skel, pose, j, r.mul(&fk.rot(j)));
}

/// Simple deterministic xorshift generator (no `rand` dependency).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }
}

/// A random but anatomically limited standing/moving pose, snapped to the floor.
pub fn random_pose(skel: &Skeleton, seed: u64) -> Pose {
    let mut r = Rng::new(seed);
    let mut pose = Pose::rest(skel);
    pose.set(Joint::Pelvis, [r.range(-15.0, 15.0), r.range(-60.0, 60.0), r.range(-8.0, 8.0)]);
    pose.set(Joint::Waist, [r.range(-10.0, 25.0), r.range(-20.0, 20.0), r.range(-10.0, 10.0)]);
    pose.set(Joint::Chest, [r.range(-10.0, 20.0), r.range(-25.0, 25.0), r.range(-10.0, 10.0)]);
    pose.set(Joint::Neck, [r.range(-10.0, 20.0), r.range(-30.0, 30.0), r.range(-10.0, 10.0)]);
    pose.set(Joint::Head, [r.range(-15.0, 15.0), r.range(-30.0, 30.0), r.range(-10.0, 10.0)]);
    for (sign, ua, fa, ha) in [
        (1.0, Joint::UpperArmL, Joint::ForearmL, Joint::HandL),
        (-1.0, Joint::UpperArmR, Joint::ForearmR, Joint::HandR),
    ] {
        pose.set(ua, [r.range(-120.0, 40.0), sign * r.range(-40.0, 40.0), sign * r.range(5.0, 100.0)]);
        pose.set(fa, [r.range(-130.0, -5.0), 0.0, 0.0]);
        pose.set(ha, [r.range(-30.0, 30.0), r.range(-40.0, 40.0), sign * r.range(-20.0, 20.0)]);
    }
    let stance = r.range(0.0, 1.0);
    for (sign, th, sh, ft) in
        [(1.0, Joint::ThighL, Joint::ShinL, Joint::FootL), (-1.0, Joint::ThighR, Joint::ShinR, Joint::FootR)]
    {
        let lift = if (stance > 0.5) == (sign > 0.0) { r.range(-70.0, 0.0) } else { r.range(-15.0, 15.0) };
        pose.set(th, [lift, sign * r.range(-10.0, 25.0), sign * r.range(0.0, 15.0)]);
        pose.set(sh, [r.range(0.0, -lift + 20.0), 0.0, 0.0]);
        pose.set(ft, [r.range(-15.0, 25.0), 0.0, 0.0]);
    }
    snap_to_floor(skel, &mut pose);
    pose
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mannequin::math::v3;
    use crate::mannequin::skeleton::Proportions;

    fn skel() -> Skeleton {
        Skeleton::new(Proportions::default())
    }

    #[test]
    fn set_limb_matches_directions() {
        let s = skel();
        let mut pose = Pose::rest(&s);
        pose.set(Joint::Chest, [10.0, 30.0, -5.0]);
        for (upper, du, dl) in [
            (Joint::UpperArmL, v3(1.0, -0.2, 0.3), v3(0.2, 0.5, 1.0)),
            (Joint::UpperArmR, v3(-0.3, -1.0, 0.0), v3(0.0, -1.0, 0.0)),
            (Joint::ThighL, v3(0.1, -0.2, 1.0), v3(0.0, -1.0, -0.1)),
            (Joint::ThighR, v3(0.0, -1.0, 0.0), v3(0.0, -0.3, -1.0)),
        ] {
            set_limb(&s, &mut pose, upper, du, dl);
            let fk = forward(&s, &pose);
            let hinge = hinge_of(upper);
            let got_u = (fk.pos(hinge) - fk.pos(upper)).normalized();
            let got_l = (fk.tip(hinge) - fk.pos(hinge)).normalized();
            assert!((got_u - du.normalized()).length() < 1e-3, "{upper:?} upper {got_u:?}");
            assert!((got_l - dl.normalized()).length() < 1e-3, "{upper:?} lower {got_l:?}");
        }
    }

    #[test]
    fn ik_limb_reaches_target_on_skeleton() {
        let s = skel();
        let mut pose = Pose::rest(&s);
        let fk = forward(&s, &pose);
        let target = fk.pos(Joint::UpperArmL) + v3(0.15, 0.1, 0.35);
        assert!(ik_limb(&s, &mut pose, Joint::ForearmL, target, None));
        let fk = forward(&s, &pose);
        assert!((fk.tip(Joint::ForearmL) - target).length() < 1e-3);
        // Elbow flexes forwards only (negative X rotation).
        assert!(pose.get(Joint::ForearmL)[0] < 0.0);

        let foot_target = fk.pos(Joint::ThighR) + v3(0.0, -0.6, 0.3);
        assert!(ik_limb(&s, &mut pose, Joint::ShinR, foot_target, None));
        let fk = forward(&s, &pose);
        assert!((fk.tip(Joint::ShinR) - foot_target).length() < 1e-3);
        assert!(pose.get(Joint::ShinR)[0] > 0.0, "knee bends backwards");
        assert!(fk.pos(Joint::ShinR).z > fk.pos(Joint::ThighR).z, "knee in front");
    }

    #[test]
    fn aim_joint_moves_handle_towards_target() {
        let s = skel();
        let mut pose = Pose::rest(&s);
        let fk = forward(&s, &pose);
        let sh = fk.pos(Joint::UpperArmL);
        let target = sh + v3(0.3, 0.0, 0.0);
        aim_joint(&s, &mut pose, Joint::UpperArmL, target);
        let fk = forward(&s, &pose);
        let dir = (fk.tip(Joint::UpperArmL) - sh).normalized();
        assert!((dir - v3(1.0, 0.0, 0.0)).length() < 1e-3, "{dir:?}");
    }

    #[test]
    fn random_poses_are_finite_and_grounded() {
        let s = skel();
        for seed in 1..50 {
            let p = random_pose(&s, seed);
            assert!(p.is_finite());
            let b = crate::mannequin::body::Body::new(&s, &p);
            assert!(b.min_y().abs() < 1e-3);
        }
    }
}
