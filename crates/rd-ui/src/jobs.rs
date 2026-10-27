//! Job journal: the model side of the engine binding.
//!
//! `rd-run` executes renders off the UI thread, streams measured
//! [`rd_run::Progress`] blocks, and takes cooperative cancellation
//! through an `Arc<AtomicBool>`. This crate spawns nothing — the shipping
//! shell owns the worker thread, the cancel flag, and the callback
//! wiring. What lives here is the *state contract* those callbacks bind
//! to, so the interface can be built and tested without a renderer:
//!
//! - an explicit lifecycle, `Queued → Running → terminal`, with
//!   `Cancelling` while a cancel request propagates to the engine;
//! - progress that only ever carries measured values — a percentage is
//!   derived from the frame total when one is known, and stays `None`
//!   (`measuring…`) otherwise, never estimated;
//! - a journal retaining every outcome and its proof references, capped
//!   so a long session cannot grow it without bound.
//!
//! Transitions that cannot happen (progress on a finished job, a second
//! terminal state, cancelling an unknown id) are errors, not silent
//! no-ops: a shell that requests them has a wiring bug worth surfacing.

use rd_run::{Progress, RunReport};
use serde::{Deserialize, Serialize};

use crate::JobSummary;

/// Maximum journal entries retained. When enqueue would exceed it, the
/// oldest *terminal* entry is evicted; an active entry is never removed.
pub const JOB_HISTORY_LIMIT: usize = 100;

/// Stable identifier for one job within a session's journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(u64);

impl JobId {
    /// The numeric value, for keys and display.
    pub fn value(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What the job is doing. The kind decides which evidence a successful
/// outcome updates on its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// A short segment render through the current chain.
    Preview,
    /// A full preservation, mezzanine or access export.
    Export,
    /// Checking an output against its proof sidecars.
    Verify,
    /// Re-executing a recorded recipe beside a new output.
    Rerun,
    /// Recomputing a source's SHA-256.
    Fixity,
    /// Re-running signal analysis and field-order detection on a source.
    Inspect,
}

impl JobKind {
    /// The word the panel prints; state is never carried by color alone.
    pub fn label(self) -> &'static str {
        match self {
            JobKind::Preview => "Preview",
            JobKind::Export => "Export",
            JobKind::Verify => "Verify",
            JobKind::Rerun => "Re-run",
            JobKind::Fixity => "Fixity",
            JobKind::Inspect => "Inspect",
        }
    }
}

/// Measured progress at one point in a run. Every field comes from an
/// engine progress block; nothing here is interpolated or estimated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct JobProgress {
    pub frames_processed: u64,
    pub out_time_us: i64,
    pub fps: f64,
    pub speed: f64,
    pub total_size: u64,
    /// Frames as a percentage of the job's known total. `None` while the
    /// total is unknown, which the panel renders as `measuring…`.
    pub percent: Option<u8>,
}

impl JobProgress {
    /// Build a snapshot from one engine block, deriving the percentage
    /// from the planned total. An unknown or zero total yields `None`
    /// rather than a fabricated number.
    fn from_measured(progress: &Progress, total_frames: Option<u64>) -> Self {
        let percent = total_frames.filter(|t| *t > 0).map(|total| {
            ((progress.frame.saturating_mul(100)) / total).min(100) as u8
        });
        JobProgress {
            frames_processed: progress.frame,
            out_time_us: progress.out_time_us,
            fps: progress.fps,
            speed: progress.speed,
            total_size: progress.total_size,
            percent,
        }
    }
}

/// The proof-bearing outcome of a successful run, in platform-neutral
/// strings so the model stays free of path types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOutcome {
    pub output: String,
    pub output_sha256: String,
    pub frames_processed: u64,
    pub recipe_sidecar: String,
    pub log_sidecar: String,
    pub warnings: Vec<String>,
}

impl From<&RunReport> for JobOutcome {
    fn from(report: &RunReport) -> Self {
        JobOutcome {
            output: report.output.display().to_string(),
            output_sha256: report.output_sha256.clone(),
            frames_processed: report.frames_processed,
            recipe_sidecar: report.recipe_path.display().to_string(),
            log_sidecar: report.log_path.display().to_string(),
            warnings: report.warnings.clone(),
        }
    }
}

