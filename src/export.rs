//! Export: per-shot draft PNGs, an overview contact sheet, storyboard.md,
//! storyboard.html and project.json — each export in its own timestamped folder
//! (same folder scheme as whitebox-video-storyboard).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tiny_skia::Pixmap;

use crate::describe::{shot_png_name, shot_summary, storyboard_md};
use crate::draw::{DraftOptions, P, shot_items};
use crate::html::storyboard_html;
use crate::model::Project;
use crate::raster;

/// Which files an export writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Formats {
    /// `shot_XX_<id>.png` draft per shot.
    pub png: bool,
    /// `storyboard_overview.png` contact sheet of all shots.
    pub overview: bool,
    /// `storyboard.md` narration (Traditional Chinese).
    pub md: bool,
    /// `storyboard.html` (self-contained, images embedded).
    pub html: bool,
    /// `project.json` (re-editable project file).
    pub project: bool,
}

impl Default for Formats {
    fn default() -> Self {
        Formats { png: true, overview: true, md: true, html: true, project: true }
    }
}

impl Formats {
    /// `(cli key, 中文 label, file name)` for every format, in display order.
    pub const INFO: [(&'static str, &'static str, &'static str); 5] = [
        ("png", "鏡頭草稿 PNG", "shot_XX_*.png"),
        ("overview", "分鏡總覽圖", "storyboard_overview.png"),
        ("md", "Markdown 敘述", "storyboard.md"),
        ("html", "HTML 分鏡頁", "storyboard.html"),
        ("project", "專案檔 JSON", "project.json"),
    ];

    pub const NONE: Formats = Formats { png: false, overview: false, md: false, html: false, project: false };

    pub fn get_mut(&mut self, key: &str) -> Option<&mut bool> {
        Some(match key {
            "png" => &mut self.png,
            "overview" => &mut self.overview,
            "md" => &mut self.md,
            "html" => &mut self.html,
            "project" => &mut self.project,
            _ => return None,
        })
    }

    /// Parse a comma separated list (`png,md,html` or `all`).
    pub fn parse(list: &str) -> Result<Formats, String> {
        let mut f = Formats::NONE;
        for k in list.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()) {
            if k == "all" {
                return Ok(Formats::default());
            }
            let key = match k.as_str() {
                "markdown" => "md",
                "json" => "project",
                "sheet" | "contact" => "overview",
                other => other,
            };
            *f.get_mut(key).ok_or_else(|| format!("未知的格式 \"{k}\"（可用：png,overview,md,html,project,all）"))? =
                true;
        }
        Ok(f)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    pub formats: Formats,
    pub labels: bool,
    pub motion: bool,
    pub dialogue: bool,
    pub header: bool,
    pub ghosts: bool,
    pub prop_labels: bool,
    /// Embed the PNGs in storyboard.html (otherwise link to the files).
    pub embed_images: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            formats: Formats::default(),
            labels: true,
            motion: true,
            dialogue: true,
            header: true,
            ghosts: true,
            prop_labels: true,
            embed_images: true,
        }
    }
}

impl ExportOptions {
    pub fn draft(&self) -> DraftOptions {
        DraftOptions {
            labels: self.labels,
            motion: self.motion,
            dialogue: self.dialogue,
            header: self.header,
            ghosts: self.ghosts,
            prop_labels: self.prop_labels,
        }
    }
}

/// Folder-name-safe version of a project / file name (keeps CJK letters).
pub fn sanitize_name(name: &str) -> String {
    let s: String =
        name.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let s = s.split('_').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("_");
    if s.is_empty() { "untitled".into() } else { s.chars().take(60).collect() }
}

/// Export folder base name for a project file path: its file stem, or `untitled`.
pub fn project_dir_name(path: Option<&Path>) -> String {
    path.and_then(|p| p.file_stem()).map(|s| sanitize_name(&s.to_string_lossy())).unwrap_or_else(|| "untitled".into())
}

