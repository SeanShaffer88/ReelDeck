//! Batch manifests and batch state: many recipes, run in order, resumable.
//!
//! A **manifest** is the operator's list of work: one entry per source,
//! each naming a recipe file and, optionally, the input and output paths.
//! The **state** is the record of what has happened to each entry so far.
//! It is saved after every transition, so a batch that stops — on a
//! failure, a cancel, or a crash — resumes at the entry that did not
//! finish, without re-rendering the ones that did.
//!
//! Every attempt at an entry keeps its own record and its own output
//! name. A retry never writes over an earlier attempt's output or its
//! proof sidecars: attempt 1 uses the planned output, attempt *n* uses
//! [`attempt_output_path`] (`clip.restored.attempt2.mkv`).
//!
//! This module is pure: no filesystem access and no clock. Paths are
//! carried as strings, and timestamps are supplied by the caller.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hash::is_sha256_hex;

/// Version of the manifest and state formats written by this build.
pub const BATCH_SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// The operator's batch: an ordered list of entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchManifest {
    pub schema_version: u32,
    pub entries: Vec<BatchEntry>,
}

/// One source to render. Relative paths are resolved against the
/// manifest's folder by the runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchEntry {
    /// The recipe file to run.
    pub recipe: String,
    /// The source to render; defaults to the recipe's `source_name`.
    #[serde(default)]
    pub input: Option<String>,
    /// Where to write the output; defaults to
    /// `<source-stem>.restored.<preset extension>`.
    #[serde(default)]
    pub output: Option<String>,
}

/// Why a manifest or state record was rejected.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum BatchError {
    #[error(
        "unsupported batch schema version {0} (this build reads version {BATCH_SCHEMA_VERSION})"
    )]
    UnsupportedSchemaVersion(u32),
    #[error("the batch has no entries")]
    Empty,
    #[error("entry {index} has an empty recipe path")]
    EmptyRecipePath { index: usize },
    #[error("entry {index} has an empty {field} path; omit the field to use the default")]
    EmptyPath { index: usize, field: &'static str },
    #[error("the batch state records a manifest hash that is not a SHA-256: {0}")]
    BadManifestHash(String),
}

impl BatchManifest {
    pub fn new(entries: Vec<BatchEntry>) -> Self {
        BatchManifest {
            schema_version: BATCH_SCHEMA_VERSION,
            entries,
        }
    }

    /// Validate the manifest's shape. Entry numbers in errors are
    /// 1-based, matching what the operator sees.
    pub fn validate(&self) -> Result<(), BatchError> {
        if self.schema_version != BATCH_SCHEMA_VERSION {
            return Err(BatchError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.entries.is_empty() {
            return Err(BatchError::Empty);
        }
        for (i, entry) in self.entries.iter().enumerate() {
            let index = i + 1;
            if entry.recipe.trim().is_empty() {
                return Err(BatchError::EmptyRecipePath { index });
            }
            for (field, value) in [("input", &entry.input), ("output", &entry.output)] {
                if value.as_deref().is_some_and(|v| v.trim().is_empty()) {
                    return Err(BatchError::EmptyPath { index, field });
                }
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Where an entry, or one attempt at it, stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    /// Not attempted yet.
    Pending,
    /// An attempt started and has not been recorded as finished. Seen
    /// on load, this means the previous run was interrupted.
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl EntryStatus {
    /// The lower-case word shown to the operator.
    pub fn label(self) -> &'static str {
        match self {
            EntryStatus::Pending => "pending",
            EntryStatus::Running => "running",
            EntryStatus::Succeeded => "succeeded",
            EntryStatus::Failed => "failed",
            EntryStatus::Cancelled => "cancelled",
        }
    }
}

/// One attempt at rendering an entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    /// 1-based attempt number.
    pub number: u32,
    /// The output this attempt writes.
    pub output: String,
    pub status: EntryStatus,
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
    /// SHA-256 of the output, when the attempt succeeded.
    #[serde(default)]
    pub output_sha256: Option<String>,
    /// What went wrong, when it did not.
    #[serde(default)]
    pub error: Option<String>,
}

/// One manifest entry, resolved, with its attempts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryState {
    /// Resolved recipe path.
    pub recipe: String,
    /// SHA-256 of the recipe file when the batch started; a resume
    /// refuses if the recipe has changed since.
    pub recipe_sha256: String,
    /// Resolved input path.
    pub input: String,
    /// Planned output path (attempt 1).
    pub output: String,
    pub status: EntryStatus,
    #[serde(default)]
    pub attempts: Vec<Attempt>,
}

impl EntryState {
    pub fn new(recipe: String, recipe_sha256: String, input: String, output: String) -> Self {
        EntryState {
            recipe,
            recipe_sha256,
            input,
            output,
            status: EntryStatus::Pending,
            attempts: Vec::new(),
        }
    }

    /// The most recent attempt, if any.
    pub fn last_attempt(&self) -> Option<&Attempt> {
        self.attempts.last()
    }

    /// The successful attempt, if the entry succeeded.
    pub fn succeeded_attempt(&self) -> Option<&Attempt> {
        if self.status != EntryStatus::Succeeded {
            return None;
        }
        self.attempts
            .iter()
            .rev()
            .find(|a| a.status == EntryStatus::Succeeded)
    }
}

/// How an attempt ended, as reported by the runner.
#[derive(Debug, Clone, PartialEq)]
pub enum AttemptOutcome {
    Succeeded { output_sha256: String },
    Failed { error: String },
    Cancelled,
}

/// Entry counts by status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct BatchCounts {
    pub pending: usize,
    pub running: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: usize,
}

/// The saved progress of a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchState {
    pub schema_version: u32,
    /// SHA-256 of the manifest file this state belongs to.
    pub manifest_sha256: String,
    pub entries: Vec<EntryState>,
}

/// Recorded as an interrupted attempt's error on load.
pub const INTERRUPTED_ERROR: &str =
    "interrupted: the previous batch run stopped before this attempt finished";

impl BatchState {
    pub fn new(manifest_sha256: String, entries: Vec<EntryState>) -> Self {
        BatchState {
            schema_version: BATCH_SCHEMA_VERSION,
            manifest_sha256,
            entries,
        }
    }

