//! Storyboard action poses: everyday acting poses (standing, walking, running,
//! sitting, pointing, waving…) authored with the world-space pose builder of
//! rust-pose-studio's presets. Together with the 25 floor poses they form the
//! pose library of the storyboard editor.
//!
//! Pose keys are stable identifiers stored in project files: action keys such
//! as `"walk"`, and `"floor:N"` (1-based) for the rust-pose-studio floor poses.

use super::body::snap_to_floor;
use super::floor_presets::{ARMS, B, SIDES, d, presets};
use super::math::{Vec3, v3};
use super::skeleton::Joint::*;
use super::skeleton::{Pose, Skeleton};

/// Metadata of a pose in the library.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseInfo {
    pub key: &'static str,
    /// Traditional Chinese name (primary UI language).
    pub zh: &'static str,
    pub en: &'static str,
    /// Short Traditional Chinese description used in the narration.
    pub desc: &'static str,
}

/// All action poses, in palette order.
pub const ACTIONS: [PoseInfo; 19] = [
    PoseInfo { key: "stand", zh: "站立", en: "Standing", desc: "自然站立，雙手垂放身側" },
    PoseInfo { key: "walk", zh: "行走", en: "Walking", desc: "邁步行走，雙臂自然前後擺動" },
    PoseInfo { key: "run", zh: "奔跑", en: "Running", desc: "身體前傾快跑，手肘彎曲用力擺臂" },
    PoseInfo {
        key: "sit", zh: "坐（椅子）", en: "Sitting on a chair", desc: "端坐在椅子上，雙手放在大腿上"
    },
    PoseInfo {
        key: "point", zh: "指向前方", en: "Pointing", desc: "右手向前伸直指向前方，視線跟著手指"
    },
    PoseInfo { key: "wave", zh: "揮手", en: "Waving", desc: "右手舉起揮手打招呼" },
    PoseInfo { key: "arms_crossed", zh: "雙手抱胸", en: "Arms crossed", desc: "雙手交叉抱在胸前" },
    PoseInfo {
        key: "hands_on_hips", zh: "雙手叉腰", en: "Hands on hips", desc: "雙手叉腰，手肘向外張開"
    },
    PoseInfo {
        key: "talk", zh: "說話（手勢）", en: "Talking / gesturing", desc: "邊說話邊比手勢，掌心朝上向前攤開"
    },
    PoseInfo {
        key: "think", zh: "思考（托下巴）", en: "Thinking", desc: "右手托著下巴、左手扶著右手肘思考"
    },
    PoseInfo { key: "crouch", zh: "蹲下", en: "Crouching", desc: "雙膝彎曲蹲低，前臂靠在膝蓋上" },
    PoseInfo { key: "jump", zh: "跳躍", en: "Jumping", desc: "雙腳離地跳起，雙手高舉成 V 字" },
    PoseInfo {
        key: "reach_up", zh: "伸手取高處", en: "Reaching up", desc: "踮起腳尖，右手向上伸直拿高處的東西"
    },
    PoseInfo {
        key: "kneel", zh: "單膝跪地", en: "Kneeling on one knee", desc: "右膝跪地、左腳在前踩地，左手扶膝"
    },
    PoseInfo { key: "lie", zh: "仰躺", en: "Lying on the back", desc: "仰躺在地上，雙手放在身體兩側" },
    PoseInfo { key: "carry", zh: "捧著物品", en: "Carrying", desc: "雙手在胸前捧著物品" },
    PoseInfo { key: "phone", zh: "講電話", en: "On the phone", desc: "右手拿手機貼在耳邊講電話" },
    PoseInfo { key: "bow", zh: "鞠躬", en: "Bowing", desc: "上身向前彎腰鞠躬" },
    PoseInfo {
        key: "push", zh: "推（門／物品）", en: "Pushing", desc: "身體前傾、弓步，雙手向前推"
    },
];

/// Look up the metadata of an action pose key.
pub fn action_info(key: &str) -> Option<&'static PoseInfo> {
    ACTIONS.iter().find(|p| p.key == key)
}

/// Torso "up" / "forward" pair leaning forward by `t` (0 = upright).
fn lean(t: f32) -> (Vec3, Vec3) {
    (d(0.0, 1.0, t), d(0.0, -t, 1.0))
}

fn upright(b: &mut B<'_>) {
    b.spine(Vec3::Y, Vec3::Y, d(0.0, 1.0, 0.02), Vec3::Z);
    b.head(Vec3::Y, Vec3::Z);
}

