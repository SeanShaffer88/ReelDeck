/* ReelDeck prototype — sample project data and the catalogs the shell
   renders.

   Everything here is *sample data* for a design prototype: one fictional
   oral-history project, its measurements, and the static catalogs
   (modules, presets, commands) that mirror the real model in
   crates/rd-ui/src. No file is read and no process is run. */
(function (global) {
  "use strict";

  var RD = global.RD;
  var f = RD.format;
  var HZ = 2; // measurement samples per second

  // ---------------------------------------------------------------- modules
  var MODULES = {
    deinterlace: {
      id: "deinterlace",
      label: "Deinterlace",
      blurb: "Rebuild one full picture from each interlaced pair, so motion stops combing.",
      cost: "fast",
      damage: [
        "The wrong field order does not fail — it judders. Check a pan in the preview.",
        "One frame per field doubles the frame count and the master's size."
      ],
      defaults: { engine: "bwdif", fieldOrder: "tff", outputRate: "send_frame", onlyFlagged: false }
    },
    stabilize: {
      id: "stabilize",
      label: "Stabilize",
      blurb: "Smooth camera movement over time. It cannot recover what was never captured.",
      cost: "slowest (two passes)",
      damage: [
        "Smoothing cannot repair sync loss, rolling-shutter wobble, or missing signal.",
        "Zoom hides the moving border by cropping picture area. The cost is shown before rendering."
      ],
      defaults: { smoothing: 15, zoom: 0, tripod: false }
    },
    denoise: {
      id: "denoise",
      label: "Denoise",
      blurb: "Reduce grain and low-light noise, luma and chroma separately.",
      cost: "slow",
      damage: [
        "High temporal strength smears motion and erases film grain first.",
        "Denoising is irreversible detail loss. A preservation export needs a light touch."
      ],
      defaults: { engine: "hqdn3d", luma: 2, chroma: 2, temporal: 3 }
    },
    finish: {
      id: "finish",
      label: "Finish",
      blurb: "Crop blanking, scale, gentle level correction, deflicker.",
      cost: "fast",
      damage: [
        "Scaling resamples: the scaler used is recorded, and upscaling invents nothing.",
        "Levels change can clip highlights. The out-of-range ratio is in Inspect before you move it."
      ],
      defaults: { crop: "none", scaleWidth: 720, scaleHeight: 576, deflicker: false, normalizeLevels: false }
    }
  };
  var MODULE_ORDER = ["deinterlace", "stabilize", "denoise", "finish"];

  // ---------------------------------------------------------------- presets
  var PRESETS = [
    {
      id: "preservation",
      title: "Preservation master",
      format: "FFV1 v3 · Matroska · FLAC",
      extension: "mkv",
      why: "Lossless and open, with per-slice checksums so corruption is localised. Never a delivery file.",
      facts: { lossless: "Yes", compat: "Archive", sizeFactor: 9.6 }
    },
    {
      id: "mezzanine",
      title: "Mezzanine / edit",
      format: "ProRes 422 HQ · QuickTime · PCM",
      extension: "mov",
      why: "Drops straight onto an editing timeline with headroom for grading.",
      facts: { lossless: "Visually lossless", compat: "Edit", sizeFactor: 6.2 }
    },
    {
      id: "access",
      title: "Access / delivery",
      format: "H.264 · MP4 · AAC",
      extension: "mp4",
      why: "For viewing and sharing. Never presented as a master.",
      facts: { lossless: "No", compat: "Share", sizeFactor: 0.4 }
    }
  ];

  // ---------------------------------------------------------------- clip data
  var RATE = { num: 25, den: 1 };
  var SHA = "9f3c17be4a0d5c8e6b2f9147ac3d80ee52b6c419d7f0a83b1c6e5d4279a0f3b8";

  var clips = [
    {
      id: "02",
      name: "Church service",
      sub: "St. Alban's · 1979",
      file: "02_StAlbans_service_1979.mkv",
      duration: 1885,
      rate: RATE,
      width: 720, height: 576,
      codec: "DV · Matroska",
      sizeBytes: 7.1 * 1024 * 1024 * 1024,
      fieldOrderFlag: "bb",
      audio: "48 kHz · stereo · −21.0 LUFS",
      fixity: { state: "ok", sha256: "3d8a91c05e77b2f4a6d1c8e0934b7f25aa10c6d8e4b93f1702cc5a9d8e6b4f21" },
      inspect: { state: "done", suggestion: "bff", confirmed: "bff", sampled: "full file", frames: 47125 },
      idet: { singleTff: 12, singleBff: 604, singleProgressive: 41, singleUndetermined: 118, multiTff: 6, multiBff: 688, multiProgressive: 22, multiUndetermined: 39 },
      signal: { yAvg: 88.4, satAvg: 11.2, toutMax: 0.21, brngMean: 0.004 },
      anomalies: [
        { kind: "black", start: 0, end: 3.8, label: "Leader" },
        { kind: "outlier", start: 644.2, frame: 16105, tout: 0.31 }
      ]
    },
    {
      id: "04",
      name: "Interview — E. Vance",
      sub: "Local history project · 1987",
      file: "04_EVance_interview_1987.mkv",
      duration: 2538,
      rate: RATE,
      width: 720, height: 576,
      codec: "DV · Matroska",
      sizeBytes: 9.4 * 1024 * 1024 * 1024,
      fieldOrderFlag: "tt",
      audio: "48 kHz · stereo · −18.4 LUFS · true peak −1.2 dBTP",
      fixity: { state: "ok", sha256: SHA },
      inspect: {
        state: "done",
        suggestion: "tff",
        confirmed: "tff",
        sampled: "full file",
        frames: 63450
      },
      idet: {
        singleTff: 618, singleBff: 3, singleProgressive: 24, singleUndetermined: 105,
        multiTff: 695, multiBff: 1, multiProgressive: 12, multiUndetermined: 42
      },
      signal: { yAvg: 96.8, satAvg: 14.6, toutMax: 0.81, brngMean: 0.011 },
      anomalies: [
        { kind: "black", start: 0, end: 4.2, label: "Leader" },
        { kind: "outlier", start: 372.4, frame: 9310, tout: 0.68 },
        { kind: "outlier", start: 1122.0, frame: 28050, tout: 0.81 },
        { kind: "freeze", start: 1334.0, end: 1336.4 },
        { kind: "outlier", start: 1865.8, frame: 46645, tout: 0.42 },
        { kind: "black", start: 2390.0, end: 2392.4, label: "Head clog" }
      ]
    },
    {
      id: "07",
      name: "Mill walkthrough",
      sub: "Riverside Mill · 1983",
      file: "07_RiversideMill_walkthrough_1983.mkv",
      duration: 3120,
      rate: RATE,
      width: 720, height: 576,
      codec: "DV · Matroska",
      sizeBytes: 11.6 * 1024 * 1024 * 1024,
      fieldOrderFlag: "tt",
      audio: "48 kHz · stereo · −24.8 LUFS",
      fixity: { state: "mismatch", sha256: "b71e4c9a02d6f385a1c7e5904db28f6a0c93e17b5d8a42f60e9c1b7d3a5f8024" },
      inspect: { state: "failed", suggestion: null, confirmed: null, sampled: "not analyzed" },
      idet: null,
      signal: null,
      anomalies: []
    },
    {
      id: "11",
      name: "Council meeting",
      sub: "Ashland council · 1991",
      file: "11_Ashland_council_1991_incomplete.mkv",
      duration: 1450,
      rate: RATE,
      width: 720, height: 576,
      codec: "DV · Matroska",
      sizeBytes: 3.7 * 1024 * 1024 * 1024,
      fieldOrderFlag: "unknown",
      audio: "48 kHz · stereo",
      fixity: { state: "pending", sha256: null },
      inspect: { state: "none", suggestion: null, confirmed: null, sampled: "not analyzed" },
      idet: null,
      signal: null,
      anomalies: []
    }
  ];

  // ---------------------------------------------------------------- series
  function lcg(seed) {
    var s = seed >>> 0;
    return function () {
      s = (Math.imul(1664525, s) + 1013904223) >>> 0;
      return s / 4294967296;
    };
  }

  /** Deterministic measurement series for a clip, at 2 samples per second.
      Spikes land exactly on the clip's anomaly times, as they would when
      the inspection measured them. */
  function buildSeries(clip) {
    var n = Math.round(clip.duration * HZ);
    var rand = lcg(clip.duration * 7919 + Number(clip.id) * 104729);
    var luma = new Array(n);
    var tout = new Array(n);
    var brng = new Array(n);
    var sat = new Array(n);

    var anomalies = clip.anomalies || [];
    var toutAt = {};
    var blackAt = [];
    var brngWindows = [
      { from: 748, to: 762, peak: 0.038 },   // titles over a bright card
      { from: 1618, to: 1632, peak: 0.021 },
      { from: 2244, to: 2262, peak: 0.027 }
    ];
    anomalies.forEach(function (a) {
      if (a.kind === "outlier") toutAt[Math.round(a.start * HZ)] = a.tout;
      if (a.kind === "black") blackAt.push(a);
    });

    var base = 92 + (clip.signal ? clip.signal.yAvg - 92 : 0);
    for (var i = 0; i < n; i++) {
      var t = i / HZ;
      // Slow drift between shots plus a little texture: an honest-looking
      // signal trace rather than a sine wave.
      var drift = 10 * Math.sin(t / 210) + 5 * Math.sin(t / 47 + 1.2);
      var grain = (rand() - 0.5) * 7;
      var l = base + drift + grain;

      blackAt.forEach(function (b) {
        if (t >= b.start && t <= b.end) l = 17 + rand() * 2;
      });
      luma[i] = Math.max(16, Math.min(235, l));

      var hot = toutAt[i];
      var near = 0;
      for (var k = -2; k <= 2; k++) {
        if (toutAt[i + k] && Math.abs(k) > 0) near = Math.max(near, toutAt[i + k] * 0.28);
      }
      tout[i] = hot != null ? hot : 0.006 + rand() * 0.011 + near * (0.4 + rand() * 0.4);

      var b = 0.0008 + rand() * 0.0016;
      brngWindows.forEach(function (w) {
        if (t >= w.from && t <= w.to) {
          var mid = (w.from + w.to) / 2;
          var span = (w.to - w.from) / 2;
          b += w.peak * Math.max(0, 1 - Math.abs(t - mid) / span);
        }
      });
      brng[i] = b;

      sat[i] = 12 + 7 * Math.sin(t / 133 + 0.4) + (luma[i] < 40 ? -6 : 0) + rand() * 2;
    }

    return [
      { id: "y_avg", label: "Luma", unit: " y", domain: [16, 235], values: luma, threshold: null, scaleLabel: "0–255" },
      { id: "tout", label: "Temporal outliers", unit: "", domain: [0, 0.25], values: tout, threshold: 0.05, scaleLabel: "0–0.25" },
      { id: "brng", label: "Out of broadcast range", unit: "", domain: [0, 0.05], values: brng, threshold: null, scaleLabel: "0–0.05" },
      { id: "sat_avg", label: "Saturation", unit: "", domain: null, values: sat, threshold: null, scaleLabel: "auto" }
    ];
  }

  // ---------------------------------------------------------------- chain
  var initialChain = [
    {
      module: "deinterlace", open: true,
      params: { engine: "bwdif", fieldOrder: "tff", outputRate: "send_frame", onlyFlagged: false }
    },
    {
      module: "denoise", open: false,
      params: { engine: "hqdn3d", luma: 2, chroma: 2, temporal: 3 }
    },
    {
      module: "finish", open: false,
      params: { crop: "blanking", scaleWidth: 720, scaleHeight: 576, deflicker: false, normalizeLevels: false }
    }
  ];

  // ---------------------------------------------------------------- commands
  // Mirrors crates/rd-ui/src/shortcuts.rs. `run` names the handler.
  var COMMANDS = [
    { id: "stage.ingest", run: "stage", arg: "ingest", title: "Go to Ingest", keys: "Alt+1", group: "Workflow", hint: "", aliases: "sources files fixity add" },
    { id: "stage.inspect", run: "stage", arg: "inspect", title: "Go to Inspect", keys: "Alt+2", group: "Workflow", hint: "", aliases: "analysis findings measure" },
    { id: "stage.decide", run: "stage", arg: "decide", title: "Go to Decide", keys: "Alt+3", group: "Workflow", hint: "", aliases: "chain modules operations" },
    { id: "stage.preview", run: "stage", arg: "preview", title: "Go to Preview", keys: "Alt+4", group: "Workflow", hint: "", aliases: "segment compare before after" },
    { id: "stage.export", run: "stage", arg: "export", title: "Go to Export", keys: "Alt+5", group: "Workflow", hint: "", aliases: "deliver render preset" },
    { id: "stage.prove", run: "stage", arg: "prove", title: "Go to Prove", keys: "Alt+6", group: "Workflow", hint: "", aliases: "verify sidecar proof log recipe" },
    { id: "source.add", run: "addSource", title: "Add source…", keys: "Ctrl+O", group: "Project", hint: "Sources are hashed on add and never modified.", aliases: "ingest open import file" },
    { id: "source.rehash", run: "rehash", title: "Re-check source fixity", keys: "Ctrl+Shift+H", group: "Project", hint: "Recomputes SHA-256 and compares it with the recorded value.", aliases: "hash verify integrity" },
    { id: "prefs.open", run: "prefs", title: "Preferences…", keys: "Ctrl+,", group: "Project", hint: "", aliases: "settings options theme density" },
    { id: "edit.undo", run: "undo", title: "Undo chain edit", keys: "Ctrl+Z", group: "Edit", hint: "Reverts the last chain edit on the selected source; each source keeps its own history.", aliases: "revert back history" },
    { id: "edit.redo", run: "redo", title: "Redo chain edit", keys: "Ctrl+Shift+Z", group: "Edit", hint: "Re-applies an undone chain edit on the selected source. Ctrl+Y also works.", aliases: "repeat forward history" },
    { id: "viewer.program", run: "viewMode", arg: "program", title: "Viewer: program", keys: "P", group: "Viewer", hint: "", aliases: "picture image" },
    { id: "viewer.waveform", run: "viewMode", arg: "waveform", title: "Viewer: waveform", keys: "W", group: "Viewer", hint: "", aliases: "luma levels scope" },
    { id: "viewer.histogram", run: "viewMode", arg: "histogram", title: "Viewer: histogram", keys: "H", group: "Viewer", hint: "", aliases: "distribution levels" },
    { id: "viewer.vectorscope", run: "viewMode", arg: "vectorscope", title: "Viewer: vectorscope", keys: "U", group: "Viewer", hint: "", aliases: "chroma saturation scope" },
    { id: "viewer.field_split", run: "viewMode", arg: "field", title: "Viewer: field split", keys: "F", group: "Viewer", hint: "Shows the two fields separately — combing is obvious here.", aliases: "interlace fields combing" },
    { id: "viewer.temporal", run: "viewMode", arg: "temporal", title: "Viewer: temporal difference", keys: "D", group: "Viewer", hint: "", aliases: "difference motion dropout" },
    { id: "viewer.compare", run: "compare", title: "Toggle before/after", keys: "\\", group: "Viewer", hint: "Compares the chain against the untouched source at the same frame.", aliases: "split wipe" },
    { id: "viewer.zoom_fit", run: "zoom", title: "Toggle 100% pixels", keys: "Ctrl+1", group: "Viewer", hint: "", aliases: "actual pixels 1:1" },
    { id: "viewer.safe_areas", run: "safe", title: "Toggle safe areas", keys: "'", group: "Viewer", hint: "", aliases: "guides title action" },
    { id: "timeline.step_back", run: "step", arg: -1, title: "Previous frame", keys: "Left", group: "Timeline", hint: "", aliases: "frame step" },
    { id: "timeline.step_forward", run: "step", arg: 1, title: "Next frame", keys: "Right", group: "Timeline", hint: "", aliases: "frame step" },
    { id: "timeline.jump_back", run: "jump", arg: -1, title: "Back one second", keys: "Shift+Left", group: "Timeline", hint: "", aliases: "seek" },
    { id: "timeline.jump_forward", run: "jump", arg: 1, title: "Forward one second", keys: "Shift+Right", group: "Timeline", hint: "", aliases: "seek" },
    { id: "timeline.prev_mark", run: "markPrev", title: "Previous finding", keys: "N", group: "Timeline", hint: "", aliases: "anomaly mark worry" },
    { id: "timeline.next_mark", run: "markNext", title: "Next finding", keys: "Shift+N", group: "Timeline", hint: "", aliases: "anomaly mark worry" },
    { id: "timeline.set_in", run: "setIn", title: "Set preview in-point", keys: "I", group: "Timeline", hint: "", aliases: "range start selection" },
    { id: "timeline.set_out", run: "setOut", title: "Set preview out-point", keys: "O", group: "Timeline", hint: "", aliases: "range end selection" },
    { id: "timeline.clear_range", run: "clearRange", title: "Clear preview range", keys: "Esc", group: "Timeline", hint: "", aliases: "selection deselect" },
    { id: "timeline.graphs", run: "lanes", title: "Toggle measurement lanes", keys: "G", group: "Timeline", hint: "", aliases: "graphs signal lanes" },
    { id: "chain.add", run: "addModule", title: "Add module…", keys: "A", group: "Chain", hint: "Each module states what it can damage.", aliases: "operation step deinterlace stabilize denoise finish" },
    { id: "chain.move_up", run: "moveStep", arg: -1, title: "Move step earlier", keys: "Alt+Up", group: "Chain", hint: "Order is part of the recipe: reordering is a different recipe.", aliases: "reorder order" },
    { id: "chain.move_down", run: "moveStep", arg: 1, title: "Move step later", keys: "Alt+Down", group: "Chain", hint: "", aliases: "reorder order" },
    { id: "chain.remove", run: "removeStep", title: "Remove selected step", keys: "Delete", group: "Chain", hint: "The source file is untouched; only the chain changes.", aliases: "delete operation" },
    { id: "chain.reset", run: "resetStep", title: "Reset step to defaults", keys: "Alt+Backspace", group: "Chain", hint: "", aliases: "defaults restore" },
    { id: "preview.range_suggestion", run: "suggestRange", title: "Suggest a preview range", keys: "Shift+I", group: "Deliver", hint: "Centres a segment on the worst measured moment, not the first seconds.", aliases: "worst frames anomalies segment" },
    { id: "preview.render", run: "renderPreview", title: "Render preview segment", keys: "Ctrl+Enter", group: "Deliver", hint: "Renders the chain over the selected segment, with its proof.", aliases: "segment preview render test" },
    { id: "batch.open", run: "batch", title: "Batch render…", keys: "Ctrl+B", group: "Deliver", hint: "Render several sources in order, each with its own proof; a stopped batch resumes at the file that did not finish.", aliases: "queue many sources resume all" },
    { id: "export.open", run: "tab", arg: "export", title: "Open export panel", keys: "Ctrl+E", group: "Deliver", hint: "", aliases: "deliver render master" },
    { id: "export.run", run: "renderExport", title: "Render export", keys: "Ctrl+Shift+Enter", group: "Deliver", hint: "Writes the output plus its recipe and export log beside it.", aliases: "deliver render master" },
    { id: "proof.verify", run: "verify", title: "Verify output", keys: "Ctrl+Shift+V", group: "Deliver", hint: "", aliases: "check hash prove sidecar" },
    { id: "help.palette", run: "palette", title: "Command palette", keys: "Ctrl+K", group: "Help", hint: "", aliases: "search commands find run" },
    { id: "help.shortcuts", run: "shortcuts", title: "Keyboard shortcuts", keys: "?", group: "Help", hint: "", aliases: "keys help sheet" },
    { id: "help.about", run: "about", title: "About ReelDeck", keys: "", group: "Help", hint: "", aliases: "version build engine tools" }
  ];

  var STAGES = [
    { id: "ingest", label: "Ingest", purpose: "Add sources, compute fixity, and record what the file is.", requires: [] },
    { id: "inspect", label: "Inspect", purpose: "Measure the signal. Findings are suggestions; you confirm them.", requires: ["a queued source"] },
    { id: "decide", label: "Decide", purpose: "Build the ordered chain. Every step shows its damage note and its exact filter.", requires: ["an inspection report"] },
    { id: "preview", label: "Preview", purpose: "Judge a segment — chosen where the footage is worst — before committing.", requires: ["a chain with at least one operation"] },
    { id: "export", label: "Export", purpose: "Choose one intent. Every export writes its recipe and export log beside it.", requires: ["a validated chain", "a preview, or a recorded override"] },
    { id: "prove", label: "Prove", purpose: "Check the output against its proof. A check without evidence is a skip.", requires: ["a finished output"] }
  ];

  function activeClip(state) {
    return clips.find(function (c) { return c.id === state.selectedId; }) || clips[0];
  }

  global.RD.data = {
    HZ: HZ,
    project: {
      name: "Ashland Oral Histories",
      id: "ashland-oral-histories",
      sources: clips.length,
      outputCount: 0,
      defaultDestination: "D:/Archive/Ashland/04_EVance.restored.mkv",
      freeBytes: 412 * 1024 * 1024 * 1024
    },
    clips: clips,
    activeClip: activeClip,
    modules: MODULES,
    moduleOrder: MODULE_ORDER,
    presets: PRESETS,
    commands: COMMANDS,
    stages: STAGES,
    initialChain: initialChain,
    rate: RATE,
    buildSeries: buildSeries,
    engine: {
      ffmpeg: "8.0.1",
      ffmpegPath: "C:\\Tools\\ffmpeg\\bin\\ffmpeg.exe",
      vapoursynth: null,
      schema: 1
    }
  };
})(window);
