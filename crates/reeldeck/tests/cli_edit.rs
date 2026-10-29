//! `reeldeck cut` and `reeldeck split` end to end through the real CLI
//! binary: frame-accurate and lossless cuts land where the plan said,
//! the lossless cut's keyframe snap is recorded in its export log,
//! split parts each carry their own proof, and a re-run of the same
//! split resumes instead of rendering again.
//!
//! Ignored by default: it needs FFmpeg with ffv1, libx264 and aac on
//! the PATH, and CI runners are not guaranteed one. Run locally with:
//!
//! ```sh
//! cargo test -p reeldeck --test cli_edit -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("reeldeck-cli-edit-{}-{tag}", std::process::id()));
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

/// Six seconds of 25 fps picture with a tone, keyframes every 2 s.
fn generate_clip(path: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=6:size=320x240:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=6",
            "-c:v",
            "libx264",
            "-g",
            "50",
            "-c:a",
            "aac",
            "-shortest",
            "-y",
        ])
        .arg(path)
        .status()
        .expect("spawn ffmpeg to generate the test clip");
    assert!(status.success(), "clip generation failed");
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
#[ignore = "requires ffmpeg with ffv1, libx264 and aac on PATH"]
fn cut_and_split_with_proof() {
    let dir = temp_dir("edit");
    let clip = dir.join("clip.mp4");
    generate_clip(&clip);

    // No cut mode chosen: refused, nothing written.
    let refused = run_bin(&dir, &["cut", "clip.mp4", "--keep", "1-2"]);
    assert_eq!(refused.status.code(), Some(2), "{}", text(&refused));
    assert!(text(&refused).contains("choose how to cut"));

    // Frame-accurate: remove the middle, keep 0–1 s and 3 s–end.
    let accurate = run_bin(
        &dir,
        &[
            "cut",
            "clip.mp4",
            "--keep",
            "0-1",
            "--keep",
            "3-",
            "--preset",
            "access",
            "--out",
            "trimmed.mp4",
        ],
    );
    assert_eq!(accurate.status.code(), Some(0), "{}", text(&accurate));
    assert_eq!(video_frames(&dir.join("trimmed.mp4")), 25 + 75);
    let verify = run_bin(&dir, &["verify", "trimmed.mp4"]);
    assert_eq!(verify.status.code(), Some(0), "{}", text(&verify));

    // Lossless from 2.5 s: snaps to the 2 s keyframe, and the log says so.
    let lossless = run_bin(
        &dir,
        &[
            "cut",
            "clip.mp4",
            "--keep",
            "2.5-4.5",
            "--lossless",
            "--out",
            "copy.mkv",
        ],
    );
    assert_eq!(lossless.status.code(), Some(0), "{}", text(&lossless));
    let log = std::fs::read_to_string(dir.join("copy.mkv.export-log.json")).expect("log");
    assert!(
        log.contains("the copy starts at the keyframe at 2s, 0.500s earlier"),
        "{log}"
    );
    let recipe = std::fs::read_to_string(dir.join("copy.mkv.recipe.json")).expect("recipe");
    assert!(
        recipe.contains("\"stream_copy\"") && recipe.contains("\"schema_version\": 2"),
        "{recipe}"
    );

    // Split at 2 s and 4 s: three frame-accurate parts, each proved.
    let split = run_bin(
        &dir,
        &[
            "split", "clip.mp4", "--at", "2", "--at", "4", "--preset", "access",
        ],
    );
    assert_eq!(split.status.code(), Some(0), "{}", text(&split));
    let parts = dir.join("clip.split");
    for n in 1..=3 {
        let part = parts.join(format!("clip.part{n:02}.mp4"));
        assert_eq!(video_frames(&part), 50, "part {n}");
        let verify = run_bin(
            &parts,
            &["verify", part.file_name().unwrap().to_str().unwrap()],
        );
        assert_eq!(verify.status.code(), Some(0), "{}", text(&verify));
    }

    // The same split again resumes: every part re-verified and skipped.
    let again = run_bin(
        &dir,
        &[
            "split", "clip.mp4", "--at", "2", "--at", "4", "--preset", "access",
        ],
    );
    let again_text = text(&again);
    assert_eq!(again.status.code(), Some(0), "{again_text}");
    assert_eq!(
        again_text.matches("skip: already succeeded").count(),
        3,
        "{again_text}"
    );

    // A different split into the same folder is refused, not overwritten.
    let other = run_bin(
        &dir,
        &["split", "clip.mp4", "--at", "3", "--preset", "access"],
    );
    assert_eq!(other.status.code(), Some(1), "{}", text(&other));
    assert!(text(&other).contains("not overwritten"));
}
