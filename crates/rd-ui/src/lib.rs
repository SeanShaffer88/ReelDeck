//! ReelDeck user-interface state model.
//!
//! The shell is dense and keyboard-first: a queue on the left, the picture
//! and its measurements in the centre, the restoration chain on the
//! right, jobs along the bottom, and the proof beside the export. This
//! crate holds that shell's *state* and the presentation logic around
//! it — which modules exist, what each one can damage, when a chain must
//! be previewed, what the pre-export checks say — with no GUI framework
//! attached.
//!
//! Two things are deliberate:
//!
//! - **The filter string on screen is the string the executor runs.**
//!   [`ShellModel::chain_filter`] calls `rd-pipeline`'s planner rather
//!   than formatting anything itself. If the planner refuses a chain, the
//!   inspector shows the refusal.
//! - **No measurements are invented here.** A value the shell does not
//!   know is absent (`None`), and formatting renders it as `—`.
//!
//! The GUI framework decision is still open (see `docs/UI_DESIGN.md`):
//! the framework-neutral prototype under `crates/rd-ui/prototype/` and
//! this model are both candidates for the shipping shell, and keeping
//! the model here means that decision stays cheap.

#![forbid(unsafe_code)]

pub mod chain;
pub mod format;
pub mod prefs;
pub mod shortcuts;
pub mod stages;
pub mod timeline;

use serde::{Deserialize, Serialize};

use rd_core::{Operation, Recipe};

pub use chain::{
    add_module, export_is_ready, export_preflight, has_module, insert_step, move_step_down,
    move_step_up, parameter_watch_points, preview_required, remove_step, replace_step, reset_step,
    ChainEditError, CheckLevel, ExportFacts, ModuleId, PreflightCheck,
};
pub use format::{bytes_human, clock_time, duration, FrameRate};
pub use prefs::{Accent, Density, TimeDisplay, WorkspacePrefs};
pub use stages::{Stage, StageAccess};
pub use timeline::{MarkKind, MetricLane, RangeSelection, TimelineMark, TimelineModel};

impl Default for Stage {
    fn default() -> Self {
        Stage::Ingest
    }
}

impl Default for TimelineModel {
    fn default() -> Self {
        TimelineModel::new(0.0, FrameRate::FALLBACK)
    }
}

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

impl FixityStatus {
    /// The word shown in the queue. Text, not color, carries the state.
    pub fn label(self) -> &'static str {
        match self {
            FixityStatus::Pending => "Hashing",
            FixityStatus::Verified => "Match",
            FixityStatus::Failed => "Mismatch",
        }
    }
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

impl InspectStatus {
    pub fn label(self) -> &'static str {
        match self {
            InspectStatus::NotInspected => "Not inspected",
            InspectStatus::Inspecting => "Inspecting",
            InspectStatus::Inspected => "Inspected",
            InspectStatus::Failed => "Inspection failed",
        }
    }
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

    /// The label on the viewer's mode control.
    pub fn label(self) -> &'static str {
        match self {
            ViewerMode::Program => "Program",
            ViewerMode::Waveform => "Waveform",
            ViewerMode::Histogram => "Histogram",
            ViewerMode::Vectorscope => "Vectorscope",
            ViewerMode::FieldSplit => "Field split",
            ViewerMode::TemporalDifference => "Temporal difference",
        }
    }

    /// Why an operator would choose this mode.
    pub fn purpose(self) -> &'static str {
        match self {
            ViewerMode::Program => "The picture, for judging the image itself.",
            ViewerMode::Waveform => "Luma levels across the frame: clipping and crushing show here.",
            ViewerMode::Histogram => "The distribution of levels, for exposure decisions.",
            ViewerMode::Vectorscope => "Chroma hue and saturation, for cast and bleed.",
            ViewerMode::FieldSplit => "The two fields apart: combing and wrong field order are obvious.",
            ViewerMode::TemporalDifference => {
                "Change from the previous frame: dropouts and frozen spans stand out."
            }
        }
    }
}

/// Centre-viewer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewerState {
    pub current_frame: u64,
    pub mode: ViewerMode,
    /// True while the "before" side of a before/after comparison shows.
    pub showing_before: bool,
    /// Show the chain's result beside the untouched source.
    pub compare: bool,
    pub zoom_100: bool,
}

