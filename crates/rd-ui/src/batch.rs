//! Batch queue: the shell side of `rd_run::batch`.
//!
//! The operator picks sources from the queue, and the shell renders them
//! one after another. This module holds the state contract that run
//! binds to, with the same honesty rules as single jobs:
//!
//! - **Only ready sources can join.** A source joins a batch only when
//!   its single export would be allowed: its hash matches, its chain
//!   plans, and the preview gate is satisfied. A blocked source says
//!   why, in the same words the export panel uses.
//! - **Every render is a job.** Each attempt goes through the job
//!   journal (`Queued → Running → terminal`), so batch progress is the
//!   engine's measured progress and nothing else, and a succeeded entry
//!   writes its export evidence back to its source like any export.
//! - **A stopped batch says where it stopped.** The resume point is the
//!   first entry that has not succeeded; resuming continues there, and
//!   entries that already succeeded are skipped (the runner re-verifies
//!   their outputs first).
//!
//! The binding layer owns the worker thread, the manifest and state
//! files, and the cancel flag: it calls [`ShellModel::begin_batch`] or
//! [`ShellModel::resume_batch`], runs `rd_run::batch::run_batch` on a
//! worker, forwards each `BatchEvent` to
//! [`ShellModel::apply_batch_event`], sets the cancel flag when
//! [`ShellModel::request_batch_cancel`] succeeds, and calls
//! [`ShellModel::batch_ended`] when the run returns.

use rd_core::{BatchState, EntryStatus};
use rd_run::batch::BatchEvent;
use serde::{Deserialize, Serialize};

use crate::chain::{export_is_ready, export_preflight, CheckLevel};
use crate::jobs::{JobError, JobId, JobKind};
use crate::{FixityStatus, ShellModel};

/// Whether a queue row may join a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchEligibility {
    Ready,
    /// Not allowed, with the reason and the fix in one sentence.
    Blocked {
        reason: String,
    },
}

/// One queue row as the batch dialog lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchCandidate {
    pub source_index: usize,
    pub name: String,
    pub eligibility: BatchEligibility,
}

/// Where the batch as a whole stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchPhase {
    /// Accepted; the runner has not reported yet.
    Running,
    /// Cancel requested; the running entry has not stopped yet.
    Cancelling,
    /// Ended with work left: a failure, a cancel, or `keep_going`
    /// finishing with failures. Resumable.
    Stopped,
    /// Every entry succeeded.
    Complete,
}

impl BatchPhase {
    pub fn word(self) -> &'static str {
        match self {
            BatchPhase::Running => "Running",
            BatchPhase::Cancelling => "Cancelling",
            BatchPhase::Stopped => "Stopped",
            BatchPhase::Complete => "Complete",
        }
    }
}

/// One entry of the batch, tied to its queue row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchSessionEntry {
    pub source_index: usize,
    pub name: String,
    pub status: EntryStatus,
    /// Attempts made so far (0 until the first starts).
    pub attempts: u32,
    /// The latest attempt's output, once one has started.
    #[serde(default)]
    pub output: Option<String>,
    /// The output hash on success, or the error otherwise.
    #[serde(default)]
    pub detail: Option<String>,
    /// The journal job for the running or latest attempt.
    #[serde(default)]
    pub job: Option<JobId>,
}

/// The batch the shell is running or last ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchSession {
    pub entries: Vec<BatchSessionEntry>,
    pub keep_going: bool,
    pub phase: BatchPhase,
}

impl BatchSession {
    /// 0-based index of the first entry that has not succeeded — where a
    /// resume starts.
    pub fn resume_point(&self) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.status != EntryStatus::Succeeded)
    }

    pub fn succeeded(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == EntryStatus::Succeeded)
            .count()
    }

    /// The entry currently rendering, if any.
    pub fn running(&self) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.status == EntryStatus::Running)
    }

    /// One line for the jobs panel header and the live region.
    pub fn summary_text(&self) -> String {
        let total = self.entries.len();
        let done = self.succeeded();
        match self.phase {
            BatchPhase::Running => match self.running() {
                Some(i) => format!(
                    "Batch running · {done} of {total} succeeded · rendering {} of {total}",
                    i + 1
                ),
                None => format!("Batch running · {done} of {total} succeeded"),
            },
            BatchPhase::Cancelling => {
                format!("Batch cancelling · {done} of {total} succeeded · waiting for the render to stop")
            }
            BatchPhase::Complete => format!("Batch complete · {done} of {total} succeeded"),
            BatchPhase::Stopped => {
                let Some(i) = self.resume_point() else {
                    return format!("Batch stopped · {done} of {total} succeeded");
                };
                let why = self.entries[i].status.label();
                format!(
                    "Batch stopped · {done} of {total} succeeded · entry {} {why} · resume continues there",
                    i + 1
                )
            }
        }
    }
}

