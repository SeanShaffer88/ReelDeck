# ReelDeck — Architecture & UX Plan

This is the design contract for ReelDeck. Implementation follows it; when
implementation proves part of it wrong, the plan changes in the same change,
with the reason stated.

> The goal isn't to make it look like it was shot yesterday. It's to make it
> look like it was shot *well* — and to be able to prove exactly what you
> did to it.

## 1. Positioning

**A transparent, reproducible alternative to black-box automatic enhancers,
for archivists and small production teams.**

| Automatic enhancers (the foil) | ReelDeck |
| --- | --- |
| Models change between versions — same file, different result months later | Pinned tool and filter versions recorded in every export; re-running a recipe reproduces the job |
| Can invent detail (faces, textures) that was never in the source | **Never invents.** Every operation is a documented, inspectable filter with manual controls |
| One "enhance" control, no record of what happened | Every control visible, every decision saved in a human-readable recipe |
| Cloud/subscription; footage leaves the machine | Local, offline-first, no telemetry; footage never leaves the disk |
| The output is the only artifact | Output + recipe + full export log + source/output hashes are the artifact |

**Anti-claims (stated in the product):** no detail synthesis, no
colorization, no frame interpolation in v1. Those invent temporal/spatial
detail and break reproducibility. If synthesis ever arrives, it ships as a
clearly labelled experimental module, off by default, and never in a
Preservation export.

**Restoration first, editing beside it.** ReelDeck is also a video
editor: cutting, trimming, splitting, joining, audio tracks, transitions,
effects, colour work, conversion, metadata, subtitles, and capture (§3.7–
§3.10). Editing is a second workspace on the same engine and under the same
rules: sources are never written, and every export — an edit as much as a
restoration — ships its recipe, export log and hashes. Restoration stays
the core; editing never weakens it. (Maintainer decision, 2026-10-29.)

### Who it is for (in order)

1. **Archivists / digitization technicians** — tape and film transfers;
   need quality control, fixity, and a defensible record.
2. **Small production teams** — rescuing legacy, interlaced, noisy, or shaky
   footage and cutting it into a current piece; need preview-before-commit,
   batch consistency, and an editor that keeps the record.
3. *Not v1:* one-click consumer enhancement. Wrong promise, wrong support
   burden.

## 2. Principles

- Sources are never modified or overwritten. Ever.
- Suggestions are not decisions: detections (interlacing, anomalies) are
  shown as evidence; the operator confirms.
- Every consequential control explains what it does, what it can damage,
  and how to recover, next to the control — not buried in a manual.
- Grain is signal until the operator says otherwise; defaults are
  conservative.
- Honest measurement: progress, speeds, and quality claims in the UI are
  measured on the test corpus or labelled as relative. Nothing fabricated.
- Offline, local-only, no accounts, no telemetry.

## 3. The workflow — Ingest → Inspect → Decide → Preview → Export → Prove

There is never a single "process" button. Restoration is a chain of
decisions, and the UI is shaped like one.

### 3.1 Ingest & fixity

- Add a file; its SHA-256 is computed up front; probe metadata is captured
  (codec, field-order flags, sample/display aspect, colour
  primaries/matrix/transfer, timecode, audio).
- Disk-space preflight before any render, with real numbers (a lossless
  master is typically several times the size of a compressed source).

### 3.2 Inspect (frame analysis)

Per-frame signal data, following established archival quality-control
practice:

- **Signal statistics per frame:** luma/chroma min/avg/max, saturation,
  temporal outliers (dropouts and head clogs look like this), and
  out-of-broadcast-range pixel ratios, plus black-span, freeze-span, and
  scene-change detection.
- **Interlacing analysis** over a sample, reported as
  top-field-first / bottom-field-first / progressive / telecined with the
  evidence shown — a suggestion the operator confirms.
- **Graphs over time** for the key metrics; clicking a spike lands on that
  frame. Thumbnails at anomalies.
