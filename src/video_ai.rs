//! 「影片 AI 用」export: clean first / last frames plus one structured prompt per
//! shot (English + Traditional Chinese) for image-to-video models.
//!
//! The default preset targets **Seedance 2.0** (ByteDance) first-and-last-frame
//! generation:
//! - first frame = `shot_XX_start.png` (`@Image1`), last frame = `shot_XX_end.png`
//!   (`@Image2`); a shot without movement only gets a first frame;
//! - prompt order Subject → Action → Environment → Camera (one move) → Style →
//!   constraints, written as affirmative sentences, dialogue in double quotes
//!   (spoken when `generate_audio` is on), ~60–120 English words;
//! - `duration` is an integer of 4–15 s, `ratio` one of adaptive / 16:9 / 9:16 /
//!   4:3 / 3:4 / 1:1 / 21:9 (adaptive keeps the first frame's shape).
//!
//! The generic preset uses plain file names instead of `@Image` tags, keeps the
//! shot length and adds an explicit negative prompt.

use std::path::Path;

use serde::Serialize;
use tiny_skia::{IntRect, Pixmap};

use crate::describe::{direction_desc, facing_desc, region};
use crate::draw::clean_items;
use crate::export::{ExportReport, write};
use crate::figure::FigureSpec;
use crate::mannequin::actions::{action_info, pose_label};
use crate::model::{
    Actor, BubbleStyle, CameraMove, InOut, MoveStyle, Project, Shot, ShotSize, VideoAspect, VideoPreset,
};
use crate::raster;

/// Seedance 2.0 accepts whole seconds from 4 to 15.
pub const SEEDANCE_MIN_S: u32 = 4;
pub const SEEDANCE_MAX_S: u32 = 15;
/// Soft English word budget per prompt.
pub const MAX_WORDS: usize = 120;

pub const NEGATIVE_EN: &str = "text, captions, subtitles, labels, name tags, arrows, numbers, speech bubbles, \
grid lines, stick figures, mannequins, line-art drawing, sketch, watermark, extra people, distorted limbs";
pub const NEGATIVE_ZH: &str =
    "文字、字幕、標籤、名牌、箭頭、數字、對白泡泡、格線、火柴人、人偶、線稿、素描、浮水印、多餘的人、肢體變形";

/// One shot's video-AI job.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShotPrompt {
    pub shot: usize,
    pub id: String,
    pub name: String,
    /// First frame file (always present).
    pub first_frame: String,
    /// Last frame file (`None` when nobody moves in the shot).
    pub last_frame: Option<String>,
    /// Depth maps of the first / last frame (near = white), when enabled.
    pub first_depth: Option<String>,
    pub last_depth: Option<String>,
    /// Requested clip length in seconds (Seedance: clamped integer 4–15).
    pub duration: f32,
    /// The shot length from the storyboard.
    pub storyboard_duration: f32,
    pub aspect_ratio: String,
    pub generate_audio: bool,
    pub prompt_en: String,
    pub prompt_zh: String,
    /// Negative prompt (generic preset; Seedance prefers the affirmative constraint
    /// sentence already in the prompt, but the list is provided for tools that take one).
    pub negative_en: String,
    pub negative_zh: String,
    pub word_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct PromptFile {
    pub format: &'static str,
    pub project: String,
    pub preset: String,
    pub aspect_ratio: String,
    pub style: String,
    pub frame_size: [u32; 2],
    pub shots: Vec<ShotPrompt>,
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{2E80}'..='\u{9FFF}').contains(&c) || ('\u{F900}'..='\u{FFEF}').contains(&c))
}

/// Text usable inside the English prompt (free text written in English).
fn en_text(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty() && !has_cjk(t)).then(|| t.trim_end_matches('.').to_string())
}

pub fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn norm(a: f32) -> f32 {
    let a = a.rem_euclid(360.0);
    if a > 180.0 { a - 360.0 } else { a }
}

