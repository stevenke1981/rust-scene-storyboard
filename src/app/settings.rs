//! Small persisted preferences (export choices, editor toggles) in the user config dir.
//! Same scheme as whitebox-video-storyboard.

use rss::export::ExportOptions;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub export: ExportOptions,
    /// Last chosen export base folder.
    pub export_base: Option<PathBuf>,
    /// Create `<name>_<timestamp>` sub-folder (default on).
    pub export_subdir: bool,
    /// Scale characters / props with depth while dragging them.
    pub auto_depth: bool,
    /// Show joint handles of the selected character.
    pub show_handles: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            export: ExportOptions::default(),
            export_base: None,
            export_subdir: true,
            auto_depth: true,
            show_handles: true,
        }
    }
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn config_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home_dir().map(|h| h.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| home_dir().map(|h| h.join(".config")))
    }?;
    Some(base.join("rust-scene-storyboard").join("settings.json"))
}

impl Settings {
    pub fn load() -> Settings {
        config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = config_path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, serde_json::to_string_pretty(self).unwrap_or_default());
        }
    }
}

/// Open a folder in the platform file manager.
pub fn open_folder(dir: &std::path::Path) {
    let cmd = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(dir).spawn();
}
