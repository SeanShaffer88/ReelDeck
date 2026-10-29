# Roadmap

ReelDeck ships in small, green-gated steps. Exit criteria come from the
[Architecture & UX Plan](docs/ARCHITECTURE_AND_UX_PLAN.md); a release is
done when its criteria pass, not when its code merely compiles.

## v0.1 — Inspect & Prove (no restoration rendering yet)

- [x] Workspace skeleton: `rd-core`, `rd-analyze`, `rd-pipeline`, `rd-ui`,
  `reeldeck` CLI
- [x] Recipe + export-log schemas (JSON in v0.1) with validation
- [x] SHA-256 fixity (`reeldeck hash`), pure-Rust, vector-tested
- [x] ffprobe / signalstats / blackdetect / freezedetect parsing with
  fixture tests
- [x] Pipeline planning: filter strings, two-pass stabilization plan,
  export argument vectors (golden-tested)
- [x] `reeldeck inspect` report surface (JSON/HTML) over a sampled range
- [x] Export-log writer wired to real runs (`rd-run` writes the
  recipe + export-log sidecars beside every executed output)

## v0.2 — First restoration chain

- [x] Deinterlace (bwdif/yadif) executed end to end from a recipe
  (`reeldeck export`)
- [x] Segment preview (operator-chosen segment) via
  `reeldeck preview` (before/after compare lands with the GUI shell)
- [x] Preservation (FFV1/Matroska) + Mezzanine + Access exports
- [x] Interface model: workflow stages with evidence gates, chain editing
  and the canonical module catalog, the preview gate, timeline lanes and
  marks, preferences, the command table, and pre-export checks
- [x] Framework-neutral prototype of the shell (design reference, not the
  shipping binary) — see [Interface Design](docs/UI_DESIGN.md)
- [ ] Shipping shell: framework decision, then binding the real engine
  (measured progress, cancellation) to the layout the prototype proves
- [x] Per-source recipes in the shell model; per-file undo/redo

## v0.3 — The difficult-footage release

- Two-pass stabilization with transforms saved in the project
- Denoise (hqdn3d/nlmeans) and finish tools (crop/scale/levels/deflicker)
- [x] Rerun-a-recipe (`reeldeck rerun`, with a determinism report
  against the recorded run) + verify (hash comparison,
  `reeldeck verify`)
- [x] Batch queue (`reeldeck batch`): per-file state saved after every
  step, stop at the failed file and resume there, re-verified skips
- VapourSynth/QTGMC as the optional archival tier

Editing joins restoration from v0.4 on: a second workspace on the same
engine, under the same proof rules (plan §3.7–§3.10).

## v0.4 — Cut, join, convert

- [x] Trim to a range, remove unwanted parts (keep several ranges), and
  split into one output per range — frame-accurate (re-encode, can carry
  the restoration chain) and lossless (stream copy, keyframe-snapped,
  stated before the run and recorded in the log): `reeldeck cut`,
  `reeldeck split`. Several ranges in one lossless file are not yet
  supported
- [x] Join / merge several sources (AVI, MP4, MOV, MKV, ASF, …) into one
  output, each source pinned by hash and re-verified; lossless when
  codecs and picture size match (`reeldeck join`). Conforming mismatched
  sizes or missing audio is not done yet — those joins are refused
- [x] Format conversion between containers and codecs, with resolution,
  frame rate and bitrate settings and saved profiles; incompatible
  combinations refused with the reason (`reeldeck convert`). Conversion
  settings on cut/split/join outputs are not exposed in the CLI yet
- Metadata editing (title, description, tags)
- [x] Recipe schema 2 for edit operations; schema-1 recipes keep working
- Lossless remux into another container (the stream-copy intent with no
  ranges) exists
- Batch queue: copy settings across sources
- Proxy media for scrubbing long captures
- Chapter and attachment preservation for Matroska masters
- Scene-change detection as timeline markers

## v0.5 — Audio and subtitles

- Add, remove, or replace audio tracks; per-track gain and mixing
- Audio codec choice; loudness measured before and after
- Subtitle tracks: add from file, keep, or remove

## v0.6 — The editor

- Shipping GUI: framework decision, then the shell the prototype proves
- Multi-track timeline with preview, undo, and keyboard-first editing
- Transitions between clips; built-in effects
- Colour correction (levels, white balance, lift/gamma/gain) and grading
- Output profiles for devices and platforms
- Creative operations refused in Preservation Master exports
- Windows, macOS and Linux builds; installer and portable versions
- Quality-control overlays for review copies (never in a preservation
  master)

## v0.7 — Capture

- Capture from USB, HDV/FireWire and capture-card devices, hashed as
  written, with dropped/duplicated frames reported from device counters

## v0.8 — HDR, plugins, advanced audio

- HDR10/HLG pass-through with metadata preserved; HDR grading
- Plugins: name, version and file hash recorded per use; refused in
  Preservation Master; Prove marks plugin-dependent outputs
- Advanced audio editing and mixing tools

## v0.9 — Multi-camera

- Multi-camera editing, angles synced by timecode or audio

## Later / maybe never

- Experimental synthesis module (labelled, opt-in, excluded from
  Preservation exports)
- Network render nodes

## Explicit non-goals

Inventing detail in a preservation export; creative operations or plugins
in a preservation export; one-click "enhance" with no record of what
happened; cloud processing, accounts, or telemetry of any kind; modifying
or overwriting a source file, ever; stereoscopic 3D editing.
