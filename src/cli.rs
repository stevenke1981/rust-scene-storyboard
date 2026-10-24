//! Headless command line: export / render without a display (CI, scripts, agents).

use std::path::{Path, PathBuf};

use crate::export::{ExportOptions, Formats, export_to, project_dir_name, render_shot};
use crate::mannequin::actions::{ACTIONS, floor_pose_names};
use crate::mannequin::skeleton::{Proportions, Skeleton};
use crate::model::{Project, VideoAspect, VideoPreset};

pub const HELP: &str = "\
rust-scene-storyboard — 影片分鏡：角色線稿姿勢 × 道具 × 場景 × 走位
用法 Usage:
  rust-scene-storyboard [project.json]                 開啟 GUI（可指定專案）
  rust-scene-storyboard --export <project.json> [out_base] [選項]
        建立 <out_base>/<專案名>_<YYYYMMDD_HHMMSS>/ 並輸出；stdout 第一行為資料夾路徑
        --formats <list>   png,overview,md,html,project,video_ai 或 all（all 不含 video_ai）
        --no-subdir        直接寫入 out_base
        --name <name>      子資料夾名稱前綴（預設為專案檔名）
        --no-labels / --no-motion / --no-dialogue / --no-header / --no-ghosts / --no-prop-labels
        --link-images      HTML 以檔名連結圖片（預設以 base64 內嵌）
        --depth / --depth16  每張草稿另存 shot_XX_<id>_depth.png 深度圖（近白遠黑；16 位元）
  rust-scene-storyboard --export-video-ai <project.json> <out_dir> [選項]
        影片 AI 用：每鏡 shot_XX_start.png / shot_XX_end.png（乾淨畫面）＋ prompts.txt/json/md
        --preset seedance|generic   目標模型（預設 seedance = Seedance 2.0 首尾幀）
        --aspect adaptive|16:9|9:16|4:3|3:4|1:1|21:9
        --style <text>              風格描述（預設 cinematic realistic, natural lighting）
        --keep-floor / --no-floor   保留淡淡的地平線
        --no-audio                  generate_audio = false
        --depth / --no-depth        首尾幀深度圖 shot_XX_start_depth.png / _end_depth.png（預設開）
        --depth16                   深度圖輸出 16 位元灰階 PNG
  rust-scene-storyboard --render <project.json> <N> <out.png>   只輸出第 N 個鏡頭
  rust-scene-storyboard --sample <out.json>            寫出範例專案
  rust-scene-storyboard --poses                        列出姿勢庫
  rust-scene-storyboard --help | --version
";

fn load(path: &str) -> Result<Project, String> {
    Project::load(Path::new(path))
}

fn fail(msg: impl AsRef<str>) -> Option<i32> {
    eprintln!("錯誤：{}", msg.as_ref());
    Some(2)
}

