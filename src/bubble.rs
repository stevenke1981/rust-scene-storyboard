//! Comic / manga dialogue balloons (對白框): speech, thought, shout, whisper and
//! narration captions, with a tail that points at the speaker, CJK-aware wrapping
//! and optional vertical text (直書). Produces [`P`] primitives shared by the GUI
//! canvas and the PNG export.

use crate::draw::{INK, P, PAPER, Rgba, Stroke2};
use crate::figure::FigureSpec;
use crate::model::{Actor, BubbleSettings, BubbleStyle, Project, Shot};
use crate::raster::Fonts;
use std::f32::consts::{PI, TAU};

/// Superellipse exponent: a rounded oval that hugs a text rectangle better than an ellipse.
const SUPER_N: f32 = 2.5;
const CAPTION_FILL: Rgba = [255, 248, 214, 255];

/// A laid-out balloon in canvas px.
#[derive(Clone, Debug, PartialEq)]
pub struct BubbleLayout {
    pub center: [f32; 2],
    /// Bounds of the balloon body `[x0, y0, x1, y1]` (for hit testing / overlap checks).
    pub rect: [f32; 4],
    /// Tail tip, if the balloon has a tail.
    pub tip: Option<[f32; 2]>,
    pub prims: Vec<P>,
}

/// Text placed relative to the balloon centre.
struct TextBlock {
    w: f32,
    h: f32,
    /// (dx, dy, text, anchor)
    items: Vec<(f32, f32, String, [f32; 2])>,
}

/// Vertical presentation form of CJK punctuation (if the bundled font has it).
pub fn vertical_form(c: char) -> char {
    let v = match c {
        '，' | ',' => '︐',
        '、' => '︑',
        '。' => '︒',
        '：' | ':' => '︓',
        '；' | ';' => '︔',
        '！' | '!' => '︕',
        '？' | '?' => '︖',
        '「' => '﹁',
        '」' => '﹂',
        '『' => '﹃',
        '』' => '﹄',
        '（' | '(' => '︵',
        '）' | ')' => '︶',
        '【' => '︻',
        '】' => '︼',
        '《' => '︽',
        '》' => '︾',
        '〈' => '︿',
        '〉' => '﹀',
        '…' => '︙',
        '—' | 'ー' | '－' | '-' => '︱',
        '～' | '~' => '≀',
        _ => return c,
    };
    if Fonts::get().has(v) { v } else { c }
}

fn text_block(text: &str, size: f32, wrap: f32, vertical: bool, left: bool) -> TextBlock {
    let fonts = Fonts::get();
    if vertical {
        let cell = size * 1.12;
        let colw = size * 1.4;
        let per_col = ((wrap / cell).floor() as usize).max(1);
        let mut cols: Vec<Vec<char>> = vec![];
        for para in text.split('\n') {
            let chars: Vec<char> = para.chars().filter(|c| *c != '\r').collect();
            if chars.is_empty() {
                cols.push(vec![]);
                continue;
            }
            // Fill columns top to bottom; closing punctuation never starts a column
            // (it hangs at the bottom of the previous one, like 禁則 in print).
            let mut col: Vec<char> = vec![];
            for c in chars {
                let hang = crate::raster::NO_LINE_START.contains(c);
                if col.len() >= per_col && !(hang && col.len() == per_col) {
                    cols.push(std::mem::take(&mut col));
                }
                col.push(c);
            }
            cols.push(col);
        }
        let rows = cols.iter().map(|c| c.len()).max().unwrap_or(1).max(1);
        let (w, h) = (colw * cols.len() as f32, cell * rows as f32);
        let mut items = vec![];
        for (i, col) in cols.iter().enumerate() {
            for (j, c) in col.iter().enumerate() {
                if c.is_whitespace() {
                    continue;
                }
                let x = w / 2.0 - colw * (i as f32 + 0.5);
                let y = -h / 2.0 + cell * (j as f32 + 0.5);
                items.push((x, y, vertical_form(*c).to_string(), [0.5, 0.5]));
            }
        }
        TextBlock { w, h, items }
    } else {
        let lines = fonts.wrap(text, size, wrap);
        let lh = size * 1.32;
        let w = lines.iter().map(|l| fonts.measure(l, size)).fold(0.0, f32::max);
        let h = lh * lines.len() as f32;
        let items = lines
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                let y = -h / 2.0 + lh * (i as f32 + 0.5);
                if left { (-w / 2.0, y, l, [0.0, 0.5]) } else { (0.0, y, l, [0.5, 0.5]) }
            })
            .collect();
        TextBlock { w, h, items }
    }
}

