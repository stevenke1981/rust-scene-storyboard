//! Project data model: canvas, cast, shots (scene / camera settings), actors
//! (posed line-art characters with movement) and props.
//!
//! Coordinates are canvas pixels with the origin at the top-left corner, x to the
//! right and y down (same convention as whitebox-video-storyboard). Actors and
//! props are anchored at their **ground point** (bottom centre), so a larger y
//! means closer to the camera.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::mannequin::skeleton::{BodyType, JOINT_COUNT, Joint, Pose, Proportions};

pub const FORMAT: &str = "rust-scene-storyboard/project";
pub const VERSION: u32 = 1;

/// An sRGB colour, serialised as `"#RRGGBB"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    pub fn hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2])
    }
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim().trim_start_matches('#');
        let v = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            6 | 8 => Some(Rgb([v(0)?, v(2)?, v(4)?])),
            _ => None,
        }
    }
    pub fn rgba(&self, a: u8) -> [u8; 4] {
        [self.0[0], self.0[1], self.0[2], a]
    }
}

impl Serialize for Rgb {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid colour {s:?}, expected #RRGGBB")))
    }
}

/// Character tag colours offered for new cast members.
pub const CAST_COLORS: [Rgb; 8] = [
    Rgb([230, 90, 40]),
    Rgb([40, 120, 220]),
    Rgb([40, 160, 90]),
    Rgb([170, 70, 200]),
    Rgb([210, 160, 20]),
    Rgb([220, 60, 120]),
    Rgb([20, 160, 170]),
    Rgb([120, 100, 80]),
];

/// Enum with a stable snake_case key and Traditional Chinese / English labels.
macro_rules! labeled_enum {
    ($(#[$m:meta])* $name:ident default $def:ident { $($var:ident => ($key:literal, $zh:literal, $en:literal)),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $key)] $var),+
        }
        impl Default for $name {
            fn default() -> Self {
                $name::$def
            }
        }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$var),+];
            pub fn key(self) -> &'static str {
                match self { $($name::$var => $key),+ }
            }
            pub fn zh(self) -> &'static str {
                match self { $($name::$var => $zh),+ }
            }
            pub fn en(self) -> &'static str {
                match self { $($name::$var => $en),+ }
            }
            /// "中文 English" label for the UI.
            pub fn label(self) -> String {
                format!("{} {}", self.zh(), self.en())
            }
        }
    };
}

labeled_enum! {
    /// Interior / exterior (內景 / 外景).
    InOut default Interior {
        Interior => ("interior", "內景", "INT."),
        Exterior => ("exterior", "外景", "EXT."),
    }
}

labeled_enum! {
    TimeOfDay default Day {
        Dawn => ("dawn", "清晨", "Dawn"),
        Morning => ("morning", "上午", "Morning"),
        Day => ("day", "白天", "Day"),
        Noon => ("noon", "中午", "Noon"),
        Afternoon => ("afternoon", "下午", "Afternoon"),
        Dusk => ("dusk", "黃昏", "Dusk"),
        Night => ("night", "夜晚", "Night"),
        LateNight => ("late_night", "深夜", "Late night"),
    }
}

labeled_enum! {
    /// Shot size (景別).
    ShotSize default Full {
        ExtremeWide => ("extreme_wide", "大遠景", "EWS"),
        Wide => ("wide", "遠景", "WS"),
        Full => ("full", "全景", "FS"),
        MediumWide => ("medium_wide", "中遠景", "MWS"),
        Medium => ("medium", "中景", "MS"),
        MediumClose => ("medium_close", "中近景", "MCU"),
        CloseUp => ("close_up", "特寫", "CU"),
        ExtremeClose => ("extreme_close", "大特寫", "ECU"),
        OverShoulder => ("over_shoulder", "過肩鏡頭", "OTS"),
        Pov => ("pov", "主觀鏡頭", "POV"),
        TwoShot => ("two_shot", "雙人鏡頭", "Two-shot"),
    }
}

labeled_enum! {
    /// Camera angle (機位角度); sets the viewing pitch of the line-art figures.
    CameraAngle default EyeLevel {
        EyeLevel => ("eye_level", "平視", "Eye level"),
        High => ("high", "俯視", "High angle"),
        Low => ("low", "仰視", "Low angle"),
        BirdsEye => ("birds_eye", "鳥瞰", "Bird's-eye"),
        WormsEye => ("worms_eye", "蟲視（極低角度）", "Worm's-eye"),
        Dutch => ("dutch", "斜角（荷蘭角）", "Dutch angle"),
    }
}

