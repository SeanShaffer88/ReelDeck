//! Display formatting: timecode, durations, byte sizes, and rates.
//!
//! Every number an operator reads is formatted here, so a value is
//! written the same way in the queue, the viewer, the timeline and the
//! export panel. Nothing is rounded into a claim it cannot support:
//! byte sizes keep a decimal in their unit and say the unit; a rate with
//! an unknown value is `—`, not `0`.

use crate::prefs::TimeDisplay;

/// A frame rate as an exact rational — `30000/1001` is not 29.97.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    /// A sane default when a probe reported no usable rate. 25/1 is the
    /// lowest common denominator for the archival material ReelDeck
    /// targets; callers that know better must pass a real rate.
    pub const FALLBACK: FrameRate = FrameRate { num: 25, den: 1 };

    pub fn new(num: u32, den: u32) -> Option<Self> {
        if num == 0 || den == 0 {
            None
        } else {
            Some(FrameRate { num, den })
        }
    }

    /// Parse an ffprobe-style rate string (`25/1`, `30000/1001`, `25`).
    pub fn parse(text: &str) -> Option<Self> {
        let (num, den) = match text.split_once('/') {
            Some((n, d)) => (n.trim().parse().ok()?, d.trim().parse().ok()?),
            None => (text.trim().parse().ok()?, 1),
        };
        FrameRate::new(num, den)
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    /// Frames per second, written for display: `25`, `29.97`, `23.976`.
    pub fn display(self) -> String {
        let fps = self.as_f64();
        if (fps - fps.round()).abs() < 1e-9 {
            format!("{}", fps.round() as u64)
        } else {
            let s = format!("{fps:.3}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    }

    /// Whole frames in `secs`, rounded to nearest. A frame count is an
    /// index, so it is an integer — never a fractional "frame 41.7".
    pub fn frames_for_secs(self, secs: f64) -> u64 {
        if !secs.is_finite() || secs <= 0.0 {
            return 0;
        }
        (secs * self.as_f64()).round().max(0.0) as u64
    }

    /// Seconds at a frame index.
    pub fn secs_for_frame(self, frame: u64) -> f64 {
        frame as f64 / self.as_f64()
    }
}

/// Non-drop timecode for a frame index: `HH:MM:SS:FF`.
///
/// Non-drop is the honest choice here: it counts real frames at the real
/// rate, so a timecode read out of ReelDeck matches the frame number the
/// analysis report names. Drop-frame notation is a broadcast delivery
/// convention, not a measurement, and the shell does not present it as
/// a measurement.
pub fn timecode(frame: u64, rate: FrameRate) -> String {
    let fps = rate.as_f64();
    if !(fps.is_finite() && fps > 0.0) {
        return "—".to_string();
    }
    // Frame counts are compared against the *integer* rate for the
    // frame field: 29.97 fps has 30 frame positions per timecode second.
    let nominal = fps.round().max(1.0) as u64;
    let total_secs = (frame as f64 / fps).floor().max(0.0) as u64;
    let hh = total_secs / 3600;
    let mm = (total_secs % 3600) / 60;
    let ss = total_secs % 60;
    let ff = frame % nominal;
    format!("{hh:02}:{mm:02}:{ss:02}:{ff:02}")
}

/// `01:02:03.500` — wall-clock time, useful next to a transcript.
pub fn clock_time(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "—".to_string();
    }
    let total = secs;
    let hh = (total / 3600.0).floor() as u64;
    let mm = ((total % 3600.0) / 60.0).floor() as u64;
    let ss = total % 60.0;
    format!("{hh:02}:{mm:02}:{ss:06.3}")
}

/// `2h 07m` / `07m 12s` / `42s` — a duration for a column, not a
/// position. Sub-second durations keep one decimal.
pub fn duration(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "—".to_string();
    }
    if secs < 1.0 {
        return format!("{secs:.1}s");
    }
    let whole = secs.round() as u64;
    let (h, m, s) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, _) => format!("{m}m {s:02}s"),
        _ => format!("{h}h {m:02}m"),
    }
}

/// A position written the way the operator asked for it.
pub fn position(frame: u64, secs: f64, rate: FrameRate, display: TimeDisplay) -> String {
    match display {
        TimeDisplay::Timecode => timecode(frame, rate),
        TimeDisplay::Clock => clock_time(secs),
        TimeDisplay::Frames => frame.to_string(),
    }
}

