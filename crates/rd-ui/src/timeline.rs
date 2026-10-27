//! The timeline model behind the viewer: measurement lanes, anomaly
//! marks, the playhead, and the preview range.
//!
//! The centre of the window is a picture over a strip of measurements.
//! This module holds that strip: the per-frame series an inspection
//! produced (downsampled for drawing without losing peaks), the anomaly
//! marks an operator can jump between, and the in/out points of a
//! segment preview.
//!
//! Downsampling is the one place a graph can lie, so it is deliberate:
//! each drawn bucket keeps the **minimum and maximum** of the frames it
//! covers, never their mean. A single dropout frame in an hour of
//! otherwise flat signal stays visible as a spike.

use rd_analyze::{Anomaly, FrameStats};

use crate::format::FrameRate;

/// One measurable series over the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricLane {
    /// Stable identifier (`y_avg`, `tout`, …) used for lane toggles.
    pub id: &'static str,
    /// Label shown at the lane's left edge.
    pub label: &'static str,
    /// Unit for the readout, including a leading space (`" y"`, `"%"`).
    pub unit: &'static str,
    /// One value per analyzed frame, in frame order.
    pub values: Vec<f64>,
    /// A level the lane draws a rule at (the flagging threshold), when one
    /// exists. It is a threshold, not a verdict.
    pub threshold: Option<f64>,
    /// Fixed drawing domain, or `None` to scale to the observed values.
    pub domain: Option<(f64, f64)>,
}

/// One drawn column: the range of values it covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bucket {
    pub min: f64,
    pub max: f64,
}

impl MetricLane {
    /// True when there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The drawing domain: the fixed domain when there is one, otherwise
    /// the observed extent, padded so a flat series still draws a line
    /// with room around it.
    pub fn domain(&self) -> (f64, f64) {
        if let Some(d) = self.domain {
            return d;
        }
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for v in &self.values {
            if v.is_finite() {
                min = min.min(*v);
                max = max.max(*v);
            }
        }
        if !min.is_finite() || !max.is_finite() {
            return (0.0, 1.0);
        }
        if (max - min).abs() < f64::EPSILON {
            let pad = if max.abs() > 0.0 {
                max.abs() * 0.1
            } else {
                1.0
            };
            return (min - pad, max + pad);
        }
        let pad = (max - min) * 0.08;
        (min - pad, max + pad)
    }

    /// Where a value sits in the lane, 0 at the bottom and 1 at the top.
    pub fn normalized(&self, value: f64) -> f64 {
        let (lo, hi) = self.domain();
        if hi <= lo || !value.is_finite() {
            return 0.0;
        }
        ((value - lo) / (hi - lo)).clamp(0.0, 1.0)
    }

    /// The largest value and the index it occurs at.
    pub fn peak(&self) -> Option<(usize, f64)> {
        self.values
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_finite())
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, v)| (i, *v))
    }

    /// Downsample to at most `count` buckets, preserving each bucket's
    /// min and max. When there are fewer values than buckets, each value
    /// is its own bucket (min == max).
    pub fn buckets(&self, count: usize) -> Vec<Bucket> {
        if count == 0 || self.values.is_empty() {
            return Vec::new();
        }
        let n = self.values.len();
        if n <= count {
            return self
                .values
                .iter()
                .map(|v| Bucket { min: *v, max: *v })
                .collect();
        }
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let start = i * n / count;
            let end = (((i + 1) * n) / count).max(start + 1).min(n);
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            for v in &self.values[start..end] {
                if v.is_finite() {
                    min = min.min(*v);
                    max = max.max(*v);
                }
            }
            if min.is_finite() {
                out.push(Bucket { min, max });
            } else {
                out.push(Bucket { min: 0.0, max: 0.0 });
            }
        }
        out
    }

    /// The value at a source position, or `None` when the position is off
    /// the measured span.
    pub fn value_at_secs(&self, secs: f64, rate: FrameRate) -> Option<f64> {
        if !secs.is_finite() || secs < 0.0 || self.values.is_empty() {
            return None;
        }
        let idx = rate.frames_for_secs(secs) as usize;
        self.values.get(idx).copied()
    }
}

