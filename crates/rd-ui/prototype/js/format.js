/* ReelDeck prototype — display formatting.
   Mirrors crates/rd-ui/src/format.rs: one place decides how a time, a
   duration, a rate or a byte count is written. Unknown values are "—". */
(function (global) {
  "use strict";

  function pad(n, width) {
    return String(n).padStart(width || 2, "0");
  }

  function parseRate(text) {
    if (!text) return null;
    var parts = String(text).trim().split("/");
    var num = Number(parts[0]);
    var den = parts.length > 1 ? Number(parts[1]) : 1;
    if (!isFinite(num) || !isFinite(den) || num <= 0 || den <= 0) return null;
    return { num: num, den: den };
  }

  function fps(rate) {
    return rate && rate.den ? rate.num / rate.den : 25;
  }

  function rateText(rate) {
    var v = fps(rate);
    if (Math.abs(v - Math.round(v)) < 1e-9) return String(Math.round(v));
    return String(Number(v.toFixed(3)));
  }

  /** Non-drop timecode: HH:MM:SS:FF at the real frame rate. */
  function timecode(frame, rate) {
    var f = fps(rate);
    if (!isFinite(f) || f <= 0 || !isFinite(frame) || frame < 0) return "—";
    var nominal = Math.max(1, Math.round(f));
    var totalSecs = Math.floor(frame / f);
    var hh = Math.floor(totalSecs / 3600);
    var mm = Math.floor((totalSecs % 3600) / 60);
    var ss = totalSecs % 60;
    var ff = Math.round(frame) % nominal;
    return pad(hh) + ":" + pad(mm) + ":" + pad(ss) + ":" + pad(ff);
  }

  function timecodeAt(secs, rate) {
    if (!isFinite(secs) || secs < 0) return "—";
    return timecode(Math.round(secs * fps(rate)), rate);
  }

  function clockTime(secs) {
    if (!isFinite(secs) || secs < 0) return "—";
    var hh = Math.floor(secs / 3600);
    var mm = Math.floor((secs % 3600) / 60);
    var ss = secs % 60;
    return pad(hh) + ":" + pad(mm) + ":" + (ss < 10 ? "0" : "") + ss.toFixed(3);
  }

  function duration(secs) {
    if (!isFinite(secs) || secs < 0) return "—";
    if (secs < 1) return secs.toFixed(1) + "s";
    var whole = Math.round(secs);
    var h = Math.floor(whole / 3600);
    var m = Math.floor((whole % 3600) / 60);
    var s = whole % 60;
    if (h === 0 && m === 0) return s + "s";
    if (h === 0) return m + "m " + pad(s) + "s";
    return h + "h " + pad(m) + "m";
  }

  /** Short duration for marks: 6:12.4 */
  function durationShort(secs) {
    if (!isFinite(secs) || secs < 0) return "—";
    var m = Math.floor(secs / 60);
    var s = secs % 60;
    return m + ":" + (s < 10 ? "0" : "") + s.toFixed(1);
  }

  function bytes(bytes) {
    if (!isFinite(bytes) || bytes < 0) return "—";
    var units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    var value = bytes;
    var unit = 0;
    while (value >= 1024 && unit + 1 < units.length) {
      value /= 1024;
      unit += 1;
    }
    return unit === 0 ? bytes + " B" : value.toFixed(1) + " " + units[unit];
  }

  function percent(fraction, decimals) {
    if (!isFinite(fraction)) return "—";
    return (fraction * 100).toFixed(decimals == null ? 1 : decimals) + "%";
  }

  function rate(value, unit, decimals) {
    if (!isFinite(value)) return "—";
    return value.toFixed(decimals == null ? 1 : decimals) + (unit || "");
  }

  function ordinal(n) {
    var s = ["th", "st", "nd", "rd"];
    var v = n % 100;
    return n + (s[(v - 20) % 10] || s[v] || s[0]);
  }

  function shortHash(hash) {
    if (!hash) return "—";
    return hash.slice(0, 10) + "…" + hash.slice(-6);
  }

  global.RD = global.RD || {};
  global.RD.format = {
    pad: pad,
    parseRate: parseRate,
    fps: fps,
    rateText: rateText,
    timecode: timecode,
    timecodeAt: timecodeAt,
    clockTime: clockTime,
    duration: duration,
    durationShort: durationShort,
    bytes: bytes,
    percent: percent,
    rate: rate,
    ordinal: ordinal,
    shortHash: shortHash
  };
})(window);
