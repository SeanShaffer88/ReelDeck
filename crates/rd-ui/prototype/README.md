# ReelDeck shell prototype

A working, dependency-free rendering of the ReelDeck interface, used to
settle the design before a GUI framework is chosen.

**Open `index.html` in a browser.** No build step, no server, no network
access. (A local server is fine too; nothing is fetched.)

```
python3 -m http.server -d crates/rd-ui/prototype 8080   # optional
```

## What this is — and is not

- It is the **interface**: layout, density, hierarchy, states, keyboard
  model, and the interaction rules that matter to this product (gates,
  consequences, proof).
- It is **not the engine**. Jobs are simulated and labelled as simulated.
  Sample project data is marked **Prototype** in the top bar. No file is
  read, written, hashed, or rendered.
- The rules it enforces (stage access, parameter ranges, preview gate,
  filter construction, pre-export checks) mirror `crates/rd-ui/src` —
  `js/model.js` is the same logic in the browser's language. The design
  rationale is in [`docs/UI_DESIGN.md`](../../../docs/UI_DESIGN.md).

## A five-minute tour

1. **Inspect** — the Source panel carries probe facts, fixity, the
   interlace *suggestion* with its `idet` basis counts (confirm it, or
   choose another), measured signal, findings and audio. Click a finding to
   seek to it.
2. **Viewer** — switch modes (`W`, `H`, `U`, `F`, `D`), compare the chain
   against the source (`\`) and drag the wipe, or use the arrow keys on its
   handle. `Shift+1` toggles 100 % pixels, `'` shows safe guides.
3. **Timeline** — scrub the ruler or a lane; `I` / `O` set a preview range;
   `Shift+I` suggests one centred on the worst measured dropout; `G` hides
   the measurement lanes.
4. **Decide** (`Alt+3`) — expand a step, change its parameters, and watch
   the filter fragment, the live chain, and the planner's verdict update.
   Push temporal denoise above 8 to see the **preview gate** appear. Drag
   the sliders or type exact numbers. Undo with `Ctrl+Z` (or the header
   buttons); history is per source and survives switching files.
5. **Preview** (`Alt+4`) — render a segment (simulated). The job moves
   Queued → Running with measured progress, shows `Cancelling` while a
   cancel drains, then ends `Cancelled`; let it finish and it ends `Done`
   with its proof note. Jobs without a known frame total stay on
   `measuring…` rather than inventing a percentage.
6. **Export** (`Alt+5`) — pick an intent, read the preflight checks
   (`PASS` / `CHECK` / `FAIL` / `SKIP`), and open the confirmation: it lists
   the exact command that would run, the estimate, and the proof sidecars.
7. **Prove** (`Alt+6`) — after the simulated export: verification rows, the
   sidecars written beside the output, and a determinism comparison.
8. **Batch** (`Ctrl+B`, or the layers icon by Sources) — choose sources
   to render in order. Only sources with a previewed chain and a matching
   hash can join; the rest say why. Use the prototype-only switch to make
   one source fail: the batch stops there, the queue marks it
   `Resume here`, and **Resume** retries it under a new `…attempt2` name
   before continuing.
9. **Everywhere** — `Ctrl+K` command palette, `?` shortcuts, `Ctrl+,`
   preferences (density, accent, contrast, motion, time display), `Alt+1`
   … `Alt+6` stages, arrow keys to step frames.

## Keyboard

| Keys | Action |
| --- | --- |
| `Ctrl+K` | Command palette (search by name, tool, or chord) |
| `?` · `Ctrl+,` | Shortcuts sheet · Preferences |
| `Alt+1`…`Alt+6` | Ingest, Inspect, Decide, Preview, Export, Prove |
| `←` `→` (with `Shift`) | Step one frame · jump one second |
| `N` / `Shift+N` | Previous / next finding |
| `I` / `O` / `Esc` | Preview in, out, clear range |
| `Shift+I` | Suggest a preview range around the worst measured moment |
| `G` | Show/hide measurement lanes |
| `P` `W` `H` `U` `F` `D` | Viewer modes |
| `\` · `'` · `Ctrl+1` | Compare · guides · 100 % zoom |
| `A` · `Alt+↑/↓` · `Delete` · `Alt+Backspace` | Add module · reorder · remove · reset |
| `Ctrl+Z` · `Ctrl+Shift+Z` (`Ctrl+Y`) | Undo · redo the selected source's chain edits |
| `Ctrl+B` | Batch render: choose sources, start or resume a batch |

A slider drag or numeric-field focus session is one gesture: all its
intermediate values collapse into a single undo entry, so one `Ctrl+Z`
restores the value from before the whole drag.
| `Ctrl+Enter` · `Ctrl+E` · `Ctrl+Shift+Enter` | Preview segment · export panel · render export |

One table, three uses: the palette, the shortcuts sheet and the key bindings
all come from the same `COMMANDS` table, so a chord shown in the sheet is the
chord that actually runs.

## Files

```
index.html              landmarks, panels, dialogs
styles/tokens.css       colour, type, density, contrast, motion
styles/app.css          layout, components, responsive rules
js/format.js            timecode, durations, bytes, rates, hashes
js/data.js              sample project, catalogs, deterministic series
js/model.js             stage gates, chain rules, filter text, preflight, job journal
js/components.js        icons, pills, scope and lane drawing
js/app.js               rendering, events, dialogs, keyboard, jobs
assets/media/           one generated sample frame
```

## Honest limitations

- **Simulated jobs.** Only the clock is simulated: `js/app.js` drives the
  same lifecycle, measured-only progress rules, cooperative cancellation
  and 100-entry cap that `crates/rd-ui/src/jobs.rs` enforces, with the
  rules living in `js/model.js`, not in the timer. Each row is labelled
  `Simulated in this prototype`; the real shell forwards `rd-run`
  `Progress` blocks and `RunReport` outcomes to the identical state
  machine.
- **Simulated batch failures.** The batch dialog's failure switch exists
  only so the stop-and-resume path can be seen; it is labelled as such
  and is not part of the shell's design. Batch totals come from the
  sample clips' durations and rates, as the real runner's come from a
  probe.
- **One sample frame.** Before/after, field split and temporal difference
  are styles over that frame, not decoded video.
- **No file access.** Adding files and disk-space measurements belong to
  the desktop shell; the prototype explains rather than pretending.
- **Visual verification.** The layout has been reviewed and exercised, but
  not yet rendered on a real display by the author of this change (no
  browser in the authoring environment). Treat spacing and contrast as
  designed-but-unconfirmed until the first display pass.
- **Per-source chains, undo/redo, gesture coalescing and the job journal**
  exist in both surfaces: each source keeps its own chain, evidence and a
  100-entry history, slider/numeric gestures collapse to one entry, and
  jobs follow the same six-state lifecycle, mirrored in
  `crates/rd-ui/src`.

## Adding to the prototype

Keep the model and the interface in step:

1. A rule that affects validity, gates, or wording goes in `js/model.js`
   first, then in `crates/rd-ui/src` (or the reverse, if the model leads).
2. A new command gets one entry in `COMMANDS` (`js/data.js`) — title, keys,
   group, hint, aliases — and a handler in `app.js`. Never bind a key that
   is not in the table.
3. A new color, size or spacing value gets a token; components read tokens.
4. Every state needs a word, not just a color.