/// A not-yet-existing `<base>/<name>_<YYYYMMDD_HHMMSS>[_N]` directory path (local time).
pub fn timestamped_dir(base: &Path, name: &str) -> PathBuf {
    let stem = format!("{}_{}", sanitize_name(name), chrono::Local::now().format("%Y%m%d_%H%M%S"));
    let mut dir = base.join(&stem);
    let mut n = 2;
    while dir.exists() {
        dir = base.join(format!("{stem}_{n}"));
        n += 1;
    }
    dir
}

#[derive(Debug, Default)]
pub struct ExportReport {
    pub dir: PathBuf,
    pub files: Vec<PathBuf>,
}

/// Render the draft of shot `index`.
pub fn render_shot(p: &Project, index: usize, opts: &ExportOptions) -> Result<Pixmap, String> {
    let shot = p.shots.get(index).ok_or_else(|| format!("沒有第 {} 個鏡頭", index + 1))?;
    let items = shot_items(p, shot, &opts.draft());
    raster::render_items(&items, p.canvas.width, p.canvas.height)
}

/// Contact sheet of all shots with captions.
pub fn render_overview(p: &Project, shots: &[Pixmap]) -> Result<Pixmap, String> {
    let n = shots.len().max(1);
    let cols = if n <= 4 { n.clamp(1, 2) } else { 3 };
    let rows = n.div_ceil(cols);
    let tw = 640.0f32;
    let th = tw * p.canvas.height as f32 / p.canvas.width as f32;
    let (gap, cap, title_h) = (24.0f32, 64.0f32, 70.0f32);
    let w = gap + cols as f32 * (tw + gap);
    let h = title_h + rows as f32 * (th + cap + gap) + gap;
    let mut pm = Pixmap::new(w as u32, h as u32).ok_or("invalid overview size")?;
    pm.fill(tiny_skia::Color::from_rgba8(246, 246, 244, 255));
    raster::draw_prim(
        &mut pm,
        &P::Text {
            pos: [gap, 22.0],
            text: format!("{} — 分鏡總覽（{} 個鏡頭，{:.1} 秒）", p.title, p.shots.len(), p.total_duration()),
            size: 30.0,
            color: [30, 30, 34, 255],
            anchor: [0.0, 0.0],
            bg: None,
        },
    );
    for (i, img) in shots.iter().enumerate() {
        let (c, r) = (i % cols, i / cols);
        let x = gap + c as f32 * (tw + gap);
        let y = title_h + r as f32 * (th + cap + gap);
        raster::draw_prim(
            &mut pm,
            &P::Poly {
                pts: vec![
                    [x - 1.0, y - 1.0],
                    [x + tw + 1.0, y - 1.0],
                    [x + tw + 1.0, y + th + cap],
                    [x - 1.0, y + th + cap],
                ],
                closed: true,
                fill: Some([255, 255, 255, 255]),
                stroke: Some(crate::draw::Stroke2 { width: 1.0, color: [210, 212, 218, 255] }),
                dash: false,
            },
        );
        let s = tw / img.width() as f32;
        let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
        pm.draw_pixmap(0, 0, img.as_ref(), &paint, tiny_skia::Transform::from_row(s, 0.0, 0.0, s, x, y), None);
        let shot = &p.shots[i];
        raster::draw_prim(
            &mut pm,
            &P::Text {
                pos: [x + 10.0, y + th + 8.0],
                text: format!("{:02}  {}", i + 1, shot.name),
                size: 22.0,
                color: [20, 20, 24, 255],
                anchor: [0.0, 0.0],
                bg: None,
            },
        );
        let mut summary = shot_summary(p, i);
        while raster::Fonts::get().measure(&summary, 16.0) > tw - 20.0 && summary.chars().count() > 4 {
            summary = summary.chars().take(summary.chars().count() - 2).collect::<String>();
            summary = format!("{}…", summary.trim_end_matches('…'));
        }
        raster::draw_prim(
            &mut pm,
            &P::Text {
                pos: [x + 10.0, y + th + 38.0],
                text: summary,
                size: 16.0,
                color: [100, 102, 108, 255],
                anchor: [0.0, 0.0],
                bg: None,
            },
        );
    }
    Ok(pm)
}