impl CameraAngle {
    /// Viewing pitch in degrees used to render the figures (positive = from above).
    pub fn pitch(self) -> f32 {
        match self {
            CameraAngle::EyeLevel | CameraAngle::Dutch => 4.0,
            CameraAngle::High => 24.0,
            CameraAngle::Low => -10.0,
            CameraAngle::BirdsEye => 55.0,
            CameraAngle::WormsEye => -22.0,
        }
    }
}

labeled_enum! {
    /// Camera movement (運鏡).
    CameraMove default Static {
        Static => ("static", "固定鏡頭", "Static"),
        PanLeft => ("pan_left", "向左搖攝", "Pan left"),
        PanRight => ("pan_right", "向右搖攝", "Pan right"),
        TiltUp => ("tilt_up", "向上搖攝", "Tilt up"),
        TiltDown => ("tilt_down", "向下搖攝", "Tilt down"),
        DollyIn => ("dolly_in", "推鏡（推近）", "Dolly in"),
        DollyOut => ("dolly_out", "拉鏡（拉遠）", "Dolly out"),
        TruckLeft => ("truck_left", "向左橫移", "Truck left"),
        TruckRight => ("truck_right", "向右橫移", "Truck right"),
        Follow => ("follow", "跟拍", "Follow"),
        Handheld => ("handheld", "手持晃動", "Handheld"),
        ZoomIn => ("zoom_in", "變焦推近", "Zoom in"),
        ZoomOut => ("zoom_out", "變焦拉遠", "Zoom out"),
    }
}

labeled_enum! {
    /// How a character travels along its movement path.
    MoveStyle default Walk {
        Walk => ("walk", "走", "Walk"),
        Run => ("run", "跑", "Run"),
        Sneak => ("sneak", "躡手躡腳地走", "Sneak"),
        Stroll => ("stroll", "慢慢踱步", "Stroll"),
        Jump => ("jump", "跳", "Jump"),
        Crawl => ("crawl", "爬", "Crawl"),
        Stagger => ("stagger", "踉蹌", "Stagger"),
        Slide => ("slide", "滑行", "Slide"),
    }
}

labeled_enum! {
    /// Comic / manga balloon style of a line of dialogue (對白框樣式).
    BubbleStyle default Speech {
        Speech => ("speech", "對話", "Speech"),
        Thought => ("thought", "內心獨白", "Thought"),
        Shout => ("shout", "吶喊", "Shout"),
        Whisper => ("whisper", "悄悄話", "Whisper"),
        Narration => ("narration", "旁白框", "Caption"),
    }
}

impl BubbleStyle {
    /// Shape description used in Markdown / HTML.
    pub fn shape_zh(self) -> &'static str {
        match self {
            BubbleStyle::Speech => "對話泡泡（圓角橢圓＋尖尾）",
            BubbleStyle::Thought => "思考泡泡（雲朵＋小圓圈）",
            BubbleStyle::Shout => "吶喊泡泡（爆炸鋸齒框）",
            BubbleStyle::Whisper => "悄悄話泡泡（虛線框）",
            BubbleStyle::Narration => "旁白框（矩形說明框）",
        }
    }
    /// Verb used in narrative text: 「小明大喊：…」.
    pub fn verb_zh(self) -> &'static str {
        match self {
            BubbleStyle::Speech => "說",
            BubbleStyle::Thought => "心想",
            BubbleStyle::Shout => "大喊",
            BubbleStyle::Whisper => "小聲地說",
            BubbleStyle::Narration => "的旁白",
        }
    }
}

/// How a character's dialogue balloon is drawn. Missing in v0.1 projects → defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BubbleSettings {
    pub style: BubbleStyle,
    /// Offset of the balloon from its automatic position (canvas px).
    pub offset: [f32; 2],
    /// Wrap length in canvas px (line width, or column height for vertical text); 0 = automatic.
    pub wrap: f32,
    /// Vertical CJK text (直書), columns right to left.
    pub vertical: bool,
    /// Text size multiplier.
    pub text_scale: f32,
}

