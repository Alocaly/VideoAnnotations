//! Versioned project files and bounded, gesture-level annotation history.
use crate::{
    annotations::{Annotation, Effect, Effects, Kind},
    project::Project,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_HISTORY: usize = 100;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    project: Project,
}

pub fn validate(project: &Project) -> Result<(), String> {
    let v = &project.video;
    if v.path.as_os_str().is_empty()
        || v.width == 0
        || v.height == 0
        || !v.duration.is_finite()
        || v.duration <= 0.0
        || v.fps.is_some_and(|f| !f.is_finite() || f <= 0.0)
    {
        return Err("Invalid source video metadata.".into());
    }
    if project.annotations.len() > 2000 {
        return Err("Project exceeds 2000 annotations.".into());
    }
    for a in &project.annotations {
        let e = &a.effects;
        if !e.fade_in.is_finite()
            || !e.fade_out.is_finite()
            || !(0.0..=86400.0).contains(&e.fade_in)
            || !(0.0..=86400.0).contains(&e.fade_out)
            || match e.effect {
                Effect::None => false,
                Effect::Glow { pulse_hz, .. } => {
                    !pulse_hz.is_finite() || !(0.05..=10.0).contains(&pulse_hz)
                }
                Effect::Orbit { period, .. } => {
                    !period.is_finite()
                        || !(0.1..=60.0).contains(&period)
                        || !a.kind.supports_orbit()
                }
            }
        {
            return Err("Invalid annotation effects.".into());
        }
        if (a.kind == Kind::Spotlight && e.effect != Effect::None)
            || !a.dimming.is_finite()
            || !(0.0..=1.0).contains(&a.dimming)
            || !a.start_seconds.is_finite()
            || !a.end_seconds.is_finite()
            || a.start_seconds < 0.0
            || a.end_seconds <= a.start_seconds
            || a.end_seconds > v.duration
            || !a.thickness.is_finite()
            || !(1.0..=60.0).contains(&a.thickness)
            || !a.font_size.is_finite()
            || !(6.0..=300.0).contains(&a.font_size)
            || a.name.len() > 128
            || (!a.name.is_empty() && a.name.trim().is_empty())
            || a.name.chars().any(char::is_control)
            || a.text.len() > 65536
        {
            return Err("Invalid annotation timing, style, or text length.".into());
        }
        for p in [a.a, a.b] {
            if !p[0].is_finite()
                || !p[1].is_finite()
                || p[0] < 0.0
                || p[1] < 0.0
                || p[0] > v.width as f32
                || p[1] > v.height as f32
            {
                return Err("Annotation geometry lies outside the video.".into());
            }
        }
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<Project, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open project: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Project exceeds the 8 MiB limit.".into());
    }
    let mut json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid project file: {e}"))?;
    let version = json["version"].as_u64().ok_or("Invalid project version")?;
    if !matches!(version, 1..=5) {
        return Err(format!("Unsupported project version: {version}"));
    }
    if version < 4 {
        migrate_effects(&mut json)?;
    }
    let mut document: Document =
        serde_json::from_value(json).map_err(|e| format!("Invalid project file: {e}"))?;
    validate(&document.project)?;
    if document.project.video.path.is_relative() {
        document.project.video.path = absolute(path)?
            .parent()
            .unwrap()
            .join(&document.project.video.path);
    }
    Ok(document.project)
}

