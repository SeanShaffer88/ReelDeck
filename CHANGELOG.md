# Changelog

All notable changes to ReelDeck are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