impl Default for BubbleSettings {
    fn default() -> Self {
        BubbleSettings { style: BubbleStyle::Speech, offset: [0.0, 0.0], wrap: 0.0, vertical: false, text_scale: 1.0 }
    }
}

labeled_enum! {
    /// Prop / set piece types, drawn as simple line-art primitives.
    PropKind default Box {
        Box => ("box", "箱子", "Box"),
        Chair => ("chair", "椅子", "Chair"),
        Table => ("table", "桌子", "Table"),
        Sofa => ("sofa", "沙發", "Sofa"),
        Bed => ("bed", "床", "Bed"),
        Door => ("door", "門", "Door"),
        Window => ("window", "窗戶", "Window"),
        Shelf => ("shelf", "書櫃", "Shelf"),
        Counter => ("counter", "櫃台", "Counter"),
        Screen => ("screen", "電視／螢幕", "TV / screen"),
        Lamp => ("lamp", "燈", "Lamp"),
        Plant => ("plant", "盆栽", "Potted plant"),
        Tree => ("tree", "樹", "Tree"),
        Bench => ("bench", "長椅", "Bench"),
        Car => ("car", "汽車", "Car"),
        Sign => ("sign", "招牌／告示牌", "Sign"),
        Stairs => ("stairs", "樓梯", "Stairs"),
        Building => ("building", "建築物", "Building"),
        Wall => ("wall", "牆／隔板", "Wall"),
        Cup => ("cup", "杯子", "Cup"),
        Bag => ("bag", "包包", "Bag"),
        Ball => ("ball", "球", "Ball"),
        Custom => ("custom", "自訂物件", "Custom"),
    }
}

impl PropKind {
    /// Default size (w, h) in metres-ish units relative to a 1.7 m figure.
    pub fn default_size_m(self) -> (f32, f32) {
        use PropKind::*;
        match self {
            Box => (0.6, 0.5),
            Chair => (0.5, 0.95),
            Table => (1.2, 0.75),
            Sofa => (2.0, 0.85),
            Bed => (2.0, 0.9),
            Door => (0.95, 2.1),
            Window => (1.2, 1.2),
            Shelf => (1.0, 1.9),
            Counter => (2.2, 1.05),
            Screen => (1.1, 1.2),
            Lamp => (0.5, 2.6),
            Plant => (0.5, 0.9),
            Tree => (2.2, 4.2),
            Bench => (1.6, 0.85),
            Car => (4.0, 1.5),
            Sign => (1.0, 2.0),
            Stairs => (1.6, 1.4),
            Building => (5.0, 6.0),
            Wall => (3.0, 2.6),
            Cup => (0.12, 0.14),
            Bag => (0.4, 0.4),
            Ball => (0.3, 0.3),
            Custom => (0.8, 0.8),
        }
    }
    /// Background set pieces are listed as part of the environment in narration.
    pub fn is_set_piece(self) -> bool {
        matches!(self, PropKind::Door | PropKind::Window | PropKind::Building | PropKind::Wall | PropKind::Stairs)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Aspect preset key ("16:9", "9:16", "1:1", "4:3", "2.39:1") or "custom".
    pub preset: String,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas { width: 1920, height: 1080, preset: "16:9".into() }
    }
}

pub const ASPECT_PRESETS: [(&str, &str, u32, u32); 5] = [
    ("16:9", "16:9 橫式 1920×1080", 1920, 1080),
    ("9:16", "9:16 直式 1080×1920", 1080, 1920),
    ("1:1", "1:1 方形 1080×1080", 1080, 1080),
    ("4:3", "4:3 1440×1080", 1440, 1080),
    ("2.39:1", "2.39:1 電影寬銀幕 1920×804", 1920, 804),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CastMember {
    pub id: String,
    pub name: String,
    /// Character description (role, age, look, personality…).
    pub description: String,
    pub color: Rgb,
    /// Height in metres (the mannequin is scaled accordingly).
    pub height: f32,
    pub body_type: BodyType,
    pub head_size: f32,
}

impl Default for CastMember {
    fn default() -> Self {
        CastMember {
            id: "char_1".into(),
            name: "角色".into(),
            description: String::new(),
            color: CAST_COLORS[0],
            height: 1.70,
            body_type: BodyType::Average,
            head_size: 1.0,
        }
    }
}

impl CastMember {
    pub fn proportions(&self) -> Proportions {
        Proportions {
            height: self.height,
            head_size: self.head_size,
            body_type: self.body_type,
            ..Proportions::default()
        }
        .sanitized()
    }
}

/// Joint rotations of a posed character (same joint keys and Euler convention as
/// rust-pose-studio pose files).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PoseData {
    /// Library pose key the pose started from (e.g. "walk", "floor:7").
    pub preset: String,
    /// The joints were adjusted by hand after loading the preset.
    pub edited: bool,
    /// Height above the floor in metres (airborne poses such as jumps).
    pub lift: f32,
    /// Euler angles in degrees (`Rz * Ry * Rx`) by joint key.
    pub joints: BTreeMap<String, [f32; 3]>,
}