/// Why a batch request was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BatchShellError {
    #[error("choose at least one source for the batch")]
    NothingChosen,
    #[error("queue row {0} does not exist")]
    UnknownSource(usize),
    #[error("{name} cannot join the batch: {reason}")]
    NotReady { name: String, reason: String },
    #[error("{name} is chosen twice; each source renders once per batch")]
    Duplicate { name: String },
    #[error("a batch is already running; cancel it or wait for it to end")]
    AlreadyRunning,
    #[error("there is no batch to {0}")]
    NoBatch(&'static str),
    #[error("the batch is {0}, so it cannot be {1}")]
    WrongPhase(&'static str, &'static str),
    #[error("the saved batch state has {state} entries but {given} queue rows were given for it")]
    StateMismatch { state: usize, given: usize },
    #[error("batch event for entry {0}, which this batch does not have")]
    UnknownEntry(usize),
    #[error(transparent)]
    Job(#[from] JobError),
}

impl ShellModel {
    /// Every queue row with whether it may join a batch.
    pub fn batch_candidates(&self) -> Vec<BatchCandidate> {
        self.queue
            .iter()
            .enumerate()
            .map(|(i, item)| BatchCandidate {
                source_index: i,
                name: item.name.clone(),
                eligibility: self.batch_eligibility(i),
            })
            .collect()
    }

    /// Whether queue row `index` may join a batch, and if not, why.
    pub fn batch_eligibility(&self, index: usize) -> BatchEligibility {
        let blocked = |reason: &str| BatchEligibility::Blocked {
            reason: reason.to_string(),
        };
        let Some(item) = self.queue.get(index) else {
            return blocked("This queue row no longer exists.");
        };
        if item.fixity == FixityStatus::Failed {
            return blocked(
                "Hash mismatch: the file changed since ingest, so exports from it are blocked.",
            );
        }
        let has_chain = self
            .sources
            .get(index)
            .and_then(|s| s.recipe.as_ref())
            .is_some_and(|r| !r.operations.is_empty());
        if !has_chain {
            return blocked("No chain yet: build one in Decide first.");
        }
        if !self.export_gate_open_for(index) {
            return blocked("Preview required: render a preview segment of this chain first, or record an override.");
        }
        let checks = export_preflight(&self.export_facts_for(index));
        if !export_is_ready(&checks) {
            let detail = checks
                .iter()
                .find(|c| c.level == CheckLevel::Blocked)
                .map(|c| c.detail.clone())
                .unwrap_or_else(|| "An export check is blocked.".to_string());
            return BatchEligibility::Blocked { reason: detail };
        }
        BatchEligibility::Ready
    }

    /// The current or last batch, if there is one.
    pub fn batch(&self) -> Option<&BatchSession> {
        self.batch.as_ref()
    }

    pub fn can_start_batch(&self) -> bool {
        !self.batch_is_active()
            && (0..self.queue.len()).any(|i| self.batch_eligibility(i) == BatchEligibility::Ready)
    }

    pub fn can_resume_batch(&self) -> bool {
        self.batch
            .as_ref()
            .is_some_and(|b| b.phase == BatchPhase::Stopped)
    }

    pub fn can_cancel_batch(&self) -> bool {
        self.batch
            .as_ref()
            .is_some_and(|b| b.phase == BatchPhase::Running)
    }

    fn batch_is_active(&self) -> bool {
        self.batch
            .as_ref()
            .is_some_and(|b| matches!(b.phase, BatchPhase::Running | BatchPhase::Cancelling))
    }

    /// Start a new batch over the chosen queue rows, in the order given.
    /// Every row must be [`BatchEligibility::Ready`]; the first one that
    /// is not is named in the refusal, and nothing starts.
    pub fn begin_batch(
        &mut self,
        source_indices: &[usize],
        keep_going: bool,
    ) -> Result<(), BatchShellError> {
        if self.batch_is_active() {
            return Err(BatchShellError::AlreadyRunning);
        }
        if source_indices.is_empty() {
            return Err(BatchShellError::NothingChosen);
        }
        let mut entries = Vec::with_capacity(source_indices.len());
        for (n, &i) in source_indices.iter().enumerate() {
            let item = self.queue.get(i).ok_or(BatchShellError::UnknownSource(i))?;
            if source_indices[..n].contains(&i) {
                return Err(BatchShellError::Duplicate {
                    name: item.name.clone(),
                });
            }
            if let BatchEligibility::Blocked { reason } = self.batch_eligibility(i) {
                return Err(BatchShellError::NotReady {
                    name: item.name.clone(),
                    reason,
                });
            }
            entries.push(BatchSessionEntry {
                source_index: i,
                name: item.name.clone(),
                status: EntryStatus::Pending,
                attempts: 0,
                output: None,
                detail: None,
                job: None,
            });
        }
        self.batch = Some(BatchSession {
            entries,
            keep_going,
            phase: BatchPhase::Running,
        });
        Ok(())
    }

    /// Restore a batch from its saved state (for example after the app
    /// restarts), with `source_indices[k]` the queue row of state entry
    /// `k`. Attempts the state left running are shown as interrupted
    /// failures, as the runner will record them on resume.
    pub fn restore_batch(
        &mut self,
        state: &BatchState,
        source_indices: &[usize],
        keep_going: bool,
    ) -> Result<(), BatchShellError> {
        if self.batch_is_active() {
            return Err(BatchShellError::AlreadyRunning);
        }
        if state.entries.len() != source_indices.len() {
            return Err(BatchShellError::StateMismatch {
                state: state.entries.len(),
                given: source_indices.len(),
            });
        }
        let mut entries = Vec::with_capacity(source_indices.len());
        for (entry, &i) in state.entries.iter().zip(source_indices) {
            let item = self.queue.get(i).ok_or(BatchShellError::UnknownSource(i))?;
            let last = entry.last_attempt();
            let (status, detail) = if entry.status == EntryStatus::Running {
                (
                    EntryStatus::Failed,
                    Some(rd_core::batch::INTERRUPTED_ERROR.to_string()),
                )
            } else {
                let detail = match entry.status {
                    EntryStatus::Succeeded => last.and_then(|a| a.output_sha256.clone()),
                    _ => last.and_then(|a| a.error.clone()),
                };
                (entry.status, detail)
            };
            entries.push(BatchSessionEntry {
                source_index: i,
                name: item.name.clone(),
                status,
                attempts: entry.attempts.len() as u32,
                output: last.map(|a| a.output.clone()),
                detail,
                job: None,
            });
        }
        let complete = entries.iter().all(|e| e.status == EntryStatus::Succeeded);
        self.batch = Some(BatchSession {
            entries,
            keep_going,
            phase: if complete {
                BatchPhase::Complete
            } else {
                BatchPhase::Stopped
            },
        });
        Ok(())
    }

    /// Resume a stopped batch. Returns the 0-based entry the runner will
    /// start at.
    pub fn resume_batch(&mut self) -> Result<usize, BatchShellError> {
        let batch = self
            .batch
            .as_mut()
            .ok_or(BatchShellError::NoBatch("resume"))?;
        if batch.phase != BatchPhase::Stopped {
            return Err(BatchShellError::WrongPhase(
                phase_name(batch.phase),
                "resumed",
            ));
        }
        let at = batch
            .resume_point()
            .ok_or(BatchShellError::WrongPhase("complete", "resumed"))?;
        batch.phase = BatchPhase::Running;
        Ok(at)
    }

    /// Request cancellation of the running batch. When this returns `Ok`
    /// the binding layer sets the runner's cancel flag; the running
    /// entry's job moves to `Cancelling` until the engine confirms.
    pub fn request_batch_cancel(&mut self) -> Result<(), BatchShellError> {
        let batch = self
            .batch
            .as_mut()
            .ok_or(BatchShellError::NoBatch("cancel"))?;
        if batch.phase != BatchPhase::Running {
            return Err(BatchShellError::WrongPhase(
                phase_name(batch.phase),
                "cancelled",
            ));
        }
        batch.phase = BatchPhase::Cancelling;
        let running_job = batch.running().and_then(|i| batch.entries[i].job);
        if let Some(job) = running_job {
            self.journal.request_cancel(job)?;
        }
        Ok(())
    }

    /// Apply one event from the runner.
    pub fn apply_batch_event(&mut self, event: &BatchEvent) -> Result<(), BatchShellError> {
        let total = self.batch.as_ref().map_or(0, |b| b.entries.len());
        let index = match event {
            BatchEvent::Skipped { index, .. }
            | BatchEvent::Started { index, .. }
            | BatchEvent::Progress { index, .. }
            | BatchEvent::Finished { index, .. } => *index,
        };
        if self.batch.is_none() {
            return Err(BatchShellError::NoBatch("update"));
        }
        if index == 0 || index > total {
            return Err(BatchShellError::UnknownEntry(index));
        }
        let k = index - 1;
        match event {
            BatchEvent::Skipped { output, .. } => {
                let entry = self.batch_entry_mut(k);
                entry.status = EntryStatus::Succeeded;
                entry.output = Some(output.display().to_string());
            }
            BatchEvent::Started {
                attempt, output, ..
            } => {
                let (source_index, name) = {
                    let e = self.batch_entry_mut(k);
                    (e.source_index, e.name.clone())
                };
                let label = format!("Batch {index}/{total} · {name}");
                let job = self
                    .journal
                    .enqueue(label, JobKind::Export, Some(source_index), None);
                self.journal.start(job)?;
                let entry = self.batch_entry_mut(k);
                entry.status = EntryStatus::Running;
                entry.attempts = *attempt;
                entry.output = Some(output.display().to_string());
                entry.detail = None;
                entry.job = Some(job);
            }
            BatchEvent::Progress { progress, .. } => {
                if let Some(job) = self.batch_entry_mut(k).job {
                    self.journal.apply_progress(job, progress)?;
                }
            }
            BatchEvent::Finished {
                status,
                detail,
                report,
                ..
            } => {
                let job = self.batch_entry_mut(k).job;
                if let Some(job) = job {
                    match (status, report) {
                        (EntryStatus::Succeeded, Some(report)) => {
                            self.job_succeeded(job, report)?
                        }
                        (EntryStatus::Cancelled, _) => self.journal.confirm_cancelled(job)?,
                        _ => self.journal.fail(job, detail.clone())?,
                    }
                }
                let entry = self.batch_entry_mut(k);
                entry.status = *status;
                entry.detail = Some(detail.clone());
            }
        }
        Ok(())
    }

    /// The runner returned: settle the phase from the entries.
    pub fn batch_ended(&mut self) -> Result<BatchPhase, BatchShellError> {
        let batch = self.batch.as_mut().ok_or(BatchShellError::NoBatch("end"))?;
        if !matches!(batch.phase, BatchPhase::Running | BatchPhase::Cancelling) {
            return Err(BatchShellError::WrongPhase(
                phase_name(batch.phase),
                "ended",
            ));
        }
        batch.phase = if batch.resume_point().is_none() {
            BatchPhase::Complete
        } else {
            BatchPhase::Stopped
        };
        Ok(batch.phase)
    }

    /// The batch word for a queue row's pill, when the row is in the
    /// current batch.
    pub fn batch_pill(&self, source_index: usize) -> Option<&'static str> {
        let batch = self.batch.as_ref()?;
        let (k, entry) = batch
            .entries
            .iter()
            .enumerate()
            .find(|(_, e)| e.source_index == source_index)?;
        let resume_here = batch.phase == BatchPhase::Stopped && batch.resume_point() == Some(k);
        Some(match entry.status {
            _ if resume_here => "Resume here",
            EntryStatus::Pending => "In batch",
            EntryStatus::Running => "Rendering",
            EntryStatus::Succeeded => "Batch done",
            EntryStatus::Failed => "Batch failed",
            EntryStatus::Cancelled => "Batch cancelled",
        })
    }

    fn batch_entry_mut(&mut self, k: usize) -> &mut BatchSessionEntry {
        &mut self.batch.as_mut().expect("checked by the caller").entries[k]
    }
}