fn stand_legs(b: &mut B<'_>) {
    for (sx, th, _, ft) in SIDES {
        b.limb(th, d(sx * 0.05, -1.0, 0.0), d(sx * 0.05, -1.0, -0.03));
        b.foot(ft, d(sx * 0.15, 0.0, 1.0), Vec3::Y);
    }
}

fn hang_arm(b: &mut B<'_>, sx: f32) {
    let ua = if sx > 0.0 { UpperArmL } else { UpperArmR };
    b.limb(ua, d(sx * 0.17, -1.0, -0.02), d(sx * 0.12, -1.0, 0.12));
}

fn walk_legs(b: &mut B<'_>, front_left: bool, stride: f32) {
    let (front, back, ff, fb, s) =
        if front_left { (ThighL, ThighR, FootL, FootR, 1.0) } else { (ThighR, ThighL, FootR, FootL, -1.0) };
    b.limb(front, d(s * 0.04, -1.0, 0.42 * stride), d(s * 0.04, -1.0, 0.30 * stride));
    b.foot(ff, d(s * 0.08, 0.2, 1.0), d(0.0, 1.0, -0.2));
    b.limb(back, d(-s * 0.04, -1.0, -0.25 * stride), d(-s * 0.04, -0.75, -0.65 * stride));
    b.foot(fb, d(-s * 0.08, -0.55, 0.85), d(0.0, 0.85, 0.55));
}

