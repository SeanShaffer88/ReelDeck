# Changelog

All notable changes to ReelDeck are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `rd-ui` now depends on `rd-analyze` and `rd-pipeline` (workspace-internal
  crates, no new external dependency). Justification: the interface must
  show the planner's exact filter string and the analysis model's real
  measurements rather than re-deriving either, so the panel cannot disagree
  with what the executor will run.
- `rd-ui` now also depends on `rd-run` (workspace-internal; no new external
  dependency and no dependency cycle, since `rd-run` does not depend back).
  Justification: the job journal consumes the executor's exact `Progress`
  callback payload and `RunReport` outcome types instead of re-deriving
  them, so the model's progress and proof records cannot drift from the
  engine's contract.
- `ShellModel.jobs: Vec<JobSummary>` is replaced by a persisted
  `journal: JobJournal` owning full job records and lifecycle. The field
  is `#[serde(default)]`, so saved shell state written before the journal
  still deserializes; `JobSummary` remains as the bottom-panel projection.
- License: GPL-2.0-only → MIT (maintainer decision, 2026-10-20).

### Added

- Batch queue: `reeldeck batch <manifest.json> [--keep-going]
  [--status]` renders a manifest's recipes in order, each with its
  own recipe and export-log proof. `rd-core` gains the versioned
  manifest and state model (`BatchManifest`, `BatchState`, one
  record per attempt); `rd-run` gains the runner (`rd_run::batch`).
  Every entry is validated before anything renders, and outputs may
  not collide with each other or with any source. The state is saved
  atomically to `<manifest>.state.json` after every transition; the
  batch stops at the first failure (unless `--keep-going`) and the
  next run resumes there. On resume, succeeded outputs are re-hashed
  and a missing or changed one stops the batch; retries write to a new
  attempt name so earlier proof is never touched; attempts left
  running by a crash are recorded as interrupted and retried; and a
  changed manifest or recipe refuses to resume. No new dependency.

- MediaKit probe at intake: `rd-analyze`'s probe now asks MediaKit
  for header-level facts first (container, duration, bitrate, and
  per-stream codec, dimensions, sample rate, and channels) and
  falls back to ffprobe when MediaKit is unavailable, does not
  recognize the container, or reports an error — a MediaKit failure
  is never itself a hard failure. Frame rate and field order are
  not header facts MediaKit reports, so they stay unset in a
  MediaKit-sourced result. The behaviour is reached through the
  existing `reeldeck inspect` command; no new command or flag was
  added. New workspace crate `rd-mediakit` holds the FFI
  declarations and the safe wrapper over the MediaKit DLL
  (workspace-internal dependency, no new external dependency), so
  `rd-analyze` keeps its no-unsafe rule; the MediaKit v0.1.0 DLL
  and import library are vendored under
  `crates/rd-analyze/native/` with their SHA-256 hashes recorded
  there. On non-Windows platforms the MediaKit probe reports
  itself unavailable and the ffprobe path is used exactly as
  before.
- Model-side engine binding: a `JobJournal` in `rd-ui` (`jobs.rs`) that
  owns the job lifecycle `Queued → Running → terminal`, with a visible
  `Cancelling` state while cooperative cancellation propagates. Progress
  is measured only — `JobProgress::from_measured` builds each snapshot
  directly from an `rd_run::Progress` block and derives a percentage only
  from a known nonzero frame total (clamped at 100); without a total it
  stays `None`, rendered as `measuring…`, and nothing is interpolated or
  estimated. Late progress blocks keep arriving while a cancel drains and
  never reset the Cancelling state. Successful runs store a proof-bearing
  `JobOutcome` converted from `rd_run::RunReport` (output, output hash,
  frame count, recipe and export-log sidecars, warnings); the journal
  retains at most 100 entries, evicting the oldest terminal row and never
  an active job. Illegal transitions and unknown ids return `JobError`
  rather than panicking or no-op'ing. Only a succeeded Preview or Export
  writes evidence back to its source (the preview gate opens; the output
  is named for the Prove stage); failed and cancelled jobs change no
  evidence. The model spawns nothing: the worker thread, the
  `Arc<AtomicBool>` cancel flag and the callback wiring belong to the
  shipping shell binding layer. The prototype's simulated driver and job
  panel run the same state machine in `js/model.js`, including the
  Cancelling drain, six lifecycle words and the 100-entry cap.
