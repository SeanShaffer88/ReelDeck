# Changelog

All notable changes to ReelDeck are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
