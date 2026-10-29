//! MediaKit integration for the probe path.
//!
//! MediaKit reads media headers directly (no process spawn) and
//! reports container, duration, bitrate, and per-stream facts as
//! JSON. This module maps that result into this crate's
//! [`ProbeInfo`] model and exposes the MediaKit-first probe used by
//! [`crate::probe`]. The mapping is deliberately lossy in exactly
//! the places the models differ, and never inventive:
//!
//! - `container` becomes `format_name` verbatim.
//! - `duration_ms` becomes `duration_secs` (`ms / 1000`); a null
//!   duration becomes `0.0`, the model's "unknown" value (the same
//!   convention the ffprobe parser uses for a missing duration).
//! - `bitrate_bps` becomes `bit_rate`.
//! - `size_bytes` is not a header fact MediaKit reports; the caller
//!   supplies it from the file's metadata.
//! - Each MediaKit stream keeps its position as `index`; `kind`
//!   becomes `codec_type`, `codec` becomes `codec_name` (an empty
//!   string when the container names no codec — the model field is
//!   not optional), and width / height / sample_rate / channels
//!   carry across directly.
//! - `frame_rate` becomes `avg_frame_rate` and `field_order`
//!   carries across (MediaKit 0.2.0+ reports both in ffprobe's
//!   notation, from container fields only; older results and
//!   containers that do not store them leave both `None`). One
//!   documented difference from ffprobe: for Matroska, MediaKit gives
//!   the exact stored rate (`500000000/16683333`) where ffprobe
//!   rounds to `30000/1001`. Its `language` and `duration_is_estimate` facts have no
//!   field in [`ProbeInfo`]; they remain available on
//!   [`rd_mediakit::MediaKitProbe`] for callers that need them.
//!
//! On non-Windows platforms (or any MediaKit error) this probe
//! reports the [`MediaKitError`] it was given and
//! [`crate::probe`] falls back to ffprobe.

use std::path::Path;

pub use rd_mediakit::MediaKitError;

use crate::{ProbeInfo, StreamInfo};

/// True when the MediaKit probe is compiled into this binary
/// (Windows). See [`rd_mediakit::is_available`].
pub fn mediakit_available() -> bool {
    rd_mediakit::is_available()
}

/// Map a parsed MediaKit probe into the [`ProbeInfo`] model.
/// `size_bytes` comes from the file's metadata: it is a filesystem
/// fact, not something the header probe reports.
pub fn probe_info_from_mediakit(raw: &rd_mediakit::MediaKitProbe, size_bytes: u64) -> ProbeInfo {
    let streams = raw
        .streams
        .iter()
        .enumerate()
        .map(|(i, s)| StreamInfo {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            codec_type: s.kind.clone(),
            codec_name: s.codec.clone().unwrap_or_default(),
            width: s.width,
            height: s.height,
            avg_frame_rate: s.frame_rate.clone(),
            field_order: s.field_order.clone(),
            sample_rate: s.sample_rate,
            channels: s.channels,
        })
        .collect();
    ProbeInfo {
        format_name: raw.container.clone(),
        duration_secs: raw.duration_ms.map(|ms| ms as f64 / 1000.0).unwrap_or(0.0),
        size_bytes,
        bit_rate: raw.bitrate_bps,
        streams,
    }
}

