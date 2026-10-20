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
- [ ] GUI shell: queue, viewer, timeline graphs, frame-step to anomaly

## v0.3 — The difficult-footage release

- Two-pass stabilization with transforms saved in the project
- Denoise (hqdn3d/nlmeans) and finish tools (crop/scale/levels/deflicker)
- Batch queue + rerun-a-recipe + verify (hash comparison)
- VapourSynth/QTGMC as the optional archival tier

## Later / maybe never

- Plug-in filters
- Experimental synthesis module (labelled, opt-in, excluded from
  Preservation exports)
- Network render nodes

## Explicit non-goals

Inventing detail in a preservation export; one-click "enhance" with no
record of what happened; cloud processing, accounts, or telemetry of any
kind; modifying or overwriting a source file, ever.
