//! The batch runner: renders a manifest's entries in order, one at a
//! time, saving the batch state after every transition so a stopped
//! batch resumes at the entry that did not finish.
//!
//! Rules, enforced here:
//!
//! - **Everything is checked before anything renders.** The manifest,
//!   every recipe, and the resolved paths are validated up front; two
//!   entries may not write the same output, and no output may be any
//!   entry's input. A resume also re-hashes every output recorded as
//!   succeeded and refuses if one is missing or changed, rather than
//!   reporting work as done that the disk no longer shows.
//! - **A resume continues the same batch or refuses.** The state
//!   records the manifest's hash and each recipe's hash; if either
//!   changed, the runner stops and says so instead of mixing two
//!   batches in one record.
//! - **Retries never touch earlier proof.** Each attempt writes to its
//!   own output name (see `rd_core::attempt_output_path`), so a failed
//!   attempt's sidecars stay where they are.
//! - **The state is written atomically** (temporary file, then
//!   rename), so a crash mid-save leaves the previous state intact.
//! - By default the batch **stops at the first failure**, so it
//!   resumes at the failed file; `keep_going` runs the rest first.
//!   A cancel always stops it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use thiserror::Error;

use rd_core::{
    sha256_file, sha256_hex, AttemptOutcome, BatchManifest, BatchState, EntryState, EntryStatus,
    Recipe,
};

use crate::{now_iso8601_utc, run_export, Progress, RunError, RunOptions, RunReport};

