//! The six workflow stages, as the shell presents them.
//!
//! ReelDeck is not a single "process" button, and the shell says so: the
//! top of the window names the stage the operator is in, what that stage
//! is for, and — for every stage after [`Stage::Ingest`] — what evidence
//! has to exist before its controls are usable. The stage order is the
//! product's contract (`docs/ARCHITECTURE_AND_UX_PLAN.md` §3), so it
//! lives here as data the UI renders rather than as prose in a manual.

use serde::{Deserialize, Serialize};

/// One stage of the restoration workflow, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Add sources; compute fixity; capture probe facts.
    Ingest,
    /// Measure the signal and read the findings.
    Inspect,
    /// Build the ordered chain of operations.
    Decide,
    /// Judge a segment before committing.
    Preview,
    /// Choose the export intent and render.
    Export,
    /// Verify the output against the proof it shipped with.
    Prove,
}

impl Stage {
    /// Every stage, in workflow order.
    pub const ALL: [Stage; 6] = [
        Stage::Ingest,
        Stage::Inspect,
        Stage::Decide,
        Stage::Preview,
        Stage::Export,
        Stage::Prove,
    ];

    /// Short label for the stage rail.
    pub fn label(self) -> &'static str {
        match self {
            Stage::Ingest => "Ingest",
            Stage::Inspect => "Inspect",
            Stage::Decide => "Decide",
            Stage::Preview => "Preview",
            Stage::Export => "Export",
            Stage::Prove => "Prove",
        }
    }

    /// One line: what this stage is for. Shown under the stage rail and
    /// as the panel's subtitle — never only in a tooltip.
    pub fn purpose(self) -> &'static str {
        match self {
            Stage::Ingest => "Add sources, compute fixity, and record what the file is.",
            Stage::Inspect => "Measure the signal. Findings are suggestions; you confirm them.",
            Stage::Decide => {
                "Build the ordered chain. Every step shows its damage note and its exact filter."
            }
            Stage::Preview => "Judge a segment — chosen where the footage is worst — before committing.",
            Stage::Export => {
                "Choose one intent. Every export writes its recipe and export log beside it."
            }
            Stage::Prove => "Check the output against its proof. A check without evidence is a skip.",
        }
    }

    /// The evidence that must exist before this stage's controls are
    /// usable. Empty for Ingest, which is where evidence starts.
    pub fn requires(self) -> &'static [&'static str] {
        match self {
            Stage::Ingest => &[],
            Stage::Inspect => &["a queued source"],
            Stage::Decide => &["an inspection report"],
            Stage::Preview => &["a chain with at least one operation"],
            Stage::Export => &["a validated chain", "a preview of it, or an expert override"],
            Stage::Prove => &["a finished output"],
        }
    }

    /// Index in [`Stage::ALL`], for the rail.
    pub fn index(self) -> usize {
        Stage::ALL
            .iter()
            .position(|s| *s == self)
            .expect("Stage::ALL names every stage")
    }

    pub fn next(self) -> Stage {
        let i = self.index();
        Stage::ALL.get(i + 1).copied().unwrap_or(self)
    }

    pub fn previous(self) -> Stage {
        let i = self.index();
        Stage::ALL.get(i.wrapping_sub(1)).copied().unwrap_or(self)
    }

    /// Keyboard hint for the stage rail: `Alt+1` … `Alt+6`.
    pub fn shortcut_hint(self) -> &'static str {
        match self {
            Stage::Ingest => "Alt+1",
            Stage::Inspect => "Alt+2",
            Stage::Decide => "Alt+3",
            Stage::Preview => "Alt+4",
            Stage::Export => "Alt+5",
            Stage::Prove => "Alt+6",
        }
    }
}

/// Whether a stage's controls should be enabled, given what the shell
/// already knows. The blocking reason is a sentence the operator can act
/// on — never a bare disabled state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageAccess {
    pub available: bool,
    /// Why the stage is unavailable, and what to do about it.
    pub blocked_by: Option<String>,
}

impl StageAccess {
    pub fn open() -> Self {
        StageAccess {
            available: true,
            blocked_by: None,
        }
    }

    pub fn blocked(reason: impl Into<String>) -> Self {
        StageAccess {
            available: false,
            blocked_by: Some(reason.into()),
        }
    }
}

