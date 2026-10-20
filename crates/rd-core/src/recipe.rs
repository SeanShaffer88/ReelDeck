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

/// The current recipe schema version.
pub const RECIPE_SCHEMA_VERSION: u32 = 1;

/// Why a recipe is not safe to run.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RecipeError {
    #[error("recipe has no operations; a restoration must do at least one thing, explicitly")]
    EmptyOperations,
    #[error("recipe source name is empty")]
    EmptySourceName,
    #[error(
        "unsupported recipe schema version {0}; this build understands {RECIPE_SCHEMA_VERSION}"
    )]
    UnsupportedSchemaVersion(u32),
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

/// The export intent. The three intents are never interchangeable: a
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
}

impl ExportPreset {
    /// File extension (without dot) for outputs of this preset.
    pub fn extension(self) -> &'static str {
        match self {
            ExportPreset::PreservationMaster => "mkv",
            ExportPreset::Mezzanine => "mov",
            ExportPreset::Access => "mp4",
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
        }
    }

    pub fn push(mut self, op: Operation) -> Self {
        self.operations.push(op);
        self
    }

    /// Validate the whole recipe before any planning or rendering.
    pub fn validate(&self) -> Result<(), RecipeError> {
        if self.schema_version != RECIPE_SCHEMA_VERSION {
            return Err(RecipeError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.source_name.trim().is_empty() {
            return Err(RecipeError::EmptySourceName);
        }
        if let Some(hash) = &self.source_sha256 {
            if !is_sha256_hex(hash) {
                return Err(RecipeError::BadSourceHash(hash.clone()));
            }
        }
        if self.operations.is_empty() {
            return Err(RecipeError::EmptyOperations);
        }
        for op in &self.operations {
            op.validate()?;
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
    fn preset_extensions() {
        assert_eq!(ExportPreset::PreservationMaster.extension(), "mkv");
        assert_eq!(ExportPreset::Mezzanine.extension(), "mov");
        assert_eq!(ExportPreset::Access.extension(), "mp4");
    }
}
