//! ReelDeck frame analysis.
//!
//! Turns the textual output of the engine's inspection tools — `ffprobe`
//! JSON, `signalstats` frame metadata, and the `blackdetect` /
//! `freezedetect` / `idet` log formats — into a typed model the UI and
//! CLI can present. Parsing is pure and fully unit-tested; process
//! execution in this crate is limited to [`probe`] and [`analyze`].
//! [`probe`] asks MediaKit (via the `rd-mediakit` crate) for
//! header-level facts first and falls back to shelling out to
//! `ffprobe`; [`analyze`] shells out to `ffprobe` / `ffmpeg` for its
//! measurement passes. Both report a typed
//! [`AnalyzeError::ToolUnavailable`] when a tool is missing, rather
//! than panicking or inventing data.
//!
//! Analysis findings are *observations*, never verdicts: an interlacing
//! suggestion from these numbers is confirmed by the operator, not
//! silently acted on.

#![forbid(unsafe_code)]

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod mediakit;

#[derive(Debug, Error)]
pub enum AnalyzeError {
    #[error("required tool '{0}' is not available on this system; install FFmpeg (which provides ffprobe/ffmpeg) to enable inspection")]
    ToolUnavailable(String),
    #[error("could not parse {what}: {detail}")]
    Parse { what: &'static str, detail: String },
    #[error("running {tool} failed: {detail}")]
    ToolFailed { tool: &'static str, detail: String },
    #[error("invalid sample range: {0}")]
    InvalidRange(String),
}

// ---------------------------------------------------------------------------
// Probe model (from ffprobe JSON)
// ---------------------------------------------------------------------------

/// One stream in a probed file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamInfo {
    pub index: u32,
    /// `video`, `audio`, `subtitle`, `data`, ...
    pub codec_type: String,
    pub codec_name: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// Displayed frame rate string from the probe, e.g. `30000/1001`.
    #[serde(default)]
    pub avg_frame_rate: Option<String>,
    /// Field order flag as stored in the stream, e.g. `tt`, `bb`,
    /// `progressive`, when present.
    #[serde(default)]
    pub field_order: Option<String>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub channels: Option<u32>,
}

/// Container-level facts about a probed file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub format_name: String,
    pub duration_secs: f64,
    pub size_bytes: u64,
    #[serde(default)]
    pub bit_rate: Option<u64>,
    pub streams: Vec<StreamInfo>,
}

impl ProbeInfo {
    pub fn video_streams(&self) -> impl Iterator<Item = &StreamInfo> {
        self.streams.iter().filter(|s| s.codec_type == "video")
    }

    pub fn first_video(&self) -> Option<&StreamInfo> {
        self.video_streams().next()
    }

    /// The first video stream's frame count, from the container
    /// duration and the stream's average frame rate. `None` when either
    /// is missing or unusable (a MediaKit-sourced probe carries no
    /// frame rate; ffprobe reports `0/0` for some streams).
    pub fn estimated_video_frames(&self) -> Option<u64> {
        let rate = self.first_video()?.avg_frame_rate.as_deref()?;
        let (num, den) = match rate.split_once('/') {
            Some((n, d)) => (n.trim().parse::<f64>().ok()?, d.trim().parse::<f64>().ok()?),
            None => (rate.trim().parse::<f64>().ok()?, 1.0),
        };
        if !(num > 0.0 && den > 0.0 && self.duration_secs.is_finite() && self.duration_secs > 0.0) {
            return None;
        }
        Some((self.duration_secs * num / den).round() as u64)
    }
}

// Serde shapes for ffprobe's JSON output (only the fields we consume).
#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: FfprobeFormat,
    #[serde(default)]
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    format_name: String,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: u32,
    codec_type: String,
    codec_name: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    avg_frame_rate: Option<String>,
    #[serde(default)]
    field_order: Option<String>,
    #[serde(default)]
    sample_rate: Option<String>,
    #[serde(default)]
    channels: Option<u32>,
}

fn parse_f64(s: &Option<String>, what: &'static str) -> Result<f64, AnalyzeError> {
    match s {
        Some(v) => v.parse::<f64>().map_err(|e| AnalyzeError::Parse {
            what,
            detail: e.to_string(),
        }),
        None => Ok(0.0),
    }
}

fn parse_u64(s: &Option<String>, what: &'static str) -> Result<u64, AnalyzeError> {
    match s {
        Some(v) => v.parse::<u64>().map_err(|e| AnalyzeError::Parse {
            what,
            detail: e.to_string(),
        }),
        None => Ok(0),
    }
}

/// Parse `ffprobe -show_format -show_streams -of json` output.
pub fn parse_ffprobe_json(text: &str) -> Result<ProbeInfo, AnalyzeError> {
    let raw: FfprobeOutput = serde_json::from_str(text).map_err(|e| AnalyzeError::Parse {
        what: "ffprobe JSON",
        detail: e.to_string(),
    })?;
    let mut streams = Vec::with_capacity(raw.streams.len());
    for s in raw.streams {
        streams.push(StreamInfo {
            index: s.index,
            codec_type: s.codec_type,
            codec_name: s.codec_name,
            width: s.width,
            height: s.height,
            avg_frame_rate: s.avg_frame_rate,
            field_order: s.field_order,
            sample_rate: s.sample_rate.as_deref().and_then(|v| v.parse::<u32>().ok()),
            channels: s.channels,
        });
    }
    Ok(ProbeInfo {
        format_name: raw.format.format_name,
        duration_secs: parse_f64(&raw.format.duration, "ffprobe duration")?,
        size_bytes: parse_u64(&raw.format.size, "ffprobe size")?,
        bit_rate: match &raw.format.bit_rate {
            Some(v) => v.parse::<u64>().ok(),
            None => None,
        },
        streams,
    })
}