/// Run a CLI command. Returns `None` when the GUI should start.
pub fn run(args: &[String]) -> Option<i32> {
    let first = args.first()?.as_str();
    match first {
        "--help" | "-h" => {
            print!("{HELP}");
            Some(0)
        }
        "--version" | "-V" => {
            println!("rust-scene-storyboard {}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        "--poses" => {
            println!("動作姿勢 (action poses):");
            for (i, p) in ACTIONS.iter().enumerate() {
                println!("  {:>2}. {:<14} {}（{}）— {}", i + 1, p.key, p.zh, p.en, p.desc);
            }
            println!("地面姿勢 (rust-pose-studio floor poses):");
            for (i, n) in floor_pose_names(&Skeleton::new(Proportions::default())).iter().enumerate() {
                println!("  floor:{:<3} {n}", i + 1);
            }
            Some(0)
        }
        "--sample" => {
            let Some(out) = args.get(1) else { return fail("--sample 需要輸出檔名") };
            match crate::sample::sample_project().save(Path::new(out)) {
                Ok(()) => {
                    println!("{out}");
                    Some(0)
                }
                Err(e) => fail(e),
            }
        }
        "--render" => {
            let (Some(path), Some(n), Some(out)) = (args.get(1), args.get(2), args.get(3)) else {
                return fail("用法：--render <project.json> <N> <out.png>");
            };
            let p = match load(path) {
                Ok(p) => p,
                Err(e) => return fail(e),
            };
            let Ok(n) = n.parse::<usize>() else { return fail("N 必須是數字（從 1 開始）") };
            let res = render_shot(&p, n.saturating_sub(1), &ExportOptions::default())
                .and_then(|pm| crate::raster::encode_png(&pm))
                .and_then(|b| std::fs::write(out, b).map_err(|e| e.to_string()));
            match res {
                Ok(()) => {
                    println!("{out}");
                    Some(0)
                }
                Err(e) => fail(e),
            }
        }
        "--export" => {
            let Some(path) = args.get(1) else { return fail("--export 需要專案檔") };
            let p = match load(path) {
                Ok(p) => p,
                Err(e) => return fail(e),
            };
            let mut opts = ExportOptions::default();
            let mut base = PathBuf::from(".");
            let mut subdir = true;
            let mut name = project_dir_name(Some(Path::new(path)));
            let mut i = 2;
            while i < args.len() {
                let a = args[i].as_str();
                match a {
                    "--formats" => {
                        i += 1;
                        match args.get(i).map(|s| Formats::parse(s)) {
                            Some(Ok(f)) => opts.formats = f,
                            Some(Err(e)) => return fail(e),
                            None => return fail("--formats 需要清單"),
                        }
                    }
                    "--name" => {
                        i += 1;
                        let Some(n) = args.get(i) else { return fail("--name 需要名稱") };
                        name = n.clone();
                    }
                    "--no-subdir" => subdir = false,
                    "--no-labels" => opts.labels = false,
                    "--no-motion" => opts.motion = false,
                    "--no-dialogue" => opts.dialogue = false,
                    "--no-header" => opts.header = false,
                    "--no-ghosts" => opts.ghosts = false,
                    "--no-prop-labels" => opts.prop_labels = false,
                    "--link-images" => opts.embed_images = false,
                    "--depth" => opts.depth = true,
                    "--depth16" => {
                        opts.depth = true;
                        opts.depth16 = true;
                    }
                    s if s.starts_with("--") => return fail(format!("未知的選項 {s}")),
                    s => base = PathBuf::from(s),
                }
                i += 1;
            }
            if opts.formats == Formats::NONE {
                return fail("沒有選擇任何輸出格式");
            }
            match export_to(&p, &base, &name, subdir, &opts) {
                Ok(rep) => {
                    println!("{}", rep.dir.display());
                    for f in rep.files {
                        println!("{}", f.display());
                    }
                    Some(0)
                }
                Err(e) => fail(e),
            }
        }
        "--export-video-ai" => {
            let (Some(path), Some(out)) = (args.get(1), args.get(2)) else {
                return fail("用法：--export-video-ai <project.json> <out_dir> [選項]");
            };
            let mut p = match load(path) {
                Ok(p) => p,
                Err(e) => return fail(e),
            };
            let mut i = 3;
            while i < args.len() {
                let a = args[i].as_str();
                let mut val = || {
                    i += 1;
                    args.get(i).cloned().ok_or_else(|| format!("{a} 需要參數"))
                };
                match a {
                    "--preset" => match val().map(|v| v.to_ascii_lowercase()) {
                        Ok(v) if v.starts_with("seedance") => p.video_ai.preset = VideoPreset::Seedance2,
                        Ok(v) if v == "generic" => p.video_ai.preset = VideoPreset::Generic,
                        Ok(v) => return fail(format!("未知的 preset {v}（seedance|generic）")),
                        Err(e) => return fail(e),
                    },
                    "--aspect" => match val() {
                        Ok(v) => match VideoAspect::ALL.iter().find(|x| x.key() == v) {
                            Some(x) => p.video_ai.aspect = *x,
                            None => return fail(format!("未知的比例 {v}")),
                        },
                        Err(e) => return fail(e),
                    },
                    "--style" => match val() {
                        Ok(v) => p.video_ai.style = v,
                        Err(e) => return fail(e),
                    },
                    "--keep-floor" => p.video_ai.keep_floor = true,
                    "--no-floor" => p.video_ai.keep_floor = false,
                    "--no-audio" => p.video_ai.audio = false,
                    "--depth" => p.video_ai.depth = true,
                    "--no-depth" => p.video_ai.depth = false,
                    "--depth16" => {
                        p.video_ai.depth = true;
                        p.video_ai.depth16 = true;
                    }
                    s => return fail(format!("未知的選項 {s}")),
                }
                i += 1;
            }
            match crate::video_ai::export_video_ai(&p, Path::new(out)) {
                Ok(rep) => {
                    println!("{}", rep.dir.display());
                    for f in rep.files {
                        println!("{}", f.display());
                    }
                    Some(0)
                }
                Err(e) => fail(e),
            }
        }
        s if s.starts_with("--") => fail(format!("未知的指令 {s}（--help 查看用法）")),
        _ => None,
    }
}
