# ReelDeck — Interface Design

> **Status:** design contract for the shell, alongside
> [`ARCHITECTURE_AND_UX_PLAN.md`](ARCHITECTURE_AND_UX_PLAN.md). The state
> model in `crates/rd-ui` implements the behaviour described here; the
> prototype in `crates/rd-ui/prototype` renders it. The shipping GUI
> framework is still undecided — §10 records that decision honestly.
>
> **Verification:** structure, gating, and keyboard behaviour are checked
> headlessly; the accessibility audit reports no WCAG 2.1 A/AA
> violations. Pixel-level rendering has **not** been verified on a real
> display (§11).

---

## 1. What the interface is for

ReelDeck is used by archivists and small production teams on footage that
is old, interlaced, noisy, shaky, or otherwise awkward — and by people who
must later say, with evidence, exactly what was done to it.

That changes what the interface has to be:

| The work | What the interface must do |
| --- | --- |
| Judging a picture on a good monitor | Keep the picture large, uncluttered and accurately framed; put scopes and comparisons one keystroke away. |
| Deciding a chain of destructive-ish operations | Show every parameter, its consequence, and the exact filter it produces — before the render, not after. |
| Proving the result months later | Make the recipe, the export log and the hashes first-class, readable artifacts rather than log files. |
| Working through many long files | Keep the queue, per-file state and job progress visible and interruptible. |
| Working under time pressure | Make the common path two or three clicks and never hide a needed control behind jargon. |

Three users, one interface: the **first-time operator** needs defaults that
are safe and copy that explains them; the **daily operator** needs density
and keyboard speed; the **reviewer** needs to read state without
interpreting colour. The shell resolves this with progressive disclosure —
collapsed summaries with real numbers, expanding to full controls — rather
than with separate modes.

**Not the product:** it is not a narrative editor. There are no transitions,
titles, multicam, or composite timelines. The editing surface is the
restoration chain; the timeline is for measurement, judgement, and segment
selection.

---

## 2. Design principles

1. **The picture is the work.** The viewer gets the largest area and the
   fewest interruptions. Panels are sized so the picture never drops below
   a judgeable size at the target window sizes.
2. **Calm, not flat.** One accent colour, used for the active stage,
   selection, focus and the primary action. Everything else is a step on a
   grey scale or a state word. No gradients on controls, no shadows except
   where something floats (dialogs, drawers).
3. **Every state is written.** Colour reinforces; it never carries meaning
   alone. `PASS`, `CHECK`, `FAIL`, `SKIP`, `Hash mismatch`, `Preview
   required` are all text. The same rule applies to the timeline's finding
   glyphs (`B`, `F`, `!`).
4. **Nothing is invented.** A value the shell does not know is `—`; a
   measurement always has units and a basis; an estimate says *estimate*;
   progress is the rate the engine reported, or `measuring…`.
5. **Consequences next to controls.** Damage notes, field-order warnings
   and the crop cost of a zoom live in the module that causes them. A
   tooltip is never the only place a risk is stated.
6. **Gate, don't nag.** A stage that needs evidence is disabled *with the
   reason and the fix*. Aggressive chains require a preview or a recorded
   override — recorded, so the decision survives the session.
7. **Density with rhythm.** 12 px UI type, 26 px rows, 4 px spacing steps,
   1 px hairlines. Grouping is done with space and rules, not boxes inside
   boxes.
8. **Keyboard is first-class, not a shortcut.** Every command is in one
   table; the palette, the shortcuts sheet, and the bindings all read it.
9. **Recoverable.** Removing a step returns it for undo; refused
   parameters leave the chain untouched; cancelling a job deletes its
   partial file and says so.
10. **Quiet at rest.** No animation unless it explains a change (drawer,
    toast, progress). Reduced motion is honoured from the OS and from
    preferences.

---

## 3. Visual language

### 3.1 Colour

The palette is a dark neutral ramp with one accent. Values are tokens
(`crates/rd-ui/prototype/styles/tokens.css`), not literals in components.

