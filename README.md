# ReelDeck

**Restore old footage in the open — every knob visible, every export provable.**

ReelDeck is a professional video-restoration toolbox for archivists and small
production teams. It exists for footage that is old, interlaced, noisy, shaky,
or otherwise difficult — and for people who need to say, later and with
evidence, exactly what was done to it.

Restoration is the core; editing sits beside it. From v0.4 the roadmap
adds cutting, trimming, splitting, joining, conversion, audio and
subtitle tracks, then a multi-track editor with transitions, effects and
colour tools, capture, and HDR — all under the same rules: sources are
never written, and every export ships its proof. Creative operations
(transitions, effects, grading looks, plugins) are allowed in edit and
access exports, never in a Preservation Master. See the
[roadmap](ROADMAP.md) for what exists today and what is planned.

> **Status: v0.2 in progress.** This repository holds the workspace, the
> recipe/export-log data model, frame-analysis parsing and reporting,
> pipeline planning, render execution with proof sidecars, a small CLI —
> and now the **interface model plus an interactive prototype of the
> shell**. The shipping GUI is not bound to a framework yet; the
> [interface design](docs/UI_DESIGN.md) records that decision honestly.
> Follow the [roadmap](ROADMAP.md) as it becomes real, slice by slice.

## Why not a black box?

Automatic "enhance" tools ask for trust and give back a file. ReelDeck gives
back four things: the output, the **recipe** (every operation, every
parameter, in order), the **export log** (the exact command that ran, tool
versions, frame counts, warnings), and **hashes** of source and output.
Re-running a recipe reproduces the job. Nothing is invented: ReelDeck never
synthesizes detail that was not in the source, and a preservation export may
not contain any operation that does.

## The workflow

**Ingest → Inspect → Decide → Preview → Export → Prove**

1. **Ingest** — add a file; its SHA-256 is computed. Sources are never
   modified or overwritten, ever.
2. **Inspect** — per-frame signal statistics (luma/chroma ranges, temporal
   outliers, out-of-broadcast-range pixels), black/freeze spans, and an
   interlacing *suggestion* the operator confirms.
3. **Decide** — build an ordered chain of manual operations: deinterlace,
   stabilize, denoise, finish. Each step shows what it does, what it can
   damage, and the exact filter string it will run.
4. **Preview** — render a short segment (chosen from the worst frames, not
   the first seconds) and compare before/after before committing.
5. **Export** — one of three intents, never an ambiguous output:
   *Preservation Master* (FFV1 in Matroska), *Mezzanine* (edit-ready), or
   *Access* (viewing/sharing).
6. **Prove** — every export ships with its recipe and export log beside it.

## The interface

The shell is built around the work, not around a single button: a source
queue on the left, the picture and its measurements in the centre, the
restoration chain on the right, jobs along the bottom, and the proof beside
the export. It is dense, dark and keyboard-first, with one restrained
accent, written state everywhere, and consequences printed next to the
controls that cause them.

- **[Interface design](docs/UI_DESIGN.md)** — layout, tokens, components,
  states, interaction rules, accessibility, and the open framework
  decision.
- **[Prototype](crates/rd-ui/prototype/index.html)** — the interface as
  dependency-free HTML/CSS/JS: open the file, no build step and no network.
  Sample project data and simulated jobs are labelled as such.
- **`rd-ui`** — the same rules as testable model code: stage gates, chain
  editing, the preview gate, timeline lanes and marks, preferences, the
  command table, and the pre-export checks. Its filter string comes from
  the same planner the executor runs, so what the panel shows is what would
  be executed.

## Engine

ReelDeck builds on FFmpeg's documented filters (bwdif/yadif deinterlacing,
vidstab two-pass stabilization, hqdn3d/nlmeans denoising, signalstats
analysis), with an optional VapourSynth tier (QTGMC) for archival-grade
deinterlacing. The FFmpeg path is complete without the optional tier, and
the pipeline refuses — rather than silently substituting — when a recipe
asks for an engine it cannot run.

Local only. Offline-first. No accounts, no telemetry, your footage never
leaves the disk.

## Workspace layout

| Crate | Role |
| ----- | ---- |
| `crates/rd-core` | Pure logic: recipe + export-log model, batch manifest + state, validation, SHA-256 fixity, export verification. No FFI. |
| `crates/rd-analyze` | ffprobe / signalstats / detect-filter / idet parsing into a typed inspection model, plus the sampled-range `analyze` pass that runs the tools. Its header-level probe asks MediaKit first and falls back to ffprobe. |
| `crates/rd-mediakit` | Safe wrapper over the MediaKit DLL (vendored under `crates/rd-analyze/native/`): header-level media probing on Windows, an honest unavailable error elsewhere. The workspace's only FFI crate. |
| `crates/rd-pipeline` | Planning: recipe → filter strings, two-pass stabilization plan, export argument vectors. Plans only; never executes. |
| `crates/rd-run` | Execution: runs plans with FFmpeg (fixity gate, never-overwrite, measured progress, cancellation) and writes the recipe + export-log proof beside every output; runs resumable batches. |
| `crates/rd-ui` | Interface state model and design system: workflow stages and gates, per-source chain state with undo/redo, chain editing and guards, timeline lanes/marks, formatting, preferences, commands, pre-export checks — plus a framework-neutral shell prototype. |
| `crates/reeldeck` | The `reeldeck` CLI binary: `inspect`, `plan`, `export`, `preview`, `verify`, `rerun`, `batch`, `cut`, `split`, `join`, `convert`, `metadata`, `hash`, `version`. |

