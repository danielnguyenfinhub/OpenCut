use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Project;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub enum FfmpegError {
    /// `ffmpeg`/`ffprobe` is not on PATH.
    ToolNotFound {
        tool: &'static str,
    },
    ProbeFailed {
        path: PathBuf,
        stderr: String,
    },
    ProbeOutputUnreadable {
        path: PathBuf,
        reason: String,
    },
    ExportFailed {
        stderr: String,
    },
    EmptyTimeline,
    ClipNotFound(String),
}

impl fmt::Display for FfmpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ToolNotFound { tool } => write!(
                f,
                "{tool} was not found on PATH. OpenCut shells out to ffmpeg for media probing \
                 and export. Install it from https://ffmpeg.org and make sure {tool} runs from \
                 a new terminal, then retry."
            ),
            Self::ProbeFailed { path, stderr } => write!(
                f,
                "ffprobe could not read {}: {}. The file may be missing, unreadable, or not a \
                 media file ffmpeg understands.",
                path.display(),
                stderr.trim()
            ),
            Self::ProbeOutputUnreadable { path, reason } => write!(
                f,
                "ffprobe's output for {} was not the JSON this build expects ({reason}). This is \
                 a bug in opencut-core, not in the file; report it with the ffprobe version.",
                path.display()
            ),
            Self::ExportFailed { stderr } => {
                write!(
                    f,
                    "ffmpeg export failed: {}. Check that every clip path in the project still exists.",
                    stderr.trim()
                )
            }
            Self::EmptyTimeline => {
                write!(f, "cannot export a project with nothing on the timeline")
            }
            Self::ClipNotFound(id) => write!(
                f,
                "timeline references clip {id}, which is not in the project's clip pool"
            ),
        }
    }
}

impl std::error::Error for FfmpegError {}

