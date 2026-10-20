//! The export log: the proof that ships with every output.
//!
//! Where the recipe records *intent*, the log records *what actually
//! happened*: the exact argument vector that ran, tool versions, frame
//! counts, warnings, and the SHA-256 of both the source and the output.

use serde::{Deserialize, Serialize};

use crate::recipe::ExportPreset;

/// One export run, as recorded at render time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportLog {
    /// Recipe schema version the run was planned from.
    pub recipe_schema_version: u32,
    pub source_name: String,
    /// SHA-256 of the source, computed at ingest.
    pub source_sha256: String,
    pub output_name: String,
    /// SHA-256 of the finished output; `None` while a run is in progress
    /// or after a failed run.
    #[serde(default)]
    pub output_sha256: Option<String>,
    /// Hex SHA-256 of the concatenated FFmpeg streamhash values for all
    /// streams of the output — the per-stream SHA-256 values from a
    /// stream-copy streamhash pass, in stream order, joined and hashed
    /// by `rd_run::parse_streamhash_digest` — computed after a
    /// successful render. Unlike [`Self::output_sha256`] this digest
    /// covers only the elementary streams, not the container, so two
    /// runs of the same recipe can be compared at stream level even
    /// when the container bytes differ (a Matroska muxer writes a
    /// per-run Segment UUID and DateUTC). `None` on failed runs, when
    /// the streamhash pass itself could not be completed (the log's
    /// warnings then say so), and in logs written before this field
    /// existed.
    #[serde(default)]
    pub output_stream_sha256: Option<String>,
    pub preset: ExportPreset,
    /// Version string of the FFmpeg build that ran, when known.
    #[serde(default)]
    pub ffmpeg_version: Option<String>,
    /// Version string of the VapourSynth pipeline, when it was used.
    #[serde(default)]
    pub vapoursynth_version: Option<String>,
    /// The exact argument vector executed (program name first).
    pub command: Vec<String>,
    /// ISO-8601 start timestamp, recorded by the caller (rd-core itself
    /// never reads a clock, so logs stay testable).
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
    pub frames_processed: u64,
    pub frames_dropped: u64,
    pub frames_duplicated: u64,
    /// Every warning the render emitted, verbatim.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Operator notes copied from the recipe.
    #[serde(default)]
    pub notes: String,
    pub success: bool,
}

impl ExportLog {
    pub fn new(
        recipe_schema_version: u32,
        source_name: impl Into<String>,
        source_sha256: impl Into<String>,
        output_name: impl Into<String>,
        preset: ExportPreset,
        command: Vec<String>,
        started_at: impl Into<String>,
    ) -> Self {
        ExportLog {
            recipe_schema_version,
            source_name: source_name.into(),
            source_sha256: source_sha256.into(),
            output_name: output_name.into(),
            output_sha256: None,
            output_stream_sha256: None,
            preset,
            ffmpeg_version: None,
            vapoursynth_version: None,
            command,
            started_at: started_at.into(),
            finished_at: None,
            frames_processed: 0,
            frames_dropped: 0,
            frames_duplicated: 0,
            warnings: Vec::new(),
            notes: String::new(),
            success: false,
        }
    }

    /// Mark the run finished. `output_sha256` is required for a successful
    /// run to be verifiable later; callers must hash the output first.
    pub fn finish(
        &mut self,
        finished_at: impl Into<String>,
        output_sha256: Option<String>,
        success: bool,
    ) {
        self.finished_at = Some(finished_at.into());
        self.output_sha256 = output_sha256;
        self.success = success;
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

    #[test]
    fn export_log_json_round_trip() {
        let mut log = ExportLog::new(
            1,
            "capture_1987.mkv",
            "a".repeat(64),
            "capture_1987.restored.mkv",
            ExportPreset::PreservationMaster,
            vec!["ffmpeg".into(), "-i".into(), "capture_1987.mkv".into()],
            "2026-10-20T03:00:00Z",
        );
        log.frames_processed = 14_400;
        log.finish("2026-10-20T03:12:00Z", Some("b".repeat(64)), true);
        let json = log.to_json().expect("serialize");
        let back = ExportLog::from_json(&json).expect("deserialize");
        assert_eq!(log, back);
        assert!(back.success);
        assert_eq!(back.frames_processed, 14_400);
    }

    fn sample_log() -> ExportLog {
        let mut log = ExportLog::new(
            1,
            "capture_1987.mkv",
            "a".repeat(64),
            "capture_1987.restored.mkv",
            ExportPreset::PreservationMaster,
            vec!["ffmpeg".into(), "-i".into(), "capture_1987.mkv".into()],
            "2026-10-20T03:00:00Z",
        );
        log.finish("2026-10-20T03:12:00Z", Some("b".repeat(64)), true);
        log
    }

    #[test]
    fn log_without_stream_hash_field_deserializes_as_none() {
        // Logs written before output_stream_sha256 existed must still
        // load, with the missing field read as "not recorded".
        let mut value: serde_json::Value =
            serde_json::to_value(sample_log()).expect("log to value");
        value
            .as_object_mut()
            .expect("log is an object")
            .remove("output_stream_sha256");
        let text = serde_json::to_string(&value).expect("value to json");
        let back = ExportLog::from_json(&text).expect("pre-field log deserializes");
        assert_eq!(back.output_stream_sha256, None);
        assert_eq!(back.output_sha256.as_deref(), Some("b".repeat(64).as_str()));
    }

    #[test]
    fn stream_hash_field_round_trips() {
        let mut log = sample_log();
        log.output_stream_sha256 = Some("c".repeat(64));
        let json = log.to_json().expect("serialize");
        let back = ExportLog::from_json(&json).expect("deserialize");
        assert_eq!(back, log);
        assert_eq!(
            back.output_stream_sha256.as_deref(),
            Some("c".repeat(64).as_str())
        );
    }
}
