//! Natural-language (Traditional Chinese) description of shots: where every
//! character stands, which way they face, their pose, how they move, where the
//! props are, plus an auto-generated narrative paragraph. Used by the Markdown and
//! HTML exports (the "describe the layout so an agent need not look at the image"
//! idea of whitebox-video-storyboard, applied to characters and staging).

use crate::draw::circled;
use crate::figure::{FigureSpec, end_scale_factor};
use crate::mannequin::actions::{action_info, pose_label};
use crate::model::{Actor, Project, Prop, Shot};

/// Nine-grid region of a canvas point.
pub fn region(x: f32, y: f32, cw: f32, ch: f32) -> String {
    let col = ((x / cw) * 3.0).floor().clamp(0.0, 2.0) as usize;
    let row = ((y / ch) * 3.0).floor().clamp(0.0, 2.0) as usize;
    match (row, col) {
        (1, 1) => "畫面正中央".into(),
        (r, c) => {
            let v = ["上", "中", "下"][r];
            let h = ["左", "中", "右"][c];
            match (r, c) {
                (1, _) => format!("畫面{h}側中段"),
                (_, 1) => format!("畫面{v}方正中"),
                _ => format!("畫面{h}{v}方"),
            }
        }
    }
}

/// Depth layer from the ground y relative to the horizon.
pub fn depth_layer(y: f32, horizon_px: f32, ch: f32) -> &'static str {
    if y < horizon_px {
        return "背景（地平線以上）";
    }
    let t = (y - horizon_px) / (ch - horizon_px).max(1.0);
    if t < 0.3 {
        "遠景（背景）"
    } else if t < 0.65 {
        "中景"
    } else {
        "前景"
    }
}

fn norm_deg(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a > 180.0 {
        a -= 360.0;
    } else if a <= -180.0 {
        a += 360.0;
    }
    a
}

/// Facing description (0 = towards the camera, 90 = screen right).
pub fn facing_desc(f: f32) -> String {
    let a = norm_deg(f);
    let side = if a > 0.0 { "右" } else { "左" };
    let m = a.abs();
    if m <= 22.5 {
        "正面朝向鏡頭".into()
    } else if m <= 67.5 {
        format!("斜向畫面{side}前方（3/4 側身）")
    } else if m <= 112.5 {
        format!("面向畫面{side}側（側面）")
    } else if m <= 157.5 {
        format!("半背對鏡頭，朝畫面{side}後方")
    } else {
        "背對鏡頭".into()
    }
}

/// Screen-x component of a facing (+1 = right, -1 = left).
fn facing_x(f: f32) -> f32 {
    norm_deg(f).to_radians().sin()
}

/// Direction words for a canvas displacement.
pub fn direction_desc(dx: f32, dy: f32) -> String {
    let h = if dx > 20.0 {
        Some("向畫面右方")
    } else if dx < -20.0 {
        Some("向畫面左方")
    } else {
        None
    };
    let d = if dy > 20.0 {
        Some("朝鏡頭走近（往前景）")
    } else if dy < -20.0 {
        Some("往畫面深處（遠離鏡頭）")
    } else {
        None
    };
    match (h, d) {
        (Some(h), Some(d)) => format!("{h}、同時{d}"),
        (Some(h), None) => format!("{h}橫向移動"),
        (None, Some(d)) => d.to_string(),
        (None, None) => "原地".into(),
    }
}

fn pct(v: f32, total: f32) -> String {
    format!("{:.0}%", v / total.max(1.0) * 100.0)
}

/// Approximate metres for a canvas distance at the given figure scale.
fn metres(px: f32, p: &Project, scale: f32) -> f32 {
    px / (p.base_ppm() * scale.max(0.05))
}