/// Screen position in English: "left foreground", "center background"…
pub fn region_en(p: &Project, shot: &Shot, x: f32, y: f32) -> String {
    let (cw, ch) = (p.canvas.width as f32, p.canvas.height as f32);
    let col = ["left", "center", "right"][((x / cw) * 3.0).floor().clamp(0.0, 2.0) as usize];
    let hz = shot.horizon * ch;
    let t = (y - hz) / (ch - hz).max(1.0);
    let depth = if t < 0.3 {
        "background"
    } else if t < 0.65 {
        "midground"
    } else {
        "foreground"
    };
    format!("{col} {depth}")
}

pub fn facing_en(f: f32) -> &'static str {
    let a = norm(f);
    let m = a.abs();
    let right = a > 0.0;
    match m {
        m if m <= 22.5 => "facing the camera",
        m if m <= 67.5 => {
            if right {
                "3/4 view facing right"
            } else {
                "3/4 view facing left"
            }
        }
        m if m <= 112.5 => {
            if right {
                "profile facing right"
            } else {
                "profile facing left"
            }
        }
        m if m <= 157.5 => {
            if right {
                "back 3/4 view, right"
            } else {
                "back 3/4 view, left"
            }
        }
        _ => "back to camera",
    }
}

/// Direction of a screen displacement in English.
pub fn direction_en(dx: f32, dy: f32) -> String {
    let h = if dx > 20.0 {
        Some("toward screen right")
    } else if dx < -20.0 {
        Some("toward screen left")
    } else {
        None
    };
    let d = if dy > 20.0 {
        Some("toward the camera")
    } else if dy < -20.0 {
        Some("away from the camera")
    } else {
        None
    };
    match (h, d) {
        (Some(h), Some(d)) => format!("{h} and {d}"),
        (Some(h), None) => h.to_string(),
        (None, Some(d)) => d.to_string(),
        (None, None) => "in place".into(),
    }
}

fn move_verb(m: MoveStyle) -> (&'static str, &'static str) {
    match m {
        MoveStyle::Walk => ("walks", "走"),
        MoveStyle::Run => ("runs", "跑"),
        MoveStyle::Sneak => ("sneaks", "躡手躡腳地走"),
        MoveStyle::Stroll => ("strolls", "慢慢踱步"),
        MoveStyle::Jump => ("jumps", "跳"),
        MoveStyle::Crawl => ("crawls", "爬"),
        MoveStyle::Stagger => ("staggers", "踉蹌地走"),
        MoveStyle::Slide => ("slides", "滑行"),
    }
}

fn pose_en(key: &str) -> String {
    action_info(key).map(|i| i.en.to_lowercase()).unwrap_or_else(|| "posing".into())
}

fn pose_zh(key: &str) -> String {
    action_info(key).map(|i| i.zh.to_string()).unwrap_or_else(|| pose_label(key))
}

fn size_en(s: ShotSize) -> &'static str {
    match s {
        ShotSize::ExtremeWide => "extreme wide shot",
        ShotSize::Wide => "wide shot",
        ShotSize::Full => "full shot",
        ShotSize::MediumWide => "medium wide shot",
        ShotSize::Medium => "medium shot",
        ShotSize::MediumClose => "medium close-up",
        ShotSize::CloseUp => "close-up",
        ShotSize::ExtremeClose => "extreme close-up",
        ShotSize::OverShoulder => "over-the-shoulder shot",
        ShotSize::Pov => "POV shot",
        ShotSize::TwoShot => "two-shot",
    }
}

fn camera_move_en(m: CameraMove) -> &'static str {
    match m {
        CameraMove::Static => "fixed camera",
        CameraMove::PanLeft => "slow smooth pan left",
        CameraMove::PanRight => "slow smooth pan right",
        CameraMove::TiltUp => "slow tilt up",
        CameraMove::TiltDown => "slow tilt down",
        CameraMove::DollyIn => "slow push-in",
        CameraMove::DollyOut => "slow pull-out",
        CameraMove::TruckLeft => "smooth lateral tracking to the left",
        CameraMove::TruckRight => "smooth lateral tracking to the right",
        CameraMove::Follow => "tracking shot following the subject",
        CameraMove::Handheld => "handheld, slight natural shake",
        CameraMove::ZoomIn => "gradual zoom in",
        CameraMove::ZoomOut => "gradual zoom out",
    }
}

