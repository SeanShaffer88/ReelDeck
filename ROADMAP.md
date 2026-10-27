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
- [ ] Per-source recipes in the shell model; per-file undo/redo

## v0.3 — The difficult-footage release

- Two-pass stabilization with transforms saved in the project
- Denoise (hqdn3d/nlmeans) and finish tools (crop/scale/levels/deflicker)
- [x] Rerun-a-recipe (`reeldeck rerun`, with a determinism report
  against the recorded run) + verify (hash comparison,
  `reeldeck verify`)
- [ ] Batch queue
- VapourSynth/QTGMC as the optional archival tier

## v0.4 — Cutting and throughput

- Lossless, keyframe-snapped segment export (stream copy) with its own
  proof, kept distinct from a re-encode
- Batch queue: per-file state, resume at the failed file, copy settings
- Proxy media for scrubbing long captures
- Chapter and attachment preservation for Matroska masters
- Scene-change detection as timeline markers
- Quality-control overlays for review copies (never in a preservation
  master)

## Later / maybe never

- Plug-in filters
- Experimental synthesis module (labelled, opt-in, excluded from
  Preservation exports)
- Network render nodes

## Explicit non-goals

Inventing detail in a preservation export; one-click "enhance" with no
record of what happened; cloud processing, accounts, or telemetry of any
kind; modifying or overwriting a source file, ever.