/// Probe a file with MediaKit and map the result into [`ProbeInfo`].
///
/// Fails with the underlying [`MediaKitError`] whenever MediaKit is
/// unavailable on this platform, does not recognize the container,
/// or reports an error — the caller decides the fallback.
pub fn probe_via_mediakit(path: &Path) -> Result<ProbeInfo, MediaKitError> {
    let raw = rd_mediakit::probe(path)?;
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(probe_info_from_mediakit(&raw, size_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MP4_FIXTURE: &str = r#"{
      "container": "mp4",
      "duration_ms": 123456,
      "duration_is_estimate": false,
      "bitrate_bps": 1234567,
      "streams": [
        {"kind": "video", "codec": "avc1", "width": 1920, "height": 1080,
         "sample_rate": null, "channels": null, "language": null},
        {"kind": "audio", "codec": "mp4a", "width": null, "height": null,
         "sample_rate": 48000, "channels": 2, "language": "eng"}
      ]
    }"#;

    const MP3_ESTIMATE_FIXTURE: &str = r#"{
      "container": "mp3",
      "duration_ms": 187000,
      "duration_is_estimate": true,
      "bitrate_bps": null,
      "streams": [
        {"kind": "audio", "codec": null, "width": null, "height": null,
         "sample_rate": null, "channels": null, "language": null}
      ]
    }"#;

    #[test]
    fn maps_mp4_fixture_into_probe_info() {
        let raw = rd_mediakit::parse_probe_json(MP4_FIXTURE).expect("parse fixture");
        let info = probe_info_from_mediakit(&raw, 19_000_000);
        assert_eq!(info.format_name, "mp4");
        assert_eq!(info.duration_secs, 123.456);
        assert_eq!(info.size_bytes, 19_000_000);
        assert_eq!(info.bit_rate, Some(1_234_567));
        assert_eq!(info.streams.len(), 2);

        let video = info.first_video().expect("video stream");
        assert_eq!(video.index, 0);
        assert_eq!(video.codec_name, "avc1");
        assert_eq!(video.width, Some(1920));
        assert_eq!(video.height, Some(1080));
        // A 0.1-shaped result carries no frame-rate or field-order
        // facts; both stay unset rather than being filled from
        // another source.
        assert_eq!(video.avg_frame_rate, None);
        assert_eq!(video.field_order, None);

        let audio = &info.streams[1];
        assert_eq!(audio.index, 1);
        assert_eq!(audio.codec_type, "audio");
        assert_eq!(audio.sample_rate, Some(48_000));
        assert_eq!(audio.channels, Some(2));
    }

    #[test]
    fn maps_frame_rate_and_field_order() {
        let raw = rd_mediakit::parse_probe_json(
            r#"{"container": "matroska", "duration_ms": 2000, "duration_is_estimate": false,
                "bitrate_bps": null, "streams": [
                {"kind": "video", "codec": "V_MPEG4/ISO/AVC", "width": 720, "height": 576,
                 "frame_rate": "25/1", "field_order": "bb"}]}"#,
        )
        .expect("parse fixture");
        let info = probe_info_from_mediakit(&raw, 1);
        let video = info.first_video().expect("video");
        assert_eq!(video.avg_frame_rate.as_deref(), Some("25/1"));
        assert_eq!(video.field_order.as_deref(), Some("bb"));
        // The rate feeds the same frame estimate the ffprobe path uses.
        assert_eq!(info.estimated_video_frames(), Some(50));
    }

    #[test]
    fn maps_null_fields_and_estimated_duration() {
        let raw = rd_mediakit::parse_probe_json(MP3_ESTIMATE_FIXTURE).expect("parse fixture");
        // The estimate flag itself lives on the MediaKit model...
        assert!(raw.duration_is_estimate);
        // ...and the estimated duration maps like any other value:
        // ProbeInfo has no estimate field, so nothing extra is
        // claimed in the mapped model.
        let info = probe_info_from_mediakit(&raw, 4_500_000);
        assert_eq!(info.format_name, "mp3");
        assert_eq!(info.duration_secs, 187.0);
        assert_eq!(info.bit_rate, None);
        let audio = &info.streams[0];
        // A null codec maps to the model's empty string, never to a
        // guessed codec name.
        assert_eq!(audio.codec_name, "");
        assert_eq!(audio.width, None);
        assert_eq!(audio.sample_rate, None);
        assert_eq!(audio.channels, None);
    }

    #[test]
    fn maps_null_duration_to_zero_like_the_ffprobe_parser() {
        let raw = rd_mediakit::parse_probe_json(
            r#"{"container": "wav", "duration_ms": null,
                "duration_is_estimate": false, "bitrate_bps": 1411200,
                "streams": []}"#,
        )
        .expect("parse fixture");
        let info = probe_info_from_mediakit(&raw, 10);
        assert_eq!(info.duration_secs, 0.0);
        assert_eq!(info.bit_rate, Some(1_411_200));
        assert!(info.streams.is_empty());
    }

    #[cfg(not(windows))]
    #[test]
    fn probe_via_mediakit_reports_unavailable_off_windows() {
        assert!(!mediakit_available());
        assert!(matches!(
            probe_via_mediakit(Path::new("anything.mp4")),
            Err(MediaKitError::Unavailable(_))
        ));
    }
}