/// A byte count with one decimal and a binary unit: `1.2 GB`.
///
/// Binary units are labelled accordingly (a "GB" here is 2^30 bytes), and
/// the shell says so once in Preferences; an operator comparing against a
/// decimal-unit disk tool must see the difference, not discover it.
pub fn bytes_human(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A rate (`24.0 fps`, `3.2x`) or `—` when it was never measured.
pub fn rate_or_dash(value: Option<f64>, unit: &str, decimals: usize) -> String {
    match value {
        Some(v) if v.is_finite() => format!("{:.*}{unit}", decimals, v),
        _ => "—".to_string(),
    }
}

/// A 0..=1 fraction as a percentage with one decimal: `99.6%`.
pub fn percent(fraction: f64) -> String {
    if !fraction.is_finite() {
        return "—".to_string();
    }
    format!("{:.1}%", fraction * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NTSC: FrameRate = FrameRate {
        num: 30000,
        den: 1001,
    };
    const PAL: FrameRate = FrameRate { num: 25, den: 1 };

    #[test]
    fn frame_rate_parses_probe_strings() {
        assert_eq!(FrameRate::parse("25/1"), Some(PAL));
        assert_eq!(FrameRate::parse("25"), Some(PAL));
        assert_eq!(FrameRate::parse(" 30000/1001 "), Some(NTSC));
        assert_eq!(FrameRate::parse("0/1"), None);
        assert_eq!(FrameRate::parse("25/0"), None);
        assert_eq!(FrameRate::parse("nonsense"), None);
        assert_eq!(FrameRate::new(0, 1), None);
    }

    #[test]
    fn frame_rate_display_trims_without_lying() {
        assert_eq!(PAL.display(), "25");
        assert_eq!(NTSC.display(), "29.97");
        assert_eq!(
            FrameRate {
                num: 24000,
                den: 1001
            }
            .display(),
            "23.976"
        );
        assert_eq!(FrameRate { num: 50, den: 1 }.display(), "50");
    }

    #[test]
    fn frames_and_seconds_round_trip_within_a_frame() {
        assert_eq!(PAL.frames_for_secs(0.0), 0);
        assert_eq!(PAL.frames_for_secs(-3.0), 0);
        assert_eq!(PAL.frames_for_secs(f64::NAN), 0);
        assert_eq!(PAL.frames_for_secs(1.0), 25);
        assert_eq!(PAL.frames_for_secs(1.5), 38); // 37.5 rounds to even-ish nearest
        assert!((PAL.secs_for_frame(25) - 1.0).abs() < 1e-9);
        let frames = NTSC.frames_for_secs(60.0);
        assert_eq!(frames, 1798); // 59.94 fps × 60 s ≈ 1798.2
    }

    #[test]
    fn timecode_counts_real_frames_non_drop() {
        assert_eq!(timecode(0, PAL), "00:00:00:00");
        assert_eq!(timecode(24, PAL), "00:00:00:24");
        assert_eq!(timecode(25, PAL), "00:00:01:00");
        assert_eq!(timecode(90_000, PAL), "01:00:00:00");
        assert_eq!(timecode(25 * 61 + 3, PAL), "00:01:01:03");
        // NTSC: 30 frame positions, 29.97 real frames per second.
        assert_eq!(timecode(1798, NTSC), "00:00:59:28"); // 59.99 s → 59 s, frame 28
        assert_eq!(timecode(0, FrameRate { num: 0, den: 0 }), "—");
    }

    #[test]
    fn clock_time_is_millisecond_precise() {
        assert_eq!(clock_time(0.0), "00:00:00.000");
        assert_eq!(clock_time(61.5), "00:01:01.500");
        assert_eq!(clock_time(3723.25), "01:02:03.250");
        assert_eq!(clock_time(-1.0), "—");
        assert_eq!(clock_time(f64::INFINITY), "—");
    }

    #[test]
    fn durations_read_as_durations() {
        assert_eq!(duration(0.24), "0.2s");
        assert_eq!(duration(42.0), "42s");
        assert_eq!(duration(432.0), "7m 12s");
        assert_eq!(duration(7620.0), "2h 07m");
        assert_eq!(duration(-1.0), "—");
    }

    #[test]
    fn position_honours_the_preference() {
        let secs = PAL.secs_for_frame(25 * 61 + 3);
        assert_eq!(
            position(25 * 61 + 3, secs, PAL, TimeDisplay::Timecode),
            "00:01:01:03"
        );
        assert_eq!(
            position(25 * 61 + 3, secs, PAL, TimeDisplay::Clock),
            "00:01:01.120"
        );
        assert_eq!(
            position(25 * 61 + 3, secs, PAL, TimeDisplay::Frames),
            "1528"
        );
    }

    #[test]
    fn bytes_use_binary_units_and_label_them() {
        assert_eq!(bytes_human(0), "0 B");
        assert_eq!(bytes_human(512), "512 B");
        assert_eq!(bytes_human(1024), "1.0 KiB");
        assert_eq!(bytes_human(1536), "1.5 KiB");
        assert_eq!(bytes_human(1_048_576), "1.0 MiB");
        assert_eq!(bytes_human(4_200_000_000), "3.9 GiB");
        // The unit list stops at PiB; a value beyond it stays in PiB
        // rather than inventing a unit the shell cannot label.
        assert_eq!(bytes_human(u64::MAX), "16384.0 PiB");
    }

    #[test]
    fn unknowns_are_dashes_not_zeros() {
        assert_eq!(rate_or_dash(None, " fps", 2), "—");
        assert_eq!(rate_or_dash(Some(f64::NAN), " fps", 2), "—");
        assert_eq!(rate_or_dash(Some(24.0), " fps", 2), "24.00 fps");
        assert_eq!(percent(f64::NAN), "—");
        assert_eq!(percent(0.996), "99.6%");
    }
}