- **Viewing modes:** waveform, histogram, vectorscope, field-split (fields
  shown separately), temporal difference.
- **Audio:** integrated loudness / true peak, silence spans, and a drift
  warning when audio and video durations disagree.

### 3.3 Decide (the restoration chain)

Every module shows what it does, what it can damage, its exact generated
filter string (live), and a relative cost. Module order is part of the
recipe.

- **Deinterlace** — engines: bwdif (default; motion-adaptive), yadif (fast),
  QTGMC via VapourSynth (archival tier; slower). Controls: field order
  (auto / force TFF / force BFF), output rate (one frame per field —
  preserves motion — or one per frame), flagged-frames-only vs all.
  Inverse telecine is a separate, explicitly chosen mode for film
  transfers, never confused with deinterlacing. Warning copy lives here:
  *"Wrong field order doesn't fail — it judders. Check the preview on a
  pan before committing."*
- **Stabilize** — two passes, visibly: analysis writes a transforms file
  kept inside the project so any re-run reuses it; the transform pass
  offers smoothing, zoom, tripod mode, interpolation, and border fill.
  The UI states the limits plainly: stabilization smooths camera motion;
  it cannot repair sync loss, rolling-shutter wobble, or signal that was
  never captured. The cropping cost is shown before rendering.
- **Denoise** — luma and chroma always controlled separately: luma,
  chroma, and temporal strengths (hqdn3d for speed, nlmeans for quality),
  each with its damage note (*"High temporal strength smears motion and
  erases film grain first."*).
- **Finish** — crop (with tape-blanking presets), scale with the scaler
  named, colourspace conversion recorded in the log as the decision it is,
  levels/broadcast-range correction, deflicker for film transfers.

### 3.4 Preview before commit

- Any chain renders first on a 10–30 second segment — chosen by the
  operator from the worst frames Inspect found — with split / wipe /
  toggle before-after at 100% and frame-step.
- A full render of an aggressive chain (strong denoise, the archival
  deinterlace tier, high stabilization zoom) requires a preview first,
  or an expert override — and the override is recorded in the export log.

### 3.5 Export — three intents

| Preset | Format | Why |
| --- | --- | --- |
| Preservation Master | FFV1 v3 in Matroska, FLAC audio | Lossless, open, hardware-independent; per-slice CRCs localise corruption. Never a delivery file. |
| Mezzanine / Edit | ProRes 422 HQ (QuickTime) | Drops straight onto an editing timeline. |
| Access / Delivery | H.264 (MP4), CRF-based | Viewing and sharing. Never presented as a master. |

### 3.6 Prove — recipe + export log

Alongside every output, in the same folder:

- **Recipe** — human-readable, diff-able, re-runnable: source SHA-256,
  tool versions (FFmpeg, VapourSynth where used), ordered operations with
  every parameter, export preset, operator notes. (JSON in v0.1; a YAML
  surface syntax may follow without changing the model.)
- **Export log** — the exact argument vector or script that ran, start/end
  times, frames processed/dropped/duplicated, warnings verbatim, output
  SHA-256, output probe data, preview-override events.
- **Re-run invariant:** the same recipe on the same toolchain must
  produce a bit-identical output, or the difference must be documented.
  This is tested, not asserted (§7).

### 3.7 Edit — the second workspace

Editing operations are recipe operations like any other: recorded in
order, with every parameter, re-runnable, and proved.

- **Cut, trim, split, remove.** A recipe can keep one or more ranges of
  its source (trim = one range; removing unwanted parts = the ranges
  between them), in seconds or timecode. *Split* writes one output per
  range, run as a batch so each part has its own proof.
  - **Two cut modes, never confused.** *Frame-accurate* cuts re-encode
    and can carry the restoration chain. *Lossless* cuts copy the streams
    untouched, so they can only fall on keyframes: the planner snaps each
    range to the keyframe at or before its start, says so before the run,
    and records the snapped times in the log. A lossless cut cannot carry
    filters; the planner refuses that combination instead of re-encoding
    silently.
- **Join and merge.** Several sources (AVI, MP4, MOV, MKV, ASF and
  anything else FFmpeg demuxes) into one output, in order. Every source
  is pinned by its own SHA-256 in the recipe. Sources whose streams match
  can be joined losslessly; otherwise the join re-encodes, and the plan
  shows which.
- **Audio tracks.** Add, remove, or replace tracks; per-track gain and
  simple mixing; audio codec choice. Loudness is measured before and
  after, never asserted.
- **Transitions, effects, colour.** Transitions between joined clips;
  built-in effects; colour correction (levels, white balance, lift/gamma/
  gain) and grading. These are **creative operations** (§3.9).
- **Metadata and subtitles.** Container title, description and tags;
  subtitle tracks added from files or kept from the source. Metadata
  edits are recorded like any other operation.
- **Conversion and output settings.** Any FFmpeg-supported container and
  codec pair, with resolution, frame rate, bitrate or quality, and saved
  output profiles for specific devices or platforms. A conversion the
  chosen container cannot hold (for example a codec AVI cannot carry) is
  refused with the reason, not substituted.
- **Timeline.** Multi-track timeline with preview, keyboard-first, every
  edit undoable. Automation: everything the GUI does, the CLI and batch
  manifests can do.
- **Multi-camera editing** (angles synced by timecode or audio) comes last
  on the roadmap; it serves new productions more than restoration.

### 3.8 Capture

Digitising tapes is the start of most restoration work, so capture is
planned after the editing core: USB, HDV/FireWire and capture-card
sources through the platform's capture APIs. Capture writes a new file,
hashes it as it is written, and reports dropped and duplicated frames
from the device's own counters — a capture with drops says so in its log.

### 3.9 Creative operations and plugins

- **Creative operations** (transitions, effects, colour grading looks,
  plugins) change what the picture shows rather than repairing it. A
  recipe containing one cannot be exported as a **Preservation Master**;
  the planner refuses and names the operation. Mezzanine and Access
  exports may use them, and the export log lists each one.
- **Plugins** are supported, fenced: each use records the plugin's name,
  version and file SHA-256 in the export log, a recipe that names a plugin
  fails loudly if that exact file is absent, and Prove marks the output as
  plugin-dependent. (Maintainer decision, 2026-10-29.)

### 3.10 HDR and formats

- **HDR:** HDR10 and HLG sources pass through with their transfer,
  primaries and mastering metadata preserved and recorded; a conversion
  that would drop HDR metadata says so before the run. HDR grading comes
  after the colour tools exist.
- **3D (stereoscopic) editing is out of scope.** (Maintainer decision,
  2026-10-29: little engine support and little remaining use.)

## 4. Architecture

A testable core, a CLI that can do everything the GUI can, and the GUI on
top. If the GUI dies, the recipe still runs.

```
rd-core       Recipe model, job model, export-log schema, validation, hashes
              (pure Rust, #![forbid(unsafe_code)], fully unit-tested)
rd-analyze    ffprobe + signalstats / blackdetect / freezedetect / idet
              parsing -> frame-analysis model (pure parsing, fixture-tested)
rd-pipeline   Filter-graph / VapourSynth-script planning, two-pass
              stabilization plan, export argument plans. Plans, never
              executes in v0.1; execution arrives behind the same plans.
rd-ui         Shell state model (queue / viewer / chain / jobs); no
              framework code yet — see the GUI decision below
reeldeck      CLI binary: inspect | plan | export | preview | verify |
              rerun | batch | hash | version
```

- **Engine:** FFmpeg (a pinned build; note that the stabilization library
  is not present in every distributed build, so the bundled build must be
  verified for it), plus the optional VapourSynth pipeline for the
  QTGMC tier. VapourSynth is optional, never a hard dependency: the
  FFmpeg path is complete without it, and a recipe that needs the
  optional tier fails loudly in its absence instead of substituting.
- **Jobs:** long work off the UI thread, cancellable at any point, queued
  per file so a failed batch resumes at the failed file. Progress comes
  from the engine's machine-readable progress output (real percent,
  frames/sec, ETA from the measured rate).
