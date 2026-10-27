/* ReelDeck prototype — icons, small builders, and the scope/lane drawing.
   SVG paths are original, drawn for this interface. */
(function (global) {
  "use strict";

  var RD = global.RD;

  var PATHS = {
    "folder": '<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4l2 2.2h7A1.5 1.5 0 0 1 19 8.7v9.8A1.5 1.5 0 0 1 17.5 20h-13A1.5 1.5 0 0 1 3 18.5z"/>',
    "chevron-down": '<path d="M6 9l6 6 6-6"/>',
    "chevron-right": '<path d="M9 6l6 6-6 6"/>',
    "chevron-up": '<path d="M6 15l6-6 6 6"/>',
    "flask": '<path d="M9 3h6M10 3v5.2L4.8 18a2 2 0 0 0 1.8 3h10.8a2 2 0 0 0 1.8-3L14 8.2V3"/><path d="M7.4 14h9.2"/>',
    "search": '<circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.4 15.4L21 21"/>',
    "help": '<circle cx="12" cy="12" r="9"/><path d="M9.6 9.2a2.5 2.5 0 1 1 3.4 2.4c-.7.3-1 .9-1 1.6v.4"/><circle cx="12" cy="17.2" r=".9" fill="currentColor" stroke="none"/>',
    "settings": '<circle cx="12" cy="12" r="3.1"/><path d="M12 2.8v2.6M12 18.6v2.6M4.5 4.5l1.9 1.9M17.6 17.6l1.9 1.9M2.8 12h2.6M18.6 12h2.6M4.5 19.5l1.9-1.9M17.6 6.4l1.9-1.9"/>',
    "panel-left": '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/>',
    "plus": '<path d="M12 5v14M5 12h14"/>',
    "close": '<path d="M6 6l12 12M18 6L6 18"/>',
    "compare": '<rect x="3" y="5" width="18" height="14" rx="1.6"/><path d="M12 5v14"/><path d="M6.5 9l2 2-2 2" fill="none"/>',
    "zoom": '<circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.4 15.4L21 21M8 10.5h5M10.5 8v5"/>',
    "grid": '<rect x="3.5" y="5.5" width="17" height="13" rx="1"/><path d="M8.2 5.5v13M15.8 5.5v13M3.5 9.2h17M3.5 14.8h17"/>',
    "skip-back": '<path d="M18 6L9 12l9 6z" fill="currentColor" stroke="none"/><path d="M6 6v12"/>',
    "skip-fwd": '<path d="M6 6l9 6-9 6z" fill="currentColor" stroke="none"/><path d="M18 6v12"/>',
    "step-back": '<path d="M14.5 6.5L9 12l5.5 5.5z" fill="currentColor" stroke="none"/><path d="M7 6v12"/>',
    "step-fwd": '<path d="M9.5 6.5L15 12l-5.5 5.5z" fill="currentColor" stroke="none"/><path d="M17 6v12"/>',
    "target": '<circle cx="12" cy="12" r="7.5"/><circle cx="12" cy="12" r="2.6"/><path d="M12 2.5v3.4M12 18.1v3.4M2.5 12h3.4M18.1 12h3.4"/>',
    "wave": '<path d="M3 13.5h3l2.2-6 3 12 2.6-9 2.2 3h5"/>',
    "warning": '<path d="M12 3.5L21.2 20H2.8z"/><path d="M12 9.4v4.4"/><circle cx="12" cy="16.9" r=".9" fill="currentColor" stroke="none"/>',
    "check": '<path d="M4.5 12.5l5 5L20 6.5"/>',
    "shield": '<path d="M12 3l7.5 3v5.5c0 4.4-3.1 8.2-7.5 9.5-4.4-1.3-7.5-5.1-7.5-9.5V6z"/><path d="M8.7 12.2l2.4 2.4 4.2-4.6"/>',
    "lock": '<rect x="4.5" y="10.5" width="15" height="9.5" rx="1.6"/><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5"/>',
    "file": '<path d="M13.5 3H6.5A1.5 1.5 0 0 0 5 4.5v15A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5V8.5z"/><path d="M13.5 3v5.5H18"/>',
    "hash": '<path d="M9.5 3.5L7.5 20.5M16.5 3.5l-2 17M4 8.5h16M3.5 15.5h16"/>',
    "copy": '<rect x="8.5" y="8.5" width="11.5" height="11.5" rx="1.6"/><path d="M15.5 8.5V5A1.5 1.5 0 0 0 14 3.5H5A1.5 1.5 0 0 0 3.5 5v9A1.5 1.5 0 0 0 5 15.5h3.5"/>',
    "arrow-up": '<path d="M12 19V5M6 11l6-6 6 6"/>',
    "arrow-down": '<path d="M12 5v14M6 13l6 6 6-6"/>',
    "trash": '<path d="M4.5 7h15M9.5 7V4.8h5V7M6.5 7l1 13h9l1-13"/><path d="M10.5 10.5v6M13.5 10.5v6"/>',
    "reset": '<path d="M4.5 10.5A7.8 7.8 0 1 1 6 16.4"/><path d="M4 4.5v6h6"/>',
    "info": '<circle cx="12" cy="12" r="9"/><path d="M12 10.6V17"/><circle cx="12" cy="7.6" r=".9" fill="currentColor" stroke="none"/>',
    "spark": '<path d="M12 3l1.9 5.6 5.6 1.9-5.6 1.9L12 18l-1.9-5.6L4.5 10.5l5.6-1.9z"/>',
    "layers": '<path d="M12 3.5l8.5 4.5L12 12.5 3.5 8z"/><path d="M3.5 12.5L12 17l8.5-4.5M3.5 16.5L12 21l8.5-4.5"/>',
    "clock": '<circle cx="12" cy="12" r="9"/><path d="M12 7v5.4l3.4 2"/>',
    "play": '<path d="M7.5 4.8l12 7.2-12 7.2z" fill="currentColor" stroke="none"/>',
    "pause": '<path d="M8 5v14M16 5v14" stroke-width="2.6"/>',
    "film": '<rect x="3" y="4.5" width="18" height="15" rx="1.6"/><path d="M7.5 4.5v15M16.5 4.5v15M3 9.5h4.5M3 14.5h4.5M16.5 9.5H21M16.5 14.5H21"/>',
    "sliders": '<path d="M4 7h6M14 7h6M4 12h10M18 12h2M4 17h3M11 17h9"/><circle cx="12" cy="7" r="1.7"/><circle cx="16" cy="12" r="1.7"/><circle cx="9" cy="17" r="1.7"/>',
    "external": '<path d="M14 4h6v6M20 4l-8 8"/><path d="M18 14v5.5A1.5 1.5 0 0 1 16.5 21h-11A1.5 1.5 0 0 1 4 19.5v-11A1.5 1.5 0 0 1 5.5 7H11"/>',
    "link": '<path d="M9.5 14.5l5-5"/><path d="M11 7.5l1.6-1.6a3.6 3.6 0 0 1 5.1 5.1L16 12.6"/><path d="M13 16.5l-1.6 1.6a3.6 3.6 0 0 1-5.1-5.1L8 11.4"/>'
  };

  function icon(name, cls) {
    var path = PATHS[name] || PATHS.info;
    return '<svg class="' + (cls || "") + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" ' +
      'stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">' +
      path + "</svg>";
  }

  function esc(value) {
    return String(value == null ? "" : value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function pill(text, state, title) {
    return '<span class="pill" data-state="' + state + '"' +
      (title ? ' title="' + esc(title) + '"' : "") + ">" + esc(text) + "</span>";
  }

  function checkChip(level) {
    var words = { pass: "PASS", warn: "CHECK", blocked: "FAIL", unknown: "SKIP" };
    return '<span class="check-chip" data-level="' + level + '">' + words[level] + "</span>";
  }

  function hydrateIcons(root) {
    (root || document).querySelectorAll("[data-icon]").forEach(function (el) {
      var name = el.getAttribute("data-icon");
      el.innerHTML = icon(name);
      el.removeAttribute("data-icon");
      el.style.display = "inline-flex";
    });
  }

  // ------------------------------------------------------------------ scopes
  function setupCanvas(canvas) {
    var dpr = global.devicePixelRatio || 1;
    var rect = canvas.getBoundingClientRect();
    var w = Math.max(1, Math.round(rect.width || canvas.clientWidth || 320));
    var h = Math.max(1, Math.round(rect.height || canvas.clientHeight || 180));
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    var ctx = canvas.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return { ctx: ctx, w: w, h: h, dpr: dpr };
  }

  /** One measurement lane: a peak-preserving min/max column chart. */
  function drawLane(canvas, lane, opts) {
    opts = opts || {};
    var c = setupCanvas(canvas);
    var ctx = c.ctx, w = c.w, h = c.h;
    var css = getComputedStyle(document.documentElement);
    var line = css.getPropertyValue("--scope-line").trim() || "#9fb4bd";
    var fill = css.getPropertyValue("--scope-fill").trim() || "rgba(159,180,189,.16)";
    var grid = css.getPropertyValue("--scope-grid").trim() || "rgba(255,255,255,.06)";

    ctx.clearRect(0, 0, w, h);

    // Ground grid: 4 rows.
    ctx.strokeStyle = grid;
    ctx.lineWidth = 1;
    for (var r = 1; r < 4; r++) {
      var y = Math.round((h * r) / 4) + 0.5;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }

    var domain = lane.domain || (function () {
      var min = Infinity, max = -Infinity;
      lane.values.forEach(function (v) { if (v < min) min = v; if (v > max) max = v; });
      if (!isFinite(min)) return [0, 1];
      if (max - min < 1e-9) return [min - 1, max + 1];
      return [min, max];
    })();
    var lo = domain[0], hi = domain[1];

    function yFor(v) {
      var t = (v - lo) / (hi - lo);
      t = Math.max(0, Math.min(1, t));
      return h - 3 - t * (h - 6);
    }

    var count = Math.max(1, Math.floor(w / 2));
    var data = RD.model.buckets(lane, count);
    var colW = w / count;

    // Selection shading.
    if (opts.range && opts.duration) {
      var x0 = (opts.range.in / opts.duration) * w;
      var x1 = (opts.range.out / opts.duration) * w;
      ctx.fillStyle = "rgba(127,168,212,0.10)";
      ctx.fillRect(x0, 0, Math.max(1, x1 - x0), h);
    }

    // Area + per-column min/max bars preserve peaks.
    ctx.fillStyle = fill;
    for (var i = 0; i < data.length; i++) {
      var top = yFor(data[i].max);
      var bottom = yFor(data[i].min);
      ctx.fillRect(i * colW, top, Math.max(1, colW - 0.6), Math.max(1, bottom - top));
    }
    ctx.strokeStyle = line;
    ctx.globalAlpha = 0.85;
    ctx.lineWidth = 1;
    for (var j = 0; j < data.length; j++) {
      var yt = yFor(data[j].max);
      var yb = yFor(data[j].min);
      ctx.beginPath();
      ctx.moveTo(j * colW + colW / 2, yt);
      ctx.lineTo(j * colW + colW / 2, Math.max(yt + 1, yb));
      ctx.stroke();
    }
    ctx.globalAlpha = 1;

    // Threshold rule, labelled by CSS (.threshold element) rather than here.
    if (lane.threshold != null && opts.thresholdY != null) {
      ctx.strokeStyle = "rgba(216,171,85,0.55)";
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(0, opts.thresholdY + 0.5);
      ctx.lineTo(w, opts.thresholdY + 0.5);
      ctx.stroke();
      ctx.setLineDash([]);
    }
  }

  /** Waveform: luma distribution, as many thin vertical traces. */
  function drawWaveform(canvas, lane) {
    var c = setupCanvas(canvas);
    var ctx = c.ctx, w = c.w, h = c.h;
    ctx.clearRect(0, 0, w, h);
    var grid = getComputedStyle(document.documentElement).getPropertyValue("--scope-grid").trim();
    ctx.strokeStyle = grid || "rgba(255,255,255,.06)";
    for (var i = 1; i < 5; i++) {
      var y = Math.round((h * i) / 5) + 0.5;
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke();
    }
    var values = lane.values;
    var cols = Math.max(1, Math.floor(w / 3));
    var stride = Math.max(1, Math.floor(values.length / cols));
    ctx.fillStyle = "rgba(190,214,222,0.35)";
    for (var x = 0; x < cols; x++) {
      var v = values[x * stride];
      if (v == null) continue;
      var t = (v - 16) / (235 - 16);
      var yv = h - 2 - t * (h - 4);
      // A little vertical spread reads as a trace, not a chart bar.
      for (var k = -2; k <= 2; k++) {
        ctx.globalAlpha = 0.28 - Math.abs(k) * 0.05;
        ctx.fillRect(x * 3, yv + k, 2, 1);
      }
    }
    ctx.globalAlpha = 1;
  }

  function drawHistogram(canvas, lane) {
    var c = setupCanvas(canvas);
    var ctx = c.ctx, w = c.w, h = c.h;
    ctx.clearRect(0, 0, w, h);
    var bins = new Array(48).fill(0);
    var lo = lane.domain ? lane.domain[0] : 16;
    var hi = lane.domain ? lane.domain[1] : 235;
    lane.values.forEach(function (v) {
      var t = Math.max(0, Math.min(0.999, (v - lo) / (hi - lo)));
      bins[Math.floor(t * bins.length)] += 1;
    });
    var max = Math.max.apply(null, bins) || 1;
    var bw = w / bins.length;
    for (var i = 0; i < bins.length; i++) {
      var bh = (bins[i] / max) * (h - 8);
      ctx.fillStyle = i > 42 ? "rgba(216,171,85,0.55)" : "rgba(190,214,222,0.42)";
      ctx.fillRect(i * bw + 0.5, h - 4 - bh, Math.max(1, bw - 1), bh);
    }
    ctx.strokeStyle = "rgba(255,255,255,0.12)";
    ctx.beginPath(); ctx.moveTo(0, h - 3.5); ctx.lineTo(w, h - 3.5); ctx.stroke();
  }

  function drawVectorscope(canvas, lane) {
    var c = setupCanvas(canvas);
    var ctx = c.ctx, w = c.w, h = c.h;
    ctx.clearRect(0, 0, w, h);
    var cx = w / 2, cy = h / 2, r = Math.min(w, h) * 0.42;
    ctx.strokeStyle = "rgba(255,255,255,0.10)";
    ctx.beginPath(); ctx.arc(cx, cy, r, 0, Math.PI * 2); ctx.stroke();
    ctx.beginPath(); ctx.arc(cx, cy, r * 0.5, 0, Math.PI * 2); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(cx - r, cy); ctx.lineTo(cx + r, cy); ctx.moveTo(cx, cy - r); ctx.lineTo(cx, cy + r); ctx.stroke();
    var targets = [
      { angle: -0.5, color: "rgba(217,123,109,0.55)" },
      { angle: 2.1, color: "rgba(116,185,141,0.55)" },
      { angle: 4.2, color: "rgba(127,168,212,0.55)" }
    ];
    targets.forEach(function (t) {
      var x = cx + Math.cos(t.angle) * r * 0.82;
      var y = cy + Math.sin(t.angle) * r * 0.82;
      ctx.fillStyle = t.color;
      ctx.beginPath(); ctx.arc(x, y, 2, 0, Math.PI * 2); ctx.fill();
    });
    // A soft chroma cloud, seeded by saturation values so it is stable.
    var sats = lane.values;
    var stride = Math.max(1, Math.floor(sats.length / 420));
    for (var i = 0; i < sats.length; i += stride) {
      var t = sats[i] / 32;
      var ang = (i * 0.618) % (Math.PI * 2);
      var rad = Math.min(r * 0.95, t * r);
      var px = cx + Math.cos(ang) * rad * (0.85 + 0.15 * Math.sin(i));
      var py = cy + Math.sin(ang) * rad * (0.85 + 0.15 * Math.cos(i));
      ctx.fillStyle = "rgba(190,214,222,0.12)";
      ctx.fillRect(px, py, 1.6, 1.6);
    }
  }

  /** Timeline ruler ticks, step chosen so labels never collide. */
  function rulerTicks(duration, rate, widthPx) {
    var steps = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800];
    var target = Math.max(6, Math.floor(widthPx / 92));
    var step = steps[steps.length - 1];
    for (var i = 0; i < steps.length; i++) {
      if (duration / steps[i] <= target) { step = steps[i]; break; }
    }
    var out = [];
    for (var t = 0; t <= duration; t += step) {
      out.push({ secs: t, major: true });
      if (step >= 60) out.push({ secs: t + step / 2, major: false });
    }
    return { ticks: out, step: step };
  }

  RD.ui = {
    icon: icon,
    esc: esc,
    pill: pill,
    checkChip: checkChip,
    hydrateIcons: hydrateIcons,
    drawLane: drawLane,
    drawWaveform: drawWaveform,
    drawHistogram: drawHistogram,
    drawVectorscope: drawVectorscope,
    rulerTicks: rulerTicks
  };
})(window);