/// Build an action pose. Returns the pose (snapped to the floor) and the lift
/// above the floor in metres (non-zero for airborne poses such as jumping).
pub fn action_pose(s: &Skeleton, key: &str) -> Option<(Pose, f32)> {
    let mut b = B::new(s);
    let mut lift = 0.0;
    match key {
        "stand" => {
            upright(&mut b);
            stand_legs(&mut b);
            hang_arm(&mut b, 1.0);
            hang_arm(&mut b, -1.0);
        }
        "walk" => {
            b.spine(Vec3::Y, Vec3::Y, d(0.0, 1.0, 0.05), Vec3::Z);
            b.head(Vec3::Y, Vec3::Z);
            walk_legs(&mut b, true, 1.0);
            b.limb(UpperArmR, d(-0.12, -1.0, 0.35), d(-0.08, -0.7, 0.75));
            b.limb(UpperArmL, d(0.14, -1.0, -0.32), d(0.12, -1.0, -0.1));
        }
        "run" => {
            let (u, f) = lean(0.18);
            b.spine(d(0.0, 1.0, 0.12), u, d(0.0, 1.0, 0.22), f);
            b.head(d(0.0, 1.0, 0.05), Vec3::Z);
            b.limb(ThighR, d(-0.05, -0.55, 0.85), d(-0.03, -1.0, -0.1));
            b.foot(FootR, d(0.0, -0.3, 1.0), d(0.0, 1.0, 0.3));
            b.limb(ThighL, d(0.05, -0.85, -0.5), d(0.03, 0.1, -1.0));
            b.foot(FootL, d(0.0, -0.8, -0.6), d(0.0, 0.6, -0.8));
            b.limb(UpperArmL, d(0.1, -0.75, 0.65), d(0.0, 0.55, 0.85));
            b.limb(UpperArmR, d(-0.12, -0.8, -0.6), d(-0.05, -0.35, 0.95));
        }
        "sit" => {
            b.spine(d(0.0, 1.0, -0.05), Vec3::Y, d(0.0, 1.0, 0.03), Vec3::Z);
            b.head(Vec3::Y, Vec3::Z);
            for (sx, th, _, ft) in SIDES {
                b.limb(th, d(sx * 0.12, -0.08, 1.0), d(sx * 0.04, -1.0, -0.06));
                b.foot(ft, d(sx * 0.1, 0.0, 1.0), Vec3::Y);
            }
            for (sx, _, fa, ha) in ARMS {
                let knee = b.pos(if sx > 0.0 { ShinL } else { ShinR });
                let t = b.off(knee, -sx * 0.02, 0.07, -0.16);
                b.ik(fa, t, d(sx * 0.4, -0.3, -1.0));
                b.hand(ha, d(0.0, -0.4, 1.0), -Vec3::Y);
            }
        }
        "point" => {
            upright(&mut b);
            b.head(Vec3::Y, d(-0.25, 0.0, 1.0));
            stand_legs(&mut b);
            hang_arm(&mut b, 1.0);
            b.limb(UpperArmR, d(-0.25, 0.12, 1.0), d(-0.2, 0.16, 1.0));
            b.hand(HandR, d(-0.2, 0.16, 1.0), -Vec3::Y);
        }
        "wave" => {
            upright(&mut b);
            b.head(d(-0.08, 1.0, 0.0), Vec3::Z);
            stand_legs(&mut b);
            hang_arm(&mut b, 1.0);
            b.limb(UpperArmR, d(-0.85, -0.05, 0.2), d(-0.25, 1.0, 0.15));
            b.hand(HandR, d(-0.2, 1.0, 0.1), Vec3::Z);
        }
        "arms_crossed" => {
            upright(&mut b);
            stand_legs(&mut b);
            for (sx, _, fa, _) in ARMS {
                let t = b.at(Chest, -sx * 0.10, if sx > 0.0 { 0.10 } else { 0.07 }, 0.17);
                b.ik(fa, t, d(sx, -0.6, -0.2));
            }
        }
        "hands_on_hips" => {
            upright(&mut b);
            stand_legs(&mut b);
            for (sx, _, fa, ha) in ARMS {
                let t = b.at(Pelvis, sx * 0.17, 0.08, -0.02);
                b.ik(fa, t, d(sx, 0.1, -0.4));
                b.hand(ha, d(-sx * 0.3, -0.5, 0.6), d(-sx, 0.0, 0.0));
            }
        }
        "talk" => {
            upright(&mut b);
            b.head(d(0.05, 1.0, 0.0), d(-0.15, 0.0, 1.0));
            stand_legs(&mut b);
            b.limb(UpperArmR, d(-0.2, -1.0, 0.25), d(-0.25, 0.05, 1.0));
            b.hand(HandR, d(-0.3, 0.1, 1.0), Vec3::Y);
            b.limb(UpperArmL, d(0.2, -1.0, 0.1), d(0.25, -0.3, 1.0));
            b.hand(HandL, d(0.35, -0.1, 1.0), d(0.3, 1.0, 0.0));
        }
        "think" => {
            upright(&mut b);
            b.head(d(-0.08, 1.0, 0.1), d(0.0, -0.15, 1.0));
            stand_legs(&mut b);
            let chin = b.at(Head, -0.02, -0.08, 0.14);
            b.ik(ForearmR, chin, d(-0.2, -1.0, 0.3));
            b.hand(HandR, d(0.05, 1.0, 0.2), d(0.0, 0.0, -1.0));
            let elbow = b.at(Chest, -0.08, -0.02, 0.16);
            b.ik(ForearmL, elbow, d(1.0, -0.5, -0.2));
        }
        "crouch" => {
            let (u, f) = lean(0.6);
            b.spine(d(0.0, 1.0, 0.5), u, d(0.0, 1.0, 0.45), f);
            b.head(Vec3::Y, Vec3::Z);
            for (sx, th, _, ft) in SIDES {
                b.limb(th, d(sx * 0.35, -0.15, 1.0), d(sx * 0.05, -1.0, -0.45));
                b.foot(ft, d(sx * 0.2, 0.0, 1.0), Vec3::Y);
            }
            for (sx, _, fa, _) in ARMS {
                let knee = b.pos(if sx > 0.0 { ShinL } else { ShinR });
                let t = b.off(knee, -sx * 0.1, 0.02, 0.2);
                b.ik(fa, t, d(sx * 0.3, -0.5, -1.0));
            }
        }
        "jump" => {
            b.spine(Vec3::Y, Vec3::Y, d(0.0, 1.0, -0.05), Vec3::Z);
            b.head(d(0.0, 1.0, -0.2), d(0.0, 0.2, 1.0));
            for (sx, th, _, ft) in SIDES {
                b.limb(th, d(sx * 0.12, -0.45, 0.9), d(sx * 0.05, -1.0, -0.55));
                b.foot(ft, d(0.0, -0.7, 0.7), d(0.0, 0.7, 0.7));
            }
            for (sx, ua, _, ha) in ARMS {
                b.limb(ua, d(sx * 0.5, 1.0, 0.15), d(sx * 0.4, 1.0, 0.25));
                b.hand(ha, d(sx * 0.4, 1.0, 0.25), Vec3::Z);
            }
            lift = 0.35 * s.scale();
        }
        "reach_up" => {
            upright(&mut b);
            b.head(d(0.0, 1.0, -0.35), d(0.0, 0.4, 1.0));
            for (sx, th, _, ft) in SIDES {
                b.limb(th, d(sx * 0.05, -1.0, 0.0), d(sx * 0.05, -1.0, -0.03));
                b.foot(ft, d(sx * 0.1, -0.75, 0.65), d(0.0, 0.65, 0.75));
            }
            hang_arm(&mut b, 1.0);
            b.limb(UpperArmR, d(-0.1, 1.0, 0.12), d(-0.08, 1.0, 0.18));
            b.hand(HandR, d(-0.08, 1.0, 0.18), Vec3::Z);
        }
        "kneel" => {
            upright(&mut b);
            b.limb(ThighL, d(0.08, 0.0, 1.0), d(0.04, -1.0, -0.03));
            b.foot(FootL, d(0.1, 0.0, 1.0), Vec3::Y);
            b.limb(ThighR, d(-0.06, -1.0, -0.05), d(-0.02, -0.08, -1.0));
            b.foot(FootR, d(0.0, -0.1, -1.0), d(0.0, -1.0, 0.1));
            let knee = b.pos(ShinL);
            let t = b.off(knee, -0.02, 0.06, -0.06);
            b.ik(ForearmL, t, d(0.5, -0.3, -1.0));
            hang_arm(&mut b, -1.0);
        }
        "lie" => {
            b.root(0.0, 0.12, 0.0);
            let up = d(-1.0, 0.0, 0.0);
            b.orient(Pelvis, up, Vec3::Y).orient(Waist, up, Vec3::Y).orient(Chest, up, Vec3::Y);
            b.head(d(-1.0, 0.05, 0.0), Vec3::Y);
            // The figure's left side is world -Z in this orientation.
            b.limb(ThighL, d(1.0, 0.0, -0.06), d(1.0, -0.03, -0.06));
            b.limb(ThighR, d(1.0, 0.0, 0.06), d(1.0, -0.03, 0.06));
            for ft in [FootL, FootR] {
                b.foot(ft, d(0.3, 1.0, 0.0), d(-1.0, 0.3, 0.0));
            }
            b.limb(UpperArmL, d(1.0, -0.05, -0.25), d(1.0, 0.05, -0.2));
            b.limb(UpperArmR, d(1.0, -0.05, 0.25), d(1.0, 0.05, 0.2));
        }
        "carry" => {
            upright(&mut b);
            b.head(d(0.0, 1.0, 0.05), d(0.0, -0.15, 1.0));
            stand_legs(&mut b);
            for (sx, _, fa, ha) in ARMS {
                let t = b.at(Chest, sx * 0.16, -0.02, 0.33);
                b.ik(fa, t, d(sx * 0.6, -1.0, -0.2));
                b.hand(ha, d(-sx * 0.2, 0.0, 1.0), d(-sx, 0.0, 0.0));
            }
        }
        "phone" => {
            upright(&mut b);
            b.head(d(-0.1, 1.0, 0.0), Vec3::Z);
            stand_legs(&mut b);
            hang_arm(&mut b, 1.0);
            let ear = b.at(Head, -0.11, -0.02, 0.06);
            b.ik(ForearmR, ear, d(-0.3, -1.0, 0.4));
            b.hand(HandR, d(0.1, 1.0, 0.0), d(1.0, 0.0, 0.0));
        }
        "bow" => {
            let (u0, f0) = lean(0.25);
            let (u1, f1) = lean(0.9);
            let (u2, f2) = lean(1.1);
            b.orient(Pelvis, u0, f0).orient(Waist, u1, f1).orient(Chest, u2, f2);
            let (uh, fh) = lean(1.2);
            b.head(uh, fh);
            stand_legs(&mut b);
            for (sx, ua, _, _) in ARMS {
                b.limb(ua, d(sx * 0.08, -1.0, 0.05), d(sx * 0.05, -1.0, 0.1));
            }
        }
        "push" => {
            let (u, f) = lean(0.15);
            b.spine(d(0.0, 1.0, 0.1), u, d(0.0, 1.0, 0.18), f);
            b.head(Vec3::Y, Vec3::Z);
            b.limb(ThighL, d(0.05, -1.0, 0.35), d(0.04, -1.0, -0.05));
            b.foot(FootL, d(0.1, 0.0, 1.0), Vec3::Y);
            b.limb(ThighR, d(-0.05, -1.0, -0.35), d(-0.05, -1.0, -0.38));
            b.foot(FootR, d(-0.05, -0.4, 1.0), d(0.0, 1.0, 0.4));
            for (sx, ua, _, ha) in ARMS {
                b.limb(ua, d(sx * 0.12, -0.1, 1.0), d(sx * 0.05, 0.15, 1.0));
                b.hand(ha, d(0.0, 1.0, 0.2), Vec3::Z);
            }
        }
        _ => return None,
    }
    let mut pose = b.done();
    snap_to_floor(s, &mut pose);
    pose.root = v3(0.0, pose.root.y, 0.0);
    Some((pose, lift))
}

