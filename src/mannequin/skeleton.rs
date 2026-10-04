//! Humanoid skeleton: joint hierarchy, body proportions, poses and forward kinematics.
//!
//! Coordinate system: right-handed, metres, +Y up, the floor is the plane `y = 0`.
//! In the rest pose the figure stands upright facing +Z with the arms hanging
//! down, so +X is the figure's *left*.
//!
//! Every joint owns the bone that starts at it (e.g. `UpperArmL` sits at the
//! shoulder and rotates the upper arm). A joint's rotation is stored as Euler
//! angles in degrees relative to its parent, applied as `Rz * Ry * Rx`.

use serde::{Deserialize, Serialize};

use crate::mannequin::math::{Mat3, Vec3, v3};

pub const JOINT_COUNT: usize = 17;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Joint {
    Pelvis,
    Waist,
    Chest,
    Neck,
    Head,
    UpperArmL,
    ForearmL,
    HandL,
    UpperArmR,
    ForearmR,
    HandR,
    ThighL,
    ShinL,
    FootL,
    ThighR,
    ShinR,
    FootR,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Center,
    Left,
    Right,
}

impl Joint {
    pub const ALL: [Joint; JOINT_COUNT] = [
        Joint::Pelvis,
        Joint::Waist,
        Joint::Chest,
        Joint::Neck,
        Joint::Head,
        Joint::UpperArmL,
        Joint::ForearmL,
        Joint::HandL,
        Joint::UpperArmR,
        Joint::ForearmR,
        Joint::HandR,
        Joint::ThighL,
        Joint::ShinL,
        Joint::FootL,
        Joint::ThighR,
        Joint::ShinR,
        Joint::FootR,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Stable identifier used in pose JSON files.
    pub fn key(self) -> &'static str {
        match self {
            Joint::Pelvis => "pelvis",
            Joint::Waist => "waist",
            Joint::Chest => "chest",
            Joint::Neck => "neck",
            Joint::Head => "head",
            Joint::UpperArmL => "upper_arm_l",
            Joint::ForearmL => "forearm_l",
            Joint::HandL => "hand_l",
            Joint::UpperArmR => "upper_arm_r",
            Joint::ForearmR => "forearm_r",
            Joint::HandR => "hand_r",
            Joint::ThighL => "thigh_l",
            Joint::ShinL => "shin_l",
            Joint::FootL => "foot_l",
            Joint::ThighR => "thigh_r",
            Joint::ShinR => "shin_r",
            Joint::FootR => "foot_r",
        }
    }

    pub fn from_key(k: &str) -> Option<Joint> {
        Joint::ALL.into_iter().find(|j| j.key() == k)
    }

