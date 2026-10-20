//! ReelDeck user-interface state model.
//!
//! Skeleton: this crate holds the *state* the future GUI renders — the
//! project queue, the viewer mode, the restoration chain, and the job
//! list — with no GUI framework code at all.
//!
//! **The GUI framework decision is deliberately deferred** (blueprint
//! §4 / `docs/ARCHITECTURE_AND_UX_PLAN.md`): Tauri 2 vs a native Windows
//! toolkit is still an open trade-off to settle against current platform
//! guidance before any UI code is written. Keeping the model here means
//! that decision does not touch `rd-core`, `rd-analyze`, or
//! `rd-pipeline` when it lands.
//!
//! This crate holds no business logic: it describes state and derives
//! display summaries from `rd-core` types only.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use rd_core::{Operation, Recipe};

/// Fixity state of a queued source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixityStatus {
    /// SHA-256 not computed yet.
    Pending,
    /// SHA-256 computed and matches the value recorded at ingest.
    Verified,
    /// Hashing failed or the file changed since ingest.
    Failed,
}

/// Inspection state of a queued source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectStatus {
    NotInspected,
    Inspecting,
    Inspected,
    Failed,
}

/// One file in the project queue (left panel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueItem {
    pub name: String,
    pub fixity: FixityStatus,
    pub inspect: InspectStatus,
}

impl QueueItem {
    pub fn new(name: impl Into<String>) -> Self {
        QueueItem {
            name: name.into(),
            fixity: FixityStatus::Pending,
            inspect: InspectStatus::NotInspected,
        }
    }
}

/// What the centre viewer is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerMode {
    /// The picture itself (before/after capable).
    Program,
    Waveform,
    Histogram,
    Vectorscope,
    /// Top and bottom fields side by side, for interlacing checks.
    FieldSplit,
    /// Frame-to-frame difference view.
    TemporalDifference,
}

impl ViewerMode {
    /// All modes, in the order the viewer cycles through them.
    pub const ALL: [ViewerMode; 6] = [
        ViewerMode::Program,
        ViewerMode::Waveform,
        ViewerMode::Histogram,
        ViewerMode::Vectorscope,
        ViewerMode::FieldSplit,
        ViewerMode::TemporalDifference,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }
}

/// Centre-viewer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerState {
    pub current_frame: u64,
    pub mode: ViewerMode,
    /// True while the "before" side of a before/after comparison shows.
    pub showing_before: bool,
}

impl Default for ViewerState {
    fn default() -> Self {
        ViewerState {
            current_frame: 0,
            mode: ViewerMode::Program,
            showing_before: false,
        }
    }
}

/// One background job in the bottom panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSummary {
    pub label: String,
    /// Measured progress 0..=100; `None` while the rate is not yet known.
    /// Never a fabricated estimate.
    pub percent: Option<u8>,
    pub cancellable: bool,
}

/// The whole shell layout state: queue (left), viewer (centre), chain
/// (right), jobs (bottom) — see the plan, §UI shape.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShellModel {
    pub queue: Vec<QueueItem>,
    pub selected: Option<usize>,
    pub viewer: ViewerState,
    /// The chain being edited, when a recipe is loaded.
    #[serde(default)]
    pub recipe: Option<Recipe>,
    pub jobs: Vec<JobSummary>,
}

impl ShellModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file to the queue and select it if nothing is selected.
    pub fn add_to_queue(&mut self, name: impl Into<String>) {
        self.queue.push(QueueItem::new(name));
        if self.selected.is_none() {
            self.selected = Some(self.queue.len() - 1);
        }
    }

    /// Select a queue row; out-of-range selections are ignored rather
    /// than panicking, because the list can shrink under the UI.
    pub fn select(&mut self, index: usize) {
        if index < self.queue.len() {
            self.selected = Some(index);
        }
    }

    pub fn selected_item(&self) -> Option<&QueueItem> {
        self.selected.and_then(|i| self.queue.get(i))
    }

    /// One-line summaries of the loaded chain, for the right panel.
    pub fn chain_summary(&self) -> Vec<String> {
        match &self.recipe {
            Some(recipe) => recipe.operations.iter().map(Operation::summary).collect(),
            None => Vec::new(),
        }
    }

    /// Step the viewer one frame in either direction (saturating at 0).
    pub fn step_frame(&mut self, delta: i64) {
        if delta >= 0 {
            self.viewer.current_frame = self.viewer.current_frame.saturating_add(delta as u64);
        } else {
            self.viewer.current_frame = self
                .viewer
                .current_frame
                .saturating_sub(delta.unsigned_abs());
        }
    }

    pub fn cycle_viewer_mode(&mut self) {
        self.viewer.mode = self.viewer.mode.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_core::{ExportPreset, FieldOrder, Operation, OutputRate};

    #[test]
    fn empty_shell_has_no_selection_and_no_chain() {
        let shell = ShellModel::new();
        assert!(shell.selected_item().is_none());
        assert!(shell.chain_summary().is_empty());
        assert_eq!(shell.viewer.mode, ViewerMode::Program);
    }

    #[test]
    fn adding_selects_first_and_selection_is_bounded() {
        let mut shell = ShellModel::new();
        shell.add_to_queue("a.mkv");
        shell.add_to_queue("b.mkv");
        assert_eq!(
            shell.selected_item().map(|i| i.name.as_str()),
            Some("a.mkv")
        );
        shell.select(1);
        assert_eq!(
            shell.selected_item().map(|i| i.name.as_str()),
            Some("b.mkv")
        );
        shell.select(99); // ignored, must not panic or clear
        assert_eq!(
            shell.selected_item().map(|i| i.name.as_str()),
            Some("b.mkv")
        );
    }

    #[test]
    fn chain_summary_uses_operation_summaries() {
        let mut shell = ShellModel::new();
        shell.recipe = Some(Recipe::new("a.mkv", ExportPreset::PreservationMaster).push(
            Operation::Deinterlace {
                engine: rd_core::DeinterlaceEngine::Bwdif,
                field_order: FieldOrder::TopFieldFirst,
                output_rate: OutputRate::SendField,
                only_flagged: false,
            },
        ));
        assert_eq!(
            shell.chain_summary(),
            vec!["Deinterlace · bwdif · tff · send_field".to_string()]
        );
    }

    #[test]
    fn viewer_cycles_through_all_modes_and_frame_steps_saturate() {
        let mut shell = ShellModel::new();
        let mut seen = vec![shell.viewer.mode];
        for _ in 0..6 {
            shell.cycle_viewer_mode();
            seen.push(shell.viewer.mode);
        }
        assert_eq!(seen[6], ViewerMode::Program); // full cycle returns
        shell.step_frame(-5);
        assert_eq!(shell.viewer.current_frame, 0);
        shell.step_frame(12);
        assert_eq!(shell.viewer.current_frame, 12);
    }
}