- **GUI framework decision — deferred, deliberately.** The contest is a
  Rust-native cross-platform shell vs the most native Windows toolkit;
  it is to be settled by inspecting the project's needs against current
  platform guidance and stating the trade-offs before code is written.
  `rd-ui` holds the state model meanwhile so the decision stays cheap.
  Cross-platform targets (below) now weigh in favour of a cross-platform
  shell; the decision record must say how Windows quality is kept.
- **Platforms:** Windows first; macOS and Linux are supported targets.
  Core, analysis, planning and execution stay platform-independent; the
  MediaKit probe is Windows-only and falls back to ffprobe elsewhere.
  Each platform is claimed only once verified on it. Builds ship as an
  installer and as a portable folder (no installer, settings beside the
  binary).
- **Recipe schema 2:** edit operations (kept ranges, multiple sources,
  audio tracks, metadata) extend the recipe. Schema-1 recipes still load
  and re-run unchanged; a recipe using an edit field is written as
  schema 2 so an older build refuses it instead of ignoring the edit.
- **Dependencies:** workspace is serde / serde_json / thiserror only.
  SHA-256 is implemented in `rd-core` rather than adding a crate.
  (Stdlib first; new dependencies need written justification.)

## 5. UI shape

Dense, professional, keyboard-first — an operations board for footage:

- **Left:** project queue (files, fixity status, inspection status).
- **Centre:** viewer (before/after, viewing modes, frame-step,
  field-split) over a timeline graph strip (signal metrics + anomaly
  flags).
- **Right:** the chain — ordered modules, each collapsed to one honest
  summary line ("Deinterlace · bwdif · tff · send_field") and expanding
  to full manual controls plus the live filter string.
- **Bottom:** jobs (progress, cancel, log tail) and the export panel
  (three intents; recipe + log always attached).
- Every interactive control has a tooltip; no essential information is
  carried by tooltip alone. Destructive-strength actions are confirmed.
  Empty, loading, success, error, and disabled states are designed, not
  accidental. Keyboard operation, text scaling, and high-contrast themes
  are requirements, not polish.

## 6. Roadmap summary

- **v0.1 — Inspect & Prove:** schemas, fixity, parsers, planning, CLI,
  GUI state model. No rendering.
- **v0.2 — First chain:** deinterlace executed, segment preview,
  the three exports, GUI shell.
- **v0.3 — Difficult footage:** two-pass stabilization, denoise, finish,
  batch + rerun + verify, QTGMC tier.
- **v0.4 — Cut, join, convert:** trim/split/remove ranges (frame-accurate
  and lossless), join sources, format conversion with output settings,
  metadata.
- **v0.5 — Audio and subtitles:** add/remove/replace tracks, mixing,
  audio codecs, subtitle tracks.
- **v0.6 — The editor:** shipping GUI with a multi-track timeline,
  transitions, effects, colour correction and grading, output profiles;
  macOS and Linux builds; portable builds.
- **v0.7 — Capture.** **v0.8 — HDR, plugins, advanced audio.**
  **v0.9 — Multi-camera.**

## 7. Validation plan

- **Test corpus (build it first):** interlaced DV; a tape capture with
  dropouts and head-switching noise; a telecined film transfer; shaky
  phone video; noisy low-light progressive footage; a clip with
  intentional film grain that must survive default settings.
- **Golden tests:** recipe → exact filter string / argument vector;
  log-schema round-trips; parser fixtures for every analysis format.
- **Determinism test:** the same recipe rendered twice yields the same
  output hash where the encoder is deterministic; any exception is
  documented with its cause.
- **Honesty gate:** no quality or speed number appears in the UI or docs
  unless it was measured on the corpus. Relative labels ("fast"/"slow")
  are labelled as relative.