impl Stage {
    /// Gate one stage on facts the shell has. `has_source` — a source is
    /// queued; `inspected` — its inspection report exists; `chain_len` —
    /// the number of operations in the chain; `previewed` — a segment
    /// preview of the current chain finished successfully; `exported` —
    /// an output exists for the current chain.
    pub fn access(
        self,
        has_source: bool,
        inspected: bool,
        chain_len: usize,
        previewed: bool,
        exported: bool,
    ) -> StageAccess {
        match self {
            Stage::Ingest => StageAccess::open(),
            Stage::Inspect => {
                if has_source {
                    StageAccess::open()
                } else {
                    StageAccess::blocked("Add a source first: inspection measures a queued file.")
                }
            }
            Stage::Decide => {
                if !has_source {
                    StageAccess::blocked("Add a source first.")
                } else if !inspected {
                    StageAccess::blocked(
                        "Inspect the source first: the chain's defaults should be informed by its \
                         measurements, not guessed.",
                    )
                } else {
                    StageAccess::open()
                }
            }
            Stage::Preview => {
                if chain_len == 0 {
                    StageAccess::blocked("Add at least one operation to the chain to preview it.")
                } else {
                    StageAccess::open()
                }
            }
            Stage::Export => {
                if chain_len == 0 {
                    StageAccess::blocked("Nothing to export: the chain is empty.")
                } else if !previewed {
                    StageAccess::blocked(
                        "Preview this chain once before exporting — or record an expert override \
                         in the export panel.",
                    )
                } else {
                    StageAccess::open()
                }
            }
            Stage::Prove => {
                if exported {
                    StageAccess::open()
                } else {
                    StageAccess::blocked("Prove opens once an export has finished.")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_ordered_and_bounded() {
        assert_eq!(Stage::ALL.len(), 6);
        assert_eq!(Stage::Ingest.next(), Stage::Inspect);
        assert_eq!(Stage::Inspect.previous(), Stage::Ingest);
        assert_eq!(Stage::Ingest.previous(), Stage::Ingest);
        assert_eq!(Stage::Prove.next(), Stage::Prove);
        for (i, stage) in Stage::ALL.iter().enumerate() {
            assert_eq!(stage.index(), i, "{stage:?} index");
            assert_eq!(stage.shortcut_hint(), format!("Alt+{}", i + 1));
            assert!(!stage.purpose().is_empty());
            assert!(!stage.label().is_empty());
        }
    }

    #[test]
    fn ingest_is_always_open_and_later_stages_gate_on_evidence() {
        assert!(Stage::Ingest.access(false, false, 0, false, false).available);
        for stage in Stage::ALL[1..].iter() {
            let access = stage.access(false, false, 0, false, false);
            assert!(!access.available, "{stage:?} must be blocked with no source");
            assert!(
                access.blocked_by.as_deref().is_some_and(|s| !s.is_empty()),
                "{stage:?} must say why it is blocked"
            );
        }
    }

    #[test]
    fn each_stage_opens_when_its_evidence_exists() {
        assert!(Stage::Inspect.access(true, false, 0, false, false).available);
        // A source that has not been inspected still cannot be decided on.
        assert!(!Stage::Decide.access(true, false, 1, false, false).available);
        assert!(Stage::Decide.access(true, true, 1, false, false).available);
        assert!(!Stage::Preview.access(true, true, 0, false, false).available);
        assert!(Stage::Preview.access(true, true, 1, false, false).available);
        // Export needs a preview (or an override recorded elsewhere).
        assert!(!Stage::Export.access(true, true, 1, false, false).available);
        assert!(Stage::Export.access(true, true, 1, true, false).available);
        assert!(Stage::Prove.access(true, true, 1, true, true).available);
        // An output with nothing exported yet has no proof to check.
        assert!(!Stage::Prove.access(true, true, 1, true, false).available);
    }

    #[test]
    fn requirements_are_named_per_stage() {
        assert!(Stage::Ingest.requires().is_empty());
        assert_eq!(Stage::Preview.requires().len(), 1);
        assert_eq!(
            Stage::Preview.requires(),
            ["a chain with at least one operation"].as_slice()
        );
        assert_eq!(Stage::Prove.requires(), ["a finished output"].as_slice());
    }

    #[test]
    fn stage_serde_is_snake_case() {
        let json = serde_json::to_string(&Stage::Preview).expect("serialize");
        assert_eq!(json, "\"preview\"");
        let back: Stage = serde_json::from_str("\"prove\"").expect("deserialize");
        assert_eq!(back, Stage::Prove);
    }
}