// ---------------------------------------------------------------------------
// Per-frame signal statistics (from the signalstats filter)
// ---------------------------------------------------------------------------

/// Signal statistics for one frame, as reported by the `signalstats`
/// filter. Values are in the filter's native 0..=255 luma/chroma scale
/// for 8-bit sources; `tout` is the temporal-outlier ratio and `brng`
/// the fraction of pixels outside broadcast range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameStats {
    pub frame: u64,
    pub pts_time: f64,
    pub y_min: f64,
    pub y_avg: f64,
    pub y_max: f64,
    pub u_avg: f64,
    pub v_avg: f64,
    pub sat_avg: f64,
    pub tout: f64,
    pub brng: f64,
}

impl FrameStats {
    fn from_tags(frame: u64, pts_time: f64, tags: &[(String, String)]) -> Option<Self> {
        let get = |key: &str| -> Option<f64> {
            tags.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.parse::<f64>().ok())
        };
        Some(FrameStats {
            frame,
            pts_time,
            y_min: get("lavfi.signalstats.YMIN")?,
            y_avg: get("lavfi.signalstats.YAVG")?,
            y_max: get("lavfi.signalstats.YMAX")?,
            u_avg: get("lavfi.signalstats.UAVG")?,
            v_avg: get("lavfi.signalstats.VAVG")?,
            sat_avg: get("lavfi.signalstats.SATAVG")?,
            tout: get("lavfi.signalstats.TOUT")?,
            brng: get("lavfi.signalstats.BRNG")?,
        })
    }
}

