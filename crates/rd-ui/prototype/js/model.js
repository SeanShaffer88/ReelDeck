/* ReelDeck prototype — the shell model.

   Mirrors the rules in crates/rd-ui/src: stage gating, chain editing,
   parameter ranges, the preview gate, the exact filter string (built the
   way rd-pipeline builds it), and the pre-export checks. Keeping these in
   one place is what lets the interface promise what the engine will do. */
(function (global) {
  "use strict";

  var RD = global.RD;
  var D = RD.data;
  var f = RD.format;

  var LIMITS = {
    smoothing: [0, 100],
    zoom: [0, 50],
    denoise: [0, 20],
    scale: [16, 7680]
  };

  // Mirrors UNDO_LIMIT in crates/rd-ui/src/lib.rs: per-source chain
  // history is bounded, oldest entry first.
  var UNDO_LIMIT = 100;

  function defaultPrefs() {
    return {
      density: "compact",
      accent: "amber",
      timeDisplay: "timecode",
      reduceMotion: false,
      highContrast: false,
      safeAreas: false,
      timelineGraphs: true
    };
  }

  function initialState() {
    var chain = D.initialChain.map(function (s) {
      return { module: s.module, params: Object.assign({}, s.params), open: !!s.open };
    });
    var chains = { "04": chain };
    return {
      selectedId: "04",
      chains: chains,
      // Monotonic source of gesture ids for edit coalescing.
      gestureSeq: 0,
      // Per-source chain history: { [clipId]: { undo: [], redo: [] } },
      // each entry { chain, gesture }. Undo never crosses sources.
      history: {},
      // Per-source evidence flags, swapped alongside the chain so a
      // preview or export stays attached to its own file.
      sourceFlags: {},
      stage: "ingest",
      tab: "chain",
      chain: chain,
      selectedStep: 0,
      presetId: "preservation",
      previewed: false,
      override: false,
      exported: null,
      verified: null,
      destination: D.project.defaultDestination,
      viewer: { mode: "program", compare: false, zoom100: false, safe: false, playhead: 0 },
      range: null,
      prefs: Object.assign(defaultPrefs(), readPrefs()),
      lanesOpen: true,
      queueOpen: false,
      inspectorOpen: false,
      queueFilter: "",
      jobs: [],
      toastSeq: 0
    };
  }

  function readPrefs() {
    try {
      var raw = global.localStorage && global.localStorage.getItem("reeldeck.prefs");
      return raw ? JSON.parse(raw) : {};
    } catch (e) {
      return {};
    }
  }

  function savePrefs(prefs) {
    try {
      if (global.localStorage) global.localStorage.setItem("reeldeck.prefs", JSON.stringify(prefs));
    } catch (e) { /* storage may be unavailable; preferences still apply for the session */ }
  }

  function clipMeta(state) {
    return D.clips.find(function (c) { return c.id === state.selectedId; }) || null;
  }

  function stageById(id) {
    return D.stages.find(function (s) { return s.id === id; }) || D.stages[0];
  }

  // ------------------------------------------------------------- stage gates
  function access(state, stageId) {
    var clip = clipMeta(state);
    var chainLen = state.chain.length;
    var inspected = !!(clip && clip.inspect && clip.inspect.state === "done");
    function blocked(reason) { return { ok: false, reason: reason }; }
    switch (stageId) {
      case "ingest":
        return { ok: true, reason: null };
      case "inspect":
        return clip ? { ok: true, reason: null } : blocked("Add a source first: inspection measures a queued file.");
      case "decide":
        if (!clip) return blocked("Add a source first.");
        if (!inspected) return blocked("Inspect the source first: the chain's defaults should be informed by its measurements, not guessed.");
        return { ok: true, reason: null };
      case "preview":
        return chainLen ? { ok: true, reason: null } : blocked("Add at least one operation to the chain to preview it.");
      case "export":
        if (!chainLen) return blocked("Nothing to export: the chain is empty.");
        if (previewGate(state) && !state.previewed && !state.override) {
          return blocked("Preview this chain once before exporting — or record an expert override in the export panel.");
        }
        return { ok: true, reason: null };
      case "prove":
        return state.exported ? { ok: true, reason: null } : blocked("Prove opens once an export has finished.");
      default:
        return blocked("Unknown stage.");
    }
  }

  function nextStageWithAccess(state) {
    var i = D.stages.findIndex(function (s) { return s.id === state.stage; });
    for (var j = i + 1; j < D.stages.length; j++) {
      if (access(state, D.stages[j].id).ok) return D.stages[j].id;
    }
    return null;
  }

  // ------------------------------------------------------------- chain edits
  function hasModule(state, id) {
    return state.chain.some(function (s) { return s.module === id; });
  }

  // ------------------------------------------------------------- history
  function historyFor(state) {
    var id = state.selectedId;
    if (!state.history[id]) state.history[id] = { undo: [], redo: [] };
    return state.history[id];
  }

  function cloneChain(chain) {
    return chain.map(function (s) {
      return { module: s.module, params: Object.assign({}, s.params), open: !!s.open };
    });
  }

  /** Begin one continuous gesture (slider drag, numeric-field session).
      Every edit tagged with the returned id collapses into one undo
      entry, mirroring GestureId in crates/rd-ui/src/lib.rs. */
  function beginGesture(state) {
    state.gestureSeq = (state.gestureSeq || 0) + 1;
    return state.gestureSeq;
  }

  /**
   * Remember the chain before an edit. Returns false when the edit
   * continues a gesture whose anchor entry is already stored, so a drag
   * of fifty ticks still costs one undo. A fresh edit clears redo.
   */
  function snapshot(state, gesture) {
    var h = historyFor(state);
    var top = h.undo[h.undo.length - 1];
    if (gesture != null && top && top.gesture === gesture) {
      return false;
    }
    h.undo.push({ chain: cloneChain(state.chain), gesture: gesture == null ? null : gesture });
    if (h.undo.length > UNDO_LIMIT) h.undo.shift();
    h.redo = [];
    return true;
  }

  function canUndo(state) {
    return historyFor(state).undo.length > 0;
  }

  function canRedo(state) {
    return historyFor(state).redo.length > 0;
  }

  /** Restore the previous chain. Refused when there is nothing to undo. */
  function undo(state) {
    var h = historyFor(state);
    if (!h.undo.length) return { ok: false };
    h.redo.push({ chain: cloneChain(state.chain), gesture: null });
    state.chain = h.undo.pop().chain;
    state.chains[state.selectedId] = state.chain;
    // The restored chain has not itself been previewed in this state.
    state.previewed = false;
    state.selectedStep = Math.max(0, Math.min(state.selectedStep || 0, state.chain.length - 1));
    return { ok: true };
  }

  function redo(state) {
    var h = historyFor(state);
    if (!h.redo.length) return { ok: false };
    h.undo.push({ chain: cloneChain(state.chain), gesture: null });
    state.chain = h.redo.pop().chain;
    state.chains[state.selectedId] = state.chain;
    state.previewed = false;
    state.selectedStep = Math.max(0, Math.min(state.selectedStep || 0, state.chain.length - 1));
    return { ok: true };
  }

  function findStep(state, moduleId) {
    return state.chain.findIndex(function (s) { return s.module === moduleId; });
  }

  function addModule(state, id) {
    if (hasModule(state, id)) {
      return { ok: false, error: D.modules[id].label + " is already in the chain — adjust it there, or remove it first." };
    }
    // Enter at the canonical position: deinterlace → stabilize → denoise →
    // finish. The operator can move it afterwards; order is part of the
    // recipe, so this default is a starting point, not a rule.
    var rank = function (moduleId) { return D.moduleOrder.indexOf(moduleId); };
    var index = state.chain.length;
    for (var i = 0; i < state.chain.length; i++) {
      if (rank(state.chain[i].module) > rank(id)) { index = i; break; }
    }
    snapshot(state);
    state.chain.splice(index, 0, { module: id, params: Object.assign({}, D.modules[id].defaults), open: true });
    state.selectedStep = index;
    state.previewed = false;
    return { ok: true, index: index };
  }

  function removeStep(state, index) {
    if (index < 0 || index >= state.chain.length) return { ok: false, error: "Step " + (index + 1) + " no longer exists; the chain changed." };
    snapshot(state);
    var removed = state.chain.splice(index, 1)[0];
    state.selectedStep = Math.max(0, Math.min(index, state.chain.length - 1));
    state.previewed = false;
    return { ok: true, removed: removed, index: index };
  }

  function insertStep(state, index, step) {
    if (hasModule(state, step.module)) {
      return { ok: false, error: D.modules[step.module].label + " is already in the chain." };
    }
    snapshot(state);
    state.chain.splice(Math.max(0, Math.min(index, state.chain.length)), 0, step);
    state.previewed = false;
    return { ok: true };
  }

  function moveStep(state, index, direction) {
    var target = index + direction;
    if (index < 0 || index >= state.chain.length) return false;
    if (target < 0 || target >= state.chain.length) return false;
    snapshot(state);
    var tmp = state.chain[index];
    state.chain[index] = state.chain[target];
    state.chain[target] = tmp;
    state.selectedStep = target;
    state.previewed = false;
    return true;
  }

  /**
   * Validate and apply a parameter patch; refused values change nothing.
   * An optional gesture id coalesces the edit into the gesture's single
   * undo entry (slider drag, numeric-field focus session).
   */
  function updateParams(state, index, patch, gesture) {
    var step = state.chain[index];
    if (!step) return { ok: false, error: "Step " + (index + 1) + " no longer exists." };
    var next = Object.assign({}, step.params, patch);
    var error = validateParams(step.module, next);
    if (error) return { ok: false, error: error };
    snapshot(state, gesture);
    step.params = next;
    state.previewed = false;
    return { ok: true };
  }

  function validateParams(module, p) {
    function range(name, value, min, max, unit) {
      if (!isFinite(value) || value < min || value > max) {
        return name + " must be between " + min + " and " + max + (unit || "") + ".";
      }
      return null;
    }
    if (module === "stabilize") {
      return range("Smoothing", p.smoothing, LIMITS.smoothing[0], LIMITS.smoothing[1]) ||
        range("Zoom", p.zoom, LIMITS.zoom[0], LIMITS.zoom[1], "%");
    }
    if (module === "denoise") {
      return range("Luma", p.luma, 0, 20) || range("Chroma", p.chroma, 0, 20) || range("Temporal", p.temporal, 0, 20);
    }
    if (module === "finish" && p.crop === "custom") {
      return range("Scale width", p.scaleWidth, LIMITS.scale[0], LIMITS.scale[1]) ||
        range("Scale height", p.scaleHeight, LIMITS.scale[0], LIMITS.scale[1]);
    }
    return null;
  }

  // ------------------------------------------------------------- filter text
  function fmtNum(v) {
    return Number.isInteger(v) ? String(v) : String(Number(v.toFixed(3)));
  }

  function filterFor(step) {
    var p = step.params;
    switch (step.module) {
      case "deinterlace": {
        var parity = p.fieldOrder === "bff" ? "bff" : p.fieldOrder === "tff" ? "tff" : "auto";
        var deint = p.onlyFlagged ? "interlaced" : "all";
        return p.engine + "=mode=" + p.outputRate + ":parity=" + parity + ":deint=" + deint;
      }
      case "stabilize":
        return "vidstabtransform=input=transforms.trf:smoothing=" + p.smoothing +
          ":zoom=" + fmtNum(p.zoom) + ":tripod=" + (p.tripod ? 1 : 0);
      case "denoise":
        if (p.engine === "nlmeans") return "nlmeans=s=" + fmtNum(p.luma) + ":p=7:r=15";
        return "hqdn3d=" + fmtNum(p.luma) + ":" + fmtNum(p.chroma) + ":" + fmtNum(p.temporal) + ":" + fmtNum(p.temporal);
      case "finish": {
        var parts = [];
        if (p.crop === "blanking") parts.push("crop=704:560:8:8");
        if (p.crop === "custom") parts.push("crop=" + p.cropWidth + ":" + p.cropHeight + ":0:0");
        if (p.scaleWidth && p.scaleHeight && (p.scaleWidth !== 720 || p.scaleHeight !== 576)) {
          parts.push("scale=" + p.scaleWidth + ":" + p.scaleHeight);
        }
        if (p.deflicker) parts.push("deflicker");
        if (p.normalizeLevels) parts.push("normalize");
        return parts.length ? parts.join(",") : "null";
      }
      default:
        return "";
    }
  }

  function chainFilter(state) {
    if (!state.chain.length) return { ok: false, filter: "", error: "a restoration must do at least one thing, explicitly" };
    var fragments = [];
    for (var i = 0; i < state.chain.length; i++) {
      var step = state.chain[i];
      if (step.module === "deinterlace" && step.params.engine === "qtgmc") {
        return {
          ok: false,
          filter: "",
          error: "operation requires the VapourSynth pipeline (QTGMC); the FFmpeg pipeline will not substitute a different engine"
        };
      }
      fragments.push(filterFor(step));
    }
    return { ok: true, filter: fragments.join(","), error: null };
  }

  function summaryFor(step) {
    var p = step.params;
    switch (step.module) {
      case "deinterlace":
        return "Deinterlace · " + p.engine + " · " + (p.fieldOrder === "auto" ? "auto" : p.fieldOrder) + " · " + p.outputRate;
      case "stabilize":
        return p.tripod ? "Stabilize · tripod" :
          "Stabilize · smoothing " + p.smoothing + " · zoom " + Number(p.zoom).toFixed(1) + "%";
      case "denoise":
        return "Denoise · " + p.engine + " · luma " + p.luma + " · chroma " + p.chroma + " · temporal " + p.temporal;
      case "finish": {
        var bits = [];
        if (p.crop !== "none") bits.push("crop " + p.crop);
        if (p.scaleWidth !== 720 || p.scaleHeight !== 576) bits.push("scale " + p.scaleWidth + "×" + p.scaleHeight);
        if (p.deflicker) bits.push("deflicker");
        if (p.normalizeLevels) bits.push("levels");
        return "Finish · " + (bits.length ? bits.join(" · ") : "no change yet");
      }
      default:
        return step.module;
    }
  }

  /** Consequences of a step's current values — the copy beside the controls. */
  function watchPoints(step) {
    var p = step.params;
    var out = [];
    if (step.module === "deinterlace") {
      if (p.outputRate === "send_field") {
        out.push("One frame per field: the output has twice the frames of the source (25i → 50p). The master will be roughly twice the size and the render twice as long.");
      }
      if (p.fieldOrder === "auto") {
        out.push("Auto follows the stream's field-order flag. When a capture's flag is wrong, force the order the preview shows as correct.");
      }
    }
    if (step.module === "stabilize") {
      if (p.tripod) out.push("Tripod mode holds every shot still, including deliberate camera moves. It suits locked-off transfers, not handheld footage.");
      if (p.zoom > 10) out.push("Zoom " + Number(p.zoom).toFixed(1) + "% crops about " + Math.round((1 - 1 / Math.pow(1 + p.zoom / 100, 2)) * 100) + "% of the frame area to hide the border.");
    }
    if (step.module === "denoise" && p.temporal > 0) {
      if (p.temporal > 8) out.push("Temporal strength " + p.temporal + " smears motion and erases grain. Preview a moving shot before committing.");
      else out.push("Temporal denoise compares neighbouring frames; fast motion and flashes can smear. Zero it to leave grain alone.");
    }
    if (step.module === "finish" && p.normalizeLevels) {
      out.push("Level correction moves values that were out of broadcast range. It clips by definition; a preservation master may prefer the untouched range.");
    }
    return out;
  }

  /** Why this chain must be previewed first, or null. */
  function previewGate(state) {
    for (var i = 0; i < state.chain.length; i++) {
      var s = state.chain[i];
      var p = s.params;
      if (s.module === "deinterlace" && p.engine === "qtgmc") {
        return "The archival deinterlace tier is slow and its motion handling differs from the preview-grade engines; judge a segment first.";
      }
      if (s.module === "denoise" && p.engine === "nlmeans") {
        return "Quality denoise removes detail no later step can restore; judge it on the worst seconds first.";
      }
      if (s.module === "denoise" && p.temporal > 8) {
        return "Temporal strength " + p.temporal + " smears motion and erases grain. Preview a moving shot before committing.";
      }
      if (s.module === "stabilize" && p.zoom > 10) {
        return "Zoom " + Number(p.zoom).toFixed(1) + "% crops " +
          Math.round((1 - 1 / Math.pow(1 + p.zoom / 100, 2)) * 100) + "% of the frame area to hide the border. Confirm the framing in a preview.";
      }
    }
    return null;
  }

  // ------------------------------------------------------------- measurement
  function seriesFor(clip) {
    if (!clip) return null;
    if (!clip._series) clip._series = D.buildSeries(clip);
    return clip._series;
  }

  function laneById(clip, id) {
    var series = seriesFor(clip);
    return series ? series.find(function (l) { return l.id === id; }) : null;
  }

  function valueAt(lane, secs, rate) {
    if (!lane || !isFinite(secs)) return null;
    var idx = Math.round(secs * D.HZ);
    if (idx < 0 || idx >= lane.values.length) return null;
    return lane.values[idx];
  }

  /** Peak-preserving bucket: min and max of each slice, never the mean. */
  function buckets(lane, count) {
    var v = lane.values;
    if (!v.length || !count) return [];
    if (v.length <= count) return v.map(function (x) { return { min: x, max: x }; });
    var out = [];
    for (var i = 0; i < count; i++) {
      var start = Math.floor((i * v.length) / count);
      var end = Math.max(start + 1, Math.floor(((i + 1) * v.length) / count));
      var min = Infinity, max = -Infinity;
      for (var j = start; j < end && j < v.length; j++) {
        if (v[j] < min) min = v[j];
        if (v[j] > max) max = v[j];
      }
      out.push({ min: min, max: max });
    }
    return out;
  }

  function marks(clip) {
    if (!clip) return [];
    return (clip.anomalies || []).map(function (a) {
      if (a.kind === "outlier") {
        return {
          kind: "outlier", start: a.start, end: a.start, frame: a.frame,
          label: "Dropout risk", detail: "Temporal outlier at " + f.durationShort(a.start) + " (frame " + a.frame + ", TOUT " + a.tout.toFixed(2) + ")"
        };
      }
      if (a.kind === "freeze") {
        return { kind: "freeze", start: a.start, end: a.end, label: "Freeze", detail: "Freeze span " + f.durationShort(a.start) + "–" + f.durationShort(a.end) };
      }
      return { kind: "black", start: a.start, end: a.end, label: a.label || "Black", detail: "Black span " + f.durationShort(a.start) + "–" + f.durationShort(a.end) };
    });
  }

  function marksIn(clip, range) {
    if (!range) return [];
    return marks(clip).filter(function (m) { return m.end >= range.in && m.start <= range.out; });
  }

  function suggestedRange(state, spanSecs) {
    var clip = clipMeta(state);
    if (!clip) return null;
    var span = Math.min(spanSecs || 20, clip.duration);
    var list = marks(clip);
    var outliers = list.filter(function (m) { return m.kind === "outlier"; });
    var anchor = outliers.length ? outliers[0].start : list.length ? list[0].start : 0;
    var start = Math.max(0, anchor - span / 2);
    start = Math.min(start, Math.max(0, clip.duration - span));
    return { in: start, out: start + span, anchor: anchor };
  }

  // ------------------------------------------------------------- preflight
  function estimateBytes(state) {
    var clip = clipMeta(state);
    if (!clip) return null;
    var preset = D.presets.find(function (p) { return p.id === state.presetId; });
    if (!preset) return null;
    var factor = preset.facts.sizeFactor;
    var step = state.chain.find(function (s) { return s.module === "deinterlace"; });
    if (step && step.params.outputRate === "send_field") factor *= 1.9;
    return Math.round(clip.sizeBytes * factor);
  }

  function preflight(state) {
    var clip = clipMeta(state);
    var plan = chainFilter(state);
    var gate = previewGate(state);
    var preset = D.presets.find(function (p) { return p.id === state.presetId; });
    var estimate = estimateBytes(state);
    var outExists = !!state.exported; // the prototype's "destination" check
    var sidecar = false;
    var checks = [];

    checks.push(!state.chain.length || !plan.ok
      ? {
        id: "chain", label: "Chain", level: "blocked",
        detail: !state.chain.length
          ? "The chain is empty. Add at least one operation before exporting."
          : "The planner refuses this chain: " + plan.error
      }
      : {
        id: "chain", label: "Chain plans", level: "pass",
        detail: state.chain.length + " operation" + (state.chain.length === 1 ? "" : "s") + " planned in order. The exact command is in the plan below."
      });

    checks.push(state.previewed
      ? { id: "preview", label: "Segment preview", level: "pass", detail: "A segment of this chain rendered and its proof was written." }
      : gate
        ? state.override
          ? { id: "preview", label: "Segment preview", level: "warn", detail: "No preview. An expert override will be recorded in the export log so the decision is visible later." }
          : { id: "preview", label: "Segment preview", level: "blocked", detail: "This chain is aggressive enough to need a preview first: " + gate }
        : { id: "preview", label: "Segment preview", level: "warn", detail: "No preview yet. Rendering a 20-second segment first is quick, and catches settings that look wrong in motion." });

    checks.push(outExists || sidecar
      ? { id: "destination", label: "Destination", level: "blocked", detail: "A file already exists at the output path. Outputs are always new files — choose another name." }
      : { id: "destination", label: "Destination", level: "pass", detail: "The output path is free; the recipe and export log will be written beside it." });

    checks.push(clip && clip.fixity && clip.fixity.state === "ok"
      ? { id: "fixity", label: "Source fixity", level: "pass", detail: "The SHA-256 measured at ingest matches the file. The export log will record it again at run time." }
      : { id: "fixity", label: "Source fixity", level: "blocked", detail: "This source's fixity is not verified. A run refuses when the recipe pins a hash the file no longer matches; re-check fixity first." });

    checks.push(estimate
      ? { id: "space", label: "Disk space", level: D.project.freeBytes >= estimate * 1.1 ? "pass" : "blocked",
        detail: f.bytes(D.project.freeBytes) + " free; estimated output " + f.bytes(estimate) + " (" + preset.format.split(" · ")[0] + ", estimate) plus 10% headroom." }
      : { id: "space", label: "Disk space", level: "unknown", detail: "Free space was not measured for the destination. Measure it before a long render." });

    return checks;
  }

  function ready(checks) {
    return checks.every(function (c) { return c.level !== "blocked"; });
  }

  function exportCommand(state) {
    var clip = clipMeta(state);
    var plan = chainFilter(state);
    var preset = D.presets.find(function (p) { return p.id === state.presetId; });
    if (!clip || !plan.ok || !preset) return null;
    var codec = preset.id === "preservation"
      ? ["-c:v", "ffv1", "-level", "3", "-coder", "1", "-context", "1", "-g", "1", "-slices", "24", "-slicecrc", "1", "-c:a", "flac"]
      : preset.id === "mezzanine"
        ? ["-c:v", "prores_ks", "-profile:v", "3", "-vendor", "apl0", "-pix_fmt", "yuv422p10le", "-c:a", "pcm_s16le"]
        : ["-c:v", "libx264", "-crf", "18", "-preset", "slow", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k"];
    return ["ffmpeg", "-hide_banner", "-i", clip.file, "-vf", plan.filter]
      .concat(codec)
      .concat(["-n", state.destination]);
  }

  function sizeEstimateText(state) {
    var estimate = estimateBytes(state);
    return estimate ? f.bytes(estimate) : "—";
  }

  // ------------------------------------------------------------- verification
  function verification(state) {
    var out = state.exported;
    if (!out) return null;
    return {
      verdict: out.verified === false ? "fail" : "ok",
      checks: [
        { name: "Run succeeded", status: "pass", detail: "The export log records success = true and carries an output hash." },
        { name: "Output hash", status: "pass", detail: "The file hashes to " + f.shortHash(out.sha256) + ", matching the export log." },
        { name: "Source hash", status: "pass", detail: "The source hashes to " + f.shortHash(out.sourceSha256) + ", matching the recipe and the export log." },
        { name: "Preset match", status: "pass", detail: "Recipe and export log both name " + out.presetLabel + "." },
        { name: "Schema match", status: "pass", detail: "Recipe and export log both name schema version 1." }
      ],
      skips: [
        { name: "Example — source not found", status: "skip", detail: "A check whose evidence is missing is reported SKIP, never PASS." }
      ]
    };
  }

  // ------------------------------------------------------------- jobs
  // Mirror of crates/rd-ui/src/jobs.rs: an explicit lifecycle with
  // measured-only progress and cooperative cancellation. The app's
  // timers simulate the engine; these rules are the contract.
  var jobSeq = 0;
  var JOB_HISTORY_LIMIT = 100;

  function jobWord(stateName) {
    return {
      queued: "Queued",
      running: "Running",
      cancelling: "Cancelling",
      done: "Done",
      failed: "Failed",
      cancelled: "Cancelled"
    }[stateName] || stateName;
  }

  /** Accept a job (Queued). Returns its id. totalFrames, when known,
      turns measured frame counts into an honest percentage. */
  function enqueueJob(state, opts) {
    var id = "job-" + (++jobSeq);
    state.jobs.push({
      id: id,
      label: opts.label,
      sub: opts.sub || "",
      kind: opts.kind,
      sourceId: opts.sourceId == null ? state.selectedId : opts.sourceId,
      totalFrames: opts.totalFrames == null ? null : opts.totalFrames,
      state: "queued",
      progress: { frames: 0, fps: 0, speed: 0, outTimeUs: 0, totalSize: 0, percent: null },
      outcome: null,
      note: ""
    });
    if (state.jobs.length > JOB_HISTORY_LIMIT) {
      // Evict the oldest terminal entry, never an active one.
      var idx = -1;
      for (var i = 0; i < state.jobs.length; i++) {
        if (["done", "failed", "cancelled"].indexOf(state.jobs[i].state) >= 0) { idx = i; break; }
      }
      state.jobs.splice(idx < 0 ? 0 : idx, 1);
    }
    return id;
  }

  function findJob(state, id) {
    for (var i = 0; i < state.jobs.length; i++) {
      if (state.jobs[i].id === id) return state.jobs[i];
    }
    return null;
  }

  function illegal(job, to) {
    return { ok: false, error: "job " + (job ? job.id : "?") + " cannot move to " + to + " from " + (job ? job.state : "unknown") };
  }

  /** Queued → Running. */
  function startJobModel(state, id) {
    var job = findJob(state, id);
    if (!job) return { ok: false, error: "unknown job " + id };
    if (job.state !== "queued") return illegal(job, "running");
    job.state = "running";
    return { ok: true };
  }

  /** Apply one measured engine block while running or cancelling. */
  function jobProgress(state, id, measured) {
    var job = findJob(state, id);
    if (!job) return { ok: false, error: "unknown job " + id };
    if (job.state !== "running" && job.state !== "cancelling") return illegal(job, "running");
    job.progress.frames = measured.frame || 0;
    job.progress.fps = measured.fps || 0;
    job.progress.speed = measured.speed || 0;
    job.progress.outTimeUs = measured.outTimeUs || 0;
    job.progress.totalSize = measured.totalSize || 0;
    if (job.totalFrames) {
      job.progress.percent = Math.min(100, Math.round((job.progress.frames * 100) / job.totalFrames));
    } else {
      job.progress.percent = null;
    }
    return { ok: true };
  }

  /** Running → Cancelling; idempotent while already cancelling. */
  function requestCancel(state, id) {
    var job = findJob(state, id);
    if (!job) return { ok: false, error: "unknown job " + id };
    if (job.state === "cancelling") return { ok: true };
    if (job.state !== "running") return illegal(job, "cancelling");
    job.state = "cancelling";
    return { ok: true };
  }

  /** Only a running job can succeed; a cancelled one cannot also succeed. */
  function succeedJob(state, id, outcome) {
    var job = findJob(state, id);
    if (!job) return { ok: false, error: "unknown job " + id };
    if (job.state !== "running") return illegal(job, "done");
    job.state = "done";
    job.progress.percent = 100;
    job.outcome = outcome || null;
    job.note = "complete";
    // Evidence wiring mirrors ShellModel::job_succeeded.
    if (job.kind === "preview") {
      state.previewed = true;
      if (state.sourceFlags[state.selectedId]) state.sourceFlags[state.selectedId].previewed = true;
      else state.sourceFlags[state.selectedId] = { previewed: true, override: false, exported: null, verified: null };
    }
    return { ok: true };
  }

  function failJob(state, id, detail) {
    var job = findJob(state, id);
    if (!job) return { ok: false, error: "unknown job " + id };
    if (["done", "failed", "cancelled"].indexOf(job.state) >= 0) return illegal(job, "failed");
    job.state = "failed";
    job.note = detail || "failed";
    return { ok: true };
  }

  /** Cancelling → Cancelled: the engine stopped and removed partial output. */
  function confirmCancelled(state, id) {
    var job = findJob(state, id);
    if (!job) return { ok: false, error: "unknown job " + id };
    if (job.state !== "cancelling") return illegal(job, "cancelled");
    job.state = "cancelled";
    job.note = "cancelled";
    job.progress.percent = null;
    return { ok: true };
  }

  global.RD.model = {
    LIMITS: LIMITS,
    defaultPrefs: defaultPrefs,
    initialState: initialState,
    savePrefs: savePrefs,
    clipMeta: clipMeta,
    stageById: stageById,
    access: access,
    nextStageWithAccess: nextStageWithAccess,
    hasModule: hasModule,
    findStep: findStep,
    addModule: addModule,
    removeStep: removeStep,
    insertStep: insertStep,
    moveStep: moveStep,
    updateParams: updateParams,
    validateParams: validateParams,
    canUndo: canUndo,
    canRedo: canRedo,
    undo: undo,
    redo: redo,
    beginGesture: beginGesture,
    enqueueJob: enqueueJob,
    startJobModel: startJobModel,
    jobProgress: jobProgress,
    requestCancel: requestCancel,
    confirmCancelled: confirmCancelled,
    succeedJob: succeedJob,
    failJob: failJob,
    jobWord: jobWord,
    filterFor: filterFor,
    chainFilter: chainFilter,
    summaryFor: summaryFor,
    watchPoints: watchPoints,
    previewGate: previewGate,
    seriesFor: seriesFor,
    laneById: laneById,
    valueAt: valueAt,
    buckets: buckets,
    marks: marks,
    marksIn: marksIn,
    suggestedRange: suggestedRange,
    estimateBytes: estimateBytes,
    preflight: preflight,
    ready: ready,
    exportCommand: exportCommand,
    sizeEstimateText: sizeEstimateText,
    verification: verification
  };
})(window);
