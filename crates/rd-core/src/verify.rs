//! Verification: checking an export's proof after the fact.
//!
//! Every export ships with two sidecars — the recipe as executed and
//! the export log. [`verify_export`] compares the *record* (the two
//! sidecars against each other) and the *evidence* (the files on
//! disk, hashed by the caller and passed in) and reports each check
//! separately. The distinction that matters: a check whose evidence
//! is absent is **skipped**, never passed. Verification that cannot
//! see the source file says so, instead of implying the source was
//! confirmed.
//!
//! This module is pure: it reads no files and computes no hashes, so
//! every rule here is unit-testable without a filesystem.

use serde::Serialize;

use crate::log::ExportLog;
use crate::recipe::Recipe;

/// Outcome of one verification check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    /// The evidence this check needs was not available (for example
    /// the source file could not be found). A skip is not a pass.
    Skipped,
}

/// One named check, its outcome, and the facts behind the outcome.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
}

impl Check {
    fn new(name: &'static str, status: CheckStatus, detail: String) -> Self {
        Check {
            name,
            status,
            detail,
        }
    }
}

/// The result of verifying one export: every check, in order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerificationReport {
    pub checks: Vec<Check>,
}

impl VerificationReport {
    /// True when no check failed. Skipped checks do not fail a
    /// verification — but they are counted and printed, so a report
    /// that verified less than it appears to is visible as such.
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.status != CheckStatus::Fail)
    }

    pub fn passed(&self) -> usize {
        self.count(CheckStatus::Pass)
    }

    pub fn failed(&self) -> usize {
        self.count(CheckStatus::Fail)
    }

    pub fn skipped(&self) -> usize {
        self.count(CheckStatus::Skipped)
    }

    fn count(&self, status: CheckStatus) -> usize {
        self.checks.iter().filter(|c| c.status == status).count()
    }
}

/// Verify an export from its sidecars and the caller's hashes.
///
/// `source_sha256` / `output_sha256` are the SHA-256 of the files on
/// disk *now*, computed by the caller; `None` means the file was not
/// found or not hashed, and the affected check is skipped with the
/// reason — never silently passed. Checks run in a fixed order:
///
/// 1. `run_succeeded` — the recorded run itself: the log says it
///    succeeded and carries an output hash. This is about the
///    record, independent of what is on disk now.
/// 2. `output_hash` — the output file on disk hashes to the value
///    the log recorded.
/// 3. `source_hash` — the recipe's pinned source hash, the log's
///    recorded source hash, and the file on disk all agree.
/// 4. `preset_match` — recipe and log name the same export preset.
/// 5. `schema_match` — recipe and log name the same schema version.
pub fn verify_export(
    log: &ExportLog,
    recipe: &Recipe,
    source_sha256: Option<&str>,
    output_sha256: Option<&str>,
) -> VerificationReport {
    VerificationReport {
        checks: vec![
            check_run_succeeded(log),
            check_output_hash(log, output_sha256),
            check_source_hash(log, recipe, source_sha256),
            check_preset_match(log, recipe),
            check_schema_match(log, recipe),
        ],
    }
}

fn check_run_succeeded(log: &ExportLog) -> Check {
    if !log.success {
        Check::new(
            "run_succeeded",
            CheckStatus::Fail,
            "the recorded run did not succeed (the export log records success = false)".to_string(),
        )
    } else if log.output_sha256.is_none() {
        Check::new(
            "run_succeeded",
            CheckStatus::Fail,
            "the recorded run reports success but carries no output hash; the proof is incomplete"
                .to_string(),
        )
    } else {
        Check::new(
            "run_succeeded",
            CheckStatus::Pass,
            "the recorded run succeeded and carries an output hash".to_string(),
        )
    }
}

fn check_output_hash(log: &ExportLog, output_sha256: Option<&str>) -> Check {
    let Some(recorded) = &log.output_sha256 else {
        return Check::new(
            "output_hash",
            CheckStatus::Skipped,
            "the export log carries no output hash; there is nothing to check the file against"
                .to_string(),
        );
    };
    let Some(actual) = output_sha256 else {
        return Check::new(
            "output_hash",
            CheckStatus::Skipped,
            "the output file was not hashed by the caller; there is nothing to compare".to_string(),
        );
    };
    if actual.eq_ignore_ascii_case(recorded) {
        Check::new(
            "output_hash",
            CheckStatus::Pass,
            format!("the output hashes to {actual}, matching the export log"),
        )
    } else {
        Check::new(
            "output_hash",
            CheckStatus::Fail,
            format!(
                "output hash mismatch: the export log records {recorded} \
                 but the file hashes to {actual}"
            ),
        )
    }
}

