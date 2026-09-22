//! Headless project model and export pipeline for OpenCut.
//!
//! This crate has no UI dependency. The desktop app and (once built) the MCP
//! server both sit on top of it, so an agent editing through MCP tools and a
//! person editing through the GPUI shell are driving the same project state.

pub mod ffmpeg;
pub mod project;

pub use ffmpeg::{FfmpegError, MediaInfo};
pub use project::{Clip, Project, ProjectError, TimelineItem};
