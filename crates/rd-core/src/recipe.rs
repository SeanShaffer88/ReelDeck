//! The restoration recipe: the ordered, human-readable record of every
//! decision made about a source file.
//!
//! A recipe plus the pinned tool versions in its export log must be enough
//! to re-run a job and reproduce it. Recipes serialize with serde.
//!
//! Format note: v0.1 serializes recipes as **JSON** (via `serde_json`).
//! A YAML surface syntax is planned for hand-editing once the schema
//! settles; the in-memory model will not change when it lands.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hash::is_sha256_hex;

/// The recipe schema version this build writes. Schema 2 adds edit
/// fields (kept ranges, the stream-copy intent); schema-1 recipes still
/// load and re-run unchanged.
pub const RECIPE_SCHEMA_VERSION: u32 = 2;

/// The oldest recipe schema version this build reads.
pub const MIN_RECIPE_SCHEMA_VERSION: u32 = 1;

/// Why a recipe is not safe to run.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RecipeError {
    #[error("recipe has no operations and keeps no ranges; a recipe must do at least one thing, explicitly")]
    EmptyOperations,
    #[error("recipe source name is empty")]
    EmptySourceName,
    #[error(
        "unsupported recipe schema version {0}; this build understands {MIN_RECIPE_SCHEMA_VERSION} to {RECIPE_SCHEMA_VERSION}"
    )]
    UnsupportedSchemaVersion(u32),
    #[error(
        "this recipe uses {0}, which needs recipe schema 2; a schema-1 recipe cannot carry it"
    )]
    NeedsSchema2(&'static str),
    #[error("kept range {index} is invalid: {detail}")]
    BadRange { index: usize, detail: String },
    #[error("a stream-copy export copies the streams untouched, so it cannot apply operations; remove them or choose an intent that re-encodes")]
    StreamCopyWithOperations,
    #[error("source SHA-256 is not a 64-character hex string: {0}")]
    BadSourceHash(String),
    #[error("denoise {field} strength {value} is outside the allowed range 0.0..=20.0")]
    DenoiseOutOfRange { field: &'static str, value: f32 },
    #[error("stabilize smoothing {0} is outside the allowed range 0..=100")]
    SmoothingOutOfRange(u32),
    #[error("stabilize zoom {0}% is outside the allowed range 0.0..=50.0")]
    ZoomOutOfRange(f32),
    #[error("finish scale dimensions must both be present and greater than zero")]
    BadScale,
    #[error("crop rectangle must have non-zero width and height")]
    BadCrop,
}

/// Field order of interlaced material, as confirmed by the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldOrder {
    /// Detect from the stream flags; the inspection report says what was found.
    Auto,
    TopFieldFirst,
    BottomFieldFirst,
}

impl FieldOrder {
    /// The FFmpeg `parity` value for this field order.
    pub fn ffmpeg_parity(self) -> &'static str {
        match self {
            FieldOrder::Auto => "auto",
            FieldOrder::TopFieldFirst => "tff",
            FieldOrder::BottomFieldFirst => "bff",
        }
    }
}

/// Whether a deinterlacer emits one frame per frame or one frame per field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputRate {
    /// One output frame per input frame (25i -> 25p).
    SendFrame,
    /// One output frame per input field (25i -> 50p); preserves motion.
    SendField,
}

impl OutputRate {
    /// The FFmpeg `mode` value for this output rate.
    pub fn ffmpeg_mode(self) -> &'static str {
        match self {
            OutputRate::SendFrame => "send_frame",
            OutputRate::SendField => "send_field",
        }
    }
}

/// The deinterlacing engine. `Qtgmc` runs through the VapourSynth pipeline;
/// the FFmpeg-only pipeline reports it as unavailable rather than silently
/// substituting another engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeinterlaceEngine {
    Bwdif,
    Yadif,
    Qtgmc,
}

/// The denoising engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenoiseEngine {
    Hqdn3d,
    Nlmeans,
}