- Per-source shell state in `rd-ui`: `ShellModel` now carries one
  `SourceState` per queue row (parallel to the queue) instead of a single
  chain, so each source keeps its own recipe, preview evidence, expert
  override, export result, and undo history. Switching sources swaps the
  whole record; one file's settings can never describe another file's
  picture. Legacy shell state without the `sources` field still
  deserializes to an empty store.
- Per-source undo/redo over every chain edit (add, remove, reorder,
  reset, parameter change). Each edit snapshots the recipe first, so
  refused edits and no-op reorders record nothing; a new edit clears the
  redo branch; history is bounded at 100 entries per source. Undo and
  redo clear the preview flag because the restored chain has not itself
  been previewed. The command table gains an Edit group (`Mod+Z`,
  `Mod+Shift+Z`), and the prototype renders disabled-until-available
  history buttons with accessible names and shortcut tooltips.
- Continuous-gesture coalescing for undo history. `ShellModel::
  begin_gesture` issues a `GestureId`; parameter replacements tagged
  with the same id (one slider drag from press to release, or one
  numeric-field focus session) collapse into a single undo entry whose
  anchor is the pre-gesture recipe. Separate gestures stay separate even
  on the same control, a refused tick preserves the gesture's anchor,
  and a discrete edit between gestures breaks the chain. The prototype
  opens a gesture on slider press or numeric-field focus and closes it
  on release or blur, syncing the history controls at gesture end.
- Interface model (`rd-ui`) and a framework-neutral shell prototype. The
  model turns the deferred-shell notes into working rules: the six
  workflow stages with evidence gates (each blocked stage names the missing
  evidence and the fix), chain editing over the canonical module catalog
  (blurbs, damage notes, relative costs, conservative defaults, canonical
  insertion order, refused parameters that leave the chain untouched), the
  preview gate for aggressive settings, the exact filter string taken from
  `rd-pipeline` rather than rebuilt, timeline measurement lanes with
  peak-preserving downsampling, anomaly marks and a "worst moments"
  preview-range suggestion, display formatting (non-drop timecode, binary
  units, honest `—` for unknowns), workspace preferences, one command table
  driving bindings/palette/help, and the pre-export checks (chain, preview,
  destination, fixity, disk space) with `UNMEASURED` as a real outcome.
  Alongside the model, `crates/rd-ui/prototype` renders the interface as
  dependency-free HTML/CSS/JS for design review: a dense four-region shell,
  viewer with scopes, field split and a before/after wipe, measurement
  lanes, chain inspector with live filter strings, export preflight, proof
  panel, command palette, preferences dialog and full keyboard operation.
  Sample data and simulated jobs are labelled as such in the interface.
- Interface documentation: [docs/UI_DESIGN.md](docs/UI_DESIGN.md) (layout,
  tokens, components, states, accessibility, verification record, and the
  still-open shipping-framework decision) and an internal capability scan
  under `docs/research/` that records which conventions were adopted,
  queued or rejected, and why.
- Closing the Prove loop: export verification in `rd-core`
  (`verify_export` — the recorded run, the output hash, the source
  hash, the preset, and the schema version, each checked separately
  against the sidecars and the files on disk; a check whose evidence
  is absent is reported as skipped, never passed) and the
  `reeldeck verify` / `reeldeck rerun` subcommands. `verify` checks
  an output against the proof it shipped with and names any failed
  check. `rerun` re-executes a recorded run's recipe beside a new
  output and reports how the result compares to the recorded run
  (see the stream-level verdict below). Rerunning a failed run's
  log is refused (its recipe can still be run directly with
  `export`).
- Stream-level fixity for exports, following a measured finding:
  two runs of the same recipe writing FFV1 in Matroska are not
  byte-identical as files — the muxer writes a per-run Segment UUID
  and DateUTC — while their elementary streams are bit-identical.
  The export log therefore records `output_stream_sha256` next to
  the output file hash: the digest of the output's per-stream
  SHA-256 values, computed by a stream-copy streamhash pass after
  a successful render. A streamhash pass that fails is recorded as
  a warning in the log, never as a failed run; logs written before
  the field existed load with it absent. `rerun`'s verdict is now
  three-level: bit-identical files; streams bit-identical, with
  the file difference named as container metadata; or streams
  differing, reported with both file hashes, both stream digests,
  and both FFmpeg versions, so a changed tool version is named as
  the likely cause rather than the difference passing silently.
  When one side predates stream hashing, the verdict says the
  stream-level comparison is unavailable instead of guessing.
