//! Built-in sample project: 「咖啡店的相遇」 — three shots, three characters,
//! props, movement paths and dialogue.

use crate::mannequin::actions::library_pose;
use crate::mannequin::skeleton::{BodyType, Skeleton};
use crate::model::*;

/// Make an actor of `cast` with a library pose.
#[allow(clippy::too_many_arguments)]
pub fn actor(p: &Project, id: &str, cast: &str, pose: &str, x: f32, y: f32, facing: f32, scale: f32) -> Actor {
    let props = p.cast_member(cast).map(|c| c.proportions()).unwrap_or_default();
    let (pz, lift) = library_pose(&Skeleton::new(props), pose)
        .unwrap_or_else(|| library_pose(&Skeleton::new(props), "stand").expect("stand pose exists"));
    Actor {
        id: id.into(),
        cast: cast.into(),
        x,
        y,
        facing,
        scale,
        pose: PoseData::from_pose(pose, &pz, lift),
        ..Actor::default()
    }
}

/// Make a prop with its default size for the canvas scale.
pub fn prop(p: &Project, id: &str, kind: PropKind, x: f32, y: f32, scale: f32) -> Prop {
    let (w, h) = kind.default_size_m();
    let ppm = p.base_ppm() * scale;
    Prop { id: id.into(), kind, x, y, w: w * ppm, h: h * ppm, ..Prop::default() }
}