/// Parse frame metadata in `ffprobe -show_frames` text form: `[FRAME]`
/// blocks whose `TAG:lavfi.signalstats.*` lines carry the metrics.
/// Frames missing any required metric are skipped, not guessed at.
pub fn parse_signalstats(text: &str) -> Vec<FrameStats> {
    let mut out = Vec::new();
    let mut in_frame = false;
    let mut frame_no: u64 = 0;
    let mut pts_time: f64 = 0.0;
    let mut tags: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line == "[FRAME]" {
            in_frame = true;
            tags.clear();
            pts_time = 0.0;
        } else if line == "[/FRAME]" {
            if in_frame {
                if let Some(stats) = FrameStats::from_tags(frame_no, pts_time, &tags) {
                    out.push(stats);
                    frame_no += 1;
                }
            }
            in_frame = false;
        } else if in_frame {
            if let Some(v) = line.strip_prefix("pts_time=") {
                pts_time = v.parse().unwrap_or(0.0);
            } else if let Some(tag) = line.strip_prefix("TAG:") {
                if let Some((k, v)) = tag.split_once('=') {
                    tags.push((k.to_string(), v.to_string()));
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Anomalies
// ---------------------------------------------------------------------------

/// Something in the signal worth an operator's eyes. Anomalies flag
/// *where to look*; they never auto-trigger a restoration decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Anomaly {
    /// A span of (almost) completely black frames.
    BlackSpan { start_secs: f64, end_secs: f64 },
    /// A span where the picture did not change.
    FreezeSpan { start_secs: f64, end_secs: f64 },
    /// A single frame whose temporal-outlier ratio spiked (dropouts and
    /// head clogs look like this).
    TemporalOutlier {
        frame: u64,
        pts_time: f64,
        tout: f64,
    },
}

impl Anomaly {
    /// Position on the source timeline used to order anomalies:
    /// the span start for span anomalies, the frame's own timestamp
    /// for a temporal outlier.
    pub fn sort_key(&self) -> f64 {
        match *self {
            Anomaly::BlackSpan { start_secs, .. } | Anomaly::FreezeSpan { start_secs, .. } => {
                start_secs
            }
            Anomaly::TemporalOutlier { pts_time, .. } => pts_time,
        }
    }
}

/// Order anomalies by [`Anomaly::sort_key`]; the sort is stable, so
/// anomalies at the same position keep their merge order.
pub fn sort_anomalies(anomalies: &mut [Anomaly]) {
    anomalies.sort_by(|a, b| {
        a.sort_key()
            .partial_cmp(&b.sort_key())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Frames whose temporal-outlier ratio strictly exceeds `threshold`
/// (a ratio, typically 0.05..=0.20 for tape captures).
pub fn temporal_outliers(stats: &[FrameStats], threshold: f64) -> Vec<Anomaly> {
    stats
        .iter()
        .filter(|s| s.tout > threshold)
        .map(|s| Anomaly::TemporalOutlier {
            frame: s.frame,
            pts_time: s.pts_time,
            tout: s.tout,
        })
        .collect()
}

/// Parse `blackdetect` log lines of the form
/// `[blackdetect @ ...] black_start:1.5 black_end:4 black_duration:2.5`.
pub fn parse_blackdetect_log(text: &str) -> Vec<Anomaly> {
    let mut out = Vec::new();
    for line in text.lines() {
        let (mut start, mut end) = (None, None);
        for token in line.split_whitespace() {
            if let Some(v) = token.strip_prefix("black_start:") {
                start = v.parse::<f64>().ok();
            } else if let Some(v) = token.strip_prefix("black_end:") {
                end = v.parse::<f64>().ok();
            }
        }
        if let (Some(start_secs), Some(end_secs)) = (start, end) {
            out.push(Anomaly::BlackSpan {
                start_secs,
                end_secs,
            });
        }
    }
    out
}

/// Parse `freezedetect` metadata lines of the form
/// `lavfi.freezedetect.freeze_start: 12.25` / `freeze_end: 15.5`,
/// pairing each start with the next end.
pub fn parse_freezedetect_log(text: &str) -> Vec<Anomaly> {
    let mut out = Vec::new();
    let mut pending_start: Option<f64> = None;
    for line in text.lines() {
        for (marker, is_start) in [
            ("lavfi.freezedetect.freeze_start:", true),
            ("lavfi.freezedetect.freeze_end:", false),
        ] {
            if let Some(idx) = line.find(marker) {
                let rest = line[idx + marker.len()..].trim();
                if let Ok(v) = rest.split_whitespace().next().unwrap_or("").parse::<f64>() {
                    if is_start {
                        pending_start = Some(v);
                    } else if let Some(start_secs) = pending_start.take() {
                        out.push(Anomaly::FreezeSpan {
                            start_secs,
                            end_secs: v,
                        });
                    }
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Sampled range, signal summary, interlace suggestion, and the report
// ---------------------------------------------------------------------------

/// A sampled range of the source to analyze: starting at `start_secs`
/// and running for `duration_secs` (or to the end of the file when
/// `None`). Analyzing a range — chosen by the operator in Inspect —
/// keeps a first pass over a long capture fast while still covering
/// the segment that matters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SampleRange {
    pub start_secs: f64,
    pub duration_secs: Option<f64>,
}

impl SampleRange {
    pub fn new(start_secs: f64, duration_secs: Option<f64>) -> Self {
        SampleRange {
            start_secs,
            duration_secs,
        }
    }

    /// The whole file.
    pub fn full() -> Self {
        SampleRange::new(0.0, None)
    }

    /// Reject ranges that cannot mean anything: a negative or
    /// non-finite start, or a non-positive or non-finite duration.
    pub fn validate(&self) -> Result<(), AnalyzeError> {
        if !self.start_secs.is_finite() || self.start_secs < 0.0 {
            return Err(AnalyzeError::InvalidRange(format!(
                "start must be a finite number >= 0 seconds, got {}",
                self.start_secs
            )));
        }
        if let Some(d) = self.duration_secs {
            if !d.is_finite() || d <= 0.0 {
                return Err(AnalyzeError::InvalidRange(format!(
                    "duration must be a finite number > 0 seconds, got {d}"
                )));
            }
        }
        Ok(())
    }
}

/// Min / max / arithmetic mean of one per-frame metric.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetricSummary {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

impl MetricSummary {
    fn of(values: impl Iterator<Item = f64>) -> Option<Self> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut sum = 0.0f64;
        let mut n = 0usize;
        for v in values {
            min = min.min(v);
            max = max.max(v);
            sum += v;
            n += 1;
        }
        if n == 0 {
            None
        } else {
            Some(MetricSummary {
                min,
                max,
                mean: sum / n as f64,
            })
        }
    }
}

/// Aggregated signal statistics over the analyzed frames.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SignalSummary {
    pub y_avg: MetricSummary,
    pub sat_avg: MetricSummary,
    pub tout: MetricSummary,
    pub brng: MetricSummary,
    pub frames: usize,
}

/// Summarize a run of per-frame statistics. Returns `None` for an
/// empty slice: a summary of no frames is *absent*, never a fabricated
/// row of zeros that would read as a measurement.
pub fn summarize(stats: &[FrameStats]) -> Option<SignalSummary> {
    if stats.is_empty() {
        return None;
    }
    Some(SignalSummary {
        y_avg: MetricSummary::of(stats.iter().map(|s| s.y_avg))?,
        sat_avg: MetricSummary::of(stats.iter().map(|s| s.sat_avg))?,
        tout: MetricSummary::of(stats.iter().map(|s| s.tout))?,
        brng: MetricSummary::of(stats.iter().map(|s| s.brng))?,
        frames: stats.len(),
    })
}

/// Frame-classification counts reported by the `idet` filter, in its
/// three summary lines. `idet` reports *cumulative* totals and prints
/// the lines more than once (an all-zero set at graph start, then the
/// running totals), so the parser keeps the **last** occurrence of
/// each line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdetCounts {
    pub neither: u64,
    pub repeated_top: u64,
    pub repeated_bottom: u64,
    pub single_tff: u64,
    pub single_bff: u64,
    pub single_progressive: u64,
    pub single_undetermined: u64,
    pub multi_tff: u64,
    pub multi_bff: u64,
    pub multi_progressive: u64,
    pub multi_undetermined: u64,
}

/// What the `idet` counts *suggest* about the source's interlacing.
///
/// This is a suggestion for the operator to confirm in the viewer —
/// the counts it was derived from travel with it in the report, so
/// the basis is always visible. The tool never silently acts on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterlaceSuggestion {
    TopFieldFirst,
    BottomFieldFirst,
    Progressive,
    Undetermined,
}

impl IdetCounts {
    /// Total frames `idet` placed in each determined category,
    /// combining its single-frame and multi-frame detection passes.
    pub fn determined(&self) -> (u64, u64, u64) {
        (
            self.single_tff + self.multi_tff,
            self.single_bff + self.multi_bff,
            self.single_progressive + self.multi_progressive,
        )
    }

    /// Suggest a classification from the determined counts. The
    /// leading category must hold at least 60% of the determined
    /// frames; anything weaker — or no determined frames at all — is
    /// honestly [`InterlaceSuggestion::Undetermined`].
    pub fn suggestion(&self) -> InterlaceSuggestion {
        let (tff, bff, prog) = self.determined();
        let total = tff + bff + prog;
        if total == 0 {
            return InterlaceSuggestion::Undetermined;
        }
        let (leading, suggestion) = [
            (tff, InterlaceSuggestion::TopFieldFirst),
            (bff, InterlaceSuggestion::BottomFieldFirst),
            (prog, InterlaceSuggestion::Progressive),
        ]
        .into_iter()
        .max_by_key(|(count, _)| *count)
        .expect("three fixed candidates");
        if (leading as f64) / (total as f64) >= 0.60 {
            suggestion
        } else {
            InterlaceSuggestion::Undetermined
        }
    }
}

/// Parse the `key: value` pairs that follow `marker` on a line, e.g.
/// `TFF: 4 BFF: 0 Progressive: 51 Undetermined: 24`.
fn parse_idet_pairs(line: &str, marker: &str, keys: &[&str]) -> Option<Vec<u64>> {
    let rest = &line[line.find(marker)? + marker.len()..];
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let mut values = Vec::with_capacity(keys.len());
    for key in keys {
        let pos = tokens.iter().position(|t| *t == format!("{key}:"))?;
        values.push(tokens.get(pos + 1)?.parse::<u64>().ok()?);
    }
    Some(values)
}

/// Parse `idet` summary lines from ffmpeg's log output into counts.
/// Unrecognized lines are ignored; a missing line leaves its counts
/// at zero. When a line appears repeatedly (cumulative re-prints),
/// the last occurrence wins.
pub fn parse_idet_log(text: &str) -> IdetCounts {
    let mut counts = IdetCounts::default();
    for line in text.lines() {
        if line.contains("Repeated Fields:") {
            if let Some(v) =
                parse_idet_pairs(line, "Repeated Fields:", &["Neither", "Top", "Bottom"])
            {
                counts.neither = v[0];
                counts.repeated_top = v[1];
                counts.repeated_bottom = v[2];
            }
        } else if line.contains("Single frame detection:") {
            if let Some(v) = parse_idet_pairs(
                line,
                "Single frame detection:",
                &["TFF", "BFF", "Progressive", "Undetermined"],
            ) {
                counts.single_tff = v[0];
                counts.single_bff = v[1];
                counts.single_progressive = v[2];
                counts.single_undetermined = v[3];
            }
        } else if line.contains("Multi frame detection:") {
            if let Some(v) = parse_idet_pairs(
                line,
                "Multi frame detection:",
                &["TFF", "BFF", "Progressive", "Undetermined"],
            ) {
                counts.multi_tff = v[0];
                counts.multi_bff = v[1];
                counts.multi_progressive = v[2];
                counts.multi_undetermined = v[3];
            }
        }
    }
    counts
}

/// One analysis pass over a sampled range of a source file: the
/// report behind `reeldeck inspect`'s text, JSON, and HTML surfaces.
/// Every number in it was measured by the tools on this run; the
/// interlace suggestion is labelled a suggestion and carries its
/// basis counts (`idet`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub source: String,
    pub range: SampleRange,
    pub probe: ProbeInfo,
    pub frames_analyzed: usize,
    /// `None` when the range produced no measurable frames — absent,
    /// never fabricated zeros.
    pub signal: Option<SignalSummary>,
    /// Sorted by source-timeline position.
    pub anomalies: Vec<Anomaly>,
    pub idet: IdetCounts,
    pub suggestion: InterlaceSuggestion,
    /// Temporal-outlier ratio above which a frame was flagged.
    pub tout_threshold: f64,
}

// ---------------------------------------------------------------------------
// Tool access
// ---------------------------------------------------------------------------

/// True when `ffprobe` answers `--version` successfully.
pub fn ffprobe_available() -> bool {
    Command::new("ffprobe")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// True when `ffmpeg` answers `-version` successfully.
pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Escape a filesystem path for use as the `movie=` source of a lavfi
/// filter graph (no surrounding quotes).
///
/// Filter-graph text is parsed in *two* passes — once as graph
/// syntax, once as filter-option values — and a path has to survive
/// both, so characters special to either pass are escaped for both:
/// `\` becomes `\\\\`, `:` becomes `\\:`, `'` becomes `\\\'`, and the
/// graph separators `, ; [ ]` get a single `\`. Everything else
/// (including spaces) passes through literally. The mapping is
/// verified against the real `ffprobe` by unit tests on the argument
/// shape and by the ignored integration test for actual files whose
/// names contain these characters.
pub fn escape_lavfi_movie_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            '\\' => out.push_str("\\\\\\\\"),
            ':' => out.push_str("\\\\:"),
            '\'' => out.push_str("\\\\\\'"),
            ',' | ';' | '[' | ']' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn secs(v: f64) -> String {
    // Rust's f64 Display never uses exponent notation, so this is
    // always a plain decimal the tools accept ("0.5", "2", "1.5").
    format!("{v}")
}

/// Probe a media file for header-level facts: container, duration,
/// bitrate, and per-stream codec / dimensions / sample-rate /
/// channel facts.
///
/// MediaKit is asked first (see [`mediakit`]): it reads the headers
/// directly, without spawning a process. When MediaKit is
/// unavailable on this platform, does not recognize the container,
/// or reports an error, the probe falls back to
/// [`probe_ffprobe`] — MediaKit failing to answer is never itself a
/// hard failure. Fields MediaKit cannot supply (frame rate, field
/// order) stay unset in a MediaKit-sourced result.
///
/// Returns [`AnalyzeError::ToolUnavailable`] when the fallback is
/// reached and ffprobe is not installed — inspection is impossible
/// without it, and this crate will not fabricate a result.
pub fn probe(path: &Path) -> Result<ProbeInfo, AnalyzeError> {
    match mediakit::probe_via_mediakit(path) {
        Ok(info) => Ok(info),
        Err(_) => probe_ffprobe(path),
    }
}

/// Probe a media file with `ffprobe` and parse the result.
///
/// This is the fallback behind [`probe`], kept public so callers can
/// ask for the ffprobe view specifically. Returns
/// [`AnalyzeError::ToolUnavailable`] when ffprobe is not installed.
pub fn probe_ffprobe(path: &Path) -> Result<ProbeInfo, AnalyzeError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|_| AnalyzeError::ToolUnavailable("ffprobe".to_string()))?;
    if !output.status.success() {
        return Err(AnalyzeError::ToolFailed {
            tool: "ffprobe",
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_ffprobe_json(&text)
}

/// Run a full analysis pass over `range` of the file at `path` and
/// build the [`AnalysisReport`]: probe data, per-frame signal
/// statistics over the range, black/freeze/temporal-outlier
/// anomalies, and an `idet`-based interlace *suggestion*.
///
/// Two tool passes do the measuring:
///
/// 1. **Frame stats** via `ffprobe`'s lavfi `movie` source with
///    `trim` applying the range. `trim` keeps source timestamps, so
///    frame times and the temporal outliers derived from them are on
///    the source timeline. The path goes through
///    [`escape_lavfi_movie_path`] — the two-pass graph/option parsing
///    makes raw paths with `\`, `'`, or `:` break or silently open
///    the wrong file.
/// 2. **Detectors + `idet`** in one `ffmpeg` pass
///    (`blackdetect,freezedetect,idet`). Input seeking (`-ss` before
///    `-i`) restarts timestamps at zero, so black/freeze span times
///    come back *range-relative* and are shifted by
///    `range.start_secs` here to join the same source timeline.
///    (`freezedetect` announces its spans in ffmpeg's log output at
///    info level, as `lavfi.freezedetect.freeze_start:` lines — the
///    existing log parser consumes exactly those.)
///
/// Any tool failure aborts with [`AnalyzeError::ToolFailed`]
/// carrying the tool's stderr: a partial or invented report is
/// worse than none.
pub fn analyze(
    path: &Path,
    range: SampleRange,
    tout_threshold: f64,
) -> Result<AnalysisReport, AnalyzeError> {
    range.validate()?;
    if !ffprobe_available() {
        return Err(AnalyzeError::ToolUnavailable("ffprobe".to_string()));
    }
    if !ffmpeg_available() {
        return Err(AnalyzeError::ToolUnavailable("ffmpeg".to_string()));
    }
    let probe = probe(path)?;

    // Pass 1: per-frame signal statistics over the range.
    let graph = {
        let mut g = format!("movie={}", escape_lavfi_movie_path(&path.to_string_lossy()));
        if let Some(d) = range.duration_secs {
            g.push_str(&format!(
                ",trim=start={}:duration={}",
                secs(range.start_secs),
                secs(d)
            ));
        } else if range.start_secs > 0.0 {
            g.push_str(&format!(",trim=start={}", secs(range.start_secs)));
        }
        // `stat=tout+brng` is load-bearing, not decoration:
        // signalstats only *computes* the temporal-outlier and
        // broadcast-range metrics when asked (its `stat` option
        // defaults to none in FFmpeg 8.x). Without the flags those
        // keys never appear in the frame metadata, and every real
        // frame would fail the parser's required-keys check.
        g.push_str(",signalstats=stat=tout+brng");
        g
    };
    let stats_output = Command::new("ffprobe")
        .args(["-v", "error", "-show_frames", "-f", "lavfi"])
        .arg(&graph)
        .output()
        .map_err(|_| AnalyzeError::ToolUnavailable("ffprobe".to_string()))?;
    if !stats_output.status.success() {
        return Err(AnalyzeError::ToolFailed {
            tool: "ffprobe",
            detail: String::from_utf8_lossy(&stats_output.stderr)
                .trim()
                .to_string(),
        });
    }
    let stats_text = String::from_utf8_lossy(&stats_output.stdout);
    let stats = parse_signalstats(&stats_text);
    let signal = summarize(&stats);
    let frames_analyzed = stats.len();

    // Pass 2: black/freeze detection and idet over the same range.
    let mut detect = Command::new("ffmpeg");
    detect
        .args(["-hide_banner", "-ss", &secs(range.start_secs), "-i"])
        .arg(path);
    if let Some(d) = range.duration_secs {
        detect.args(["-t", &secs(d)]);
    }
    detect.args([
        "-vf",
        "blackdetect=d=1.0:pix_th=0.10,freezedetect=n=-60dB:d=2,idet",
        "-an",
        "-f",
        "null",
        "-",
    ]);
    let detect_output = detect
        .output()
        .map_err(|_| AnalyzeError::ToolUnavailable("ffmpeg".to_string()))?;
    let detect_log = String::from_utf8_lossy(&detect_output.stderr).into_owned();
    if !detect_output.status.success() {
        return Err(AnalyzeError::ToolFailed {
            tool: "ffmpeg",
            detail: detect_log.trim().to_string(),
        });
    }
    // Detect-pass times are range-relative (input seeking); shift
    // them onto the source timeline the frame stats already use.
    let shift = |a: Anomaly| -> Anomaly {
        match a {
            Anomaly::BlackSpan {
                start_secs,
                end_secs,
            } => Anomaly::BlackSpan {
                start_secs: start_secs + range.start_secs,
                end_secs: end_secs + range.start_secs,
            },
            Anomaly::FreezeSpan {
                start_secs,
                end_secs,
            } => Anomaly::FreezeSpan {
                start_secs: start_secs + range.start_secs,
                end_secs: end_secs + range.start_secs,
            },
            other => other,
        }
    };
    let mut anomalies: Vec<Anomaly> = Vec::new();
    anomalies.extend(parse_blackdetect_log(&detect_log).into_iter().map(shift));
    anomalies.extend(parse_freezedetect_log(&detect_log).into_iter().map(shift));
    anomalies.extend(temporal_outliers(&stats, tout_threshold));
    sort_anomalies(&mut anomalies);

    let idet = parse_idet_log(&detect_log);
    Ok(AnalysisReport {
        source: path.to_string_lossy().into_owned(),
        range,
        probe,
        frames_analyzed,
        signal,
        anomalies,
        suggestion: idet.suggestion(),
        idet,
        tout_threshold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FFPROBE_FIXTURE: &str = r#"{
      "streams": [
        {"index": 0, "codec_name": "ffv1", "codec_type": "video",
         "width": 720, "height": 576, "avg_frame_rate": "25/1",
         "field_order": "tt"},
        {"index": 1, "codec_name": "flac", "codec_type": "audio",
         "sample_rate": "48000", "channels": 2}
      ],
      "format": {"format_name": "matroska,webm", "duration": "600.000000",
                 "size": "123456789", "bit_rate": "1646090"}
    }"#;

    const SIGNALSTATS_FIXTURE: &str = "\
[FRAME]
pts_time=0.000000
TAG:lavfi.signalstats.YMIN=16
TAG:lavfi.signalstats.YAVG=100.5
TAG:lavfi.signalstats.YMAX=235
TAG:lavfi.signalstats.UAVG=128
TAG:lavfi.signalstats.VAVG=127
TAG:lavfi.signalstats.SATAVG=10.25
TAG:lavfi.signalstats.TOUT=0.001
TAG:lavfi.signalstats.BRNG=0.000
[/FRAME]
[FRAME]
pts_time=0.040000
TAG:lavfi.signalstats.YMIN=16
TAG:lavfi.signalstats.YAVG=101
TAG:lavfi.signalstats.YMAX=235
TAG:lavfi.signalstats.UAVG=128
TAG:lavfi.signalstats.VAVG=127
TAG:lavfi.signalstats.SATAVG=10
TAG:lavfi.signalstats.TOUT=0.42
TAG:lavfi.signalstats.BRNG=0.010
[/FRAME]
";

    #[test]
    fn estimates_video_frames_from_duration_and_rate() {
        let mut info = parse_ffprobe_json(FFPROBE_FIXTURE).expect("parse");
        // 600 s at 25/1.
        assert_eq!(info.estimated_video_frames(), Some(15_000));
        let video = info
            .streams
            .iter_mut()
            .find(|s| s.codec_type == "video")
            .unwrap();
        video.avg_frame_rate = Some("30000/1001".into());
        assert_eq!(info.estimated_video_frames(), Some(17_982));
        for unusable in [None, Some("0/0"), Some("n/a")] {
            let video = info
                .streams
                .iter_mut()
                .find(|s| s.codec_type == "video")
                .unwrap();
            video.avg_frame_rate = unusable.map(String::from);
            assert_eq!(info.estimated_video_frames(), None, "{unusable:?}");
        }
    }

    #[test]
    fn parses_ffprobe_json_fixture() {
        let info = parse_ffprobe_json(FFPROBE_FIXTURE).expect("parse");
        assert_eq!(info.format_name, "matroska,webm");
        assert_eq!(info.duration_secs, 600.0);
        assert_eq!(info.size_bytes, 123_456_789);
        assert_eq!(info.bit_rate, Some(1_646_090));
        let video = info.first_video().expect("video stream");
        assert_eq!(video.codec_name, "ffv1");
        assert_eq!(video.width, Some(720));
        assert_eq!(video.field_order.as_deref(), Some("tt"));
        let audio = &info.streams[1];
        assert_eq!(audio.sample_rate, Some(48_000));
        assert_eq!(audio.channels, Some(2));
    }

    #[test]
    fn bad_ffprobe_json_is_a_parse_error() {
        assert!(matches!(
            parse_ffprobe_json("{not json"),
            Err(AnalyzeError::Parse { .. })
        ));
    }

    #[test]
    fn parses_signalstats_frames() {
        let stats = parse_signalstats(SIGNALSTATS_FIXTURE);
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].frame, 0);
        assert_eq!(stats[0].y_avg, 100.5);
        assert_eq!(stats[1].frame, 1);
        assert_eq!(stats[1].tout, 0.42);
    }

    #[test]
    fn temporal_outliers_flags_only_spikes() {
        let stats = parse_signalstats(SIGNALSTATS_FIXTURE);
        let found = temporal_outliers(&stats, 0.05);
        assert_eq!(found.len(), 1);
        match found[0] {
            Anomaly::TemporalOutlier { frame, tout, .. } => {
                assert_eq!(frame, 1);
                assert_eq!(tout, 0.42);
            }
            ref other => panic!("expected TemporalOutlier, got {other:?}"),
        }
    }

    #[test]
    fn parses_blackdetect_log() {
        let log = "[blackdetect @ 0x55] black_start:1.5 black_end:4 black_duration:2.5\nnoise line";
        let found = parse_blackdetect_log(log);
        assert_eq!(
            found,
            vec![Anomaly::BlackSpan {
                start_secs: 1.5,
                end_secs: 4.0
            }]
        );
    }

    #[test]
    fn parses_freezedetect_log_pairs() {
        let log = "lavfi.freezedetect.freeze_start: 12.25\nlavfi.freezedetect.freeze_duration: 3.25\nlavfi.freezedetect.freeze_end: 15.5";
        let found = parse_freezedetect_log(log);
        assert_eq!(
            found,
            vec![Anomaly::FreezeSpan {
                start_secs: 12.25,
                end_secs: 15.5
            }]
        );
    }

    #[test]
    fn sample_range_validation() {
        assert!(SampleRange::full().validate().is_ok());
        assert!(SampleRange::new(12.5, Some(30.0)).validate().is_ok());
        assert!(SampleRange::new(12.5, None).validate().is_ok());
        for bad in [
            SampleRange::new(-0.1, None),
            SampleRange::new(f64::NAN, None),
            SampleRange::new(f64::INFINITY, None),
            SampleRange::new(0.0, Some(0.0)),
            SampleRange::new(0.0, Some(-2.0)),
            SampleRange::new(0.0, Some(f64::NAN)),
            SampleRange::new(0.0, Some(f64::INFINITY)),
        ] {
            assert!(
                matches!(bad.validate(), Err(AnalyzeError::InvalidRange(_))),
                "{bad:?} must be rejected"
            );
        }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn summarize_known_frames_exact() {
        let stats = parse_signalstats(SIGNALSTATS_FIXTURE);
        let summary = summarize(&stats).expect("two frames summarize");
        assert_eq!(summary.frames, 2);
        assert_close(summary.y_avg.min, 100.5);
        assert_close(summary.y_avg.max, 101.0);
        assert_close(summary.y_avg.mean, 100.75);
        assert_close(summary.sat_avg.min, 10.0);
        assert_close(summary.sat_avg.max, 10.25);
        assert_close(summary.sat_avg.mean, 10.125);
        assert_close(summary.tout.min, 0.001);
        assert_close(summary.tout.max, 0.42);
        assert_close(summary.tout.mean, 0.2105);
        assert_close(summary.brng.min, 0.0);
        assert_close(summary.brng.max, 0.01);
        assert_close(summary.brng.mean, 0.005);
    }

    #[test]
    fn summarize_empty_is_none_not_zeros() {
        assert!(summarize(&[]).is_none());
    }

    const IDET_FIXTURE: &str = "\
[Parsed_idet_2 @ 0x55] Repeated Fields: Neither:     0 Top:     0 Bottom:     0
[Parsed_idet_2 @ 0x55] Single frame detection: TFF:     0 BFF:     0 Progressive:     0 Undetermined:     0
[Parsed_idet_2 @ 0x55] Multi frame detection: TFF:     0 BFF:     0 Progressive:     0 Undetermined:     0
frame=   75 fps= 25 q=-0.0 size=N/A time=00:00:03.00
[Parsed_idet_2 @ 0x55] Repeated Fields: Neither:    74 Top:     1 Bottom:     0
[Parsed_idet_2 @ 0x55] Single frame detection: TFF:     0 BFF:     0 Progressive:     4 Undetermined:    71
[Parsed_idet_2 @ 0x55] Multi frame detection: TFF:     0 BFF:     0 Progressive:    51 Undetermined:    24
";

    #[test]
    fn parses_idet_log_last_occurrence_wins() {
        let counts = parse_idet_log(IDET_FIXTURE);
        assert_eq!(counts.neither, 74);
        assert_eq!(counts.repeated_top, 1);
        assert_eq!(counts.repeated_bottom, 0);
        assert_eq!(counts.single_tff, 0);
        assert_eq!(counts.single_progressive, 4);
        assert_eq!(counts.single_undetermined, 71);
        assert_eq!(counts.multi_progressive, 51);
        assert_eq!(counts.multi_undetermined, 24);
    }

    #[test]
    fn idet_suggestion_progressive_dominant() {
        let counts = parse_idet_log(IDET_FIXTURE);
        // Determined: progressive 4 + 51 = 55 of 55.
        assert_eq!(counts.suggestion(), InterlaceSuggestion::Progressive);
    }

    #[test]
    fn idet_suggestion_tff_at_sixty_percent() {
        let counts = IdetCounts {
            single_tff: 30,
            multi_tff: 30,
            single_progressive: 40,
            ..Default::default()
        };
        // 60 of 100 determined = exactly the 60% bar.
        assert_eq!(counts.suggestion(), InterlaceSuggestion::TopFieldFirst);
    }

    #[test]
    fn idet_suggestion_ambiguous_is_undetermined() {
        let counts = IdetCounts {
            single_tff: 40,
            single_bff: 20,
            single_progressive: 40,
            ..Default::default()
        };
        // Leader holds only 40% — no honest suggestion.
        assert_eq!(counts.suggestion(), InterlaceSuggestion::Undetermined);
    }

    #[test]
    fn idet_suggestion_zero_determined_is_undetermined() {
        assert_eq!(
            IdetCounts::default().suggestion(),
            InterlaceSuggestion::Undetermined
        );
        let only_undetermined = IdetCounts {
            single_undetermined: 90,
            multi_undetermined: 85,
            ..Default::default()
        };
        assert_eq!(
            only_undetermined.suggestion(),
            InterlaceSuggestion::Undetermined
        );
    }

    #[test]
    fn anomalies_sort_by_time() {
        let mut anomalies = vec![
            Anomaly::BlackSpan {
                start_secs: 10.0,
                end_secs: 11.0,
            },
            Anomaly::TemporalOutlier {
                frame: 3,
                pts_time: 0.12,
                tout: 0.5,
            },
            Anomaly::FreezeSpan {
                start_secs: 4.0,
                end_secs: 6.5,
            },
        ];
        sort_anomalies(&mut anomalies);
        let keys: Vec<f64> = anomalies.iter().map(Anomaly::sort_key).collect();
        assert_eq!(keys, vec![0.12, 4.0, 10.0]);
    }

    #[test]
    fn analysis_report_json_round_trip() {
        let report = AnalysisReport {
            source: "capture_1987.mkv".to_string(),
            range: SampleRange::new(60.0, Some(30.0)),
            probe: parse_ffprobe_json(FFPROBE_FIXTURE).expect("parse"),
            frames_analyzed: 2,
            signal: summarize(&parse_signalstats(SIGNALSTATS_FIXTURE)),
            anomalies: vec![Anomaly::BlackSpan {
                start_secs: 65.0,
                end_secs: 67.5,
            }],
            idet: parse_idet_log(IDET_FIXTURE),
            suggestion: InterlaceSuggestion::Progressive,
            tout_threshold: 0.05,
        };
        let json = serde_json::to_string_pretty(&report).expect("serialize");
        let back: AnalysisReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }

    #[test]
    fn escape_lavfi_movie_path_cases() {
        // Plain paths pass through untouched (spaces included).
        assert_eq!(
            escape_lavfi_movie_path("/tmp/my capture.mp4"),
            "/tmp/my capture.mp4"
        );
        // Colon: option-level escape, escaped again for the graph.
        assert_eq!(
            escape_lavfi_movie_path("/tmp/rd:probe.mp4"),
            "/tmp/rd\\\\:probe.mp4"
        );
        // A literal backslash doubles at both levels.
        assert_eq!(
            escape_lavfi_movie_path("/tmp/back\\slash.mp4"),
            "/tmp/back\\\\\\\\slash.mp4"
        );
        // Apostrophe: survives both parse levels.
        assert_eq!(
            escape_lavfi_movie_path("/tmp/apos'probe.mp4"),
            "/tmp/apos\\\\\\'probe.mp4"
        );
        // Graph separators get a single escape.
        assert_eq!(
            escape_lavfi_movie_path("/tmp/a,b;c[d].mp4"),
            "/tmp/a\\,b\\;c\\[d\\].mp4"
        );
    }
}