| Token | Value | Role |
| --- | --- | --- |
| `--bg-0` | `#08090b` | Application canvas, gutters, timeline wells |
| `--bg-1` | `#0e1013` | Panels, inspector, queue |
| `--bg-2` | `#131619` | Panel headers, cards, status bar |
| `--bg-3` | `#191d22` | Hover surface, secondary buttons |
| `--bg-4` | `#20252b` | Pressed/active surface, segmented selection |
| `--bg-inset` | `#0a0c0e` | Inputs, code blocks, graph wells |
| `--line-1` | `#1c2127` | Row separators (the quietest rule) |
| `--line-2` | `#2a3038` | Panel borders, control outlines |
| `--line-3` | `#3a424c` | Hover borders, strong dividers |
| `--text-1` | `#e9ecf1` | Primary text |
| `--text-2` | `#a7aeb9` | Secondary text, labels |
| `--text-3` | `#767e8a` | Tertiary text, units, hints |
| `--text-4` | `#565d68` | Disabled, metadata, scale marks |
| accent | amber `#e0a45c` (default), teal `#62b3a3`, slate `#84a1c8` | Active stage, selection, focus, primary action |
| `--ok` | `#74b98d` | Verified, pass, match |
| `--warn` | `#d8ab55` | Check, preview required, estimates |
| `--err` | `#d9776d` | Failure, mismatch, blocked |
| `--info` | `#7fa8d4` | Neutral information, frozen spans, ranges |
| `--scope-line` | `#9fb4bd` | Measurement traces (deliberately low chroma) |

Rules:

- **Accent budget.** At most one accent-filled control per region, plus
  the active stage underline and the selection bar. Selection tint is 9 %
  accent; focus ring is a 2 px accent outline at 1 px offset.
- **No state on text alone.** Status pills pair a dot and a word; check
  rows pair a chip (`PASS`) with the explanatory sentence.
- **Scope colours stay desaturated** so the picture is judged against a
  neutral reference, not a coloured one.
- **Contrast targets:** body text ≥ 4.5:1 on its surface; secondary labels
  ≥ 4.5:1; non-essential metadata ≥ 3:1; focus ring ≥ 3:1 against both the
  control and the canvas. The high-contrast preference raises text and
  border contrast and strengthens washes (and follows
  `prefers-contrast: more` where supported).

### 3.2 Type

| Use | Face | Size / weight | Notes |
| --- | --- | --- | --- |
| UI text, labels | system UI stack | 11–13 px, 400–600 | Segoe UI Variable on Windows |
| Panel headings | system UI | 11 px, 620, uppercase, 0.06 em | The only uppercase that is not a shortcut |
| Micro labels | system UI | 9–10 px, 600, 0.08–0.09 em | Section eyebrows; never the sole carrier of meaning |
| Values, numbers | system UI | 11–18 px, 600, tabular | Tabular numerals everywhere a column can move |
| Machine strings | monospace stack | 9–11 px | Filter strings, commands, hashes, timecode, paths |
| Long prose | system UI | 10–11.5 px, line-height 1.45 | Damage notes, block reasons |

Timecode is monospace and tabular so a scrubbed playhead does not make the
digits jump. Hashes are monospace and wrap at arbitrary points.

### 3.3 Space, shape, depth

- 4 px base scale: `4 8 12 16 20 24 32`. Panel padding 10 px (14 px in
  comfortable density).
- Row height 26 px compact / 34 px comfortable; control height 24 px / 30 px.
- Radii: 3 px inputs and chips, 5 px cards and buttons, 8 px dialogs.
- Borders are 1 px `--line-1`/`--line-2`. Elevation exists only for
  floating surfaces: dialogs (`--shadow-dialog`), the responsive inspector
  drawer, and toasts.
- No decorative imagery, no branded illustration, no glassmorphism.

### 3.4 Motion

90 ms for state changes, 160 ms for surfaces entering or leaving. Only
opacity and small translates. Progress bars animate at the engine's
reported rate, not a decorative loop. Reduced motion (preference or OS)
sets both durations to zero.