/// A crop rectangle in pixels, from the top-left of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// One step in a restoration chain. Order in the recipe is render order;
/// reordering operations is a different recipe and produces a different log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    Deinterlace {
        engine: DeinterlaceEngine,
        field_order: FieldOrder,
        output_rate: OutputRate,
        /// Deinterlace only frames flagged interlaced, leaving progressive
        /// frames untouched.
        #[serde(default)]
        only_flagged: bool,
    },
    Stabilize {
        /// vidstab smoothing window, in frames (0..=100).
        smoothing: u32,
        /// Extra zoom in percent (0.0..=50.0); hides stabilization borders
        /// at the cost of picture area.
        zoom_percent: f32,
        /// Virtual-tripod mode: hold the camera still instead of smoothing.
        tripod: bool,
    },
    Denoise {
        engine: DenoiseEngine,
        /// Luma strength, 0.0..=20.0.
        luma: f32,
        /// Chroma strength, 0.0..=20.0.
        chroma: f32,
        /// Temporal strength, 0.0..=20.0. High values smear motion and
        /// erase film grain first; the defaults are deliberately conservative.
        temporal: f32,
    },
    Finish {
        #[serde(default)]
        crop: Option<CropRect>,
        #[serde(default)]
        scale_width: Option<u32>,
        #[serde(default)]
        scale_height: Option<u32>,
        /// Reduce frame-to-frame exposure flicker (film transfers).
        #[serde(default)]
        deflicker: bool,
        /// Gently pull out-of-range levels back into broadcast range.
        #[serde(default)]
        normalize_levels: bool,
    },
}

impl Operation {
    /// A one-line, operator-facing summary, e.g. for the chain panel:
    /// `Deinterlace · bwdif · tff · send_field`.
    pub fn summary(&self) -> String {
        match self {
            Operation::Deinterlace {
                engine,
                field_order,
                output_rate,
                ..
            } => {
                let engine = match engine {
                    DeinterlaceEngine::Bwdif => "bwdif",
                    DeinterlaceEngine::Yadif => "yadif",
                    DeinterlaceEngine::Qtgmc => "qtgmc",
                };
                format!(
                    "Deinterlace · {engine} · {} · {}",
                    field_order.ffmpeg_parity(),
                    output_rate.ffmpeg_mode()
                )
            }
            Operation::Stabilize {
                smoothing,
                zoom_percent,
                tripod,
            } => {
                if *tripod {
                    "Stabilize · tripod".to_string()
                } else {
                    format!("Stabilize · smoothing {smoothing} · zoom {zoom_percent:.1}%")
                }
            }
            Operation::Denoise {
                engine,
                luma,
                chroma,
                temporal,
            } => {
                let engine = match engine {
                    DenoiseEngine::Hqdn3d => "hqdn3d",
                    DenoiseEngine::Nlmeans => "nlmeans",
                };
                format!("Denoise · {engine} · luma {luma} · chroma {chroma} · temporal {temporal}")
            }
            Operation::Finish { .. } => "Finish · crop/scale/levels".to_string(),
        }
    }

    fn validate(&self) -> Result<(), RecipeError> {
        match self {
            Operation::Deinterlace { .. } => Ok(()),
            Operation::Stabilize {
                smoothing,
                zoom_percent,
                ..
            } => {
                if *smoothing > 100 {
                    return Err(RecipeError::SmoothingOutOfRange(*smoothing));
                }
                if !zoom_percent.is_finite() || !(0.0..=50.0).contains(zoom_percent) {
                    return Err(RecipeError::ZoomOutOfRange(*zoom_percent));
                }
                Ok(())
            }
            Operation::Denoise {
                luma,
                chroma,
                temporal,
                ..
            } => {
                for (field, value) in [
                    ("luma", *luma),
                    ("chroma", *chroma),
                    ("temporal", *temporal),
                ] {
                    if !value.is_finite() || !(0.0..=20.0).contains(&value) {
                        return Err(RecipeError::DenoiseOutOfRange { field, value });
                    }
                }
                Ok(())
            }
            Operation::Finish {
                crop,
                scale_width,
                scale_height,
                ..
            } => {
                if let Some(c) = crop {
                    if c.width == 0 || c.height == 0 {
                        return Err(RecipeError::BadCrop);
                    }
                }
                match (scale_width, scale_height) {
                    (Some(w), Some(h)) if *w > 0 && *h > 0 => Ok(()),
                    (None, None) => Ok(()),
                    _ => Err(RecipeError::BadScale),
                }
            }
        }
    }
}

/// The export intent. The intents are never interchangeable: a
/// Preservation master is not a delivery file and vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportPreset {
    /// FFV1 v3 in Matroska, FLAC audio: lossless, open, per-frame CRCs.
    PreservationMaster,
    /// ProRes 422 HQ in QuickTime: drops onto an editing timeline.
    Mezzanine,
    /// H.264 in MP4, CRF-based: for viewing and sharing, never a master.
    Access,
    /// Streams copied untouched into a new container (schema 2): a
    /// lossless cut or remux. Nothing is decoded, so no operation can
    /// apply, and a cut can only start on a keyframe. Not a
    /// preservation master: the codecs are whatever the source used.
    StreamCopy,
}

