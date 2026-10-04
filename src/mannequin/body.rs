//! Mannequin body volumes (capsules and ellipsoids) attached to the skeleton,
//! plus floor snapping.

use crate::mannequin::math::{Mat3, Vec3, v3};
use crate::mannequin::skeleton::{BodyType, Fk, Joint, Pose, Skeleton, forward};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Volume {
    /// Tapered capsule ("round cone"): sphere `ra` at `a`, sphere `rb` at `b` and their
    /// hull, optionally flattened by factor `k` along a unit axis perpendicular to `b - a`.
    Capsule { a: Vec3, ra: f32, b: Vec3, rb: f32, flat: Option<(Vec3, f32)> },
    /// Ellipsoid `center + axes * u` for unit vectors `u` (columns = semi-axes).
    Ellipsoid { center: Vec3, axes: Mat3 },
}

impl Volume {
    /// Lowest world Y of the volume.
    pub fn min_y(&self) -> f32 {
        match *self {
            Volume::Capsule { a, ra, b, rb, flat } => {
                let m = flat_extent(flat, Vec3::Y);
                (a.y - ra * m).min(b.y - rb * m)
            }
            Volume::Ellipsoid { center, axes } => center.y - extent(&axes, Vec3::Y),
        }
    }
    pub fn center(&self) -> Vec3 {
        match *self {
            Volume::Capsule { a, b, .. } => a.lerp(b, 0.5),
            Volume::Ellipsoid { center, .. } => center,
        }
    }
    fn translate(&mut self, d: Vec3) {
        match self {
            Volume::Capsule { a, b, .. } => {
                *a += d;
                *b += d;
            }
            Volume::Ellipsoid { center, .. } => *center += d,
        }
    }
}

/// Length of `M n` for the flattening matrix `M = I + (k - 1) u u^T`.
pub fn flat_extent(flat: Option<(Vec3, f32)>, n: Vec3) -> f32 {
    match flat {
        None => 1.0,
        Some((u, k)) => {
            let d = u.dot(n);
            (1.0 + (k * k - 1.0) * d * d).max(0.0).sqrt()
        }
    }
}

/// Half extent of an ellipsoid with the given axes along unit direction `n`.
pub fn extent(axes: &Mat3, n: Vec3) -> f32 {
    let a = axes.x.dot(n);
    let b = axes.y.dot(n);
    let c = axes.z.dot(n);
    (a * a + b * b + c * c).sqrt()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub joint: Joint,
    pub volume: Volume,
    /// Draw face-direction guide lines (the head).
    pub is_head: bool,
}

/// A posed mannequin: FK result plus its body volumes, in world space.
#[derive(Clone, Debug)]
pub struct Body {
    pub fk: Fk,
    pub parts: Vec<Part>,
}

struct Dims {
    limb: f32,
    thigh: f32,
    hip: f32,
    waist: f32,
    bust: f32,
}

fn dims(t: BodyType) -> Dims {
    match t {
        BodyType::Slim => Dims { limb: 0.88, thigh: 0.86, hip: 0.9, waist: 0.92, bust: 0.75 },
        BodyType::Average => Dims { limb: 1.0, thigh: 1.0, hip: 1.0, waist: 1.0, bust: 1.0 },
        BodyType::Curvy => Dims { limb: 1.05, thigh: 1.12, hip: 1.1, waist: 1.0, bust: 1.18 },
        BodyType::Masculine => Dims { limb: 1.08, thigh: 0.9, hip: 0.86, waist: 1.2, bust: 0.0 },
    }
}

impl Body {
    pub fn new(skel: &Skeleton, pose: &Pose) -> Body {
        let fk = forward(skel, pose);
        let parts = build_parts(skel, &fk);
        Body { fk, parts }
    }

    /// Lowest point of the whole body.
    pub fn min_y(&self) -> f32 {
        self.parts.iter().map(|p| p.volume.min_y()).fold(f32::INFINITY, f32::min)
    }

    pub fn translate(&mut self, d: Vec3) {
        self.fk.translate(d);
        for p in &mut self.parts {
            p.volume.translate(d);
        }
    }

    /// Axis-aligned bounds of all volumes.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = v3(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut hi = -lo;
        for p in &self.parts {
            let (c, r) = match p.volume {
                Volume::Capsule { a, ra, b, rb, flat } => {
                    let e = v3(flat_extent(flat, Vec3::X), flat_extent(flat, Vec3::Y), flat_extent(flat, Vec3::Z));
                    for (q, r) in [(a, ra), (b, rb)] {
                        lo = v3(lo.x.min(q.x - r * e.x), lo.y.min(q.y - r * e.y), lo.z.min(q.z - r * e.z));
                        hi = v3(hi.x.max(q.x + r * e.x), hi.y.max(q.y + r * e.y), hi.z.max(q.z + r * e.z));
                    }
                    continue;
                }
                Volume::Ellipsoid { center, axes } => {
                    (center, v3(extent(&axes, Vec3::X), extent(&axes, Vec3::Y), extent(&axes, Vec3::Z)))
                }
            };
            lo = v3(lo.x.min(c.x - r.x), lo.y.min(c.y - r.y), lo.z.min(c.z - r.z));
            hi = v3(hi.x.max(c.x + r.x), hi.y.max(c.y + r.y), hi.z.max(c.z + r.z));
        }
        (lo, hi)
    }
}