/// Clip length for the preset.
pub fn clip_duration(preset: VideoPreset, d: f32) -> f32 {
    match preset {
        VideoPreset::Seedance2 => (d.round() as u32).clamp(SEEDANCE_MIN_S, SEEDANCE_MAX_S) as f32,
        VideoPreset::Generic => (d * 10.0).round() / 10.0,
    }
}

/// Ratio string written to the prompt files.
pub fn aspect_label(p: &Project) -> String {
    let a = p.video_ai.aspect;
    match (a, p.video_ai.preset) {
        (VideoAspect::Adaptive, VideoPreset::Seedance2) => "adaptive".into(),
        (VideoAspect::Adaptive, VideoPreset::Generic) => {
            VideoAspect::nearest(p.canvas.width as f32, p.canvas.height as f32).en().into()
        }
        (a, _) => a.en().into(),
    }
}

/// Centre crop `[x, y, w, h]` of the canvas for the chosen aspect ratio.
pub fn crop_rect(p: &Project) -> [u32; 4] {
    let (w, h) = (p.canvas.width, p.canvas.height);
    let Some(r) = p.video_ai.aspect.ratio() else { return [0, 0, w, h] };
    let cr = w as f32 / h as f32;
    if (cr - r).abs() < 0.01 {
        [0, 0, w, h]
    } else if cr > r {
        let nw = ((h as f32 * r).round() as u32).clamp(1, w);
        [(w - nw) / 2, 0, nw, h]
    } else {
        let nh = ((w as f32 / r).round() as u32).clamp(1, h);
        [0, (h - nh) / 2, w, nh]
    }
}

fn shot_moves(shot: &Shot) -> bool {
    shot.actors.iter().any(|a| a.movement.is_active())
}

/// Render the clean first (`end = false`) or last frame of shot `index`.
pub fn render_clean(p: &Project, index: usize, end: bool) -> Result<Pixmap, String> {
    let shot = p.shots.get(index).ok_or_else(|| format!("沒有第 {} 個鏡頭", index + 1))?;
    let items = clean_items(p, shot, end, p.video_ai.keep_floor);
    let pm = raster::render_items(&items, p.canvas.width, p.canvas.height)?;
    let [x, y, w, h] = crop_rect(p);
    if [x, y, w, h] == [0, 0, p.canvas.width, p.canvas.height] {
        return Ok(pm);
    }
    let r = IntRect::from_xywh(x as i32, y as i32, w, h).ok_or("裁切範圍錯誤")?;
    pm.clone_rect(r).ok_or_else(|| "裁切失敗".into())
}

pub fn depth_name(i: usize, end: bool) -> String {
    format!("shot_{:02}_{}_depth.png", i + 1, if end { "end" } else { "start" })
}

pub fn frame_name(i: usize, end: bool) -> String {
    format!("shot_{:02}_{}.png", i + 1, if end { "end" } else { "start" })
}

struct Who {
    name: String,
    look_en: Option<String>,
    look_zh: Option<String>,
}

fn who(p: &Project, a: &Actor) -> Who {
    let c = p.cast_of(a);
    let zh = if c.appearance_zh.trim().is_empty() { c.description.trim() } else { c.appearance_zh.trim() };
    Who {
        name: c.name.clone(),
        look_en: en_text(&c.appearance),
        look_zh: (!zh.is_empty()).then(|| zh.trim_end_matches('。').to_string()),
    }
}

fn dialogue_en(name: &str, a: &Actor, audio: bool) -> Option<String> {
    let t = a.dialogue.trim();
    if t.is_empty() {
        return None;
    }
    let t = t.replace('"', "'");
    Some(match a.bubble.style {
        BubbleStyle::Thought => format!("{name} thinks silently (voice-over): \"{t}\""),
        BubbleStyle::Narration => format!("Voice-over narration: \"{t}\""),
        BubbleStyle::Shout => format!("{name} shouts: \"{t}\""),
        BubbleStyle::Whisper => format!("{name} whispers: \"{t}\""),
        BubbleStyle::Speech if audio => format!("{name} says: \"{t}\""),
        BubbleStyle::Speech => format!("{name} talks, lips moving: \"{t}\""),
    })
}