/// Build the four signal lanes from an inspection's per-frame statistics.
/// The analysis supplies both the values and the temporal-outlier
/// threshold that marked the frames, so the lane's rule and the anomaly
/// marks agree by construction.
pub fn lanes_from_stats(stats: &[FrameStats], tout_threshold: f64) -> Vec<MetricLane> {
    let lane = |id, label, unit, domain, threshold, pick: &dyn Fn(&FrameStats) -> f64| MetricLane {
        id,
        label,
        unit,
        values: stats.iter().map(pick).collect(),
        threshold,
        domain,
    };
    vec![
        lane("y_avg", "Luma", " y", Some((16.0, 235.0)), None, &|s| {
            s.y_avg
        }),
        lane("sat_avg", "Saturation", "", None, None, &|s| s.sat_avg),
        lane(
            "tout",
            "Temporal outliers",
            "",
            Some((0.0, 1.0)),
            Some(tout_threshold),
            &|s| s.tout,
        ),
        lane("brng", "Out of range", "", Some((0.0, 1.0)), None, &|s| {
            s.brng
        }),
    ]
}

/// What kind of mark an anomaly becomes on the strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Black,
    Freeze,
    Outlier,
}

impl MarkKind {
    pub fn label(self) -> &'static str {
        match self {
            MarkKind::Black => "Black span",
            MarkKind::Freeze => "Freeze span",
            MarkKind::Outlier => "Temporal outlier",
        }
    }

    /// A short text glyph, so the marks differ by more than color alone.
    pub fn glyph(self) -> &'static str {
        match self {
            MarkKind::Black => "B",
            MarkKind::Freeze => "F",
            MarkKind::Outlier => "!",
        }
    }
}

/// A clickable mark on the strip. Spans have an extent; single-frame
/// findings do not.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineMark {
    pub kind: MarkKind,
    pub start_secs: f64,
    pub end_secs: f64,
    /// The frame the mark belongs to, for single-frame findings.
    pub frame: Option<u64>,
    /// A sentence for the mark's tooltip and for the anomalies list.
    pub detail: String,
}

impl TimelineMark {
    pub fn is_point(&self) -> bool {
        self.end_secs <= self.start_secs
    }

    pub fn duration(&self) -> f64 {
        (self.end_secs - self.start_secs).max(0.0)
    }

    pub fn contains(&self, secs: f64) -> bool {
        secs >= self.start_secs && secs <= self.end_secs.max(self.start_secs)
    }
}

/// Convert inspection anomalies into strip marks. Order is preserved
/// (the analysis already sorted by source position).
pub fn marks_from_anomalies(anomalies: &[Anomaly]) -> Vec<TimelineMark> {
    anomalies
        .iter()
        .map(|a| match *a {
            Anomaly::BlackSpan {
                start_secs,
                end_secs,
            } => TimelineMark {
                kind: MarkKind::Black,
                start_secs,
                end_secs,
                frame: None,
                detail: format!("Black span {start_secs:.2}s – {end_secs:.2}s"),
            },
            Anomaly::FreezeSpan {
                start_secs,
                end_secs,
            } => TimelineMark {
                kind: MarkKind::Freeze,
                start_secs,
                end_secs,
                frame: None,
                detail: format!("Freeze span {start_secs:.2}s – {end_secs:.2}s"),
            },
            Anomaly::TemporalOutlier {
                frame,
                pts_time,
                tout,
            } => TimelineMark {
                kind: MarkKind::Outlier,
                start_secs: pts_time,
                end_secs: pts_time,
                frame: Some(frame),
                detail: format!(
                    "Temporal outlier at {pts_time:.2}s (frame {frame}, TOUT {tout:.4})"
                ),
            },
        })
        .collect()
}

/// A segment selected on the timeline, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeSelection {
    pub in_secs: f64,
    pub out_secs: f64,
}

impl RangeSelection {
    /// A selection from two points in any order.
    pub fn between(a: f64, b: f64) -> Self {
        RangeSelection {
            in_secs: a.min(b),
            out_secs: a.max(b),
        }
    }

