/* ReelDeck prototype — rendering, events, and feedback.
   This file is deliberately a shell: it presents state and dispatches
   actions. The rules it enforces live in js/model.js (the mirror of
   crates/rd-ui/src). */
(function (global) {
  "use strict";

  var RD = global.RD;
  var D = RD.data;
  var M = RD.model;
  var U = RD.ui;
  var f = RD.format;

  var state = M.initialState();
  var selectedStep = 0;
  var palette = { items: [], index: 0, query: "" };
  var lastFocus = null;
  var jobSeq = 0;
  var timers = {};

  var $ = function (id) { return document.getElementById(id); };

  // ------------------------------------------------------------------ boot
  function init() {
    U.hydrateIcons(document);
    applyPrefs();
    bindEvents();
    renderAll();
    // The prototype opens on the source it can say the most about.
    setStage("inspect", true);
  }

  function applyPrefs() {
    var p = state.prefs;
    var root = document.documentElement;
    root.dataset.accent = p.accent;
    root.dataset.density = p.density;
    root.dataset.contrast = p.highContrast ? "high" : "normal";
    root.dataset.reduceMotion = p.reduceMotion ? "true" : "false";
    state.viewer.safe = p.safeAreas;
  }

  // ------------------------------------------------------------------ render
  function renderAll() {
    renderStages();
    renderContext();
    renderQueue();
    renderViewer();
    renderTimeline();
    renderJobs();
    renderInspector();
    renderStatus();
  }

  function renderStages() {
    var rail = $("stage-rail");
    rail.innerHTML = D.stages.map(function (s, i) {
      var a = M.access(state, s.id);
      var current = state.stage === s.id;
      return '<button class="stage" role="link" data-action="stage" data-stage="' + s.id + '"' +
        ' aria-current="' + (current ? "step" : "false") + '"' +
        ' data-blocked="' + (!a.ok) + '"' +
        ' title="' + U.esc(a.ok ? s.purpose : a.reason) + '">' +
        '<span class="idx">' + (i + 1) + '</span>' +
        '<span class="stage-label">' + s.label + "</span>" +
        (!a.ok && !current ? '<span class="blocked-dot" aria-label="needs evidence"></span>' : "") +
        "</button>";
    }).join("");
  }

  function renderContext() {
    var stage = M.stageById(state.stage);
    $("context-stage").textContent = stage.label;
    $("context-purpose").textContent = stage.purpose;
    var actions = [];
    var clip = M.clipMeta(state);
    var plan = M.chainFilter(state);
    var gate = M.previewGate(state);
    var checks = M.preflight(state);

    if (state.stage === "ingest") {
      actions.push(actionBtn("addSource", "plus", "Add source…", "primary", "Ctrl+O"));
      actions.push(actionBtn("rehash", "reset", "Re-check fixity", "ghost"));
    } else if (state.stage === "inspect") {
      if (clip && clip.inspect.state === "done") {
        actions.push('<span class="pill" data-state="ok" title="Interlace suggestion confirmed by the operator">Field order confirmed · ' + clip.inspect.confirmed.toUpperCase() + "</span>");
      }
      actions.push(actionBtn("rerunInspect", "target", "Re-run inspection", "ghost"));
    } else if (state.stage === "decide") {
      actions.push(actionBtn("addModule", "plus", "Add module", "ghost", "A"));
      actions.push(actionBtn("tab", "layers", "Export", "ghost", null, 'data-tab="export"'));
    } else if (state.stage === "preview") {
      actions.push(actionBtn("suggestRange", "target", "Suggest range", "ghost", "Shift+I"));
      actions.push(actionBtn("renderPreview", "film", "Render preview", "primary", "Ctrl+Enter"));
    } else if (state.stage === "export") {
      actions.push(actionBtn("renderExport", "shield", "Render export", M.ready(checks) ? "primary" : "ghost", "Ctrl+Shift+Enter"));
    } else if (state.stage === "prove") {
      actions.push(actionBtn("verify", "check", "Verify output", state.exported ? "primary" : "ghost", "Ctrl+Shift+V"));
    }

    // A single line of live status on the right of the stage title.
    if (state.stage === "decide" && plan.ok) {
      actions.unshift('<span class="faint mono" style="font-size:9.5px;max-width:340px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="' +
        U.esc(plan.filter) + '">' + U.esc(plan.filter) + "</span>");
    }
    if (gate && !state.previewed && !state.override && (state.stage === "export" || state.stage === "preview")) {
      actions.unshift('<span class="flag">' + U.icon("warning") + "Preview required</span>");
    }
    $("context-actions").innerHTML = actions.join("");
  }

  function actionBtn(action, iconName, label, variant, kbd, extra) {
    return '<button class="btn btn-' + (variant || "ghost") + '" data-action="' + action + '" ' + (extra || "") + ">" +
      U.icon(iconName) + "<span>" + label + "</span>" +
      (kbd ? '<kbd class="kbd">' + kbd + "</kbd>" : "") + "</button>";
  }

  function renderQueue() {
    var body = $("queue-body");
    var filter = state.queueFilter.toLowerCase();
    var rows = D.clips.filter(function (c) {
      return !filter || (c.name + " " + c.file + " " + c.sub).toLowerCase().indexOf(filter) >= 0;
    });
    body.innerHTML = rows.map(function (c) {
      var fix = fixityPill(c);
      var insp = inspectPill(c);
      var selected = c.id === state.selectedId;
      return '<tr data-action="select-clip" data-id="' + c.id + '" data-selected="' + selected + '"' +
        ' aria-selected="' + selected + '" tabindex="' + (selected ? "0" : "-1") + '">' +
        '<td><span class="clip-name"><img class="queue-thumb" alt="" src="assets/media/frame-ashland.jpg">' +
        '<span class="clip-label"><b class="truncate">' + U.esc(c.name) + "</b><span>" + U.esc(c.sub) + "</span></span></span></td>" +
        '<td><span class="row tight">' + fix + insp + "</span></td>" +
        '<td class="num mono-num faint">' + f.duration(c.duration) + "</td></tr>";
    }).join("");
    $("queue-empty").hidden = rows.length > 0;
    $("queue-count").textContent = D.clips.length + " files";
    var total = D.clips.reduce(function (sum, c) { return sum + c.duration; }, 0);
    var bytes = D.clips.reduce(function (sum, c) { return sum + c.sizeBytes; }, 0);
    $("queue-foot").innerHTML = '<span class="row tight">' + U.icon("film") +
      "<span>" + D.clips.length + " sources · " + f.duration(total) + " · " + f.bytes(bytes) + "</span></span>";

    var active = M.clipMeta(state);
    $("status-clip").textContent = active ? active.id + " · " + active.name : "No source selected";
  }

  function fixityPill(c) {
    if (c.fixity.state === "ok") return U.pill("Hash match", "ok", "SHA-256 matches the value recorded at ingest");
    if (c.fixity.state === "mismatch") return U.pill("Hash mismatch", "err", "The file changed since ingest. Sources are never modified by ReelDeck; restore the original before continuing.");
    return U.pill("Hashing", "idle", "SHA-256 is being computed");
  }

  function inspectPill(c) {
    if (c.inspect.state === "done") {
      return U.pill("Inspected", "ok", "Measurements recorded for " + c.inspect.sampled);
    }
    if (c.inspect.state === "failed") return U.pill("Inspect failed", "err", "The inspection tools did not finish; see the Source panel.");
    if (c.inspect.state === "inspecting") return U.pill("Inspecting", "info", "Analysis running");
    return U.pill("Not inspected", "idle", "No measurements yet");
  }

  // ------------------------------------------------------------------ viewer
  var MODES = [
    { id: "program", label: "Program", icon: "film", key: "P" },
    { id: "waveform", label: "Waveform", icon: "wave", key: "W" },
    { id: "histogram", label: "Histogram", icon: "layers", key: "H" },
    { id: "vectorscope", label: "Vectorscope", icon: "target", key: "U" },
    { id: "field", label: "Field split", icon: "grid", key: "F" },
    { id: "temporal", label: "Temporal diff", icon: "compare", key: "D" }
  ];

  function renderViewer() {
    var clip = M.clipMeta(state);
    var modes = $("viewer-modes");
    modes.innerHTML = MODES.map(function (m) {
      return '<button class="mode-btn" data-action="view-mode" data-mode="' + m.id + '"' +
        ' aria-pressed="' + (state.viewer.mode === m.id) + '" title="' + m.label + " — " + m.key + '">' +
        U.icon(m.icon) + '<span class="mode-text">' + m.label + "</span></button>";
    }).join("");

    var single = $("single-frame");
    var compare = $("compare-wrap");
    var stack = $("frame-stack");
    var isScope = ["waveform", "histogram", "vectorscope"].indexOf(state.viewer.mode) >= 0;
    var isField = state.viewer.mode === "field";

    compare.hidden = !state.viewer.compare;
    single.style.visibility = state.viewer.compare ? "hidden" : "visible";
    $("field-wrap").hidden = !isField;
    stack.classList.toggle("zoom-100", state.viewer.zoom100);

    single.style.filter = "";
    single.style.clipPath = "";
    if (state.viewer.mode === "temporal") {
      single.style.filter = "invert(1) grayscale(1) contrast(2.1) brightness(0.85)";
    }
    if (isField) {
      single.style.visibility = "hidden";
      $("field-wrap").querySelector(".field-left").style.filter = "saturate(0.95) contrast(1.04)";
      $("field-wrap").querySelector(".field-right").style.filter = "saturate(0.95) contrast(0.9) blur(0.5px)";
    }

    $("scope-overlay").hidden = !isScope;
    if (isScope) {
      var lane = M.laneById(clip, "y_avg");
      var canvas = $("scope-canvas");
      $("scope-caption").textContent =
        state.viewer.mode === "waveform" ? "Waveform · luma" :
        state.viewer.mode === "histogram" ? "Histogram · luma" : "Vectorscope · chroma";
      requestAnimationFrame(function () {
        if (state.viewer.mode === "waveform") U.drawWaveform(canvas, lane);
        else if (state.viewer.mode === "histogram") U.drawHistogram(canvas, lane);
        else U.drawVectorscope(canvas, lane || { values: [] });
      });
    }

    $("safe-guides").hidden = !state.viewer.safe;
    $("btn-compare").setAttribute("aria-pressed", String(state.viewer.compare));
    $("btn-zoom").setAttribute("aria-pressed", String(state.viewer.zoom100));
    $("btn-zoom").querySelector("span:last-child").textContent = state.viewer.zoom100 ? "100%" : "Fit";
    $("btn-safe").setAttribute("aria-pressed", String(state.viewer.safe));

    var modeLabel = MODES.find(function (m) { return m.id === state.viewer.mode; });
    $("hud-mode").textContent = (state.viewer.compare ? "COMPARE · " : "") + (modeLabel ? modeLabel.label.toUpperCase() : "PROGRAM");
    $("hud-mode").style.color = state.viewer.compare ? "var(--accent)" : "";

    $("hud-title").textContent = clip ? clip.id + " · " + clip.name + " (" + (clip.sub.split("·")[1] || "").trim() + ")" : "No source";
    $("hud-facts").textContent = clip ? (clip.width + "×" + clip.height + " · " + f.rateText(clip.rate) + " fps · " + (clip.fieldOrderFlag === "tt" ? "TFF flagged" : clip.fieldOrderFlag === "bb" ? "BFF flagged" : "progressive")) : "—";

    renderReadout();
    renderFramePosition();
  }

  function renderReadout() {
    var clip = M.clipMeta(state);
    var secs = state.viewer.playhead;
    var readout = $("viewer-readout");
    if (!clip || !clip.inspect || clip.inspect.state !== "done") {
      readout.innerHTML = '<span class="faint">No measurements for this source yet.</span>';
      $("viewer-note").textContent = clip && clip.fixity.state === "mismatch"
        ? "Fixity mismatch: restore the original file before restoring it."
        : "Inspect this source to populate the measurement lanes.";
      return;
    }
    var y = M.valueAt(M.laneById(clip, "y_avg"), secs, clip.rate);
    var sat = M.valueAt(M.laneById(clip, "sat_avg"), secs, clip.rate);
    var tout = M.valueAt(M.laneById(clip, "tout"), secs, clip.rate);
    var brng = M.valueAt(M.laneById(clip, "brng"), secs, clip.rate);
    function cell(label, value, unit) {
      return "<span>" + label + " <b>" + (value == null ? "—" : value.toFixed(unit === " y" ? 1 : 3)) + "</b></span>";
    }
    readout.innerHTML = cell("Y", y) + cell("SAT", sat) + cell("TOUT", tout) +
      "<span>Frame <b>" + Math.round(secs * f.fps(clip.rate)) + "</b></span>" +
      (tout != null && tout > 0.05 ? '<span class="flag">' + U.icon("warning") + "Outlier above threshold</span>" : "");
  }

  function renderFramePosition() {
    var clip = M.clipMeta(state);
    if (!clip) return;
    $("tc-current").textContent = f.timecode(Math.round(state.viewer.playhead * f.fps(clip.rate)), clip.rate);
    $("tc-total").textContent = f.timecode(Math.round(clip.duration * f.fps(clip.rate)), clip.rate);
    $("hud-time").textContent = f.timecodeAt(state.viewer.playhead, clip.rate);
    var rangeEl = $("tc-range");
    if (state.range) {
      rangeEl.textContent = f.durationShort(state.range.in) + " → " + f.durationShort(state.range.out) +
        " (" + f.duration(state.range.out - state.range.in) + ")";
      rangeEl.classList.remove("dim");
    } else {
      rangeEl.textContent = "no range";
      rangeEl.classList.add("dim");
    }
    updatePlayhead();
  }

  // ------------------------------------------------------------------ timeline
  function renderTimeline() {
    var clip = M.clipMeta(state);
    if (!clip) return;
    var r = $("ruler");
    var inner = $("timeline-inner");
    var width = Math.max(320, inner.clientWidth - gutter());
    var ticks = U.rulerTicks(clip.duration, clip.rate, width);

    r.innerHTML = '<div class="ruler-spacer">' + f.duration(clip.duration) + "</div>" +
      '<div class="ruler-track" id="ruler-track">' +
      ticks.ticks.map(function (t) {
        var pct = (t.secs / clip.duration) * 100;
        return '<span class="ruler-tick' + (t.major ? "" : " minor") + '" style="left:' + pct + '%">' +
          (t.major ? f.durationShort(t.secs) : "") + "</span>";
      }).join("") + "</div>";

    $("clip-name").textContent = clip.name;

    // Mark ticks on the picture track.
    var track = $("track-area");
    track.querySelectorAll(".mark-tick").forEach(function (n) { n.remove(); });
    M.marks(clip).forEach(function (m, i) {
      var pct = (m.start / clip.duration) * 100;
      var tick = document.createElement("button");
      tick.className = "mark-tick";
      tick.style.left = pct + "%";
      tick.dataset.kind = m.kind;
      tick.dataset.action = "jump-mark";
      tick.dataset.index = String(i);
      tick.title = m.detail;
      tick.setAttribute("aria-label", "Go to " + m.detail);
      tick.innerHTML = m.kind === "outlier" ? "!" : m.kind === "freeze" ? "F" : "B";
      track.appendChild(tick);
    });

    // Measurement lanes.
    var lanes = $("lanes");
    lanes.dataset.open = String(state.lanesOpen && clip.inspect.state === "done");
    lanes.innerHTML = state.lanesOpen && clip.inspect.state === "done"
      ? M.seriesFor(clip).filter(function (l) { return l.id !== "sat_avg"; }).map(function (lane) {
        var domainText = lane.scaleLabel;
        return '<div class="lane" data-lane="' + lane.id + '">' +
          '<div class="lane-label"><b>' + lane.label + '</b><span>' + domainText + "</span></div>" +
          '<div class="lane-graph" data-seek="lane"><canvas data-lane-canvas="' + lane.id + '"></canvas>' +
          (lane.threshold != null ? '<div class="threshold" data-threshold="' + lane.threshold +
            '" data-label="flag > ' + lane.threshold + '" style="top:' +
            (100 - ((lane.threshold - lane.domain[0]) / (lane.domain[1] - lane.domain[0])) * 100) + '%"></div>' : "") +
          "</div></div>";
      }).join("")
      : "";

    // Draw lanes after layout so canvases have their real size.
    requestAnimationFrame(function () {
      if (!state.lanesOpen || clip.inspect.state !== "done") return;
      lanes.querySelectorAll("canvas[data-lane-canvas]").forEach(function (canvas) {
        var lane = M.laneById(clip, canvas.dataset.laneCanvas);
        var graph = canvas.parentElement;
        var th = graph.querySelector(".threshold");
        U.drawLane(canvas, lane, {
          range: state.range,
          duration: clip.duration,
          thresholdY: lane.threshold != null ? graph.clientHeight * (1 - (lane.threshold - lane.domain[0]) / (lane.domain[1] - lane.domain[0])) : null
        });
      });
    });

    renderMarksStrip();
    updatePlayhead();
    updateSelectionBand();
  }

  function renderMarksStrip() {
    var clip = M.clipMeta(state);
    var strip = $("marks-strip");
    if (!clip || !clip.inspect || clip.inspect.state !== "done") {
      strip.innerHTML = '<span class="eyebrow">Findings</span><span class="none">No inspection yet — findings appear here after analysis.</span>';
      return;
    }
    var marks = M.marks(clip);
    var secs = state.viewer.playhead;
    strip.innerHTML = '<span class="eyebrow">Findings · ' + marks.length + "</span>" +
      marks.map(function (m, i) {
        var glyph = m.kind === "outlier" ? "!" : m.kind === "freeze" ? "F" : "B";
        var active = secs >= m.start - 0.5 && secs <= Math.max(m.end, m.start) + 0.5;
        var text = m.kind === "outlier" ? f.durationShort(m.start) + " · TOUT " + m.detail.split("TOUT ")[1].replace(")", "")
          : m.kind === "freeze" ? f.durationShort(m.start) + " – " + f.durationShort(m.end)
            : m.label + " · " + f.durationShort(m.start) + " – " + f.durationShort(m.end);
        return '<button class="mark-chip" data-action="jump-mark" data-index="' + i + '"' +
          ' data-active="' + active + '" title="' + U.esc(m.detail) + '">' +
          '<span class="glyph" data-kind="' + m.kind + '">' + glyph + "</span>" + U.esc(text) + "</button>";
      }).join("");
  }

  function updatePlayhead() {
    var clip = M.clipMeta(state);
    if (!clip) return;
    var inner = $("timeline-inner");
    var g = gutter();
    var width = Math.max(0, inner.clientWidth - g);
    var x = g + (state.viewer.playhead / clip.duration) * width;
    $("playhead").style.left = x + "px";
    var band = $("selection-band");
    if (state.range) {
      band.hidden = false;
      band.style.left = (g + (state.range.in / clip.duration) * width) + "px";
      band.style.width = Math.max(2, ((state.range.out - state.range.in) / clip.duration) * width) + "px";
    } else {
      band.hidden = true;
    }
  }

  function updateSelectionBand() {
    updatePlayhead();
    var clip = M.clipMeta(state);
    if (!clip) return;
    $("tc-range").textContent = state.range
      ? f.durationShort(state.range.in) + " → " + f.durationShort(state.range.out) + " (" + f.duration(state.range.out - state.range.in) + ")"
      : "no range";
    $("tc-range").classList.toggle("dim", !state.range);
    renderMarksStrip();
    renderContext();
  }

  function seekTo(secs, fromMouse) {
    var clip = M.clipMeta(state);
    if (!clip) return;
    state.viewer.playhead = Math.max(0, Math.min(clip.duration, secs));
    renderFramePosition();
    renderReadout();
    renderMarksStrip();
    if (state.viewer.mode === "waveform" || state.viewer.mode === "histogram" || state.viewer.mode === "vectorscope") {
      renderViewer();
    }
  }

  function seekFromEvent(e, el) {
    var rect = el.getBoundingClientRect();
    var pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    var clip = M.clipMeta(state);
    if (!clip) return;
    seekTo(pct * clip.duration);
  }

  // ------------------------------------------------------------------ jobs
  function renderJobs() {
    var wrap = $("jobs");
    var count = state.jobs.filter(function (j) { return j.state === "running"; }).length;
    $("jobs-count").textContent = count ? count + " running" : "none running";
    var hasAny = state.jobs.length > 0;
    $("jobs-summary").textContent = hasAny
      ? state.jobs[state.jobs.length - 1].label
      : "Renders appear here with measured progress.";
    if (count) wrap.dataset.open = "true";
    $("jobs-head").setAttribute("aria-expanded", wrap.dataset.open);
    wrap.querySelector(".jobs-caret").style.transform = wrap.dataset.open === "true" ? "" : "rotate(-90deg)";

    var body = $("jobs-body");
    if (!hasAny) {
      body.innerHTML = '<div class="job-empty">No jobs. Previews and exports run here, off the interface, and can be cancelled at any time.</div>';
      return;
    }
    body.innerHTML = state.jobs.map(function (j) {
      var pct = j.percent == null ? "measuring…" : j.percent + "%";
      return '<div class="job-row" data-job="' + j.id + '">' +
        '<div class="job-label"><b>' + U.esc(j.label) + '</b>' +
        '<div class="job-sub">' + U.esc(j.sub) + "</div></div>" +
        '<div class="job-progress"><div class="progress" data-state="' + j.state + '"><span style="width:' + (j.percent || 0) + '%"></span></div>' +
        '<span class="job-percent">' + (j.state === "done" ? "done" : j.state === "cancelled" ? "cancelled" : j.state === "error" ? "failed" : pct) + "</span></div>" +
        (j.state === "running"
          ? '<button class="btn btn-sm" data-action="cancel-job" data-job="' + j.id + '">Cancel</button>'
          : '<span class="faint" style="font-size:9.5px;min-width:74px;text-align:right">' + U.esc(j.note || "") + "</span>") +
        "</div>";
    }).join("");
  }

  function startJob(opts) {
    var id = "job-" + (++jobSeq);
    var job = {
      id: id,
      label: opts.label,
      sub: "Simulated in this prototype · " + (opts.sub || "chain as planned"),
      percent: null,
      state: "running",
      note: ""
    };
    state.jobs.push(job);
    renderJobs();
    announce(opts.label + " started.");
    var started = Date.now();
    var duration = opts.duration || 3200;
    var tick = setInterval(function () {
      var elapsed = Date.now() - started;
      var t = Math.min(1, elapsed / duration);
      // Measured-looking curve: fast start, slow tail, like a real render.
      job.percent = Math.min(99, Math.round((1 - Math.pow(1 - t, 1.8)) * 100));
      renderJobs();
      if (t >= 1) {
        clearInterval(tick);
        delete timers[id];
        job.percent = 100;
        job.state = "done";
        job.note = opts.doneNote || "complete";
        renderJobs();
        announce(opts.label + " finished.");
        if (opts.onDone) opts.onDone();
      }
    }, 130);
    timers[id] = tick;
    return id;
  }

  function cancelJob(id) {
    var job = state.jobs.find(function (j) { return j.id === id; });
    if (!job || job.state !== "running") return;
    clearInterval(timers[id]);
    delete timers[id];
    job.state = "cancelled";
    job.note = "cancelled";
    renderJobs();
    toast("warn", "Preview cancelled", "No output was written. The partial file was removed.");
    announce("Job cancelled.");
  }

  // ------------------------------------------------------------------ inspector
  function setTab(tab, focus) {
    state.tab = tab;
    document.querySelectorAll(".inspector-tab").forEach(function (b) {
      var on = b.dataset.tab === tab;
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
      if (on && focus) b.focus();
    });
    renderInspector();
    document.querySelectorAll(".inspector-panel").forEach(function (p) {
      p.hidden = p.id !== "panel-" + tab;
    });
  }

  function renderInspector() {
    $("chain-count").textContent = state.chain.length;
    // All four panels render every time: a hidden panel that still shows
    // last clip's proof would be read out by a screen reader and shown in
    // print, so stale content is a correctness problem, not a paint cost.
    renderSourcePanel();
    renderChainPanel();
    renderExportPanel();
    renderProvePanel();
  }

  function renderSourcePanel() {
    var panel = $("panel-source");
    var clip = M.clipMeta(state);
    if (!clip) {
      panel.innerHTML = emptyState("No source selected", "Choose a file in the queue, or add one. Sources are hashed on add and never modified.");
      return;
    }
    var fix = clip.fixity;
    var insp = clip.inspect;
    var html = "";

    html += '<div class="chain-head"><div class="chain-meta"><b>' + U.esc(clip.name) + "</b>" +
      '<span class="mono">' + U.esc(clip.file) + "</span></div>" +
      '<span class="spacer"></span>' + fixityPill(clip) + "</div>";

    html += '<div class="panel-section"><span class="eyebrow">Probe facts</span>' +
      '<dl class="proof-summary">' +
      row("Container", clip.codec) +
      row("Picture", clip.width + "×" + clip.height + " · " + f.rateText(clip.rate) + " fps · 4:3") +
      row("Field order flag", clip.fieldOrderFlag === "unknown" ? "not set" : clip.fieldOrderFlag) +
      row("Audio", clip.audio) +
      row("Length", f.timecodeAt(clip.duration, clip.rate) + " · " + f.duration(clip.duration)) +
      row("Size", f.bytes(clip.sizeBytes)) +
      "</dl></div>";

    html += '<div class="panel-section"><span class="eyebrow">Fixity</span>';
    if (fix.sha256) {
      html += '<div class="sidecar"><span data-icon="hash"></span><span class="sidecar-name" title="' + U.esc(fix.sha256) + '">' + U.esc(fix.sha256) + "</span>" +
        '<button class="btn btn-sm btn-ghost" data-action="copy-sha" data-sha="' + U.esc(fix.sha256) + '" title="Copy SHA-256">Copy</button></div>';
    } else {
      html += '<p class="field-help">Not computed yet. Fixity is measured on add and recorded in every export log.</p>';
    }
    if (fix.state === "mismatch") {
      html += callout("danger", "Hash mismatch", "The file changed after ingest. ReelDeck never writes to a source; restore the original before restoring from it. Exports from this file are blocked.");
    } else if (fix.state === "ok") {
      html += callout("info", "The source is untouched", "The SHA-256 measured now matches the value recorded at ingest. Every export log will record it again at run time.");
    }
    html += "</div>";

    if (insp.state !== "done") {
      html += '<div class="panel-section"><span class="eyebrow">Inspection</span>' +
        emptyState(
          insp.state === "failed" ? "Inspection did not finish" : "Not inspected yet",
          insp.state === "failed"
            ? "The tools reported an error. Nothing is guessed: fix the cause and run it again."
            : "Measure signal, anomalies and interlacing over a sampled range. Findings are suggestions."
        ) + actionBtn("rerunInspect", "target", "Inspect this source", "primary") + "</div>";
      panel.innerHTML = html;
      return;
    }

    // Interlace suggestion
    var idet = clip.idet;
    var tff = idet.singleTff + idet.multiTff;
    var bff = idet.singleBff + idet.multiBff;
    var prog = idet.singleProgressive + idet.multiProgressive;
    var total = tff + bff + prog;
    var pctTff = total ? Math.round((tff / total) * 100) : 0;
    var pctBff = total ? Math.round((bff / total) * 100) : 0;
    var pctProg = total ? 100 - pctTff - pctBff : 0;
    var confirmed = insp.confirmed;
    html += '<div class="panel-section"><span class="eyebrow">Interlace suggestion</span>' +
      '<div class="finding">' +
      '<div class="finding-main"><b>' + (confirmed ? confirmationText(confirmed) : "Undetermined") + "</b>" +
      '<span class="faint">' + (confirmed ? "Confirmed by you · operator decision" : "Suggestion only") + "</span></div>" +
      '<div class="bar-stack" role="img" aria-label="idet counts: top field first ' + pctTff + "%, bottom field first " + pctBff + "%, progressive " + pctProg + '%">' +
      bar(pctTff, "tff") + bar(pctBff, "bff") + bar(pctProg, "prog") + "</div>" +
      '<div class="basis">' +
      '<span><b>' + tff + '</b> TFF</span><span><b>' + bff + '</b> BFF</span><span><b>' + prog + '</b> Progressive</span>' +
      '<span class="faint">' + (idet.singleUndetermined + idet.multiUndetermined) + " undetermined</span></div>" +
      '<p class="field-help">Basis: idet single-frame and multi-frame counts. The leading category holds at least 60% before a suggestion is made.</p>' +
      "</div>" +
      '<div class="field"><span class="eyebrow" style="font-size:9px">Confirm the decision</span>' +
      '<div class="segmented" role="group" aria-label="Confirm field order">' +
      ["tff", "bff", "progressive"].map(function (v) {
        var label = v === "tff" ? "Top first" : v === "bff" ? "Bottom first" : "Progressive";
        return '<button data-action="confirm-order" data-value="' + v + '" aria-pressed="' + (confirmed === v) + '">' +
          (confirmed === v ? '<span class="tick">✓ </span>' : "") + label + "</button>";
      }).join("") + "</div>" +
      '<p class="field-help">The tool never acts on a detection silently. This decision is what the deinterlace step uses.</p></div>' +
      "</div>";

    // Signal summary
    html += '<div class="panel-section"><span class="eyebrow">Signal · measured</span><div class="stat-grid">' +
      stat("YAVG mean", clip.signal.yAvg.toFixed(1), " y, 16–235") +
      stat("SATAVG mean", clip.signal.satAvg.toFixed(1), "") +
      stat("TOUT peak", clip.signal.toutMax.toFixed(2), "ratio") +
      stat("Out of range", f.percent(clip.signal.brngMean, 2), "of pixels") +
      "</div>" +
      '<p class="field-help">' + clip.inspect.frames.toLocaleString("en-US") + " frames analyzed over the " + clip.inspect.sampled + ". Thresholds: temporal outlier > 0.05.</p></div>";

    // Findings table
    var marks = M.marks(clip);
    html += '<div class="panel-section"><span class="eyebrow">Findings · ' + marks.length + "</span>";
    if (!marks.length) {
      html += '<p class="field-help">No anomalies measured in this range.</p>';
    } else {
      html += '<div class="finding-list">' + marks.map(function (m, i) {
        return '<button class="finding-row" data-action="jump-mark" data-index="' + i + '" title="' + U.esc(m.detail) + '">' +
          '<span class="glyph" data-kind="' + m.kind + '">' + (m.kind === "outlier" ? "!" : m.kind === "freeze" ? "F" : "B") + "</span>" +
          "<span><b>" + (m.kind === "outlier" ? "Temporal outlier" : m.kind === "freeze" ? "Freeze span" : "Black span") + "</b>" +
          "<small>" + U.esc(m.detail) + "</small></span>" +
          '<span class="mono faint">' + f.durationShort(m.start) + "</span></button>";
      }).join("") + "</div>";
      html += '<p class="field-help">Findings are observations. Confirm them in the viewer before acting on them.</p>';
    }
    html += "</div>";

    // Audio
    html += '<div class="panel-section"><span class="eyebrow">Audio</span><dl class="proof-summary">' +
      row("Format", clip.audio.split("·").slice(0, 2).join(" · ")) +
      row("Integrated", (clip.audio.match(/−?[\d.]+ LUFS/) || ["not measured"])[0]) +
      row("True peak", (clip.audio.match(/−?[\d.]+ dBTP/) || ["not measured"])[0]) +
      row("Drift", "none measured — audio and video durations agree") +
      "</dl></div>";

    panel.innerHTML = html;
  }

  function confirmationText(v) {
    return v === "tff" ? "Top field first" : v === "bff" ? "Bottom field first" : "Progressive";
  }

  function bar(pct, kind) {
    return '<span class="bar" data-kind="' + kind + '" style="width:' + pct + '%" title="' + pct + '%"></span>';
  }

  function stat(label, value, unit) {
    return '<div class="stat"><span class="stat-label">' + label + '</span><b class="stat-value">' + value +
      (unit ? '<small>' + unit + "</small>" : "") + "</b></div>";
  }

  function row(label, value) {
    return "<dt>" + U.esc(label) + "</dt><dd" + (value && String(value).length > 34 ? ' title="' + U.esc(value) + '"' : "") + ">" + U.esc(value == null ? "—" : value) + "</dd>";
  }

  function emptyState(title, body, extra) {
    return '<div class="empty"><strong>' + U.esc(title) + "</strong>" + U.esc(body) + (extra || "") + "</div>";
  }

  function callout(kind, title, body) {
    var icon = kind === "danger" ? "warning" : kind === "info" ? "info" : "warning";
    return '<div class="callout ' + kind + '">' + U.icon(icon) + "<div><b>" + U.esc(title) + ".</b> " + U.esc(body) + "</div></div>";
  }

  function renderChainPanel() {
    var panel = $("panel-chain");
    var clip = M.clipMeta(state);
    if (!clip) {
      panel.innerHTML = emptyState("No chain yet", "Select a source to work on. A chain is the ordered set of operations a recipe records.");
      return;
    }
    if (clip.inspect.state !== "done") {
      panel.innerHTML = '<div class="chain-head"><div class="chain-meta"><b>Restoration chain</b><span>' + U.esc(clip.name) + "</span></div></div>" +
        emptyState("Inspect this source first", "The chain's defaults should follow the measurements, not guesses. A source that has not been inspected gets no default chain.",
          '<button class="btn btn-primary" data-action="rerunInspect">' + U.icon("target") + "<span>Inspect now</span></button>");
      return;
    }

    var plan = M.chainFilter(state);
    var gate = M.previewGate(state);
    var html = '<div class="chain-head">' +
      '<div class="chain-meta"><b>Restoration chain</b><span>' + U.esc(clip.name) + " · " + state.chain.length + " step" + (state.chain.length === 1 ? "" : "s") + "</span></div>" +
      '<span class="spacer"></span>' +
      '<button class="btn btn-sm" data-action="addModule" aria-haspopup="dialog" title="Add a module — A">' +
      U.icon("plus") + "<span>Module</span></button>" +
      "</div>";

    if (!state.chain.length) {
      html += emptyState("The chain is empty", "A restoration must do at least one thing, explicitly. Add a module to begin.",
        '<button class="btn btn-primary" data-action="addModule">' + U.icon("plus") + "<span>Add a module</span></button>");
      panel.innerHTML = html;
      return;
    }

    html += '<div class="chain-list">';
    state.chain.forEach(function (step, i) {
      html += renderStep(step, i);
    });
    html += "</div>";

    html += '<div class="chain-foot">' +
      '<div class="row tight"><span class="eyebrow" style="font-size:9px">Live filter chain</span>' +
      '<span class="spacer"></span>' +
      '<button class="btn btn-sm btn-ghost" data-action="copy-filter" title="Copy the exact filter string">' + U.icon("copy") + "<span>Copy</span></button></div>" +
      '<div class="filter-line" title="' + U.esc(plan.filter || plan.error) + '">' + U.esc(plan.ok ? plan.filter : "⚠ " + plan.error) + "</div>";

    if (gate) {
      html += '<div class="gate' + (state.previewed || state.override ? "" : " blocked") + '">' +
        U.icon(state.previewed ? "check" : "warning") + "<span>" +
        (state.previewed ? "Previewed. The gate is satisfied for this chain."
          : "Preview required: " + gate + (state.override ? " An expert override is recorded." : "")) +
        "</span></div>";
    } else {
      html += '<div class="gate" style="color:var(--text-3)">' + U.icon("info") +
        "<span>Conservative settings: this chain can render without a preview. A preview is still the fastest way to catch a wrong field order.</span></div>";
    }
    html += '<div class="row tight">' +
      '<button class="btn btn-sm" data-action="renderPreview">' + U.icon("film") + "<span>Preview segment</span></button>" +
      '<span class="spacer"></span>' +
      '<button class="btn btn-sm btn-ghost" data-action="tab" data-tab="export">Export panel ' + U.icon("chevron-right") + "</button>" +
      "</div>";
    if (gate) {
      html += '<label class="check"><input type="checkbox" data-control="override" ' + (state.override ? "checked" : "") + ">" +
        '<span class="box">' + U.icon("check") + "</span>" +
        '<span class="check-text"><b>Record an expert override</b><small>Written to the export log, so the missing preview is visible later.</small></span></label>';
    }
    html += "</div>";

    panel.innerHTML = html;

    // Sliders need their fill drawn from the current value.
    panel.querySelectorAll('input[type="range"]').forEach(function (r) {
      var min = Number(r.min), max = Number(r.max);
      var pct = ((Number(r.value) - min) / (max - min)) * 100;
      r.style.setProperty("--fill", pct + "%");
    });
  }

  function renderStep(step, i) {
    var mod = D.modules[step.module];
    var warns = M.watchPoints(step);
    var gate = step.params.engine === "qtgmc" ||
      (step.module === "denoise" && (step.params.engine === "nlmeans" || step.params.temporal > 8)) ||
      (step.module === "stabilize" && step.params.zoom > 10);
    var html = '<div class="step" data-step="' + i + '" data-open="' + step.open + '" data-selected="' + (selectedStep === i) + '">' +
      '<div class="step-head">' +
      '<span class="step-order">' +
      '<button data-action="move-step" data-index="' + i + '" data-dir="-1" ' + (i === 0 ? "disabled" : "") + ' aria-label="Move ' + mod.label + ' earlier" title="Move earlier — Alt+↑">' + U.icon("chevron-up") + "</button>" +
      '<button data-action="move-step" data-index="' + i + '" data-dir="1" ' + (i === state.chain.length - 1 ? "disabled" : "") + ' aria-label="Move ' + mod.label + ' later" title="Move later — Alt+↓">' + U.icon("chevron-down") + "</button>" +
      "</span>" +
      '<span class="step-index">' + (i + 1) + "</span>" +
      '<span class="step-title"><b>' + mod.label + '</b><span class="step-summary" title="' + U.esc(M.summaryFor(step)) + '">' + U.esc(M.summaryFor(step)) + "</span></span>" +
      '<span class="step-flags">' + (warns.length ? '<span class="flag" title="' + warns.length + ' note' + (warns.length === 1 ? "" : "s") + '">' + U.icon("warning") + warns.length + "</span>" : "") +
      (gate ? '<span class="flag" title="Aggressive settings: preview required" style="color:var(--err);background:var(--err-wash)">Gate</span>' : "") +
      "</span>" +
      '<button class="step-caret" data-action="toggle-step" data-index="' + i + '" aria-expanded="' + step.open + '" aria-label="' + (step.open ? "Collapse" : "Expand") + " " + mod.label + '">' + U.icon("chevron-right") + "</button>" +
      "</div>";

    html += '<div class="step-body">';
    html += '<p class="field-help" style="margin-top:6px">' + U.esc(mod.blurb) + "</p>";
    html += controlsFor(step, i);

    warns.forEach(function (w) {
      html += '<div class="callout">' + U.icon("warning") + "<div>" + U.esc(w) + "</div></div>";
    });

    html += '<div class="filter-block"><div class="filter-head">' + U.icon("hash") +
      "<span>Filter fragment · exact</span></div><code>" + U.esc(M.filterFor(step)) + "</code></div>";

    html += '<div class="step-actions">' +
      '<button class="btn btn-sm btn-ghost" data-action="reset-step" data-index="' + i + '" title="Reset to module defaults — Alt+Backspace">' + U.icon("reset") + "<span>Reset</span></button>" +
      '<span class="spacer"></span>' +
      '<button class="btn btn-sm btn-ghost btn-danger" data-action="remove-step" data-index="' + i + '" title="Remove this step — Delete">' + U.icon("trash") + "<span>Remove</span></button>" +
      "</div></div></div>";
    return html;
  }

  function controlsFor(step, i) {
    var p = step.params;
    var html = "";
    function segment(label, key, options, note) {
      return '<div class="field"><div class="field-head"><label>' + label + "</label>" +
        (note ? '<span class="field-note">' + note + "</span>" : "") + "</div>" +
        '<div class="segmented" role="group" aria-label="' + label + '">' +
        options.map(function (o) {
          var disabled = o.disabled ? " disabled" : "";
          return '<button data-action="set-param" data-index="' + i + '" data-key="' + key + '" data-value="' + o.value + '"' +
            ' aria-pressed="' + (String(p[key]) === String(o.value)) + '"' + disabled + ' title="' + U.esc(o.title || "") + '">' +
            (String(p[key]) === String(o.value) ? '<span class="tick">✓ </span>' : "") + o.label + "</button>";
        }).join("") + "</div>" + (options.some(function (o) { return o.help; }) ?
          '<p class="field-help">' + options.filter(function (o) { return o.help; }).map(function (o) { return U.esc(o.help); }).join(" ") + "</p>" : "") + "</div>";
    }
    function slider(label, key, min, max, stepSize, unit, help) {
      return '<div class="field"><div class="field-head"><label for="p-' + i + "-" + key + '">' + label + "</label>" +
        '<span class="field-note">' + (p[key]) + (unit || "") + "</span></div>" +
        '<div class="slider-row"><input id="p-' + i + "-" + key + '" type="range" min="' + min + '" max="' + max + '" step="' + stepSize +
        '" value="' + p[key] + '" data-control="param" data-index="' + i + '" data-key="' + key + '" data-unit="' + (unit || "") + '"' +
        ' aria-label="' + label + '">' +
        '<input class="num-input" type="number" min="' + min + '" max="' + max + '" step="' + stepSize + '" value="' + p[key] +
        '" data-control="param-number" data-index="' + i + '" data-key="' + key + '" aria-label="' + label + ' numeric value"></div>' +
        (help ? '<p class="field-help">' + U.esc(help) + "</p>" : "") + "</div>";
    }

    if (step.module === "deinterlace") {
      html += segment("Engine", "engine", [
        { value: "bwdif", label: "bwdif", title: "Motion-adaptive; the default", help: "bwdif is motion-adaptive and keeps more vertical detail in motion than yadif." },
        { value: "yadif", label: "yadif", title: "Faster, simpler" },
        { value: "qtgmc", label: "QTGMC", title: "Needs VapourSynth, which is not installed", disabled: true }
      ]);
      html += segment("Field order", "fieldOrder", [
        { value: "auto", label: "Auto", title: "Follow the stream flag" },
        { value: "tff", label: "Top", title: "Top field first" },
        { value: "bff", label: "Bottom", title: "Bottom field first" }
      ]);
      html += segment("Output rate", "outputRate", [
        { value: "send_frame", label: "1 per frame", title: "25i → 25p" },
        { value: "send_field", label: "1 per field", title: "25i → 50p, preserves motion" }
      ], "One per frame keeps the runtime; one per field keeps every motion sample.");
      html += checkbox(i, "onlyFlagged", "Only frames flagged interlaced", "Leaves progressive frames untouched — useful for mixed telecine.", p.onlyFlagged);
    }

    if (step.module === "stabilize") {
      html += '<div class="callout info" style="margin-top:6px">' + U.icon("info") +
        "<div>Two passes: analysis writes <b>transforms.trf</b> into the project, and re-runs reuse it. The analysis pass must see the whole file, so this chain cannot be segment-previewed.</div></div>";
      html += slider("Smoothing", "smoothing", 0, 100, 1, "", "Higher values hold the frame steadier and make deliberate camera moves feel mechanical.");
      html += slider("Zoom", "zoom", 0, 50, 0.5, "%", "Zoom hides the moving border by cropping — the picture area is lost, not invented.");
      html += checkbox(i, "tripod", "Tripod mode", "Locks every shot still; wrong for handheld material.", p.tripod);
    }

    if (step.module === "denoise") {
      html += segment("Engine", "engine", [
        { value: "hqdn3d", label: "hqdn3d", title: "Fast, spatial + temporal" },
        { value: "nlmeans", label: "nlmeans", title: "Slower, higher quality" }
      ]);
      html += slider("Luma", "luma", 0, 20, 0.5, "", "Spatial strength on luma. Start low; grain is signal until you decide otherwise.");
      html += slider("Chroma", "chroma", 0, 20, 0.5, "", "Chroma noise is usually worse on tape than luma noise, and cheaper to remove.");
      html += slider("Temporal", "temporal", 0, 20, 0.5, "", "Compares neighbouring frames. Above 8, motion smears and film grain disappears.");
    }

    if (step.module === "finish") {
      html += segment("Crop", "crop", [
        { value: "none", label: "None" },
        { value: "blanking", label: "Tape blanking", title: "Removes 8 px of head-switching and edge blanking" },
        { value: "custom", label: "Custom" }
      ]);
      html += '<div class="field"><div class="field-head"><label>Scale</label><span class="field-note">' + p.scaleWidth + "×" + p.scaleHeight + "</span></div>" +
        '<div class="row tight"><input class="num-input" type="number" min="16" max="7680" step="2" value="' + p.scaleWidth + '" data-control="param-number" data-index="' + i + '" data-key="scaleWidth" aria-label="Scale width">' +
        '<span class="faint">×</span>' +
        '<input class="num-input" type="number" min="16" max="7680" step="2" value="' + p.scaleHeight + '" data-control="param-number" data-index="' + i + '" data-key="scaleHeight" aria-label="Scale height">' +
        '<span class="faint" style="font-size:9.5px">lanczos · recorded</span></div>' +
        '<p class="field-help">Scaling resamples. The scaler is recorded in the export log; upscaling invents nothing.</p></div>';
      html += checkbox(i, "deflicker", "Deflicker", "Reduces frame-to-frame exposure flicker on film transfers.", p.deflicker);
      html += checkbox(i, "normalizeLevels", "Pull levels into broadcast range", "Clips out-of-range values by definition. Check the out-of-range ratio in Source first.", p.normalizeLevels);
    }

    return html;
  }

  function checkbox(i, key, label, help, checked) {
    return '<label class="check"><input type="checkbox" data-control="param" data-index="' + i + '" data-key="' + key + '" ' + (checked ? "checked" : "") + ">" +
      '<span class="box">' + U.icon("check") + "</span>" +
      '<span class="check-text"><b>' + label + "</b>" + (help ? "<small>" + U.esc(help) + "</small>" : "") + "</span></label>";
  }

  // ------------------------------------------------------------------ export
  function renderExportPanel() {
    var panel = $("panel-export");
    var clip = M.clipMeta(state);
    var checks = M.preflight(state);
    var ready = M.ready(checks);
    var preset = D.presets.find(function (p) { return p.id === state.presetId; });
    var estimate = M.estimateBytes(state);
    var gate = M.previewGate(state);
    var plan = M.chainFilter(state);
    var command = M.exportCommand(state);

    var html = '<div class="panel-section"><span class="eyebrow">Intent · choose one</span><div class="intent-list" role="radiogroup" aria-label="Export intent">';
    html += D.presets.map(function (p) {
      var est = estimate ? Math.round(estimate * (p.facts.sizeFactor / (preset ? preset.facts.sizeFactor : 1))) : null;
      return '<button class="intent" role="radio" aria-checked="' + (state.presetId === p.id) + '" data-action="set-preset" data-preset="' + p.id + '">' +
        '<span class="radio-dot" aria-hidden="true"></span>' +
        "<span>" + '<span class="intent-title"><b>' + p.title + '</b><span class="format">' + p.format.split(" · ")[0] + "</span></span>" +
        "<p>" + U.esc(p.why) + "</p>" +
        '<span class="intent-facts"><span>Codec <b>' + p.format.split(" · ")[0] + "</b></span>" +
        "<span>Est. size <b>" + (est ? f.bytes(est) : "—") + "</b></span>" +
        "<span>Extension <b>." + p.extension + "</b></span></span>" +
        "</span></button>";
    }).join("");
    html += "</div>";
    html += '<p class="field-help" style="margin-top:6px">A preservation master is not a delivery file. The preset is recorded in the recipe, and the proof names it.</p></div>';

    html += '<div class="panel-section"><span class="eyebrow">Destination</span>' +
      '<div class="dest-row"><input class="text-input" id="dest-input" value="' + U.esc(state.destination) + '" aria-label="Output file path" spellcheck="false">' +
      '<button class="btn btn-sm" data-action="dest-check" title="Check the destination">Check</button></div>' +
      '<p class="field-help">Outputs are always new files. The recipe and export log are written beside the output as <span class="mono">' +
      U.esc(basename(state.destination)) + ".recipe.json</span> and <span class=\"mono\">" + U.esc(basename(state.destination)) + ".export-log.json</span>.</p></div>";

    html += '<div class="panel-section"><span class="eyebrow">Before the render · ' + checks.filter(function (c) { return c.level === "pass"; }).length + " of " + checks.length + " clear</span>" +
      '<div class="preflight">' + checks.map(function (c) {
        return '<div class="check-row">' + U.checkChip(c.level) +
          '<div class="check-text"><b>' + U.esc(c.label) + "</b><p>" + U.esc(c.detail) + "</p></div></div>";
      }).join("") + "</div>";

    if (gate) {
      html += '<label class="check" style="margin-top:8px"><input type="checkbox" data-control="override" ' + (state.override ? "checked" : "") + ">" +
        '<span class="box">' + U.icon("check") + "</span>" +
        '<span class="check-text"><b>Record an expert override</b><small>Allows the export without a preview. The export log records it, so the decision is auditable.</small></span></label>';
    }
    html += "</div>";

    html += '<div class="panel-section"><span class="eyebrow">Exact command</span>';
    if (command) {
      html += '<div class="plan-preview"><code>' + U.esc(command.join(" ")) + "</code></div>" +
        '<p class="field-help">This is the argument vector the executor would run — the same planner that draws the filter string above. Planning only in this prototype: nothing runs.</p>';
    } else {
      html += '<div class="callout danger">' + U.icon("warning") + "<div><b>The planner refuses this chain.</b> " +
        U.esc(plan.error || "Add at least one operation.") + "</div></div>";
    }
    html += "</div>";

    html += '<div class="export-actions">' +
      '<button class="btn btn-primary" data-action="renderExport" ' + (ready && command ? "" : "disabled") + ">" +
      "<span>Render " + (preset ? preset.title.toLowerCase() : "export") + "</span><span class=\"mono\" style=\"font-size:9.5px\">" + (estimate ? f.bytes(estimate) : "") + "</span></button>" +
      '<div class="hint">' + (ready
        ? "New output · recipe and export log written beside it · the source is never written to."
        : blockReason(checks)) + "</div>" +
      '<div class="row tight"><button class="btn btn-sm" data-action="renderPreview">' + U.icon("film") + "<span>Preview a segment first</span></button>" +
      '<span class="spacer"></span><button class="btn btn-sm btn-ghost" data-action="copy-command" title="Copy the command">' + U.icon("copy") + "<span>Copy</span></button></div>" +
      "</div>";

    panel.innerHTML = html;
  }

  function blockReason(checks) {
    var blocked = checks.filter(function (c) { return c.level === "blocked"; })[0];
    return blocked ? blocked.label + ": " + blocked.detail : "The render is not ready yet.";
  }

  function basename(path) {
    return String(path).split(/[\\/]/).pop();
  }

  // ------------------------------------------------------------------ prove
  function renderProvePanel() {
    var panel = $("panel-prove");
    if (!state.exported) {
      var clip = M.clipMeta(state);
      panel.innerHTML =
        '<div class="panel-section"><span class="eyebrow">Prove</span>' +
        emptyState("No output yet", "When an export finishes, this panel checks the file against the proof it shipped with: the recorded run, the output hash, the source hash, the preset and the schema.", "") +
        '<div class="preflight">' +
        [["Run succeeded", "The recorded run reports success and carries an output hash."],
         ["Output hash", "The file on disk still hashes to the recorded value."],
         ["Source hash", "The source matches the hash pinned in the recipe."],
         ["Preset match", "Recipe and export log name the same intent."],
         ["Schema match", "Recipe and export log name the same schema version."]].map(function (c) {
          return '<div class="check-row"><span class="check-chip" data-level="unknown">SKIP</span><div class="check-text"><b>' + c[0] + "</b><p>" + c[1] + "</p></div></div>";
        }).join("") + "</div>" +
        '<p class="field-help" style="margin-top:8px">Nothing is verified yet, so nothing claims to pass. ' +
        (clip ? "Export " + U.esc(clip.name) + " to create its proof." : "") + "</p></div>";
      return;
    }

    var out = state.exported;
    var verification = M.verification(state);
    var html = '<div class="panel-section">' +
      '<div class="verify-verdict" data-state="ok">' + U.icon("shield") +
      "<div><b>Proof complete</b><span>The output, recipe and export log agree.</span></div></div>" +
      '<dl class="proof-summary">' +
      row("Output", basename(out.name)) +
      row("Size", f.bytes(out.size)) +
      row("Preset", out.presetLabel) +
      row("Frames", out.frames.toLocaleString("en-US")) +
      row("SHA-256", out.sha256) +
      "</dl></div>";

    html += '<div class="panel-section"><div class="row tight"><span class="eyebrow">Verification</span><span class="spacer"></span>' +
      '<button class="btn btn-sm btn-ghost" data-action="verify">' + U.icon("check") + "<span>Verify again</span></button></div>" +
      '<div class="preflight">' + verification.checks.map(function (c) {
        return '<div class="check-row">' + U.checkChip(c.status === "pass" ? "pass" : "unknown") +
          '<div class="check-text"><b>' + U.esc(c.name) + "</b><p>" + U.esc(c.detail) + "</p></div></div>";
      }).join("") +
      '<div class="check-row"><span class="check-chip" data-level="unknown">SKIP</span><div class="check-text"><b>Example — evidence missing</b>' +
      "<p>A check whose evidence is absent is reported SKIP, never PASS. Verification that cannot see a file says so.</p></div></div>" +
      "</div>" +
      '<p class="field-help">Verified ' + U.esc(out.verifiedAt || "just now") + " · the report is a fact table, not a score.</p></div>";

    html += '<div class="panel-section"><span class="eyebrow">Sidecars written beside the output</span>' +
      '<div class="sidecar-list">' +
      sidecar("file", basename(out.name) + ".recipe.json", "recipe as executed") +
      sidecar("file", basename(out.name) + ".export-log.json", "command, versions, hashes") +
      sidecar("hash", "stream sha256", f.shortHash(out.streamSha)) +
      "</div></div>";

    html += '<div class="panel-section"><span class="eyebrow">Determinism</span>' +
      (out.rerun
        ? '<div class="determinism"><b>Streams bit-identical · container differs</b>' +
        "<p>The re-run's elementary streams hash the same as the recorded run. The file bytes differ only because Matroska writes a per-run Segment UUID and DateUTC.</p>" +
        '<div class="hashes"><span>recorded</span><span>' + U.esc(f.shortHash(out.sha256)) + "</span>" +
        "<span>re-run</span><span>" + U.esc(f.shortHash(out.rerunSha)) + "</span>" +
        "<span>streams</span><span>" + U.esc(f.shortHash(out.streamSha)) + " (both runs)</span></div></div>"
        : '<div class="determinism"><b>Not yet re-run</b><p>Re-running this recipe beside a new output proves the job reproduces. The verdict compares files first, then the elementary streams, so container metadata is not mistaken for a changed picture.</p>' +
        '<button class="btn btn-sm" data-action="rerun" style="margin-top:6px">' + U.icon("reset") + "<span>Re-run and compare</span></button></div>") +
      "</div>";

    panel.innerHTML = html;
  }

  function sidecar(iconName, name, meta) {
    return '<div class="sidecar">' + U.icon(iconName) + '<span class="sidecar-name" title="' + U.esc(name) + '">' +
      U.esc(name) + '</span><span class="sidecar-meta">' + U.esc(meta) + "</span></div>";
  }

  // ------------------------------------------------------------------ status
  function renderStatus() {
    var clip = M.clipMeta(state);
    $("status-save").textContent = "Project saved";
    if (clip) {
      var fix = clip.fixity.state === "ok" ? "Fixity match" : clip.fixity.state === "mismatch" ? "Fixity mismatch" : "Hashing";
      $("status-fixity").innerHTML = '<span class="dot ' + (clip.fixity.state === "ok" ? "" : "warn") + '"></span> ' + fix;
    } else {
      $("status-fixity").textContent = "";
    }
    $("status-engine").textContent = "Engine: FFmpeg " + D.engine.ffmpeg + " · VapourSynth not installed · offline";
  }

  // ------------------------------------------------------------------ actions
  function setStage(id, silent) {
    var a = M.access(state, id);
    if (!a.ok) {
      toast("warn", M.stageById(id).label + " is not available yet", a.reason);
      announce(M.stageById(id).label + " unavailable: " + a.reason);
      return;
    }
    var previous = state.stage;
    state.stage = id;
    var tabFor = { ingest: "source", inspect: "source", decide: "chain", preview: "chain", export: "export", prove: "prove" };
    if (previous !== id) setTab(tabFor[id]);
    if (id === "preview" && !state.range) {
      var suggestion = M.suggestedRange(state, 20);
      if (suggestion) state.range = { in: suggestion.in, out: suggestion.out };
    }
    renderStages();
    renderContext();
    renderTimeline();
    renderInspector();
    if (!silent) announce(M.stageById(id).label + " stage.");
  }

  function setViewMode(mode) {
    state.viewer.mode = mode;
    state.viewer.compare = mode === "program" ? state.viewer.compare : false;
    renderViewer();
    var m = MODES.find(function (x) { return x.id === mode; });
    if (m) announce(m.label + " view. " + modePurpose(mode));
  }

  function modePurpose(mode) {
    return {
      program: "The picture, for judging the image itself.",
      waveform: "Luma levels across the frame.",
      histogram: "Distribution of levels.",
      vectorscope: "Chroma hue and saturation.",
      field: "The two fields apart — combing is obvious here.",
      temporal: "Change from the previous frame."
    }[mode] || "";
  }

  function toggleCompare() {
    if (state.viewer.mode !== "program") setViewMode("program");
    state.viewer.compare = !state.viewer.compare;
    renderViewer();
    announce(state.viewer.compare ? "Comparing source and chain." : "Comparison off.");
  }

  function jumpToMark(index) {
    var clip = M.clipMeta(state);
    if (!clip) return;
    var m = M.marks(clip)[index];
    if (!m) return;
    seekTo(m.start);
    var graph = $("timeline-inner");
    if (graph) {
      var pct = m.start / clip.duration;
      var width = graph.clientWidth - gutter();
      graph.parentElement.scrollLeft = Math.max(0, gutter() + pct * width - graph.parentElement.clientWidth / 2);
    }
    announce(m.detail);
  }

  function suggestRange() {
    var s = M.suggestedRange(state, 20);
    if (!s) return;
    state.range = { in: s.in, out: s.out };
    seekTo(s.in);
    updateSelectionBand();
    toast("ok", "Preview range suggested", "Centred on the worst measured moment (" + f.durationShort(s.anchor) + "), not the first seconds. Drag or set in/out to change it.");
  }

  function renderPreview() {
    if (!state.chain.length) {
      toast("warn", "Nothing to preview", "Add at least one module to the chain first.");
      return;
    }
    if (!state.range) {
      var s = M.suggestedRange(state, 20);
      if (s) state.range = { in: s.in, out: s.out };
    }
    var clip = M.clipMeta(state);
    if (!clip) return;
    var plan = M.chainFilter(state);
    if (!plan.ok) {
      toast("err", "The planner refuses this chain", plan.error);
      return;
    }
    var label = "Preview · " + clip.name + " · " + f.durationShort(state.range.in) + "–" + f.durationShort(state.range.out);
    startJob({
      label: label,
      sub: "chain as planned · " + state.chain.length + " steps · proof written beside the preview",
      doneNote: "proof written",
      onDone: function () {
        state.previewed = true;
        renderInspector();
        renderContext();
        renderStages();
        toast("ok", "Preview finished", "A 20-second segment rendered with its recipe and export log. Judge it before exporting the whole file.");
        if (state.stage === "preview") setStage("export", true);
      }
    });
    announce("Preview started. Progress is reported from the render.");
  }

  function openExport() {
    var checks = M.preflight(state);
    var clip = M.clipMeta(state);
    if (!clip) { toast("warn", "No source", "Select a source first."); return; }
    if (!M.ready(checks)) {
      setTab("export");
      if (state.stage !== "export") setStage("export", true);
      toast("warn", "Export is blocked", blockReason(checks));
      return;
    }
    var preset = D.presets.find(function (p) { return p.id === state.presetId; });
    var command = M.exportCommand(state);
    $("confirm-copy").innerHTML = "A new file will be written. <b>The source is never modified.</b> " +
      "The recipe and export log ship beside the output, so this render can be proved and re-run later.";
    $("confirm-facts").innerHTML = '<dl class="proof-summary">' +
      row("Output", state.destination) +
      row("Preset", preset.title + " · " + preset.format) +
      row("Chain", state.chain.length + " steps · " + M.chainFilter(state).filter) +
      row("Estimated size", f.bytes(M.estimateBytes(state))) +
      row("Proof", basename(state.destination) + ".recipe.json · .export-log.json") +
      "</dl>" +
      (state.override ? callout("danger", "Expert override recorded", "No preview of this chain exists. The export log will carry the override, so the decision is visible later.") : "") +
      (command ? '<div class="plan-preview"><code>' + U.esc(command.join(" ")) + "</code></div>" : "");
    $("confirm-hint").textContent = "Simulated in this prototype — no file is written.";
    openDialog("scrim-confirm", $("confirm-run"));
  }

  function runExport() {
    closeDialog("scrim-confirm");
    var clip = M.clipMeta(state);
    var preset = D.presets.find(function (p) { return p.id === state.presetId; });
    var estimate = M.estimateBytes(state);
    startJob({
      label: "Export · " + basename(state.destination),
      sub: preset.title + " · " + state.chain.length + " steps · simulated",
      duration: 4600,
      doneNote: "proof written",
      onDone: function () {
        var sha = fakeHash("out" + Date.now());
        state.exported = {
          name: state.destination,
          size: estimate,
          sha256: sha,
          streamSha: fakeHash("streams" + sha),
          sourceSha256: clip.fixity.sha256,
          presetLabel: preset.title,
          frames: Math.round(clip.duration * f.fps(clip.rate) * (state.chain.some(function (s) {
            return s.module === "deinterlace" && s.params.outputRate === "send_field";
          }) ? 2 : 1)),
          verifiedAt: f.clockTime(nowSeconds()).slice(0, 8),
          rerun: null
        };
        state.previewed = true;
        D.project.outputCount += 1;
        renderAll();
        setStage("prove", true);
        toast("ok", "Export finished", "The recipe and export log were written beside the output. Prove checks the file against them.");
      }
    });
    announce("Export started.");
  }

  function verifyExport() {
    if (!state.exported) {
      toast("warn", "Nothing to verify", "Prove opens once an export has finished.");
      return;
    }
    startJob({
      label: "Verify · " + basename(state.exported.name),
      sub: "hashing output and source · simulated",
      duration: 1500,
      doneNote: "all checks pass",
      onDone: function () {
        state.exported.verifiedAt = f.clockTime(nowSeconds()).slice(0, 8);
        renderInspector();
        toast("ok", "Verification passed", "Five checks: recorded run, output hash, source hash, preset and schema. Skips would be reported as SKIP.");
      }
    });
  }

  function rerunExport() {
    if (!state.exported) return;
    startJob({
      label: "Re-run · " + basename(state.exported.name),
      sub: "same recipe beside a new output · simulated",
      duration: 2600,
      doneNote: "streams identical",
      onDone: function () {
        state.exported.rerun = true;
        state.exported.rerunSha = fakeHash("rerun" + state.exported.sha256);
        renderInspector();
        toast("ok", "Re-run compared", "Streams are bit-identical. The file bytes differ only in container metadata — Matroska writes a Segment UUID and DateUTC per run.");
      }
    });
  }

  function fakeHash(seed) {
    var hex = "0123456789abcdef";
    var s = 2166136261;
    for (var i = 0; i < seed.length; i++) {
      s ^= seed.charCodeAt(i);
      s = Math.imul(s, 16777619) >>> 0;
    }
    var out = "";
    for (var j = 0; j < 64; j++) {
      s = (Math.imul(s, 1103515245) + 12345) >>> 0;
      out += hex[(s >>> 16) & 15];
    }
    return out;
  }

  function nowSeconds() {
    var d = new Date();
    return d.getHours() * 3600 + d.getMinutes() * 60 + d.getSeconds();
  }

  function selectClip(id) {
    if (id === state.selectedId) return;
    // Chains belong to their source: swap the working chain out and the
    // selected clip's chain in, so one file's settings never describe
    // another file's picture.
    state.chains[state.selectedId] = state.chain;
    state.selectedId = id;
    if (!state.chains[id]) {
      var target = M.clipMeta(state);
      state.chains[id] = target && target.inspect.state === "done"
        ? D.initialChain.map(function (s) { return { module: s.module, params: Object.assign({}, s.params), open: s.module === "deinterlace" }; })
        : [];
    }
    state.chain = state.chains[id];
    state.range = null;
    state.viewer.playhead = 0;
    state.previewed = false;
    state.override = false;
    state.exported = null;
    state.verified = null;
    state.viewer.compare = false;
    selectedStep = 0;
    renderQueue();
    renderViewer();
    renderTimeline();
    renderInspector();
    renderStages();
    renderStatus();
    var clip = M.clipMeta(state);
    if (!M.access(state, state.stage).ok) fallbackStage();
    announce("Selected " + clip.name + ". " + (clip.inspect.state === "done" ? "Inspection available." : "Not inspected."));
    if (clip.inspect.state !== "done") {
      toast("warn", "No inspection for this source", "Inspect it before building a chain — defaults should follow measurements, not guesses.");
    }
  }

  /** Move to the furthest stage the evidence still supports. */
  function fallbackStage() {
    var best = "ingest";
    D.stages.forEach(function (s) {
      if (M.access(state, s.id).ok) best = s.id;
    });
    var tabFor = { ingest: "source", inspect: "source", decide: "chain", preview: "chain", export: "export", prove: "prove" };
    state.stage = best;
    state.tab = tabFor[best];
    selectedStep = 0;
    renderStages();
    renderContext();
    renderInspector();
    setTab(state.tab);
  }

  function addSource() {
    var names = ["", ""];
    toast("warn", "Adding files happens in the desktop app", "A browser prototype cannot read local files. In the app, this opens a file picker, hashes each source, and records the probe facts.");
    announce("Add source is not simulated: file access belongs to the desktop app.");
  }

  function rehash() {
    var clip = M.clipMeta(state);
    if (!clip) return;
    startJob({
      label: "Fixity · " + clip.name,
      sub: "SHA-256 of the source · simulated",
      duration: 1400,
      doneNote: "hash matches",
      onDone: function () {
        toast("ok", "Fixity re-checked", "The source still hashes to the value recorded at ingest.");
      }
    });
  }

  function rerunInspect() {
    var clip = M.clipMeta(state);
    if (!clip) return;
    if (clip.fixity.state === "mismatch") {
      toast("err", "Inspection refused", "The source changed since ingest. Restore the original file first; inspecting a different file would describe the wrong material.");
      return;
    }
    startJob({
      label: "Inspect · " + clip.name,
      sub: "signal, anomalies, idet · simulated",
      duration: 2200,
      doneNote: "report written",
      onDone: function () {
        if (clip.inspect.state !== "done") {
          clip.inspect = { state: "done", suggestion: "tff", confirmed: null, sampled: "full file", frames: Math.round(clip.duration * f.fps(clip.rate)) };
        }
        renderAll();
        setStage("inspect", true);
        toast("ok", "Inspection finished", "Findings are suggestions. Confirm the field order in the Source panel before relying on it.");
      }
    });
  }

  function confirmOrder(value) {
    var clip = M.clipMeta(state);
    if (!clip) return;
    clip.inspect.confirmed = value;
    var stepIndex = M.findStep(state, "deinterlace");
    if (stepIndex >= 0) {
      M.updateParams(state, stepIndex, { fieldOrder: value === "progressive" ? "auto" : value });
    }
    renderInspector();
    renderContext();
    toast("ok", "Field order confirmed · " + confirmationText(value),
      value === "progressive" ? "The deinterlace step is left on auto; no deinterlace is forced." : "The deinterlace step now forces " + value.toUpperCase() + ". Check a pan in the preview.");
  }

  // ------------------------------------------------------------------ dialogs
  function openDialog(id, focusEl) {
    lastFocus = document.activeElement;
    var scrim = $(id);
    if (!scrim) return;
    scrim.hidden = false;
    // The rest of the shell is inert while a modal is open: focus cannot
    // wander behind the dialog, and screen readers stay in the dialog.
    var app = document.querySelector(".app");
    if (app) app.setAttribute("inert", "");
    document.querySelectorAll(".scrim:not([hidden])").forEach(function (s) {
      if (s !== scrim) s.hidden = true;
    });
    if (focusEl) requestAnimationFrame(function () { focusEl.focus(); });
    else {
      var first = scrim.querySelector("button, input, [tabindex]");
      if (first) requestAnimationFrame(function () { first.focus(); });
    }
  }

  function closeDialog(id) {
    var scrim = $(id);
    if (!scrim) return;
    scrim.hidden = true;
    var app = document.querySelector(".app");
    if (app) app.removeAttribute("inert");
    if (lastFocus && lastFocus.focus) lastFocus.focus();
  }

  function closeAllDialogs() {
    var open = document.querySelector(".scrim:not([hidden])");
    if (open) closeDialog(open.id);
  }

  function openPalette() {
    palette.query = "";
    $("palette-input").value = "";
    renderPalette("");
    openDialog("scrim-palette", $("palette-input"));
  }

  function renderPalette(query) {
    palette.query = query;
    var items = D.commands.filter(function (c) {
      if (!query.trim()) return true;
      var hay = (c.title + " " + c.group + " " + c.aliases + " " + c.keys + " " + c.id).toLowerCase();
      return query.toLowerCase().split(/\s+/).every(function (w) { return hay.indexOf(w) >= 0; });
    });
    palette.items = items;
    palette.index = Math.min(palette.index, Math.max(0, items.length - 1));
    var results = $("palette-results");
    if (!items.length) {
      results.innerHTML = '<div class="palette-empty">No command matches “' + U.esc(query) + '”. Try a stage name, a tool, or a description.</div>';
      $("palette-count").textContent = "0 commands";
      return;
    }
    // Group in the order the table declares them.
    var groups = [];
    items.forEach(function (c) { if (groups.indexOf(c.group) < 0) groups.push(c.group); });
    var html = "";
    groups.forEach(function (g) {
      html += '<div class="palette-group-label">' + g + "</div>";
      items.filter(function (c) { return c.group === g; }).forEach(function (c) {
        var idx = items.indexOf(c);
        html += '<button class="palette-item" role="option" id="pal-' + idx + '" data-action="run-command" data-command="' + c.id + '"' +
          ' aria-selected="' + (idx === palette.index) + '">' +
          '<span><span class="pi-title">' + U.esc(c.title) + "</span>" +
          (c.hint ? '<span class="pi-hint"> — ' + U.esc(c.hint) + "</span>" : "") + "</span>" +
          (c.keys ? '<kbd class="kbd">' + U.esc(c.keys) + "</kbd>" : "") +
          "</button>";
      });
    });
    results.innerHTML = html;
    $("palette-count").textContent = items.length + " command" + (items.length === 1 ? "" : "s");
    $("palette-input").setAttribute("aria-activedescendant", "pal-" + palette.index);
    var active = $("pal-" + palette.index);
    if (active && active.scrollIntoView) active.scrollIntoView({ block: "nearest" });
  }

  function runCommand(id) {
    var cmd = D.commands.find(function (c) { return c.id === id; });
    if (!cmd) return;
    closeAllDialogs();
    switch (cmd.run) {
      case "stage": setStage(cmd.arg); break;
      case "viewMode": setViewMode(cmd.arg); break;
      case "step": stepFrames(cmd.arg); break;
      case "jump": seekTo(state.viewer.playhead + cmd.arg); break;
      case "markNext": nextMark(1); break;
      case "markPrev": nextMark(-1); break;
      case "setIn": setIn(); break;
      case "setOut": setOut(); break;
      case "clearRange": state.range = null; updateSelectionBand(); announce("Preview range cleared."); break;
      case "lanes": toggleLanes(); break;
      case "addModule": openModules(); break;
      case "moveStep": moveSelectedStep(cmd.arg); break;
      case "removeStep": removeSelectedStep(); break;
      case "resetStep": resetSelectedStep(); break;
      case "renderPreview": renderPreview(); break;
      case "renderExport": openExport(); break;
      case "verify": verifyExport(); break;
      case "suggestRange": suggestRange(); break;
      case "compare": toggleCompare(); break;
      case "zoom": toggleZoom(); break;
      case "safe": toggleSafe(); break;
      case "tab": setTab(cmd.arg); break;
      case "palette": openPalette(); break;
      case "shortcuts": openShortcuts(); break;
      case "prefs": openPrefs(); break;
      case "addSource": addSource(); break;
      case "rehash": rehash(); break;
      case "about": toast("ok", "ReelDeck prototype", "Shell prototype for the restoration workspace. Engine: FFmpeg. Local only: no accounts, no telemetry, no network."); break;
      default: break;
    }
  }

  function openModules() {
    var list = $("module-list");
    list.innerHTML = D.moduleOrder.map(function (id) {
      var m = D.modules[id];
      var present = M.hasModule(state, id);
      return '<button class="module-option" data-action="add-module" data-module="' + id + '"' + (present ? " disabled" : "") + ">" +
        "<span><b>" + m.label + "</b><p>" + U.esc(m.blurb) + "</p>" +
        '<span class="mo-meta"><span>Cost · ' + m.cost + "</span><span>" + U.esc(m.damage[0]) + "</span></span></span>" +
        '<span class="mo-state">' + (present ? "In chain" : "Add") + "</span></button>";
    }).join("");
    openDialog("scrim-modules", list.querySelector("button:not([disabled])"));
  }

  function openShortcuts() {
    var groups = [];
    D.commands.forEach(function (c) { if (groups.indexOf(c.group) < 0) groups.push(c.group); });
    $("shortcut-groups").innerHTML = groups.map(function (g) {
      var items = D.commands.filter(function (c) { return c.group === g; });
      return '<div class="shortcut-group"><h3>' + g + "</h3>" + items.map(function (c) {
        return '<div class="shortcut-row"><span>' + U.esc(c.title) + '</span><kbd class="kbd">' + U.esc(c.keys || "—") + "</kbd></div>";
      }).join("") + "</div>";
    }).join("");
    openDialog("scrim-shortcuts", document.querySelector('#scrim-shortcuts .dialog-head .icon-btn'));
  }

  function openPrefs() {
    renderPrefs();
    openDialog("scrim-prefs", document.querySelector("#prefs-body .segmented button"));
  }

  function renderPrefs() {
    var p = state.prefs;
    function seg(key, options) {
      return '<div class="segmented" role="group" aria-label="' + key + '">' + options.map(function (o) {
        var on = String(p[key]) === String(o.value);
        return '<button data-action="set-pref" data-pref="' + key + '" data-value="' + o.value + '" aria-pressed="' + on + '">' +
          (on ? '<span class="tick">✓ </span>' : "") + o.label + "</button>";
      }).join("") + "</div>";
    }
    $("prefs-body").innerHTML =
      '<div class="pref-section"><h3>Appearance</h3>' +
      '<div class="pref-row"><div><div class="pref-label">Density</div><div class="pref-help">Compact fits more rows; comfortable is easier to hit.</div></div>' +
      '<div class="pref-control">' + seg("density", [{ value: "compact", label: "Compact" }, { value: "comfortable", label: "Comfortable" }]) + "</div></div>" +
      '<div class="pref-row"><div><div class="pref-label">Accent</div><div class="pref-help">One accent; state is always also written.</div></div>' +
      '<div class="pref-control swatches">' + ["amber", "teal", "slate"].map(function (a) {
        return '<button class="swatch" data-accent="' + a + '" data-action="set-pref" data-pref="accent" data-value="' + a +
          '" aria-pressed="' + (p.accent === a) + '" aria-label="' + a + ' accent"></button>';
      }).join("") + "</div></div></div>" +

      '<div class="pref-section"><h3>Time &amp; measurement</h3>' +
      '<div class="pref-row"><div><div class="pref-label">Position readout</div><div class="pref-help">Non-drop timecode counts real frames.</div></div>' +
      '<div class="pref-control">' + seg("timeDisplay", [{ value: "timecode", label: "Timecode" }, { value: "clock", label: "Clock" }, { value: "frames", label: "Frames" }]) + "</div></div>" +
      '<div class="pref-row"><div><div class="pref-label">Measurement lanes</div><div class="pref-help">Show the signal graphs under the timeline.</div></div>' +
      '<div class="pref-control">' + seg("timelineGraphs", [{ value: "true", label: "Show" }, { value: "false", label: "Hide" }]) + "</div></div></div>" +

      '<div class="pref-section"><h3>Accessibility</h3>' +
      '<label class="check"><input type="checkbox" data-control="pref-toggle" data-key="reduceMotion" ' + (p.reduceMotion ? "checked" : "") + ">" +
      '<span class="box">' + U.icon("check") + '</span><span class="check-text"><b>Reduce motion</b><small>Removes transitions. Also follows the operating system setting.</small></span></label>' +
      '<label class="check"><input type="checkbox" data-control="pref-toggle" data-key="highContrast" ' + (p.highContrast ? "checked" : "") + ">" +
      '<span class="box">' + U.icon("check") + '</span><span class="check-text"><b>Higher contrast</b><small>Stronger text and borders. Follows prefers-contrast too.</small></span></label>' +
      '<label class="check"><input type="checkbox" data-control="pref-toggle" data-key="safeAreas" ' + (p.safeAreas ? "checked" : "") + ">" +
      '<span class="box">' + U.icon("check") + '</span><span class="check-text"><b>Safe-area guides</b><small>Title and action safe over the picture.</small></span></label>' +
      "</div>";
  }

  function setPref(key, value) {
    if (key === "timelineGraphs") {
      state.lanesOpen = value === "true";
      state.prefs.timelineGraphs = state.lanesOpen;
    } else {
      state.prefs[key] = value;
    }
    if (key === "safeAreas") { /* handled via pref-toggle */ }
    M.savePrefs(state.prefs);
    applyPrefs();
    renderPrefs();
    renderViewer();
    renderTimeline();
    renderInspector();
    announce(key + " set to " + value + ".");
  }

  function toast(kind, title, body) {
    var wrap = $("toasts");
    var el = document.createElement("div");
    el.className = "toast";
    el.dataset.state = kind;
    el.innerHTML = U.icon(kind === "ok" ? "check" : kind === "err" ? "warning" : "info") +
      "<div><b>" + U.esc(title) + "</b>" + U.esc(body) + "</div>";
    wrap.appendChild(el);
    setTimeout(function () {
      el.style.opacity = "0";
      el.style.transform = "translateY(4px)";
      setTimeout(function () { el.remove(); }, 200);
    }, 4600);
  }

  function announce(text) {
    var live = $("live-region");
    if (live) live.textContent = text;
  }

  // ------------------------------------------------------------------ input
  function stepFrames(n) {
    var clip = M.clipMeta(state);
    if (!clip) return;
    seekTo(state.viewer.playhead + n / f.fps(clip.rate));
  }

  function nextMark(dir) {
    var clip = M.clipMeta(state);
    if (!clip) return;
    var list = M.marks(clip);
    var cur = state.viewer.playhead + (dir > 0 ? 0.001 : -0.001);
    var found = dir > 0
      ? list.filter(function (m) { return m.start > cur; }).sort(function (a, b) { return a.start - b.start; })[0]
      : list.filter(function (m) { return m.start < cur; }).sort(function (a, b) { return b.start - a.start; })[0];
    if (!found) {
      announce(dir > 0 ? "No later finding." : "No earlier finding.");
      return;
    }
    jumpToMark(list.indexOf(found));
  }

  function setIn() {
    state.range = state.range
      ? { in: Math.min(state.viewer.playhead, state.range.out), out: state.range.out }
      : { in: state.viewer.playhead, out: Math.min(state.viewer.playhead + 20, M.clipMeta(state).duration) };
    if (state.range.out <= state.range.in) state.range.out = Math.min(state.range.in + 1, M.clipMeta(state).duration);
    updateSelectionBand();
    announce("In-point at " + f.durationShort(state.range.in) + ".");
  }

  function setOut() {
    state.range = state.range
      ? { in: state.range.in, out: Math.max(state.viewer.playhead, state.range.in + 0.2) }
      : { in: Math.max(0, state.viewer.playhead - 20), out: state.viewer.playhead };
    if (state.range.out <= state.range.in) state.range.out = state.range.in + 1;
    updateSelectionBand();
    announce("Out-point at " + f.durationShort(state.range.out) + ".");
  }

  function toggleLanes() {
    state.lanesOpen = !state.lanesOpen;
    state.prefs.timelineGraphs = state.lanesOpen;
    M.savePrefs(state.prefs);
    $("btn-lanes").setAttribute("aria-pressed", String(state.lanesOpen));
    renderTimeline();
  }

  function toggleZoom() {
    state.viewer.zoom100 = !state.viewer.zoom100;
    renderViewer();
    announce(state.viewer.zoom100 ? "Zoom 100 percent." : "Zoom to fit.");
  }

  function toggleSafe() {
    state.viewer.safe = !state.viewer.safe;
    state.prefs.safeAreas = state.viewer.safe;
    M.savePrefs(state.prefs);
    renderViewer();
    announce(state.viewer.safe ? "Safe-area guides on." : "Safe-area guides off.");
  }

  function moveSelectedStep(dir) {
    if (M.moveStep(state, selectedStep, dir)) {
      selectedStep += dir;
      renderInspector();
      renderContext();
      announce("Step moved. Order is part of the recipe.");
    }
  }

  function removeSelectedStep() {
    var removed = M.removeStep(state, selectedStep);
    if (!removed.ok) { toast("warn", "Cannot remove", removed.error); return; }
    selectedStep = Math.max(0, Math.min(selectedStep, state.chain.length - 1));
    renderInspector();
    renderContext();
    renderStages();
    toast("ok", D.modules[removed.removed.module].label + " removed", "The source file is untouched — only the chain changed.");
  }

  function resetSelectedStep() {
    var step = state.chain[selectedStep];
    if (!step) return;
    M.updateParams(state, selectedStep, Object.assign({}, D.modules[step.module].defaults));
    step.params = Object.assign({}, D.modules[step.module].defaults);
    renderInspector();
    toast("ok", D.modules[step.module].label + " reset", "Back to the module's conservative defaults.");
  }

  // ------------------------------------------------------------------ keys
  function signature(e) {
    var mods = [];
    if (e.ctrlKey || e.metaKey) mods.push("Ctrl");
    if (e.altKey) mods.push("Alt");
    var key = e.key;
    var named = {
      ArrowLeft: "Left", ArrowRight: "Right", ArrowUp: "Up", ArrowDown: "Down",
      Escape: "Esc", " ": "Space", Backspace: "Backspace", Enter: "Enter", Delete: "Delete", Tab: "Tab"
    };
    if (named[key]) key = named[key];
    else if (/^[a-zA-Z]$/.test(key)) key = key.toUpperCase();
    else if (!/^[0-9]$/.test(key) && key.length === 1) {
      // punctuation keeps its character and ignores Shift
    } else {
      if (e.shiftKey) mods.push("Shift");
    }
    if (/^[a-zA-Z]$/.test(key) && e.shiftKey) mods.push("Shift");
    if (/^[A-Z]$/.test(key) && !e.shiftKey) {
      // caps-lock style uppercase letters still mean the letter key
    }
    return mods.concat([key]).join("+");
  }

  function bindEvents() {
    document.addEventListener("click", onClick);
    document.addEventListener("input", onInput);
    document.addEventListener("change", onChange);
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("pointerdown", onPointerDown);
    global.addEventListener("resize", debounce(function () { renderTimeline(); renderViewer(); }, 120));

    // Wipe drag
    var handle = $("wipe-handle");
    var wrap = $("compare-wrap");
    function moveWipe(e) {
      var rect = wrap.getBoundingClientRect();
      var pct = Math.max(2, Math.min(98, ((e.clientX - rect.left) / rect.width) * 100));
      handle.style.left = pct + "%";
      handle.setAttribute("aria-valuenow", String(Math.round(pct)));
      wrap.style.setProperty("--wipe", pct + "%");
      wrap.style.setProperty("--wipe-rest", pct + "%");
      wrap.querySelector(".compare-after").style.clipPath = "inset(0 0 0 " + pct + "%)";
      wrap.querySelector(".compare-before").style.clipPath = "inset(0 " + (100 - pct) + "% 0 0)";
    }
    handle.addEventListener("pointerdown", function (e) {
      handle.setPointerCapture(e.pointerId);
      handle.addEventListener("pointermove", moveWipe);
      handle.addEventListener("pointerup", function up() {
        handle.removeEventListener("pointermove", moveWipe);
        handle.removeEventListener("pointerup", up);
      });
    });
    handle.addEventListener("keydown", function (e) {
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        e.preventDefault();
        var now = Number(handle.getAttribute("aria-valuenow")) || 50;
        now = Math.max(2, Math.min(98, now + (e.key === "ArrowLeft" ? -4 : 4)));
        var rect = wrap.getBoundingClientRect();
        moveWipe({ clientX: rect.left + (now / 100) * rect.width });
      }
    });

    $("queue-filter").addEventListener("input", function (e) {
      state.queueFilter = e.target.value;
      renderQueue();
    });
    $("dest-input") && $("dest-input").addEventListener("input", function (e) {
      state.destination = e.target.value;
    });
    $("palette-input").addEventListener("input", function (e) {
      palette.index = 0;
      renderPalette(e.target.value);
    });
    $("palette-input").addEventListener("keydown", function (e) {
      if (e.key === "ArrowDown") { e.preventDefault(); palette.index = Math.min(palette.items.length - 1, palette.index + 1); renderPalette(palette.query); }
      else if (e.key === "ArrowUp") { e.preventDefault(); palette.index = Math.max(0, palette.index - 1); renderPalette(palette.query); }
      else if (e.key === "Enter") {
        e.preventDefault();
        var item = palette.items[palette.index];
        if (item) runCommand(item.id);
      } else if (e.key === "Escape") { e.preventDefault(); closeAllDialogs(); }
    });
    document.querySelectorAll("[data-close-dialog]").forEach(function (b) {
      b.addEventListener("click", function () { closeDialog(b.getAttribute("data-close-dialog")); });
    });
    document.querySelectorAll(".scrim").forEach(function (s) {
      s.addEventListener("mousedown", function (e) {
        if (e.target === s) closeDialog(s.id);
      });
    });
    $("confirm-run").addEventListener("click", runExport);
    $("prefs-reset").addEventListener("click", function () {
      state.prefs = M.defaultPrefs();
      state.lanesOpen = true;
      M.savePrefs(state.prefs);
      applyPrefs();
      renderPrefs();
      renderViewer();
      renderTimeline();
      toast("ok", "Preferences reset", "Back to the defaults.");
    });
    $("jobs-head").addEventListener("click", function () {
      var wrap = $("jobs");
      wrap.dataset.open = wrap.dataset.open === "true" ? "false" : "true";
      this.setAttribute("aria-expanded", wrap.dataset.open);
      renderJobs();
    });
    $("btn-palette-top").addEventListener("click", openPalette);
    $("btn-help").addEventListener("click", openShortcuts);
    $("btn-prefs").addEventListener("click", openPrefs);
    $("btn-queue-toggle").addEventListener("click", function () {
      document.body.dataset.queueOpen = document.body.dataset.queueOpen === "true" ? "false" : "true";
      this.setAttribute("aria-expanded", document.body.dataset.queueOpen);
    });
    $("btn-queue-close").addEventListener("click", function () {
      document.body.dataset.queueOpen = "false";
      $("btn-queue-toggle").setAttribute("aria-expanded", "false");
    });
    $("btn-inspector-close").addEventListener("click", function () {
      document.body.dataset.inspectorOpen = "false";
    });
    $("inspector-scrim").addEventListener("click", function () {
      document.body.dataset.inspectorOpen = "false";
    });
    // Tabs: arrow-key navigation, per WAI-ARIA.
    document.querySelectorAll(".inspector-tab").forEach(function (tab) {
      tab.addEventListener("keydown", function (e) {
        var tabs = Array.prototype.slice.call(document.querySelectorAll(".inspector-tab"));
        var i = tabs.indexOf(tab);
        if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
          e.preventDefault();
          var next = tabs[(i + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length];
          setTab(next.dataset.tab, true);
        }
      });
    });
  }

  function onClick(e) {
    var target = e.target.closest("[data-action]");
    if (!target) {
      var tab = e.target.closest(".inspector-tab");
      if (tab) setTab(tab.dataset.tab);
      return;
    }
    var action = target.dataset.action;
    switch (action) {
      case "stage": setStage(target.dataset.stage); break;
      case "select-clip": selectClip(target.dataset.id); break;
      case "view-mode": setViewMode(target.dataset.mode); break;
      case "tab": setTab(target.dataset.tab); break;
      case "jump-mark": jumpToMark(Number(target.dataset.index)); break;
      case "suggest-range":
      case "suggestRange": suggestRange(); break;
      case "set-in":
      case "setIn": setIn(); break;
      case "set-out":
      case "setOut": setOut(); break;
      case "addModule":
      case "add-module": {
        if (action === "addModule") { openModules(); break; }
        var res = M.addModule(state, target.dataset.module);
        if (!res.ok) { toast("warn", "Not added", res.error); break; }
        selectedStep = res.index;
        closeAllDialogs();
        setTab("chain");
        renderInspector();
        renderContext();
        renderStages();
        toast("ok", D.modules[target.dataset.module].label + " added", "Conservative defaults. Every control shows what it can damage.");
        break;
      }
      case "toggle-step": {
        var idx = Number(target.dataset.index);
        state.chain[idx].open = !state.chain[idx].open;
        selectedStep = idx;
        renderInspector();
        break;
      }
      case "move-step":
        M.moveStep(state, Number(target.dataset.index), Number(target.dataset.dir));
        selectedStep = Math.max(0, Math.min(state.chain.length - 1, Number(target.dataset.index) + Number(target.dataset.dir)));
        renderInspector();
        renderContext();
        break;
      case "remove-step": {
        selectedStep = Number(target.dataset.index);
        removeSelectedStep();
        break;
      }
      case "reset-step":
        selectedStep = Number(target.dataset.index);
        resetSelectedStep();
        break;
      case "set-param": {
        var index = Number(target.dataset.index);
        var patch = {};
        patch[target.dataset.key] = target.dataset.value;
        var r = M.updateParams(state, index, patch);
        if (!r.ok) { toast("err", "Value refused", r.error); break; }
        renderInspector();
        renderContext();
        break;
      }
      case "confirm-order": confirmOrder(target.dataset.value); break;
      case "set-preset":
        state.presetId = target.dataset.preset;
        renderInspector();
        renderContext();
        announce("Export intent set to " + (D.presets.find(function (p) { return p.id === state.presetId; }) || {}).title + ".");
        break;
      case "dest-check": {
        var free = D.project.freeBytes;
        var est = M.estimateBytes(state);
        var ok = free >= est * 1.1 && !state.exported;
        toast(ok ? "ok" : "warn", ok ? "Destination is free" : "Check the destination",
          ok ? f.bytes(free) + " free; estimate " + f.bytes(est) + ". The proof sidecars will be new files." :
            "An output already exists at this path. Outputs are always new files.");
        break;
      }
      case "render-preview":
      case "renderPreview": renderPreview(); break;
      case "render-export":
      case "renderExport": openExport(); break;
      case "verify": verifyExport(); break;
      case "rerun": rerunExport(); break;
      case "rehash": rehash(); break;
      case "rerunInspect": rerunInspect(); break;
      case "addSource": addSource(); break;
      case "copy-sha": copy(target.dataset.sha, "SHA-256 copied"); break;
      case "copy-filter": copy(M.chainFilter(state).filter, "Filter string copied"); break;
      case "copy-command": {
        var cmd = M.exportCommand(state);
        copy(cmd ? cmd.join(" ") : "", cmd ? "Command copied" : "Nothing to copy");
        break;
      }
      case "step": stepFrames(Number(target.dataset.delta)); break;
      case "markPrev": nextMark(-1); break;
      case "markNext": nextMark(1); break;
      case "setIn": setIn(); break;
      case "setOut": setOut(); break;
      case "lanes": toggleLanes(); break;
      case "compare": toggleCompare(); break;
      case "zoom": toggleZoom(); break;
      case "safe": toggleSafe(); break;
      case "projectSwitch":
        toast("ok", "One project at a time in this prototype", "The desktop app switches projects without unloading the current one; recipes live with the project.");
        break;
      case "cancel-job": cancelJob(target.dataset.job); break;
      case "set-pref": setPref(target.dataset.pref, target.dataset.value); break;
      case "run-command": runCommand(target.dataset.command); break;
      default: break;
    }
    // Inspector is a drawer under 1080px; open it when a tab is chosen from the context bar.
    if (["tab", "addModule", "add-module", "renderPreview", "renderExport", "verify", "rerun"].indexOf(action) >= 0) {
      document.body.dataset.inspectorOpen = "true";
    }
  }

  function onInput(e) {
    var el = e.target;
    if (el.dataset.control === "param" && el.type === "range") {
      var i = Number(el.dataset.index), key = el.dataset.key;
      var value = Number(el.value);
      var patch = {};
      patch[key] = value;
      var r = M.updateParams(state, i, patch);
      if (!r.ok) return;
      var pct = ((value - Number(el.min)) / (Number(el.max) - Number(el.min))) * 100;
      el.style.setProperty("--fill", pct + "%");
      var row = el.closest(".slider-row");
      if (row) {
        var num = row.querySelector(".num-input");
        if (num) num.value = value;
      }
      var head = el.closest(".field").querySelector(".field-note");
      if (head) head.textContent = value + el.dataset.unit;
      // Re-render only the summary/gate-level views, not the control being dragged.
      var stepCard = el.closest(".step");
      if (stepCard) {
        var title = stepCard.querySelector(".step-summary");
        if (title) title.textContent = M.summaryFor(state.chain[i]);
      }
      renderContext();
      updateChainFoot();
    } else if (el.id === "dest-input") {
      state.destination = el.value;
    }
  }

  function onChange(e) {
    var el = e.target;
    if (el.dataset.control === "param" && el.type === "checkbox") {
      var patch = {};
      patch[el.dataset.key] = el.checked;
      var r = M.updateParams(state, Number(el.dataset.index), patch);
      if (!r.ok) { toast("err", "Value refused", r.error); el.checked = !el.checked; return; }
      renderInspector();
      renderContext();
    } else if (el.dataset.control === "param-number") {
      var patch2 = {};
      patch2[el.dataset.key] = Number(el.value);
      var res = M.updateParams(state, Number(el.dataset.index), patch2);
      if (!res.ok) {
        el.setAttribute("aria-invalid", "true");
        toast("err", "Value refused", res.error);
        return;
      }
      el.removeAttribute("aria-invalid");
      renderInspector();
      renderContext();
    } else if (el.dataset.control === "override") {
      state.override = el.checked;
      renderInspector();
      renderContext();
      toast(state.override ? "warn" : "ok", state.override ? "Expert override recorded" : "Override removed",
        state.override ? "The export log will record that this chain shipped without a preview." : "The preview gate applies again.");
    } else if (el.dataset.control === "pref-toggle") {
      state.prefs[el.dataset.key] = el.checked;
      M.savePrefs(state.prefs);
      applyPrefs();
      renderViewer();
      renderInspector();
      announce(el.dataset.key + (el.checked ? " on." : " off."));
    }
  }

  function updateChainFoot() {
    var foot = document.querySelector(".chain-foot .filter-line");
    if (!foot) return;
    var plan = M.chainFilter(state);
    foot.textContent = plan.ok ? plan.filter : "⚠ " + plan.error;
  }

  function onPointerDown(e) {
    var ruler = e.target.closest("#ruler-track");
    if (ruler) {
      seekFromEvent(e, ruler);
      var move = function (ev) { seekFromEvent(ev, ruler); };
      var up = function () {
        document.removeEventListener("pointermove", move);
        document.removeEventListener("pointerup", up);
      };
      document.addEventListener("pointermove", move);
      document.addEventListener("pointerup", up);
      return;
    }
    var lane = e.target.closest(".lane-graph");
    if (lane) {
      seekFromEvent(e, lane);
      return;
    }
    var track = e.target.closest("#track-area");
    if (track && !e.target.closest(".mark-tick")) {
      seekFromEvent(e, track);
    }
  }

  function onKeyDown(e) {
    var openScrim = document.querySelector(".scrim:not([hidden])");
    if (openScrim && e.key === "Tab") {
      var focusables = Array.prototype.slice.call(
        openScrim.querySelectorAll('button:not([disabled]), [href], input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"])')
      ).filter(function (el) { return el.offsetParent !== null || el.getClientRects().length > 0 || true; });
      if (focusables.length) {
        var first = focusables[0];
        var last = focusables[focusables.length - 1];
        if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
        else if (!openScrim.contains(document.activeElement)) { e.preventDefault(); first.focus(); }
      }
      return;
    }
    var typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName) || e.target.isContentEditable;
    if (e.key === "Escape") {
      if (document.querySelector(".scrim:not([hidden])")) { closeAllDialogs(); return; }
      if (typing) { e.target.blur(); return; }
      state.range = null;
      updateSelectionBand();
      announce("Preview range cleared.");
      return;
    }
    var sig = signature(e);
    if (typing && sig !== "Ctrl+K" && sig !== "Ctrl+,") return;
    // Stage shortcuts
    var stage = { "Alt+1": "ingest", "Alt+2": "inspect", "Alt+3": "decide", "Alt+4": "preview", "Alt+5": "export", "Alt+6": "prove" }[sig];
    if (stage) { e.preventDefault(); setStage(stage); return; }

    var map = {
      "Ctrl+K": "help.palette",
      "Ctrl+,": "prefs.open",
      "Ctrl+O": "source.add",
      "Ctrl+Shift+H": "source.rehash",
      "Ctrl+E": "export.open",
      "Ctrl+Enter": "preview.render",
      "Ctrl+Shift+Enter": "export.run",
      "Ctrl+Shift+V": "proof.verify",
      "Ctrl+Shift+R": "proof.rerun",
      "Ctrl+1": "viewer.zoom_fit",
      "?": "help.shortcuts",
      "\\": "viewer.compare",
      "'": "viewer.safe_areas",
      "P": "viewer.program",
      "W": "viewer.waveform",
      "H": "viewer.histogram",
      "U": "viewer.vectorscope",
      "F": "viewer.field_split",
      "D": "viewer.temporal",
      "Left": "timeline.step_back",
      "Right": "timeline.step_forward",
      "Shift+Left": "timeline.jump_back",
      "Shift+Right": "timeline.jump_forward",
      "N": "timeline.prev_mark",
      "Shift+N": "timeline.next_mark",
      "I": "timeline.set_in",
      "O": "timeline.set_out",
      "G": "timeline.graphs",
      "Shift+I": "preview.range_suggestion",
      "A": "chain.add",
      "Alt+Up": "chain.move_up",
      "Alt+Down": "chain.move_down",
      "Delete": "chain.remove",
      "Alt+Backspace": "chain.reset",
      "Enter": "chain.toggle_step"
    };
    if (sig === "Enter") {
      e.preventDefault();
      toggleSelectedStep();
      return;
    }
    var id = map[sig];
    if (id) {
      e.preventDefault();
      var cmd = D.commands.find(function (c) { return c.id === id; });
      if (cmd) runCommand(cmd.id);
    }
  }

  function toggleSelectedStep() {
    var step = state.chain[selectedStep];
    if (!step) { openModules(); return; }
    step.open = !step.open;
    renderInspector();
  }

  function copy(text, message) {
    if (!text) { toast("warn", "Nothing to copy", "There is no value here yet."); return; }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(function () { toast("ok", message, text.length > 90 ? text.slice(0, 90) + "…" : text); },
        function () { toast("warn", "Clipboard unavailable", "Select the value and copy it manually."); });
    } else {
      toast("warn", "Clipboard unavailable", "Select the value and copy it manually.");
    }
  }

  /** The timeline gutter width, read from CSS so labels and playhead agree. */
  function gutter() {
    var v = getComputedStyle(document.documentElement).getPropertyValue("--tl-gutter");
    var n = parseFloat(v);
    return isFinite(n) && n > 0 ? n : 96;
  }

  function debounce(fn, ms) {
    var t;
    return function () {
      clearTimeout(t);
      t = setTimeout(fn, ms);
    };
  }

  document.addEventListener("DOMContentLoaded", init);
})(window);