pub fn sample_project() -> Project {
    let mut p = Project {
        title: "咖啡店的相遇（範例）".into(),
        synopsis: "大學生小明每天下午都會到街角的「晨光咖啡」報到。今天，店員小美為他端上一杯特別的拿鐵，店長老陳在一旁默默看著。"
            .into(),
        ..Project::default()
    };
    p.cast = vec![
        CastMember {
            id: "char_1".into(),
            name: "小明".into(),
            description: "大學生，20 歲，背著後背包，個性急躁但善良。".into(),
            color: CAST_COLORS[0],
            height: 1.76,
            body_type: BodyType::Masculine,
            head_size: 1.0,
        },
        CastMember {
            id: "char_2".into(),
            name: "小美".into(),
            description: "咖啡店店員，22 歲，綁馬尾、穿圍裙，笑容親切。".into(),
            color: CAST_COLORS[1],
            height: 1.62,
            body_type: BodyType::Slim,
            head_size: 1.0,
        },
        CastMember {
            id: "char_3".into(),
            name: "老陳".into(),
            description: "咖啡店店長，50 多歲，話不多，總是在吧台後觀察客人。".into(),
            color: CAST_COLORS[2],
            height: 1.72,
            body_type: BodyType::Masculine,
            head_size: 1.05,
        },
    ];

    // Perspective-consistent scale for a ground y in a shot with horizon `hz` (fraction).
    let ds = |hz: f32, y: f32| ((y - hz * p.canvas.height as f32) / (CAMERA_HEIGHT_M * p.base_ppm())).clamp(0.05, 4.0);

    // ---- Shot 1: outside the café
    let hz1 = 0.40;
    let mut s1 = Shot {
        id: "shot_1".into(),
        name: "咖啡店門口".into(),
        duration: 4.0,
        setting: Setting {
            location: "街角「晨光咖啡」門口".into(),
            in_out: InOut::Exterior,
            time_of_day: TimeOfDay::Afternoon,
            weather: "晴朗，陽光斜照".into(),
            environment: "安靜的巷弄，人行道旁有行道樹與長椅，咖啡店掛著木製招牌。".into(),
        },
        camera: ShotCamera {
            size: ShotSize::Full,
            angle: CameraAngle::EyeLevel,
            movement: CameraMove::PanRight,
            notes: "跟著小明的移動慢慢向右搖。".into(),
        },
        horizon: hz1,
        narration: "午後的街角，陽光灑在咖啡店的招牌上。小明一如往常，快步走向那扇熟悉的木門。".into(),
        notes: "開場建立地點，讓觀眾看清楚門口與招牌。".into(),
        ..Shot::default()
    };
    let s = ds(hz1, 640.0);
    s1.props = vec![
        Prop {
            w: 1400.0,
            h: 560.0,
            label: "咖啡店".into(),
            ..prop(&p, "building_1", PropKind::Building, 1330.0, 640.0, 1.0)
        },
        Prop { label: "咖啡店大門".into(), ..prop(&p, "door_1", PropKind::Door, 1120.0, 640.0, s * 1.05) },
        Prop { elevation: 0.9 * p.base_ppm() * s, ..prop(&p, "window_1", PropKind::Window, 1480.0, 640.0, s) },
        Prop { elevation: 0.9 * p.base_ppm() * s, ..prop(&p, "window_2", PropKind::Window, 1780.0, 640.0, s) },
        Prop { label: "晨光咖啡".into(), ..prop(&p, "sign_1", PropKind::Sign, 880.0, 700.0, ds(hz1, 700.0)) },
        prop(&p, "tree_1", PropKind::Tree, 200.0, 690.0, ds(hz1, 690.0)),
        prop(&p, "bench_1", PropKind::Bench, 500.0, 730.0, ds(hz1, 730.0)),
        Prop { flip: true, ..prop(&p, "lamp_1", PropKind::Lamp, 700.0, 665.0, ds(hz1, 665.0)) },
    ];
    let mut ming = actor(&p, "actor_1", "char_1", "walk", 300.0, 1010.0, 90.0, ds(hz1, 1010.0));
    ming.action = "背著後背包，快步從畫面左側走向咖啡店大門".into();
    ming.expression = "期待、心情很好".into();
    ming.movement = Movement {
        enabled: true,
        path: vec![[760.0, 870.0], [1100.0, 672.0]],
        style: MoveStyle::Walk,
        start: 0.0,
        end: 3.2,
        end_pose: "push".into(),
        end_facing: Some(160.0),
        description: "沿著人行道斜斜走向門口，最後伸手推門".into(),
        ..Movement::default()
    };
    let mut mei = actor(&p, "actor_2", "char_2", "wave", 1420.0, 730.0, -60.0, ds(hz1, 730.0));
    mei.action = "站在店門旁，看到小明後舉手揮手打招呼".into();
    mei.expression = "微笑".into();
    mei.dialogue = "嗨！今天也是老樣子嗎？".into();
    ming.dialogue = "今天一定要試試新口味……".into();
    ming.bubble = BubbleSettings { style: BubbleStyle::Thought, ..BubbleSettings::default() };
    s1.actors = vec![ming, mei];
    s1.narration_box = true;

    // ---- Shot 2: at the counter
    let hz2 = 0.36;
    let mut s2 = Shot {
        id: "shot_2".into(),
        name: "店內櫃台點餐".into(),
        duration: 5.0,
        setting: Setting {
            location: "晨光咖啡店內・吧台".into(),
            in_out: InOut::Interior,
            time_of_day: TimeOfDay::Afternoon,
            weather: "室內暖色燈光".into(),
            environment: "木質吧台、牆上的菜單螢幕與書櫃，空氣中有咖啡香。".into(),
        },
        camera: ShotCamera {
            size: ShotSize::MediumWide,
            angle: CameraAngle::High,
            movement: CameraMove::DollyIn,
            notes: "從高處慢慢推近吧台。".into(),
        },
        horizon: hz2,
        narration: "小明走到吧台前，指著牆上的菜單點了他的老樣子；小美一邊聽一邊比手勢確認。".into(),
        ..Shot::default()
    };
    let sc = ds(hz2, 730.0);
    s2.props = vec![
        Prop {
            w: 320.0,
            h: 300.0,
            label: "菜單螢幕".into(),
            elevation: 110.0,
            ..prop(&p, "screen_1", PropKind::Screen, 1500.0, 560.0, 1.0)
        },
        prop(&p, "shelf_1", PropKind::Shelf, 330.0, 560.0, ds(hz2, 560.0)),
        Prop { w: 820.0, label: "吧台".into(), ..prop(&p, "counter_1", PropKind::Counter, 1220.0, 730.0, sc) },
        Prop {
            elevation: 1.05 * p.base_ppm() * sc,
            label: "咖啡杯".into(),
            ..prop(&p, "cup_1", PropKind::Cup, 1000.0, 732.0, sc * 2.0)
        },
        prop(&p, "plant_1", PropKind::Plant, 1800.0, 920.0, ds(hz2, 920.0)),
    ];
    let mut mei2 = actor(&p, "actor_1", "char_2", "talk", 1260.0, 690.0, -15.0, ds(hz2, 690.0));
    mei2.action = "站在吧台後方，一邊聽點餐一邊比手勢".into();
    mei2.dialogue = "一杯拿鐵、少冰，對吧？".into();
    mei2.bubble = BubbleSettings { vertical: true, ..BubbleSettings::default() };
    let mut ming2 = actor(&p, "actor_2", "char_1", "walk", 300.0, 1010.0, 70.0, ds(hz2, 1010.0));
    ming2.action = "走到吧台前，伸手指向牆上的菜單".into();
    ming2.dialogue = "對！再加一份肉桂！".into();
    ming2.bubble = BubbleSettings { style: BubbleStyle::Shout, ..BubbleSettings::default() };
    ming2.movement = Movement {
        enabled: true,
        path: vec![[520.0, 940.0], [760.0, 925.0]],
        style: MoveStyle::Stroll,
        start: 0.0,
        end: 2.0,
        end_pose: "point".into(),
        end_facing: Some(60.0),
        ghost: true,
        description: "慢慢走到吧台前，停下來指向菜單".into(),
        ..Movement::default()
    };
    s2.actors = vec![mei2, ming2];

    // ---- Shot 3: window seat
    let hz3 = 0.44;
    let mut s3 = Shot {
        id: "shot_3".into(),
        name: "靠窗的座位".into(),
        duration: 6.0,
        setting: Setting {
            location: "晨光咖啡店內・靠窗座位".into(),
            in_out: InOut::Interior,
            time_of_day: TimeOfDay::Dusk,
            weather: "夕陽從窗外照進來".into(),
            environment: "窗邊的小圓桌與兩張椅子，窗外是逐漸變暗的街道。".into(),
        },
        camera: ShotCamera {
            size: ShotSize::MediumWide,
            angle: CameraAngle::EyeLevel,
            movement: CameraMove::Static,
            notes: String::new(),
        },
        horizon: hz3,
        narration: "黃昏時分，小美端著拿鐵走向坐在窗邊的小明。吧台後的老陳雙手抱胸，嘴角微微上揚。".into(),
        notes: "結尾鏡頭，保留老陳在背景的反應。".into(),
        ..Shot::default()
    };
    let st = ds(hz3, 900.0);
    s3.props = vec![
        Prop {
            w: 600.0,
            h: 330.0,
            label: "大窗戶".into(),
            ..prop(&p, "window_1", PropKind::Window, 900.0, 455.0, 1.0)
        },
        Prop {
            w: 620.0,
            label: "吧台".into(),
            ..prop(&p, "counter_1", PropKind::Counter, 1650.0, 612.0, ds(hz3, 612.0))
        },
        prop(&p, "chair_1", PropKind::Chair, 585.0, 885.0, ds(hz3, 885.0)),
        Prop { flip: true, ..prop(&p, "chair_2", PropKind::Chair, 1270.0, 880.0, ds(hz3, 880.0)) },
        Prop { w: 1.1 * p.base_ppm() * st, ..prop(&p, "table_1", PropKind::Table, 930.0, 905.0, st) },
        Prop {
            elevation: 0.75 * p.base_ppm() * st,
            label: "拿鐵".into(),
            ..prop(&p, "cup_1", PropKind::Cup, 890.0, 908.0, st * 2.0)
        },
        prop(&p, "plant_1", PropKind::Plant, 230.0, 950.0, ds(hz3, 950.0)),
    ];
    let mut ming3 = actor(&p, "actor_1", "char_1", "sit", 610.0, 900.0, 80.0, ds(hz3, 900.0));
    ming3.action = "坐在窗邊的椅子上，轉頭看向走過來的小美".into();
    ming3.expression = "驚喜".into();
    let mut mei3 = actor(&p, "actor_2", "char_2", "carry", 1720.0, 1000.0, -90.0, ds(hz3, 1000.0));
    mei3.action = "雙手捧著拿鐵，從右側走向桌邊".into();
    mei3.dialogue = "請慢用，今天的拉花是愛心喔。".into();
    mei3.bubble = BubbleSettings { style: BubbleStyle::Whisper, ..BubbleSettings::default() };
    mei3.movement = Movement {
        enabled: true,
        path: vec![[1450.0, 960.0], [1290.0, 935.0]],
        style: MoveStyle::Walk,
        start: 0.5,
        end: 3.5,
        end_pose: "carry".into(),
        description: "小心地端著咖啡走到桌邊".into(),
        ..Movement::default()
    };
    let mut chen = actor(&p, "actor_3", "char_3", "arms_crossed", 1500.0, 600.0, -40.0, ds(hz3, 600.0));
    chen.action = "在吧台後雙手抱胸，看著兩人".into();
    chen.expression = "嘴角微微上揚".into();
    chen.dialogue = "店長老陳默默記下：這是小明的第 100 杯拿鐵。".into();
    chen.bubble =
        BubbleSettings { style: BubbleStyle::Narration, offset: [-160.0, -230.0], ..BubbleSettings::default() };
    s3.actors = vec![chen, ming3, mei3];

    p.shots = vec![s1, s2, s3];
    p
}

#[cfg(test)]
mod tests {
    #[test]
    fn sample_is_consistent() {
        let p = super::sample_project();
        assert!(p.shots.len() >= 3);
        for s in &p.shots {
            for a in &s.actors {
                assert!(p.cast_member(&a.cast).is_some(), "{} has unknown cast {}", a.id, a.cast);
                assert_eq!(a.pose.joints.len(), 17);
            }
        }
        assert!(p.shots.iter().any(|s| s.actors.iter().any(|a| a.movement.is_active())));
    }
}
