# AGENTS.md — working agreements for ReelDeck

This file records how work is done in this repository. It applies to human
and automated contributors alike.

## The contract

- The design contract is `docs/ARCHITECTURE_AND_UX_PLAN.md`. Read it before
  substantial work. If implementation reveals the plan is wrong, change the
  plan in the same change and say why.
- Small, reviewable steps. Each change does one thing and leaves the tree
  green: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace`, `cargo build --workspace --release`.

## Engineering standards

- Safe Rust. `unsafe` is forbidden in `rd-core`, `rd-analyze`,
  `rd-pipeline`, and `rd-ui` (`#![forbid(unsafe_code)]`); any future
  platform/FFI crate must scope `unsafe` narrowly, with each block
  commented with its invariants, and justify its existence first.
- Small, reputable dependency set; stdlib first. The workspace currently
  depends only on serde, serde_json, and thiserror. A new dependency
  requires a written justification in the change description.
- Pure logic goes in `rd-core` with unit tests. Parsing goes in
  `rd-analyze`. Pipeline planning goes in `rd-pipeline` and never executes
  a process. The UI holds state and presentation, no business logic.
- Long-running work runs off the UI thread and is cancellable. Progress
  reported to the user is measured, never fabricated.
- Local data only, offline-first. No networking, no telemetry, no accounts
  — not behind a flag, not "for diagnostics", not ever.
- Platform: Windows-first desktop, with core/analyze/pipeline kept
  platform-independent and fully testable anywhere.

## Safety rules (product-specific, non-negotiable)

- **Source files are never modified or overwritten.** Outputs, recipes,
  and logs are always new files. Any code path that could write to a
  source path is a bug, not a feature.
- **No operation may invent detail in a Preservation export.** No
  synthesis, hallucination, or generative fill — in preservation, ever;
  elsewhere only as a clearly labelled, opt-in, non-default step whose
  use is recorded in the export log.
- **Every export writes its recipe and its export log** beside the
  output: source and output SHA-256, tool versions, the exact command,
  frame counts, and warnings. An export without its proof is incomplete.
- Suggested findings (interlacing, anomaly flags) are suggestions. The
  operator confirms; the tool never silently acts on a detection.
- A plan that cannot be executed exactly as shown — wrong engine, missing
  optional tier — fails loudly. Silent substitution is forbidden.

## Documentation and reporting

- Documentation names no third-party products. Refer to capabilities
  generically. The engine stack itself may be named where technically
  necessary: FFmpeg, VapourSynth, and QTGMC are components ReelDeck
  actually builds on.
- Report exactly what ran: which checks, on which platforms, with what
  result. Never claim a scenario passed that did not run, and never
  describe a build as production-ready without review and the test-corpus
  validation (plan §7) behind it.
- User-facing copy is honest and plain: what an operation does, what it
  can damage, and how to recover. No absolutes the evidence can't carry.