/// Pose description: library name + description (+ "edited").
pub fn pose_desc(a: &Actor) -> String {
    let mut s = pose_label(&a.pose.preset);
    if let Some(i) = action_info(&a.pose.preset) {
        s.push_str(&format!("：{}", i.desc));
    }
    if a.pose.edited {
        s.push_str("（已手動微調關節）");
    }
    if a.pose.lift > 0.01 {
        s.push_str(&format!("，離地約 {:.2} 公尺", a.pose.lift));
    }
    s
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveDesc {
    pub number: usize,
    pub text: String,
    pub start: f32,
    pub end: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActorDesc {
    pub id: String,
    pub name: String,
    pub color: String,
    pub cast_desc: String,
    pub position: String,
    pub facing: String,
    pub pose: String,
    pub size: String,
    pub action: String,
    pub expression: String,
    pub dialogue: String,
    /// Balloon style of the dialogue.
    pub bubble: crate::model::BubbleStyle,
    /// e.g. "對話泡泡（圓角橢圓＋尖尾）・直書"
    pub bubble_desc: String,
    pub movement: Option<MoveDesc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropDesc {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub position: String,
    pub size: String,
    pub relation: String,
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShotDesc {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub start: f32,
    pub duration: f32,
    pub location: String,
    pub in_out: String,
    pub time_of_day: String,
    pub weather: String,
    pub environment: String,
    pub camera: String,
    pub camera_notes: String,
    pub narrative: String,
    pub narration: String,
    /// The narration is also drawn as a caption box in the draft.
    pub narration_box: bool,
    pub notes: String,
    pub actors: Vec<ActorDesc>,
    pub relations: Vec<String>,
    pub props: Vec<PropDesc>,
    pub png: String,
}

/// File name of a shot's draft image.
pub fn shot_png_name(i: usize, id: &str) -> String {
    let safe: String =
        id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    format!("shot_{:02}_{}.png", i + 1, safe)
}

fn quote(s: &str) -> String {
    format!("「{}」", s.trim())
}

fn movement_desc(p: &Project, shot: &Shot, a: &Actor, n: usize) -> Option<MoveDesc> {
    let m = &a.movement;
    if !m.is_active() {
        return None;
    }
    let (cw, ch) = (p.canvas.width as f32, p.canvas.height as f32);
    let pts = a.path_points();
    let e = *pts.last().unwrap();
    let len: f32 = pts.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum();
    let es = end_scale_factor(p, shot, a);
    let scale_avg = a.scale * (1.0 + es) / 2.0;
    let dist = metres(len, p, scale_avg);
    let dur = (m.end - m.start).max(0.01);
    let mut t = format!(
        "{:.1}–{:.1} 秒（{:.1} 秒），以「{}」的方式從{}（{:.0}, {:.0}）移動到{}（{:.0}, {:.0}）",
        m.start,
        m.end,
        dur,
        m.style.zh(),
        region(a.x, a.y, cw, ch),
        a.x,
        a.y,
        region(e[0], e[1], cw, ch),
        e[0],
        e[1]
    );
    if pts.len() > 2 {
        let via: Vec<String> = pts[1..pts.len() - 1].iter().map(|q| format!("（{:.0}, {:.0}）", q[0], q[1])).collect();
        t.push_str(&format!("，途經 {}", via.join("、")));
    }
    t.push_str(&format!(
        "；整體方向：{}；路徑長約 {:.1} 公尺，平均速度約 {:.1} 公尺/秒",
        direction_desc(e[0] - a.x, e[1] - a.y),
        dist,
        dist / dur
    ));
    if (es - 1.0).abs() > 0.05 {
        t.push_str(&format!("；因景深變化，終點時人物大小約為起點的 {:.0}%", es * 100.0));
    }
    if !m.end_pose.is_empty() && m.end_pose != a.pose.preset {
        t.push_str(&format!("；抵達後改為「{}」姿勢", pose_label(&m.end_pose)));
    }
    if let Some(f) = FigureSpec::ghost_of_actor(p, shot, a).map(|g| g.facing).or(m.end_facing)
        && (norm_deg(f) - norm_deg(a.facing)).abs() > 20.0
    {
        t.push_str(&format!("；終點朝向：{}", facing_desc(f)));
    }
    if !m.description.trim().is_empty() {
        t.push_str(&format!("。說明：{}", m.description.trim()));
    }
    Some(MoveDesc { number: n, text: t, start: m.start, end: m.end })
}

fn relations(p: &Project, shot: &Shot) -> Vec<String> {
    let mut out = vec![];
    let acts = &shot.actors;
    for i in 0..acts.len() {
        for j in i + 1..acts.len() {
            let (a, b) = (&acts[i], &acts[j]);
            let (na, nb) = (p.cast_of(a).name, p.cast_of(b).name);
            let (l, r, nl, nr) = if a.x <= b.x { (a, b, &na, &nb) } else { (b, a, &nb, &na) };
            let s_avg = (a.scale + b.scale) / 2.0;
            let d = metres(((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt(), p, s_avg);
            let mut s = format!("{nl}在{nr}的左側，兩人相距約 {d:.1} 公尺（畫面上）");
            if (a.y - b.y).abs() > 40.0 {
                let (near, far) = if a.y > b.y { (&na, &nb) } else { (&nb, &na) };
                s.push_str(&format!("；{near}比{far}更靠近鏡頭"));
            }
            let (fl, fr) = (facing_x(l.facing), facing_x(r.facing));
            if fl > 0.35 && fr < -0.35 {
                s.push_str("；兩人面對面");
            } else if fl < -0.35 && fr > 0.35 {
                s.push_str("；兩人背對背");
            } else if fl > 0.35 {
                s.push_str(&format!("；{nl}朝向{nr}"));
            } else if fr < -0.35 {
                s.push_str(&format!("；{nr}朝向{nl}"));
            }
            out.push(s + "。");
        }
    }
    out
}

fn prop_relation(p: &Project, shot: &Shot, pr: &Prop) -> String {
    let near = shot
        .actors
        .iter()
        .map(|a| (a, ((a.x - pr.x).powi(2) + (a.y - pr.y).powi(2)).sqrt()))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let Some((a, d)) = near else { return String::new() };
    let m = metres(d, p, a.scale);
    if m > 3.5 {
        return String::new();
    }
    let name = p.cast_of(a).name;
    let side = if (pr.x - a.x).abs() < pr.w * 0.25 {
        if pr.y < a.y { "後方" } else { "前方" }
    } else if pr.x > a.x {
        "右側"
    } else {
        "左側"
    };
    format!("在{name}的{side}（約 {m:.1} 公尺）")
}

/// Describe one shot.
pub fn describe_shot(p: &Project, i: usize) -> ShotDesc {
    let shot = &p.shots[i];
    let (cw, ch) = (p.canvas.width as f32, p.canvas.height as f32);
    let hz = shot.horizon * ch;
    let start = p.shot_starts()[i];
    let mut n_move = 0;
    let mut actors = vec![];
    for a in &shot.actors {
        let cast = p.cast_of(a);
        let f = FigureSpec::of_actor(p, shot, a);
        let b = f.bounds();
        let hpx = (b[3] - b[1]).max(1.0);
        let movement = if a.movement.is_active() {
            n_move += 1;
            movement_desc(p, shot, a, n_move)
        } else {
            None
        };
        actors.push(ActorDesc {
            id: a.id.clone(),
            name: cast.name.clone(),
            color: cast.color.hex(),
            cast_desc: cast.description.clone(),
            position: format!(
                "{}・{}；站立點 ({:.0}, {:.0})（距左 {}、距上 {}）",
                region(a.x, a.y, cw, ch),
                depth_layer(a.y, hz, ch),
                a.x,
                a.y,
                pct(a.x, cw),
                pct(a.y, ch)
            ),
            facing: facing_desc(a.facing),
            pose: pose_desc(a),
            size: format!("人物高度約佔畫面高 {}（比例 {:.2}）", pct(hpx, ch), a.scale),
            action: a.action.trim().to_string(),
            expression: a.expression.trim().to_string(),
            dialogue: a.dialogue.trim().to_string(),
            bubble: a.bubble.style,
            bubble_desc: format!("{}{}", a.bubble.style.shape_zh(), if a.bubble.vertical { "・直書" } else { "" }),
            movement,
        });
    }
    let props = shot
        .props
        .iter()
        .map(|pr| PropDesc {
            id: pr.id.clone(),
            name: pr.display_name(),
            kind: pr.kind.zh().to_string(),
            position: format!(
                "{}・{}{}",
                region(pr.x, pr.y - pr.elevation - pr.h / 2.0, cw, ch),
                depth_layer(pr.y, hz, ch),
                if pr.elevation > 1.0 { "（放在較高的平面上）" } else { "" }
            ),
            size: format!(
                "{:.0} × {:.0} px（約 {:.1} × {:.1} 公尺）",
                pr.w,
                pr.h,
                metres(pr.w, p, 1.0),
                metres(pr.h, p, 1.0)
            ),
            relation: prop_relation(p, shot, pr),
            notes: pr.notes.trim().to_string(),
        })
        .collect();
    let camera = format!(
        "{}（{}）・{}・{}",
        shot.camera.size.zh(),
        shot.camera.size.en(),
        shot.camera.angle.zh(),
        shot.camera.movement.zh()
    );
    let mut d = ShotDesc {
        index: i,
        id: shot.id.clone(),
        name: shot.name.clone(),
        start,
        duration: shot.duration,
        location: shot.setting.location.trim().to_string(),
        in_out: shot.setting.in_out.zh().to_string(),
        time_of_day: shot.setting.time_of_day.zh().to_string(),
        weather: shot.setting.weather.trim().to_string(),
        environment: shot.setting.environment.trim().to_string(),
        camera,
        camera_notes: shot.camera.notes.trim().to_string(),
        narrative: String::new(),
        narration: shot.narration.trim().to_string(),
        narration_box: shot.narration_box,
        notes: shot.notes.trim().to_string(),
        actors,
        relations: relations(p, shot),
        props,
        png: shot_png_name(i, &shot.id),
    };
    d.narrative = narrative(p, shot, &d);
    d
}

/// Auto-generated narrative paragraph for a shot.
pub fn narrative(p: &Project, shot: &Shot, d: &ShotDesc) -> String {
    let mut s = String::new();
    let loc = if d.location.is_empty() { "某處".to_string() } else { d.location.clone() };
    s.push_str(&format!("【{}・{}・{}】", d.in_out, loc, d.time_of_day));
    if !d.weather.is_empty() {
        s.push_str(&format!("{}。", d.weather.trim_end_matches('。')));
    }
    if !d.environment.is_empty() {
        s.push_str(&d.environment);
        if !d.environment.ends_with('。') {
            s.push('。');
        }
    }
    s.push_str(&format!("鏡頭以{}拍攝，長 {:.1} 秒。", d.camera.replace('・', "、"), d.duration));
    let set: Vec<String> = shot.props.iter().map(|pr| pr.display_name()).collect();
    if !set.is_empty() {
        s.push_str(&format!("畫面中可以看到{}。", set.join("、")));
    }
    if d.actors.is_empty() {
        s.push_str("畫面中沒有人物。");
        return s;
    }
    let names: Vec<&str> = d.actors.iter().map(|a| a.name.as_str()).collect();
    s.push_str(&format!("出場人物：{}。", names.join("、")));
    for (a, ad) in shot.actors.iter().zip(&d.actors) {
        let (cw, ch) = (p.canvas.width as f32, p.canvas.height as f32);
        let mut t = format!(
            "{}位於{}的{}，{}，",
            ad.name,
            region(a.x, a.y, cw, ch),
            depth_layer(a.y, shot.horizon * ch, ch).split('（').next().unwrap_or(""),
            ad.facing
        );
        if !ad.action.is_empty() {
            t.push_str(&ad.action);
        } else {
            t.push_str(&format!("呈「{}」姿勢", pose_label(&a.pose.preset)));
        }
        if !ad.expression.is_empty() {
            t.push_str(&format!("，神情{}", ad.expression));
        }
        t.push('。');
        if let Some(m) = &ad.movement {
            let e = a.end_point();
            t.push_str(&format!(
                "在 {:.1} 到 {:.1} 秒之間，{}{}{}，來到{}",
                m.start,
                m.end,
                ad.name,
                direction_desc(e[0] - a.x, e[1] - a.y),
                a.movement.style.zh(),
                region(e[0], e[1], cw, ch)
            ));
            if !a.movement.end_pose.is_empty() && a.movement.end_pose != a.pose.preset {
                let end = match crate::mannequin::actions::action_info(&a.movement.end_pose) {
                    Some(info) => info.desc.to_string(),
                    None => format!("擺出「{}」的姿勢", pose_label(&a.movement.end_pose)),
                };
                t.push_str(&format!("，停下來{end}"));
            }
            t.push('。');
        }
        if !ad.dialogue.is_empty() {
            t.push_str(&format!("{}{}：{}", ad.name, ad.bubble.verb_zh(), quote(&ad.dialogue)));
        }
        s.push_str(&t);
    }
    if let Some(r) = d.relations.first() {
        s.push_str(r);
    }
    s
}

/// Short one-line summary of a shot (overview captions).
pub fn shot_summary(p: &Project, i: usize) -> String {
    let shot = &p.shots[i];
    let names: Vec<String> = shot.actors.iter().map(|a| p.cast_of(a).name).collect();
    format!(
        "{}・{}・{:.1}s{}",
        shot.camera.size.zh(),
        if shot.setting.location.is_empty() { "—" } else { shot.setting.location.as_str() },
        shot.duration,
        if names.is_empty() { String::new() } else { format!("・{}", names.join("、")) }
    )
}

/// ASCII layout sketch: uppercase letters = characters, lowercase = props,
/// `*` = movement path, `-` = horizon.
pub fn ascii_sketch(p: &Project, shot: &Shot) -> (String, Vec<(char, String)>) {
    const W: usize = 64;
    const H: usize = 18;
    let (cw, ch) = (p.canvas.width as f32, p.canvas.height as f32);
    let mut g = vec![vec!['.'; W]; H];
    let hz = ((shot.horizon * H as f32) as usize).min(H - 1);
    for c in g[hz].iter_mut() {
        *c = '-';
    }
    let cell = |x: f32, y: f32| {
        (
            (x / cw * W as f32).clamp(0.0, W as f32 - 1.0) as usize,
            (y / ch * H as f32).clamp(0.0, H as f32 - 1.0) as usize,
        )
    };
    let mut legend = vec![];
    enum Thing<'a> {
        Prop(&'a Prop),
        Actor(&'a Actor),
    }
    let mut things: Vec<(f32, Thing)> = shot.props.iter().map(|p| (p.y, Thing::Prop(p))).collect();
    things.extend(shot.actors.iter().map(|a| (a.y, Thing::Actor(a))));
    things.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut np, mut na) = (0u8, 0u8);
    let fill = |g: &mut Vec<Vec<char>>, b: [f32; 4], c: char| {
        let (x0, y0) = cell(b[0], b[1]);
        let (x1, y1) = cell(b[2], b[3]);
        for row in g.iter_mut().take(y1 + 1).skip(y0) {
            for v in row.iter_mut().take(x1 + 1).skip(x0) {
                *v = c;
            }
        }
    };
    for (_, t) in &things {
        match t {
            Thing::Prop(pr) => {
                let c = (b'a' + np % 26) as char;
                np += 1;
                let y = pr.y - pr.elevation;
                fill(&mut g, [pr.x - pr.w / 2.0, y - pr.h, pr.x + pr.w / 2.0, y], c);
                legend.push((c, format!("`{}` {}（{}）", pr.id, pr.display_name(), pr.kind.zh())));
            }
            Thing::Actor(a) => {
                let c = (b'A' + na % 26) as char;
                na += 1;
                let b = FigureSpec::of_actor(p, shot, a).bounds();
                fill(&mut g, b, c);
                legend.push((c, format!("`{}` {}", a.id, p.cast_of(a).name)));
            }
        }
    }
    for a in &shot.actors {
        if a.movement.is_active() {
            let path = crate::draw::smooth_path(&a.path_points(), 10);
            for k in 1..=40 {
                let q = crate::draw::point_along(&path, k as f32 / 40.0);
                let (x, y) = cell(q[0], q[1]);
                if g[y][x] == '.' || g[y][x] == '-' {
                    g[y][x] = '*';
                }
            }
        }
    }
    legend.sort_by_key(|l| (l.0.is_lowercase(), l.0));
    let mut s = format!("+{}+\n", "-".repeat(W));
    for row in g {
        s.push('|');
        s.extend(row);
        s.push_str("|\n");
    }
    s.push_str(&format!("+{}+", "-".repeat(W)));
    (s, legend)
}

/// Markdown storyboard / narration document. `with_images` links the shot PNGs.
pub fn storyboard_md(p: &Project, with_images: bool) -> String {
    let mut s = String::new();
    let starts = p.shot_starts();
    s.push_str(&format!("# 分鏡腳本 — {}\n\n", p.title));
    s.push_str(&format!(
        "> 由 rust-scene-storyboard {} 產生。本文件以文字描述每個鏡頭的場景、攝影機、人物位置與朝向、姿勢、走位（移動路徑）與道具配置，\n> 不看草稿圖也能理解畫面；精確數值以 `project.json` 為準。\n\n",
        env!("CARGO_PKG_VERSION")
    ));
    if !p.synopsis.trim().is_empty() {
        s.push_str(&format!("## 故事概要\n\n{}\n\n", p.synopsis.trim()));
    }
    s.push_str("## 總覽\n\n");
    s.push_str(&format!(
        "- 畫布：{}×{} px（{}）。\n- 共 {} 個鏡頭，總長 {:.1} 秒。\n",
        p.canvas.width,
        p.canvas.height,
        p.canvas.preset,
        p.shots.len(),
        p.total_duration()
    ));
    s.push_str("- 座標：原點在畫面左上角，x 向右、y 向下（像素）。人物與道具的座標是它們的**站立點／底部中心**；y 越大越靠近鏡頭（前景）。\n");
    s.push_str("- 朝向：0° = 面向鏡頭、90° = 面向畫面右側、180° = 背對鏡頭、-90° = 面向畫面左側。\n");
    s.push_str("- 距離與速度以 1.7 公尺標準身高換算，僅供參考。\n\n");

    if !p.cast.is_empty() {
        s.push_str("## 角色表\n\n| 角色 | 代號 | 身高 | 體型 | 設定 | 出場鏡頭 |\n|---|---|---|---|---|---|\n");
        for c in &p.cast {
            let shots: Vec<String> = p
                .shots
                .iter()
                .enumerate()
                .filter(|(_, sh)| sh.actors.iter().any(|a| a.cast == c.id))
                .map(|(i, _)| format!("{:02}", i + 1))
                .collect();
            s.push_str(&format!(
                "| **{}** | `{}` | {:.2} m | {} | {} | {} |\n",
                c.name,
                c.id,
                c.height,
                c.body_type.zh(),
                c.description.replace('|', "／"),
                if shots.is_empty() { "—".into() } else { shots.join("、") }
            ));
        }
        s.push('\n');
    }

    s.push_str("## 鏡頭列表\n\n");
    for (i, sh) in p.shots.iter().enumerate() {
        s.push_str(&format!(
            "{}. 鏡頭 {:02}「{}」：{:.1}–{:.1} 秒（{:.1} 秒）・{}\n",
            i + 1,
            i + 1,
            sh.name,
            starts[i],
            starts[i] + sh.duration,
            sh.duration,
            shot_summary(p, i)
        ));
    }
    s.push('\n');

    for i in 0..p.shots.len() {
        let d = describe_shot(p, i);
        let shot = &p.shots[i];
        s.push_str(&format!("## 鏡頭 {:02}「{}」（`{}`）\n\n", i + 1, d.name, d.id));
        if with_images {
            s.push_str(&format!("![鏡頭 {:02} 草稿]({})\n\n", i + 1, d.png));
        }
        s.push_str(&format!("- 時間：影片 {:.1}–{:.1} 秒，長 {:.1} 秒。\n", d.start, d.start + d.duration, d.duration));
        s.push_str("\n### 場景\n\n");
        s.push_str(&format!(
            "- **地點**：{}（{}，{}）\n",
            if d.location.is_empty() { "未設定" } else { &d.location },
            d.in_out,
            d.time_of_day
        ));
        if !d.weather.is_empty() {
            s.push_str(&format!("- **天氣／光線**：{}\n", d.weather));
        }
        if !d.environment.is_empty() {
            s.push_str(&format!("- **環境**：{}\n", d.environment));
        }
        s.push_str(&format!("- **攝影機**：{}\n", d.camera));
        if !d.camera_notes.is_empty() {
            s.push_str(&format!("- **鏡頭備註**：{}\n", d.camera_notes));
        }
        s.push_str(&format!("\n### 畫面敘述\n\n> {}\n\n", d.narrative));
        if !d.narration.is_empty() {
            let boxed = if d.narration_box { "（草稿圖以旁白框顯示）" } else { "" };
            s.push_str(&format!("### 旁白{boxed}\n\n> {}\n\n", d.narration.replace('\n', "\n> ")));
        }
        if !d.actors.is_empty() {
            s.push_str("### 人物與走位\n\n");
            for a in &d.actors {
                s.push_str(&format!("#### {}（`{}`）\n\n", a.name, a.id));
                s.push_str(&format!("- **位置**：{}\n", a.position));
                s.push_str(&format!("- **朝向**：{}\n", a.facing));
                s.push_str(&format!("- **姿勢**：{}\n", a.pose));
                s.push_str(&format!("- **大小**：{}\n", a.size));
                if !a.action.is_empty() {
                    s.push_str(&format!("- **動作**：{}\n", a.action));
                }
                if !a.expression.is_empty() {
                    s.push_str(&format!("- **表情**：{}\n", a.expression));
                }
                if !a.dialogue.is_empty() {
                    s.push_str(&format!("- **對白**（{}）：{}\n", a.bubble_desc, quote(&a.dialogue)));
                }
                if let Some(m) = &a.movement {
                    s.push_str(&format!("- **移動 {}**：{}\n", circled(m.number), m.text));
                }
                s.push('\n');
            }
        }
        if !d.relations.is_empty() {
            s.push_str("### 人物相對位置\n\n");
            for r in &d.relations {
                s.push_str(&format!("- {r}\n"));
            }
            s.push('\n');
        }
        if !d.props.is_empty() {
            s.push_str(
                "### 道具與佈景\n\n| 道具 | 類型 | 位置 | 大小 | 與人物的關係 | 備註 |\n|---|---|---|---|---|---|\n",
            );
            for pr in &d.props {
                s.push_str(&format!(
                    "| **{}** (`{}`) | {} | {} | {} | {} | {} |\n",
                    pr.name,
                    pr.id,
                    pr.kind,
                    pr.position,
                    pr.size,
                    if pr.relation.is_empty() { "—" } else { &pr.relation },
                    if pr.notes.is_empty() { "—" } else { &pr.notes }
                ));
            }
            s.push('\n');
        }
        let (sketch, legend) = ascii_sketch(p, shot);
        s.push_str(
            "### 版面速寫\n\n大寫字母 = 人物，小寫字母 = 道具，`*` = 移動路徑，`-` = 地平線／牆腳線。\n\n```text\n",
        );
        s.push_str(&sketch);
        s.push_str("\n```\n\n");
        for (c, l) in legend {
            s.push_str(&format!("- `{c}` = {l}\n"));
        }
        s.push('\n');
        if !d.notes.is_empty() {
            s.push_str(&format!("### 導演備註\n\n{}\n\n", d.notes));
        }
    }

    s.push_str("## 事件時間軸\n\n");
    for (i, sh) in p.shots.iter().enumerate() {
        let t0 = starts[i];
        s.push_str(&format!("- **{:.1} s** 鏡頭 {:02}「{}」開始（{}）\n", t0, i + 1, sh.name, sh.camera.size.zh()));
        let mut ev: Vec<(f32, String)> = vec![];
        for a in &sh.actors {
            let name = p.cast_of(a).name;
            if a.movement.is_active() {
                ev.push((
                    t0 + a.movement.start,
                    format!("{name}開始{}（至 {:.1} s）", a.movement.style.zh(), t0 + a.movement.end),
                ));
            }
            if !a.dialogue.trim().is_empty() {
                let style = if a.bubble.style == crate::model::BubbleStyle::Speech {
                    String::new()
                } else {
                    format!("（{}）", a.bubble.style.zh())
                };
                ev.push((t0, format!("{name}{style}：{}", quote(&a.dialogue))));
            }
        }
        ev.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (t, e) in ev {
            s.push_str(&format!("  - {t:.1} s　{e}\n"));
        }
    }
    s.push_str(&format!("- **{:.1} s** 結束\n", p.total_duration()));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_and_facing() {
        assert_eq!(region(960.0, 540.0, 1920.0, 1080.0), "畫面正中央");
        assert_eq!(region(100.0, 1000.0, 1920.0, 1080.0), "畫面左下方");
        assert_eq!(facing_desc(0.0), "正面朝向鏡頭");
        assert_eq!(facing_desc(90.0), "面向畫面右側（側面）");
        assert_eq!(facing_desc(-90.0), "面向畫面左側（側面）");
        assert_eq!(facing_desc(180.0), "背對鏡頭");
        assert!(direction_desc(200.0, -100.0).contains("右方"));
    }

    #[test]
    fn md_mentions_everything() {
        let p = crate::sample::sample_project();
        let md = storyboard_md(&p, true);
        for c in &p.cast {
            assert!(md.contains(&c.name));
        }
        for (i, s) in p.shots.iter().enumerate() {
            assert!(md.contains(&s.name));
            assert!(md.contains(&shot_png_name(i, &s.id)));
            for pr in &s.props {
                assert!(md.contains(&pr.id), "{}", pr.id);
            }
        }
        assert!(md.contains("移動 ①"));
        assert!(md.contains("兩人"));
        assert!(md.contains("```text"));
    }
}
