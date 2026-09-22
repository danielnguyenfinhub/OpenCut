use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A piece of imported media. `duration` is seconds and may be non-finite
/// when the container does not declare a length, mirroring the web probe.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
}

impl Clip {
    /// Builds a clip from probed media info, generating a fresh id.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>, info: crate::MediaInfo) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            path: path.into(),
            duration: info.duration,
            width: info.width,
            height: info.height,
        }
    }
}

/// One clip's placement on the timeline: which clip, and which slice of it.
/// Clips play in the order they appear in `Project::timeline`; there is no
/// separate position field because reordering the list is reordering the cut.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct TimelineItem {
    pub clip_id: String,
    pub in_point: f64,
    pub out_point: f64,
}

impl TimelineItem {
    fn duration(&self) -> f64 {
        (self.out_point - self.in_point).max(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Project {
    pub name: String,
    pub clips: Vec<Clip>,
    pub timeline: Vec<TimelineItem>,
}

#[derive(Debug)]
pub enum ProjectError {
    ClipNotFound(String),
    /// end must be strictly after start, and both must fall within the clip's known duration.
    InvalidTrim {
        clip_id: String,
        in_point: f64,
        out_point: f64,
        clip_duration: f64,
    },
    IndexOutOfBounds {
        index: usize,
        len: usize,
    },
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClipNotFound(id) => write!(f, "no clip with id {id} in this project"),
            Self::InvalidTrim {
                clip_id,
                in_point,
                out_point,
                clip_duration,
            } => write!(
                f,
                "invalid trim for clip {clip_id}: in {in_point} must be less than out {out_point}, \
                 and out must not exceed the clip duration of {clip_duration}"
            ),
            Self::IndexOutOfBounds { index, len } => {
                write!(
                    f,
                    "index {index} is out of bounds for a timeline of {len} item(s)"
                )
            }
            Self::Io(err) => write!(f, "could not read or write the project file: {err}"),
            Self::Json(err) => write!(f, "project file is not valid JSON: {err}"),
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<std::io::Error> for ProjectError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for ProjectError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            clips: Vec::new(),
            timeline: Vec::new(),
        }
    }

    /// Adds a clip to the media pool and appends it to the end of the timeline,
    /// using the clip's full known length as the initial trim.
    pub fn add_clip(&mut self, clip: Clip) {
        let full_length = if clip.duration.is_finite() {
            clip.duration
        } else {
            0.0
        };
        self.timeline.push(TimelineItem {
            clip_id: clip.id.clone(),
            in_point: 0.0,
            out_point: full_length,
        });
        self.clips.push(clip);
    }

    fn clip(&self, clip_id: &str) -> Result<&Clip, ProjectError> {
        self.clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| ProjectError::ClipNotFound(clip_id.to_string()))
    }

    /// Changes the in/out points for every timeline item that references this clip.
    /// Rejects a trim that is empty or runs past what the clip actually contains.
    pub fn trim_clip(
        &mut self,
        clip_id: &str,
        in_point: f64,
        out_point: f64,
    ) -> Result<(), ProjectError> {
        let clip = self.clip(clip_id)?;
        let clip_duration = clip.duration;
        let within_bounds = !clip_duration.is_finite() || out_point <= clip_duration;
        if !(in_point >= 0.0 && in_point < out_point && within_bounds) {
            return Err(ProjectError::InvalidTrim {
                clip_id: clip_id.to_string(),
                in_point,
                out_point,
                clip_duration,
            });
        }
        for item in self
            .timeline
            .iter_mut()
            .filter(|item| item.clip_id == clip_id)
        {
            item.in_point = in_point;
            item.out_point = out_point;
        }
        Ok(())
    }

    /// Removes a clip from the pool and every timeline item that used it.
    pub fn remove_clip(&mut self, clip_id: &str) -> Result<(), ProjectError> {
        self.clip(clip_id)?;
        self.clips.retain(|clip| clip.id != clip_id);
        self.timeline.retain(|item| item.clip_id != clip_id);
        Ok(())
    }