    pub fn validate(&self) -> Result<(), BatchError> {
        if self.schema_version != BATCH_SCHEMA_VERSION {
            return Err(BatchError::UnsupportedSchemaVersion(self.schema_version));
        }
        if !is_sha256_hex(&self.manifest_sha256) {
            return Err(BatchError::BadManifestHash(self.manifest_sha256.clone()));
        }
        Ok(())
    }

    /// Index of the first entry that has not succeeded.
    pub fn next_unfinished(&self) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.status != EntryStatus::Succeeded)
    }

    pub fn is_complete(&self) -> bool {
        self.next_unfinished().is_none()
    }

    pub fn counts(&self) -> BatchCounts {
        let mut counts = BatchCounts::default();
        for entry in &self.entries {
            match entry.status {
                EntryStatus::Pending => counts.pending += 1,
                EntryStatus::Running => counts.running += 1,
                EntryStatus::Succeeded => counts.succeeded += 1,
                EntryStatus::Failed => counts.failed += 1,
                EntryStatus::Cancelled => counts.cancelled += 1,
            }
        }
        counts
    }

    /// Mark attempts left `Running` by a previous, interrupted run as
    /// failed, so the entry is retried and the record says why.
    /// Returns how many were recovered.
    pub fn recover_interrupted(&mut self, now: &str) -> usize {
        let mut recovered = 0;
        for entry in &mut self.entries {
            if entry.status != EntryStatus::Running {
                continue;
            }
            if let Some(attempt) = entry.attempts.last_mut() {
                attempt.status = EntryStatus::Failed;
                attempt.finished_at = Some(now.to_string());
                attempt.error = Some(INTERRUPTED_ERROR.to_string());
            }
            entry.status = EntryStatus::Failed;
            recovered += 1;
        }
        recovered
    }

    /// Start the next attempt at an entry and return it. The attempt's
    /// output is the planned output for attempt 1 and a numbered
    /// variant after that, so no attempt reuses an earlier one's paths.
    ///
    /// # Panics
    ///
    /// If `index` is out of range or the entry already succeeded —
    /// both caller bugs.
    pub fn begin_attempt(&mut self, index: usize, started_at: &str) -> &Attempt {
        let entry = &mut self.entries[index];
        assert_ne!(
            entry.status,
            EntryStatus::Succeeded,
            "a succeeded entry is never attempted again"
        );
        let number = entry.attempts.len() as u32 + 1;
        let output = if number == 1 {
            entry.output.clone()
        } else {
            attempt_output_path(&entry.output, number)
        };
        entry.attempts.push(Attempt {
            number,
            output,
            status: EntryStatus::Running,
            started_at: started_at.to_string(),
            finished_at: None,
            output_sha256: None,
            error: None,
        });
        entry.status = EntryStatus::Running;
        entry.attempts.last().expect("attempt just pushed")
    }

    /// Record how the entry's running attempt ended.
    ///
    /// # Panics
    ///
    /// If the entry has no running attempt — a caller bug.
    pub fn finish_attempt(&mut self, index: usize, outcome: AttemptOutcome, finished_at: &str) {
        let entry = &mut self.entries[index];
        let attempt = entry
            .attempts
            .last_mut()
            .filter(|a| a.status == EntryStatus::Running)
            .expect("finish_attempt needs a running attempt");
        attempt.finished_at = Some(finished_at.to_string());
        let status = match outcome {
            AttemptOutcome::Succeeded { output_sha256 } => {
                attempt.output_sha256 = Some(output_sha256);
                EntryStatus::Succeeded
            }
            AttemptOutcome::Failed { error } => {
                attempt.error = Some(error);
                EntryStatus::Failed
            }
            AttemptOutcome::Cancelled => {
                attempt.error = Some("cancelled by operator".to_string());
                EntryStatus::Cancelled
            }
        };
        attempt.status = status;
        entry.status = status;
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// The output path for a numbered attempt: `out/clip.restored.mkv`,
/// attempt 2 → `out/clip.restored.attempt2.mkv`. Attempt 1 (or 0) is
/// the planned path unchanged.
pub fn attempt_output_path(planned: &str, attempt: u32) -> String {
    if attempt <= 1 {
        return planned.to_string();
    }
    let path = Path::new(planned);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    let name = match path.extension() {
        Some(ext) => format!("{stem}.attempt{attempt}.{}", ext.to_string_lossy()),
        None => format!("{stem}.attempt{attempt}"),
    };
    match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => parent.join(name).to_string_lossy().into_owned(),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    fn entry(recipe: &str) -> BatchEntry {
        BatchEntry {
            recipe: recipe.to_string(),
            input: None,
            output: None,
        }
    }

    fn two_entry_state() -> BatchState {
        BatchState::new(
            HASH.to_string(),
            vec![
                EntryState::new(
                    "a.json".into(),
                    HASH.into(),
                    "a.mkv".into(),
                    "a.restored.mkv".into(),
                ),
                EntryState::new(
                    "b.json".into(),
                    HASH.into(),
                    "b.mkv".into(),
                    "b.restored.mkv".into(),
                ),
            ],
        )
    }

    #[test]
    fn manifest_json_round_trip_and_defaults() {
        let manifest = BatchManifest::from_json(
            r#"{"schema_version": 1, "entries": [{"recipe": "a.json"}, {"recipe": "b.json", "input": "b.mkv", "output": "out/b.mkv"}]}"#,
        )
        .expect("parse");
        assert_eq!(manifest.entries[0], entry("a.json"));
        assert_eq!(manifest.entries[1].output.as_deref(), Some("out/b.mkv"));
        assert!(manifest.validate().is_ok());
        let back =
            BatchManifest::from_json(&manifest.to_json().expect("serialize")).expect("parse");
        assert_eq!(back, manifest);
    }

    #[test]
    fn manifest_validation_rejects_bad_shapes() {
        assert_eq!(
            BatchManifest::new(vec![]).validate(),
            Err(BatchError::Empty)
        );
        let mut wrong_version = BatchManifest::new(vec![entry("a.json")]);
        wrong_version.schema_version = 9;
        assert_eq!(
            wrong_version.validate(),
            Err(BatchError::UnsupportedSchemaVersion(9))
        );
        assert_eq!(
            BatchManifest::new(vec![entry("a.json"), entry("  ")]).validate(),
            Err(BatchError::EmptyRecipePath { index: 2 })
        );
        let mut empty_output = entry("a.json");
        empty_output.output = Some(String::new());
        assert_eq!(
            BatchManifest::new(vec![empty_output]).validate(),
            Err(BatchError::EmptyPath {
                index: 1,
                field: "output"
            })
        );
    }

    #[test]
    fn attempts_succeed_and_the_batch_completes() {
        let mut state = two_entry_state();
        assert_eq!(state.next_unfinished(), Some(0));
        let attempt = state.begin_attempt(0, "t0");
        assert_eq!(
            (attempt.number, attempt.output.as_str()),
            (1, "a.restored.mkv")
        );
        assert_eq!(state.entries[0].status, EntryStatus::Running);
        state.finish_attempt(
            0,
            AttemptOutcome::Succeeded {
                output_sha256: HASH.into(),
            },
            "t1",
        );
        assert_eq!(state.next_unfinished(), Some(1));
        state.begin_attempt(1, "t2");
        state.finish_attempt(
            1,
            AttemptOutcome::Succeeded {
                output_sha256: HASH.into(),
            },
            "t3",
        );
        assert!(state.is_complete());
        assert_eq!(state.counts().succeeded, 2);
        assert_eq!(
            state.entries[1].succeeded_attempt().map(|a| a.number),
            Some(1)
        );
    }

    #[test]
    fn a_retry_uses_a_new_output_name_and_keeps_the_failed_record() {
        let mut state = two_entry_state();
        state.begin_attempt(0, "t0");
        state.finish_attempt(
            0,
            AttemptOutcome::Failed {
                error: "render failed".into(),
            },
            "t1",
        );
        assert_eq!(state.entries[0].status, EntryStatus::Failed);
        assert_eq!(
            state.next_unfinished(),
            Some(0),
            "resume starts at the failed entry"
        );
        let retry = state.begin_attempt(0, "t2");
        assert_eq!(retry.number, 2);
        assert_eq!(retry.output, "a.restored.attempt2.mkv");
        let first = &state.entries[0].attempts[0];
        assert_eq!(first.error.as_deref(), Some("render failed"));
        assert_eq!(first.output, "a.restored.mkv");
    }

    #[test]
    fn cancel_is_recorded_distinctly() {
        let mut state = two_entry_state();
        state.begin_attempt(0, "t0");
        state.finish_attempt(0, AttemptOutcome::Cancelled, "t1");
        assert_eq!(state.entries[0].status, EntryStatus::Cancelled);
        assert_eq!(state.counts().cancelled, 1);
        assert!(state.entries[0].succeeded_attempt().is_none());
    }

    #[test]
    fn interrupted_attempts_are_recovered_as_failures() {
        let mut state = two_entry_state();
        state.begin_attempt(0, "t0");
        let json = state.to_json().expect("serialize");
        let mut loaded = BatchState::from_json(&json).expect("parse");
        assert_eq!(loaded.recover_interrupted("t9"), 1);
        let attempt = &loaded.entries[0].attempts[0];
        assert_eq!(attempt.status, EntryStatus::Failed);
        assert_eq!(attempt.error.as_deref(), Some(INTERRUPTED_ERROR));
        assert_eq!(attempt.finished_at.as_deref(), Some("t9"));
        assert_eq!(
            loaded.recover_interrupted("t10"),
            0,
            "recovery is idempotent"
        );
    }

    #[test]
    fn state_validation_checks_version_and_hash() {
        assert!(two_entry_state().validate().is_ok());
        let mut bad = two_entry_state();
        bad.manifest_sha256 = "nope".into();
        assert!(matches!(
            bad.validate(),
            Err(BatchError::BadManifestHash(_))
        ));
    }

    #[test]
    fn attempt_output_paths() {
        assert_eq!(
            attempt_output_path("clip.restored.mkv", 1),
            "clip.restored.mkv"
        );
        assert_eq!(
            attempt_output_path("clip.restored.mkv", 3),
            "clip.restored.attempt3.mkv"
        );
        assert_eq!(
            Path::new(&attempt_output_path("out/clip.mkv", 2)),
            Path::new("out").join("clip.attempt2.mkv")
        );
        assert_eq!(attempt_output_path("noext", 2), "noext.attempt2");
    }

    #[test]
    #[should_panic(expected = "never attempted again")]
    fn a_succeeded_entry_cannot_be_attempted_again() {
        let mut state = two_entry_state();
        state.begin_attempt(0, "t0");
        state.finish_attempt(
            0,
            AttemptOutcome::Succeeded {
                output_sha256: HASH.into(),
            },
            "t1",
        );
        state.begin_attempt(0, "t2");
    }
}