impl ExportPreset {
    /// File extension (without dot) for outputs of this preset.
    pub fn extension(self) -> &'static str {
        match self {
            ExportPreset::PreservationMaster => "mkv",
            ExportPreset::Mezzanine => "mov",
            ExportPreset::Access => "mp4",
            // Matroska holds nearly any codec the source might carry.
            ExportPreset::StreamCopy => "mkv",
        }
    }
}

/// A complete, re-runnable restoration recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub schema_version: u32,
    /// Source file name (not a path the tool may write to; sources are
    /// never modified or overwritten).
    pub source_name: String,
    /// SHA-256 of the source at ingest, when fixity has been computed.
    #[serde(default)]
    pub source_sha256: Option<String>,
    /// Ordered operations; render order is list order.
    pub operations: Vec<Operation>,
    pub export_preset: ExportPreset,
    /// Free-text operator notes; carried into the export log.
    #[serde(default)]
    pub notes: String,
    /// Ranges of the source to keep, in order (schema 2). Empty keeps the
    /// whole source. One range is a trim; several remove the parts
    /// between them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keep: Vec<KeepRange>,
}

/// A range of the source to keep, in seconds from its start.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeepRange {
    pub start_secs: f64,
    /// End of the range; `None` keeps to the end of the source.
    #[serde(default)]
    pub end_secs: Option<f64>,
}

impl KeepRange {
    pub fn new(start_secs: f64, end_secs: Option<f64>) -> Self {
        KeepRange {
            start_secs,
            end_secs,
        }
    }

    /// Length in seconds, when the range has an end.
    pub fn duration_secs(&self) -> Option<f64> {
        self.end_secs.map(|end| end - self.start_secs)
    }
}

/// Parse a time as seconds (`90`, `90.5`) or timecode-style clock time
/// (`1:30`, `01:01:30.250`). Returns `None` for anything else, including
/// negative values and minute/second fields of 60 or more.
pub fn parse_time(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut secs = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        let value: f64 = if last {
            part.parse().ok()?
        } else {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()?
        };
        if !value.is_finite() || value < 0.0 || part.starts_with('+') {
            return None;
        }
        if i > 0 && value >= 60.0 {
            return None;
        }
        secs = secs * 60.0 + value;
    }
    Some(secs)
}

impl Recipe {
    pub fn new(source_name: impl Into<String>, export_preset: ExportPreset) -> Self {
        Recipe {
            schema_version: RECIPE_SCHEMA_VERSION,
            source_name: source_name.into(),
            source_sha256: None,
            operations: Vec::new(),
            export_preset,
            notes: String::new(),
            keep: Vec::new(),
        }
    }

    /// Keep a range of the source (see [`Recipe::keep`]).
    pub fn keep_range(mut self, start_secs: f64, end_secs: Option<f64>) -> Self {
        self.keep.push(KeepRange::new(start_secs, end_secs));
        self
    }

    /// True when the recipe uses a field only schema 2 can carry.
    pub fn uses_schema2(&self) -> bool {
        !self.keep.is_empty() || self.export_preset == ExportPreset::StreamCopy
    }

    pub fn push(mut self, op: Operation) -> Self {
        self.operations.push(op);
        self
    }