fn check_source_hash(log: &ExportLog, recipe: &Recipe, source_sha256: Option<&str>) -> Check {
    let Some(pinned) = &recipe.source_sha256 else {
        return Check::new(
            "source_hash",
            CheckStatus::Skipped,
            "the recipe carries no source hash; there is nothing to check against".to_string(),
        );
    };
    // The two sidecars were written by the same run from the same
    // values; a disagreement between them means one was altered
    // afterwards, and that is a failure of the record, independent
    // of whether the source file itself can still be found.
    if !log.source_sha256.eq_ignore_ascii_case(pinned) {
        return Check::new(
            "source_hash",
            CheckStatus::Fail,
            format!(
                "the sidecars disagree about the source hash: the recipe records {pinned} \
                 but the export log records {}; a sidecar was altered after the run",
                log.source_sha256
            ),
        );
    }
    let Some(actual) = source_sha256 else {
        return Check::new(
            "source_hash",
            CheckStatus::Skipped,
            "the source file was not found; the recorded source hash cannot be re-checked"
                .to_string(),
        );
    };
    if actual.eq_ignore_ascii_case(pinned) {
        Check::new(
            "source_hash",
            CheckStatus::Pass,
            format!("the source hashes to {actual}, matching the recipe and the export log"),
        )
    } else {
        Check::new(
            "source_hash",
            CheckStatus::Fail,
            format!(
                "source hash mismatch: the recorded source hash is {pinned} \
                 but the source file hashes to {actual}"
            ),
        )
    }
}

fn check_preset_match(log: &ExportLog, recipe: &Recipe) -> Check {
    if log.preset == recipe.export_preset {
        Check::new(
            "preset_match",
            CheckStatus::Pass,
            format!(
                "recipe and export log both name the {:?} preset",
                recipe.export_preset
            ),
        )
    } else {
        Check::new(
            "preset_match",
            CheckStatus::Fail,
            format!(
                "preset disagreement: the recipe names {:?} but the export log names {:?}",
                recipe.export_preset, log.preset
            ),
        )
    }
}