    /// Clamp into `[0, duration]` and normalize the order.
    pub fn clamped(self, duration_secs: f64) -> Self {
        let hi = duration_secs.max(0.0);
        let a = self.in_secs.clamp(0.0, hi);
        let b = self.out_secs.clamp(0.0, hi);
        RangeSelection::between(a, b)
    }

    pub fn duration(self) -> f64 {
        (self.out_secs - self.in_secs).max(0.0)
    }

    /// A range of zero length is not a preview; the export control says so.
    pub fn is_usable(self) -> bool {
        self.duration() > 0.0 && self.in_secs.is_finite() && self.out_secs.is_finite()
    }
}

/// The measured strip and where the operator is on it.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineModel {
    pub duration_secs: f64,
    pub rate: FrameRate,
    pub playhead_secs: f64,
    pub selection: Option<RangeSelection>,
    pub lanes: Vec<MetricLane>,
    pub marks: Vec<TimelineMark>,
}

impl TimelineModel {
    pub fn new(duration_secs: f64, rate: FrameRate) -> Self {
        TimelineModel {
            duration_secs: if duration_secs.is_finite() {
                duration_secs.max(0.0)
            } else {
                0.0
            },
            rate,
            playhead_secs: 0.0,
            selection: None,
            lanes: Vec::new(),
            marks: Vec::new(),
        }
    }

    /// Fill the strip from an inspection result: lanes from the sampled
    /// frames and marks from its anomalies. The span the analysis
    /// covered is carried by the caller through `duration_secs`.
    pub fn with_inspection(
        mut self,
        stats: &[FrameStats],
        anomalies: &[Anomaly],
        tout_threshold: f64,
    ) -> Self {
        self.lanes = lanes_from_stats(stats, tout_threshold);
        self.marks = marks_from_anomalies(anomalies);
        self
    }

    pub fn frame(&self) -> u64 {
        self.rate.frames_for_secs(self.playhead_secs)
    }

    pub fn seek_secs(&mut self, secs: f64) {
        if !secs.is_finite() {
            return;
        }
        self.playhead_secs = secs.clamp(0.0, self.duration_secs);
    }

    /// Follow a position from the viewer.
    ///
    /// A strip with no measured span (`duration_secs == 0`) is *unmeasured*
    /// rather than zero-length: it still tracks the playhead so the readout
    /// and the viewer agree. Once a duration is known, positions clamp into
    /// it as usual.
    pub fn track_playhead(&mut self, secs: f64) {
        if !secs.is_finite() {
            return;
        }
        if self.duration_secs > 0.0 {
            self.playhead_secs = secs.clamp(0.0, self.duration_secs);
        } else {
            self.playhead_secs = secs.max(0.0);
        }
    }

    /// Step whole frames from the current position. Sub-frame positions
    /// are snapped to a frame first, so stepping is predictable.
    pub fn step_frames(&mut self, delta: i64) {
        let frame = self.frame() as i64 + delta;
        let frame = frame.max(0) as u64;
        self.track_playhead(self.rate.secs_for_frame(frame));
    }

    pub fn begin_selection(&mut self, secs: f64) {
        self.seek_secs(secs);
        self.selection = Some(RangeSelection::between(secs, secs).clamped(self.duration_secs));
    }