    /// Validate the whole recipe before any planning or rendering.
    pub fn validate(&self) -> Result<(), RecipeError> {
        if !(MIN_RECIPE_SCHEMA_VERSION..=RECIPE_SCHEMA_VERSION).contains(&self.schema_version) {
            return Err(RecipeError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.schema_version < 2 {
            if !self.keep.is_empty() {
                return Err(RecipeError::NeedsSchema2("kept ranges"));
            }
            if self.export_preset == ExportPreset::StreamCopy {
                return Err(RecipeError::NeedsSchema2("the stream-copy intent"));
            }
        }
        if self.source_name.trim().is_empty() {
            return Err(RecipeError::EmptySourceName);
        }
        if let Some(hash) = &self.source_sha256 {
            if !is_sha256_hex(hash) {
                return Err(RecipeError::BadSourceHash(hash.clone()));
            }
        }
        // A stream copy changes the container even with nothing else to
        // do, so it counts as an explicit action.
        if self.operations.is_empty()
            && self.keep.is_empty()
            && self.export_preset != ExportPreset::StreamCopy
        {
            return Err(RecipeError::EmptyOperations);
        }
        if self.export_preset == ExportPreset::StreamCopy && !self.operations.is_empty() {
            return Err(RecipeError::StreamCopyWithOperations);
        }
        for op in &self.operations {
            op.validate()?;
        }
        self.validate_keep()
    }

    /// Kept ranges must be finite, start at or after 0, have positive
    /// length, and run in order without overlapping. Only the last may
    /// be open-ended.
    fn validate_keep(&self) -> Result<(), RecipeError> {
        let mut previous_end = 0.0_f64;
        for (i, range) in self.keep.iter().enumerate() {
            let index = i + 1;
            let bad = |detail: String| RecipeError::BadRange { index, detail };
            if !range.start_secs.is_finite() || range.start_secs < 0.0 {
                return Err(bad(format!(
                    "start {} must be a finite, non-negative number of seconds",
                    range.start_secs
                )));
            }
            if i > 0 && range.start_secs < previous_end {
                return Err(bad(format!(
                    "starts at {}s, before the previous range ends at {previous_end}s; ranges must be in order and not overlap",
                    range.start_secs
                )));
            }
            match range.end_secs {
                Some(end) => {
                    if !end.is_finite() || end <= range.start_secs {
                        return Err(bad(format!(
                            "end {end} must be a finite number greater than its start {}",
                            range.start_secs
                        )));
                    }
                    previous_end = end;
                }
                None if i + 1 < self.keep.len() => {
                    return Err(bad(
                        "only the last range may run to the end of the source".to_string()
                    ));
                }
                None => {}
            }
        }
        Ok(())
    }

    /// True if the chain needs the two-pass stabilization analysis.
    pub fn needs_stabilization_pass(&self) -> bool {
        self.operations
            .iter()
            .any(|op| matches!(op, Operation::Stabilize { .. }))
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_recipe() -> Recipe {
        Recipe::new("capture_1987.mkv", ExportPreset::PreservationMaster)
            .push(Operation::Deinterlace {
                engine: DeinterlaceEngine::Bwdif,
                field_order: FieldOrder::TopFieldFirst,
                output_rate: OutputRate::SendField,
                only_flagged: false,
            })
            .push(Operation::Denoise {
                engine: DenoiseEngine::Hqdn3d,
                luma: 4.0,
                chroma: 3.0,
                temporal: 6.0,
            })
    }

    #[test]
    fn json_round_trip_preserves_recipe() {
        let recipe = sample_recipe();
        let json = recipe.to_json().expect("serialize");
        let back = Recipe::from_json(&json).expect("deserialize");
        assert_eq!(recipe, back);
    }

    #[test]
    fn sample_recipe_validates() {
        assert!(sample_recipe().validate().is_ok());
    }

    #[test]
    fn empty_operation_list_is_rejected() {
        let recipe = Recipe::new("x.mkv", ExportPreset::Access);
        assert_eq!(recipe.validate(), Err(RecipeError::EmptyOperations));
    }

    #[test]
    fn out_of_range_denoise_strength_is_rejected() {
        let recipe = Recipe::new("x.mkv", ExportPreset::Access).push(Operation::Denoise {
            engine: DenoiseEngine::Nlmeans,
            luma: 25.0,
            chroma: 1.0,
            temporal: 1.0,
        });
        match recipe.validate() {
            Err(RecipeError::DenoiseOutOfRange { field, value }) => {
                assert_eq!(field, "luma");
                assert_eq!(value, 25.0);
            }
            other => panic!("expected DenoiseOutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn nan_denoise_strength_is_rejected() {
        let recipe = Recipe::new("x.mkv", ExportPreset::Access).push(Operation::Denoise {
            engine: DenoiseEngine::Hqdn3d,
            luma: f32::NAN,
            chroma: 1.0,
            temporal: 1.0,
        });
        assert!(matches!(
            recipe.validate(),
            Err(RecipeError::DenoiseOutOfRange { .. })
        ));
    }

    #[test]
    fn bad_source_hash_is_rejected() {
        let mut recipe = sample_recipe();
        recipe.source_sha256 = Some("not-a-hash".to_string());
        assert!(matches!(
            recipe.validate(),
            Err(RecipeError::BadSourceHash(_))
        ));
    }

    #[test]
    fn schema1_recipes_still_load_and_validate() {
        let mut recipe = sample_recipe();
        recipe.schema_version = 1;
        let json = recipe.to_json().expect("serialize");
        assert!(
            !json.contains("keep"),
            "an unused edit field is not written"
        );
        let back = Recipe::from_json(&json).expect("deserialize");
        assert!(back.validate().is_ok());
        assert!(!back.uses_schema2());
    }

    #[test]
    fn edit_fields_need_schema2() {
        let mut trimmed = sample_recipe().keep_range(10.0, Some(20.0));
        assert!(trimmed.validate().is_ok());
        assert!(trimmed.uses_schema2());
        trimmed.schema_version = 1;
        assert_eq!(
            trimmed.validate(),
            Err(RecipeError::NeedsSchema2("kept ranges"))
        );
        let mut copy = Recipe::new("x.mkv", ExportPreset::StreamCopy);
        copy.schema_version = 1;
        assert_eq!(
            copy.validate(),
            Err(RecipeError::NeedsSchema2("the stream-copy intent"))
        );
        let mut future = sample_recipe();
        future.schema_version = 3;
        assert_eq!(
            future.validate(),
            Err(RecipeError::UnsupportedSchemaVersion(3))
        );
    }

    #[test]
    fn a_trim_alone_is_a_valid_recipe_and_round_trips() {
        let recipe = Recipe::new("x.mkv", ExportPreset::Access)
            .keep_range(5.0, Some(9.5))
            .keep_range(12.0, None);
        assert!(recipe.validate().is_ok());
        let back = Recipe::from_json(&recipe.to_json().unwrap()).unwrap();
        assert_eq!(back, recipe);
        assert_eq!(back.keep[0].duration_secs(), Some(4.5));
        assert_eq!(back.keep[1].duration_secs(), None);
    }

    #[test]
    fn stream_copy_rules() {
        assert!(Recipe::new("x.mp4", ExportPreset::StreamCopy)
            .validate()
            .is_ok());
        assert!(Recipe::new("x.mp4", ExportPreset::StreamCopy)
            .keep_range(1.0, Some(2.0))
            .validate()
            .is_ok());
        let mut with_ops = sample_recipe();
        with_ops.export_preset = ExportPreset::StreamCopy;
        assert_eq!(
            with_ops.validate(),
            Err(RecipeError::StreamCopyWithOperations)
        );
        assert_eq!(ExportPreset::StreamCopy.extension(), "mkv");
    }

    #[test]
    fn kept_ranges_are_checked() {
        let bad = |recipe: Recipe| match recipe.validate() {
            Err(RecipeError::BadRange { index, detail }) => (index, detail),
            other => panic!("expected BadRange, got {other:?}"),
        };
        let base = || Recipe::new("x.mkv", ExportPreset::Access);
        assert_eq!(bad(base().keep_range(-1.0, Some(2.0))).0, 1);
        assert_eq!(bad(base().keep_range(5.0, Some(5.0))).0, 1);
        assert_eq!(bad(base().keep_range(f64::NAN, None)).0, 1);
        let (index, detail) = bad(base()
            .keep_range(0.0, Some(10.0))
            .keep_range(9.0, Some(12.0)));
        assert_eq!(index, 2);
        assert!(detail.contains("in order"), "{detail}");
        let (index, detail) = bad(base().keep_range(0.0, None).keep_range(20.0, Some(30.0)));
        assert_eq!(index, 1);
        assert!(detail.contains("only the last"), "{detail}");
        assert!(
            base()
                .keep_range(0.0, Some(10.0))
                .keep_range(10.0, Some(12.0))
                .validate()
                .is_ok(),
            "touching ranges are allowed"
        );
    }

    #[test]
    fn times_parse_as_seconds_or_clock_time() {
        assert_eq!(parse_time("90"), Some(90.0));
        assert_eq!(parse_time(" 90.5 "), Some(90.5));
        assert_eq!(parse_time("1:30"), Some(90.0));
        assert_eq!(parse_time("01:01:30.250"), Some(3690.25));
        for bad in [
            "", "-5", "1:60", "1:2:3:4", "a", "1::2", "+3", "1:-3", "inf", "NaN",
        ] {
            assert_eq!(parse_time(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn preset_extensions() {
        assert_eq!(ExportPreset::PreservationMaster.extension(), "mkv");
        assert_eq!(ExportPreset::Mezzanine.extension(), "mov");
        assert_eq!(ExportPreset::Access.extension(), "mp4");
    }
}