/// Where a job is in its lifecycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Accepted, waiting for a worker.
    Queued,
    /// Running, with the latest measured progress.
    Running(JobProgress),
    /// Cancel requested; the engine has not stopped reporting yet.
    Cancelling(JobProgress),
    /// Finished successfully; the outcome carries the proof references.
    Succeeded(JobOutcome),
    /// Stopped by an engine or planning failure.
    Failed { detail: String },
    /// Confirmed stopped; the engine removed any partial output.
    Cancelled,
}

impl JobStatus {
    /// Stable word for logs and tests.
    pub fn phase(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running(_) => "running",
            JobStatus::Cancelling(_) => "cancelling",
            JobStatus::Succeeded(_) => "succeeded",
            JobStatus::Failed { .. } => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    /// The word the panel shows beside the job.
    pub fn word(&self) -> &'static str {
        match self {
            JobStatus::Queued => "Queued",
            JobStatus::Running(_) => "Running",
            JobStatus::Cancelling(_) => "Cancelling",
            JobStatus::Succeeded(_) => "Done",
            JobStatus::Failed { .. } => "Failed",
            JobStatus::Cancelled => "Cancelled",
        }
    }

    /// True while the job may still emit progress or reach a terminal.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            JobStatus::Queued | JobStatus::Running(_) | JobStatus::Cancelling(_)
        )
    }
}

/// Why a journal transition was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JobError {
    #[error("unknown job {0}")]
    UnknownJob(JobId),
    #[error("job {id} cannot move from {from} to {to}")]
    IllegalTransition {
        id: JobId,
        from: &'static str,
        to: &'static str,
    },
}

/// One journal entry: identity, intent, and current lifecycle state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: JobId,
    pub label: String,
    pub kind: JobKind,
    /// Queue row this job belongs to, when it was started for a source.
    /// Evidence from the outcome is written back to this source.
    pub source_index: Option<usize>,
    /// Planned frame total, used to turn measured frames into a
    /// percentage. `None` keeps progress honestly unmeasured.
    pub total_frames: Option<u64>,
    pub status: JobStatus,
}

impl JobRecord {
    /// The bottom-panel projection of this record.
    pub fn summary(&self) -> JobSummary {
        let percent = match &self.status {
            JobStatus::Running(progress) | JobStatus::Cancelling(progress) => progress.percent,
            JobStatus::Succeeded(_) => Some(100),
            JobStatus::Queued | JobStatus::Failed { .. } | JobStatus::Cancelled => None,
        };
        JobSummary {
            label: self.label.clone(),
            percent,
            cancellable: matches!(self.status, JobStatus::Running(_)),
        }
    }
}

/// The session's job journal. Append-only in identity: a job's id is
/// never reused, and terminal entries stay until the cap evicts them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JobJournal {
    jobs: Vec<JobRecord>,
    next_id: u64,
}

