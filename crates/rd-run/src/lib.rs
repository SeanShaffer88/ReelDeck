//! ReelDeck render execution.
//!
//! This is the **only** crate that executes renders. `rd-pipeline`
//! plans (pure string building, golden-tested); this crate runs a plan
//! with FFmpeg, streams the *measured* progress the tool reports, and
//! writes the proof beside every output: the recipe as executed and
//! the export log (`<output>.recipe.json`, `<output>.export-log.json`).
//!
//! The honesty rules from the repository's AGENTS.md are enforced
//! here, in code:
//!
//! - A fixity gate runs before anything renders: when the recipe
//!   carries a source SHA-256, the input must hash to it, or the run
//!   refuses. The log's `source_sha256` is always the hash computed
//!   during the run, never a copied claim.
//! - Outputs and sidecars are always new files. Existence is checked
//!   in preflight, sidecars are written with `create_new`, and the
//!   executed command carries `-n` (never overwrite) where the plan
//!   carries `-y` — see [`executed_args`] for why the executed vector
//!   deliberately differs from the plan there.
//! - A failed or cancelled run deletes its own partial output, still
//!   writes its sidecars with `success = false`, and says what
//!   happened in the log's warnings. Nothing fails silently.

#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use rd_core::{is_sha256_hex, sha256_file, sha256_hex, ExportLog, Recipe};
use rd_pipeline::{ExportPlan, PipelineError};

pub mod batch;

// ---------------------------------------------------------------------------
// Timestamps
// ---------------------------------------------------------------------------

/// Current time as ISO-8601 UTC (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// Implemented from `SystemTime` with a civil-from-days conversion so
/// the workspace needs no clock crate.
pub fn now_iso8601_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    iso8601_utc_from_unix(secs)
}

fn iso8601_utc_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Days since 1970-01-01 → (year, month, day), via Howard Hinnant's
/// civil-from-days algorithm (public domain math, no table lookups
/// beyond the era arithmetic).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // day of era, [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era, [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year, [0, 365]
    let mp = (5 * doy + 2) / 153; // month index from March, [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

// ---------------------------------------------------------------------------
// Progress (ffmpeg -progress key=value blocks)
// ---------------------------------------------------------------------------

/// One block of FFmpeg's machine-readable `-progress` output.
///
/// Values are what FFmpeg measured; a field that never appeared in a
/// block keeps its default rather than an invented number.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub frame: u64,
    pub fps: f64,
    pub out_time_us: i64,
    pub total_size: u64,
    pub dup_frames: u64,
    pub drop_frames: u64,
    pub speed: f64,
    /// True on the final block (`progress=end`).
    pub end: bool,
}

/// Parse a whole `-progress` stream into its blocks. A block ends at
/// its `progress=` line; an unterminated trailing block is incomplete
/// and dropped. Unknown keys (bitrate, out_time_ms, stream stats, …)
/// are ignored, and malformed numbers leave the field at its current
/// value — parsing progress must never panic or fail a render.
pub fn parse_progress(text: &str) -> Vec<Progress> {
    let mut blocks = Vec::new();
    let mut cur = Progress::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "frame" => cur.frame = value.parse().unwrap_or(cur.frame),
            "fps" => cur.fps = value.parse().unwrap_or(cur.fps),
            "out_time_us" => cur.out_time_us = value.parse().unwrap_or(cur.out_time_us),
            "total_size" => cur.total_size = value.parse().unwrap_or(cur.total_size),
            "dup_frames" => cur.dup_frames = value.parse().unwrap_or(cur.dup_frames),
            "drop_frames" => cur.drop_frames = value.parse().unwrap_or(cur.drop_frames),
            "speed" => {
                let v = value.strip_suffix('x').unwrap_or(value);
                cur.speed = v.parse().unwrap_or(cur.speed);
            }
            "progress" => {
                cur.end = value == "end";
                blocks.push(cur);
                cur = Progress::default();
            }
            _ => {}
        }
    }
    blocks
}

// ---------------------------------------------------------------------------
// Stream hashes (ffmpeg -f streamhash)
// ---------------------------------------------------------------------------