## Building from source

Requirements: Rust 1.80 or newer. Inspection and rendering additionally
need FFmpeg (which provides `ffmpeg`/`ffprobe`) on the PATH at runtime.
On Windows, `media_kit.dll` (vendored in `crates/rd-analyze/native/`)
must sit beside the executable at run time; see that folder's README
for the provenance and hashes.

```sh
cargo build --workspace --release
cargo run -p reeldeck -- hash <file>
cargo run -p reeldeck -- inspect <file>
cargo run -p reeldeck -- plan <recipe.json>
cargo run -p reeldeck -- export <recipe.json>
cargo run -p reeldeck -- preview <recipe.json> --start 60 --duration 10
cargo run -p reeldeck -- verify <output-file>
cargo run -p reeldeck -- rerun <output-file>.export-log.json
```

`inspect` analyzes a sampled range of a file and prints a text
report (probe facts, signal statistics, anomalies, and an interlace
*suggestion* with its basis counts):

```sh
# Whole file, text report
cargo run -p reeldeck -- inspect capture.mkv
# A 30-second sample starting 60 seconds in, as JSON
cargo run -p reeldeck -- inspect capture.mkv --start 60 --duration 30 --json
# Text report, plus a standalone HTML report (always a new file)
cargo run -p reeldeck -- inspect capture.mkv --html report.html
```

`export` renders a recipe end to end. Before anything runs, the
input must hash to the recipe's pinned SHA-256 (when it carries
one), and the output must not already exist — outputs are always
new files. Beside the output, two sidecars are written: the recipe
as executed (`<out>.recipe.json`) and the export log
(`<out>.export-log.json`) recording the exact command that ran,
tool versions, frame counts, warnings, and both hashes:

```sh
# Input defaults to the recipe's source name,
# output to <source-stem>.restored.<ext>
cargo run -p reeldeck -- export recipe.json
# Explicit input/output
cargo run -p reeldeck -- export recipe.json --in capture.mkv --out restored.mkv
```