impl Default for PoseData {
    fn default() -> Self {
        PoseData { preset: "stand".into(), edited: false, lift: 0.0, joints: BTreeMap::new() }
    }
}

impl PoseData {
    pub fn from_pose(preset: &str, pose: &Pose, lift: f32) -> PoseData {
        let round = |v: f32| (v * 100.0).round() / 100.0;
        PoseData {
            preset: preset.to_string(),
            edited: false,
            lift,
            joints: Joint::ALL.iter().map(|j| (j.key().to_string(), pose.get(*j).map(round))).collect(),
        }
    }

    /// Joint rotations as a pose (root at the origin; callers snap it to the floor).
    pub fn rotations(&self) -> [[f32; 3]; JOINT_COUNT] {
        let mut rot = [[0.0; 3]; JOINT_COUNT];
        for (k, v) in &self.joints {
            if let Some(j) = Joint::from_key(k)
                && v.iter().all(|x| x.is_finite())
            {
                rot[j.index()] = *v;
            }
        }
        rot
    }

    pub fn set_joint(&mut self, j: Joint, e: [f32; 3]) {
        self.joints.insert(j.key().to_string(), e);
    }
}

/// Planned movement of a character during the shot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Movement {
    pub enabled: bool,
    /// Waypoints after the actor's start position (canvas px); the last one is the end point.
    pub path: Vec<[f32; 2]>,
    pub style: MoveStyle,
    /// Start / end time within the shot (seconds).
    pub start: f32,
    pub end: f32,
    /// Pose at the end point ("" = keep the current pose).
    pub end_pose: String,
    /// Facing at the end point (None = keep facing).
    pub end_facing: Option<f32>,
    /// Draw a light "ghost" figure at the end point.
    pub ghost: bool,
    /// Scale the end figure by depth (closer to the camera = larger).
    pub depth_scale: bool,
    /// Free text describing the movement.
    pub description: String,
}

impl Default for Movement {
    fn default() -> Self {
        Movement {
            enabled: false,
            path: vec![],
            style: MoveStyle::Walk,
            start: 0.0,
            end: 2.0,
            end_pose: String::new(),
            end_facing: None,
            ghost: true,
            depth_scale: true,
            description: String::new(),
        }
    }
}

impl Movement {
    pub fn is_active(&self) -> bool {
        self.enabled && !self.path.is_empty()
    }
}

/// A character placed in a shot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Actor {
    pub id: String,
    /// Cast member id.
    pub cast: String,
    /// Ground point (between the feet / under the pelvis) in canvas px.
    pub x: f32,
    pub y: f32,
    /// Facing in degrees: 0 = towards the camera, 90 = screen right, 180 = away, -90 = screen left.
    pub facing: f32,
    /// Size multiplier (1.0 = a 1.7 m figure is 55 % of the frame height).
    pub scale: f32,
    pub pose: PoseData,
    /// What the character does (action description).
    pub action: String,
    /// Facial expression / emotion.
    pub expression: String,
    /// Line of dialogue (shown as a comic balloon).
    pub dialogue: String,
    /// Balloon style / position of the dialogue.
    pub bubble: BubbleSettings,
    pub movement: Movement,
}

impl Default for Actor {
    fn default() -> Self {
        Actor {
            id: "actor_1".into(),
            cast: "char_1".into(),
            x: 960.0,
            y: 900.0,
            facing: 0.0,
            scale: 1.0,
            pose: PoseData::default(),
            action: String::new(),
            expression: String::new(),
            dialogue: String::new(),
            bubble: BubbleSettings::default(),
            movement: Movement::default(),
        }
    }
}