fn dialogue_zh(name: &str, a: &Actor) -> Option<String> {
    let t = a.dialogue.trim();
    if t.is_empty() {
        return None;
    }
    let t = t.replace('"', "'");
    Some(match a.bubble.style {
        BubbleStyle::Thought => format!("{name}的內心獨白（畫外音）：\"{t}\""),
        BubbleStyle::Narration => format!("旁白（畫外音）：\"{t}\""),
        BubbleStyle::Shout => format!("{name}大喊：\"{t}\""),
        BubbleStyle::Whisper => format!("{name}小聲地說：\"{t}\""),
        BubbleStyle::Speech => format!("{name}說：\"{t}\""),
    })
}

/// Build the English and Chinese prompt of shot `i`.
pub fn shot_prompt(p: &Project, i: usize) -> ShotPrompt {
    let shot = &p.shots[i];
    let v = &p.video_ai;
    let seed = v.preset == VideoPreset::Seedance2;
    let moves = shot_moves(shot);
    let (cw, ch) = (p.canvas.width as f32, p.canvas.height as f32);
    let duration = clip_duration(v.preset, shot.duration);
    let dur_s = if seed { format!("{duration:.0}") } else { format!("{duration:.1}") };

    // --- frames
    let (frames_en, frames_zh) = match (seed, moves) {
        (true, true) => (
            "@Image1 as the first frame, @Image2 as the last frame; generate the motion between them.".to_string(),
            "@图片1 作為首幀，@图片2 作為尾幀，生成兩者之間連續的動作。".to_string(),
        ),
        (true, false) => ("@Image1 as the first frame.".into(), "@图片1 作為首幀。".into()),
        (false, true) => (
            format!("Start frame {}, end frame {}.", frame_name(i, false), frame_name(i, true)),
            format!("首幀 {}，尾幀 {}。", frame_name(i, false), frame_name(i, true)),
        ),
        (false, false) => {
            (format!("Start frame {}.", frame_name(i, false)), format!("首幀 {}。", frame_name(i, false)))
        }
    };

    // --- subjects (appearance) and actions
    let mut subj_en = vec![];
    let mut subj_zh = vec![];
    let mut act_en = vec![];
    let mut act_zh = vec![];
    let mut say_en = vec![];
    let mut vo_en = None;
    let mut say_zh = vec![];
    for a in &shot.actors {
        let w = who(p, a);
        if let Some(l) = &w.look_en {
            subj_en.push(format!("{} ({l})", w.name));
        } else {
            subj_en.push(w.name.clone());
        }
        subj_zh.push(match &w.look_zh {
            Some(l) => format!("{}（{l}）", w.name),
            None => w.name.clone(),
        });
        let pos_en = region_en(p, shot, a.x, a.y);
        let pos_zh = region(a.x, a.y, cw, ch);
        let action_en = en_text(&a.action).unwrap_or_else(|| pose_en(&a.pose.preset));
        let action_zh = if a.action.trim().is_empty() { pose_zh(&a.pose.preset) } else { a.action.trim().to_string() };
        let mut e = format!("{}: {pos_en}, {}, {action_en}", w.name, facing_en(a.facing));
        let mut z = format!("{}在{pos_zh}，{}，{action_zh}", w.name, facing_desc(a.facing));
        if a.movement.is_active() {
            let m = &a.movement;
            let end = a.end_point();
            let (ve, vz) = move_verb(m.style);
            e.push_str(&format!("; {:.0}-{:.0}s {} {}", m.start, m.end, ve, direction_en(end[0] - a.x, end[1] - a.y)));
            e.push_str(&format!(" to the {}", region_en(p, shot, end[0], end[1])));
            z.push_str(&format!(
                "；{:.0}–{:.0} 秒以「{vz}」的方式{}，到達{}",
                m.start,
                m.end,
                direction_desc(end[0] - a.x, end[1] - a.y),
                region(end[0], end[1], cw, ch)
            ));
            if let Some(f) = FigureSpec::end_of_actor(p, shot, a) {
                if !m.end_pose.is_empty() && m.end_pose != a.pose.preset {
                    e.push_str(&format!(", then {}", pose_en(&m.end_pose)));
                    z.push_str(&format!("，改為{}", pose_zh(&m.end_pose)));
                }
                if (norm(f.facing) - norm(a.facing)).abs() > 20.0 {
                    e.push_str(&format!(", {}", facing_en(f.facing)));
                    z.push_str(&format!("，{}", facing_desc(f.facing)));
                }
            }
            if let Some(d) = en_text(&m.description) {
                e.push_str(&format!(" ({d})"));
            } else if !m.description.trim().is_empty() {
                z.push_str(&format!("（{}）", m.description.trim().trim_end_matches('。')));
            }
        }
        if let Some(x) = en_text(&a.expression) {
            e.push_str(&format!(", {x} expression"));
        }
        if !a.expression.trim().is_empty() {
            z.push_str(&format!("，表情{}", a.expression.trim()));
        }
        act_en.push(format!("{e}."));
        act_zh.push(format!("{z}。"));
        if let Some(d) = dialogue_en(&w.name, a, v.audio) {
            say_en.push(format!("{d}."));
        }
        if let Some(d) = dialogue_zh(&w.name, a) {
            say_zh.push(format!("{d}。"));
        }
    }
    if !shot.narration.trim().is_empty() && shot.narration_box {
        let n = shot.narration.trim().replace('"', "'");
        vo_en = Some(format!("Voice-over: \"{n}\"."));
        say_zh.push(format!("旁白（畫外音）：\"{n}\"。"));
    }

    // --- setting / camera / style
    let s = &shot.setting;
    let io = match s.in_out {
        InOut::Interior => ("Interior", "內景"),
        InOut::Exterior => ("Exterior", "外景"),
    };
    let mut set_en = format!("{}, {}", io.0, s.time_of_day.en().to_lowercase());
    if !s.location.trim().is_empty() {
        // Seedance reads Chinese too, so a Chinese place name is kept verbatim.
        set_en.push_str(&format!(", {}", s.location.trim()));
    }
    if let Some(w) = en_text(&s.weather) {
        set_en.push_str(&format!(", {w}"));
    }
    let mut set_zh = format!("{}，{}，{}", io.1, s.location.trim(), s.time_of_day.zh());
    if !s.weather.trim().is_empty() {
        set_zh.push_str(&format!("，{}", s.weather.trim()));
    }
    if !s.environment.trim().is_empty() {
        set_zh.push_str(&format!("。{}", s.environment.trim().trim_end_matches('。')));
    }
    let env_en = en_text(&s.environment);
    let cam = &shot.camera;
    let cam_en =
        format!("Camera: {}, {}, {}.", size_en(cam.size), cam.angle.en().to_lowercase(), camera_move_en(cam.movement));
    let cam_zh = format!("鏡頭：{}、{}、{}。", cam.size.zh(), cam.angle.zh(), cam.movement.zh());
    let style = if v.style.trim().is_empty() { "cinematic realistic" } else { v.style.trim() };
    let style_en = format!("Style: {style}. The line-art mannequins are blocking reference only; render real people.");
    let style_zh = format!("風格：{style}。畫面中的線稿人偶只是走位參考，請生成真實的人物（自然的皮膚、頭髮與服裝）。");
    let keep_en = "Consistent faces, natural anatomy, no on-screen text.";
    let keep_zh = "人物前後一致，臉孔與比例穩定，自然的人體結構，畫面乾淨、不出現任何文字。";

    // --- assemble (drop optional parts when over the word budget)
    let people = format!("People: {}.", subj_en.join("; "));
    let mut parts_en: Vec<(u8, String)> = vec![(0, frames_en.clone()), (0, people)];
    parts_en.extend(act_en.iter().map(|x| (0, x.clone())));
    parts_en.extend(say_en.iter().map(|x| (0, x.clone())));
    if let Some(vo) = vo_en {
        parts_en.push((3, vo));
    }
    parts_en.push((0, format!("Setting: {set_en}.")));
    if let Some(e) = env_en {
        parts_en.push((2, format!("{e}.")));
    }
    if let Some(n) = en_text(&shot.video_note) {
        parts_en.push((1, format!("{n}.")));
    }
    parts_en.push((0, cam_en));
    parts_en.push((0, format!("Duration {dur_s} seconds.")));
    parts_en.push((0, style_en));
    if seed {
        parts_en.push((0, keep_en.into()));
    }
    for level in [3u8, 2, 1] {
        if word_count(&join(&parts_en)) > MAX_WORDS {
            parts_en.retain(|(l, _)| *l < level);
        }
    }
    let prompt_en = join(&parts_en);

    let mut zh = vec![frames_zh, format!("人物：{}。", subj_zh.join("；"))];
    zh.extend(act_zh);
    zh.extend(say_zh);
    zh.push(format!("場景：{set_zh}。"));
    if !shot.video_note_zh.trim().is_empty() {
        zh.push(format!("{}。", shot.video_note_zh.trim().trim_end_matches('。')));
    }
    zh.push(cam_zh);
    zh.push(format!("時長 {dur_s} 秒。"));
    zh.push(style_zh);
    if seed {
        zh.push(keep_zh.into());
    }
    let prompt_zh = zh.join("");
    let (negative_en, negative_zh) = (NEGATIVE_EN.to_string(), NEGATIVE_ZH.to_string());
    ShotPrompt {
        shot: i + 1,
        id: shot.id.clone(),
        name: shot.name.clone(),
        first_frame: frame_name(i, false),
        last_frame: moves.then(|| frame_name(i, true)),
        first_depth: v.depth.then(|| depth_name(i, false)),
        last_depth: (v.depth && moves).then(|| depth_name(i, true)),
        duration,
        storyboard_duration: shot.duration,
        aspect_ratio: aspect_label(p),
        generate_audio: v.audio,
        word_count: word_count(&prompt_en),
        prompt_en,
        prompt_zh,
        negative_en,
        negative_zh,
    }
}