`preview` renders one segment of a recipe (same fixity gate, same
sidecars; the segment is recorded in the recorded recipe's notes),
so a chain can be judged on the worst seconds before committing to
a full render:

```sh
# Ten seconds starting a minute in; output defaults to <stem>.preview.<ext>
cargo run -p reeldeck -- preview recipe.json --start 60 --duration 10
```

`verify` checks an export against the proof it shipped with. Each
check is reported on its own line — the recorded run succeeded, the
output still hashes to the value its export log recorded, the
source still hashes to the value the recipe pinned, and the
sidecars agree on preset and schema. A check whose evidence is
missing (for example, the source file is no longer where the
recipe said it was) is reported as SKIP, never as PASS:

```sh
cargo run -p reeldeck -- verify restored.mkv
# The whole report as JSON
cargo run -p reeldeck -- verify restored.mkv --json
```

`rerun` closes the loop: it re-executes a recorded export's recipe
(the recipe sidecar next to the export log) beside a new output —
`<stem>.rerun.<ext>` by default — and reports how the new output
compares to the recorded run, at two levels. The export log
records both the output file's SHA-256 and `output_stream_sha256`,
a digest of the elementary streams' hashes, and the verdict names
which level matches:

- **bit-identical** — the files hash the same. For Matroska
  outputs this is the exception, not the rule: the muxer writes a
  per-run Segment UUID and DateUTC into every file.
- **streams bit-identical** — the files differ only in that
  container metadata; the picture and sound streams are
  bit-for-bit what the recorded run produced.
- **streams differ** — both stream digests, both file hashes, and
  both FFmpeg versions are printed, so a changed tool version is
  named as the likely cause instead of the difference passing
  silently.

Rerunning a *failed* run's log is refused; its recipe can still be
run directly with `export`:

```sh
cargo run -p reeldeck -- rerun restored.mkv.export-log.json
```

`batch` renders many recipes in order, one source at a time, each
with its own recipe and export-log proof. The manifest lists the
entries; relative paths resolve against the manifest's folder, and
`input`/`output` default to the recipe's source and
`<source-stem>.restored.<ext>`:

```json
{
  "schema_version": 1,
  "entries": [
    { "recipe": "tape01.recipe.json" },
    { "recipe": "tape02.recipe.json", "input": "captures/tape02.mkv", "output": "masters/tape02.mkv" }
  ]
}
```

Everything is checked before anything renders: every recipe must
load and validate, no two entries may write the same output, and no
output may be any entry's source. Progress is saved after every step
to `<manifest>.state.json` (written atomically), and the batch stops
at the first failure. Running the same command again resumes there:

- entries that already succeeded are skipped, after their outputs are
  re-hashed against the recorded SHA-256 — a missing or changed
  output stops the batch instead of being counted as done;
- a retry writes to a new attempt name (`tape02.attempt2.mkv`), so
  the failed attempt's proof sidecars are never touched;
- an attempt left running by a crash or a killed process is recorded
  as interrupted and retried;
- if the manifest or any recipe changed since the batch started, the
  resume refuses rather than mixing two batches in one record.

Progress lines show a percentage when the runner can state the frame
total up front (the input's probed frame count, doubled when the chain
deinterlaces one frame per field), and the bare frame count when it
cannot.

```sh
cargo run -p reeldeck -- batch batch.json
# Render the remaining entries even after one fails
cargo run -p reeldeck -- batch batch.json --keep-going
# Print the saved state without rendering
cargo run -p reeldeck -- batch batch.json --status
```

`cut` keeps one or more ranges of a file, and `split` writes one file
per part. Times are seconds or `h:mm:ss`; `90-` keeps to the end. There
are two ways to cut, and you always choose one:

- `--lossless` copies the streams untouched (fast, no quality change).
  Copied streams can only start on a keyframe, so the cut starts at the
  last keyframe at or before the time you asked for; the export log
  records both. One range per file — use `split` for several.
- `--preset preservation|mezzanine|access` re-encodes and cuts exactly
  where you asked, and can keep several ranges in one file.

```sh
# Remove the middle: keep 0–1:00 and 2:30 to the end, re-encoded
cargo run -p reeldeck -- cut tape.mkv --keep 0-1:00 --keep 2:30- --preset preservation
# A lossless trim
cargo run -p reeldeck -- cut tape.mkv --keep 10:00-25:00 --lossless
# Three parts in tape.split/, each with its own proof, run as a batch
cargo run -p reeldeck -- split tape.mkv --at 20:00 --at 40:00 --lossless
```

`join` puts several files together in order — any formats FFmpeg reads
(MP4, AVI, MOV, MKV, ASF, …). Each source is pinned by its own SHA-256,
the export log records every one, and `verify` re-checks them all.
Again you choose the mode:

- `--lossless` copies the streams. Every file must share its video
  codec, audio codec and picture size; if not, the join is refused and
  the differences are named.
- `--preset <name>` re-encodes, so mixed formats and codecs join fine.

Files with different picture sizes, or with audio in only some of them,
are refused in both modes: scaling, padding or inventing silence would
change the footage without a recorded decision. When a container's own
timing is uneven, the re-encode may duplicate a frame or two to keep the
rate steady; the export log counts them.

```sh
cargo run -p reeldeck -- join reel1.mp4 reel2.mp4 --lossless
cargo run -p reeldeck -- join tape.avi clip.mov extra.asf --preset mezzanine --out joined.mov
```

`convert` re-encodes a file to the output settings you choose:
container (MP4, MOV, MKV, AVI), video codec (H.264, H.265, MPEG-4,
ProRes, FFV1), audio codec (AAC, MP3, PCM, FLAC, or none), and
optionally picture size, frame rate, and video/audio bitrates. Without
a bitrate, each codec uses a quality-based default. Settings can be
saved as a profile for a device or platform and loaded with
`--settings`; flags override the profile. Examples are in
[`profiles/`](profiles/README.md).

- A combination the container can't carry (ProRes in MP4, PCM audio in
  MP4, …), a bitrate on a codec that isn't bitrate-controlled here, an
  odd picture size, or an `--out` name whose extension disagrees with
  the container is refused with the reason — never adjusted silently.
- Before rendering, the encoders the plan needs are checked against the
  installed FFmpeg build (an MP3 encoder, for example, is missing from
  some builds).
- A size change scales to exactly that size, and a frame-rate change
  drops or repeats frames (nothing is interpolated); both are stated
  before the run and recorded in the export log.

```sh
cargo run -p reeldeck -- convert tape.mkv --settings profiles/web-mp4.json
cargo run -p reeldeck -- convert clip.mov --container avi --size 640x480 --fps 25 --video-bitrate 3000
```

`metadata` shows a file's container metadata, or writes a copy with a
new title, description and tags. The copy is a new file in the same
container with the streams copied untouched; `--clear` drops the
source's own file-level fields first. Containers don't all keep every
field — AVI, for example, keeps only the title — so the output is read
back after the run, and any field that didn't survive is printed and
recorded in the export log.

```sh
cargo run -p reeldeck -- metadata tape11.mp4
cargo run -p reeldeck -- metadata tape11.mp4 --title "Council meeting, 1991" --tag council --tag 1991
```

Recipes that use these edit fields are written as schema 2; schema-1
recipes keep loading and re-running unchanged.

Checks every change must pass (also run by CI):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

## Project docs

- [Architecture & UX Plan](docs/ARCHITECTURE_AND_UX_PLAN.md) — the design
  contract. Read this before writing code.
- [Interface Design](docs/UI_DESIGN.md) — the shell: layout, visual
  language, components, interaction, accessibility, and what is verified.
- [Roadmap](ROADMAP.md)
- [Changelog](CHANGELOG.md)

## License

MIT License. See the LICENSE file in this repository.