impl Actor {
    /// Start point followed by the movement path.
    pub fn path_points(&self) -> Vec<[f32; 2]> {
        let mut v = vec![[self.x, self.y]];
        if self.movement.enabled {
            v.extend(self.movement.path.iter().copied());
        }
        v
    }
    pub fn end_point(&self) -> [f32; 2] {
        if self.movement.is_active() { *self.movement.path.last().unwrap() } else { [self.x, self.y] }
    }
}

/// A prop / set piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prop {
    pub id: String,
    pub kind: PropKind,
    pub label: String,
    /// Ground point (bottom centre) in canvas px.
    pub x: f32,
    pub y: f32,
    /// Drawn size in canvas px.
    pub w: f32,
    pub h: f32,
    /// Mirror horizontally (3/4 view seen from the other side).
    pub flip: bool,
    /// Raised above the ground point by this many px (e.g. a cup on a table).
    pub elevation: f32,
    pub notes: String,
}

impl Default for Prop {
    fn default() -> Self {
        Prop {
            id: "prop_1".into(),
            kind: PropKind::Box,
            label: String::new(),
            x: 960.0,
            y: 900.0,
            w: 200.0,
            h: 160.0,
            flip: false,
            elevation: 0.0,
            notes: String::new(),
        }
    }
}

impl Prop {
    pub fn display_name(&self) -> String {
        if self.label.trim().is_empty() { self.kind.zh().to_string() } else { self.label.clone() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Setting {
    /// Location (地點), e.g. "街角咖啡店門口".
    pub location: String,
    pub in_out: InOut,
    pub time_of_day: TimeOfDay,
    /// Weather / light (天氣、光線).
    pub weather: String,
    /// Environment description (環境描述): mood, background, sounds…
    pub environment: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ShotCamera {
    pub size: ShotSize,
    pub angle: CameraAngle,
    pub movement: CameraMove,
    /// Lens / framing notes.
    pub notes: String,
}

/// One shot (鏡頭) of the storyboard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shot {
    pub id: String,
    pub name: String,
    /// Length in seconds.
    pub duration: f32,
    pub setting: Setting,
    pub camera: ShotCamera,
    /// Horizon / floor line as a fraction of the canvas height (0 = top).
    pub horizon: f32,
    /// Draw perspective floor lines in the draft.
    pub floor_grid: bool,
    pub actors: Vec<Actor>,
    pub props: Vec<Prop>,
    /// Narration / voice-over text (旁白).
    pub narration: String,
    /// Also show the narration as a caption box in the draft image.
    pub narration_box: bool,
    /// Director's notes (導演備註).
    pub notes: String,
}

impl Default for Shot {
    fn default() -> Self {
        Shot {
            id: "shot_1".into(),
            name: "鏡頭 1".into(),
            duration: 4.0,
            setting: Setting::default(),
            camera: ShotCamera::default(),
            horizon: 0.42,
            floor_grid: true,
            actors: vec![],
            props: vec![],
            narration: String::new(),
            narration_box: false,
            notes: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub format: String,
    pub version: u32,
    pub title: String,
    /// Logline / synopsis.
    pub synopsis: String,
    pub canvas: Canvas,
    pub cast: Vec<CastMember>,
    pub shots: Vec<Shot>,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            format: FORMAT.into(),
            version: VERSION,
            title: "未命名分鏡".into(),
            synopsis: String::new(),
            canvas: Canvas::default(),
            cast: vec![],
            shots: vec![Shot::default()],
        }
    }
}

/// Next free id of the form `<prefix>_<n>`.
pub fn next_id<'a>(prefix: &str, existing: impl Iterator<Item = &'a str>) -> String {
    let used: Vec<&str> = existing.collect();
    (1..).map(|n| format!("{prefix}_{n}")).find(|id| !used.contains(&id.as_str())).unwrap_or_default()
}

impl Project {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Result<Project, String> {
        let p: Project = serde_json::from_str(s).map_err(|e| format!("專案 JSON 格式錯誤：{e}"))?;
        if p.format != FORMAT {
            return Err(format!("不是 rust-scene-storyboard 專案檔（format = \"{}\"）", p.format));
        }
        Ok(p.sanitized())
    }

    pub fn load(path: &std::path::Path) -> Result<Project, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("無法讀取 {}：{e}", path.display()))?;
        Project::from_json(&s)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        std::fs::write(path, self.to_json()).map_err(|e| format!("無法寫入 {}：{e}", path.display()))
    }

