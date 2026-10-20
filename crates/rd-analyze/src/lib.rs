//! ReelDeck frame analysis.
//!
//! Turns the textual output of the engine's inspection tools — `ffprobe`
//! JSON, `signalstats` frame metadata, and the `blackdetect` /
//! `freezedetect` log formats — into a typed model the UI and CLI can
//! present. Parsing is pure and fully unit-tested; the only process
//! execution in this crate is [`probe`], which shells out to `ffprobe`
//! and reports a typed [`AnalyzeError::ToolUnavailable`] when the tool
//! is missing, rather than panicking or inventing data.
//!
//! Analysis findings are *observations*, never verdicts: an interlacing
//! suggestion from these numbers is confirmed by the operator, not
//! silently acted on.

#![forbid(unsafe_code)]

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyzeError {
    #[error("required tool '{0}' is not available on this system; install FFmpeg (which provides ffprobe/ffmpeg) to enable inspection")]
    ToolUnavailable(String),
    #[error("could not parse {what}: {detail}")]
    Parse { what: &'static str, detail: String },
    #[error("running {tool} failed: {detail}")]
    ToolFailed { tool: &'static str, detail: String },
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

/// Probe a media file with `ffprobe` and parse the result.
///
/// Returns [`AnalyzeError::ToolUnavailable`] when ffprobe is not
/// installed — inspection is impossible without it, and this crate
/// will not fabricate a result.
pub fn probe(path: &Path) -> Result<ProbeInfo, AnalyzeError> {
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
}