fn phase_name(phase: BatchPhase) -> &'static str {
    match phase {
        BatchPhase::Running => "running",
        BatchPhase::Cancelling => "cancelling",
        BatchPhase::Stopped => "stopped",
        BatchPhase::Complete => "complete",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rd_core::{
        AttemptOutcome, DeinterlaceEngine, EntryState, ExportPreset, FieldOrder, Operation,
        OutputRate, Recipe,
    };
    use rd_run::{Progress, RunReport};

    use super::*;
    use crate::{InspectStatus, JobStatus};

    const HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    fn deinterlace() -> Recipe {
        Recipe::new("clip.mkv", ExportPreset::PreservationMaster).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::TopFieldFirst,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        })
    }

    /// Three sources: ready, ready, and one with no chain.
    fn shell() -> ShellModel {
        let mut shell = ShellModel::new();
        for (i, name) in ["a.mkv", "b.mkv", "c.mkv"].iter().enumerate() {
            shell.add_to_queue(*name);
            shell.queue[i].fixity = FixityStatus::Verified;
            shell.queue[i].inspect = InspectStatus::Inspected;
            if i < 2 {
                shell.select(i);
                shell.load_recipe(deinterlace());
                // Exports, batched or not, need a previewed chain.
                shell.sources[i].previewed = true;
            }
        }
        shell.select(0);
        shell
    }

    fn report(output: &str) -> RunReport {
        RunReport {
            output: PathBuf::from(output),
            output_sha256: HASH.to_string(),
            frames_processed: 50,
            recipe_path: PathBuf::from(format!("{output}.recipe.json")),
            log_path: PathBuf::from(format!("{output}.export-log.json")),
            warnings: Vec::new(),
        }
    }

    fn started(index: usize, attempt: u32, output: &str) -> BatchEvent {
        BatchEvent::Started {
            index,
            attempt,
            input: PathBuf::from("in.mkv"),
            output: PathBuf::from(output),
        }
    }

    fn finished_ok(index: usize, output: &str) -> BatchEvent {
        BatchEvent::Finished {
            index,
            status: EntryStatus::Succeeded,
            detail: HASH.to_string(),
            report: Some(report(output)),
        }
    }

    fn finished_failed(index: usize) -> BatchEvent {
        BatchEvent::Finished {
            index,
            status: EntryStatus::Failed,
            detail: "render failed: synthetic".to_string(),
            report: None,
        }
    }

    #[test]
    fn candidates_say_which_sources_are_ready_and_why_not() {
        let mut shell = shell();
        shell.queue[1].fixity = FixityStatus::Failed;
        let candidates = shell.batch_candidates();
        assert_eq!(candidates[0].eligibility, BatchEligibility::Ready);
        assert!(matches!(
            &candidates[1].eligibility,
            BatchEligibility::Blocked { reason } if reason.starts_with("Hash mismatch")
        ));
        assert!(matches!(
            &candidates[2].eligibility,
            BatchEligibility::Blocked { reason } if reason.starts_with("No chain yet")
        ));
        assert!(shell.can_start_batch());
    }

    #[test]
    fn a_chain_that_needs_a_preview_is_blocked_until_previewed() {
        let mut shell = shell();
        shell.select(1);
        shell.load_recipe(deinterlace().push(Operation::Denoise {
            engine: rd_core::DenoiseEngine::Hqdn3d,
            luma: 4.0,
            chroma: 3.0,
            temporal: 12.0,
        }));
        assert!(matches!(
            shell.batch_eligibility(1),
            BatchEligibility::Blocked { reason } if reason.starts_with("Preview required")
        ));
        shell.sources[1].previewed = true;
        assert_eq!(shell.batch_eligibility(1), BatchEligibility::Ready);
        // Without a preview, any chain is held back by the export checks.
        shell.sources[0].previewed = false;
        assert!(matches!(
            shell.batch_eligibility(0),
            BatchEligibility::Blocked { reason } if reason.starts_with("Preview this chain")
        ));
    }

    #[test]
    fn begin_refuses_blocked_duplicate_or_empty_choices() {
        let mut shell = shell();
        assert_eq!(
            shell.begin_batch(&[], false),
            Err(BatchShellError::NothingChosen)
        );
        assert!(matches!(
            shell.begin_batch(&[0, 2], false),
            Err(BatchShellError::NotReady { name, .. }) if name == "c.mkv"
        ));
        assert!(matches!(
            shell.begin_batch(&[0, 0], false),
            Err(BatchShellError::Duplicate { .. })
        ));
        assert_eq!(
            shell.begin_batch(&[9], false),
            Err(BatchShellError::UnknownSource(9))
        );
        assert!(
            shell.batch().is_none(),
            "a refused batch leaves nothing behind"
        );
        shell.begin_batch(&[0, 1], false).expect("begin");
        assert_eq!(
            shell.begin_batch(&[0], false),
            Err(BatchShellError::AlreadyRunning)
        );
    }

    #[test]
    fn a_full_run_goes_through_the_journal_and_writes_evidence() {
        let mut shell = shell();
        shell.begin_batch(&[0, 1], false).expect("begin");
        assert_eq!(shell.batch_pill(0), Some("In batch"));

        shell
            .apply_batch_event(&started(1, 1, "a.restored.mkv"))
            .unwrap();
        assert_eq!(shell.batch_pill(0), Some("Rendering"));
        let job = shell.batch().unwrap().entries[0].job.expect("job");
        shell
            .apply_batch_event(&BatchEvent::Progress {
                index: 1,
                progress: Progress {
                    frame: 10,
                    ..Progress::default()
                },
            })
            .unwrap();
        match &shell.journal.get(job).unwrap().status {
            JobStatus::Running(p) => {
                assert_eq!(p.frames_processed, 10);
                assert_eq!(p.percent, None, "no frame total, so no invented percentage");
            }
            other => panic!("expected running, got {other:?}"),
        }
        assert!(shell
            .batch()
            .unwrap()
            .summary_text()
            .contains("rendering 1 of 2"));

        shell
            .apply_batch_event(&finished_ok(1, "a.restored.mkv"))
            .unwrap();
        assert_eq!(
            shell.sources[0].exported_output.as_deref(),
            Some("a.restored.mkv"),
            "a batch export is evidence for Prove, like any export"
        );
        shell
            .apply_batch_event(&started(2, 1, "b.restored.mkv"))
            .unwrap();
        shell
            .apply_batch_event(&finished_ok(2, "b.restored.mkv"))
            .unwrap();
        assert_eq!(shell.batch_ended(), Ok(BatchPhase::Complete));
        assert_eq!(shell.batch_pill(1), Some("Batch done"));
        assert_eq!(
            shell.batch().unwrap().summary_text(),
            "Batch complete · 2 of 2 succeeded"
        );
        assert_eq!(shell.journal.len(), 2);
        assert!(!shell.can_resume_batch());
    }

    #[test]
    fn a_failure_stops_the_batch_and_resume_continues_there() {
        let mut shell = shell();
        shell.begin_batch(&[0, 1], false).expect("begin");
        shell
            .apply_batch_event(&started(1, 1, "a.restored.mkv"))
            .unwrap();
        shell.apply_batch_event(&finished_failed(1)).unwrap();
        assert_eq!(shell.batch_ended(), Ok(BatchPhase::Stopped));
        assert!(
            shell.sources[0].exported_output.is_none(),
            "failure writes no evidence"
        );
        assert_eq!(shell.batch_pill(0), Some("Resume here"));
        assert_eq!(shell.batch_pill(1), Some("In batch"));
        let summary = shell.batch().unwrap().summary_text();
        assert!(summary.contains("entry 1 failed"), "{summary}");
        assert!(shell.can_resume_batch() && !shell.can_cancel_batch());

        assert_eq!(shell.resume_batch(), Ok(0));
        assert_eq!(
            shell.resume_batch(),
            Err(BatchShellError::WrongPhase("running", "resumed"))
        );
        shell
            .apply_batch_event(&started(1, 2, "a.restored.attempt2.mkv"))
            .unwrap();
        let entry = &shell.batch().unwrap().entries[0];
        assert_eq!(entry.attempts, 2);
        assert_eq!(entry.output.as_deref(), Some("a.restored.attempt2.mkv"));
        shell
            .apply_batch_event(&finished_ok(1, "a.restored.attempt2.mkv"))
            .unwrap();
        shell
            .apply_batch_event(&started(2, 1, "b.restored.mkv"))
            .unwrap();
        shell
            .apply_batch_event(&finished_ok(2, "b.restored.mkv"))
            .unwrap();
        assert_eq!(shell.batch_ended(), Ok(BatchPhase::Complete));
    }

    #[test]
    fn cancel_drains_through_cancelling_and_is_resumable() {
        let mut shell = shell();
        shell.begin_batch(&[0, 1], false).expect("begin");
        shell
            .apply_batch_event(&started(1, 1, "a.restored.mkv"))
            .unwrap();
        shell.request_batch_cancel().expect("cancel");
        assert_eq!(shell.batch().unwrap().phase, BatchPhase::Cancelling);
        let job = shell.batch().unwrap().entries[0].job.unwrap();
        assert_eq!(shell.journal.get(job).unwrap().status.phase(), "cancelling");
        assert!(shell.request_batch_cancel().is_err(), "one cancel per run");
        shell
            .apply_batch_event(&BatchEvent::Finished {
                index: 1,
                status: EntryStatus::Cancelled,
                detail: "cancelled by operator".into(),
                report: None,
            })
            .unwrap();
        assert_eq!(shell.journal.get(job).unwrap().status.phase(), "cancelled");
        assert_eq!(shell.batch_ended(), Ok(BatchPhase::Stopped));
        assert_eq!(shell.batch_pill(0), Some("Resume here"));
    }

    #[test]
    fn skipped_entries_count_as_succeeded() {
        let mut shell = shell();
        shell.begin_batch(&[0, 1], false).expect("begin");
        shell
            .apply_batch_event(&BatchEvent::Skipped {
                index: 1,
                output: PathBuf::from("a.restored.mkv"),
            })
            .unwrap();
        assert_eq!(shell.batch().unwrap().succeeded(), 1);
        assert_eq!(shell.journal.len(), 0, "a skip renders nothing");
    }

    #[test]
    fn events_for_unknown_entries_are_errors() {
        let mut shell = shell();
        assert_eq!(
            shell.apply_batch_event(&started(1, 1, "x.mkv")),
            Err(BatchShellError::NoBatch("update"))
        );
        shell.begin_batch(&[0], false).expect("begin");
        assert_eq!(
            shell.apply_batch_event(&started(3, 1, "x.mkv")),
            Err(BatchShellError::UnknownEntry(3))
        );
        assert_eq!(
            shell.apply_batch_event(&started(0, 1, "x.mkv")),
            Err(BatchShellError::UnknownEntry(0))
        );
    }

    #[test]
    fn restore_maps_saved_state_and_shows_interrupted_work() {
        let mut state = BatchState::new(
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
        );
        state.begin_attempt(0, "t0");
        state.finish_attempt(
            0,
            AttemptOutcome::Succeeded {
                output_sha256: HASH.into(),
            },
            "t1",
        );
        state.begin_attempt(1, "t2");

        let mut shell = shell();
        assert!(matches!(
            shell.restore_batch(&state, &[0], false),
            Err(BatchShellError::StateMismatch { state: 2, given: 1 })
        ));
        shell
            .restore_batch(&state, &[0, 1], false)
            .expect("restore");
        let batch = shell.batch().unwrap();
        assert_eq!(batch.phase, BatchPhase::Stopped);
        assert_eq!(batch.entries[0].status, EntryStatus::Succeeded);
        assert_eq!(batch.entries[1].status, EntryStatus::Failed);
        assert_eq!(
            batch.entries[1].detail.as_deref(),
            Some(rd_core::batch::INTERRUPTED_ERROR)
        );
        assert_eq!(shell.batch_pill(1), Some("Resume here"));
    }

    #[test]
    fn batch_state_survives_a_shell_round_trip() {
        let mut shell = shell();
        shell.begin_batch(&[0, 1], true).expect("begin");
        let json = serde_json::to_string(&shell).expect("serialize");
        let back: ShellModel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.batch(), shell.batch());
        // Shell state saved before batches existed has no `batch` key.
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("value");
        value.as_object_mut().expect("object").remove("batch");
        let old: ShellModel = serde_json::from_value(value).expect("state without a batch loads");
        assert!(old.batch().is_none());
    }
}
