//! The Prove loop, end to end through the real CLI binary:
//! export writes the proof, `verify` checks it, a tampered copy
//! fails verification, and `rerun` reproduces the recorded run.
//!
//! Ignored by default: it needs an FFmpeg with bwdif / FFV1 on the
//! PATH (the binary shells out to it), and CI runners are not
//! guaranteed one. Run locally with:
//!
//! ```sh
//! cargo test -p reeldeck --test cli_prove_loop -- --ignored
//! ```
//!
//! The determinism assertion is the claim under test, stated at
//! the level where it actually holds: FFV1 on the same machine and
//! build, fed the same recipe, produces bit-identical elementary
//! streams. The Matroska container around them is not bit-stable —
//! the muxer writes a per-run Segment UUID and DateUTC — so two
//! runs differ as files while their streams match, and the stream
//! digest recorded in each export log is the invariant asserted
//! below. If that ever stops holding, this test — not the product
//! copy — is where it should surface first.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rd_core::{DeinterlaceEngine, ExportPreset, FieldOrder, Operation, OutputRate, Recipe};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("reeldeck-cli-it-{}-{tag}", std::process::id()));
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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn generate_clip(path: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=320x240:rate=25",
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
#[ignore = "requires ffmpeg with bwdif and ffv1 on PATH"]
fn export_verify_tamper_rerun() {
    let dir = temp_dir("prove");
    let input = dir.join("input.mkv");
    generate_clip(&input);
    let source_hash = rd_core::sha256_file(&input).expect("hash source");

    let mut recipe =
        Recipe::new("input.mkv", ExportPreset::PreservationMaster).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::TopFieldFirst,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        });
    recipe.source_sha256 = Some(source_hash);
    std::fs::write(
        dir.join("recipe.json"),
        recipe.to_json().expect("serialize recipe"),
    )
    .expect("write recipe");

    // -- export ----------------------------------------------------
    let out = run_bin(&dir, &["export", "recipe.json"]);
    assert!(
        out.status.success(),
        "export failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exported = dir.join("input.restored.mkv");
    assert!(exported.exists(), "export output written");

    // -- verify: the intact proof passes ----------------------------
    let out = run_bin(&dir, &["verify", "input.restored.mkv"]);
    assert!(
        out.status.success(),
        "verify of a fresh export must succeed: {}",
        stdout(&out)
    );
    assert!(
        stdout(&out).contains("Verification: OK"),
        "verify output: {}",
        stdout(&out)
    );

    // -- verify: a tampered copy fails on its output hash -----------
    let tampered = dir.join("tampered.mkv");
    std::fs::copy(&exported, &tampered).expect("copy output");
    for suffix in [".recipe.json", ".export-log.json"] {
        let from = dir.join(format!("input.restored.mkv{suffix}"));
        let to = dir.join(format!("tampered.mkv{suffix}"));
        std::fs::copy(&from, &to).expect("copy sidecar");
    }
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&tampered)
            .expect("open tampered copy");
        f.write_all(b"\x00").expect("append one byte");
    }
    let out = run_bin(&dir, &["verify", "tampered.mkv"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "verify of a tampered output must exit 1: {}",
        stdout(&out)
    );
    let text = stdout(&out);
    assert!(
        text.contains("FAIL") && text.contains("output_hash"),
        "verify names the failed output_hash check: {text}"
    );
    assert!(
        text.contains("Verification: FAILED"),
        "verify output: {text}"
    );

    // -- rerun: reproduces the recorded run -------------------------
    let out = run_bin(&dir, &["rerun", "input.restored.mkv.export-log.json"]);
    assert!(
        out.status.success(),
        "rerun failed: {} {}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout(&out);
    assert!(
        text.contains("Determinism: bit-identical")
            || text.contains("Determinism: streams bit-identical"),
        "rerun reports a positive determinism verdict, at file or stream level: {text}"
    );
    let rerun_output = dir.join("input.restored.rerun.mkv");
    assert!(rerun_output.exists(), "rerun output written");
    let (rerun_recipe, rerun_log) = rd_run::sidecar_paths(&rerun_output);
    assert!(rerun_recipe.exists(), "rerun recipe sidecar written");
    assert!(rerun_log.exists(), "rerun export-log sidecar written");

    // The stream-level invariant: both runs recorded a stream
    // digest, and the digests are equal — even though (on this
    // FFmpeg/Matroska build) the output files themselves differ.
    let recorded_log = rd_core::ExportLog::from_json(
        &std::fs::read_to_string(dir.join("input.restored.mkv.export-log.json"))
            .expect("read recorded export log"),
    )
    .expect("parse recorded export log");
    let rerun_log_parsed = rd_core::ExportLog::from_json(
        &std::fs::read_to_string(&rerun_log).expect("read rerun export log"),
    )
    .expect("parse rerun export log");
    let recorded_stream = recorded_log
        .output_stream_sha256
        .expect("recorded log carries a stream digest");
    let rerun_stream = rerun_log_parsed
        .output_stream_sha256
        .expect("rerun log carries a stream digest");
    assert_eq!(
        recorded_stream, rerun_stream,
        "the elementary streams of the two runs are bit-identical"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
