//! Headless CLI smoke test: write the sample project, export it, check the outputs.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rust-scene-storyboard"))
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rss-cli-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn sample_and_export() {
    let dir = tmp("export");
    let json = dir.join("sample.json");
    let st = bin().arg("--sample").arg(&json).status().unwrap();
    assert!(st.success());
    let out = dir.join("out");
    let st = bin().arg("--export").arg(&json).arg(&out).arg("--no-subdir").status().unwrap();
    assert!(st.success());
    for f in ["storyboard.md", "storyboard.html", "storyboard_overview.png", "project.json", "shot_01_shot_1.png"] {
        let p = out.join(f);
        assert!(p.is_file(), "missing {}", p.display());
        assert!(std::fs::metadata(&p).unwrap().len() > 100);
    }
    let md = std::fs::read_to_string(out.join("storyboard.md")).unwrap();
    assert!(md.contains("小明") && md.contains("走位"));
    let html = std::fs::read_to_string(out.join("storyboard.html")).unwrap();
    assert!(html.contains("data:image/png;base64,"));
    let png = std::fs::read(out.join("shot_01_shot_1.png")).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn render_single_shot_and_bad_input() {
    let dir = tmp("render");
    let json = dir.join("sample.json");
    assert!(bin().arg("--sample").arg(&json).status().unwrap().success());
    let png = dir.join("s2.png");
    assert!(bin().arg("--render").arg(&json).arg("2").arg(&png).status().unwrap().success());
    assert!(png.is_file());
    let bad = dir.join("bad.json");
    std::fs::write(&bad, "{\"format\":\"something-else\"}").unwrap();
    assert!(!bin().arg("--export").arg(&bad).arg(dir.join("x")).status().unwrap().success());
    let _ = std::fs::remove_dir_all(&dir);
}
