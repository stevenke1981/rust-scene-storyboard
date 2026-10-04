//! Analytic two-bone inverse kinematics (shoulder-elbow-wrist, hip-knee-ankle).

use crate::mannequin::math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwoBoneSolution {
    /// New position of the middle joint (elbow / knee).
    pub mid: Vec3,
    /// New position of the end joint; equals the target when it is reachable.
    pub end: Vec3,
    /// Whether the target was within reach.
    pub reached: bool,
}

/// Solve a two-bone chain rooted at `root` with bone lengths `len_a`, `len_b`
/// so that its end lands on `target`. `pole` is a direction hint telling which
/// way the middle joint should bulge (e.g. "elbow backwards", "knee forwards").
pub fn solve_two_bone(root: Vec3, len_a: f32, len_b: f32, target: Vec3, pole: Vec3) -> TwoBoneSolution {
    let to = target - root;
    let dist = to.length();
    let dir = to.normalized_or(Vec3::Y * -1.0);
    let min_d = (len_a - len_b).abs() + 1e-4;
    let max_d = len_a + len_b - 1e-4;
    let d = dist.clamp(min_d, max_d.max(min_d));
    let reached = dist >= min_d - 1e-4 && dist <= len_a + len_b + 1e-4;

    // Bend direction: the pole hint made perpendicular to the root->target axis.
    let mut bend = pole.reject(dir);
    if bend.length() < 1e-5 {
        let alt = if dir.y.abs() < 0.9 { Vec3::Y } else { Vec3::Z };
        bend = alt.reject(dir);
    }
    let bend = bend.normalized();

    // Law of cosines for the angle at the root.
    let cos_a = ((len_a * len_a + d * d - len_b * len_b) / (2.0 * len_a * d)).clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
    let mid = root + dir * (len_a * cos_a) + bend * (len_a * sin_a);
    let end = root + dir * d;
    TwoBoneSolution { mid, end, reached }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mannequin::math::v3;

    #[test]
    fn reachable_target_is_hit_and_lengths_kept() {
        let root = v3(0.1, 1.4, 0.0);
        let (a, b) = (0.29, 0.25);
        for target in [v3(0.3, 1.2, 0.3), v3(0.1, 1.0, 0.1), v3(-0.2, 1.6, 0.2), v3(0.1, 1.4, 0.4)] {
            let s = solve_two_bone(root, a, b, target, v3(0.0, 0.0, -1.0));
            assert!(s.reached);
            assert!((s.end - target).length() < 1e-4, "{target:?}");
            assert!(((s.mid - root).length() - a).abs() < 1e-4);
            assert!(((s.end - s.mid).length() - b).abs() < 1e-4);
        }
    }

    #[test]
    fn pole_controls_bend_direction() {
        let root = v3(0.0, 0.9, 0.0);
        let target = v3(0.0, 0.3, 0.0);
        let fwd = solve_two_bone(root, 0.42, 0.41, target, v3(0.0, 0.0, 1.0));
        let back = solve_two_bone(root, 0.42, 0.41, target, v3(0.0, 0.0, -1.0));
        assert!(fwd.mid.z > 0.1 && back.mid.z < -0.1);
    }

    #[test]
    fn unreachable_target_stretches_towards_it() {
        let root = Vec3::ZERO;
        let s = solve_two_bone(root, 0.3, 0.3, v3(2.0, 0.0, 0.0), Vec3::Y);
        assert!(!s.reached);
        assert!((s.end.x - 0.6).abs() < 1e-3 && s.end.y.abs() < 1e-3);
        assert!(s.mid.is_finite() && s.end.is_finite());
    }
}
