//! `reeldeck convert` end to end through the real CLI binary: each
//! output's container, codecs, size, rate and audio are probed back from
//! the file, the shipped profiles work, and refusals write nothing.
//!
//! Ignored by default: it needs FFmpeg with libx264, libx265, aac,
//! mpeg4, prores_ks, ffv1 and flac on the PATH, and CI runners are not
//! guaranteed one. Run locally with:
//!
//! ```sh
//! cargo test -p reeldeck --test cli_convert -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("reeldeck-cli-convert-{}-{tag}", std::process::id()));
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

/// What ffprobe says a file holds, as `key=value` fields per stream and
/// for the container, joined into one line.
fn describe(path: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=format_name:stream=codec_type,codec_name,width,height,avg_frame_rate,codec_tag_string",
            "-of",
            "compact=p=0",
        ])
        .arg(path)
        .output()
        .expect("spawn ffprobe");
    String::from_utf8_lossy(&out.stdout).replace('\n', " ; ")
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

fn convert_ok(dir: &Path, args: &[&str]) -> String {
    let mut all = vec!["convert", "src.mp4"];
    all.extend_from_slice(args);
    let out = run_bin(dir, &all);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    text(&out)
}

#[test]
#[ignore = "requires ffmpeg with libx264, libx265, aac, mpeg4, prores_ks, ffv1 and flac on PATH"]
fn convert_writes_what_the_settings_say() {
    let dir = temp_dir("convert");
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=640x360:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-shortest",
            "-y",
            "src.mp4",
        ])
        .current_dir(&dir)
        .status()
        .expect("spawn ffmpeg");
    assert!(status.success());

    // AVI with its defaults, resized and slowed to 10 fps.
    convert_ok(
        &dir,
        &[
            "--container",
            "avi",
            "--size",
            "320x240",
            "--fps",
            "10",
            "--out",
            "small.avi",
        ],
    );
    let small = describe(&dir.join("small.avi"));
    assert!(
        small.contains("codec_name=mpeg4")
            && small.contains("width=320|height=240")
            && small.contains("avg_frame_rate=10/1"),
        "{small}"
    );
    assert!(small.contains("codec_name=pcm_s16le"), "{small}");
    assert!(small.contains("format_name=avi"), "{small}");
    assert_eq!(video_frames(&dir.join("small.avi")), 20);
    let log = std::fs::read_to_string(dir.join("small.avi.export-log.json")).expect("log");
    assert!(
        log.contains("scaled to exactly 320x240") && log.contains("no frames are interpolated"),
        "{log}"
    );
    let verify = run_bin(&dir, &["verify", "small.avi"]);
    assert_eq!(verify.status.code(), Some(0), "{}", text(&verify));

    // A shipped profile.
    let profile = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/edit-prores.json");
    convert_ok(
        &dir,
        &["--settings", profile.to_str().unwrap(), "--out", "edit.mov"],
    );
    let edit = describe(&dir.join("edit.mov"));
    assert!(
        edit.contains("codec_name=prores") && edit.contains("codec_name=pcm_s16le"),
        "{edit}"
    );

    // Lossless archive codecs, HEVC with the Apple tag, and no audio.
    convert_ok(
        &dir,
        &[
            "--container",
            "mkv",
            "--video",
            "ffv1",
            "--audio",
            "flac",
            "--out",
            "a.mkv",
        ],
    );
    let archive = describe(&dir.join("a.mkv"));
    assert!(
        archive.contains("codec_name=ffv1") && archive.contains("codec_name=flac"),
        "{archive}"
    );
    convert_ok(
        &dir,
        &[
            "--container",
            "mp4",
            "--video",
            "h265",
            "--video-bitrate",
            "800",
            "--audio",
            "none",
            "--out",
            "hevc.mp4",
        ],
    );
    let hevc = describe(&dir.join("hevc.mp4"));
    assert!(
        hevc.contains("codec_name=hevc") && hevc.contains("codec_tag_string=hvc1"),
        "{hevc}"
    );
    assert!(!hevc.contains("codec_type=audio"), "{hevc}");

    // Refusals write nothing.
    let mismatch = run_bin(
        &dir,
        &[
            "convert",
            "src.mp4",
            "--container",
            "mp4",
            "--out",
            "wrong.mkv",
        ],
    );
    assert_eq!(mismatch.status.code(), Some(1), "{}", text(&mismatch));
    assert!(
        text(&mismatch).contains("name it .mp4"),
        "{}",
        text(&mismatch)
    );
    assert!(!dir.join("wrong.mkv").exists());
    let incompatible = run_bin(
        &dir,
        &[
            "convert",
            "src.mp4",
            "--container",
            "mp4",
            "--audio",
            "pcm16",
        ],
    );
    assert_eq!(
        incompatible.status.code(),
        Some(2),
        "{}",
        text(&incompatible)
    );
    assert!(text(&incompatible).contains("cannot carry pcm16 audio"));
}