/// The body is a set of overlapping smooth volumes. The line-art renderer draws the
/// visible contour of their union, so overlapping volumes read as one continuous
/// surface: tapered limb segments share radii where they meet, the torso is two
/// flattened tapered tubes (hips to waist, waist to chest) with breasts and glutes
/// on top, and the head is an egg made of cranium and jaw.
fn build_parts(skel: &Skeleton, fk: &Fk) -> Vec<Part> {
    use Joint::*;
    let s = skel.scale();
    let p = skel.props;
    let d = dims(p.body_type);
    let hs = p.head_size;
    let hw = p.hip_width * d.hip;
    let sw = p.shoulder_width;
    let mut parts = Vec::with_capacity(40);
    let ell = |j: Joint, c: Vec3, half: Vec3| {
        let r = fk.rot(j);
        Volume::Ellipsoid { center: fk.pos(j) + r.mul_vec(c * s), axes: r.scale_cols(half * s) }
    };
    // Ellipsoid with an extra local rotation.
    let ell_r = |j: Joint, c: Vec3, half: Vec3, local: Mat3| {
        let r = fk.rot(j);
        Volume::Ellipsoid { center: fk.pos(j) + r.mul_vec(c * s), axes: r.mul(&local).scale_cols(half * s) }
    };
    let at = |j: Joint, c: Vec3| fk.pos(j) + fk.rot(j).mul_vec(c * s);
    let cone = |a: Vec3, ra: f32, b: Vec3, rb: f32| Volume::Capsule { a, ra: ra * s, b, rb: rb * s, flat: None };
    // Tube flattened along `axis` (made perpendicular to the tube) by factor `k`.
    let tube = |a: Vec3, ra: f32, b: Vec3, rb: f32, axis: Vec3, k: f32| {
        let dir = (b - a).normalized_or(Vec3::Y);
        let u = axis.reject(dir).normalized_or(Vec3::Z);
        Volume::Capsule { a, ra: ra * s, b, rb: rb * s, flat: Some((u, k)) }
    };
    let part = |joint: Joint, volume: Volume| Part { joint, volume, is_head: false };

    // --- torso ---
    let waist_pt = at(Waist, v3(0.0, 0.07, 0.0));
    let waist_r = 0.114 * d.waist * sw.sqrt().sqrt();
    let pz = fk.rot(Pelvis).z;
    let cz = fk.rot(Chest).z;
    let wz = (pz + cz).normalized_or(pz);
    parts.push(part(
        Pelvis,
        tube(at(Pelvis, v3(0.0, -0.035, -0.008)), 0.188 * hw, waist_pt, waist_r, (pz + wz) * 0.5, 0.70),
    ));
    parts.push(part(
        Chest,
        tube(waist_pt, waist_r, at(Chest, v3(0.0, 0.118, -0.012)), 0.156 * sw.sqrt(), (wz + cz) * 0.5, 0.68),
    ));
    for sx in [1.0f32, -1.0] {
        // Glutes.
        // (smaller and flatter for the masculine body type added by rust-scene-storyboard)
        let g = if p.body_type == BodyType::Masculine { 0.78 } else { 1.0 };
        parts.push(part(
            Pelvis,
            ell_r(
                Pelvis,
                v3(sx * 0.078 * hw, -0.088, -0.080 * g),
                v3(0.112 * hw, 0.124, 0.114 * hw.sqrt() * g),
                Mat3::rot_z(sx * 0.10),
            ),
        ));
        // Breasts: rounded volumes elongated forwards-down, so only their outer and
        // lower contour stands out from the chest.
        let b = d.bust;
        if b < 0.05 {
            continue;
        }
        parts.push(part(
            Chest,
            ell_r(
                Chest,
                v3(sx * 0.068 * sw.sqrt(), 0.082, 0.060 + 0.012 * b),
                v3(0.074, 0.070, 0.080) * b.sqrt(),
                Mat3::rot_y(sx * 0.30).mul(&Mat3::rot_x(0.45)),
            ),
        ));
    }
    // Trapezius / shoulder line: from the neck base out to each shoulder joint.
    let neck_base = at(Chest, v3(0.0, 0.222, -0.035));
    for ua in [UpperArmL, UpperArmR] {
        parts.push(part(Chest, cone(neck_base, 0.056, fk.pos(ua), 0.056 * d.limb)));
    }
    parts.push(part(
        Neck,
        cone(
            at(Neck, v3(0.0, -0.03, -0.005)),
            0.043,
            fk.tip(Neck) + fk.rot(Neck).mul_vec(v3(0.0, 0.03, 0.012) * s),
            0.037,
        ),
    ));

    // --- head: egg shape (cranium + jaw) ---
    parts.push(Part {
        joint: Head,
        volume: ell(Head, v3(0.0, 0.150, 0.004) * hs, v3(0.100, 0.122, 0.114) * hs),
        is_head: true,
    });
    parts.push(part(Head, ell(Head, v3(0.0, 0.080, 0.036) * hs, v3(0.068, 0.080, 0.076) * hs)));

    // --- limbs ---
    for (ua, fa, ha, th, sh, ft) in
        [(UpperArmL, ForearmL, HandL, ThighL, ShinL, FootL), (UpperArmR, ForearmR, HandR, ThighR, ShinR, FootR)]
    {
        let l = d.limb;
        parts.push(part(ua, cone(fk.pos(ua), 0.061 * l, fk.tip(ua), 0.041 * l)));
        let fbulge = at(fa, v3(0.0, -0.07, 0.0));
        parts.push(part(fa, cone(fk.pos(fa), 0.041 * l, fbulge, 0.045 * l)));
        parts.push(part(fa, cone(fbulge, 0.043 * l, fk.tip(fa), 0.027 * l)));
        // Mitten hand: flat palm with a slight taper.
        parts.push(part(
            ha,
            tube(
                at(ha, v3(0.0, -0.02, 0.003)),
                0.039 * l,
                at(ha, v3(0.0, -0.115, 0.006)),
                0.031 * l,
                fk.rot(ha).x,
                0.55,
            ),
        ));
        let t = d.thigh;
        let mid = at(th, v3(0.0, -0.17, 0.008));
        parts.push(part(th, cone(fk.pos(th), 0.138 * t, mid, 0.118 * t)));
        parts.push(part(th, cone(mid, 0.118 * t, fk.tip(th), 0.066 * l)));
        let calf = at(sh, v3(0.0, -0.13, -0.014));
        parts.push(part(sh, cone(fk.pos(sh), 0.063 * l, calf, 0.070 * l)));
        parts.push(part(sh, cone(calf, 0.070 * l, fk.tip(sh), 0.035 * l)));
        // Foot: heel to toes, a little flatter than wide.
        parts.push(part(
            ft,
            tube(at(ft, v3(0.0, -0.04, -0.012)), 0.037, at(ft, v3(0.0, -0.054, 0.13)), 0.028, fk.rot(ft).y, 0.78),
        ));
    }
    parts
}