/// Write beside the destination and atomically replace it only after flushing.
pub fn save(project: &Project, path: &Path) -> Result<(), String> {
    validate(project)?;
    let path = absolute(path)?;
    let source = absolute(&project.video.path)?;
    if path == source || (path.exists() && path.canonicalize().ok() == source.canonicalize().ok()) {
        return Err("The project file cannot replace its source video.".into());
    }
    let parent = path.parent().ok_or("Project has no parent directory.")?;
    let mut stored = project.clone();
    stored.video.path = source.strip_prefix(parent).unwrap_or(&source).to_path_buf();
    let bytes = serde_json::to_vec_pretty(&Document {
        version: 5,
        project: stored,
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Project exceeds the 8 MiB limit.".into());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Cannot create project file: {e}"))?;
    temporary
        .write_all(&bytes)
        .map_err(|e| format!("Cannot write project: {e}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| format!("Cannot flush project: {e}"))?;
    temporary
        .persist(&path)
        .map_err(|e| format!("Cannot replace project: {}", e.error))?;
    Ok(())
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LegacyEffects {
    fade_in: f64,
    fade_out: f64,
    movement: [f32; 2],
    glow: f32,
    outline_period: f64,
}

fn migrate_effects(json: &mut serde_json::Value) -> Result<(), String> {
    let width = json["project"]["video"]["width"].as_u64().unwrap_or(0) as f32;
    let height = json["project"]["video"]["height"].as_u64().unwrap_or(0) as f32;
    if let Some(annotations) = json["project"]["annotations"].as_array_mut() {
        for annotation in annotations {
            let Some(stored) = annotation.get("effects") else {
                continue;
            };
            let old: LegacyEffects = serde_json::from_value(stored.clone())
                .map_err(|e| format!("Invalid legacy effects: {e}"))?;
            if !old.glow.is_finite()
                || !(0.0..=30.0).contains(&old.glow)
                || !old.outline_period.is_finite()
                || !(0.0..=60.0).contains(&old.outline_period)
                || (old.outline_period > 0.0 && old.outline_period < 0.1)
                || old.movement.iter().any(|v| !v.is_finite())
                || old.movement[0].abs() > width
                || old.movement[1].abs() > height
            {
                return Err("Invalid legacy annotation effects.".into());
            }
            let color: [u8; 4] =
                serde_json::from_value(annotation["color"].clone()).map_err(|e| e.to_string())?;
            let kind: Kind =
                serde_json::from_value(annotation["kind"].clone()).map_err(|e| e.to_string())?;
            let effect = if old.glow > 0.0 {
                let luminance =
                    0.2126 * color[0] as f32 + 0.7152 * color[1] as f32 + 0.0722 * color[2] as f32;
                let target = if luminance >= 0.75 * 255.0 {
                    0.0
                } else {
                    255.0
                };
                let mix = old.glow / 30.0 * 0.55;
                Effect::Glow {
                    color: std::array::from_fn(|i| {
                        (color[i] as f32 * (1.0 - mix) + target * mix).round() as u8
                    }),
                    pulse_hz: 0.5,
                }
            } else if old.outline_period > 0.0 && matches!(kind, Kind::Rectangle | Kind::Ellipse) {
                Effect::Orbit {
                    color: [color[0], color[1], color[2]],
                    period: old.outline_period,
                }
            } else {
                Effect::None
            };
            annotation["effects"] = serde_json::to_value(Effects {
                fade_in: old.fade_in,
                fade_out: old.fade_out,
                effect,
            })
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    std::path::absolute(path).map_err(|e| e.to_string())
}

/// Snapshots only include annotations, not playback position or GUI selection.
#[derive(Default)]
pub struct History {
    undo: Vec<Vec<Annotation>>,
    redo: Vec<Vec<Annotation>>,
    current: Vec<Annotation>,
    pending: Option<Vec<Annotation>>,
}
impl History {
    pub fn new(current: &[Annotation]) -> Self {
        Self {
            current: current.to_vec(),
            ..Self::default()
        }
    }
    pub fn observe(&mut self, annotations: &[Annotation], editing: bool) {
        if annotations != self.current {
            self.pending.get_or_insert_with(|| self.current.clone());
            self.current = annotations.to_vec();
        }
        if !editing {
            self.flush();
        }
    }
    pub fn flush(&mut self) {
        if let Some(before) = self.pending.take()
            && before != self.current
        {
            self.undo.push(before);
            if self.undo.len() > MAX_HISTORY {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending.as_ref().is_some_and(|p| *p != self.current)
    }
    pub fn can_redo(&self) -> bool {
        self.pending.is_none() && !self.redo.is_empty()
    }
    pub fn undo(&mut self) -> Option<Vec<Annotation>> {
        self.flush();
        let previous = self.undo.pop()?;
        self.redo
            .push(std::mem::replace(&mut self.current, previous));
        Some(self.current.clone())
    }
    pub fn redo(&mut self) -> Option<Vec<Annotation>> {
        self.flush();
        let next = self.redo.pop()?;
        self.undo.push(std::mem::replace(&mut self.current, next));
        Some(self.current.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{annotations::Kind, media::VideoInfo};

    fn fixture(dir: &Path) -> Project {
        let mut p = Project::new(VideoInfo {
            path: dir.join("source.mp4"),
            width: 1920,
            height: 1080,
            duration: 20.0,
            fps: Some(29.97),
        });
        for kind in [
            Kind::Text,
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Arrow,
            Kind::Line,
            Kind::Spotlight,
        ] {
            let mut a =
                Annotation::new(kind, [50.0, 60.0], [400.0, 300.0], 20.0).at_playhead(3.0, 20.0);
            a.text = "étiquette 日本語\nsecond line".into();
            a.color = [20, 30, 40, 125];
            a.thickness = 7.5;
            a.font_size = 48.0;
            p.annotations.push(a);
        }
        p
    }
    #[test]
    fn version_four_defaults_dimming_and_new_settings_are_validated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v4.vannot");
        let mut p = fixture(dir.path());
        p.annotations.truncate(4);
        let mut json = serde_json::to_value(Document {
            version: 4,
            project: p.clone(),
        })
        .unwrap();
        for a in json["project"]["annotations"].as_array_mut().unwrap() {
            a.as_object_mut().unwrap().remove("dimming");
        }
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), p);
        p.annotations[0].kind = Kind::Spotlight;
        p.annotations[0].dimming = 1.1;
        assert!(validate(&p).is_err());
        p.annotations[0].dimming = 0.75;
        p.annotations[0].effects.effect = Effect::Glow {
            color: [0; 3],
            pulse_hz: 1.0,
        };
        assert!(validate(&p).is_err());
        p.annotations[0].kind = Kind::Line;
        p.annotations[0].effects.effect = Effect::Orbit {
            color: [0, 0, 255],
            period: 1.0,
        };
        assert!(validate(&p).is_ok());
    }
    #[test]
    fn roundtrip_and_atomic_replacement_preserve_all_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.vannot");
        let mut p = fixture(dir.path());
        p.annotations[0].effects.fade_in = 1.5;
        p.annotations[0].effects.fade_out = 0.5;
        p.annotations[0].effects.effect = Effect::Glow {
            color: [255, 100, 30],
            pulse_hz: 1.5,
        };
        p.annotations[0].name = "Intro label".into();
        p.annotations[1].effects.effect = Effect::Orbit {
            color: [0, 255, 100],
            period: 2.0,
        };
        save(&p, &path).unwrap();
        assert_eq!(load(&path).unwrap(), p);
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["version"], 5);
        assert_eq!(json["project"]["video"]["path"], "source.mp4");
        p.annotations.reverse();
        p.annotations[0].start_seconds = 1.0;
        save(&p, &path).unwrap();
        assert_eq!(load(&path).unwrap(), p);
    }
    #[test]
    fn legacy_projects_load_with_effects_disabled_and_invalid_effects_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.vannot");
        let mut p = fixture(dir.path());
        let mut json = serde_json::to_value(Document {
            version: 1,
            project: p.clone(),
        })
        .unwrap();
        for a in json["project"]["annotations"].as_array_mut().unwrap() {
            a.as_object_mut().unwrap().remove("effects");
            a.as_object_mut().unwrap().remove("name");
        }
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), p);
        let mut v2 = serde_json::to_value(Document {
            version: 2,
            project: p.clone(),
        })
        .unwrap();
        for annotation in v2["project"]["annotations"].as_array_mut().unwrap() {
            annotation.as_object_mut().unwrap().remove("name");
            annotation["effects"]
                .as_object_mut()
                .unwrap()
                .remove("effect");
        }
        std::fs::write(&path, serde_json::to_vec(&v2).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), p);
        p.annotations[0].effects.effect = Effect::Glow {
            color: [255; 3],
            pulse_hz: f64::NAN,
        };
        assert!(validate(&p).is_err());
        p.annotations[0].effects.effect = Effect::None;
        p.annotations[0].effects.fade_in = -1.0;
        assert!(validate(&p).is_err());
        p.annotations[0].effects.fade_in = 0.0;
        p.annotations[0].name = "   ".into();
        assert!(validate(&p).is_err());
        p.annotations[0].name = "A".repeat(129);
        assert!(validate(&p).is_err());
    }
    #[test]
    fn legacy_effects_migrate_to_one_effect_and_discard_motion() {
        let dir = tempfile::tempdir().unwrap();
        let original = fixture(dir.path());
        let mut json = serde_json::to_value(Document {
            version: 3,
            project: original.clone(),
        })
        .unwrap();
        for a in json["project"]["annotations"].as_array_mut().unwrap() {
            a["effects"] = serde_json::json!({"fade_in": 1.0, "fade_out": 0.5, "movement": [100.0, 20.0], "glow": 0.0, "outline_period": 0.0});
        }
        json["project"]["annotations"][0]["effects"]["glow"] = 30.into();
        json["project"]["annotations"][0]["effects"]["outline_period"] = 2.into();
        json["project"]["annotations"][1]["effects"]["outline_period"] = 3.into();
        let path = dir.path().join("old.vannot");
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let mut loaded = load(&path).unwrap();
        assert_eq!(loaded.annotations[0].a, original.annotations[0].a);
        assert_eq!(loaded.annotations[0].effects.fade_in, 1.0);
        assert!(matches!(
            loaded.annotations[0].effects.effect,
            Effect::Glow { pulse_hz: 0.5, .. }
        ));
        assert_eq!(
            loaded.annotations[1].effects.effect,
            Effect::Orbit {
                color: [20, 30, 40],
                period: 3.0
            }
        );
        loaded.annotations[0].effects.effect = Effect::Orbit {
            color: [255; 3],
            period: 2.0,
        };
        assert!(
            validate(&loaded).is_err(),
            "Text cannot have an orbiting ball"
        );
    }
    #[test]
    fn moved_project_resolves_relative_source_from_new_location() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let old = a.path().join("test.vannot");
        let new = b.path().join("test.vannot");
        save(&fixture(a.path()), &old).unwrap();
        std::fs::copy(old, &new).unwrap();
        let loaded = load(&new).unwrap();
        assert_eq!(loaded.video.path, b.path().join("source.mp4"));
        // Missing media does not prevent loading settings for the relink dialog.
        assert!(!loaded.video.path.exists());
    }
    #[test]
    fn invalid_save_preserves_existing_file_and_cannot_overwrite_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.vannot");
        let mut p = fixture(dir.path());
        save(&p, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        p.annotations[0].end_seconds = -1.0;
        assert!(save(&p, &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        p = fixture(dir.path());
        std::fs::write(&p.video.path, b"source data").unwrap();
        assert!(save(&p, &p.video.path).is_err());
        assert_eq!(std::fs::read(&p.video.path).unwrap(), b"source data");
        assert!(save(&p, &dir.path().join("missing").join("x.vannot")).is_err());
    }
    #[test]
    fn rejects_versions_invalid_geometry_and_truncated_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.vannot");
        let mut p = fixture(dir.path());
        let bytes = serde_json::to_vec(&Document {
            version: 99,
            project: p.clone(),
        })
        .unwrap();
        std::fs::write(&path, bytes).unwrap();
        assert!(load(&path).unwrap_err().contains("version"));
        p.annotations[0].a[0] = f32::NAN;
        assert!(validate(&p).is_err());
        p.annotations[0].a[0] = -1.0;
        assert!(validate(&p).is_err());
        std::fs::write(&path, b"{\"version\":1,").unwrap();
        assert!(load(&path).is_err());
        let file = File::create(&path).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert!(load(&path).unwrap_err().contains("8 MiB"));
    }
    #[test]
    fn history_groups_gestures_and_discards_redo_on_new_edits() {
        let p = fixture(Path::new("."));
        let mut h = History::new(&p.annotations);
        let mut moved = p.annotations.clone();
        moved[0].a[0] = 55.0;
        h.observe(&moved, true);
        moved[0].a[0] = 65.0;
        h.observe(&moved, true);
        h.observe(&moved, false);
        assert_eq!(h.undo().unwrap(), p.annotations);
        assert!(!h.can_undo());
        assert_eq!(h.redo().unwrap(), moved);
        h.undo();
        moved.remove(0);
        h.observe(&moved, false);
        assert!(!h.can_redo());
        assert_eq!(h.undo().unwrap(), p.annotations);
    }
    #[test]
    fn annotation_rename_is_undoable() {
        let original = fixture(Path::new("."));
        let mut history = History::new(&original.annotations);
        let mut renamed = original.annotations.clone();
        renamed[0].name = "Opening title".into();
        history.observe(&renamed, false);
        assert_eq!(history.undo().unwrap(), original.annotations);
        assert_eq!(history.redo().unwrap(), renamed);
    }
    #[test]
    fn cancellation_makes_no_history_and_snapshot_count_is_bounded() {
        let p = fixture(Path::new("."));
        let mut h = History::new(&p.annotations);
        let mut edited = p.annotations.clone();
        edited[0].font_size = 80.0;
        h.observe(&edited, true);
        h.observe(&p.annotations, false);
        assert!(!h.can_undo());
        for n in 0..150 {
            edited[0].text = n.to_string();
            h.observe(&edited, false);
        }
        assert_eq!(h.undo.len(), MAX_HISTORY);
    }
}
