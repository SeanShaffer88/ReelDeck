//! `reeldeck batch` end to end through the real CLI binary: a batch
//! stops at a failing entry, resumes there after the cause is fixed
//! (writing the retry to a new attempt name), skips the entry that
//! already succeeded, and every output verifies against its proof.
//!
//! Ignored by default: it needs an FFmpeg with bwdif / FFV1 on the
//! PATH, and CI runners are not guaranteed one. Run locally with:
//!
//! ```sh
//! cargo test -p reeldeck --test cli_batch -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rd_core::{
    BatchEntry, BatchManifest, DeinterlaceEngine, ExportPreset, FieldOrder, Operation, OutputRate,
    Recipe,
};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("reeldeck-cli-batch-{}-{tag}", std::process::id()));
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

fn generate_clip(path: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
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

fn write_recipe(dir: &Path, source: &str) -> String {
    let recipe =
        Recipe::new(source, ExportPreset::PreservationMaster).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::TopFieldFirst,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        });
    let name = format!("{source}.recipe.json");
    std::fs::write(dir.join(&name), recipe.to_json().expect("serialize")).expect("write recipe");
    name
}

#[test]
#[ignore = "requires ffmpeg with bwdif and ffv1 on PATH"]
fn batch_stops_resumes_and_proves_every_output() {
    let dir = temp_dir("resume");
    generate_clip(&dir.join("one.mkv"));
    // "two.mkv" is missing at first, so entry 2 fails.
    let entries = ["one.mkv", "two.mkv"]
        .iter()
        .map(|source| BatchEntry {
            recipe: write_recipe(&dir, source),
            input: None,
            output: None,
        })
        .collect();
    std::fs::write(
        dir.join("batch.json"),
        BatchManifest::new(entries).to_json().expect("serialize"),
    )
    .expect("write manifest");

    let first = run_bin(&dir, &["batch", "batch.json"]);
    let first_text = text(&first);
    assert_eq!(first.status.code(), Some(1), "{first_text}");
    assert!(first_text.contains("[1] succeeded"), "{first_text}");
    assert!(
        first_text.contains("input file does not exist"),
        "{first_text}"
    );
    assert!(first_text.contains("resume at entry 2"), "{first_text}");
    assert!(dir.join("one.restored.mkv").exists());

    let status = run_bin(&dir, &["batch", "batch.json", "--status"]);
    let status_text = text(&status);
    assert_eq!(status.status.code(), Some(0), "{status_text}");
    assert!(
        status_text.contains("1 succeeded, 1 failed"),
        "{status_text}"
    );

    generate_clip(&dir.join("two.mkv"));
    let second = run_bin(&dir, &["batch", "batch.json"]);
    let second_text = text(&second);
    assert_eq!(second.status.code(), Some(0), "{second_text}");
    assert!(second_text.contains("[1] skip"), "{second_text}");
    assert!(
        second_text.contains("two.restored.attempt2.mkv (attempt 2)"),
        "{second_text}"
    );
    assert!(
        second_text.contains("2 succeeded, 0 failed"),
        "{second_text}"
    );

    for output in ["one.restored.mkv", "two.restored.attempt2.mkv"] {
        let verify = run_bin(&dir, &["verify", output]);
        assert_eq!(verify.status.code(), Some(0), "{}", text(&verify));
    }
}