/// Reduce FFmpeg streamhash output to a single digest of the output's
/// elementary streams.
///
/// The streamhash muxer prints one line per stream, in stream order:
///
/// ```text
/// 0,v,SHA256=8fe294d02bef24d496de17024083117e3fd052b9a59c3b8b5cd64b793003a55d
/// 1,a,SHA256=059866cdd502ec900cb6b8b6c6367ca253f5b2b58fd9bb0be46510f872d20d88
/// ```
///
/// The digest is the SHA-256 of the per-stream hash values in printed
/// order, joined with `'\n'` (no trailing newline). It deliberately
/// covers only the streams: a container may carry per-run metadata
/// (a Matroska Segment UUID and DateUTC) that makes two runs of the
/// same recipe differ as files while their streams are identical.
///
/// Returns `None` for empty or malformed output — a digest is never
/// built from a partial parse.
pub fn parse_streamhash_digest(text: &str) -> Option<String> {
    let mut hashes: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(',');
        let index = parts.next()?;
        let _stream_type = parts.next()?;
        let assignment = parts.next()?;
        if parts.next().is_some() || index.parse::<u32>().is_err() {
            return None;
        }
        let (name, value) = assignment.split_once('=')?;
        if !name.eq_ignore_ascii_case("SHA256") || !is_sha256_hex(value) {
            return None;
        }
        hashes.push(value);
    }
    if hashes.is_empty() {
        return None;
    }
    Some(sha256_hex(hashes.join("\n").as_bytes()))
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a run did not produce a verified output.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("required tool '{0}' is not available on this system; install FFmpeg to render")]
    ToolUnavailable(String),
    #[error("input file does not exist: {0}")]
    InputMissing(PathBuf),
    #[error("path already exists and will not be overwritten: {0}")]
    OutputExists(PathBuf),
    #[error("source hash mismatch: the recipe expects SHA-256 {expected} but the input hashes to {actual}; refusing to render a different source than the recipe describes")]
    SourceHashMismatch { expected: String, actual: String },
    #[error("could not plan the export: {0}")]
    PlanFailed(#[from] PipelineError),
    #[error("could not start ffmpeg: {0}")]
    SpawnFailed(String),
    #[error("render failed: {detail}")]
    RenderFailed { detail: String },
    #[error("run cancelled by operator")]
    Cancelled,
    #[error("could not write the recipe/export-log proof: {detail}")]
    SidecarFailed { detail: String },
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Options and report
// ---------------------------------------------------------------------------

/// A progress callback: called with each [`Progress`] block as
/// FFmpeg reports it.
pub type ProgressCallback = Box<dyn FnMut(&Progress) + Send>;

/// Knobs for one run. The defaults never cancel and report nothing.
pub struct RunOptions {
    /// Cooperative cancellation: set this flag and the run kills the
    /// renderer, removes the partial output, and records the cancel.
    pub cancel: Arc<AtomicBool>,
    /// Called with each progress block as FFmpeg reports it. This is
    /// the surface a UI progress bar should consume: measured, never
    /// fabricated.
    pub on_progress: Option<ProgressCallback>,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            cancel: Arc::new(AtomicBool::new(false)),
            on_progress: None,
        }
    }
}

/// What a successful run produced, including its proof.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunReport {
    pub output: PathBuf,
    pub output_sha256: String,
    pub frames_processed: u64,
    pub recipe_path: PathBuf,
    pub log_path: PathBuf,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Paths and tool facts
// ---------------------------------------------------------------------------

