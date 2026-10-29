//! `reeldeck join` end to end through the real CLI binary: lossless and
//! re-encoded joins of matching and mixed-format sources, refusals for
//! joins that would change footage silently, and verification of every
//! joined source.
//!
//! Ignored by default: it needs FFmpeg with libx264, aac, mpeg4 and
//! ffv1 on the PATH, and CI runners are not guaranteed one. Run locally
//! with:
//!
//! ```sh
//! cargo test -p reeldeck --test cli_join -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("reeldeck-cli-join-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn run_bin(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_reeldeck"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn the reeldeck binary")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// A test clip: `secs` of 25 fps picture at `size`, with a tone unless
/// `audio_codec` is `None`.
fn clip(path: &Path, secs: u32, size: &str, video_codec: &str, audio_codec: Option<&str>) {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg(format!("testsrc=duration={secs}:size={size}:rate=25"));
    if audio_codec.is_some() {
        cmd.args(["-f", "lavfi", "-i"])
            .arg(format!("sine=frequency=440:duration={secs}"));
    }
    cmd.args(["-c:v", video_codec]);
    if let Some(codec) = audio_codec {
        cmd.args(["-c:a", codec, "-shortest"]);
    }
    let status = cmd.arg("-y").arg(path).status().expect("spawn ffmpeg");
    assert!(
        status.success(),
        "clip generation failed for {}",
        path.display()
    );
}

fn video_frames(path: &Path) -> u64 {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .expect("spawn ffprobe");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("frame count")
}

#[test]
#[ignore = "requires ffmpeg with libx264, aac, mpeg4 and ffv1 on PATH"]
fn join_lossless_reencoded_and_refused() {
    let dir = temp_dir("join");
    clip(&dir.join("a.mp4"), 2, "320x240", "libx264", Some("aac"));
    clip(&dir.join("b.mp4"), 3, "320x240", "libx264", Some("aac"));
    clip(&dir.join("c.avi"), 2, "320x240", "mpeg4", Some("pcm_s16le"));
    clip(&dir.join("hd.mp4"), 1, "640x480", "libx264", Some("aac"));
    clip(&dir.join("silent.mp4"), 1, "320x240", "libx264", None);

    // Lossless: matching sources, streams copied.
    let lossless = run_bin(
        &dir,
        &["join", "a.mp4", "b.mp4", "--lossless", "--out", "ab.mkv"],
    );
    assert_eq!(lossless.status.code(), Some(0), "{}", text(&lossless));
    assert_eq!(video_frames(&dir.join("ab.mkv")), 50 + 75);
    assert!(
        !dir.join("ab.mkv.concat.txt").exists(),
        "the concat list is removed after the run"
    );
    let log = std::fs::read_to_string(dir.join("ab.mkv.export-log.json")).expect("log");
    assert!(
        log.contains("\"joined_sources\"") && log.contains("b.mp4"),
        "{log}"
    );
    assert!(log.contains("lossless join: 2 sources"), "{log}");
    let verify = run_bin(&dir, &["verify", "ab.mkv"]);
    let verify_text = text(&verify);
    assert_eq!(verify.status.code(), Some(0), "{verify_text}");
    assert!(verify_text.contains("PASS joined_sources"), "{verify_text}");

    // Re-encoded: MP4 and AVI with different codecs, one output.
    let mixed = run_bin(
        &dir,
        &[
            "join",
            "a.mp4",
            "c.avi",
            "--preset",
            "preservation",
            "--out",
            "ac.mkv",
        ],
    );
    assert_eq!(mixed.status.code(), Some(0), "{}", text(&mixed));
    assert_eq!(video_frames(&dir.join("ac.mkv")), 50 + 50);

    // Refusals: nothing is written.
    let codecs = run_bin(
        &dir,
        &["join", "a.mp4", "c.avi", "--lossless", "--out", "no1.mkv"],
    );
    assert_eq!(codecs.status.code(), Some(1), "{}", text(&codecs));
    assert!(text(&codecs).contains("video codecs differ"));
    let sizes = run_bin(
        &dir,
        &[
            "join", "a.mp4", "hd.mp4", "--preset", "access", "--out", "no2.mp4",
        ],
    );
    assert!(
        text(&sizes).contains("source 1 320x240, source 2 640x480"),
        "{}",
        text(&sizes)
    );
    let audio = run_bin(
        &dir,
        &[
            "join",
            "a.mp4",
            "silent.mp4",
            "--preset",
            "access",
            "--out",
            "no3.mp4",
        ],
    );
    assert!(
        text(&audio).contains("source 2 has no audio"),
        "{}",
        text(&audio)
    );
    for name in ["no1.mkv", "no2.mp4", "no3.mp4"] {
        assert!(!dir.join(name).exists(), "{name} must not be written");
    }

    // A joined source changed after the export: verify fails.
    clip(&dir.join("b.mp4"), 1, "320x240", "libx264", Some("aac"));
    let tampered = run_bin(&dir, &["verify", "ab.mkv"]);
    assert_eq!(tampered.status.code(), Some(1), "{}", text(&tampered));
    assert!(
        text(&tampered).contains("FAIL joined_sources"),
        "{}",
        text(&tampered)
    );
}
