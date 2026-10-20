//! Integration test for `analyze` against the real ffmpeg/ffprobe.
//!
//! Ignored by default: CI runners are not guaranteed to have FFmpeg
//! installed. Run locally with:
//!
//! ```sh
//! cargo test -p rd-analyze -- --ignored
//! ```
//!
//! The test generates its own clip (1s test pattern, 1s black, 1s
//! test pattern), so the expected black span is known exactly, and
//! nothing here depends on fixture media checked into the repo.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::Command;

use rd_analyze::{analyze, Anomaly, SampleRange};

fn make_test_clip(dir: &std::path::Path) -> PathBuf {
    let clip = dir.join("clip.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=320x240:rate=25",
            "-f",
            "lavfi",
            "-i",
            "color=black:size=320x240:rate=25:duration=1",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=320x240:rate=25",
            "-filter_complex",
            "[0][1][2]concat=n=3:v=1:a=0",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&clip)
        .status()
        .expect("run ffmpeg to generate the test clip");
    assert!(status.success(), "ffmpeg clip generation failed");
    clip
}

#[test]
#[ignore = "requires ffmpeg/ffprobe on PATH"]
fn analyze_generated_clip_finds_black_span() {
    assert!(
        rd_analyze::ffprobe_available() && rd_analyze::ffmpeg_available(),
        "test is ignored by default precisely for machines without the tools"
    );
    let dir = std::env::temp_dir().join(format!("reeldeck-analyze-it-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let clip = make_test_clip(&dir);

    let report = analyze(&clip, SampleRange::full(), 0.05).expect("analyze generated clip");

    assert_eq!(report.frames_analyzed, 75, "3s at 25fps");
    assert!(report.signal.is_some(), "frames measured, summary present");
    assert_eq!(report.signal.map(|s| s.frames), Some(75));
    let black = report.anomalies.iter().find_map(|a| match a {
        Anomaly::BlackSpan {
            start_secs,
            end_secs,
        } => Some((*start_secs, *end_secs)),
        _ => None,
    });
    let (start, end) = black.expect("the black middle second must be flagged");
    assert!(
        (0.9..=1.1).contains(&start),
        "black span starts at ~1s, got {start}"
    );
    assert!(
        (1.9..=2.1).contains(&end),
        "black span ends at ~2s, got {end}"
    );
    // The whole run must be sorted by source-timeline position.
    let keys: Vec<f64> = report.anomalies.iter().map(Anomaly::sort_key).collect();
    assert!(
        keys.windows(2).all(|w| w[0] <= w[1]),
        "anomalies sorted: {keys:?}"
    );
    // Cleaned up only on success; a failed run leaves the clip in
    // place for debugging.
    std::fs::remove_dir_all(&dir).ok();
}