    /// Clamp values into sane ranges and make sure there is at least one shot.
    pub fn sanitized(mut self) -> Project {
        self.canvas.width = self.canvas.width.clamp(64, 8192);
        self.canvas.height = self.canvas.height.clamp(64, 8192);
        if self.shots.is_empty() {
            self.shots.push(Shot::default());
        }
        for s in &mut self.shots {
            s.duration = if s.duration.is_finite() { s.duration.clamp(0.1, 3600.0) } else { 4.0 };
            s.horizon = if s.horizon.is_finite() { s.horizon.clamp(0.0, 1.0) } else { 0.42 };
            for a in &mut s.actors {
                if !a.scale.is_finite() || a.scale <= 0.0 {
                    a.scale = 1.0;
                }
                a.scale = a.scale.clamp(0.05, 8.0);
                if !a.facing.is_finite() {
                    a.facing = 0.0;
                }
                let b = &mut a.bubble;
                b.text_scale = if b.text_scale.is_finite() { b.text_scale.clamp(0.3, 4.0) } else { 1.0 };
                b.wrap = if b.wrap.is_finite() { b.wrap.clamp(0.0, 8000.0) } else { 0.0 };
                if !b.offset.iter().all(|v| v.is_finite()) {
                    b.offset = [0.0, 0.0];
                }
            }
        }
        self
    }

    pub fn cast_member(&self, id: &str) -> Option<&CastMember> {
        self.cast.iter().find(|c| c.id == id)
    }

    /// Cast member of an actor, or a default stand-in.
    pub fn cast_of(&self, a: &Actor) -> CastMember {
        self.cast_member(&a.cast).cloned().unwrap_or_else(|| CastMember {
            id: a.cast.clone(),
            name: a.cast.clone(),
            ..CastMember::default()
        })
    }

    pub fn total_duration(&self) -> f32 {
        self.shots.iter().map(|s| s.duration).sum()
    }

    /// Start time of every shot.
    pub fn shot_starts(&self) -> Vec<f32> {
        let mut t = 0.0;
        self.shots
            .iter()
            .map(|s| {
                let st = t;
                t += s.duration;
                st
            })
            .collect()
    }

    pub fn next_shot_id(&self) -> String {
        next_id("shot", self.shots.iter().map(|s| s.id.as_str()))
    }
    pub fn next_cast_id(&self) -> String {
        next_id("char", self.cast.iter().map(|s| s.id.as_str()))
    }

    /// Pixels per metre for a figure of scale 1.0.
    pub fn base_ppm(&self) -> f32 {
        self.canvas.height as f32 * 0.55 / 1.7
    }

    /// Change the canvas size, scaling every position and size.
    pub fn set_canvas(&mut self, w: u32, h: u32, preset: &str) {
        let (ow, oh) = (self.canvas.width as f32, self.canvas.height as f32);
        let (sx, sy) = (w as f32 / ow, h as f32 / oh);
        let s = sy; // figure / prop sizes follow the height
        for shot in &mut self.shots {
            for a in &mut shot.actors {
                a.x *= sx;
                a.y *= sy;
                for p in &mut a.movement.path {
                    p[0] *= sx;
                    p[1] *= sy;
                }
            }
            for p in &mut shot.props {
                p.x *= sx;
                p.y *= sy;
                p.w *= s;
                p.h *= s;
            }
        }
        self.canvas = Canvas { width: w, height: h, preset: preset.into() };
    }
}

/// Camera height (metres) assumed for depth-based scaling: at eye level the
/// horizon passes through the eyes of standing people, so the size of a figure
/// standing at canvas y is proportional to its distance below the horizon.
pub const CAMERA_HEIGHT_M: f32 = 1.5;

impl Project {
    /// Perspective-consistent scale for something standing at canvas `y` in `shot`.
    pub fn depth_scale(&self, shot: &Shot, y: f32) -> f32 {
        let hz = shot.horizon * self.canvas.height as f32;
        ((y - hz) / (CAMERA_HEIGHT_M * self.base_ppm())).clamp(0.05, 4.0)
    }
}