    /// Human readable name for the UI.
    pub fn label(self) -> &'static str {
        match self {
            Joint::Pelvis => "Pelvis (root)",
            Joint::Waist => "Waist / lower spine",
            Joint::Chest => "Chest / upper spine",
            Joint::Neck => "Neck",
            Joint::Head => "Head",
            Joint::UpperArmL => "Upper arm L (shoulder)",
            Joint::ForearmL => "Forearm L (elbow)",
            Joint::HandL => "Hand L (wrist)",
            Joint::UpperArmR => "Upper arm R (shoulder)",
            Joint::ForearmR => "Forearm R (elbow)",
            Joint::HandR => "Hand R (wrist)",
            Joint::ThighL => "Thigh L (hip)",
            Joint::ShinL => "Shin L (knee)",
            Joint::FootL => "Foot L (ankle)",
            Joint::ThighR => "Thigh R (hip)",
            Joint::ShinR => "Shin R (knee)",
            Joint::FootR => "Foot R (ankle)",
        }
    }

    pub fn parent(self) -> Option<Joint> {
        use Joint::*;
        Some(match self {
            Pelvis => return None,
            Waist => Pelvis,
            Chest => Waist,
            Neck => Chest,
            Head => Neck,
            UpperArmL | UpperArmR => Chest,
            ForearmL => UpperArmL,
            ForearmR => UpperArmR,
            HandL => ForearmL,
            HandR => ForearmR,
            ThighL | ThighR => Pelvis,
            ShinL => ThighL,
            ShinR => ThighR,
            FootL => ShinL,
            FootR => ShinR,
        })
    }

    pub fn side(self) -> Side {
        use Joint::*;
        match self {
            UpperArmL | ForearmL | HandL | ThighL | ShinL | FootL => Side::Left,
            UpperArmR | ForearmR | HandR | ThighR | ShinR | FootR => Side::Right,
            _ => Side::Center,
        }
    }

    /// The same joint on the other side of the body (centre joints map to themselves).
    pub fn mirror(self) -> Joint {
        use Joint::*;
        match self {
            UpperArmL => UpperArmR,
            ForearmL => ForearmR,
            HandL => HandR,
            ThighL => ThighR,
            ShinL => ShinR,
            FootL => FootR,
            UpperArmR => UpperArmL,
            ForearmR => ForearmL,
            HandR => HandL,
            ThighR => ThighL,
            ShinR => ShinL,
            FootR => FootL,
            j => j,
        }
    }

    /// Elbows and knees: one-axis hinge joints (bend around local X).
    pub fn is_hinge(self) -> bool {
        matches!(self, Joint::ForearmL | Joint::ForearmR | Joint::ShinL | Joint::ShinR)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyType {
    Slim,
    #[default]
    Average,
    Curvy,
    /// Added for rust-scene-storyboard: flat chest, broader shoulders, narrower hips.
    Masculine,
}

impl BodyType {
    pub const ALL: [BodyType; 4] = [BodyType::Slim, BodyType::Average, BodyType::Curvy, BodyType::Masculine];
    pub fn label(self) -> &'static str {
        match self {
            BodyType::Slim => "Slim",
            BodyType::Average => "Average",
            BodyType::Curvy => "Curvy",
            BodyType::Masculine => "Masculine",
        }
    }
    /// Traditional Chinese label.
    pub fn zh(self) -> &'static str {
        match self {
            BodyType::Slim => "纖細",
            BodyType::Average => "標準",
            BodyType::Curvy => "豐滿",
            BodyType::Masculine => "男性體型",
        }
    }
}

/// User adjustable body proportions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Proportions {
    /// Standing height in metres.
    pub height: f32,
    /// Head size multiplier (1.0 = about 1/7 of the height).
    pub head_size: f32,
    pub shoulder_width: f32,
    pub hip_width: f32,
    pub body_type: BodyType,
}

impl Default for Proportions {
    fn default() -> Self {
        Proportions { height: 1.70, head_size: 1.0, shoulder_width: 1.0, hip_width: 1.0, body_type: BodyType::Average }
    }
}

impl Proportions {
    pub fn sanitized(mut self) -> Self {
        let fix = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.height = fix(self.height, 1.0, 2.4, 1.70);
        self.head_size = fix(self.head_size, 0.6, 1.6, 1.0);
        self.shoulder_width = fix(self.shoulder_width, 0.6, 1.5, 1.0);
        self.hip_width = fix(self.hip_width, 0.6, 1.5, 1.0);
        self
    }
}

/// Rest-pose geometry derived from [`Proportions`].
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    pub props: Proportions,
    /// Joint origin relative to the parent joint, in the parent's frame.
    pub offset: [Vec3; JOINT_COUNT],
    /// Bone vector (joint origin -> bone tip) in the joint's own frame.
    pub bone: [Vec3; JOINT_COUNT],
    /// Pelvis height above the floor in the rest (standing) pose.
    pub standing_pelvis_height: f32,
}

