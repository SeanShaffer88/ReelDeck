# AGENTS.md — working agreements for ReelDeck

This file records how work is done in this repository. It applies to human
and automated contributors alike.

## The contract

- The design contract is `docs/ARCHITECTURE_AND_UX_PLAN.md`. Read it before
  substantial work. If implementation reveals the plan is wrong, change the
  plan in the same change and say why.
- Interface work follows `docs/UI_DESIGN.md`: layout, tokens, states,
  keyboard and accessibility rules. The model in `crates/rd-ui/src` is
  authoritative for behaviour; the prototype under
  `crates/rd-ui/prototype` is the visual reference until the shipping
  shell exists. Keep the two in step, and change the design document in
  the same change when the design moves.
- Small, reviewable steps. Each change does one thing and leaves the tree
  green: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace`, `cargo build --workspace --release`.

## Engineering standards

- Safe Rust. `unsafe` is forbidden in `rd-core`, `rd-analyze`,
  `rd-pipeline`, `rd-run`, and `rd-ui` (`#![forbid(unsafe_code)]`); any future
  platform/FFI crate must scope `unsafe` narrowly, with each block
  commented with its invariants, and justify its existence first.
- Small, reputable dependency set; stdlib first. The workspace currently
  depends only on serde, serde_json, and thiserror. A new dependency
  requires a written justification in the change description.
- Pure logic goes in `rd-core` with unit tests. Parsing goes in
  `rd-analyze`. Pipeline planning goes in `rd-pipeline` and never executes
  a process. Process execution for renders lives in `rd-run` only. The UI
  holds state and presentation, no business logic.
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

---

## Development standard

This project follows SEAN's **Universal Windows Desktop Master Prompt** — the standing development standard for all of SEAN's Windows desktop projects, current and future. The full text is maintained by SEAN (ask him for the current version); this section is the in-repo summary, and the project's own instructions add project-specific requirements on top of it.

Core standards:

- Production quality only: no rough demos or placeholder features presented as finished; safe defaults for beginners, explained advanced controls for professionals.
- Accessibility is a requirement: keyboard navigation, accessible names, visible focus, sufficient contrast, no meaning by color alone, specific error messages with a fix path.
- Every consequential action explains its risks, limitations, and recovery options; destructive or irreversible actions are confirmed, previewed where feasible, and backed up or undoable where feasible.
- Data safety: local-first storage, offline-capable core features, versioned data formats, atomic writes, tested migrations, and no silent data loss.
- Verification honesty: compatibility and readiness claims are labeled Verified, Partially verified, or Unverified; never claim a check that did not run.

Standing defaults (a project brief may override a specific default):

- Product-family look: the black/red aesthetic matching InstallScope is preferred, not mandatory.
- Rust is the default for new projects and for new components where it fits; existing stacks are preserved — this is not a rewrite mandate.
- Support as many Windows versions as makes sense for the project; consider Windows 10 support when feasible without materially extending development time. Never claim support that has not been verified.
- Status and task reports include a Progress % and an ETA whenever they can be estimated responsibly.
- Public documentation avoids competitor comparisons by default (internal research may name competitors; license attributions for bundled components are kept).
- License: MIT for SEAN-owned projects; forks retain their upstream license.