fn join(parts: &[(u8, String)]) -> String {
    parts.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(" ")
}

pub fn prompts(p: &Project) -> PromptFile {
    let [_, _, w, h] = crop_rect(p);
    PromptFile {
        format: "rust-scene-storyboard/video-ai-prompts",
        project: p.title.clone(),
        preset: p.video_ai.preset.en().into(),
        aspect_ratio: aspect_label(p),
        style: p.video_ai.style.clone(),
        frame_size: [w, h],
        shots: (0..p.shots.len()).map(|i| shot_prompt(p, i)).collect(),
    }
}

pub fn prompts_txt(f: &PromptFile) -> String {
    let seed = f.preset == VideoPreset::Seedance2.en();
    let mut o = format!(
        "{} — video-AI prompts ({}; ratio {}; frames {}×{})\n",
        f.project, f.preset, f.aspect_ratio, f.frame_size[0], f.frame_size[1]
    );
    if seed {
        o.push_str("Seedance 2.0: upload the start PNG as the first frame (@Image1) and the end PNG as the last frame (@Image2), set duration and ratio as listed.\n");
    }
    for s in &f.shots {
        o.push_str(&format!(
            "\n=== Shot {:02} · {} · {} s · {} ===\nFirst frame: {}\nLast frame:  {}\n\n[EN]\n{}\n\n[中文]\n{}\n",
            s.shot,
            s.name,
            if seed { format!("{:.0}", s.duration) } else { format!("{:.1}", s.duration) },
            s.aspect_ratio,
            s.first_frame,
            s.last_frame.as_deref().unwrap_or("(none — no movement in this shot)"),
            s.prompt_en,
            s.prompt_zh
        ));
        if !seed {
            o.push_str(&format!("\n[Negative]\n{}\n", s.negative_en));
        }
    }
    o
}