impl Default for ViewerState {
    fn default() -> Self {
        ViewerState {
            current_frame: 0,
            mode: ViewerMode::Program,
            showing_before: false,
            compare: false,
            zoom_100: false,
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

impl JobSummary {
    /// Progress written for display: `42%` when measured, `measuring…`
    /// when the tool has not reported a rate yet.
    pub fn percent_text(&self) -> String {
        match self.percent {
            Some(p) => format!("{p}%"),
            None => "measuring…".to_string(),
        }
    }
}

/// One row of the chain inspector: a step's summary line, its warnings,
/// and whether it is the one the gate would stop for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainLine {
    pub index: usize,
    pub module: ModuleId,
    pub summary: String,
    /// Consequences of the step's current parameter values.
    pub watch_points: Vec<String>,
    /// Set when this step is why the chain needs a preview first.
    pub preview_gate: Option<String>,
}

/// The whole shell layout state: queue (left), viewer (centre), chain
/// (right), jobs (bottom) — plus the workflow stage, preferences, and
/// the measured timeline the centre is drawn over.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShellModel {
    pub queue: Vec<QueueItem>,
    pub selected: Option<usize>,
    pub viewer: ViewerState,
    /// The chain being edited, when a recipe is loaded.
    #[serde(default)]
    pub recipe: Option<Recipe>,
    pub jobs: Vec<JobSummary>,
    /// Where the operator is in the workflow.
    #[serde(default)]
    pub stage: Stage,
    #[serde(default)]
    pub prefs: WorkspacePrefs,
    /// The measured strip behind the viewer. Rebuilt from an inspection;
    /// not persisted with the shell state.
    #[serde(skip)]
    pub timeline: TimelineModel,
    /// A segment of the current chain rendered successfully.
    #[serde(default)]
    pub previewed: bool,
    /// The operator overrode the preview gate; the export log records it.
    #[serde(default)]
    pub expert_override: bool,
    /// The finished output of the last successful export, when one exists.
    /// Prove stage and verification read this — a proof needs an output.
    #[serde(default)]
    pub exported_output: Option<String>,
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

    /// The chain inspector's rows: summaries, live warnings, and the
    /// preview gate, all derived from the current parameter values.
    pub fn chain_lines(&self) -> Vec<ChainLine> {
        let Some(recipe) = &self.recipe else {
            return Vec::new();
        };
        recipe
            .operations
            .iter()
            .enumerate()
            .map(|(index, op)| ChainLine {
                index,
                module: ModuleId::of(op),
                summary: op.summary(),
                watch_points: parameter_watch_points(op),
                preview_gate: chain::preview_required(&Recipe {
                    operations: vec![op.clone()],
                    ..recipe.clone()
                }),
            })
            .collect()
    }

    /// The exact filter chain for the loaded recipe, from the planner the
    /// executor uses. `None` when no recipe is loaded; an error when the
    /// planner refuses the chain, which the inspector displays verbatim.
    pub fn chain_filter(&self) -> Option<Result<String, String>> {
        self.recipe
            .as_ref()
            .map(|recipe| rd_pipeline::filter_chain(recipe).map_err(|e| e.to_string()))
    }

    /// Add a module to the loaded chain with conservative defaults.
    pub fn chain_add(&mut self, id: ModuleId) -> Result<usize, ChainEditError> {
        let recipe = self
            .recipe
            .as_mut()
            .ok_or(ChainEditError::InvalidParameters(
                "no chain is loaded".to_string(),
            ))?;
        let index = add_module(recipe, id)?;
        self.previewed = false;
        Ok(index)
    }

    /// Remove a step; the removed operation is returned so the shell can
    /// offer an undo that restores its exact parameters.
    pub fn chain_remove(&mut self, index: usize) -> Result<Operation, ChainEditError> {
        let recipe = self
            .recipe
            .as_mut()
            .ok_or(ChainEditError::InvalidParameters(
                "no chain is loaded".to_string(),
            ))?;
        let removed = remove_step(recipe, index)?;
        self.previewed = false;
        Ok(removed)
    }

    /// Replace a step's parameters; refused values leave the chain
    /// untouched.
    pub fn chain_replace(&mut self, index: usize, op: Operation) -> Result<(), ChainEditError> {
        let recipe = self
            .recipe
            .as_mut()
            .ok_or(ChainEditError::InvalidParameters(
                "no chain is loaded".to_string(),
            ))?;
        replace_step(recipe, index, op)?;
        self.previewed = false;
        Ok(())
    }

    /// Move a step earlier/later. Order is part of the recipe, so any
    /// reorder invalidates a preview of the previous order.
    pub fn chain_move(&mut self, index: usize, earlier: bool) -> bool {
        let Some(recipe) = self.recipe.as_mut() else {
            return false;
        };
        let moved = if earlier {
            move_step_up(recipe, index)
        } else {
            move_step_down(recipe, index)
        };
        if moved {
            self.previewed = false;
        }
        moved
    }

    /// Whether a stage may be entered, given what the shell knows.
    pub fn stage_access(&self, stage: Stage) -> StageAccess {
        stage.access(
            self.selected_item().is_some(),
            self.selected_item()
                .is_some_and(|i| i.inspect == InspectStatus::Inspected),
            self.recipe
                .as_ref()
                .map(|r| r.operations.len())
                .unwrap_or(0),
            self.previewed,
            self.exported_output.is_some(),
        )
    }

    /// The preview gate for the loaded chain, if any.
    pub fn preview_gate(&self) -> Option<String> {
        self.recipe.as_ref().and_then(preview_required)
    }

    /// Whether an export may run without an override: a preview of this
    /// chain exists, or the operator has recorded an override.
    pub fn export_gate_open(&self) -> bool {
        self.preview_gate().is_none() || self.previewed || self.expert_override
    }

    /// What the export panel checks before rendering, filled with the
    /// facts the shell knows. The app adds destination paths, free space,
    /// and an output-size estimate — facts only the filesystem has.
    pub fn export_facts(&self) -> ExportFacts {
        let chain_len = self
            .recipe
            .as_ref()
            .map(|r| r.operations.len())
            .unwrap_or(0);
        let chain_error = self
            .chain_filter()
            .and_then(|r| r.err())
            .filter(|_| chain_len > 0);
        ExportFacts {
            chain_len,
            chain_error,
            previewed: self.previewed,
            expert_override: self.expert_override,
            source_pinned: self
                .recipe
                .as_ref()
                .is_some_and(|r| r.source_sha256.is_some()),
            ..Default::default()
        }
    }

    pub fn export_checks(&self) -> Vec<PreflightCheck> {
        export_preflight(&self.export_facts())
    }

    /// Step the viewer one frame in either direction (saturating at 0)
    /// and keep the timeline playhead with it.
    pub fn step_frame(&mut self, delta: i64) {
        if delta >= 0 {
            self.viewer.current_frame = self.viewer.current_frame.saturating_add(delta as u64);
        } else {
            self.viewer.current_frame = self
                .viewer
                .current_frame
                .saturating_sub(delta.unsigned_abs());
        }
        self.timeline.seek_secs(
            self.timeline
                .rate
                .secs_for_frame(self.viewer.current_frame),
        );
    }

    /// Seek both the viewer and the playhead from a timeline click.
    pub fn seek_secs(&mut self, secs: f64) {
        self.timeline.seek_secs(secs);
        self.viewer.current_frame = self.timeline.frame();
    }

    pub fn cycle_viewer_mode(&mut self) {
        self.viewer.mode = self.viewer.mode.next();
    }

    /// The operator's time display, for the viewer readout.
    pub fn position_text(&self) -> String {
        format::position(
            self.viewer.current_frame,
            self.timeline.rate.secs_for_frame(self.viewer.current_frame),
            self.timeline.rate,
            self.prefs.time_display,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_core::{DeinterlaceEngine, DenoiseEngine, ExportPreset, FieldOrder, Operation, OutputRate};
    use rd_analyze::{Anomaly, FrameStats};

    fn chain_model() -> ShellModel {
        let mut shell = ShellModel::new();
        shell.add_to_queue("capture_1987.mkv");
        shell.queue[0].fixity = FixityStatus::Verified;
        shell.queue[0].inspect = InspectStatus::Inspected;
        shell.recipe = Some(
            Recipe::new("capture_1987.mkv", ExportPreset::PreservationMaster).push(
                Operation::Deinterlace {
                    engine: DeinterlaceEngine::Bwdif,
                    field_order: FieldOrder::TopFieldFirst,
                    output_rate: OutputRate::SendField,
                    only_flagged: false,
                },
            ),
        );
        shell
    }

    #[test]
    fn empty_shell_has_no_selection_and_no_chain() {
        let shell = ShellModel::new();
        assert!(shell.selected_item().is_none());
        assert!(shell.chain_summary().is_empty());
        assert_eq!(shell.viewer.mode, ViewerMode::Program);
        assert_eq!(shell.stage, Stage::Ingest);
        assert_eq!(shell.prefs, WorkspacePrefs::default());
        assert!(shell.chain_filter().is_none());
        assert!(!shell.export_gate_open());
        assert!(shell.chain_add(ModuleId::Denoise).is_err());
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
        let shell = chain_model();
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
        // Stepping keeps the playhead with the picture.
        assert!((shell.timeline.playhead_secs - 12.0 / 25.0).abs() < 1e-9);
    }

    #[test]
    fn mode_labels_and_purposes_are_present_for_every_mode() {
        for mode in ViewerMode::ALL {
            assert!(!mode.label().is_empty());
            assert!(!mode.purpose().is_empty());
        }
        assert_eq!(FixityStatus::Verified.label(), "Match");
        assert_eq!(InspectStatus::NotInspected.label(), "Not inspected");
    }

    #[test]
    fn filter_string_comes_from_the_planner() {
        let shell = chain_model();
        let chain = shell
            .chain_filter()
            .expect("recipe loaded")
            .expect("plans");
        assert_eq!(chain, "bwdif=mode=send_field:parity=tff:deint=all");
        // And it matches what the pipeline would export.
        let plan = rd_pipeline::export_plan(
            shell.recipe.as_ref().expect("recipe"),
            "capture_1987.mkv",
            "out.mkv",
        )
        .expect("plan");
        assert_eq!(plan.filter_chain, chain);
    }

    #[test]
    fn planner_refusal_is_surfaced_not_hidden() {
        let mut shell = chain_model();
        shell.recipe = Some(
            Recipe::new("capture_1987.mkv", ExportPreset::PreservationMaster).push(
                Operation::Deinterlace {
                    engine: DeinterlaceEngine::Qtgmc,
                    field_order: FieldOrder::Auto,
                    output_rate: OutputRate::SendFrame,
                    only_flagged: false,
                },
            ),
        );
        let err = shell
            .chain_filter()
            .expect("recipe loaded")
            .expect_err("qtgmc is refused");
        assert!(err.contains("VapourSynth"));
        let checks = shell.export_checks();
        assert_eq!(checks[0].id, "chain");
        assert_eq!(checks[0].level, CheckLevel::Blocked);
    }

    #[test]
    fn chain_editing_clears_the_preview_and_reports_warnings() {
        let mut shell = chain_model();
        assert!(shell.chain_add(ModuleId::Denoise).is_ok());
        assert_eq!(shell.chain_lines().len(), 2);
        assert_eq!(shell.chain_lines()[0].index, 0);
        assert_eq!(shell.chain_lines()[1].module, ModuleId::Denoise);
        // The send_field step carries its doubled-frame warning.
        assert!(shell.chain_lines()[0]
            .watch_points
            .iter()
            .any(|w| w.contains("twice the frames")));

        shell.previewed = true;
        assert!(shell.chain_move(1, true));
        assert!(!shell.previewed, "a reorder invalidates the preview");
        assert_eq!(shell.chain_lines()[0].module, ModuleId::Denoise);

        let removed = shell.chain_remove(0).expect("remove");
        assert_eq!(ModuleId::of(&removed), ModuleId::Denoise);
        assert!(shell.chain_remove(9).is_err());
    }

    #[test]
    fn aggressive_chain_opens_or_closes_the_export_gate() {
        let mut shell = chain_model();
        // Gentle defaults: no gate.
        assert!(shell.preview_gate().is_none());
        assert!(shell.export_gate_open());

        shell
            .chain_replace(
                0,
                Operation::Denoise {
                    engine: DenoiseEngine::Hqdn3d,
                    luma: 4.0,
                    chroma: 3.0,
                    temporal: 12.0,
                },
            )
            .expect("replace");
        assert!(shell.preview_gate().is_some());
        assert!(!shell.export_gate_open());
        assert!(!shell.export_checks().iter().all(|c| c.level != CheckLevel::Blocked));

        shell.previewed = true;
        assert!(shell.export_gate_open());
        shell.previewed = false;
        shell.expert_override = true;
        assert!(shell.export_gate_open());

        // A refused edit leaves the gate and the chain alone.
        let before = shell.recipe.clone();
        assert!(shell
            .chain_replace(
                0,
                Operation::Denoise {
                    engine: DenoiseEngine::Hqdn3d,
                    luma: 99.0,
                    chroma: 1.0,
                    temporal: 1.0,
                },
            )
            .is_err());
        assert_eq!(shell.recipe, before);
    }

    #[test]
    fn stage_access_follows_the_evidence() {
        let mut shell = ShellModel::new();
        assert!(shell.stage_access(Stage::Ingest).available);
        assert!(!shell.stage_access(Stage::Inspect).available);

        shell.add_to_queue("a.mkv");
        assert!(shell.stage_access(Stage::Inspect).available);
        assert!(!shell.stage_access(Stage::Decide).available, "inspection first");

        shell.queue[0].inspect = InspectStatus::Inspected;
        shell.recipe = Some(
            Recipe::new("a.mkv", ExportPreset::Access)
                .push(ModuleId::Denoise.default_operation()),
        );
        assert!(shell.stage_access(Stage::Decide).available);
        assert!(shell.stage_access(Stage::Preview).available);
        assert!(!shell.stage_access(Stage::Export).available, "preview first");
        shell.previewed = true;
        assert!(shell.stage_access(Stage::Export).available);
        assert!(!shell.stage_access(Stage::Prove).available, "no output yet");
        shell.exported_output = Some("a.restored.mkv".to_string());
        assert!(shell.stage_access(Stage::Prove).available);
        assert!(shell.stage_access(Stage::Prove).blocked_by.is_none());
    }

    #[test]
    fn export_facts_carry_what_the_model_knows() {
        let shell = chain_model();
        let facts = shell.export_facts();
        assert_eq!(facts.chain_len, 1);
        assert!(facts.chain_error.is_none());
        assert!(!facts.previewed);
        assert!(!facts.source_pinned, "the sample recipe pins no hash");

        let mut pinned = chain_model();
        pinned.recipe.as_mut().expect("recipe").source_sha256 = Some("a".repeat(64));
        assert!(pinned.export_facts().source_pinned);
        let checks = pinned.export_checks();
        assert_eq!(
            checks.iter().find(|c| c.id == "fixity").expect("check").level,
            CheckLevel::Pass
        );
        assert!(!shell.expert_override);
        assert!(shell.exported_output.is_none());
    }

    #[test]
    fn timeline_inspection_and_position_readout_agree() {
        let mut shell = chain_model();
        shell.timeline = TimelineModel::new(120.0, FrameRate::parse("25/1").expect("rate"))
            .with_inspection(
                &(0..500)
                    .map(|i| FrameStats {
                        frame: i,
                        pts_time: i as f64 / 25.0,
                        y_min: 16.0,
                        y_avg: 90.0,
                        y_max: 235.0,
                        u_avg: 128.0,
                        v_avg: 127.0,
                        sat_avg: 8.0,
                        tout: if i == 300 { 0.8 } else { 0.01 },
                        brng: 0.0,
                    })
                    .collect::<Vec<_>>(),
                &[Anomaly::TemporalOutlier {
                    frame: 300,
                    pts_time: 12.0,
                    tout: 0.8,
                }],
                0.05,
            );
        assert_eq!(shell.timeline.lanes.len(), 4);
        assert_eq!(shell.timeline.marks.len(), 1);
        shell.seek_secs(12.0);
        assert_eq!(shell.viewer.current_frame, 300);
        assert_eq!(shell.position_text(), "00:00:12:00");
        shell.prefs.time_display = TimeDisplay::Frames;
        assert_eq!(shell.position_text(), "300");
        let suggestion = shell.timeline.suggested_preview_range(10.0).expect("range");
        assert!((suggestion.in_secs - 7.0).abs() < 1e-9);
    }

    #[test]
    fn job_progress_is_text_or_honestly_unknown() {
        let measuring = JobSummary {
            label: "Preview · capture_1987.mkv".to_string(),
            percent: None,
            cancellable: true,
        };
        assert_eq!(measuring.percent_text(), "measuring…");
        let measured = JobSummary {
            percent: Some(42),
            ..measuring
        };
        assert_eq!(measured.percent_text(), "42%");
    }

    #[test]
    fn shell_state_round_trips_without_the_timeline() {
        let mut shell = chain_model();
        shell.stage = Stage::Decide;
        shell.prefs.accent = Accent::Teal;
        shell.timeline = TimelineModel::new(60.0, FrameRate::FALLBACK);
        let json = serde_json::to_string(&shell).expect("serialize");
        let back: ShellModel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.stage, Stage::Decide);
        assert_eq!(back.prefs.accent, Accent::Teal);
        // The measured strip is rebuilt from inspection, not persisted.
        assert_eq!(back.timeline, TimelineModel::default());
        assert_eq!(back.chain_summary(), shell.chain_summary());
    }
}