/// The sidecar paths written beside an output: `<output>.recipe.json`
/// and `<output>.export-log.json`.
pub fn sidecar_paths(output: &Path) -> (PathBuf, PathBuf) {
    let mut recipe_os = output.as_os_str().to_owned();
    recipe_os.push(".recipe.json");
    let mut log_os = output.as_os_str().to_owned();
    log_os.push(".export-log.json");
    (PathBuf::from(recipe_os), PathBuf::from(log_os))
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// First line of `ffmpeg -version` (the build banner), when available.
fn ffmpeg_version() -> Option<String> {
    let output = Command::new("ffmpeg").arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
}

/// The stream digest of a finished output, via an FFmpeg stream-copy
/// streamhash pass (`-map 0 -c copy -f streamhash -hash sha256`):
/// demuxing and re-hashing never re-encodes, so the digest covers
/// exactly the elementary streams the render wrote.
///
/// Failure is reported as a plain message, not a [`RunError`]: this
/// pass is an optional addition to the proof of a render that already
/// succeeded, and the caller records the failure as a log warning
/// rather than failing the run over it.
fn output_stream_hash(output: &Path) -> Result<String, String> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(output)
        .args([
            "-map",
            "0",
            "-c",
            "copy",
            "-f",
            "streamhash",
            "-hash",
            "sha256",
            "-",
        ])
        .output()
        .map_err(|e| format!("could not start ffmpeg: {e}"))?;
    if !out.status.success() {
        return Err(format!("ffmpeg exited with {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_streamhash_digest(&text)
        .ok_or_else(|| "ffmpeg streamhash output could not be parsed".to_string())
}

// ---------------------------------------------------------------------------
// The executed vector
// ---------------------------------------------------------------------------

/// Derive the argument vector that will actually run from a plan's
/// vector. Two deliberate, disclosed differences from the plan:
///
/// - `-progress pipe:1 -nostats` is inserted after `-hide_banner` so
///   progress arrives as parseable blocks on stdout instead of the
///   human stats display on stderr.
/// - the plan's `-y` (overwrite) becomes `-n` (never overwrite). The
///   plan describes intent ("write this output"); at execution time
///   the output must still not exist — preflight checks it, and `-n`
///   makes FFmpeg itself refuse if a file appeared in the meantime.
///   The export log records this executed vector verbatim, so the
///   substitution is part of the record, never silent.
fn executed_args(plan_args: &[String]) -> Vec<String> {
    let mut args: Vec<String> = Vec::with_capacity(plan_args.len() + 3);
    for (i, arg) in plan_args.iter().enumerate() {
        args.push(arg.clone());
        if i == 1 {
            // argv[1] is "-hide_banner" in every plan this workspace
            // builds; progress flags belong with the banner flags.
            args.push("-progress".to_string());
            args.push("pipe:1".to_string());
            args.push("-nostats".to_string());
        }
    }
    for arg in &mut args {
        if arg == "-y" {
            *arg = "-n".to_string();
        }
    }
    args
}

// ---------------------------------------------------------------------------
// Run context: everything the success/failure finishers need
// ---------------------------------------------------------------------------

struct RunContext {
    recipe: Recipe,
    input: PathBuf,
    output: PathBuf,
    recipe_path: PathBuf,
    log_path: PathBuf,
    source_sha256: String,
    started_at: String,
    ffmpeg_version: Option<String>,
    executed: Vec<String>,
}

impl RunContext {
    fn build_log(&self) -> ExportLog {
        let mut log = ExportLog::new(
            self.recipe.schema_version,
            self.input.display().to_string(),
            self.source_sha256.clone(),
            self.output.display().to_string(),
            self.recipe.export_preset,
            self.executed.clone(),
            self.started_at.clone(),
        );
        log.ffmpeg_version = self.ffmpeg_version.clone();
        log.notes = self.recipe.notes.clone();
        log
    }

    /// Write both sidecars as new files. The recipe is written first;
    /// if the log then fails, a recipe without a log remains, and the
    /// returned error says so.
    fn write_sidecars(&self, log: &ExportLog) -> std::io::Result<()> {
        let recipe_json = self.recipe.to_json().map_err(std::io::Error::other)?;
        write_new_file(&self.recipe_path, &recipe_json)?;
        let log_json = log.to_json().map_err(std::io::Error::other)?;
        write_new_file(&self.log_path, &log_json)
    }
}

fn write_new_file(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents.as_bytes())
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Plan a recipe and run the export end to end. See [`run_plan`] for
/// the execution semantics shared with previews.
pub fn run_export(
    recipe: &Recipe,
    input: &Path,
    output: &Path,
    opts: RunOptions,
) -> Result<RunReport, RunError> {
    let plan =
        rd_pipeline::export_plan(recipe, &input.to_string_lossy(), &output.to_string_lossy())?;
    run_plan(plan, recipe, input, output, opts)
}

/// Run an already-built plan (an export plan, or a preview plan from
/// `rd_pipeline::preview_plan`) under a recipe.
///
/// Order of operations, deliberately:
///
/// 1. **Preflight** (no process runs): input exists → fixity gate
///    (source hash computed and, when the recipe names one, required
///    to match) → output and sidecar paths must not exist → FFmpeg
///    must be available. Preflight failures return before any
///    sidecar is written, and the existence errors are detectable on
///    a machine without FFmpeg.
/// 2. **Stabilization pass 1**, when the plan has one and its
///    transforms file does not already exist. An existing transforms
///    file is reused and the reuse is recorded as a log warning.
/// 3. **The render**, streaming progress to `opts.on_progress`, with
///    cooperative cancellation.
/// 4. **Proof**: on success the output is hashed — its file bytes,
///    and its elementary streams via a stream-copy streamhash pass
///    (whose failure is recorded as a log warning, never a failed
///    run) — and both sidecars are written; on failure/cancel the
///    partial output is deleted and the sidecars record
///    `success = false` and what happened.
pub fn run_plan(
    plan: ExportPlan,
    recipe: &Recipe,
    input: &Path,
    output: &Path,
    mut opts: RunOptions,
) -> Result<RunReport, RunError> {
    let started_at = now_iso8601_utc();
    let (recipe_path, log_path) = sidecar_paths(output);

    // -- Preflight ---------------------------------------------------
    if !input.exists() {
        return Err(RunError::InputMissing(input.to_path_buf()));
    }
    let source_sha256 = sha256_file(input)?;
    if let Some(expected) = &recipe.source_sha256 {
        if !expected.eq_ignore_ascii_case(&source_sha256) {
            return Err(RunError::SourceHashMismatch {
                expected: expected.clone(),
                actual: source_sha256,
            });
        }
    }
    for path in [output, &recipe_path, &log_path] {
        if path.exists() {
            return Err(RunError::OutputExists(path.to_path_buf()));
        }
    }
    if !ffmpeg_available() {
        return Err(RunError::ToolUnavailable("ffmpeg".to_string()));
    }

    let ctx = RunContext {
        recipe: recipe.clone(),
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        recipe_path,
        log_path,
        source_sha256,
        started_at,
        ffmpeg_version: ffmpeg_version(),
        executed: executed_args(&plan.args),
    };

    // -- Stabilization pass 1 ----------------------------------------
    let mut extra_warnings: Vec<String> = Vec::new();
    if let Some(stab) = &plan.stabilization {
        if Path::new(&stab.transforms_file).exists() {
            extra_warnings.push(format!(
                "reused existing transforms file {} (stabilization analysis not re-run)",
                stab.transforms_file
            ));
        } else {
            match run_pass1(&stab.pass1_args, &opts.cancel) {
                Ok(()) => {}
                Err(RunError::SpawnFailed(msg)) => return Err(RunError::SpawnFailed(msg)),
                Err(err) => {
                    return Err(finish_failure(
                        &ctx,
                        &Progress::default(),
                        extra_warnings,
                        err,
                    ));
                }
            }
        }
    }

    // -- The render ----------------------------------------------------
    let outcome = execute_render(&ctx.executed, &mut opts)?;
    let mut warnings = extra_warnings;
    warnings.extend(outcome.warnings);
    match outcome.result {
        RenderResult::Success => {
            let output_sha256 = sha256_file(&ctx.output)?;
            // The stream-level proof pass. A failure here never fails
            // the completed render — the digest stays `None` and the
            // reason is recorded as a warning in the log, so the gap
            // in the proof is visible rather than silent.
            let (output_stream_sha256, stream_warning) = match output_stream_hash(&ctx.output) {
                Ok(digest) => (Some(digest), None),
                Err(why) => (
                    None,
                    Some(format!("output stream hash could not be computed: {why}")),
                ),
            };
            if let Some(warning) = stream_warning {
                warnings.push(warning);
            }
            let mut log = ctx.build_log();
            log.frames_processed = outcome.latest.frame;
            log.frames_dropped = outcome.latest.drop_frames;
            log.frames_duplicated = outcome.latest.dup_frames;
            log.warnings = warnings;
            log.finish(now_iso8601_utc(), Some(output_sha256.clone()), true);
            log.output_stream_sha256 = output_stream_sha256;
            ctx.write_sidecars(&log)
                .map_err(|e| RunError::SidecarFailed {
                    detail: format!(
                    "the render succeeded and wrote {}, but writing the proof sidecars failed: {e}",
                    ctx.output.display()
                ),
                })?;
            Ok(RunReport {
                output: ctx.output.clone(),
                output_sha256,
                frames_processed: outcome.latest.frame,
                recipe_path: ctx.recipe_path.clone(),
                log_path: ctx.log_path.clone(),
                warnings: log.warnings.clone(),
            })
        }
        RenderResult::Cancelled => Err(finish_failure(
            &ctx,
            &outcome.latest,
            warnings,
            RunError::Cancelled,
        )),
        RenderResult::Failed(detail) => Err(finish_failure(
            &ctx,
            &outcome.latest,
            warnings,
            RunError::RenderFailed { detail },
        )),
    }
}

/// Shared failure/cancel finisher: delete this run's partial output
/// (preflight proved the path did not exist, so anything there now is
/// this run's), record the failure in the log's warnings, write the
/// sidecars with `success = false`, and return the original error —
/// or [`RunError::SidecarFailed`] when even the proof cannot be
/// written, naming both failures.
fn finish_failure(
    ctx: &RunContext,
    latest: &Progress,
    mut warnings: Vec<String>,
    err: RunError,
) -> RunError {
    warnings.push(match &err {
        RunError::Cancelled => "run cancelled by operator".to_string(),
        other => format!("run failed: {other}"),
    });
    if ctx.output.exists() {
        let _ = std::fs::remove_file(&ctx.output);
    }
    let mut log = ctx.build_log();
    log.frames_processed = latest.frame;
    log.frames_dropped = latest.drop_frames;
    log.frames_duplicated = latest.dup_frames;
    log.warnings = warnings;
    log.finish(now_iso8601_utc(), None, false);
    if let Err(e) = ctx.write_sidecars(&log) {
        return RunError::SidecarFailed {
            detail: format!(
                "the run ended with \"{err}\" and writing the proof sidecars also failed: {e}"
            ),
        };
    }
    err
}

// ---------------------------------------------------------------------------
// Process execution
// ---------------------------------------------------------------------------

enum RenderResult {
    Success,
    Cancelled,
    Failed(String),
}

struct RenderOutcome {
    latest: Progress,
    warnings: Vec<String>,
    result: RenderResult,
}

/// Run a stabilization analysis pass: its planned argument vector,
/// verbatim. stderr is drained (an undrained pipe would stall a long
/// analysis) and its tail kept for the failure message. Cancellation
/// is polled while it runs.
fn run_pass1(args: &[String], cancel: &AtomicBool) -> Result<(), RunError> {
    let mut child = Command::new(&args[0])
        .args(&args[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RunError::SpawnFailed(e.to_string()))?;
    let (tx, rx) = mpsc::channel::<String>();
    let stderr_thread = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        })
    });
    let mut tail: Vec<String> = Vec::new();
    let push_tail = |line: String, tail: &mut Vec<String>| {
        tail.push(line);
        if tail.len() > 20 {
            tail.remove(0);
        }
    };
    loop {
        while let Ok(line) = rx.try_recv() {
            push_tail(line, &mut tail);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(t) = stderr_thread {
                    let _ = t.join();
                }
                while let Ok(line) = rx.try_recv() {
                    push_tail(line, &mut tail);
                }
                if status.success() {
                    return Ok(());
                }
                let suffix = tail.last().map(|l| format!(": {l}")).unwrap_or_default();
                return Err(RunError::RenderFailed {
                    detail: format!(
                        "stabilization analysis (pass 1) failed: ffmpeg exited with {status}{suffix}"
                    ),
                });
            }
            Ok(None) => {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(t) = stderr_thread {
                        let _ = t.join();
                    }
                    return Err(RunError::Cancelled);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return Err(RunError::RenderFailed {
                    detail: format!("stabilization analysis (pass 1) could not be polled: {e}"),
                });
            }
        }
    }
}

