//! rust-scene-storyboard: video scene storyboards with posable line-art characters.
//!
//! * [`mannequin`] – the 3D line-art mannequin from rust-pose-studio (+ action poses)
//! * [`model`] – project, cast, shots, actors, props, movement
//! * [`figure`] – placing posed figures on the canvas (facing, scale, camera angle)
//! * [`draw`] – backend independent draft drawing (props, motion arrows, labels…)
//! * [`raster`] – tiny-skia rasteriser for PNG export (CJK text via ab_glyph)
//! * [`describe`], [`html`] – Markdown / HTML narration of every shot
//! * [`export`], [`cli`] – export folders and the headless command line
//! * `app` – the egui editor (binary only)

pub mod bubble;
pub mod cli;
pub mod describe;
pub mod draw;
pub mod export;
pub mod figure;
pub mod fonts;
pub mod html;
pub mod mannequin;
pub mod model;
pub mod raster;
pub mod sample;
