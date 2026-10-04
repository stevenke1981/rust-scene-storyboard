//! Posable 3D line-art mannequin, vendored from
//! [rust-pose-studio](https://github.com/stevenke1981/rust-pose-studio) (same author, MIT).
//!
//! * [`skeleton`] – 17-joint hierarchy, proportions, poses, forward kinematics
//! * [`body`] – tapered body volumes, floor snapping, drag handles
//! * [`ik`], [`posing`] – two-bone IK, limb aiming, FK dragging
//! * [`lineart`] – contour / crease extraction with a software z-buffer
//! * [`render`] – camera, projection and the backend independent draw list
//! * [`floor_presets`] – the 25 floor poses of rust-pose-studio (+ the pose builder DSL)
//! * [`actions`] – storyboard action poses (standing, walking, running, sitting, pointing…)
//!
//! Only small additions were made for this app (see `render::draw_body_frame`).

pub mod actions;
pub mod body;
pub mod floor_presets;
pub mod ik;
pub mod lineart;
pub mod math;
pub mod posing;
pub mod render;
pub mod skeleton;