### 3.5 Icons and glyphs

Original 24 px line icons at 1.5 px stroke, drawn for this interface; no
icon library and no product iconography. Icons never appear without a
label in a primary surface — the icon-only controls are the toolbar
cluster (with `aria-label` and tooltip) and the viewer modes at narrow
widths. Findings use letters (`B` black, `F` freeze, `!` outlier) so they
differ by shape, not hue.

---

## 4. Layout

### 4.1 The shell

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ◉ ReelDeck │ Ashland Oral Histories / 04 · E. Vance │ ①Ingest ②Inspect …    │ 42 px
│                                                    engine · prototype · ⚙     │
├──────────────────────────────────────────────────────────────────────────────┤
│ Inspect   Measure the signal. Findings are suggestions; you confirm them.    │ 38 px
│                                           [field order ✓ TFF]  [Re-inspect]   │
├───────────────────────┬──────────────────────────────────────┬───────────────┤
│ SOURCES         4     │ VIEWER                               │ Source│Chain│…  │
│ filter…               │ Program  Waveform  Histogram  …      │               │
│ ┌───┬────────┬──────┐ │ ┌──────────────────────────────────┐ │ Probe facts   │
│ │04 │Interview│ ✓ ✓  │ │ │                                  │ │ Fixity        │
│ │02 │Church   │ ✓ ✓  │ │ │        the picture (4:3)         │ │ Interlace     │
│ │07 │Mill     │ ✗ —  │ │ │                                  │ │ suggestion    │
│ │11 │Council  │ … —  │ │ └──────────────────────────────────┘ │ Signal        │
│ └───┴────────┴──────┘ │ ┌ transport / timecode / in-out ────┐ │ Findings      │
│ 4 sources · 2h 29m    │ │ ruler   V1   A1   luma ▁▂▄▆▃▂  …   │ │ Audio         │
│                       │ │ findings B · ! · F · ! · B        │ │               │
│                       │ └ JOB ─ progress ─ cancel ─────────┘ │               │
├───────────────────────┴──────────────────────────────────────┴───────────────┤
│ ● saved   04 · Interview   ● Fixity match               Ctrl K · ? · Alt 1–6 │ 26 px
└──────────────────────────────────────────────────────────────────────────────┘
```

Regions, responsibilities, and the rule that keeps them honest:

| Region | Holds | Never holds |
| --- | --- | --- |
| Top bar (42 px) | Product mark, project/clip breadcrumb, the six stages, engine status, prototype badge, help and preferences | Actions that change a recipe |
| Context bar (38 px) | Current stage name and purpose; the actions for that stage | Navigation to another stage (that is the rail) |
| Queue (left, 292 px) | Sources with fixity, inspection state and length; filter; project totals | Chain state or export settings |
| Viewer (centre, flexible) | Picture, mode scopes, before/after wipe, safe areas, HUD readout | Editing controls |
| Timeline (centre, below viewer) | Transport, timecode, ruler, tracks, measurement lanes, findings, preview range | Clip editing, transitions |
| Jobs (centre, bottom, collapsible) | Running and finished jobs with measured progress and cancel | Settings |
| Inspector (right, 372 px) | Four tabs: **Source**, **Chain**, **Export**, **Prove** | Queue-wide actions |
| Status bar (26 px) | Save state, selected source, fixity, keyboard hints, engine mode | Transient messages (those are toasts) |

The workflow is visible as a rail rather than a wizard: stages can be
visited in any order that the evidence allows, and the rail shows which
are blocked and why (a dot plus a tooltip; the stage still responds with a
toast explaining the missing evidence and the fix).

### 4.2 Stage → surface mapping

| Stage | Inspector tab | Centre emphasis | Gate before entering |
| --- | --- | --- | --- |
| Ingest | Source (intake + fixity) | Queue, probe facts | — |
| Inspect | Source (findings) | Viewer scopes + lanes + findings | a queued source |
| Decide | Chain | Viewer compare + live filter in context bar | completed inspection |
| Preview | Chain + range controls | Viewer compare, range band, jobs | at least one operation |
| Export | Export | Destination + preflight | preview, or recorded override |
| Prove | Prove | Verification report, sidecars | a finished output |

### 4.3 Responsive behaviour

The shell is a desktop tool, so it degrades by *removing optional context*,
never by hiding the picture or the chain.

| Width | Behaviour |
| --- | --- |
| ≥ 1420 px | 292 px queue, 372 px inspector, all lane labels, labelled viewer modes |
| 1240–1420 px | Queue 258 px, inspector 340 px; purpose line hidden; stages tighten |
| 1080–1240 px | Queue 236 px; inspector becomes a dismissible drawer over the picture (scrim, focus moves to it); viewer modes icon-only with tooltips |
| 860–1080 px | Queue also becomes a drawer; stage labels collapse to numbers; viewer HUD trims |
| < 860 px | Single column: queue and inspector are drawers, timeline scrolls horizontally, lanes keep one graph and the labels shrink to a 64 px gutter |

Comfortable density raises row heights and type without changing layout,
so an operator using touch or magnification does not get a different
interface. Text scaling to 200 % is supported by the same rules (no fixed
heights around text).

---

## 5. Components

Each entry gives anatomy, states, and the accessibility contract. The
prototype implements all of them.

### 5.1 Stage rail (`nav`)
Buttons with a two-digit index, the stage name, and `aria-current="step"`.
Completed stages are not marked "done" (evidence can disappear when a file
moves), so the rail reports only **current** and **blocked**. Blocked
stages keep their position and explain themselves on activation. Keys:
`Alt+1` … `Alt+6`.

### 5.2 Queue table
Rows: thumbnail (decorative, `alt=""`), name and collection line, two
state pills (fixity, inspection), length. The selected row carries a
2 px accent bar, a tint, `aria-selected`, and roving `tabindex`; Enter or
Space selects. The table is the only place a file's *identity* is chosen.

States: `Hashing` → `Hash match` / `Hash mismatch`; `Not inspected` →
`Inspecting` → `Inspected` / `Inspection failed`. A mismatch row explains
that ReelDeck never wrote to the source and that exports from it are
blocked.

### 5.3 Viewer
Toolbar (mode group, compare, zoom, guides), picture well, HUD, readout
strip.

- **Modes:** Program, Waveform, Histogram, Vectorscope, Field split,
  Temporal difference. Scopes are drawn over the picture at 72 % opacity
  so framing and measurement stay in one gaze; the mode name is always
  visible.
- **Compare:** a vertical wipe with a keyboard-operable handle
  (`role="slider"`, arrow keys ±4 %), labelled SOURCE and CHAIN, at the
  same frame. The wipe never changes the recorded recipe.
- **HUD:** source title (top left), mode or `COMPARE` (top right),
  timecode (bottom left), format and frame rate (bottom right).
- **Readout:** Y, SAT, TOUT at the playhead, frame number, and an
  `Outlier above threshold` flag that appears only when the measured
  value exceeds the stated threshold.
- **Field split** shows both fields side by side with a divider and field
  tags; **temporal difference** shows a high-contrast difference of the
  frame, for dropouts and frozen spans.

### 5.4 Timeline
Transport (mark navigation, frame step, timecode, in/out, suggest range,
lane toggle), ruler, picture and audio tracks, measurement lanes, findings
strip, playhead, selection band.

- **Lanes** show per-frame measurements downsampled to the pixel width,
  preserving **min and max per bucket** — a single dropout in an hour of
  flat signal stays visible. Each lane states its scale (`0–0.25`) and its
  flagging threshold (dashed rule, labelled `flag > 0.05`).
- **Findings** are marks with glyphs and times; clicking one seeks to it.
  `N` / `Shift+N` step between them. The strip is a `group` with a label.
- **Preview range** is a band labelled `PREVIEW RANGE`; `I` and `O` set
  in/out, `Shift+I` centres a 20-second segment on the worst measured
  moment, `Esc` clears it. The range feeds the preview render and is
  recorded in the preview recipe's notes.
- Seeking works by click or drag on the ruler, a lane, or the track.

### 5.5 Chain inspector
The chain is an ordered list of step cards. Collapsed, a card shows the
module, its generated summary (`Deinterlace · bwdif · tff · send_frame`),
a count of live warnings, and a `Gate` flag when its settings require a
preview. Expanded, it shows:

- the module's one-line purpose;
- precise controls (§5.6);
- **damage notes** as callouts, always visible for that module;
- the **exact filter fragment** in a monospace block with copy;
- reset and remove.

A sticky footer carries the **complete live filter chain** (from the same
planner the executor uses), the planner's verdict (including refusals,
verbatim), the preview gate, and the two ways forward: preview a segment,
or record an expert override.

States: empty (with add), uninspected source (with the reason and an
inspect action), planner refusal (error text in the filter block), gate
closed (`Preview required` + override), gate open.

Keyboard: `A` add, `Alt+↑/↓` reorder, `Delete` remove, `Alt+Backspace`
reset, `Enter` expand/collapse. Refused values show the range and leave
the chain untouched.

### 5.6 Controls

| Control | Behaviour | Accessibility |
| --- | --- | --- |
| Segmented choice | One of 2–4 mutually exclusive options, tick on the chosen one, disabled options keep a reason in the tooltip | `role="group"` with `aria-label`; buttons carry `aria-pressed` |
| Slider + number | Range drag with a paired numeric input; both edit the same field; value echoed in the field header | Native `input[type=range]` + labelled number input; focus-visible ring on the track |
| Checkbox | For flags that change the rendered picture | Native input, visually styled box, label text includes the consequence |
| Text/number input | Paths, dimensions, thresholds | Labelled; `aria-invalid` on refusal; the error names the range |
| Select | Field order, sample range | Native select, never a custom listbox for these |

All controls are at least 22 px tall in compact density and 30 px in
comfortable; the slider thumb is 11 px with a 16 px hit area, and the
timeline's clickable bands are full-height.

### 5.7 Export panel
Three **intent cards** (preservation, mezzanine, access) with format,
rationale, and an estimated size marked as an estimate. The selected card
is a radio (`role="radio"`, `aria-checked`). Below: destination path with
a check action; the **preflight list**; expert override (only when the
gate needs it); the exact command; the primary action with the estimate.

Preflight rows, each with a chip and a sentence: chain plans, segment
preview, destination free, source fixity, disk space. `UNMEASURED` is a
first-class outcome. The primary button is disabled only with a stated
blocking reason directly beneath it.

### 5.8 Prove panel
A verdict card, the verification table (run succeeded, output hash,
source hash, preset, schema), the sidecars list with short hashes, and the
determinism card. A **skip example** is shown explicitly so the rule is
visible: missing evidence is `SKIP`, never `PASS`.

When no export exists the panel says so and lists what will be checked —
it does not show green rows for checks that have not run.

### 5.9 Dialogs
Command palette (`Ctrl+K`), shortcuts (`?`), preferences (`Ctrl+,`),
module picker (`A`), and the export confirmation. Each has a title, a
subtitle that states scope or consequence, a body, and a footer with the
primary and cancel actions. While open, the rest of the shell is `inert`;
focus is trapped and restored on close; `Esc` closes. The confirmation
lists output, preset, chain, estimate, proof sidecars and the exact
command — the same facts the run will use.

### 5.10 Toasts and live announcements
Toasts are bottom-right, max 380 px, coloured by state with an icon and a
bold first line, dismissed after 4.6 s. Every toast is mirrored into a
polite live region in one sentence, so a screen-reader user gets the
feedback without the visual. Toasts never carry the only copy of a
consequence: the same facts are in the panel that caused them.

### 5.11 Status bar
Save state, selected source, fixity dot plus word, keyboard hints, engine
mode (`offline, local only`). Reserve for durable status; transient
feedback goes to toasts.

---

## 6. Interaction and feedback

- **Selection vs focus.** Only one source, one step and one tab are
  selected at a time; focus is independent and always visible.
- **Order of operations.** Choosing a source updates viewer, timeline,
  chain, export and proof together. Chains belong to their source; a chain
  never describes a different file's picture.
- **Gate feedback.** Attempting a blocked stage is not a dead click: a
  toast and a live-region sentence name the missing evidence and the fix.
- **Irreversible actions.** Exports confirm with facts; deletes are
  recoverable (the removed step is returned for undo). Sources are never
  written to, so no action can destroy one.
- **Progress honesty.** A job shows `measuring…` until the engine reports
  a rate, then a percentage; cancel stops the run and removes the partial
  output, and the log records the cancellation.
- **Undo.** Removal returns the exact operation including its parameters;
  refused parameter edits never mutate the chain. A full undo stack is a
  model slice, not yet present.
- **Errors.** Three kinds, three treatments: **usage** (inline, next to
  the field), **blocked** (preflight row + disabled primary with reason),
  **failed run** (journal row + toast + proof noting the failure).

---

## 7. Accessibility

Requirements, not polish:

- **Semantics:** one `h1` per stage, labelled landmarks, real tables for
  the queue and checks, `nav` for stages, `tablist`/`tabpanel` for the
  inspector with arrow-key navigation, `radiogroup` for export intent,
  `alertdialog` for the destructive confirmation.
- **Names:** every control has an accessible name; icon-only controls have
  `aria-label` and a tooltip; decorative images are `alt=""`.
- **Focus:** 2 px accent outline at 1 px offset, visible on every
  interactive element, never removed without replacement. Dialogs trap
  focus and restore it. Selection is reachable without a mouse.
- **Colour independence:** every state has a word or glyph; scopes are
  readable in grayscale; findings differ by letter.
- **Contrast:** targets in §3.1; high-contrast mode plus
  `prefers-contrast` support.
- **Motion:** reduced motion honoured from the OS and from preferences;
  nothing flashes.
- **Targets:** minimum 22 × 22 px (comfortable 30 × 30), 4 px spacing
  between adjacent targets; the timeline remains operable with a
  trackpad.
- **Screen readers:** a polite live region announces stage changes,
  accepted edits, job start/finish/cancel, and blocked actions.
- **Audit:** axe-core on the shell, palette, preferences, shortcuts and
  export panels reports **0 violations** for WCAG 2.1 A/AA in the
  prototype (§11). Colour contrast is checked in review against the token
  table, since the audit environment has no layout engine.

---

## 8. Measurement rules in the interface

The product's promise is honesty, and the interface is where it is either
kept or broken.

- A metric shows its **basis** ("63,450 frames measured over the full
  file") and its **threshold** (`TOUT > 0.05`) next to it.
- Graphs are peak-preserving; smoothing is not applied silently.
- An interlacing classification is a **suggestion** with counts; the
  operator confirms it, and the confirmation is what the chain uses.
- Estimates are labelled `estimate`; sizes use binary units, stated once in
  preferences.
- Cost labels are relative words (`fast`, `slow`, `slowest (two passes)`),
  not seconds, until the test corpus measures them.
- A check with missing evidence reads `SKIP` / `UNMEASURED`.
- Progress is the rate the engine reported, or `measuring…`.

---

## 9. The prototype

`crates/rd-ui/prototype` is the interface, rendered without a framework:

```
index.html                 landmarks, panels, dialogs
styles/tokens.css           colour, type, density, contrast, motion
styles/app.css              layout and components
js/format.js                timecode, durations, bytes, rates
js/data.js                  sample project, catalogs, measurement series
js/model.js                 the rules (mirror of crates/rd-ui/src)
js/components.js            icons, pills, scope and lane drawing
js/app.js                   rendering, events, dialogs, keyboard
assets/media/frame-*.jpg    one sample frame (generated for this project)
```

Open `index.html` in a browser — no build step, no network access. Sample
data is labelled **Prototype** in the top bar, and every simulated job is
labelled as simulated; nothing claims to have rendered a file.

The prototype exists for three reasons: to settle the interface before the
framework choice, to exercise the model's rules against a real surface, and
to give reviewers something to click. It intentionally mirrors
`crates/rd-ui/src` function-for-function where it can, and the places it
goes further (per-source chains, timeline drawing) are listed in §12.

---

## 10. Decision record: the shipping GUI

**State: open.** The prototype deliberately settles nothing about the
framework, so the choice stays cheap.

Requirements that any candidate must meet:

1. Real native text rendering and input on Windows 10+ (the primary
   platform), at 100–200 % scaling.
2. GPU-presented video frames with frame-accurate positioning, without
   copying the picture through an accessibility tree.
3. Keyboard-first operation, focus management, and exposure of semantics
   to Windows accessibility APIs (UIA).
4. Off-thread work with measured progress and cancellation.
5. Local-only, no network surface, no telemetry; a small dependency
   budget; safe Rust for application logic (`unsafe` only in a scoped
   platform layer, if at all).
6. Package size and startup that suit an archival workstation.

Candidates: a Rust-native GPU shell, a Rust core with the system webview
(Tauri-class), and the native Windows toolkit. Trade-offs to settle with a
spike, in order: video presentation quality and colour management, input
latency, accessibility exposure, and packaging. The state model and this
prototype are framework-neutral on purpose; both candidates can host the
same layout and the same model.

**Until that decision lands:** `rd-ui` is model-only; the prototype is the
visual reference, not the shipping binary.

---

## 11. Verification record

| Check | Result | Where |
| --- | --- | --- |
| Model tests (stages, chain, guards, preflight, timeline, formatting, preferences, commands) | **Pass** — 67 `rd-ui` tests, 129 across the workspace | CI, `crates/rd-ui/src/*` |
| Format and lints (`cargo fmt --check`, clippy with `-D warnings`) | **Pass** | CI |
| Release build (workspace) | **Pass** | CI |
| Prototype structure and flow (boot, gating, chain edits, preview, export, verify, preferences) | Exercised headlessly (jsdom): no page errors; expected states reached | Authoring run (scripts not committed) |
| Accessibility (WCAG 2.1 A/AA) | **0 axe-core violations** on the shell, shortcuts, preferences and export panels | Authoring run (axe-core 4.11, jsdom) |
| Colour contrast | Reviewed against the token table; contrast targets stated in §3.1. Not machine-measured (no layout engine in the audit environment) | — |
| Typography, spacing, layout at 1440/1280/1024/860 logical widths | Designed against §4.3; **not** confirmed by rendering on a display | — |
| Pixel rendering on a real display or GPU | **Not verified** — no usable browser in the authoring environment | — |
| Real FFmpeg render through this interface | **Not verified** — the prototype simulates jobs | — |

The three failures the first test run found (a single-match warning list,
an export gate that read "open" with no chain, and a playhead clamped to
an unmeasured strip) were fixed in the model and are covered by tests.

Claims in this document stop where that table stops. Nothing here is
described as production-ready.

---

## 12. Next slices

1. **Per-source recipes in the model** — the prototype already swaps chains
   per source; `ShellModel` still holds one.
2. **Undo/redo stack** over chain edits (removal already returns the
   operation).
3. **Real engine binding** — progress, cancellation and job journal from
   `rd-run`; the interface is drawn around those exact callbacks.
4. **Lossless segment export** — the preview range becomes a stream-copy
   cut with its own proof; the trim model does not exist yet.
5. **Batch queue** — per-file state transitions already gated; needs a
   queue runner and resume.
6. **Proxies** — scrubbing long files at full resolution is the next
   performance cliff.
7. **Framework spike** (§10) and a real-display visual pass.

---

## Revision history

- **2026-10-27** — first version, with the model and prototype. Records
  the framework decision as open and the verification gaps as gaps.
