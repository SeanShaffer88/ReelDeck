//! `reeldeck metadata` end to end through the real CLI binary: fields
//! written into MP4, MOV, MKV and AVI copies are read back, the ones a
//! container drops are reported and logged, the streams are copied
//! untouched, and clearing removes the source's own fields.
//!
//! Ignored by default: it needs FFmpeg with libx264 and aac on the
//! PATH, and CI runners are not guaranteed one. Run locally with:
//!
//! ```sh
//! cargo test -p reeldeck --test cli_metadata -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "reeldeck-cli-metadata-{}-{tag}",
        std::process::id()
    ));
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

/// The elementary streams' hashes, container excluded.
fn stream_hash(path: &Path) -> String {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-map",
            "0",
            "-c",
            "copy",
            "-f",
            "streamhash",
            "-hash",
            "sha256",
            "-",
        ])
        .output()
        .expect("spawn ffmpeg");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
#[ignore = "requires ffmpeg with libx264 and aac on PATH"]
fn metadata_is_written_read_back_and_reported() {
    let dir = temp_dir("metadata");
    for (name, extra) in [("src.mp4", None), ("src.avi", Some("mpeg4"))] {
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=160x120:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=1",
        ]);
        match extra {
            Some(codec) => cmd.args(["-c:v", codec, "-c:a", "pcm_s16le"]),
            None => cmd.args(["-c:v", "libx264", "-c:a", "aac"]),
        };
        let status = cmd
            .args(["-shortest", "-metadata", "title=Original", "-y", name])
            .current_dir(&dir)
            .status()
            .expect("spawn ffmpeg");
        assert!(status.success());
    }
    let fields = [
        "--title",
        "Tape 11",
        "--description",
        "Side A",
        "--tag",
        "council",
        "--tag",
        "1991",
    ];

    // MP4 keeps every field; the streams are copied untouched.
    let mut args = vec!["metadata", "src.mp4"];
    args.extend_from_slice(&fields);
    let mp4 = run_bin(&dir, &args);
    assert_eq!(mp4.status.code(), Some(0), "{}", text(&mp4));
    assert!(!text(&mp4).contains("Warning:"), "{}", text(&mp4));
    let tagged = dir.join("src.tagged.mp4");
    assert_eq!(stream_hash(&tagged), stream_hash(&dir.join("src.mp4")));
    let show = text(&run_bin(&dir, &["metadata", "src.tagged.mp4"]));
    assert!(
        show.contains("title: Tape 11")
            && show.contains("description: Side A")
            && show.contains("keywords: council, 1991"),
        "{show}"
    );
    let verify = run_bin(&dir, &["verify", "src.tagged.mp4"]);
    assert_eq!(verify.status.code(), Some(0), "{}", text(&verify));

    // MOV keeps every field too (custom keys enabled), and so does MKV.
    for out in ["copy.mov", "copy.mkv"] {
        let mut args = vec!["metadata", "src.mp4", "--out", out];
        args.extend_from_slice(&fields);
        let run = run_bin(&dir, &args);
        assert_eq!(run.status.code(), Some(0), "{}", text(&run));
        assert!(!text(&run).contains("Warning:"), "{out}: {}", text(&run));
    }

    // AVI keeps only the title: the dropped fields are reported and logged.
    let mut args = vec!["metadata", "src.avi"];
    args.extend_from_slice(&fields);
    let avi = run_bin(&dir, &args);
    let avi_text = text(&avi);
    assert_eq!(avi.status.code(), Some(0), "{avi_text}");
    assert!(
        avi_text.contains("did not keep the description field"),
        "{avi_text}"
    );
    assert!(
        avi_text.contains("did not keep the keywords field"),
        "{avi_text}"
    );
    let log = std::fs::read_to_string(dir.join("src.tagged.avi.export-log.json")).expect("log");
    assert!(log.contains("did not keep the description field"), "{log}");

    // Clearing drops the source's own title.
    let cleared = run_bin(
        &dir,
        &[
            "metadata",
            "src.mp4",
            "--clear",
            "--tag",
            "x",
            "--out",
            "clear.mp4",
        ],
    );
    assert_eq!(cleared.status.code(), Some(0), "{}", text(&cleared));
    let show = text(&run_bin(&dir, &["metadata", "clear.mp4"]));
    assert!(
        !show.contains("Original") && show.contains("keywords: x"),
        "{show}"
    );

    // The source itself is never the output.
    let same = run_bin(
        &dir,
        &["metadata", "src.mp4", "--title", "T", "--out", "src.mp4"],
    );
    assert_eq!(same.status.code(), Some(1), "{}", text(&same));
    assert!(text(&same).contains("will not be overwritten"));
}
