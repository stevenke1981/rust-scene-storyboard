// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use eframe::egui;

/// Release builds use the GUI subsystem on Windows; re-attach to the parent console so
/// the command-line interface can print when started from a terminal.
#[cfg(windows)]
fn attach_parent_console() {
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: plain Win32 call without pointers; failure (no parent console) is harmless.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// 64×64 icon: a storyboard frame with a stick figure and a motion arrow.
fn icon() -> egui::IconData {
    let n = 64usize;
    let mut rgba = vec![0u8; n * n * 4];
    let mut set = |x: i32, y: i32, c: [u8; 4]| {
        if (0..n as i32).contains(&x) && (0..n as i32).contains(&y) {
            let i = (y as usize * n + x as usize) * 4;
            rgba[i..i + 4].copy_from_slice(&c);
        }
    };
    let ink = [30, 30, 36, 255];
    let accent = [240, 128, 20, 255];
    for y in 6..58 {
        for x in 2..62 {
            let border = !(5..=58).contains(&x) || !(9..=54).contains(&y);
            set(x, y, if border { ink } else { [250, 250, 250, 255] });
        }
    }
    let mut line = |x0: f32, y0: f32, x1: f32, y1: f32, c: [u8; 4], w: f32| {
        let steps = 80;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            let r = w / 2.0;
            for dy in -(r.ceil() as i32)..=(r.ceil() as i32) {
                for dx in -(r.ceil() as i32)..=(r.ceil() as i32) {
                    if ((dx * dx + dy * dy) as f32) <= r * r + 0.5 {
                        set(x.round() as i32 + dx, y.round() as i32 + dy, c);
                    }
                }
            }
        }
    };
    // stick figure
    for a in 0..40 {
        let t = a as f32 / 40.0 * std::f32::consts::TAU;
        line(22.0 + 4.5 * t.cos(), 18.0 + 4.5 * t.sin(), 22.0 + 4.5 * t.cos(), 18.0 + 4.5 * t.sin(), ink, 2.0);
    }
    line(22.0, 23.0, 22.0, 36.0, ink, 2.5);
    line(22.0, 27.0, 15.0, 33.0, ink, 2.5);
    line(22.0, 27.0, 29.0, 31.0, ink, 2.5);
    line(22.0, 36.0, 16.0, 48.0, ink, 2.5);
    line(22.0, 36.0, 28.0, 48.0, ink, 2.5);
    // motion arrow
    line(32.0, 46.0, 52.0, 46.0, accent, 3.0);
    line(52.0, 46.0, 46.0, 41.0, accent, 3.0);
    line(52.0, 46.0, 46.0, 51.0, accent, 3.0);
    egui::IconData { rgba, width: 64, height: 64 }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if !args.is_empty() {
        attach_parent_console();
    }
    if let Some(code) = rss::cli::run(&args) {
        std::process::exit(code);
    }
    let open = args.first().cloned();
    let viewport = egui::ViewportBuilder::default()
        .with_title("影片分鏡 Rust Scene Storyboard")
        .with_inner_size([1560.0, 940.0])
        .with_min_inner_size([1100.0, 680.0])
        .with_app_id("rust-scene-storyboard")
        .with_icon(icon());
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("rust-scene-storyboard", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open)))))
}
