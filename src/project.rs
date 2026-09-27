use crate::{annotations::Annotation, media::VideoInfo};

/// A project owns exactly one source video, with annotations ordered back to front.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub video: VideoInfo,
    pub annotations: Vec<Annotation>,
}

impl Project {
    pub fn new(video: VideoInfo) -> Self {
        Self {
            video,
            annotations: Vec::new(),
        }
    }
}