    /// Moves the timeline item at `from` to sit at `to`, shifting the rest.
    /// Operates on timeline position, not clip id, since the same clip may
    /// appear more than once on the timeline.
    pub fn reorder(&mut self, from: usize, to: usize) -> Result<(), ProjectError> {
        let len = self.timeline.len();
        if from >= len {
            return Err(ProjectError::IndexOutOfBounds { index: from, len });
        }
        if to >= len {
            return Err(ProjectError::IndexOutOfBounds { index: to, len });
        }
        let item = self.timeline.remove(from);
        self.timeline.insert(to, item);
        Ok(())
    }

    /// Sum of every timeline item's trimmed length: what the exported file's length will be.
    pub fn total_duration(&self) -> f64 {
        self.timeline.iter().map(TimelineItem::duration).sum()
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn add_clip_appends_a_full_length_timeline_item() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 4.0));

        assert_eq!(project.clips.len(), 1);
        assert_eq!(
            project.timeline,
            vec![TimelineItem {
                clip_id: "a".into(),
                in_point: 0.0,
                out_point: 4.0
            }]
        );
    }

    #[test]
    fn add_clip_with_unknown_duration_gets_a_zero_length_slot() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", f64::INFINITY));

        assert_eq!(project.timeline[0].out_point, 0.0);
    }

    #[test]
    fn trim_clip_updates_every_timeline_item_that_uses_it() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 10.0));
        project.timeline.push(TimelineItem {
            clip_id: "a".into(),
            in_point: 0.0,
            out_point: 10.0,
        });

        project.trim_clip("a", 1.0, 3.0).unwrap();

        assert!(
            project
                .timeline
                .iter()
                .all(|item| item.in_point == 1.0 && item.out_point == 3.0)
        );
    }

    #[test]
    fn trim_clip_rejects_an_out_point_past_the_clip_duration() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 5.0));

        let err = project.trim_clip("a", 0.0, 6.0).unwrap_err();

        assert!(matches!(err, ProjectError::InvalidTrim { .. }));
    }

    #[test]
    fn trim_clip_rejects_an_empty_or_inverted_range() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 5.0));

        assert!(project.trim_clip("a", 2.0, 2.0).is_err());
        assert!(project.trim_clip("a", 3.0, 1.0).is_err());
    }

    #[test]
    fn trim_clip_reports_a_missing_clip_by_id() {
        let mut project = Project::new("test");
        assert!(matches!(
            project.trim_clip("missing", 0.0, 1.0),
            Err(ProjectError::ClipNotFound(id)) if id == "missing"
        ));
    }

    #[test]
    fn remove_clip_drops_it_from_the_pool_and_the_timeline() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 3.0));
        project.add_clip(clip("b", 2.0));

        project.remove_clip("a").unwrap();

        assert_eq!(project.clips.len(), 1);
        assert!(project.timeline.iter().all(|item| item.clip_id != "a"));
    }

    #[test]
    fn reorder_moves_an_item_to_a_new_position() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 1.0));
        project.add_clip(clip("b", 1.0));
        project.add_clip(clip("c", 1.0));

        project.reorder(0, 2).unwrap();

        let order: Vec<&str> = project
            .timeline
            .iter()
            .map(|item| item.clip_id.as_str())
            .collect();
        assert_eq!(order, vec!["b", "c", "a"]);
    }

    #[test]
    fn reorder_rejects_an_out_of_range_index() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 1.0));

        assert!(matches!(
            project.reorder(0, 5),
            Err(ProjectError::IndexOutOfBounds { .. })
        ));
    }

    #[test]
    fn total_duration_sums_trimmed_lengths_not_full_clip_lengths() {
        let mut project = Project::new("test");
        project.add_clip(clip("a", 10.0));
        project.trim_clip("a", 2.0, 5.0).unwrap();
        project.add_clip(clip("b", 4.0));

        assert_eq!(project.total_duration(), 3.0 + 4.0);
    }

    #[test]
    fn save_and_load_round_trip_through_json() {
        let mut project = Project::new("round trip");
        project.add_clip(clip("a", 3.5));
        let path =
            std::env::temp_dir().join(format!("opencut-core-test-{}.json", uuid::Uuid::new_v4()));

        project.save(&path).unwrap();
        let loaded = Project::load(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded, project);
    }

    #[test]
    fn load_reports_a_readable_error_for_a_missing_file() {
        let path = std::env::temp_dir().join("opencut-core-definitely-does-not-exist.json");
        assert!(matches!(Project::load(&path), Err(ProjectError::Io(_))));
    }
}
