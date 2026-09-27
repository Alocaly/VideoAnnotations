use crate::{annotations::Annotation, media::VideoInfo};

/// A project owns exactly one source video. Persistence arrives in step 5.
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