pub fn prompts_md(f: &PromptFile) -> String {
    let seed = f.preset == VideoPreset::Seedance2.en();
    let mut o = format!("# {} — 影片 AI 提示詞\n\n", f.project);
    o.push_str(&format!(
        "- 目標模型 Preset：**{}**\n- 畫面比例 ratio：`{}`（首尾幀 {}×{} px）\n- 風格 Style：{}\n",
        f.preset, f.aspect_ratio, f.frame_size[0], f.frame_size[1], f.style
    ));
    if seed {
        o.push_str("- 使用方式：在 Seedance 2.0 選「首尾幀」，上傳 `shot_XX_start.png` 為首幀（@Image1 / @图片1）、`shot_XX_end.png` 為尾幀（@Image2 / @图片2），時長與比例依下表設定，貼上英文或中文提示詞。沒有尾幀的鏡頭只上傳首幀。\n");
    } else {
        o.push_str("- 使用方式：以 `shot_XX_start.png` 為起始畫面（支援的話以 `shot_XX_end.png` 為結束畫面），貼上提示詞與 Negative prompt。\n");
    }
    if f.shots.iter().any(|s| s.first_depth.is_some()) {
        o.push_str(DEPTH_HELP);
    }
    for s in &f.shots {
        o.push_str(&format!("\n## 鏡頭 {:02}：{}\n\n", s.shot, s.name));
        o.push_str(&format!(
            "| 首幀 | 尾幀 |\n|---|---|\n| ![]({}) | {} |\n\n",
            s.first_frame,
            s.last_frame.as_ref().map(|l| format!("![]({l})")).unwrap_or_else(|| "（無移動，不需要尾幀）".into())
        ));
        if let Some(d) = &s.first_depth {
            o.push_str(&format!(
                "| 首幀深度圖 | 尾幀深度圖 |\n|---|---|\n| ![]({d}) | {} |\n\n",
                s.last_depth.as_ref().map(|l| format!("![]({l})")).unwrap_or_else(|| "—".into())
            ));
        }
        o.push_str(&format!(
            "- 時長 duration：**{} 秒**（分鏡 {:.1} 秒）\n- 比例 ratio：`{}`\n- 音訊 generate_audio：{}\n\n",
            if seed { format!("{:.0}", s.duration) } else { format!("{:.1}", s.duration) },
            s.storyboard_duration,
            s.aspect_ratio,
            s.generate_audio
        ));
        o.push_str(&format!(
            "**English**（{} words）\n\n```text\n{}\n```\n\n**中文**\n\n```text\n{}\n```\n",
            s.word_count, s.prompt_en, s.prompt_zh
        ));
        if !seed {
            o.push_str(&format!("\n**Negative**\n\n```text\n{}\n```\n", s.negative_en));
        }
    }
    o
}