- First executed renders: new `rd-run` crate (the only crate that
  executes renders; `rd-pipeline` still plans only) and the
  `reeldeck export` / `reeldeck preview` subcommands. A run gates on
  source fixity (a recipe's pinned SHA-256 must match the input),
  refuses any output or sidecar path that already exists (the
  executed command also carries FFmpeg's never-overwrite flag),
  streams measured `-progress` blocks to the operator, supports
  cooperative cancellation, runs the two-pass stabilization
  analysis when a recipe needs it (reusing an existing transforms
  file, recorded in the log), and on failure deletes its partial
  output while still writing the proof with `success = false`.
- Export-log writer wired to real runs: every executed export and
  preview writes `<output>.recipe.json` (the recipe as executed;
  previews record their segment in its notes) and
  `<output>.export-log.json` (the exact executed argument vector,
  FFmpeg version, source/output SHA-256, frame counts including
  dropped/duplicated, warnings, timestamps) beside the output.
- `rd-pipeline::preview_plan`: segment plans with input seeking.
  Stabilization chains are refused a segment preview — their
  analysis pass must see the whole input, so a segment render
  would not show what the full render will do.
- `reeldeck inspect` report surface over a sampled range
  (`--start` / `--duration` / `--tout`): plain-text report by default,
  the full report as JSON with `--json`, and a standalone HTML report
  written to a new file with `--html <file>` (existing files are never
  overwritten). The report carries probe facts, per-frame signal
  statistics aggregated over the range (YAVG / SATAVG / TOUT / BRNG
  min/max/mean), black/freeze/temporal-outlier anomalies sorted by
  source position, and an `idet`-based interlace suggestion whose
  basis counts are printed alongside it — findings are suggestions
  for the operator to confirm, never verdicts. `rd-analyze` gained
  the sampled-range model, signal summarization, `idet` parsing, and
  the `analyze` execution pass over ffprobe/ffmpeg.
- Workspace skeleton (v0.1): Rust workspace with `rd-core`, `rd-analyze`,
  `rd-pipeline`, `rd-ui`, and the `reeldeck` CLI binary, licensed
  GPL-2.0-only.
- `rd-core`: restoration recipe model (ordered deinterlace / stabilize /
  denoise / finish operations, three export presets) with validation, the
  export-log model, and a pure-Rust SHA-256 implementation tested against
  the FIPS 180-4 vectors. Recipes serialize as JSON in v0.1.
- `rd-analyze`: typed probe and frame-statistics model; parsers for ffprobe
  JSON, signalstats frame metadata, and the blackdetect / freezedetect log
  formats; temporal-outlier flagging.
- `rd-pipeline`: filter-graph string generation, the two-pass
  stabilization plan, and full export argument plans per preset
  (golden-tested; planning only, nothing executes yet).
- `rd-ui`: shell state model for the future GUI (queue / viewer / chain /
  jobs). No GUI framework yet; the decision is deliberately deferred.
- `reeldeck` CLI: `inspect`, `plan`, `hash`, and `version` subcommands.
- Project documentation: README, AGENTS.md working agreements, roadmap,
  and the Architecture & UX Plan as the design contract.

### Fixed

- Prototype: selecting the fully inspected "Church service" sample source
  crashed the source panel because its sample inspection record was marked
  done over the full file but carried no analyzed-frame count, which the
  panel renders as a measurement basis. The record now carries its frame
  count (47,125 at 25 fps), matching every other completed inspection.
- Stabilization analysis plan (`vidstabdetect`) aborted outright on
  multi-plane RGB sources (e.g. FFV1 preservation captures) — found
  the first time the plan was executed for real. The analysis pass
  now converts to `yuv420p` for measurement only; the export chain
  that reaches the output is unchanged.
- The stabilization transforms file was planned with double quotes
  (`result="transforms.trf"`), which FFmpeg's filter parser treats
  as literal characters — runs created a file actually named
  `"transforms.trf"`, and existence checks for `transforms.trf`
  never matched. The plan now names it unquoted (it is this
  workspace's own constant, with no filter-special characters).