/// Why a batch could not start or continue. Per-entry render failures
/// are not errors at this level: they are recorded in the state and
/// reported in the [`BatchReport`].
#[derive(Debug, Error)]
pub enum BatchRunError {
    #[error("cannot read the batch manifest {path}: {source}")]
    ManifestUnreadable {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a valid batch manifest: {detail}")]
    ManifestInvalid { path: PathBuf, detail: String },
    #[error("entry {index}: {detail}")]
    EntryInvalid { index: usize, detail: String },
    #[error("{detail}")]
    PathConflict { detail: String },
    #[error("the batch state {path} cannot be used: {detail}")]
    StateInvalid { path: PathBuf, detail: String },
    #[error("{detail}; to start this batch over, move {path} aside (it is the record of the earlier run)")]
    StateMismatch { path: PathBuf, detail: String },
    #[error("entry {index} is recorded as succeeded, but {detail}; refusing to continue a batch whose finished work no longer checks out")]
    SucceededOutputChanged { index: usize, detail: String },
    #[error("could not save the batch state {path}: {source}")]
    StateWrite {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Something the operator should see while a batch runs.
#[derive(Debug, Clone, PartialEq)]
pub enum BatchEvent {
    /// A previously succeeded entry was re-verified and is skipped.
    Skipped { index: usize, output: PathBuf },
    /// An attempt started.
    Started {
        index: usize,
        attempt: u32,
        input: PathBuf,
        output: PathBuf,
    },
    /// A measured progress block for the running entry.
    Progress { index: usize, progress: Progress },
    /// An attempt ended; `detail` is the output hash or the error, and
    /// `report` is the run's report when it succeeded.
    Finished {
        index: usize,
        status: EntryStatus,
        detail: String,
        report: Option<RunReport>,
    },
}

/// Receives [`BatchEvent`]s. Shared so per-entry progress callbacks,
/// which must be `'static`, can forward into it.
pub type BatchEventSink = Arc<Mutex<dyn FnMut(&BatchEvent) + Send>>;

/// Knobs for a batch run.
pub struct BatchOptions {
    /// Keep rendering later entries after one fails (the batch still
    /// ends unfinished). Off by default: stop at the failed file.
    pub keep_going: bool,
    /// Cooperative cancellation for the whole batch; the running
    /// entry is cancelled and no further entry starts.
    pub cancel: Arc<AtomicBool>,
    pub on_event: Option<BatchEventSink>,
}

impl Default for BatchOptions {
    fn default() -> Self {
        BatchOptions {
            keep_going: false,
            cancel: Arc::new(AtomicBool::new(false)),
            on_event: None,
        }
    }
}

/// How a batch run ended.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchReport {
    pub state_path: PathBuf,
    pub state: BatchState,
    /// Entries that were attempted during this run.
    pub attempted: usize,
    /// Entries skipped because an earlier run already succeeded them.
    pub skipped: usize,
    /// The run ended on a cancel.
    pub cancelled: bool,
}

impl BatchReport {
    /// Every entry has succeeded.
    pub fn is_complete(&self) -> bool {
        self.state.is_complete()
    }
}

/// `<manifest>.state.json`, beside the manifest.
pub fn state_path_for(manifest: &Path) -> PathBuf {
    let mut os = manifest.as_os_str().to_owned();
    os.push(".state.json");
    PathBuf::from(os)
}

/// Read a batch's saved state without running anything, for status
/// reports. `Ok(None)` means the batch has not started.
pub fn load_state(manifest: &Path) -> Result<Option<BatchState>, BatchRunError> {
    let path = state_path_for(manifest);
    if !path.exists() {
        return Ok(None);
    }
    read_state(&path).map(Some)
}

/// Run (or resume) a batch with the real renderer.
pub fn run_batch(manifest: &Path, opts: BatchOptions) -> Result<BatchReport, BatchRunError> {
    run_batch_with(manifest, opts, run_export)
}

/// [`run_batch`] with the per-entry renderer supplied by the caller —
/// the seam the tests use; production callers want [`run_batch`].
pub fn run_batch_with<F>(
    manifest_path: &Path,
    opts: BatchOptions,
    mut render: F,
) -> Result<BatchReport, BatchRunError>
where
    F: FnMut(&Recipe, &Path, &Path, RunOptions) -> Result<RunReport, RunError>,
{
    let state_path = state_path_for(manifest_path);
    let plan = prepare(manifest_path)?;

    // -- Load or create the state ------------------------------------
    let mut state = if state_path.exists() {
        let state = read_state(&state_path)?;
        check_same_batch(&state, &plan, &state_path)?;
        state
    } else {
        BatchState::new(plan.manifest_sha256.clone(), plan.fresh_entries())
    };
    state.recover_interrupted(&now_iso8601_utc());
    write_state(&state_path, &state)?;

    let emit = |event: BatchEvent| {
        if let Some(sink) = &opts.on_event {
            if let Ok(mut f) = sink.lock() {
                f(&event);
            }
        }
    };

    // -- Re-verify finished work before starting new work -------------
    let mut skipped = 0;
    for (i, entry) in state.entries.iter().enumerate() {
        let Some(attempt) = entry.succeeded_attempt() else {
            continue;
        };
        let output = PathBuf::from(&attempt.output);
        let recorded = attempt.output_sha256.clone().unwrap_or_default();
        let detail = match sha256_file(&output) {
            Ok(actual) if actual.eq_ignore_ascii_case(&recorded) => None,
            Ok(actual) => Some(format!(
                "its output {} now hashes to {actual}, not the recorded {recorded}",
                output.display()
            )),
            Err(e) => Some(format!(
                "its output {} cannot be read: {e}",
                output.display()
            )),
        };
        if let Some(detail) = detail {
            return Err(BatchRunError::SucceededOutputChanged {
                index: i + 1,
                detail,
            });
        }
        skipped += 1;
        emit(BatchEvent::Skipped {
            index: i + 1,
            output,
        });
    }

    // -- Render what is left, in order ----------------------------------
    let mut attempted = 0;
    let mut cancelled = false;
    for i in 0..state.entries.len() {
        if state.entries[i].status == EntryStatus::Succeeded {
            continue;
        }
        if opts.cancel.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        let attempt = state.begin_attempt(i, &now_iso8601_utc());
        let (number, output) = (attempt.number, PathBuf::from(&attempt.output));
        write_state(&state_path, &state)?;
        attempted += 1;
        let input = PathBuf::from(&state.entries[i].input);
        emit(BatchEvent::Started {
            index: i + 1,
            attempt: number,
            input: input.clone(),
            output: output.clone(),
        });

        let sink = opts.on_event.clone();
        let index = i + 1;
        let run_opts = RunOptions {
            cancel: Arc::clone(&opts.cancel),
            on_progress: Some(Box::new(move |p: &Progress| {
                if let Some(sink) = &sink {
                    if let Ok(mut f) = sink.lock() {
                        f(&BatchEvent::Progress {
                            index,
                            progress: *p,
                        });
                    }
                }
            })),
        };
        let mut succeeded_report = None;
        let outcome = match render(&plan.recipes[i], &input, &output, run_opts) {
            Ok(report) => {
                let output_sha256 = report.output_sha256.clone();
                succeeded_report = Some(report);
                AttemptOutcome::Succeeded { output_sha256 }
            }
            Err(RunError::Cancelled) => AttemptOutcome::Cancelled,
            Err(e) => AttemptOutcome::Failed {
                error: e.to_string(),
            },
        };
        let detail = match &outcome {
            AttemptOutcome::Succeeded { output_sha256 } => output_sha256.clone(),
            AttemptOutcome::Failed { error } => error.clone(),
            AttemptOutcome::Cancelled => "cancelled by operator".to_string(),
        };
        state.finish_attempt(i, outcome, &now_iso8601_utc());
        write_state(&state_path, &state)?;
        let status = state.entries[i].status;
        emit(BatchEvent::Finished {
            index,
            status,
            detail,
            report: succeeded_report,
        });

        match status {
            EntryStatus::Cancelled => {
                cancelled = true;
                break;
            }
            EntryStatus::Failed if !opts.keep_going => break,
            _ => {}
        }
    }

    Ok(BatchReport {
        state_path,
        state,
        attempted,
        skipped,
        cancelled,
    })
}

// ---------------------------------------------------------------------------
// Preparation: everything checked before anything renders
// ---------------------------------------------------------------------------

struct PreparedBatch {
    manifest_sha256: String,
    recipe_paths: Vec<PathBuf>,
    recipe_hashes: Vec<String>,
    recipes: Vec<Recipe>,
    inputs: Vec<PathBuf>,
    outputs: Vec<PathBuf>,
}

impl PreparedBatch {
    fn fresh_entries(&self) -> Vec<EntryState> {
        (0..self.recipes.len())
            .map(|i| {
                EntryState::new(
                    self.recipe_paths[i].display().to_string(),
                    self.recipe_hashes[i].clone(),
                    self.inputs[i].display().to_string(),
                    self.outputs[i].display().to_string(),
                )
            })
            .collect()
    }
}

fn prepare(manifest_path: &Path) -> Result<PreparedBatch, BatchRunError> {
    let bytes =
        std::fs::read(manifest_path).map_err(|source| BatchRunError::ManifestUnreadable {
            path: manifest_path.to_path_buf(),
            source,
        })?;
    let invalid = |detail: String| BatchRunError::ManifestInvalid {
        path: manifest_path.to_path_buf(),
        detail,
    };
    let text = String::from_utf8(bytes.clone()).map_err(|e| invalid(e.to_string()))?;
    let manifest = BatchManifest::from_json(&text).map_err(|e| invalid(e.to_string()))?;
    manifest.validate().map_err(|e| invalid(e.to_string()))?;

    let base = manifest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        // An empty base keeps paths relative to the working directory
        // without a `./` prefix in what the operator reads.
        .unwrap_or_default();

    let mut plan = PreparedBatch {
        manifest_sha256: sha256_hex(&bytes),
        recipe_paths: Vec::new(),
        recipe_hashes: Vec::new(),
        recipes: Vec::new(),
        inputs: Vec::new(),
        outputs: Vec::new(),
    };
    for (i, entry) in manifest.entries.iter().enumerate() {
        let index = i + 1;
        let entry_err = |detail: String| BatchRunError::EntryInvalid { index, detail };
        let recipe_path = base.join(&entry.recipe);
        let recipe_bytes = std::fs::read(&recipe_path)
            .map_err(|e| entry_err(format!("cannot read recipe {}: {e}", recipe_path.display())))?;
        let recipe = std::str::from_utf8(&recipe_bytes)
            .map_err(|e| e.to_string())
            .and_then(|t| Recipe::from_json(t).map_err(|e| e.to_string()))
            .and_then(|r| r.validate().map(|()| r).map_err(|e| e.to_string()))
            .map_err(|e| {
                entry_err(format!(
                    "{} is not a valid recipe: {e}",
                    recipe_path.display()
                ))
            })?;
        let input = base.join(entry.input.as_deref().unwrap_or(&recipe.source_name));
        let output = match &entry.output {
            Some(out) => base.join(out),
            None => base.join(default_output_name(&recipe)),
        };
        plan.recipe_hashes.push(sha256_hex(&recipe_bytes));
        plan.recipe_paths.push(recipe_path);
        plan.recipes.push(recipe);
        plan.inputs.push(input);
        plan.outputs.push(output);
    }
    check_path_conflicts(&plan.inputs, &plan.outputs)?;
    Ok(plan)
}

/// `capture.mkv` + PreservationMaster → `capture.restored.mkv`, the
/// same default `reeldeck export` uses.
fn default_output_name(recipe: &Recipe) -> String {
    let stem = Path::new(&recipe.source_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    format!("{stem}.restored.{}", recipe.export_preset.extension())
}

/// Two entries may not share an output, and no output may be an input
/// (sources are never written). Compared lexically, and by canonical
/// path where the file already exists.
fn check_path_conflicts(inputs: &[PathBuf], outputs: &[PathBuf]) -> Result<(), BatchRunError> {
    let key = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let input_keys: Vec<PathBuf> = inputs.iter().map(|p| key(p)).collect();
    let output_keys: Vec<PathBuf> = outputs.iter().map(|p| key(p)).collect();
    for (i, out) in output_keys.iter().enumerate() {
        if let Some(j) = output_keys[..i].iter().position(|o| o == out) {
            return Err(BatchRunError::PathConflict {
                detail: format!(
                    "entries {} and {} both write {}; give one of them its own output",
                    j + 1,
                    i + 1,
                    outputs[i].display()
                ),
            });
        }
        if let Some(j) = input_keys.iter().position(|inp| inp == out) {
            return Err(BatchRunError::PathConflict {
                detail: format!(
                    "entry {}'s output {} is entry {}'s source; sources are never written",
                    i + 1,
                    outputs[i].display(),
                    j + 1
                ),
            });
        }
    }
    Ok(())
}

fn check_same_batch(
    state: &BatchState,
    plan: &PreparedBatch,
    state_path: &Path,
) -> Result<(), BatchRunError> {
    let mismatch = |detail: String| BatchRunError::StateMismatch {
        path: state_path.to_path_buf(),
        detail,
    };
    if !state
        .manifest_sha256
        .eq_ignore_ascii_case(&plan.manifest_sha256)
    {
        return Err(mismatch(
            "the manifest has changed since this batch started".to_string(),
        ));
    }
    if state.entries.len() != plan.recipes.len() {
        return Err(mismatch(format!(
            "the saved state has {} entries but the manifest has {}",
            state.entries.len(),
            plan.recipes.len()
        )));
    }
    for (i, entry) in state.entries.iter().enumerate() {
        if !entry
            .recipe_sha256
            .eq_ignore_ascii_case(&plan.recipe_hashes[i])
        {
            return Err(mismatch(format!(
                "entry {}'s recipe {} has changed since this batch started",
                i + 1,
                plan.recipe_paths[i].display()
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// State persistence
// ---------------------------------------------------------------------------

fn read_state(path: &Path) -> Result<BatchState, BatchRunError> {
    let invalid = |detail: String| BatchRunError::StateInvalid {
        path: path.to_path_buf(),
        detail,
    };
    let text = std::fs::read_to_string(path).map_err(|e| invalid(e.to_string()))?;
    let state = BatchState::from_json(&text).map_err(|e| invalid(e.to_string()))?;
    state.validate().map_err(|e| invalid(e.to_string()))?;
    Ok(state)
}

/// Write the state to `<state>.tmp`, flush it to disk, then rename it
/// over the state file, so the saved state is always a complete one.
fn write_state(path: &Path, state: &BatchState) -> Result<(), BatchRunError> {
    let fail = |source: std::io::Error| BatchRunError::StateWrite {
        path: path.to_path_buf(),
        source,
    };
    let json = state
        .to_json()
        .map_err(|e| fail(std::io::Error::other(e)))?;
    let mut tmp_os = path.as_os_str().to_owned();
    tmp_os.push(".tmp");
    let tmp = PathBuf::from(tmp_os);
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp).map_err(fail)?;
        file.write_all(json.as_bytes()).map_err(fail)?;
        file.sync_all().map_err(fail)?;
    }
    std::fs::rename(&tmp, path).map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_core::{DeinterlaceEngine, ExportPreset, FieldOrder, Operation, OutputRate};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rd-run-batch-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn recipe(source: &str) -> Recipe {
        Recipe::new(source, ExportPreset::PreservationMaster).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::TopFieldFirst,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        })
    }

    /// A batch of `sources` in `dir`: one recipe and one source file
    /// each, and a manifest naming them.
    fn write_batch(dir: &Path, sources: &[&str]) -> PathBuf {
        let mut entries = Vec::new();
        for source in sources {
            std::fs::write(dir.join(source), format!("source bytes of {source}")).unwrap();
            let recipe_name = format!("{source}.recipe.json");
            std::fs::write(dir.join(&recipe_name), recipe(source).to_json().unwrap()).unwrap();
            entries.push(rd_core::BatchEntry {
                recipe: recipe_name,
                input: None,
                output: None,
            });
        }
        let manifest = dir.join("batch.json");
        std::fs::write(&manifest, BatchManifest::new(entries).to_json().unwrap()).unwrap();
        manifest
    }

    /// A stand-in renderer: writes the output and reports success,
    /// unless the input's name is in `fail_on`.
    fn fake_render(
        fail_on: Vec<String>,
        calls: Arc<Mutex<Vec<PathBuf>>>,
    ) -> impl FnMut(&Recipe, &Path, &Path, RunOptions) -> Result<RunReport, RunError> {
        move |_recipe, input, output, opts| {
            calls.lock().unwrap().push(output.to_path_buf());
            if let Some(cb) = opts.on_progress {
                let mut cb = cb;
                cb(&Progress {
                    frame: 7,
                    ..Progress::default()
                });
            }
            let name = input.file_name().unwrap().to_string_lossy().into_owned();
            if fail_on.contains(&name) {
                return Err(RunError::RenderFailed {
                    detail: format!("synthetic failure for {name}"),
                });
            }
            if output.exists() {
                return Err(RunError::OutputExists(output.to_path_buf()));
            }
            std::fs::write(output, format!("rendered from {name}")).unwrap();
            Ok(RunReport {
                output: output.to_path_buf(),
                output_sha256: sha256_file(output).unwrap(),
                frames_processed: 7,
                recipe_path: PathBuf::new(),
                log_path: PathBuf::new(),
                warnings: Vec::new(),
            })
        }
    }

    fn calls() -> Arc<Mutex<Vec<PathBuf>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    #[test]
    fn runs_every_entry_and_saves_the_state() {
        let dir = temp_dir("all");
        let manifest = write_batch(&dir, &["a.mkv", "b.mkv"]);
        let events: Arc<Mutex<Vec<BatchEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let opts = BatchOptions {
            on_event: Some(Arc::new(Mutex::new(move |e: &BatchEvent| {
                sink_events.lock().unwrap().push(e.clone());
            }))),
            ..BatchOptions::default()
        };
        let report = run_batch_with(&manifest, opts, fake_render(vec![], calls())).unwrap();
        assert!(report.is_complete());
        assert_eq!((report.attempted, report.skipped), (2, 0));
        assert!(dir.join("a.restored.mkv").exists());
        assert!(dir.join("b.restored.mkv").exists());

        let saved = load_state(&manifest).unwrap().expect("state saved");
        assert_eq!(saved, report.state);
        assert!(!state_path_for(&manifest)
            .with_extension("json.tmp")
            .exists());

        let events = events.lock().unwrap();
        assert!(events.contains(&BatchEvent::Progress {
            index: 2,
            progress: Progress {
                frame: 7,
                ..Progress::default()
            }
        }));
        assert!(matches!(
            events.last(),
            Some(BatchEvent::Finished {
                index: 2,
                status: EntryStatus::Succeeded,
                ..
            })
        ));
    }

    #[test]
    fn stops_at_a_failure_and_resumes_there_with_a_new_output_name() {
        let dir = temp_dir("resume");
        let manifest = write_batch(&dir, &["a.mkv", "b.mkv", "c.mkv"]);

        let first = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec!["b.mkv".into()], calls()),
        )
        .unwrap();
        assert!(!first.is_complete());
        assert_eq!(first.attempted, 2, "stopped at the failed entry");
        let counts = first.state.counts();
        assert_eq!((counts.succeeded, counts.failed, counts.pending), (1, 1, 1));
        assert!(first.state.entries[1].attempts[0]
            .error
            .as_deref()
            .unwrap()
            .contains("synthetic failure"));

        let seen = calls();
        let second = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], seen.clone()),
        )
        .unwrap();
        assert!(second.is_complete());
        assert_eq!((second.skipped, second.attempted), (1, 2));
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                dir.join("b.restored.attempt2.mkv"),
                dir.join("c.restored.mkv")
            ],
            "resume starts at the failed file and never re-renders a finished one"
        );
        assert_eq!(second.state.entries[1].attempts.len(), 2);
    }

    #[test]
    fn keep_going_runs_past_a_failure() {
        let dir = temp_dir("keep-going");
        let manifest = write_batch(&dir, &["a.mkv", "b.mkv", "c.mkv"]);
        let opts = BatchOptions {
            keep_going: true,
            ..BatchOptions::default()
        };
        let report =
            run_batch_with(&manifest, opts, fake_render(vec!["a.mkv".into()], calls())).unwrap();
        let counts = report.state.counts();
        assert_eq!((counts.failed, counts.succeeded), (1, 2));
        assert!(!report.is_complete());
    }

    #[test]
    fn a_cancel_stops_the_batch_and_is_recorded() {
        let dir = temp_dir("cancel");
        let manifest = write_batch(&dir, &["a.mkv", "b.mkv"]);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let render = move |_: &Recipe, _: &Path, _: &Path, _: RunOptions| {
            flag.store(true, Ordering::SeqCst);
            Err(RunError::Cancelled)
        };
        let opts = BatchOptions {
            cancel,
            ..BatchOptions::default()
        };
        let report = run_batch_with(&manifest, opts, render).unwrap();
        assert!(report.cancelled);
        assert_eq!(report.attempted, 1);
        assert_eq!(report.state.entries[0].status, EntryStatus::Cancelled);
        assert_eq!(report.state.entries[1].status, EntryStatus::Pending);
    }

    #[test]
    fn an_interrupted_run_is_retried_on_resume() {
        let dir = temp_dir("interrupted");
        let manifest = write_batch(&dir, &["a.mkv"]);
        // Simulate a crash: the state says attempt 1 is still running.
        let plan = prepare(&manifest).unwrap();
        let mut state = BatchState::new(plan.manifest_sha256.clone(), plan.fresh_entries());
        state.begin_attempt(0, "2026-01-01T00:00:00Z");
        write_state(&state_path_for(&manifest), &state).unwrap();

        let seen = calls();
        let report = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], seen.clone()),
        )
        .unwrap();
        assert!(report.is_complete());
        let attempts = &report.state.entries[0].attempts;
        assert_eq!(
            attempts[0].error.as_deref(),
            Some(rd_core::batch::INTERRUPTED_ERROR)
        );
        assert_eq!(
            *seen.lock().unwrap(),
            vec![dir.join("a.restored.attempt2.mkv")]
        );
    }

    #[test]
    fn a_changed_manifest_or_recipe_refuses_to_resume() {
        let dir = temp_dir("changed");
        let manifest = write_batch(&dir, &["a.mkv", "b.mkv"]);
        run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec!["b.mkv".into()], calls()),
        )
        .unwrap();

        let recipe_path = dir.join("b.mkv.recipe.json");
        let mut edited = recipe("b.mkv");
        edited.notes = "edited after the batch started".into();
        std::fs::write(&recipe_path, edited.to_json().unwrap()).unwrap();
        let err = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], calls()),
        )
        .unwrap_err();
        assert!(matches!(err, BatchRunError::StateMismatch { .. }), "{err}");
        assert!(err.to_string().contains("entry 2's recipe"));

        let mut text = std::fs::read_to_string(&manifest).unwrap();
        text.push('\n');
        std::fs::write(&manifest, text).unwrap();
        let err = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], calls()),
        )
        .unwrap_err();
        assert!(err.to_string().contains("manifest has changed"), "{err}");
    }

    #[test]
    fn a_changed_succeeded_output_refuses_to_resume() {
        let dir = temp_dir("tampered");
        let manifest = write_batch(&dir, &["a.mkv", "b.mkv"]);
        run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec!["b.mkv".into()], calls()),
        )
        .unwrap();
        std::fs::write(dir.join("a.restored.mkv"), "tampered").unwrap();
        let seen = calls();
        let err = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], seen.clone()),
        )
        .unwrap_err();
        assert!(
            matches!(err, BatchRunError::SucceededOutputChanged { index: 1, .. }),
            "{err}"
        );
        assert!(
            seen.lock().unwrap().is_empty(),
            "nothing renders after the refusal"
        );
    }

    #[test]
    fn preflight_rejects_bad_entries_before_rendering() {
        let dir = temp_dir("preflight");
        let manifest = write_batch(&dir, &["a.mkv"]);
        std::fs::write(
            &manifest,
            r#"{"schema_version": 1, "entries": [{"recipe": "a.mkv.recipe.json"}, {"recipe": "missing.json"}]}"#,
        )
        .unwrap();
        let seen = calls();
        let err = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], seen.clone()),
        )
        .unwrap_err();
        assert!(
            matches!(err, BatchRunError::EntryInvalid { index: 2, .. }),
            "{err}"
        );
        assert!(seen.lock().unwrap().is_empty());
        assert!(
            !state_path_for(&manifest).exists(),
            "no state for a batch that never started"
        );
    }

    #[test]
    fn preflight_rejects_shared_outputs_and_outputs_that_are_sources() {
        let dir = temp_dir("conflicts");
        let manifest = write_batch(&dir, &["a.mkv", "b.mkv"]);
        std::fs::write(
            &manifest,
            r#"{"schema_version": 1, "entries": [
                {"recipe": "a.mkv.recipe.json", "output": "same.mkv"},
                {"recipe": "b.mkv.recipe.json", "output": "same.mkv"}]}"#,
        )
        .unwrap();
        let err = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], calls()),
        )
        .unwrap_err();
        assert!(err.to_string().contains("both write"), "{err}");

        std::fs::write(
            &manifest,
            r#"{"schema_version": 1, "entries": [
                {"recipe": "a.mkv.recipe.json"},
                {"recipe": "b.mkv.recipe.json", "output": "a.mkv"}]}"#,
        )
        .unwrap();
        let err = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], calls()),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("sources are never written"),
            "{err}"
        );
    }

    #[test]
    fn explicit_paths_resolve_against_the_manifest_folder() {
        let dir = temp_dir("paths");
        let manifest = write_batch(&dir, &["a.mkv"]);
        std::fs::create_dir_all(dir.join("out")).unwrap();
        std::fs::write(
            &manifest,
            r#"{"schema_version": 1, "entries": [{"recipe": "a.mkv.recipe.json", "input": "a.mkv", "output": "out/a.master.mkv"}]}"#,
        )
        .unwrap();
        let report = run_batch_with(
            &manifest,
            BatchOptions::default(),
            fake_render(vec![], calls()),
        )
        .unwrap();
        assert!(report.is_complete());
        assert!(dir.join("out").join("a.master.mkv").exists());
        assert_eq!(
            report.state.entries[0].input,
            dir.join("a.mkv").display().to_string()
        );
    }
}