    pub fn extend_selection(&mut self, secs: f64) {
        let anchor = self
            .selection
            .map(|s| s.in_secs)
            .unwrap_or(self.playhead_secs);
        self.seek_secs(secs);
        self.selection = Some(RangeSelection::between(anchor, secs).clamped(self.duration_secs));
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Marks that overlap a range (used for "what is inside the preview?").
    pub fn marks_in(&self, range: RangeSelection) -> Vec<&TimelineMark> {
        self.marks
            .iter()
            .filter(|m| {
                m.end_secs.max(m.start_secs) >= range.in_secs && m.start_secs <= range.out_secs
            })
            .collect()
    }

    /// The next mark after the playhead, for `n` / shift-`n`.
    pub fn next_mark(&self) -> Option<&TimelineMark> {
        self.marks
            .iter()
            .filter(|m| m.start_secs > self.playhead_secs + 1e-9)
            .min_by(|a, b| {
                a.start_secs
                    .partial_cmp(&b.start_secs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// The previous mark before the playhead.
    pub fn previous_mark(&self) -> Option<&TimelineMark> {
        self.marks
            .iter()
            .filter(|m| m.start_secs < self.playhead_secs - 1e-9)
            .max_by(|a, b| {
                a.start_secs
                    .partial_cmp(&b.start_secs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Jump the playhead to a mark and return where it landed.
    pub fn jump_to(&mut self, mark: &TimelineMark) -> f64 {
        self.seek_secs(mark.start_secs);
        self.playhead_secs
    }

    /// A suggested preview segment, `span_secs` long, centred on the
    /// worst-measured moment rather than the first seconds: the mark with
    /// the highest measured severity (falls back to the peak of the
    /// temporal-outlier lane, then to the start).
    ///
    /// It is returned as a *suggestion* — the operator can drag any other
    /// range — and it is labelled as one in the UI.
    pub fn suggested_preview_range(&self, span_secs: f64) -> Option<RangeSelection> {
        if self.duration_secs <= 0.0 {
            return None;
        }
        let span = span_secs.max(0.1).min(self.duration_secs);
        let anchor = self
            .marks
            .iter()
            .filter(|m| m.kind == MarkKind::Outlier)
            .map(|m| m.start_secs)
            .next()
            .or_else(|| self.marks.first().map(|m| m.start_secs))
            .or_else(|| {
                self.lanes
                    .iter()
                    .find(|l| l.id == "tout")
                    .and_then(|l| l.peak())
                    .map(|(i, _)| self.rate.secs_for_frame(i as u64))
            })
            .unwrap_or(0.0);
        // Centre the span on the anchor, then slide it inside the source.
        let start = (anchor - span / 2.0).max(0.0);
        let start = start.min((self.duration_secs - span).max(0.0));
        Some(RangeSelection::between(start, start + span))
    }

    /// The lane's value under the playhead, for the readout strip.
    pub fn readout(&self, lane_id: &str) -> Option<f64> {
        self.lanes
            .iter()
            .find(|l| l.id == lane_id)
            .and_then(|l| l.value_at_secs(self.playhead_secs, self.rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_analyze::FrameStats;

    const PAL: FrameRate = FrameRate { num: 25, den: 1 };

    fn frame(i: u64, tout: f64) -> FrameStats {
        FrameStats {
            frame: i,
            pts_time: i as f64 / 25.0,
            y_min: 16.0,
            y_avg: 100.0,
            y_max: 235.0,
            u_avg: 128.0,
            v_avg: 127.0,
            sat_avg: 10.0,
            tout,
            brng: 0.0,
        }
    }

    fn lane(values: Vec<f64>) -> MetricLane {
        MetricLane {
            id: "test",
            label: "Test",
            unit: "",
            values,
            threshold: None,
            domain: None,
        }
    }

    #[test]
    fn bucketing_preserves_a_single_spike() {
        let mut values = vec![0.01; 1000];
        values[437] = 0.9;
        let peaks = lane(values).buckets(80);
        assert_eq!(peaks.len(), 80);
        let tallest = peaks
            .iter()
            .max_by(|a, b| a.max.partial_cmp(&b.max).unwrap())
            .expect("buckets");
        assert_eq!(tallest.max, 0.9, "the spike survives downsampling");
        assert_eq!(tallest.min, 0.01);
    }

    #[test]
    fn bucketing_keeps_dips_too() {
        let mut values = vec![200.0; 500];
        values[201] = 3.0;
        let buckets = lane(values).buckets(50);
        assert_eq!(
            buckets.iter().map(|b| b.min).fold(f64::INFINITY, f64::min),
            3.0
        );
    }

    #[test]
    fn few_values_draw_as_their_own_buckets() {
        let buckets = lane(vec![1.0, 5.0, 3.0]).buckets(80);
        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[1], Bucket { min: 5.0, max: 5.0 });
        assert!(lane(vec![]).buckets(10).is_empty());
        assert!(lane(vec![1.0]).buckets(0).is_empty());
    }

    #[test]
    fn normalization_handles_flat_and_fixed_domains() {
        let flat = lane(vec![7.0, 7.0, 7.0]);
        assert_eq!(flat.normalized(7.0), 0.5);
        let fixed = MetricLane {
            domain: Some((0.0, 1.0)),
            ..lane(vec![0.0, 0.5, 1.0])
        };
        assert_eq!(fixed.normalized(0.5), 0.5);
        assert_eq!(fixed.normalized(2.0), 1.0);
        assert_eq!(fixed.normalized(-1.0), 0.0);
        assert_eq!(fixed.normalized(f64::NAN), 0.0);
    }

    #[test]
    fn lanes_from_stats_carry_the_threshold_that_marked_frames() {
        let stats = vec![frame(0, 0.01), frame(1, 0.4)];
        let lanes = lanes_from_stats(&stats, 0.05);
        assert_eq!(lanes.len(), 4);
        let tout = lanes.iter().find(|l| l.id == "tout").expect("lane");
        assert_eq!(tout.threshold, Some(0.05));
        assert_eq!(tout.values, vec![0.01, 0.4]);
        assert_eq!(tout.domain, Some((0.0, 1.0)));
        assert_eq!(lanes[0].domain, Some((16.0, 235.0)));
    }

    #[test]
    fn marks_describe_spans_and_points() {
        let marks = marks_from_anomalies(&[
            Anomaly::BlackSpan {
                start_secs: 1.0,
                end_secs: 2.5,
            },
            Anomaly::FreezeSpan {
                start_secs: 4.0,
                end_secs: 6.0,
            },
            Anomaly::TemporalOutlier {
                frame: 100,
                pts_time: 4.0,
                tout: 0.5,
            },
        ]);
        assert_eq!(marks.len(), 3);
        assert_eq!(marks[0].kind, MarkKind::Black);
        assert!(!marks[0].is_point());
        assert_eq!(marks[0].duration(), 1.5);
        assert_eq!(marks[2].kind, MarkKind::Outlier);
        assert!(marks[2].is_point());
        assert_eq!(marks[2].frame, Some(100));
        assert!(marks[2].detail.contains("TOUT 0.5000"));
        // Distinct glyphs: shape is not carried by color alone.
        let glyphs: Vec<&str> = [MarkKind::Black, MarkKind::Freeze, MarkKind::Outlier]
            .iter()
            .map(|k| k.glyph())
            .collect();
        assert_eq!(glyphs, vec!["B", "F", "!"]);
    }

    #[test]
    fn seeking_and_stepping_are_bounded() {
        let mut t = TimelineModel::new(10.0, PAL);
        t.seek_secs(-5.0);
        assert_eq!(t.playhead_secs, 0.0);
        t.seek_secs(99.0);
        assert_eq!(t.playhead_secs, 10.0);
        t.seek_secs(f64::NAN);
        assert_eq!(
            t.playhead_secs, 10.0,
            "a NaN seek is ignored, not clamped to 0"
        );
        t.seek_secs(1.0);
        assert_eq!(t.frame(), 25);
        t.step_frames(-3);
        assert_eq!(t.frame(), 22);
        t.step_frames(-1000);
        assert_eq!(t.frame(), 0);
        t.step_frames(5);
        assert_eq!(t.frame(), 5);
        assert!((t.playhead_secs - 0.2).abs() < 1e-9);
    }

    #[test]
    fn an_unmeasured_strip_still_tracks_the_playhead() {
        let mut t = TimelineModel::new(0.0, PAL);
        t.track_playhead(3.5);
        assert_eq!(t.playhead_secs, 3.5, "no measured span: nothing to clamp to");
        t.track_playhead(-2.0);
        assert_eq!(t.playhead_secs, 0.0);
        t.track_playhead(f64::NAN);
        assert_eq!(t.playhead_secs, 0.0, "a NaN position is ignored");
        t.step_frames(25);
        assert!((t.playhead_secs - 1.0).abs() < 1e-9);
        // With a duration, positions clamp into the measured span.
        t.duration_secs = 0.5;
        t.track_playhead(10.0);
        assert_eq!(t.playhead_secs, 0.5);
    }

    #[test]
    fn selection_normalizes_and_clamps() {
        let mut t = TimelineModel::new(10.0, PAL);
        t.begin_selection(6.0);
        t.extend_selection(2.0);
        let sel = t.selection.expect("selection");
        assert_eq!(sel.in_secs, 2.0);
        assert_eq!(sel.out_secs, 6.0);
        assert!(sel.is_usable());
        t.extend_selection(-4.0);
        assert_eq!(t.selection.expect("selection").in_secs, 0.0);
        t.clear_selection();
        assert!(t.selection.is_none());
        assert!(!RangeSelection::between(3.0, 3.0).is_usable());
        assert!(!RangeSelection::between(f64::NAN, 3.0).is_usable());
    }

    #[test]
    fn mark_navigation_moves_between_findings() {
        let mut t = TimelineModel::new(60.0, PAL);
        t.marks = marks_from_anomalies(&[
            Anomaly::BlackSpan {
                start_secs: 5.0,
                end_secs: 6.0,
            },
            Anomaly::TemporalOutlier {
                frame: 750,
                pts_time: 30.0,
                tout: 0.4,
            },
            Anomaly::FreezeSpan {
                start_secs: 45.0,
                end_secs: 50.0,
            },
        ]);
        t.seek_secs(10.0);
        let next = t.next_mark().expect("next mark").clone();
        assert_eq!(next.start_secs, 30.0);
        assert_eq!(t.jump_to(&next), 30.0);
        let prev = t.previous_mark().expect("previous mark");
        assert_eq!(prev.start_secs, 5.0);
        t.seek_secs(0.0);
        assert!(t.previous_mark().is_none());
        t.seek_secs(60.0);
        assert!(t.next_mark().is_none());
        assert_eq!(t.marks_in(RangeSelection::between(0.0, 10.0)).len(), 1);
        assert_eq!(t.marks_in(RangeSelection::between(4.0, 46.0)).len(), 3);
    }

    #[test]
    fn suggested_preview_lands_on_the_worst_moment_and_stays_inside() {
        let mut t = TimelineModel::new(100.0, PAL);
        t.marks = marks_from_anomalies(&[
            Anomaly::FreezeSpan {
                start_secs: 20.0,
                end_secs: 21.0,
            },
            Anomaly::TemporalOutlier {
                frame: 12_000,
                pts_time: 48.0,
                tout: 0.6,
            },
        ]);
        let range = t.suggested_preview_range(10.0).expect("range");
        assert!(
            (range.in_secs - 43.0).abs() < 1e-9,
            "centred on the outlier at 48s"
        );
        assert!((range.duration() - 10.0).abs() < 1e-9);

        // Near the end, the span slides inward instead of overflowing.
        let mut t = TimelineModel::new(100.0, PAL);
        t.marks = marks_from_anomalies(&[Anomaly::BlackSpan {
            start_secs: 99.0,
            end_secs: 100.0,
        }]);
        let range = t.suggested_preview_range(20.0).expect("range");
        assert!((range.out_secs - 100.0).abs() < 1e-9);
        assert!((range.in_secs - 80.0).abs() < 1e-9);

        // No marks: fall back to the outlier lane's peak.
        let mut t = TimelineModel::new(100.0, PAL);
        let mut stats: Vec<FrameStats> = (0..2500).map(|i| frame(i, 0.01)).collect();
        stats[1000].tout = 0.7;
        t = t.with_inspection(&stats, &[], 0.05);
        let range = t.suggested_preview_range(8.0).expect("range");
        assert!((range.in_secs - 36.0).abs() < 1e-6, "{range:?}");

        assert!(TimelineModel::new(0.0, PAL)
            .suggested_preview_range(10.0)
            .is_none());
    }

    #[test]
    fn readout_reports_the_value_under_the_playhead() {
        let mut t = TimelineModel::new(10.0, PAL);
        t.lanes = lanes_from_stats(&[frame(0, 0.01), frame(1, 0.5)], 0.05);
        t.seek_secs(0.0);
        assert_eq!(t.readout("tout"), Some(0.01));
        t.seek_secs(0.04);
        assert_eq!(t.readout("tout"), Some(0.5));
        t.seek_secs(5.0);
        assert_eq!(t.readout("tout"), None, "past the measured span: no value");
        assert_eq!(t.readout("nope"), None);
    }
}