/// Execute the render vector, streaming progress blocks to the
/// caller's callback as they arrive and collecting warning lines from
/// stderr. Only spawn failure is an `Err` here; everything else is
/// reported in the outcome so the caller can write the failure proof.
fn execute_render(args: &[String], opts: &mut RunOptions) -> Result<RenderOutcome, RunError> {
    let mut child = Command::new(&args[0])
        .args(&args[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RunError::SpawnFailed(e.to_string()))?;

    // stdout thread: accumulate each progress block's lines and hand
    // the parsed block to the main loop as soon as it completes.
    let (prog_tx, prog_rx) = mpsc::channel::<Progress>();
    let stdout_thread = child.stdout.take().map(|stdout| {
        std::thread::spawn(move || {
            let mut block = String::new();
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let is_end = line.starts_with("progress=");
                block.push_str(&line);
                block.push('\n');
                if is_end {
                    if let Some(p) = parse_progress(&block).into_iter().next() {
                        if prog_tx.send(p).is_err() {
                            break;
                        }
                    }
                    block.clear();
                }
            }
        })
    });
    // stderr thread: one line at a time.
    let (err_tx, err_rx) = mpsc::channel::<String>();
    let stderr_thread = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if err_tx.send(line).is_err() {
                    break;
                }
            }
        })
    });

    let mut latest = Progress::default();
    let mut warnings: Vec<String> = Vec::new();
    let mut stderr_tail: Vec<String> = Vec::new();
    let collect_stderr = |line: String, warnings: &mut Vec<String>, tail: &mut Vec<String>| {
        if line.to_lowercase().contains("warning") {
            warnings.push(line.clone());
            if warnings.len() > 100 {
                warnings.remove(0);
            }
        }
        tail.push(line);
        if tail.len() > 20 {
            tail.remove(0);
        }
    };

    let mut cancelled = false;
    let exit_status = loop {
        match prog_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(p) => {
                latest = p;
                if let Some(cb) = opts.on_progress.as_mut() {
                    cb(&p);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        while let Ok(line) = err_rx.try_recv() {
            collect_stderr(line, &mut warnings, &mut stderr_tail);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if !cancelled && opts.cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    cancelled = true;
                }
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(RenderOutcome {
                    latest,
                    warnings,
                    result: RenderResult::Failed(format!("could not poll the ffmpeg process: {e}")),
                });
            }
        }
    };

    if let Some(t) = stdout_thread {
        let _ = t.join();
    }
    if let Some(t) = stderr_thread {
        let _ = t.join();
    }
    // Anything the threads delivered after the last drain.
    while let Ok(p) = prog_rx.try_recv() {
        latest = p;
        if let Some(cb) = opts.on_progress.as_mut() {
            cb(&p);
        }
    }
    while let Ok(line) = err_rx.try_recv() {
        collect_stderr(line, &mut warnings, &mut stderr_tail);
    }

    let result = if cancelled {
        RenderResult::Cancelled
    } else if exit_status.success() {
        RenderResult::Success
    } else {
        let suffix = stderr_tail
            .last()
            .map(|l| format!(": {l}"))
            .unwrap_or_default();
        RenderResult::Failed(format!("ffmpeg exited with {exit_status}{suffix}"))
    };
    Ok(RenderOutcome {
        latest,
        warnings,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_core::{DeinterlaceEngine, ExportPreset, FieldOrder, Operation, OutputRate};

    // -- timestamps ---------------------------------------------------

    #[test]
    fn iso8601_known_epochs() {
        assert_eq!(iso8601_utc_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc_from_unix(1_700_000_000), "2023-11-14T22:13:20Z");
        // Leap day: 2000 was a leap year.
        assert_eq!(iso8601_utc_from_unix(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(
            iso8601_utc_from_unix(951_782_400 + 86_400),
            "2000-03-01T00:00:00Z"
        );
        // Negative input (pre-epoch) still converts sanely.
        assert_eq!(iso8601_utc_from_unix(-1), "1969-12-31T23:59:59Z");
    }

    // -- progress parsing ----------------------------------------------

    const PROGRESS_FIXTURE: &str = "\
frame=10
fps=0.00
stream_0_0_q=0.0
bitrate=N/A
total_size=0
out_time_us=-41666
out_time_ms=-41666
out_time=00:00:00.041666
dup_frames=0
drop_frames=0
speed=N/A
progress=continue
frame=50
fps=25.31
stream_0_0_q=1.0
bitrate=1024.5kbits/s
total_size=524288
out_time_us=1960000
out_time_ms=1960000
out_time=00:00:01.960000
dup_frames=2
drop_frames=3
speed=1.96x
progress=end
";

    #[test]
    fn parses_progress_blocks() {
        let blocks = parse_progress(PROGRESS_FIXTURE);
        assert_eq!(blocks.len(), 2);
        let first = blocks[0];
        assert_eq!(first.frame, 10);
        assert_eq!(first.out_time_us, -41_666);
        assert!(!first.end);
        // "speed=N/A" is malformed for us: the field stays at default.
        assert_eq!(first.speed, 0.0);
        let last = blocks[1];
        assert_eq!(last.frame, 50);
        assert_eq!(last.fps, 25.31);
        assert_eq!(last.total_size, 524_288);
        assert_eq!(last.dup_frames, 2);
        assert_eq!(last.drop_frames, 3);
        assert_eq!(last.speed, 1.96);
        assert!(last.end);
    }

    #[test]
    fn parse_progress_ignores_garbage_and_unterminated_blocks() {
        assert!(parse_progress("").is_empty());
        assert!(parse_progress("not key value\n===\n").is_empty());
        // No progress= terminator: the block is incomplete, dropped.
        assert!(parse_progress("frame=7\nfps=3.0\n").is_empty());
    }

    // -- streamhash parsing ----------------------------------------------
    //
    // Both fixtures are raw stdout captured from FFmpeg 8.1.2 running
    // `ffmpeg -v error -i <file> -map 0 -c copy -f streamhash -hash
    // sha256 -` on lavfi-generated clips (testsrc video, sine audio).
    // The expected digests are the SHA-256 of the per-stream values
    // joined with '\n', computed independently of this crate.

    const STREAMHASH_TWO_STREAMS: &str = "\
0,v,SHA256=8fe294d02bef24d496de17024083117e3fd052b9a59c3b8b5cd64b793003a55d
1,a,SHA256=059866cdd502ec900cb6b8b6c6367ca253f5b2b58fd9bb0be46510f872d20d88
";

    const STREAMHASH_VIDEO_ONLY: &str = "\
0,v,SHA256=8fe294d02bef24d496de17024083117e3fd052b9a59c3b8b5cd64b793003a55d
";

    #[test]
    fn streamhash_digest_two_streams() {
        assert_eq!(
            parse_streamhash_digest(STREAMHASH_TWO_STREAMS).as_deref(),
            Some("96f278d652872808af6b8b211ed5e40d279c04dcd5de4173f72bb2c2fdaaa52b")
        );
    }

    #[test]
    fn streamhash_digest_video_only() {
        assert_eq!(
            parse_streamhash_digest(STREAMHASH_VIDEO_ONLY).as_deref(),
            Some("fb7e487976031248f6cc6b204251d1635c77c5e938280e9cde26c2c1df166ffc")
        );
    }

    #[test]
    fn streamhash_digest_rejects_empty_and_malformed_output() {
        assert_eq!(parse_streamhash_digest(""), None);
        assert_eq!(parse_streamhash_digest("\n"), None);
        // Not a streamhash line at all.
        assert_eq!(parse_streamhash_digest("hello\n"), None);
        // Hash value is not a SHA-256 hex string.
        assert_eq!(parse_streamhash_digest("0,v,SHA256=xyz\n"), None);
        // A different hash algorithm than the one requested.
        assert_eq!(
            parse_streamhash_digest(
                "0,v,MD5=8fe294d02bef24d496de17024083117e3fd052b9a59c3b8b5cd64b793003a55d\n"
            ),
            None
        );
        // One good line does not rescue a malformed second line: no
        // digest is built from a partial parse.
        let mixed = format!("{STREAMHASH_VIDEO_ONLY}garbage\n");
        assert_eq!(parse_streamhash_digest(&mixed), None);
    }

    // -- executed vector -------------------------------------------------

    #[test]
    fn executed_vector_inserts_progress_and_never_overwrites() {
        let plan_args: Vec<String> = [
            "ffmpeg",
            "-hide_banner",
            "-i",
            "in.mkv",
            "-vf",
            "null",
            "-c:v",
            "ffv1",
            "-y",
            "out.mkv",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let executed = executed_args(&plan_args);
        let pos = executed
            .iter()
            .position(|a| a == "-hide_banner")
            .expect("hide_banner present");
        assert_eq!(executed[pos + 1], "-progress");
        assert_eq!(executed[pos + 2], "pipe:1");
        assert_eq!(executed[pos + 3], "-nostats");
        assert!(executed.iter().any(|a| a == "-n"));
        assert!(!executed.iter().any(|a| a == "-y"));
        // Input/output order preserved.
        assert_eq!(executed.last().map(String::as_str), Some("out.mkv"));
    }

    #[test]
    fn sidecar_paths_append_to_full_output_name() {
        let (recipe, log) = sidecar_paths(Path::new("clip.restored.mkv"));
        assert_eq!(recipe, PathBuf::from("clip.restored.mkv.recipe.json"));
        assert_eq!(log, PathBuf::from("clip.restored.mkv.export-log.json"));
    }

    // -- hermetic preflight ------------------------------------------------

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rd-run-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn bwdif_recipe() -> Recipe {
        Recipe::new("in.mkv", ExportPreset::PreservationMaster).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::TopFieldFirst,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        })
    }

    #[test]
    fn preflight_input_missing() {
        let dir = temp_dir("missing");
        let input = dir.join("nope.mkv");
        let output = dir.join("out.mkv");
        let err = run_export(&bwdif_recipe(), &input, &output, RunOptions::default())
            .expect_err("missing input must refuse");
        assert!(matches!(err, RunError::InputMissing(_)), "got {err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preflight_output_exists_wins_and_writes_nothing() {
        let dir = temp_dir("exists");
        let input = dir.join("in.mkv");
        std::fs::write(&input, b"dummy").expect("write input");
        let output = dir.join("out.mkv");
        std::fs::write(&output, b"previous").expect("write existing output");
        let err = run_export(&bwdif_recipe(), &input, &output, RunOptions::default())
            .expect_err("existing output must refuse");
        match err {
            RunError::OutputExists(p) => assert_eq!(p, output),
            other => panic!("expected OutputExists, got {other:?}"),
        }
        let (recipe_path, log_path) = sidecar_paths(&output);
        assert!(!recipe_path.exists(), "no recipe sidecar may be written");
        assert!(!log_path.exists(), "no log sidecar may be written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preflight_source_hash_mismatch_refuses_before_any_spawn() {
        let dir = temp_dir("hashgate");
        let input = dir.join("in.mkv");
        std::fs::write(&input, b"the real source bytes").expect("write input");
        let actual = rd_core::sha256_file(&input).expect("hash input");
        let mut recipe = bwdif_recipe();
        recipe.source_sha256 = Some("0".repeat(64));
        let output = dir.join("out.mkv");
        let err = run_export(&recipe, &input, &output, RunOptions::default())
            .expect_err("wrong pinned hash must refuse");
        match err {
            RunError::SourceHashMismatch {
                expected,
                actual: got,
            } => {
                assert_eq!(expected, "0".repeat(64));
                assert_eq!(got, actual);
            }
            other => panic!("expected SourceHashMismatch, got {other:?}"),
        }
        assert!(!output.exists());
        let (recipe_path, log_path) = sidecar_paths(&output);
        assert!(!recipe_path.exists());
        assert!(!log_path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_report_json_round_trip() {
        let report = RunReport {
            output: PathBuf::from("out.mkv"),
            output_sha256: "a".repeat(64),
            frames_processed: 50,
            recipe_path: PathBuf::from("out.mkv.recipe.json"),
            log_path: PathBuf::from("out.mkv.export-log.json"),
            warnings: Vec::new(),
        };
        let json = serde_json::to_string(&report).expect("serialize report");
        let back: RunReport = serde_json::from_str(&json).expect("deserialize report");
        assert_eq!(report, back);
    }
}
