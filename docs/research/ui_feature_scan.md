# Internal research: interface conventions across editing and conversion tools

> **Internal document. Not published, not linked from the README, and not
> quoted in the product.** ReelDeck's public documentation names no
> third-party products (AGENTS.md); this note is the working record behind
> that decision, kept so future contributors can see what was considered
> and why.
>
> **No code, assets, icons, layouts, or naming were copied.** Every entry
> below is a *capability* observation, translated into ReelDeck's own
> model and visual language. The tools referenced carry their own licences
> (GPL/LGPL, MIT, proprietary); nothing from them is bundled.

## Why this note exists

The brief was to study a wide field — professional NLEs and grading
suites, open-source video editors, lossless cutters and joiners, encoders
and converters, screen recorders, and browser-based editors — and to adopt
what makes ReelDeck better *for its users*: archivists and small
production teams restoring difficult footage and needing to prove what
they did.

The field splits into two families:

- **Editing/authoring suites** (Resolve, Premiere Pro, Final Cut Pro,
  Media Composer, Lightworks, Kdenlive, Shotcut, OpenShot, Avidemux,
  Blender's sequencer, VideoPad, PowerDirector, Filmora, Movavi, Magix,
  CyberLink, EaseUS, and the web editors CapCut, Clipchamp, Canva, VEED,
  Kapwing, FlexClip, InVideo, plus consumer tools such as iMovie,
  Bandicut, GiliSoft, Boilsoft, AVS, Aiseesoft, 4Videosoft, Gihosoft,
  DVDVideoSoft, Online-Video-Cutter, 123apps, Ezgif).
- **Processing/QC utilities** (FFmpeg, HandBrake, Shutter Encoder,
  XMedia Recode, Any Video Converter, VideoProc, MKVToolNix, mp4joiner,
  mp4tools, Weenysoft, VLC, LosslessCut, VidCutter).

ReelDeck belongs to neither exactly: it is a *restoration* workstation with
an archival proof obligation. So the useful question for each observed
convention was not "is this good?" but **"does this help someone restore
old footage and defend the result?"**

Status legend: **S** shipped in `rd-ui`'s model, **P** present in the
prototype (design proven, not bound to an engine), **Q** queued with a
named slice, **R** rejected for this product.

---

## A. Finding and judging the picture

| Capability | Where it is practised | Decision | ReelDeck | Status |
| --- | --- | --- | --- | --- |
| Waveform, histogram, vectorscope overlays; scope be used while framing | Grading suites; NLEs | Adopt, but keep scopes *over* the picture with a neutral trace colour, because a restoration operator judges motion and noise, not just levels | Viewer modes + canvas scopes | S/P |
| Before/after comparison with a draggable wipe and a toggle | Consumer editors, web editors, NLEs | Adopt with a keyboard-operable wipe; the wipe compares the chain against the untouched source at the same frame and never alters the recipe | `ViewerState.compare`, wipe handle | S/P |
| Field-by-field inspection for interlacing | Archival QC practice, some broadcast tools | Adopt as a first-class viewer mode; ReelDeck's whole premise is interlaced material | `ViewerMode::FieldSplit` | S/P |
| Frame-accurate stepping and a timecode/frames switch | NLEs, lossless cutters | Adopt; non-drop timecode so the number matches the analysis frame index; preference for timecode/clock/frames | `ViewerState`, `prefs::TimeDisplay`, `format::position` | S/P |
| Temporal difference view for dropouts and frozen frames | QC/deinterlace tooling | Adopt as a viewer mode | `ViewerMode::TemporalDifference` | S/P |
| Timeline graphs of signal metrics with click-to-seek | Broadcast QC, audio tools | Adopt, but downsample **peak-preservingly** (min/max per bucket) and label scales and thresholds | `timeline.rs` lanes, prototype drawing | S/P |
| Marker/jump navigation between findings | NLE markers, QC notes | Adopt: findings are marks with glyphs, `N`/`Shift+N`, and a strip | `TimelineModel::next_mark`, marks strip | S/P |
| Suggested "worst frames" segment for review | None of the scanned tools do this well | **ReelDeck's own**: centre a preview range on the highest temporal outlier instead of the first seconds | `suggested_preview_range` | S/P |
| Safe-area/title guides | Broadcast NLEs | Adopt as a preference | `prefs.safe_areas` | S/P |
| Proxy media for smooth scrubbing of long files | NLEs, web editors | Adopt later; it is a performance feature, not a decision surface | — | Q (proxy slice) |
| Audio loudness, true peak, silence, drift checks | NLE meters, loudness tools, QC suites | Partially adopt now (values shown with basis), meters queued | Source panel audio facts | S/P |
| Scene-change detection as markers | NLE scene detection, archival tooling | Queued; analysis already has the primitives | — | Q |
| Burn-in of timecode/metadata for review copies | QC and screener workflows | Queued; must be opt-in and excluded from preservation masters by default | — | Q |

## B. Cutting, trimming, segments