impl JobJournal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accept a job. It starts `Queued`; call [`JobJournal::start`] when
    /// a worker picks it up. Returns the id the engine callbacks use.
    pub fn enqueue(
        &mut self,
        label: impl Into<String>,
        kind: JobKind,
        source_index: Option<usize>,
        total_frames: Option<u64>,
    ) -> JobId {
        self.next_id += 1;
        let id = JobId(self.next_id);
        self.jobs.push(JobRecord {
            id,
            label: label.into(),
            kind,
            source_index,
            total_frames,
            status: JobStatus::Queued,
        });
        self.enforce_cap();
        id
    }

    /// Drop the oldest terminal entry when over the cap, so an active
    /// job is never evicted; if every entry is active the oldest goes.
    fn enforce_cap(&mut self) {
        while self.jobs.len() > JOB_HISTORY_LIMIT {
            let terminal = self
                .jobs
                .iter()
                .position(|job| !job.status.is_active());
            let index = terminal.unwrap_or(0);
            self.jobs.remove(index);
        }
    }

    fn index_of(&self, id: JobId) -> Result<usize, JobError> {
        self.jobs.iter().position(|job| job.id == id).ok_or(JobError::UnknownJob(id))
    }

    /// A worker took the job: `Queued → Running`.
    pub fn start(&mut self, id: JobId) -> Result<(), JobError> {
        let index = self.index_of(id)?;
        if !matches!(self.jobs[index].status, JobStatus::Queued) {
            return Err(JobError::IllegalTransition {
                id,
                from: self.jobs[index].status.phase(),
                to: "running",
            });
        }
        self.jobs[index].status = JobStatus::Running(JobProgress::default());
        Ok(())
    }

    /// Apply one measured engine block. Allowed while running or
    /// cancelling, since the renderer keeps reporting as it drains.
    pub fn apply_progress(
        &mut self,
        id: JobId,
        progress: &Progress,
    ) -> Result<(), JobError> {
        let index = self.index_of(id)?;
        let total = self.jobs[index].total_frames;
        let next = JobProgress::from_measured(progress, total);
        match &self.jobs[index].status {
            // The engine keeps reporting while it drains a cancel; the
            // cancelling state must survive those late blocks.
            JobStatus::Running(_) => {
                self.jobs[index].status = JobStatus::Running(next);
                Ok(())
            }
            JobStatus::Cancelling(_) => {
                self.jobs[index].status = JobStatus::Cancelling(next);
                Ok(())
            }
            _ => Err(JobError::IllegalTransition {
                id,
                from: self.jobs[index].status.phase(),
                to: "running",
            }),
        }
    }

    /// Ask the engine to stop. `Running → Cancelling`; requesting again
    /// while already cancelling is harmless. The cancel flag itself
    /// lives in the binding layer (`RunOptions.cancel`).
    pub fn request_cancel(&mut self, id: JobId) -> Result<(), JobError> {
        let index = self.index_of(id)?;
        match &self.jobs[index].status {
            JobStatus::Running(progress) => {
                let progress = *progress;
                self.jobs[index].status = JobStatus::Cancelling(progress);
                Ok(())
            }
            JobStatus::Cancelling(_) => Ok(()),
            _ => Err(JobError::IllegalTransition {
                id,
                from: self.jobs[index].status.phase(),
                to: "cancelling",
            }),
        }
    }

    /// Record a successful run with its proof. Only a running job can
    /// succeed; a job whose cancel was requested ends through
    /// [`JobJournal::confirm_cancelled`].
    pub fn succeed(&mut self, id: JobId, report: &RunReport) -> Result<(), JobError> {
        let index = self.index_of(id)?;
        if !matches!(self.jobs[index].status, JobStatus::Running(_)) {
            return Err(JobError::IllegalTransition {
                id,
                from: self.jobs[index].status.phase(),
                to: "succeeded",
            });
        }
        self.jobs[index].status = JobStatus::Succeeded(JobOutcome::from(report));
        Ok(())
    }

    /// Record an engine or planning failure. Allowed from either active
    /// render state, including while a cancel was draining.
    pub fn fail(&mut self, id: JobId, detail: impl Into<String>) -> Result<(), JobError> {
        let index = self.index_of(id)?;
        if !self.jobs[index].status.is_active() {
            return Err(JobError::IllegalTransition {
                id,
                from: self.jobs[index].status.phase(),
                to: "failed",
            });
        }
        self.jobs[index].status = JobStatus::Failed {
            detail: detail.into(),
        };
        Ok(())
    }

    /// The engine confirmed the cancel and removed the partial output:
    /// `Cancelling → Cancelled`.
    pub fn confirm_cancelled(&mut self, id: JobId) -> Result<(), JobError> {
        let index = self.index_of(id)?;
        if !matches!(self.jobs[index].status, JobStatus::Cancelling(_)) {
            return Err(JobError::IllegalTransition {
                id,
                from: self.jobs[index].status.phase(),
                to: "cancelled",
            });
        }
        self.jobs[index].status = JobStatus::Cancelled;
        Ok(())
    }

    /// The record for an id, if it exists.
    pub fn get(&self, id: JobId) -> Option<&JobRecord> {
        self.jobs.iter().find(|job| job.id == id)
    }

    /// Every record, oldest first.
    pub fn records(&self) -> &[JobRecord] {
        &self.jobs
    }

    /// Records that have not reached a terminal state.
    pub fn active(&self) -> Vec<&JobRecord> {
        self.jobs.iter().filter(|job| job.status.is_active()).collect()
    }

    /// Bottom-panel projections for every record, oldest first.
    pub fn summaries(&self) -> Vec<JobSummary> {
        self.jobs.iter().map(JobRecord::summary).collect()
    }

    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(frame: u64) -> Progress {
        Progress {
            frame,
            fps: 42.0,
            out_time_us: frame as i64 * 40_000,
            total_size: frame * 1024,
            speed: 1.2,
            ..Progress::default()
        }
    }

    fn report() -> RunReport {
        RunReport {
            output: std::path::PathBuf::from("D:/out/interview.restored.mkv"),
            output_sha256: "a".repeat(64),
            frames_processed: 100,
            recipe_path: std::path::PathBuf::from("D:/out/interview.restored.mkv.recipe.json"),
            log_path: std::path::PathBuf::from("D:/out/interview.restored.mkv.export-log.json"),
            warnings: vec!["one dropped frame".to_string()],
        }
    }

    #[test]
    fn full_lifecycle_is_queued_running_then_succeeded_with_proof() {
        let mut journal = JobJournal::new();
        let id = journal.enqueue("Export · interview", JobKind::Export, Some(0), Some(100));
        assert_eq!(journal.get(id).unwrap().status, JobStatus::Queued);
        journal.start(id).expect("worker picks it up");
        journal
            .apply_progress(id, &measured(25))
            .expect("progress accepted");
        let record = journal.get(id).unwrap();
        match record.status {
            JobStatus::Running(progress) => {
                assert_eq!(progress.frames_processed, 25);
                assert_eq!(progress.percent, Some(25));
                assert_eq!(progress.fps, 42.0);
            }
            other => panic!("expected running, got {other:?}"),
        }
        journal.succeed(id, &report()).expect("succeeds");
        let outcome = match &journal.get(id).unwrap().status {
            JobStatus::Succeeded(outcome) => outcome.clone(),
            other => panic!("expected succeeded, got {other:?}"),
        };
        assert_eq!(outcome.frames_processed, 100);
        assert!(outcome.recipe_sidecar.ends_with(".recipe.json"));
        assert_eq!(outcome.warnings, vec!["one dropped frame".to_string()]);
        assert_eq!(record.summary().percent, Some(100));
        assert!(!record.summary().cancellable);
    }

    #[test]
    fn progress_without_a_total_stays_unmeasured_never_estimated() {
        let mut journal = JobJournal::new();
        let id = journal.enqueue("Preview", JobKind::Preview, None, None);
        journal.start(id).unwrap();
        journal.apply_progress(id, &measured(900)).unwrap();
        let progress = match &journal.get(id).unwrap().status {
            JobStatus::Running(progress) => *progress,
            other => panic!("{other:?}"),
        };
        assert_eq!(progress.frames_processed, 900);
        assert_eq!(progress.percent, None);
        assert_eq!(journal.summaries()[0].percent_text(), "measuring…");
    }

    #[test]
    fn percentage_saturates_at_one_hundred() {
        let mut journal = JobJournal::new();
        let id = journal.enqueue("Export", JobKind::Export, None, Some(50));
        journal.start(id).unwrap();
        journal.apply_progress(id, &measured(50)).unwrap();
        assert_eq!(journal.get(id).unwrap().summary().percent, Some(100));
        // A late block past the planned total cannot report over 100%.
        journal.apply_progress(id, &measured(60)).unwrap();
        assert_eq!(journal.get(id).unwrap().summary().percent, Some(100));
    }

    #[test]
    fn illegal_transitions_are_errors_not_silent_no_ops() {
        let mut journal = JobJournal::new();
        let id = journal.enqueue("Export", JobKind::Export, None, Some(10));
        // Progress before start.
        assert!(matches!(
            journal.apply_progress(id, &measured(1)),
            Err(JobError::IllegalTransition { .. })
        ));
        journal.start(id).unwrap();
        journal.succeed(id, &report()).unwrap();
        // A terminal job accepts nothing further.
        assert!(journal.apply_progress(id, &measured(2)).is_err());
        assert!(journal.request_cancel(id).is_err());
        assert!(journal.succeed(id, &report()).is_err());
        assert!(journal.confirm_cancelled(id).is_err());
        // Unknown ids are named, not panicked on.
        assert!(matches!(
            journal.start(JobId(999)),
            Err(JobError::UnknownJob(_))
        ));
    }

    #[test]
    fn cancellation_drains_then_confirms_and_keeps_measuring() {
        let mut journal = JobJournal::new();
        let id = journal.enqueue("Export", JobKind::Export, None, Some(100));
        journal.start(id).unwrap();
        journal.apply_progress(id, &measured(40)).unwrap();
        journal.request_cancel(id).expect("cancel requested");
        assert!(matches!(
            journal.get(id).unwrap().status,
            JobStatus::Cancelling(_)
        ));
        // A second request is idempotent.
        journal.request_cancel(id).unwrap();
        // Blocks keep arriving while the engine winds down.
        journal.apply_progress(id, &measured(45)).unwrap();
        assert!(matches!(
            journal.get(id).unwrap().status,
            JobStatus::Cancelling(_)
        ));
        // A cancelling job cannot also succeed.
        assert!(journal.succeed(id, &report()).is_err());
        journal.confirm_cancelled(id).unwrap();
        assert_eq!(journal.get(id).unwrap().status, JobStatus::Cancelled);
        assert_eq!(journal.get(id).unwrap().summary().percent, None);
        assert!(!journal.get(id).unwrap().summary().cancellable);
    }

    #[test]
    fn failure_records_its_detail_and_a_queued_job_can_fail() {
        let mut journal = JobJournal::new();
        let id = journal.enqueue("Export", JobKind::Export, None, None);
        journal
            .fail(id, "render failed: exit status 1")
            .expect("a queued job may fail before its worker starts");
        assert_eq!(
            journal.get(id).unwrap().status,
            JobStatus::Failed {
                detail: "render failed: exit status 1".to_string()
            }
        );
        assert_eq!(journal.get(id).unwrap().status.word(), "Failed");
    }

    #[test]
    fn the_cap_evicts_terminal_entries_but_never_active_ones() {
        let mut journal = JobJournal::new();
        // Fill the cap with finished exports.
        for n in 0..JOB_HISTORY_LIMIT {
            let id = journal.enqueue(format!("job {n}"), JobKind::Export, None, None);
            journal.start(id).unwrap();
            journal.succeed(id, &report()).unwrap();
        }
        let active_id = journal.enqueue("running", JobKind::Preview, None, None);
        journal.start(active_id).unwrap();
        assert_eq!(journal.len(), JOB_HISTORY_LIMIT);
        assert!(journal.get(active_id).is_some(), "the active job survives");
        // The oldest finished entry was evicted: job 1's id is gone.
        assert!(journal.get(JobId(1)).is_none());
        assert!(journal.get(JobId(2)).is_some());
    }

    #[test]
    fn ids_are_unique_and_monotonic_and_words_cover_every_state() {
        let mut journal = JobJournal::new();
        let first = journal.enqueue("a", JobKind::Preview, None, None);
        let second = journal.enqueue("b", JobKind::Fixity, None, None);
        assert!(second > first);
        assert_ne!(first, second);
        for status in [
            JobStatus::Queued,
            JobStatus::Running(JobProgress::default()),
            JobStatus::Cancelling(JobProgress::default()),
            JobStatus::Succeeded(JobOutcome::from(&report())),
            JobStatus::Failed {
                detail: "x".to_string(),
            },
            JobStatus::Cancelled,
        ] {
            assert!(!status.word().is_empty());
            assert!(!status.phase().is_empty());
        }
        assert_eq!(JobKind::Rerun.label(), "Re-run");
        assert_eq!(JobKind::Inspect.label(), "Inspect");
    }

    #[test]
    fn journal_round_trips_through_serialization() {
        let mut journal = JobJournal::new();
        let id = journal.enqueue("Export · interview", JobKind::Export, Some(2), Some(100));
        journal.start(id).unwrap();
        journal.apply_progress(id, &measured(10)).unwrap();
        let json = serde_json::to_string(&journal).expect("serialize");
        let back: JobJournal = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, journal);
        assert_eq!(back.records()[0].source_index, Some(2));
    }
}