const DEPTH_HELP: &str = "\n### 深度圖 Depth maps（`shot_XX_start_depth.png` / `shot_XX_end_depth.png`）\n\n\
- 灰階、**近 = 白、遠 = 黑**（MiDaS / ControlNet depth 慣例的相對深度），尺寸與首尾幀相同、逐像素對齊；由場景幾何（地面、道具、人偶實心體積）算出，不是從圖片估計。\n\
- **ControlNet depth**（ComfyUI、Stable Diffusion WebUI / Forge、SDXL / Flux depth ControlNet、AnimateDiff / Wan 等影片管線）：把深度圖接到 depth ControlNet，preprocessor 選 **none**（已經是深度圖，不要再跑 MiDaS / Depth Anything），control weight 約 0.4–0.7；首幀深度圖控制首幀構圖，尾幀深度圖用於結束畫面或關鍵幀。\n\
- **Seedance 2.0**：首尾幀模式只接受首幀＋尾幀，**不能同時加參考圖**；要用深度圖時改用「全能參考 / 多模態參考」（最多 9 張圖），例如：`@Image1 為首幀構圖參考，@Image2 為深度與空間配置參考，保持人物位置與前後景深，生成寫實畫面`。Seedance 沒有官方的 depth 控制，深度圖只會被當作一般參考圖理解，效果不保證；較穩定的作法是先用 ControlNet depth 把首尾幀生成寫實圖，再把寫實首尾幀交給 Seedance 2.0 首尾幀模式。\n";