/// Any library pose: an action key or `"floor:N"` (1-based rust-pose-studio floor pose).
pub fn library_pose(s: &Skeleton, key: &str) -> Option<(Pose, f32)> {
    if let Some(n) = key.strip_prefix("floor:") {
        let i: usize = n.parse().ok()?;
        let mut p = presets(s).into_iter().nth(i.checked_sub(1)?)?.pose;
        p.root.x = 0.0;
        p.root.z = 0.0;
        return Some((p, 0.0));
    }
    action_pose(s, key)
}

/// Names of the floor poses (index 0 = `"floor:1"`), in English as in rust-pose-studio.
pub fn floor_pose_names(s: &Skeleton) -> Vec<&'static str> {
    presets(s).into_iter().map(|p| p.name).collect()
}

/// Display name of a pose key (Traditional Chinese, English in parentheses).
pub fn pose_label(key: &str) -> String {
    if let Some(i) = action_info(key) {
        return format!("{}（{}）", i.zh, i.en);
    }
    if let Some(n) = key.strip_prefix("floor:") {
        let s = Skeleton::new(Default::default());
        let names = floor_pose_names(&s);
        if let Some(name) = n.parse::<usize>().ok().and_then(|i| names.get(i.wrapping_sub(1))) {
            return format!("地面姿勢 {n}（{name}）");
        }
    }
    if key.is_empty() { "自訂姿勢".to_string() } else { key.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mannequin::body::Body;
    use crate::mannequin::skeleton::{BodyType, Joint, Proportions};

    #[test]
    fn every_action_is_valid_and_grounded() {
        for props in [
            Proportions::default(),
            Proportions { height: 1.3, head_size: 1.2, body_type: BodyType::Slim, ..Proportions::default() },
            Proportions { height: 1.9, shoulder_width: 1.2, body_type: BodyType::Curvy, ..Proportions::default() },
        ] {
            let s = Skeleton::new(props);
            for info in ACTIONS {
                let (pose, lift) = action_pose(&s, info.key).unwrap_or_else(|| panic!("{} missing", info.key));
                assert!(pose.is_finite(), "{} has NaNs", info.key);
                let body = Body::new(&s, &pose);
                assert!(body.min_y().abs() < 1e-3, "{} min_y {}", info.key, body.min_y());
                let (lo, hi) = body.bounds();
                assert!(hi.y < 2.6 && (hi - lo).length() < 3.2, "{} too large", info.key);
                assert!(lift >= 0.0);
                for j in [Joint::HandL, Joint::HandR, Joint::Head] {
                    assert!(body.fk.tip(j).y > -0.02, "{} {j:?} below floor", info.key);
                }
            }
        }
    }

    #[test]
    fn actions_are_distinct_and_keys_unique() {
        let s = Skeleton::new(Proportions::default());
        let poses: Vec<_> = ACTIONS.iter().map(|i| (i.key, action_pose(&s, i.key).unwrap().0)).collect();
        for (i, (ka, a)) in poses.iter().enumerate() {
            for (kb, b) in &poses[i + 1..] {
                assert_ne!(ka, kb);
                let diff: f32 = a.rot.iter().flatten().zip(b.rot.iter().flatten()).map(|(x, y)| (x - y).abs()).sum();
                assert!(diff > 30.0, "{ka} and {kb} are too similar");
            }
        }
    }

    #[test]
    fn floor_poses_resolve() {
        let s = Skeleton::new(Proportions::default());
        assert!(library_pose(&s, "floor:1").is_some());
        assert!(library_pose(&s, "floor:25").is_some());
        assert!(library_pose(&s, "floor:0").is_none());
        assert!(library_pose(&s, "floor:26").is_none());
        assert!(library_pose(&s, "nope").is_none());
        assert!(pose_label("floor:3").starts_with("地面姿勢 3"));
        assert_eq!(pose_label("walk"), "行走（Walking）");
    }
}