impl Skeleton {
    pub fn new(props: Proportions) -> Skeleton {
        let p = props.sanitized();
        let s = p.height / 1.70;
        let hs = p.head_size;
        let sw = p.shoulder_width * if p.body_type == BodyType::Masculine { 1.1 } else { 1.0 };
        let hw = p.hip_width * if p.body_type == BodyType::Curvy { 1.06 } else { 1.0 };
        let mut offset = [Vec3::ZERO; JOINT_COUNT];
        let mut bone = [Vec3::ZERO; JOINT_COUNT];
        use Joint::*;
        let set = |o: &mut [Vec3; JOINT_COUNT], b: &mut [Vec3; JOINT_COUNT], j: Joint, off: Vec3, bn: Vec3| {
            o[j.index()] = off * s;
            b[j.index()] = bn * s;
        };
        set(&mut offset, &mut bone, Pelvis, Vec3::ZERO, v3(0.0, 0.10, 0.0));
        set(&mut offset, &mut bone, Waist, v3(0.0, 0.10, 0.0), v3(0.0, 0.12, 0.0));
        set(&mut offset, &mut bone, Chest, v3(0.0, 0.12, 0.0), v3(0.0, 0.25, 0.0));
        set(&mut offset, &mut bone, Neck, v3(0.0, 0.25, -0.012), v3(0.0, 0.07, 0.0));
        set(&mut offset, &mut bone, Head, v3(0.0, 0.07, 0.0), v3(0.0, 0.24 * hs, 0.0));
        for (sign, ua, fa, ha, th, sh, ft) in [
            (1.0, UpperArmL, ForearmL, HandL, ThighL, ShinL, FootL),
            (-1.0, UpperArmR, ForearmR, HandR, ThighR, ShinR, FootR),
        ] {
            set(&mut offset, &mut bone, ua, v3(sign * 0.168 * sw, 0.212, -0.018), v3(0.0, -0.28, 0.0));
            set(&mut offset, &mut bone, fa, v3(0.0, -0.28, 0.0), v3(0.0, -0.24, 0.0));
            set(&mut offset, &mut bone, ha, v3(0.0, -0.24, 0.0), v3(0.0, -0.155, 0.0));
            set(&mut offset, &mut bone, th, v3(sign * 0.088 * hw, -0.065, 0.0), v3(0.0, -0.43, 0.0));
            set(&mut offset, &mut bone, sh, v3(0.0, -0.43, 0.0), v3(0.0, -0.41, 0.0));
            set(&mut offset, &mut bone, ft, v3(0.0, -0.41, 0.0), v3(0.0, -0.055, 0.155));
        }
        Skeleton { props: p, offset, bone, standing_pelvis_height: 0.98 * s }
    }

    /// Bone length of a joint.
    pub fn length(&self, j: Joint) -> f32 {
        self.bone[j.index()].length()
    }

    /// Body scale factor relative to the 1.70 m reference figure.
    pub fn scale(&self) -> f32 {
        self.props.height / 1.70
    }
}

/// A pose: root (pelvis) position plus a local rotation for every joint.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub root: Vec3,
    /// Euler angles in degrees (`Rz * Ry * Rx`), indexed by [`Joint::index`].
    pub rot: [[f32; 3]; JOINT_COUNT],
}

impl Pose {
    /// Upright standing rest pose for the given skeleton.
    pub fn rest(skel: &Skeleton) -> Pose {
        Pose { root: v3(0.0, skel.standing_pelvis_height, 0.0), rot: [[0.0; 3]; JOINT_COUNT] }
    }

    pub fn get(&self, j: Joint) -> [f32; 3] {
        self.rot[j.index()]
    }

    pub fn set(&mut self, j: Joint, e: [f32; 3]) {
        self.rot[j.index()] = e;
    }

    pub fn is_finite(&self) -> bool {
        self.root.is_finite() && self.rot.iter().flatten().all(|v| v.is_finite())
    }

    /// Mirror the pose left <-> right (reflection through the figure's YZ plane).
    pub fn mirrored(&self) -> Pose {
        let mut out = self.clone();
        out.root.x = -self.root.x;
        for j in Joint::ALL {
            let [x, y, z] = self.get(j);
            // M * R * M with M = diag(-1, 1, 1) negates the Y and Z rotations.
            out.set(j.mirror(), [x, -y, -z]);
        }
        out
    }
}

/// Result of forward kinematics: world-space joint frames.
#[derive(Clone, Debug)]
pub struct Fk {
    pub pos: [Vec3; JOINT_COUNT],
    pub rot: [Mat3; JOINT_COUNT],
    /// World position of each bone tip.
    pub tip: [Vec3; JOINT_COUNT],
}

impl Fk {
    pub fn pos(&self, j: Joint) -> Vec3 {
        self.pos[j.index()]
    }
    pub fn rot(&self, j: Joint) -> Mat3 {
        self.rot[j.index()]
    }
    pub fn tip(&self, j: Joint) -> Vec3 {
        self.tip[j.index()]
    }
    /// World rotation of the parent frame (identity for the root).
    pub fn parent_rot(&self, j: Joint) -> Mat3 {
        j.parent().map(|p| self.rot(p)).unwrap_or(Mat3::IDENTITY)
    }
    /// Translate the whole result (used for floor snapping).
    pub fn translate(&mut self, d: Vec3) {
        for i in 0..JOINT_COUNT {
            self.pos[i] += d;
            self.tip[i] += d;
        }
    }
}