fn write(dir: &Path, name: &str, data: &[u8], files: &mut Vec<PathBuf>) -> Result<(), String> {
    let path = dir.join(name);
    std::fs::write(&path, data).map_err(|e| format!("寫入 {} 失敗：{e}", path.display()))?;
    files.push(path);
    Ok(())
}

/// Write the selected formats directly into `dir` (created if needed).
pub fn export_all(p: &Project, dir: &Path, opts: &ExportOptions) -> Result<ExportReport, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("無法建立資料夾 {}：{e}", dir.display()))?;
    let f = opts.formats;
    let mut files = vec![];
    let need_images = f.png || f.overview || (f.html && opts.embed_images);
    let mut pixmaps = vec![];
    let mut pngs = vec![];
    if need_images {
        for i in 0..p.shots.len() {
            let pm = render_shot(p, i, opts)?;
            pngs.push(raster::encode_png(&pm)?);
            pixmaps.push(pm);
        }
    }
    if f.png {
        for (i, png) in pngs.iter().enumerate() {
            write(dir, &shot_png_name(i, &p.shots[i].id), png, &mut files)?;
        }
    }
    if f.overview {
        let ov = render_overview(p, &pixmaps)?;
        write(dir, "storyboard_overview.png", &raster::encode_png(&ov)?, &mut files)?;
    }
    if f.md {
        write(dir, "storyboard.md", storyboard_md(p, f.png).as_bytes(), &mut files)?;
    }
    if f.html {
        write(dir, "storyboard.html", storyboard_html(p, &pngs, opts.embed_images).as_bytes(), &mut files)?;
    }
    if f.project {
        write(dir, "project.json", p.to_json().as_bytes(), &mut files)?;
    }
    Ok(ExportReport { dir: dir.to_path_buf(), files })
}

/// Export into `<base>/<name>_<timestamp>/` (or straight into `base` when `subdir` is false).
pub fn export_to(
    p: &Project,
    base: &Path,
    name: &str,
    subdir: bool,
    opts: &ExportOptions,
) -> Result<ExportReport, String> {
    let dir = if subdir { timestamped_dir(base, name) } else { base.to_path_buf() };
    export_all(p, &dir, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_sample_writes_everything() {
        let p = crate::sample::sample_project();
        let dir = std::env::temp_dir().join(format!("rss_export_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rep = export_all(&p, &dir, &ExportOptions::default()).unwrap();
        assert_eq!(rep.files.len(), p.shots.len() + 4);
        for f in &rep.files {
            assert!(std::fs::metadata(f).unwrap().len() > 100, "{}", f.display());
        }
        let png = image::open(dir.join("shot_01_shot_1.png")).unwrap();
        assert_eq!((png.width(), png.height()), (p.canvas.width, p.canvas.height));
        // the draft is mostly light paper with dark line art
        let rgb = png.to_rgb8();
        let dark = rgb.pixels().filter(|px| px.0.iter().all(|&v| v < 80)).count();
        assert!(dark > 2000, "dark pixels {dark}");
        let html = std::fs::read_to_string(dir.join("storyboard.html")).unwrap();
        assert!(html.contains("data:image/png;base64,"));
        let md = std::fs::read_to_string(dir.join("storyboard.md")).unwrap();
        assert!(md.contains("![鏡頭 01 草稿](shot_01_shot_1.png)"));
        let back = Project::load(&dir.join("project.json")).unwrap();
        assert_eq!(back, p);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn formats_parse() {
        assert_eq!(Formats::parse("all").unwrap(), Formats::default());
        let f = Formats::parse("png, markdown").unwrap();
        assert!(f.png && f.md && !f.html && !f.overview);
        assert!(Formats::parse("gif").is_err());
    }

    #[test]
    fn dir_names() {
        assert_eq!(sanitize_name("咖啡店 的相遇!"), "咖啡店_的相遇");
        assert_eq!(project_dir_name(None), "untitled");
        let d = timestamped_dir(Path::new("/tmp"), "x y");
        assert!(d.file_name().unwrap().to_string_lossy().starts_with("x_y_20"));
    }
}