fn sp(c: [f32; 2], rx: f32, ry: f32, t: f32) -> [f32; 2] {
    let (s, co) = t.sin_cos();
    let e = 2.0 / SUPER_N;
    [c[0] + rx * co.signum() * co.abs().powf(e), c[1] + ry * s.signum() * s.abs().powf(e)]
}

/// Superellipse parameter whose point lies in the (normalised) direction `phi`.
fn param_for(phi: f32) -> f32 {
    let (s, c) = phi.sin_cos();
    let e = SUPER_N / 2.0;
    (s.signum() * s.abs().powf(e)).atan2(c.signum() * c.abs().powf(e))
}

fn inside(c: [f32; 2], rx: f32, ry: f32, p: [f32; 2]) -> bool {
    ((p[0] - c[0]) / rx).abs().powf(SUPER_N) + ((p[1] - c[1]) / ry).abs().powf(SUPER_N) <= 1.0
}

fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// Deterministic pseudo random in [0, 1) for spike jitter.
fn jitter(seed: u64, i: usize) -> f32 {
    let mut x = seed ^ (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// Lay out a balloon for `text` spoken by someone whose head top is at `head`.
pub fn balloon(text: &str, b: &BubbleSettings, head: [f32; 2], u: f32, canvas: [f32; 2]) -> BubbleLayout {
    let ts = b.text_scale.clamp(0.3, 4.0);
    let size = 26.0 * u * ts * if b.style == BubbleStyle::Shout { 1.12 } else { 1.0 };
    let wrap = if b.wrap > 0.0 {
        b.wrap
    } else if b.vertical {
        size * 1.12 * 7.0
    } else {
        size * 14.5
    };
    let tb = text_block(text, size, wrap, b.vertical, false);
    let (a, bb) = (tb.w / 2.0, tb.h / 2.0);
    let pad = 11.0 * u * ts.sqrt();
    // body half sizes and extra extent (spikes / bumps)
    let (rx, ry, extra) = match b.style {
        BubbleStyle::Speech | BubbleStyle::Whisper => ((a + pad) * 1.32, (bb + pad * 0.8) * 1.32, 0.0),
        BubbleStyle::Thought => ((a + pad * 1.1) * 1.36, (bb + pad) * 1.36, 19.0 * u * ts.sqrt()),
        BubbleStyle::Shout => ((a + pad) * 1.40, (bb + pad) * 1.40, 30.0 * u * ts.sqrt()),
        BubbleStyle::Narration => (a + pad * 1.3, bb + pad, 0.0),
    };
    let (hw, hh) = (rx + extra, ry + extra);
    let m = 6.0 * u;
    let mut c = [head[0] + hw * 0.45 + b.offset[0], head[1] - 92.0 * u - hh + b.offset[1]];
    c[0] = if canvas[0] > 2.0 * (hw + m) { c[0].clamp(hw + m, canvas[0] - hw - m) } else { canvas[0] / 2.0 };
    c[1] = if canvas[1] > 2.0 * (hh + m) { c[1].clamp(hh + m, canvas[1] - hh - m) } else { canvas[1] / 2.0 };
    let rect = [c[0] - hw, c[1] - hh, c[0] + hw, c[1] + hh];

    // tail: from the balloon towards the speaker, stopping just above the head
    let (dx, dy) = (head[0] - c[0], head[1] - c[1]);
    let dl = (dx * dx + dy * dy).sqrt().max(1e-3);
    let gap = 48.0 * u;
    let tip = [head[0] - dx / dl * gap, head[1] - dy / dl * gap];
    let has_tail = b.style != BubbleStyle::Narration
        && dl > gap + 4.0
        && !inside(c, (rx + extra) * 1.05, (ry + extra) * 1.05, tip);
    let phi = ((tip[1] - c[1]) / ry).atan2((tip[0] - c[0]) / rx);
    let tt = param_for(phi);
    let mean_r = (rx + ry) / 2.0;

    let mut prims = vec![];
    let ink_w = 2.4 * u * ts.sqrt();
    match b.style {
        BubbleStyle::Speech | BubbleStyle::Whisper => {
            let n = 120;
            let mut pts = vec![];
            if has_tail {
                let d = (15.0 * u * ts.sqrt() / mean_r).clamp(0.08, 0.4);
                for i in 0..n {
                    let t = tt + d + (TAU - 2.0 * d) * i as f32 / (n - 1) as f32;
                    pts.push(sp(c, rx, ry, t));
                }
                pts.push(tip);
            } else {
                pts.extend((0..n).map(|i| sp(c, rx, ry, TAU * i as f32 / n as f32)));
            }
            let whisper = b.style == BubbleStyle::Whisper;
            prims.push(P::Shape {
                pts,
                center: c,
                fill: Some(PAPER),
                stroke: Some(Stroke2 { width: if whisper { ink_w * 0.85 } else { ink_w }, color: INK }),
                dash: whisper,
            });
        }
        BubbleStyle::Thought => {
            let perim = PI * (rx + ry);
            let bumps = ((perim / (68.0 * u)).round() as usize).clamp(7, 22);
            let amp = extra / mean_r;
            let n = bumps * 12;
            let pts = (0..n)
                .map(|i| {
                    let t = TAU * i as f32 / n as f32;
                    let p = sp(c, rx, ry, t);
                    let k = 1.0 + amp * (bumps as f32 * t / 2.0).sin().abs().powf(0.7);
                    [c[0] + (p[0] - c[0]) * k, c[1] + (p[1] - c[1]) * k]
                })
                .collect();
            prims.push(P::Shape {
                pts,
                center: c,
                fill: Some(PAPER),
                stroke: Some(Stroke2 { width: ink_w * 0.9, color: INK }),
                dash: false,
            });
            if has_tail {
                let e = sp(c, rx + extra, ry + extra, tt);
                let (ex, ey) = (tip[0] - e[0], tip[1] - e[1]);
                let len = (ex * ex + ey * ey).sqrt();
                let r0 = 12.0 * u * ts.sqrt();
                for (k, (t, r)) in [(0.22, r0), (0.55, r0 * 0.68), (0.85, r0 * 0.45)].iter().enumerate() {
                    if len < 26.0 * u && k == 0 {
                        continue;
                    }
                    let q = [e[0] + ex * t, e[1] + ey * t];
                    prims.push(P::Ellipse {
                        c: q,
                        r: [*r * 1.15, *r],
                        fill: Some(PAPER),
                        stroke: Some(Stroke2 { width: ink_w * 0.8, color: INK }),
                    });
                }
            }
        }
        BubbleStyle::Shout => {
            let perim = PI * (rx + ry);
            let spikes = ((perim / (44.0 * u * ts.sqrt())).round() as usize).clamp(12, 34);
            let seed = hash(text);
            let mut pts = vec![];
            let mut tail_i = None;
            let mut best = f32::MAX;
            for i in 0..spikes * 2 {
                let t = tt + PI * i as f32 / spikes as f32;
                let p = sp(c, rx, ry, t);
                if i % 2 == 0 {
                    let (vx, vy) = (p[0] - c[0], p[1] - c[1]);
                    let l = (vx * vx + vy * vy).sqrt().max(1e-3);
                    let len = extra * (0.55 + 0.45 * jitter(seed, i));
                    let tw = (jitter(seed, i + 1000) - 0.5) * 0.25;
                    let (ux, uy) = (vx / l, vy / l);
                    let q = [p[0] + (ux - uy * tw) * len, p[1] + (uy + ux * tw) * len];
                    let ang = ((t - tt + PI).rem_euclid(TAU) - PI).abs();
                    if ang < best {
                        best = ang;
                        tail_i = Some(pts.len());
                    }
                    pts.push(q);
                } else {
                    // inner corners slightly pulled in for a jagged look
                    let k = 0.97 + 0.04 * jitter(seed, i);
                    pts.push([c[0] + (p[0] - c[0]) * k, c[1] + (p[1] - c[1]) * k]);
                }
            }
            if has_tail && let Some(i) = tail_i {
                pts[i] = tip;
            }
            prims.push(P::Shape {
                pts,
                center: c,
                fill: Some(PAPER),
                stroke: Some(Stroke2 { width: ink_w * 1.25, color: INK }),
                dash: false,
            });
        }
        BubbleStyle::Narration => {
            let pts =
                vec![[c[0] - rx, c[1] - ry], [c[0] + rx, c[1] - ry], [c[0] + rx, c[1] + ry], [c[0] - rx, c[1] + ry]];
            prims.push(P::Shape {
                pts,
                center: c,
                fill: Some(CAPTION_FILL),
                stroke: Some(Stroke2 { width: ink_w * 0.9, color: INK }),
                dash: false,
            });
        }
    }
    let tc = if b.style == BubbleStyle::Whisper { [86, 88, 96, 255] } else { INK };
    for (x, y, s, anchor) in tb.items {
        prims.push(P::Text { pos: [c[0] + x, c[1] + y], text: s, size, color: tc, anchor, bg: None });
    }
    BubbleLayout { center: c, rect, tip: if has_tail { Some(tip) } else { None }, prims }
}

/// Balloon of an actor's dialogue in a shot (None without dialogue).
pub fn actor_bubble(p: &Project, shot: &Shot, a: &Actor) -> Option<BubbleLayout> {
    let text = a.dialogue.trim();
    if text.is_empty() {
        return None;
    }
    let (cw, ch) = (p.canvas.width as f32, p.canvas.height as f32);
    let head = FigureSpec::of_actor(p, shot, a).head_top();
    Some(balloon(text, &a.bubble, head, ch / 1080.0, [cw, ch]))
}

/// Narration caption box with its top-left corner at `pos` (left-aligned text).
pub fn caption(text: &str, pos: [f32; 2], max_w: f32, u: f32) -> (Vec<P>, [f32; 4]) {
    let size = 24.0 * u;
    let pad = 12.0 * u;
    let tb = text_block(text, size, (max_w - 2.0 * pad).max(size * 4.0), false, true);
    let (w, h) = (tb.w + 2.0 * pad, tb.h + 1.6 * pad);
    let c = [pos[0] + w / 2.0, pos[1] + h / 2.0];
    let pts = vec![pos, [pos[0] + w, pos[1]], [pos[0] + w, pos[1] + h], [pos[0], pos[1] + h]];
    let mut prims = vec![
        // offset shadow block, comic style
        P::Shape {
            pts: pts.iter().map(|q| [q[0] + 5.0 * u, q[1] + 5.0 * u]).collect(),
            center: [c[0] + 5.0 * u, c[1] + 5.0 * u],
            fill: Some([40, 40, 46, 255]),
            stroke: None,
            dash: false,
        },
        P::Shape {
            pts,
            center: c,
            fill: Some(CAPTION_FILL),
            stroke: Some(Stroke2 { width: 2.2 * u, color: INK }),
            dash: false,
        },
    ];
    for (x, y, s, anchor) in tb.items {
        prims.push(P::Text { pos: [c[0] + x, c[1] + y], text: s, size, color: INK, anchor, bg: None });
    }
    (prims, [pos[0], pos[1], pos[0] + w, pos[1] + h])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes(l: &BubbleLayout) -> Vec<&P> {
        l.prims.iter().filter(|p| matches!(p, P::Shape { .. })).collect()
    }

    #[test]
    fn every_style_has_body_text_and_tail_rules() {
        let head = [900.0, 600.0];
        for &style in BubbleStyle::ALL {
            let b = BubbleSettings { style, ..BubbleSettings::default() };
            let l = balloon("今天的拉花是愛心喔！", &b, head, 1.0, [1920.0, 1080.0]);
            assert_eq!(shapes(&l).len(), 1, "{style:?}");
            assert!(l.prims.iter().any(|p| matches!(p, P::Text { .. })));
            // balloon sits above the head and inside the canvas
            assert!(l.rect[3] < head[1], "{style:?} {:?}", l.rect);
            assert!(l.rect[0] >= 0.0 && l.rect[2] <= 1920.0 && l.rect[1] >= 0.0);
            assert_eq!(l.tip.is_some(), style != BubbleStyle::Narration, "{style:?}");
            if style == BubbleStyle::Speech {
                // the tail tip is part of the outline and points towards the head
                let P::Shape { pts, .. } = shapes(&l)[0] else { unreachable!() };
                let tip = l.tip.unwrap();
                assert!(pts.contains(&tip));
                assert!(tip[1] > l.center[1]);
            }
        }
        let th = balloon(
            "嗯……",
            &BubbleSettings { style: BubbleStyle::Thought, ..Default::default() },
            head,
            1.0,
            [1920.0, 1080.0],
        );
        assert!(th.prims.iter().filter(|p| matches!(p, P::Ellipse { .. })).count() >= 2, "thought trail circles");
        let wh = balloon(
            "噓",
            &BubbleSettings { style: BubbleStyle::Whisper, ..Default::default() },
            head,
            1.0,
            [1920.0, 1080.0],
        );
        assert!(matches!(shapes(&wh)[0], P::Shape { dash: true, .. }));
    }

    #[test]
    fn offset_moves_balloon_and_is_clamped() {
        let head = [900.0, 600.0];
        let a = balloon("你好", &BubbleSettings::default(), head, 1.0, [1920.0, 1080.0]);
        let b = balloon(
            "你好",
            &BubbleSettings { offset: [-200.0, 40.0], ..Default::default() },
            head,
            1.0,
            [1920.0, 1080.0],
        );
        assert!((a.center[0] - b.center[0] - 200.0).abs() < 0.01);
        assert!((b.center[1] - a.center[1] - 40.0).abs() < 0.01);
        let far = balloon(
            "你好",
            &BubbleSettings { offset: [-5000.0, -5000.0], ..Default::default() },
            head,
            1.0,
            [1920.0, 1080.0],
        );
        assert!(far.rect[0] >= 0.0 && far.rect[1] >= 0.0);
    }

    #[test]
    fn long_text_wraps_and_vertical_text_stacks() {
        let head = [900.0, 900.0];
        let long = "這是一段很長很長的對白，用來測試中文自動換行是否正常，而且不會超出畫面範圍。";
        let h = balloon(long, &BubbleSettings::default(), head, 1.0, [1920.0, 1080.0]);
        let lines: Vec<&P> = h.prims.iter().filter(|p| matches!(p, P::Text { .. })).collect();
        assert!(lines.len() >= 2, "{} lines", lines.len());
        let max_w = 26.0 * 14.5 + 26.0;
        for l in lines {
            let P::Text { text, size, .. } = l else { unreachable!() };
            assert!(Fonts::get().measure(text, *size) <= max_w, "{text}");
        }
        let v = balloon(
            "一杯拿鐵，少冰。",
            &BubbleSettings { vertical: true, ..Default::default() },
            head,
            1.0,
            [1920.0, 1080.0],
        );
        let chars: Vec<&P> = v.prims.iter().filter(|p| matches!(p, P::Text { .. })).collect();
        assert_eq!(chars.len(), 8, "one glyph per cell");
        // "，" would start column 2 with 7 cells per column: it hangs on column 1 instead.
        let hang = balloon(
            "一杯拿鐵、少冰，對吧？",
            &BubbleSettings { vertical: true, ..Default::default() },
            head,
            1.0,
            [1920.0, 1080.0],
        );
        let xs: Vec<f32> =
            hang.prims.iter().filter_map(|p| if let P::Text { pos, .. } = p { Some(pos[0]) } else { None }).collect();
        assert_eq!(xs.len(), 11);
        assert_eq!(xs[7], xs[0], "comma stays in the first column");
        assert!(xs[8] < xs[0], "next column is to the left");
        assert!(v.rect[3] - v.rect[1] > v.rect[2] - v.rect[0], "vertical balloon is tall");
    }

    #[test]
    fn caption_box_left_aligned() {
        let (prims, r) = caption("午後的街角，陽光灑在咖啡店的招牌上。", [16.0, 60.0], 500.0, 1.0);
        assert!(r[2] - r[0] <= 500.0 + 1.0);
        assert!(prims.iter().any(|p| matches!(p, P::Text { anchor, .. } if anchor[0] == 0.0)));
    }
}