/// Forward kinematics. Joints in [`Joint::ALL`] are ordered parents-first.
pub fn forward(skel: &Skeleton, pose: &Pose) -> Fk {
    let mut pos = [Vec3::ZERO; JOINT_COUNT];
    let mut rot = [Mat3::IDENTITY; JOINT_COUNT];
    let mut tip = [Vec3::ZERO; JOINT_COUNT];
    for j in Joint::ALL {
        let i = j.index();
        let local = Mat3::from_euler_deg(pose.rot[i]);
        match j.parent() {
            None => {
                pos[i] = pose.root;
                rot[i] = local;
            }
            Some(p) => {
                let pi = p.index();
                pos[i] = pos[pi] + rot[pi].mul_vec(skel.offset[i]);
                rot[i] = rot[pi].mul(&local);
            }
        }
        tip[i] = pos[i] + rot[i].mul_vec(skel.bone[i]);
    }
    Fk { pos, rot, tip }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_come_first() {
        for (i, j) in Joint::ALL.iter().enumerate() {
            assert_eq!(j.index(), i);
            if let Some(p) = j.parent() {
                assert!(p.index() < i);
            }
            assert_eq!(Joint::from_key(j.key()), Some(*j));
            assert_eq!(j.mirror().mirror(), *j);
        }
    }

    #[test]
    fn rest_pose_fk_is_symmetric_and_upright() {
        let skel = Skeleton::new(Proportions::default());
        let fk = forward(&skel, &Pose::rest(&skel));
        // Ankles a little above the floor, head top near the standing height.
        let ankle = fk.pos(Joint::FootL);
        assert!((ankle.y - 0.075).abs() < 1e-4, "{ankle:?}");
        let top = fk.tip(Joint::Head);
        assert!((top.y - 1.76).abs() < 0.03, "{top:?}");
        for j in Joint::ALL {
            let a = fk.pos(j);
            let b = fk.pos(j.mirror());
            assert!((a.x + b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 && (a.z - b.z).abs() < 1e-5);
        }
    }

    #[test]
    fn fk_rotation_propagates_to_children() {
        let skel = Skeleton::new(Proportions::default());
        let mut pose = Pose::rest(&skel);
        // Raise the left arm sideways by 90 degrees: the wrist moves out along +X.
        pose.set(Joint::UpperArmL, [0.0, 0.0, 90.0]);
        let fk = forward(&skel, &pose);
        let sh = fk.pos(Joint::UpperArmL);
        let wrist = fk.pos(Joint::HandL);
        let arm = skel.length(Joint::UpperArmL) + skel.length(Joint::ForearmL);
        assert!((wrist - (sh + v3(arm, 0.0, 0.0))).length() < 1e-4, "{sh:?} {wrist:?}");
        // Bending the knee (+X) moves the ankle backwards (-Z) and keeps bone lengths.
        pose.set(Joint::ShinL, [90.0, 0.0, 0.0]);
        let fk = forward(&skel, &pose);
        let knee = fk.pos(Joint::ShinL);
        let ankle = fk.pos(Joint::FootL);
        assert!((ankle.z - (knee.z - skel.length(Joint::ShinL))).abs() < 1e-4);
        assert!(((ankle - knee).length() - skel.length(Joint::ShinL)).abs() < 1e-5);
    }

    #[test]
    fn mirror_twice_is_identity() {
        let skel = Skeleton::new(Proportions::default());
        let mut pose = Pose::rest(&skel);
        pose.set(Joint::UpperArmL, [10.0, 20.0, 30.0]);
        pose.set(Joint::Chest, [5.0, 15.0, -8.0]);
        pose.root.x = 0.3;
        let m = pose.mirrored();
        assert_eq!(m.get(Joint::UpperArmR), [10.0, -20.0, -30.0]);
        assert_eq!(m.mirrored(), pose);
        // Mirrored FK positions are reflections of the original ones.
        let a = forward(&skel, &pose);
        let b = forward(&skel, &m);
        for j in Joint::ALL {
            let p = a.tip(j);
            let q = b.tip(j.mirror());
            assert!((p.x + q.x).abs() < 1e-4 && (p.y - q.y).abs() < 1e-4 && (p.z - q.z).abs() < 1e-4, "{j:?}");
        }
    }
}
