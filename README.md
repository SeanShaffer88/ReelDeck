# ReelDeck

**Restore old footage in the open — every knob visible, every export provable.**

ReelDeck is a professional video-restoration toolbox for archivists and small
production teams. It exists for footage that is old, interlaced, noisy, shaky,
or otherwise difficult — and for people who need to say, later and with
evidence, exactly what was done to it.

> **Status: v0.2 in progress.** This repository holds the workspace, the
> recipe/export-log data model, frame-analysis parsing and reporting,
> pipeline planning, render execution with proof sidecars, and a small
> CLI. The GUI shell has not landed yet. Follow the
> [roadmap](ROADMAP.md) as it becomes real, slice by slice.

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
| `crates/rd-core` | Pure logic: recipe + export-log model, validation, SHA-256 fixity, export verification. No FFI. |
| `crates/rd-analyze` | ffprobe / signalstats / detect-filter / idet parsing into a typed inspection model, plus the sampled-range `analyze` pass that runs the tools. |
| `crates/rd-pipeline` | Planning: recipe → filter strings, two-pass stabilization plan, export argument vectors. Plans only; never executes. |
| `crates/rd-run` | Execution: runs plans with FFmpeg (fixity gate, never-overwrite, measured progress, cancellation) and writes the recipe + export-log proof beside every output. |
| `crates/rd-ui` | GUI state model (queue/viewer/chain/jobs). Framework decision deferred — see the plan. |
| `crates/reeldeck` | The `reeldeck` CLI binary: `inspect`, `plan`, `export`, `preview`, `verify`, `rerun`, `hash`, `version`. |

## Building from source

Requirements: Rust 1.80 or newer. Inspection and rendering additionally
need FFmpeg (which provides `ffmpeg`/`ffprobe`) on the PATH at runtime.

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
- [Roadmap](ROADMAP.md)
- [Changelog](CHANGELOG.md)

## License

MIT License. See the LICENSE file in this repository.