| Capability | Where it is practised | Decision | ReelDeck | Status |
| --- | --- | --- | --- | --- |
| In/out points and a range band on the timeline | Every editor | Adopt for the preview segment; the range is recorded in the preview recipe notes | `RangeSelection` | S/P |
| Suggesting a range rather than defaulting to 0:00 | — | ReelDeck's own (see above) | `suggested_preview_range` | S/P |
| Lossless, keyframe-aware stream-copy trimming | Lossless cutters, remux tools | **Adopt as the next major feature** — archivists cut clips without re-encoding. Needs a `Trim` operation in the recipe (keyframe-snapped, stream copy), its own proof, and a clear distinction from a re-encode | — | Q (lossless segment slice) |
| Joining/concat without re-encode | Joiners, remux tools | Queued with the trim op; the recipe must record segment order and container constraints | — | Q |
| Frame-exact cuts with re-encode | NLEs, encoders | Already possible via the chain; a dedicated cut UI is not a restoration decision | — | — |
| Splitting clips on a timeline, ripple edits | NLEs | **Rejected.** Restoration is not a narrative edit; adding an edit model would dilute the chain and the proof | — | R |
| Speed ramps, transitions, titles, keyframes | Consumer and pro editors | **Rejected** for scope and for the no-synthesis rule | — | R |

## C. The restoration chain (the "effects" surface)

| Capability | Where it is practised | Decision | ReelDeck | Status |
| --- | --- | --- | --- | --- |
| Stacked, reorderable effects with per-effect parameters | NLE effect stacks | Adopt the pattern, but each step states **what it can damage** and its **exact filter string** | `chain.rs` module catalog, step cards | S/P |
| Collapsed summary expanded to full controls | Pro NLEs; "basic/advanced" panels in consumer tools | Adopt as the default interaction: one honest summary line, expanding to precise controls | step cards | S/P |
| Presets that explain themselves | Pro grading/encoding presets | Adopt: rationale text plus measurable facts, never a mystery preset | export intents, module defaults | S/P |
| Disabled controls with a reason | Rare; usually a dead control or a generic tooltip | Adopt strictly: every disabled control names the missing evidence and the fix | stage gates, overrides, VapourSynth engine | S/P |
| Two-pass analysis for stabilization with a saved transforms file | Stabilizer tooling | Adopt, and refuse a segment preview when the analysis must see the whole file (shown, not silently approximated) | `stabilization_plan`, preview gate | S (existing) |
| Automatic "enhance" and AI cleanup | Consumer/web editors | **Rejected** in preservation; permitted only as a labelled, opt-in, recorded module that is excluded from preservation exports | plan §1 | R (except labelled lab module) |
| Generative fill, upscaling that invents detail, frame interpolation | Consumer editors, AI tools | **Rejected** — violates "never invents" | plan §1 | R |
| Colour grading (LUTs, wheels, curves) | Grading suites | **Rejected as an editing surface.** Level correction is a documented Finish operation; creative grading is out of scope | Finish module | R (beyond level correction) |
| Node compositing | Compositing suites | Rejected — wrong user, wrong proof burden | — | R |

## D. Media management, batch, state

| Capability | Where it is practised | Decision | ReelDeck | Status |
| --- | --- | --- | --- | --- |
| Media bin with per-item status badges | NLEs | Adopt: fixity + inspection state per source, counts and totals in the header | queue table | S/P |
| Filter/search the bin | NLEs, web editors | Adopt (simple substring filter); richer search queued | queue filter | P |
| Probe/media-info inspector | Encoders, remux GUIs | Adopt, but show only decisions-relevant facts, with field-order flag and audio facts | Source panel | S/P |
| Per-clip settings | Every NLE | **Adopt** — the prototype swaps chains per source; the model still holds one chain (next slice) | prototype | P / Q |
| Batch queue with per-file progress, cancel, resume at the failed file | Encoders, converters | Adopt in design; runner is a queued slice. Cancellation and partial-file deletion already exist in `rd-run` | Jobs panel | Q |
| Drag-and-drop media | NLEs, web editors | Queued (desktop shell concern) | — | Q |
| Autosave / crash recovery | NLEs | Queued; project format must be atomic and versioned first | — | Q |
| Undo/redo | Every editor | Partially: removal returns the exact operation for undo; a real stack is queued | `chain_remove` returns op | Q |
| Copy settings between clips | NLEs, converters | Queued with per-source recipes (copy chain, record the copy) | — | Q |
| Proxy generation and background conform | NLEs | Queued | — | Q |
| Cloud sync, accounts, team review | Web editors | **Rejected** — local-only, offline-first, no telemetry | plan §2 | R |

## E. Export, encoding, containers

