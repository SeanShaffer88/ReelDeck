//! End-to-end export against the real FFmpeg.
//!
//! Ignored by default: CI runners are not guaranteed an FFmpeg with
//! bwdif / FFV1 / libvidstab, and a render test must not pretend to
//! pass where it cannot run. Run locally with:
//!
//! ```sh
//! cargo test -p rd-run --test run_export_integration -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use rd_core::{
    DeinterlaceEngine, ExportLog, ExportPreset, FieldOrder, Operation, OutputRate, Recipe,
};
use rd_run::{run_export, RunError, RunOptions};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rd-run-it-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn generate_clip(path: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=320x240:rate=25",
            "-c:v",
            "ffv1",
            "-y",
        ])
        .arg(path)
        .status()
        .expect("spawn ffmpeg to generate the test clip");
    assert!(status.success(), "clip generation failed");
}

#[test]
#[ignore = "requires ffmpeg/ffprobe with bwdif and ffv1 on PATH"]
fn export_bwdif_send_field_doubles_rate_and_writes_proof() {
    let dir = temp_dir("export");
    let input = dir.join("clip.mkv");
    generate_clip(&input);
    let source_hash = rd_core::sha256_file(&input).expect("hash source");

    let mut recipe =
        Recipe::new("clip.mkv", ExportPreset::PreservationMaster).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::TopFieldFirst,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        });
    recipe.source_sha256 = Some(source_hash.clone());

    let output = dir.join("clip.restored.mkv");
    let report =
        run_export(&recipe, &input, &output, RunOptions::default()).expect("export should succeed");
    assert!(output.exists(), "output written");
    // bwdif send_field: one frame per field, 25 fps in -> 50 fps out.
    assert_eq!(report.frames_processed, 100);
    assert_eq!(
        report.output_sha256,
        rd_core::sha256_file(&output).expect("hash output")
    );

    // ffprobe directly: the frame rate is not a header fact the MediaKit
    // probe (tried first on Windows) reports.
    let probe = rd_analyze::probe_ffprobe(&output).expect("probe output");
    let video = probe.first_video().expect("output has a video stream");
    assert_eq!(video.avg_frame_rate.as_deref(), Some("50/1"));

    // The proof: both sidecars beside the output.
    let (recipe_path, log_path) = rd_run::sidecar_paths(&output);
    assert_eq!(report.recipe_path, recipe_path);
    assert_eq!(report.log_path, log_path);
    let logged_recipe =
        Recipe::from_json(&std::fs::read_to_string(&recipe_path).expect("read recipe sidecar"))
            .expect("recipe sidecar parses");
    assert_eq!(logged_recipe, recipe);

    let log = ExportLog::from_json(&std::fs::read_to_string(&log_path).expect("read export log"))
        .expect("export log parses");
    assert!(log.success);
    assert_eq!(log.source_sha256, source_hash);
    assert_eq!(
        log.output_sha256.as_deref(),
        Some(report.output_sha256.as_str())
    );
    assert_eq!(log.frames_processed, 100);
    assert!(
        log.command
            .iter()
            .any(|a| a.contains("bwdif=mode=send_field")),
        "log records the executed chain: {:?}",
        log.command
    );
    // The executed vector is the log's truth: progress flags in,
    // never-overwrite in, plain -y gone.
    assert!(log.command.iter().any(|a| a == "-progress"));
    assert!(log.command.iter().any(|a| a == "-n"));
    assert!(!log.command.iter().any(|a| a == "-y"));
    assert!(log.ffmpeg_version.is_some());

    // A second run to the same output refuses, in preflight.
    let err = run_export(&recipe, &input, &output, RunOptions::default())
        .expect_err("second export to the same output must refuse");
    assert!(
        matches!(err, RunError::OutputExists(ref p) if *p == output),
        "got {err:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