impl Shot {
    pub fn next_actor_id(&self) -> String {
        next_id("actor", self.actors.iter().map(|s| s.id.as_str()))
    }
    pub fn next_prop_id(&self, kind: PropKind) -> String {
        next_id(kind.key(), self.props.iter().map(|s| s.id.as_str()))
    }
    pub fn actor(&self, id: &str) -> Option<&Actor> {
        self.actors.iter().find(|a| a.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_settings_round_trip() {
        let p = crate::sample::sample_project();
        let s = p.to_json();
        for style in ["thought", "shout", "whisper", "narration", "speech"] {
            assert!(s.contains(&format!("\"style\": \"{style}\"")), "{style}");
        }
        assert!(s.contains("\"vertical\": true"));
        assert!(s.contains("\"narration_box\": true"));
        let q = Project::from_json(&s).unwrap();
        let b = &q.shots[2].actors.iter().find(|a| a.bubble.style == BubbleStyle::Narration).unwrap().bubble;
        assert_eq!(b.offset, [-160.0, -230.0]);
        assert_eq!(p, q);
    }

    #[test]
    fn v01_projects_without_bubbles_still_load() {
        // Strip every v0.2 field from the sample → what a v0.1 file looks like.
        let mut v: serde_json::Value = serde_json::from_str(&crate::sample::sample_project().to_json()).unwrap();
        for shot in v["shots"].as_array_mut().unwrap() {
            shot.as_object_mut().unwrap().remove("narration_box");
            for a in shot["actors"].as_array_mut().unwrap() {
                a.as_object_mut().unwrap().remove("bubble");
            }
        }
        let old = serde_json::to_string(&v).unwrap();
        assert!(!old.contains("bubble") && !old.contains("narration_box"));
        let p = Project::from_json(&old).unwrap();
        for shot in &p.shots {
            assert!(!shot.narration_box);
            for a in &shot.actors {
                assert_eq!(a.bubble, BubbleSettings::default());
            }
        }
        // Partial / out-of-range bubble objects are filled in and clamped.
        let partial =
            old.replacen("\"dialogue\":", "\"bubble\":{\"style\":\"shout\",\"text_scale\":99.0},\"dialogue\":", 1);
        let p = Project::from_json(&partial).unwrap();
        let b = &p.shots[0].actors[0].bubble;
        assert_eq!(b.style, BubbleStyle::Shout);
        assert!(b.text_scale <= 4.0 && !b.vertical && b.offset == [0.0, 0.0]);
    }

    #[test]
    fn json_round_trip() {
        let p = crate::sample::sample_project();
        let s = p.to_json();
        let q = Project::from_json(&s).unwrap();
        assert_eq!(p, q);
        assert!(s.contains("\"format\": \"rust-scene-storyboard/project\""));
        assert!(s.contains("\"in_out\": \"exterior\""));
        assert!(s.contains("\"color\": \"#"));
    }

    #[test]
    fn rejects_foreign_json_and_fills_defaults() {
        assert!(Project::from_json("{\"format\":\"other\"}").is_err());
        let p = Project::from_json("{\"format\":\"rust-scene-storyboard/project\",\"shots\":[]}").unwrap();
        assert_eq!(p.shots.len(), 1);
        assert_eq!(p.canvas.width, 1920);
    }

    #[test]
    fn ids_and_canvas_rescale() {
        assert_eq!(next_id("shot", ["shot_1", "shot_3"].into_iter()), "shot_2");
        let mut p = crate::sample::sample_project();
        let a0 = p.shots[0].actors[0].clone();
        p.set_canvas(960, 540, "16:9");
        let a1 = &p.shots[0].actors[0];
        assert!((a1.x - a0.x / 2.0).abs() < 1e-3 && (a1.y - a0.y / 2.0).abs() < 1e-3);
    }

    #[test]
    fn enum_labels() {
        assert_eq!(ShotSize::CloseUp.zh(), "特寫");
        assert_eq!(serde_json::to_string(&CameraMove::DollyIn).unwrap(), "\"dolly_in\"");
        assert!(PropKind::ALL.len() >= 20);
    }
}