| Capability | Where it is practised | Decision | ReelDeck | Status |
| --- | --- | --- | --- | --- |
| Explicit intent presets (master / mezzanine / delivery) rather than a format list | Encoders, NLEs | Adopt with rationale, container/codec facts, and a size **estimate** | export intents | S/P |
| Preflight before encode: destination free, space, codec compatibility, source integrity | Encoders (vary), archival workflows | Adopt and extend: chain plans, preview gate, fixity pin, free space, sidecar collision | `export_preflight` | S/P |
| Disk-space headroom check with real numbers | Encoders | Adopt, with binary units and an explicit "estimate" | preflight | S/P |
| Queue multiple encodes | Encoders | Queued (batch) | — | Q |
| Container/chapter/metadata editing | MKVToolNix-class tools | Queued for preservation masters (attachment and chapter preservation are real archival needs) | — | Q |
| Hardware acceleration | Encoders, NLEs | Queued; must be recorded in the log because it changes determinism | — | Q |
| Watermark/branding, templates, caption burn-in | Web/consumer editors | **Rejected** | — | R |
| Direct upload/share to services | Web editors | **Rejected** — no network in the product | — | R |

## F. Proof, reproducibility, integrity (ReelDeck's differentiator)

None of the scanned tools ship a re-runnable proof with the output. The
closest analogues are checksums in archival transfer tools and encoder
logs, and both are partial: a log without a pinned recipe cannot be
re-run, and a hash without the tool versions cannot explain a difference.

ReelDeck's answer, now visible in the interface:

| Capability | ReelDeck | Status |
| --- | --- | --- |
| Recipe as a first-class, diffable artifact | `Recipe` model, live filter string in the chain | S |
| Export log with command, versions, frame counts, warnings, hashes | `ExportLog`, `rd-run` sidecars | S (engine) / P (UI) |
| Stream-level fixity (container metadata is not a changed picture) | `output_stream_sha256`, rerun verdict | S (existing) |
| Verification report with `SKIP ≠ PASS` | `verify_export`, Prove panel | S/P |
| Determinism comparison across runs | `reeldeck rerun`, determinism card | S/P |
| Suggested findings that require operator confirmation | interlace suggestion with basis counts | S/P |
| Preview gate with a recorded expert override | `preview_required`, preflight, log | S/P |

## G. Learning curve and accessibility

| Capability | Where it is practised | Decision | ReelDeck | Status |
| --- | --- | --- | --- | --- |
| Command palette with searchable actions and shortcuts | Pro creative tools | Adopt; one table drives bindings, palette, and help | `shortcuts.rs`, palette | S/P |
| Keyboard shortcut sheet | Pro tools | Adopt, generated from the same table | shortcuts dialog | P |
| Beginner defaults with advanced controls one expansion away | Consumer editors | Adopt as progressive disclosure — no separate "simple mode" that hides state | collapsed steps, defaults | S/P |
| Guided onboarding tours | Consumer/web editors | Rejected as a first-run obstacle; contextual explanations do the work | — | R |
| Density, accent, contrast, motion preferences | Pro tools (partially), OS settings | Adopt and follow OS reduced-motion/contrast | `prefs.rs` | S/P |
| Full keyboard operation and visible focus | Rare in consumer tools | Adopt as a requirement | focus ring, dialog focus trap, arrow-key tabs | P |
| Text scaling to 200 % without layout breakage | Rare | Adopt as a requirement (no fixed text heights) | density tokens | P (designed, not yet measured at 200 %) |
| Screen-reader semantics for a dense pro UI | Rare | Adopt: landmarks, tables, tab semantics, live region | markup + audit | P |

---

## What the scan says ReelDeck should do better than all of them

1. **Ship the proof, not just the file.** Recipe, log, hashes, and a
   re-run verdict beside every output — the one thing no editor in the scan
   does.
2. **Make measurement honest and visible.** Basis, thresholds, units,
   peak-preserving graphs, `SKIP` where evidence is missing.
3. **Treat consequences as part of the control.** Damage notes and the
   exact filter string are not buried in help.
4. **Gate destructive-quality work on evidence** — preview or recorded
   override — rather than either blocking silently or letting a click ruin
   an hour of encoding.
5. **Suggest, never decide.** The interlacing classification carries its
   counts and waits for the operator.
6. **Be fast for experts without being opaque to newcomers** — one layout,
   progressive disclosure, keyboard-first.

## Rejections, with reasons

- **Narrative editing** (splits, transitions, titles, multicam): wrong job,
  and every edit surface added to a restoration tool weakens the chain as
  the record of the decision.
- **Synthesis** (AI enhancement, generative fill, frame interpolation):
  contradicts the preservation contract.
- **Cloud, accounts, telemetry, sharing**: contradicts local-first.
- **Creative grading**: a documented level correction is a restoration
  decision; a grade is not.
- **One-click "fix everything"**: the product's premise is that the
  operator decides and the record shows it.

## Open questions for the next scan

- Keyframe-snapped stream-copy trimming: how much of the "lossless cutter"
  workflow can be adopted without weakening the proof and the chain?
- Proxy generation strategy for 2-hour DV captures on modest hardware.
- QC overlays and burn-ins: which are acceptable in a preservation file
  (probably none) and which belong only to review copies.
- Chapter/attachment preservation for Matroska masters.