fn check_schema_match(log: &ExportLog, recipe: &Recipe) -> Check {
    if log.recipe_schema_version == recipe.schema_version {
        Check::new(
            "schema_match",
            CheckStatus::Pass,
            format!(
                "recipe and export log both name recipe schema version {}",
                recipe.schema_version
            ),
        )
    } else {
        Check::new(
            "schema_match",
            CheckStatus::Fail,
            format!(
                "schema disagreement: the recipe names schema version {} \
                 but the export log names {}",
                recipe.schema_version, log.recipe_schema_version
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{DeinterlaceEngine, ExportPreset, FieldOrder, Operation, OutputRate};

    const SRC: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OUT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const OTHER: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn fixture_recipe() -> Recipe {
        let mut recipe = Recipe::new("capture.mkv", ExportPreset::PreservationMaster).push(
            Operation::Deinterlace {
                engine: DeinterlaceEngine::Bwdif,
                field_order: FieldOrder::TopFieldFirst,
                output_rate: OutputRate::SendField,
                only_flagged: false,
            },
        );
        recipe.source_sha256 = Some(SRC.to_string());
        recipe
    }

    fn fixture_log() -> ExportLog {
        let mut log = ExportLog::new(
            1,
            "capture.mkv",
            SRC,
            "capture.restored.mkv",
            ExportPreset::PreservationMaster,
            vec!["ffmpeg".into(), "-i".into(), "capture.mkv".into()],
            "2026-10-20T03:00:00Z",
        );
        log.finish("2026-10-20T03:01:00Z", Some(OUT.to_string()), true);
        log
    }

    fn check<'a>(report: &'a VerificationReport, name: &str) -> &'a Check {
        report
            .checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no check named {name}"))
    }

    #[test]
    fn fully_matching_proof_passes_every_check() {
        let report = verify_export(&fixture_log(), &fixture_recipe(), Some(SRC), Some(OUT));
        assert_eq!(report.checks.len(), 5);
        assert!(
            report.checks.iter().all(|c| c.status == CheckStatus::Pass),
            "all five checks pass: {:?}",
            report.checks
        );
        assert!(report.ok());
        assert_eq!(report.passed(), 5);
        assert_eq!(report.failed(), 0);
        assert_eq!(report.skipped(), 0);
    }

    #[test]
    fn failed_record_fails_run_succeeded_only() {
        let mut log = fixture_log();
        // A recorded failure that still carries an output hash
        // (not a state the runner produces, but the record check is
        // about the log's own claims).
        log.finish("2026-10-20T03:01:00Z", Some(OUT.to_string()), false);
        let report = verify_export(&log, &fixture_recipe(), Some(SRC), Some(OUT));
        assert_eq!(check(&report, "run_succeeded").status, CheckStatus::Fail);
        assert_eq!(check(&report, "output_hash").status, CheckStatus::Pass);
        assert!(!report.ok());
        assert_eq!(report.failed(), 1);
    }

    #[test]
    fn success_without_output_hash_fails_run_succeeded() {
        let mut log = fixture_log();
        log.output_sha256 = None;
        let report = verify_export(&log, &fixture_recipe(), Some(SRC), Some(OUT));
        assert_eq!(check(&report, "run_succeeded").status, CheckStatus::Fail);
        assert!(check(&report, "run_succeeded")
            .detail
            .contains("no output hash"));
        // And the output-hash check has nothing recorded to compare.
        assert_eq!(check(&report, "output_hash").status, CheckStatus::Skipped);
        assert!(!report.ok());
    }

    #[test]
    fn tampered_output_fails_output_hash_only() {
        let report = verify_export(&fixture_log(), &fixture_recipe(), Some(SRC), Some(OTHER));
        let c = check(&report, "output_hash");
        assert_eq!(c.status, CheckStatus::Fail);
        assert!(c.detail.contains(OUT), "detail names the recorded hash");
        assert!(c.detail.contains(OTHER), "detail names the actual hash");
        assert_eq!(report.failed(), 1);
        assert!(!report.ok());
    }

    #[test]
    fn unhashed_output_skips_output_hash() {
        let report = verify_export(&fixture_log(), &fixture_recipe(), Some(SRC), None);
        assert_eq!(check(&report, "output_hash").status, CheckStatus::Skipped);
        // A skip is not a failure.
        assert!(report.ok());
        assert_eq!(report.skipped(), 1);
    }

    #[test]
    fn recipe_without_source_hash_skips_source_hash() {
        let mut recipe = fixture_recipe();
        recipe.source_sha256 = None;
        let report = verify_export(&fixture_log(), &recipe, Some(SRC), Some(OUT));
        let c = check(&report, "source_hash");
        assert_eq!(c.status, CheckStatus::Skipped);
        assert!(c.detail.contains("carries no source hash"));
        assert!(report.ok());
    }

    #[test]
    fn missing_source_file_skips_source_hash() {
        let report = verify_export(&fixture_log(), &fixture_recipe(), None, Some(OUT));
        let c = check(&report, "source_hash");
        assert_eq!(c.status, CheckStatus::Skipped);
        assert!(c.detail.contains("not found"));
        assert!(report.ok());
    }

    #[test]
    fn sidecar_disagreement_about_source_fails() {
        let mut log = fixture_log();
        log.source_sha256 = OTHER.to_string();
        let report = verify_export(&log, &fixture_recipe(), Some(SRC), Some(OUT));
        let c = check(&report, "source_hash");
        assert_eq!(c.status, CheckStatus::Fail);
        assert!(c.detail.contains("disagree"));
        assert!(!report.ok());
    }

    #[test]
    fn changed_source_fails_source_hash() {
        let report = verify_export(&fixture_log(), &fixture_recipe(), Some(OTHER), Some(OUT));
        let c = check(&report, "source_hash");
        assert_eq!(c.status, CheckStatus::Fail);
        assert!(c.detail.contains(SRC), "detail names the recorded hash");
        assert!(c.detail.contains(OTHER), "detail names the actual hash");
    }

    #[test]
    fn preset_disagreement_fails_preset_match_only() {
        let mut recipe = fixture_recipe();
        recipe.export_preset = ExportPreset::Access;
        let report = verify_export(&fixture_log(), &recipe, Some(SRC), Some(OUT));
        assert_eq!(check(&report, "preset_match").status, CheckStatus::Fail);
        assert_eq!(report.failed(), 1);
    }

    #[test]
    fn schema_disagreement_fails_schema_match_only() {
        let mut recipe = fixture_recipe();
        recipe.schema_version = 2;
        let report = verify_export(&fixture_log(), &recipe, Some(SRC), Some(OUT));
        assert_eq!(check(&report, "schema_match").status, CheckStatus::Fail);
        assert_eq!(report.failed(), 1);
    }

    #[test]
    fn ok_semantics_skips_do_not_fail_fails_do() {
        let report = VerificationReport {
            checks: vec![
                Check::new("a", CheckStatus::Pass, String::new()),
                Check::new("b", CheckStatus::Skipped, String::new()),
            ],
        };
        assert!(report.ok());
        assert_eq!(report.passed(), 1);
        assert_eq!(report.skipped(), 1);
        let report = VerificationReport {
            checks: vec![
                Check::new("a", CheckStatus::Pass, String::new()),
                Check::new("b", CheckStatus::Fail, String::new()),
            ],
        };
        assert!(!report.ok());
        assert_eq!(report.failed(), 1);
    }
}