/// Runs `ffprobe` on a media file and reads its duration and video dimensions.
/// Mirrors `apps/web/src/lib/clips.ts::probeVideo`, so a clip probed on the
/// desktop or through the MCP server reports the same shape as one probed
/// in the browser.
pub fn probe(path: &Path) -> Result<MediaInfo, FfmpegError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_entries",
            "format=duration:stream=width,height,codec_type",
        ])
        .arg(path)
        .output()
        .map_err(|_| FfmpegError::ToolNotFound { tool: "ffprobe" })?;

    if !output.status.success() {
        return Err(FfmpegError::ProbeFailed {
            path: path.to_path_buf(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    parse_probe_output(path, &output.stdout)
}

fn parse_probe_output(path: &Path, stdout: &[u8]) -> Result<MediaInfo, FfmpegError> {
    let json: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|err| FfmpegError::ProbeOutputUnreadable {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;

    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(f64::NAN);

    let video_stream = json["streams"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|stream| stream["codec_type"] == "video");

    let width = video_stream
        .and_then(|stream| stream["width"].as_u64())
        .unwrap_or(0) as u32;
    let height = video_stream
        .and_then(|stream| stream["height"].as_u64())
        .unwrap_or(0) as u32;

    Ok(MediaInfo {
        duration,
        width,
        height,
    })
}

/// Builds the `-filter_complex` graph for a cuts-only, no-transitions export:
/// each timeline item becomes `trim` + `setpts` to reset its clock, then every
/// segment is joined end to end with `concat`. Verified against the installed
/// ffmpeg's own `-h filter=trim|concat|setpts` output, not against memory.
///
/// ponytail: video only (concat `a=0`). No audio track in export yet, matching
/// the web preview's scope; add `atrim`/`asetpts` per segment and `a=1` here
/// when audio matters.
fn filter_complex_for(project: &Project) -> Result<String, FfmpegError> {
    if project.timeline.is_empty() {
        return Err(FfmpegError::EmptyTimeline);
    }

    let mut segments = Vec::with_capacity(project.timeline.len());
    for (index, item) in project.timeline.iter().enumerate() {
        segments.push(format!(
            "[{index}:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS[v{index}]",
            item.in_point, item.out_point
        ));
    }

    let concat_inputs: String = (0..project.timeline.len())
        .map(|index| format!("[v{index}]"))
        .collect();

    Ok(format!(
        "{};{concat_inputs}concat=n={}:v=1:a=0[outv]",
        segments.join(";"),
        project.timeline.len()
    ))
}

/// Renders a project to a single file. Every timeline item's clip must still
/// exist at the path it was imported from; nothing is re-probed at export time.
pub fn export(project: &Project, output_path: &Path) -> Result<(), FfmpegError> {
    let filter_complex = filter_complex_for(project)?;

    let mut command = Command::new("ffmpeg");
    command.arg("-y");
    for item in &project.timeline {
        let clip = project
            .clips
            .iter()
            .find(|clip| clip.id == item.clip_id)
            .ok_or_else(|| FfmpegError::ClipNotFound(item.clip_id.clone()))?;
        command.arg("-i").arg(&clip.path);
    }
    command
        .arg("-filter_complex")
        .arg(filter_complex)
        .arg("-map")
        .arg("[outv]")
        .arg(output_path);

    let output = command
        .output()
        .map_err(|_| FfmpegError::ToolNotFound { tool: "ffmpeg" })?;
    if !output.status.success() {
        return Err(FfmpegError::ExportFailed {
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clip, TimelineItem};
    use std::path::PathBuf;

    fn ffmpeg_available() -> bool {
        Command::new("ffmpeg").arg("-version").output().is_ok()
    }

    fn clip(id: &str, duration: f64) -> Clip {
        Clip {
            id: id.to_string(),
            name: format!("{id}.mp4"),
            path: PathBuf::from(format!("{id}.mp4")),
            duration,
            width: 1920,
            height: 1080,
        }
    }

    #[test]
    fn filter_complex_trims_and_concats_every_segment_in_order() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 5.0));
        project.trim_clip("a", 1.0, 3.0).unwrap();
        project.add_clip(clip("b", 5.0));
        project.trim_clip("b", 0.0, 2.0).unwrap();

        let graph = filter_complex_for(&project).unwrap();

        assert_eq!(
            graph,
            "[0:v]trim=start=1.000:end=3.000,setpts=PTS-STARTPTS[v0];\
             [1:v]trim=start=0.000:end=2.000,setpts=PTS-STARTPTS[v1];\
             [v0][v1]concat=n=2:v=1:a=0[outv]"
        );
    }

    #[test]
    fn filter_complex_rejects_an_empty_timeline() {
        let project = Project::new("empty");
        assert!(matches!(
            filter_complex_for(&project),
            Err(FfmpegError::EmptyTimeline)
        ));
    }

    #[test]
    fn export_reports_a_clip_missing_from_the_pool() {
        let mut project = Project::new("test");
        project.timeline.push(TimelineItem {
            clip_id: "ghost".into(),
            in_point: 0.0,
            out_point: 1.0,
        });

        let err = export(&project, Path::new("out.mp4")).unwrap_err();

        assert!(matches!(err, FfmpegError::ClipNotFound(id) if id == "ghost"));
    }

    #[test]
    fn parses_real_ffprobe_json_for_duration_and_video_dimensions() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg not found on PATH");
            return;
        }
        let dir = std::env::temp_dir();
        let clip_path = dir.join(format!(
            "opencut-core-probe-test-{}.mp4",
            uuid::Uuid::new_v4()
        ));
        let generated = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=64x36:rate=10",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&clip_path)
            .status()
            .expect("ffmpeg must run to generate the fixture");
        assert!(
            generated.success(),
            "ffmpeg failed to generate the test fixture clip"
        );

        let info = probe(&clip_path).unwrap();
        std::fs::remove_file(&clip_path).ok();

        assert!(
            (info.duration - 2.0).abs() < 0.2,
            "expected ~2s, got {}",
            info.duration
        );
        assert_eq!((info.width, info.height), (64, 36));
    }
}