/// Move the pose vertically so that the lowest body point rests on the floor.
pub fn snap_to_floor(skel: &Skeleton, pose: &mut Pose) {
    let body = Body::new(skel, pose);
    let m = body.min_y();
    // Ignore float noise so repeated snapping is idempotent (keeps undo history clean).
    if m.is_finite() && m.abs() > 1e-4 {
        pose.root.y -= m;
    }
}

/// World position of the drag handle that controls `j`.
///
/// The pelvis handle sits on the pelvis itself (it translates the figure), the head
/// handle on the face, every other handle at the tip of the joint's bone.
pub fn handle_point(skel: &Skeleton, fk: &Fk, j: Joint) -> Vec3 {
    match j {
        Joint::Pelvis => fk.pos(j),
        Joint::Head => {
            let hs = skel.props.head_size * skel.scale();
            fk.pos(j) + fk.rot(j).mul_vec(v3(0.0, 0.11, 0.11) * hs)
        }
        _ => fk.tip(j),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mannequin::skeleton::Proportions;

    #[test]
    fn rest_pose_stands_on_floor() {
        let skel = Skeleton::new(Proportions::default());
        let body = Body::new(&skel, &Pose::rest(&skel));
        assert!(body.min_y().abs() < 0.01, "{}", body.min_y());
        let mut pose = Pose::rest(&skel);
        pose.root.y += 0.5;
        snap_to_floor(&skel, &mut pose);
        assert!(Body::new(&skel, &pose).min_y().abs() < 1e-4);
        // Snapping an already grounded pose must not change it (undo history relies on it).
        let before = pose.clone();
        snap_to_floor(&skel, &mut pose);
        assert_eq!(before, pose);
    }

    #[test]
    fn ellipsoid_extent() {
        let axes = Mat3::IDENTITY.scale_cols(v3(1.0, 2.0, 3.0));
        assert!((extent(&axes, Vec3::Y) - 2.0).abs() < 1e-6);
        let r = Mat3::rot_x(std::f32::consts::FRAC_PI_2).mul(&axes);
        assert!((extent(&r, Vec3::Y) - 3.0).abs() < 1e-5);
    }
}
