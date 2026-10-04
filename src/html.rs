//! Single-file HTML storyboard: shot drafts embedded as base64 PNG next to the
//! scene / camera / staging narration.

use crate::describe::{describe_shot, shot_summary};
use crate::draw::circled;

/// Standard base64 (RFC 4648) without line breaks.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Escape text for HTML.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            '\n' => o.push_str("<br>"),
            c => o.push(c),
        }
    }
    o
}

const CSS: &str = r#"
:root { --ink:#1d1e22; --muted:#6b6f78; --line:#e3e5ea; --accent:#f08014; --bg:#f6f6f4; }
* { box-sizing: border-box; }
body { margin:0; font-family: "Noto Sans TC","Noto Sans CJK TC","Microsoft JhengHei","PingFang TC",sans-serif; color:var(--ink); background:var(--bg); line-height:1.65; }
header { background:#202227; color:#fff; padding:28px 40px; }
header h1 { margin:0 0 6px; font-size:28px; }
header p { margin:0; color:#c9ccd3; }
main { max-width:1280px; margin:0 auto; padding:24px 28px 60px; }
section.card { background:#fff; border:1px solid var(--line); border-radius:12px; padding:22px 26px; margin:22px 0; box-shadow:0 1px 3px rgba(0,0,0,.04); }
h2 { margin:0 0 12px; font-size:22px; }
h2 .no { display:inline-block; background:var(--accent); color:#fff; border-radius:8px; padding:0 10px; margin-right:8px; }
h3 { font-size:16px; margin:18px 0 8px; color:#333; border-left:4px solid var(--accent); padding-left:8px; }
.shot { display:grid; grid-template-columns: minmax(0,1.25fr) minmax(0,1fr); gap:24px; }
.shot img { width:100%; border:1px solid var(--line); border-radius:8px; background:#fff; }
.meta { display:flex; flex-wrap:wrap; gap:6px; margin:6px 0 10px; }
.tag { background:#f0f1f4; border-radius:999px; padding:2px 10px; font-size:13px; color:#333; }
.narr { background:#fff8ef; border-left:4px solid var(--accent); padding:10px 14px; border-radius:6px; }
.vo { background:#f3f6ff; border-left:4px solid #3c6fd8; padding:10px 14px; border-radius:6px; }
table { border-collapse:collapse; width:100%; font-size:14px; }
th, td { border-bottom:1px solid var(--line); text-align:left; padding:6px 8px; vertical-align:top; }
th { background:#fafafa; color:#555; font-weight:600; }
.actor { border:1px solid var(--line); border-radius:10px; padding:10px 14px; margin:8px 0; }
.actor h4 { margin:0 0 4px; font-size:16px; }
.dot { display:inline-block; width:12px; height:12px; border-radius:50%; margin-right:6px; vertical-align:middle; }
.actor dl { display:grid; grid-template-columns: 4.5em 1fr; gap:2px 10px; margin:0; font-size:14px; }
.actor dt { color:var(--muted); }
.actor dd { margin:0; }
.say { font-weight:600; }
.timeline li { margin:2px 0; }
.overview { display:grid; grid-template-columns: repeat(auto-fill, minmax(260px,1fr)); gap:14px; }
.overview a { color:inherit; text-decoration:none; }
.overview figure { margin:0; border:1px solid var(--line); border-radius:8px; overflow:hidden; background:#fff; }
.overview img { width:100%; display:block; }
.overview figcaption { padding:6px 10px; font-size:13px; }
footer { color:var(--muted); font-size:13px; text-align:center; padding:20px; }
@media (max-width: 900px) { .shot { grid-template-columns: 1fr; } }
@media print { body { background:#fff; } section.card { break-inside: avoid; box-shadow:none; } }
"#;

/// Build the storyboard HTML. `pngs[i]` is the encoded draft of shot `i`; when
/// `embed` is false the images are linked by file name instead.
pub fn storyboard_html(p: &crate::model::Project, pngs: &[Vec<u8>], embed: bool) -> String {
    let mut h = String::new();
    h.push_str("<!DOCTYPE html>\n<html lang=\"zh-Hant\">\n<head>\n<meta charset=\"utf-8\">\n");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    h.push_str(&format!("<title>{} — 分鏡腳本</title>\n<style>{CSS}</style>\n</head>\n<body>\n", esc(&p.title)));
    h.push_str(&format!(
        "<header><h1>{}</h1><p>分鏡腳本 Storyboard · {} 個鏡頭 · 總長 {:.1} 秒 · {}×{} px</p></header>\n<main>\n",
        esc(&p.title),
        p.shots.len(),
        p.total_duration(),
        p.canvas.width,
        p.canvas.height
    ));
    let src = |i: usize| -> String {
        if embed && let Some(b) = pngs.get(i) {
            format!("data:image/png;base64,{}", base64(b))
        } else {
            esc(&crate::describe::shot_png_name(i, &p.shots[i].id))
        }
    };
    if !p.synopsis.trim().is_empty() || !p.cast.is_empty() {
        h.push_str("<section class=\"card\">");
        if !p.synopsis.trim().is_empty() {
            h.push_str(&format!("<h2>故事概要</h2><p>{}</p>", esc(p.synopsis.trim())));
        }
        if !p.cast.is_empty() {
            h.push_str("<h3>角色表</h3><table><tr><th>角色</th><th>身高</th><th>設定</th></tr>");
            for c in &p.cast {
                h.push_str(&format!(
                    "<tr><td><span class=\"dot\" style=\"background:{}\"></span><b>{}</b> <code>{}</code></td><td>{:.2} m</td><td>{}</td></tr>",
                    c.color.hex(),
                    esc(&c.name),
                    esc(&c.id),
                    c.height,
                    esc(&c.description)
                ));
            }
            h.push_str("</table>");
        }
        h.push_str("</section>\n");
    }
    // overview grid
    h.push_str("<section class=\"card\"><h2>鏡頭總覽</h2><div class=\"overview\">");
    for (i, s) in p.shots.iter().enumerate() {
        h.push_str(&format!(
            "<a href=\"#{}\"><figure><img src=\"{}\" alt=\"鏡頭 {:02}\"><figcaption><b>{:02} {}</b><br>{}</figcaption></figure></a>",
            esc(&s.id),
            src(i),
            i + 1,
            i + 1,
            esc(&s.name),
            esc(&shot_summary(p, i))
        ));
    }
    h.push_str("</div></section>\n");

    for i in 0..p.shots.len() {
        let d = describe_shot(p, i);
        h.push_str(&format!(
            "<section class=\"card\" id=\"{}\"><h2><span class=\"no\">{:02}</span>{}</h2>",
            esc(&d.id),
            i + 1,
            esc(&d.name)
        ));
        h.push_str("<div class=\"meta\">");
        for t in [
            format!("{:.1}–{:.1} 秒（{:.1} 秒）", d.start, d.start + d.duration, d.duration),
            format!("{}・{}", d.in_out, d.time_of_day),
            if d.location.is_empty() { "未設定地點".into() } else { d.location.clone() },
            d.camera.clone(),
        ] {
            h.push_str(&format!("<span class=\"tag\">{}</span>", esc(&t)));
        }
        h.push_str("</div><div class=\"shot\"><div>");
        h.push_str(&format!("<img src=\"{}\" alt=\"鏡頭 {:02} 草稿\">", src(i), i + 1));
        if !d.weather.is_empty() || !d.environment.is_empty() || !d.camera_notes.is_empty() {
            h.push_str("<h3>場景與攝影機</h3><table>");
            if !d.weather.is_empty() {
                h.push_str(&format!("<tr><th>天氣／光線</th><td>{}</td></tr>", esc(&d.weather)));
            }
            if !d.environment.is_empty() {
                h.push_str(&format!("<tr><th>環境</th><td>{}</td></tr>", esc(&d.environment)));
            }
            h.push_str(&format!("<tr><th>攝影機</th><td>{}</td></tr>", esc(&d.camera)));
            if !d.camera_notes.is_empty() {
                h.push_str(&format!("<tr><th>鏡頭備註</th><td>{}</td></tr>", esc(&d.camera_notes)));
            }
            h.push_str("</table>");
        }
        h.push_str("</div><div>");
        h.push_str(&format!("<h3>畫面敘述</h3><div class=\"narr\">{}</div>", esc(&d.narrative)));
        if !d.narration.is_empty() {
            h.push_str(&format!("<h3>旁白</h3><div class=\"vo\">{}</div>", esc(&d.narration)));
        }
        if !d.actors.is_empty() {
            h.push_str("<h3>人物與走位</h3>");
            for a in &d.actors {
                h.push_str(&format!(
                    "<div class=\"actor\"><h4><span class=\"dot\" style=\"background:{}\"></span>{} <code>{}</code></h4><dl>",
                    a.color,
                    esc(&a.name),
                    esc(&a.id)
                ));
                let mut row = |k: &str, v: &str, class: &str| {
                    if !v.is_empty() {
                        h.push_str(&format!("<dt>{k}</dt><dd class=\"{class}\">{}</dd>", esc(v)));
                    }
                };
                row("位置", &a.position, "");
                row("朝向", &a.facing, "");
                row("姿勢", &a.pose, "");
                row("大小", &a.size, "");
                row("動作", &a.action, "");
                row("表情", &a.expression, "");
                let say = if a.dialogue.is_empty() { String::new() } else { format!("「{}」", a.dialogue) };
                row("對白", &say, "say");
                if let Some(m) = &a.movement {
                    row(&format!("移動 {}", circled(m.number)), &m.text, "");
                }
                h.push_str("</dl></div>");
            }
        }
        if !d.relations.is_empty() {
            h.push_str("<h3>人物相對位置</h3><ul>");
            for r in &d.relations {
                h.push_str(&format!("<li>{}</li>", esc(r)));
            }
            h.push_str("</ul>");
        }
        h.push_str("</div></div>");
        if !d.props.is_empty() {
            h.push_str("<h3>道具與佈景</h3><table><tr><th>道具</th><th>類型</th><th>位置</th><th>大小</th><th>與人物的關係</th><th>備註</th></tr>");
            for pr in &d.props {
                h.push_str(&format!(
                    "<tr><td><b>{}</b> <code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    esc(&pr.name),
                    esc(&pr.id),
                    esc(&pr.kind),
                    esc(&pr.position),
                    esc(&pr.size),
                    esc(&pr.relation),
                    esc(&pr.notes)
                ));
            }
            h.push_str("</table>");
        }
        if !d.notes.is_empty() {
            h.push_str(&format!("<h3>導演備註</h3><p>{}</p>", esc(&d.notes)));
        }
        h.push_str("</section>\n");
    }
    h.push_str(&format!(
        "</main><footer>由 <a href=\"https://github.com/stevenke1981/rust-scene-storyboard\">rust-scene-storyboard</a> {} 產生</footer>\n</body>\n</html>\n",
        env!("CARGO_PKG_VERSION")
    ));
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_and_escape() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(esc("<a & \"b\">"), "&lt;a &amp; &quot;b&quot;&gt;");
    }

    #[test]
    fn html_contains_shots() {
        let p = crate::sample::sample_project();
        let html = storyboard_html(&p, &[vec![1, 2, 3]], true);
        assert!(html.contains("data:image/png;base64,AQID"));
        assert!(html.contains(&p.shots[1].name));
        assert!(html.contains("人物與走位"));
        let linked = storyboard_html(&p, &[], false);
        assert!(linked.contains("shot_01_shot_1.png"));
    }
}