/// Write clean frames and prompts into `dir`.
pub fn export_video_ai(p: &Project, dir: &Path) -> Result<ExportReport, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("無法建立資料夾 {}：{e}", dir.display()))?;
    let mut files = vec![];
    let jobs: Vec<(usize, bool)> = (0..p.shots.len())
        .flat_map(|i| {
            let mv = shot_moves(&p.shots[i]);
            [(i, false)].into_iter().chain(mv.then_some((i, true)))
        })
        .collect();
    let depth = p.video_ai.depth;
    let crop = crop_rect(p);
    #[allow(clippy::type_complexity)]
    let pngs: Vec<Result<(Vec<u8>, Option<Vec<u8>>), String>> = std::thread::scope(|sc| {
        let hs: Vec<_> = jobs
            .iter()
            .map(|&(i, end)| {
                sc.spawn(move || {
                    let png = raster::encode_png(&render_clean(p, i, end)?)?;
                    let d = if depth {
                        Some(crate::depth::render_depth(p, &p.shots[i], end).crop(crop).to_png(p.video_ai.depth16)?)
                    } else {
                        None
                    };
                    Ok((png, d))
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("算圖執行緒失敗".into()))).collect()
    });
    for (&(i, end), png) in jobs.iter().zip(pngs) {
        let (png, d) = png?;
        write(dir, &frame_name(i, end), &png, &mut files)?;
        if let Some(d) = d {
            write(dir, &depth_name(i, end), &d, &mut files)?;
        }
    }
    let f = prompts(p);
    write(dir, "prompts.txt", prompts_txt(&f).as_bytes(), &mut files)?;
    let json = serde_json::to_string_pretty(&f).map_err(|e| e.to_string())?;
    write(dir, "prompts.json", json.as_bytes(), &mut files)?;
    write(dir, "prompts.md", prompts_md(&f).as_bytes(), &mut files)?;
    Ok(ExportReport { dir: dir.to_path_buf(), files })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{Item, P};
    use crate::sample::sample_project;

    #[test]
    fn clean_frames_have_no_overlays() {
        let p = sample_project();
        for shot in &p.shots {
            for end in [false, true] {
                for it in clean_items(&p, shot, end, false) {
                    match it {
                        Item::Prim(P::Text { .. }) => panic!("text in clean frame"),
                        Item::Figure(f) => assert!(!f.ghost, "ghost in clean frame"),
                        _ => {}
                    }
                }
            }
        }
    }

    #[test]
    fn end_frame_differs_and_crop() {
        let mut p = sample_project();
        let a = render_clean(&p, 0, false).unwrap();
        let b = render_clean(&p, 0, true).unwrap();
        assert_ne!(a.data(), b.data());
        p.video_ai.aspect = VideoAspect::R9x16;
        let c = render_clean(&p, 0, false).unwrap();
        assert!((c.width() as f32 / c.height() as f32 - 9.0 / 16.0).abs() < 0.01);
    }

    #[test]
    fn seedance_duration_and_prompt() {
        assert_eq!(clip_duration(VideoPreset::Seedance2, 1.2), 4.0);
        assert_eq!(clip_duration(VideoPreset::Seedance2, 7.6), 8.0);
        assert_eq!(clip_duration(VideoPreset::Seedance2, 40.0), 15.0);
        assert_eq!(clip_duration(VideoPreset::Generic, 2.5), 2.5);
        let p = sample_project();
        let f = prompts(&p);
        assert_eq!(f.shots.len(), p.shots.len());
        for s in &f.shots {
            assert!(s.word_count < 130, "{} words: {}", s.word_count, s.prompt_en);
            assert!(s.prompt_en.starts_with("@Image1 as the first frame"));
            assert!(s.prompt_zh.starts_with("@图片1"));
            assert!(s.prompt_en.contains("render real people"));
            assert!((4.0..=15.0).contains(&s.duration));
        }
        assert!(f.shots[0].prompt_en.contains("@Image2") && f.shots[0].last_frame.is_some());
    }

    #[test]
    fn static_shot_has_no_last_frame() {
        let mut p = sample_project();
        for a in &mut p.shots[0].actors {
            a.movement.enabled = false;
        }
        let s = shot_prompt(&p, 0);
        assert!(s.last_frame.is_none() && !s.prompt_en.contains("@Image2"));
        p.video_ai.preset = VideoPreset::Generic;
        let s = shot_prompt(&p, 0);
        assert!(s.prompt_en.starts_with("Start frame shot_01_start.png"));
    }

    #[test]
    fn old_projects_load_with_defaults() {
        let p = sample_project();
        let mut v: serde_json::Value = serde_json::from_str(&p.to_json()).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("video_ai");
        for c in o["cast"].as_array_mut().unwrap() {
            c.as_object_mut().unwrap().remove("appearance");
        }
        let back: Project = serde_json::from_value(v).unwrap();
        assert_eq!(back.video_ai.preset, VideoPreset::Seedance2);
        assert!(back.cast[0].appearance.is_empty());
    }
}
