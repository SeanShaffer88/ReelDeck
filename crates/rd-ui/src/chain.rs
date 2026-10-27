//! Chain editing: the module catalog, parameter guards, and the
//! pre-export checks.
//!
//! This is presentation logic over `rd-core`'s recipe model. It decides
//! what the inspector offers, which copy sits next to which control, and
//! when a chain is aggressive enough that a preview must come first. It
//! never builds a filter string — that is `rd-pipeline`'s job, so the
//! string on screen is the string the executor runs (see
//! [`ShellModel::filter_preview`]).
//!
//! Three rules shape every function here:
//!
//! 1. Every control says what it does and what it can damage.
//! 2. Cost labels are relative (`slower`), never absolute seconds, until
//!    the test corpus measures them.
//! 3. A warning names the consequence and the recovery, not just "careful".

use rd_core::{DeinterlaceEngine, DenoiseEngine, FieldOrder, Operation, OutputRate, Recipe};

/// One module in the restoration chain, identified by kind. A chain holds
/// at most one of each; order between kinds is free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleId {
    Deinterlace,
    Stabilize,
    Denoise,
    Finish,
}

impl ModuleId {
    /// Catalog order — the order the palette lists them in, which is also
    /// the order they appear in a well-formed chain.
    pub const ALL: [ModuleId; 4] = [
        ModuleId::Deinterlace,
        ModuleId::Stabilize,
        ModuleId::Denoise,
        ModuleId::Finish,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ModuleId::Deinterlace => "Deinterlace",
            ModuleId::Stabilize => "Stabilize",
            ModuleId::Denoise => "Denoise",
            ModuleId::Finish => "Finish",
        }
    }

    /// One sentence: what the module does. Shown collapsed in the chain.
    pub fn blurb(self) -> &'static str {
        match self {
            ModuleId::Deinterlace => {
                "Rebuild one full picture from each interlaced pair, so motion stops combing."
            }
            ModuleId::Stabilize => {
                "Smooth camera movement over time. It cannot recover what was never captured."
            }
            ModuleId::Denoise => "Reduce grain and low-light noise, luma and chroma separately.",
            ModuleId::Finish => "Crop blanking, scale, gentle level correction, deflicker.",
        }
    }

    /// What the module can damage, stated plainly. These strings are
    /// rendered next to the module's controls and are never shortened.
    pub fn damage_notes(self) -> &'static [&'static str] {
        match self {
            ModuleId::Deinterlace => &[
                "The wrong field order does not fail — it judders. Check a pan in the preview.",
                "One frame per field doubles the frame count and the master's size.",
            ],
            ModuleId::Stabilize => &[
                "Smoothing cannot repair sync loss, rolling-shutter wobble, or missing signal.",
                "Zoom hides the moving border by cropping picture area. The cost is shown before rendering.",
            ],
            ModuleId::Denoise => &[
                "High temporal strength smears motion and erases film grain first.",
                "Denoising is irreversible detail loss. A preservation export needs a light touch.",
            ],
            ModuleId::Finish => &[
                "Scaling resamples: the scaler used is recorded, and upscaling invents nothing.",
                "Levels change can clip highlights. The out-of-range ratio is in Inspect before you move it.",
            ],
        }
    }

    /// Relative cost — a word, not a fabricated duration.
    pub fn relative_cost(self) -> &'static str {
        match self {
            ModuleId::Deinterlace => "fast",
            ModuleId::Stabilize => "slowest (two passes)",
            ModuleId::Denoise => "slow",
            ModuleId::Finish => "fast",
        }
    }

    pub fn of(op: &Operation) -> ModuleId {
        match op {
            Operation::Deinterlace { .. } => ModuleId::Deinterlace,
            Operation::Stabilize { .. } => ModuleId::Stabilize,
            Operation::Denoise { .. } => ModuleId::Denoise,
            Operation::Finish { .. } => ModuleId::Finish,
        }
    }

    /// The conservative default for this module. Defaults are chosen so a
    /// first chain does the least damage: motion-adaptive deinterlace that
    /// keeps motion, stabilization that holds the frame without zooming,
    /// gentle denoise, and a Finish step that changes nothing until the
    /// operator ticks a box.
    pub fn default_operation(self) -> Operation {
        match self {
            ModuleId::Deinterlace => Operation::Deinterlace {
                engine: DeinterlaceEngine::Bwdif,
                field_order: FieldOrder::Auto,
                output_rate: OutputRate::SendFrame,
                only_flagged: false,
            },
            ModuleId::Stabilize => Operation::Stabilize {
                smoothing: 15,
                zoom_percent: 0.0,
                tripod: false,
            },
            ModuleId::Denoise => Operation::Denoise {
                engine: DenoiseEngine::Hqdn3d,
                luma: 2.0,
                chroma: 2.0,
                temporal: 3.0,
            },
            ModuleId::Finish => Operation::Finish {
                crop: None,
                scale_width: None,
                scale_height: None,
                deflicker: false,
                normalize_levels: false,
            },
        }
    }
}

/// Why a chain edit was refused. Each variant carries what the operator
/// needs to understand the refusal — the inspector shows it inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainEditError {
    /// The module is already in the chain.
    AlreadyPresent(ModuleId),
    /// No module of that kind to remove or reset.
    NotPresent(ModuleId),
    /// An index past the end of the chain (the list shrank under the UI).
    NoSuchStep(usize),
    /// The step's parameters are out of their allowed range.
    InvalidParameters(String),
}

impl ChainEditError {
    pub fn message(&self) -> String {
        match self {
            ChainEditError::AlreadyPresent(id) => format!(
                "{} is already in the chain — adjust it there, or remove it first.",
                id.label()
            ),
            ChainEditError::NotPresent(id) => {
                format!("{} is not in the chain.", id.label())
            }
            ChainEditError::NoSuchStep(i) => {
                format!("Step {} no longer exists; the chain changed.", i + 1)
            }
            ChainEditError::InvalidParameters(detail) => {
                format!("These parameters cannot be planned: {detail}")
            }
        }
    }
}

/// Validate one operation by planning it inside the recipe's context.
fn validate_operation(recipe: &Recipe, op: &Operation) -> Result<(), ChainEditError> {
    let mut probe = recipe.clone();
    probe.operations = vec![op.clone()];
    probe
        .validate()
        .map_err(|e| ChainEditError::InvalidParameters(e.to_string()))
}

/// Add a module with its conservative defaults; returns its index.
///
/// The new step enters at its **canonical position** — deinterlace before
/// stabilize before denoise before finish — rather than at the end, so the
/// chain a first-time operator builds is the order that works. The
/// operator can move it afterwards; order is part of the recipe.
pub fn add_module(recipe: &mut Recipe, id: ModuleId) -> Result<usize, ChainEditError> {
    if recipe.operations.iter().any(|op| ModuleId::of(op) == id) {
        return Err(ChainEditError::AlreadyPresent(id));
    }
    let rank = |m: ModuleId| ModuleId::ALL.iter().position(|x| *x == m).unwrap_or(usize::MAX);
    let index = recipe
        .operations
        .iter()
        .position(|op| rank(ModuleId::of(op)) > rank(id))
        .unwrap_or(recipe.operations.len());
    recipe.operations.insert(index, id.default_operation());
    Ok(index)
}

/// Remove a step by index; returns the removed operation so an undo can
/// put it back exactly where it was.
pub fn remove_step(recipe: &mut Recipe, index: usize) -> Result<Operation, ChainEditError> {
    if index >= recipe.operations.len() {
        return Err(ChainEditError::NoSuchStep(index));
    }
    Ok(recipe.operations.remove(index))
}

/// Put a step back at an index (undo of [`remove_step`]).
pub fn insert_step(
    recipe: &mut Recipe,
    index: usize,
    op: Operation,
) -> Result<usize, ChainEditError> {
    if index > recipe.operations.len() {
        return Err(ChainEditError::NoSuchStep(index));
    }
    if recipe.operations.iter().any(|o| ModuleId::of(o) == ModuleId::of(&op)) {
        return Err(ChainEditError::AlreadyPresent(ModuleId::of(&op)));
    }
    recipe.operations.insert(index, op);
    Ok(index)
}

/// Move a step earlier. At the top, the chain is unchanged and `false` is
/// returned so the UI can keep the move button disabled rather than
/// silently doing nothing.
pub fn move_step_up(recipe: &mut Recipe, index: usize) -> bool {
    if index == 0 || index >= recipe.operations.len() {
        return false;
    }
    recipe.operations.swap(index, index - 1);
    true
}

/// Move a step later; see [`move_step_up`].
pub fn move_step_down(recipe: &mut Recipe, index: usize) -> bool {
    if index + 1 >= recipe.operations.len() {
        return false;
    }
    recipe.operations.swap(index, index + 1);
    true
}

/// Replace one step's parameters, refusing values the planner would
/// reject — so the inspector can never show a plan that cannot run.
pub fn replace_step(
    recipe: &mut Recipe,
    index: usize,
    op: Operation,
) -> Result<(), ChainEditError> {
    if index >= recipe.operations.len() {
        return Err(ChainEditError::NoSuchStep(index));
    }
    validate_operation(recipe, &op)?;
    // A step keeps its module kind: swapping a deinterlace for a second
    // denoise would silently create a chain the inspector cannot render.
    let kind = ModuleId::of(&op);
    if has_module(recipe, kind) && ModuleId::of(&recipe.operations[index]) != kind {
        return Err(ChainEditError::AlreadyPresent(kind));
    }
    recipe.operations[index] = op;
    Ok(())
}

/// Restore one step to its module defaults.
pub fn reset_step(recipe: &mut Recipe, index: usize) -> Result<(), ChainEditError> {
    let id = recipe
        .operations
        .get(index)
        .map(ModuleId::of)
        .ok_or(ChainEditError::NoSuchStep(index))?;
    replace_step(recipe, index, id.default_operation())
}

/// True when the chain contains this module.
pub fn has_module(recipe: &Recipe, id: ModuleId) -> bool {
    recipe.operations.iter().any(|op| ModuleId::of(op) == id)
}

/// Why this chain must be previewed before it may be exported, or `None`
/// when its settings are conservative enough to run directly.
///
/// The rule comes from the plan (§3.4): an aggressive chain gets a
/// preview, or an expert override that the export log records.
pub fn preview_required(recipe: &Recipe) -> Option<String> {
    for op in &recipe.operations {
        if let Some(reason) = step_preview_reason(op) {
            return Some(reason);
        }
    }
    None
}

fn step_preview_reason(op: &Operation) -> Option<String> {
    match op {
        Operation::Deinterlace {
            engine: DeinterlaceEngine::Qtgmc,
            ..
        } => Some(
            "The archival deinterlace tier is slow and its motion handling differs from the \
             preview-grade engines; judge a segment first."
                .to_string(),
        ),
        Operation::Denoise {
            engine: DenoiseEngine::Nlmeans,
            ..
        } => Some(
            "Quality denoise removes detail no later step can restore; judge it on the worst \
             seconds first."
                .to_string(),
        ),
        Operation::Denoise { temporal, .. } if *temporal > 8.0 => Some(format!(
            "Temporal strength {temporal} smears motion and erases grain. Preview a moving shot \
             before committing."
        )),
        Operation::Stabilize { zoom_percent, .. } if *zoom_percent > 10.0 => Some(format!(
            "Zoom {zoom_percent:.1}% crops {:.0}% of the frame area to hide the border. \
             Confirm the framing in a preview.",
            (1.0 - 1.0 / (1.0 + zoom_percent / 100.0).powi(2)) * 100.0
        )),
        _ => None,
    }
}

/// Parameter-level warnings for the inspector, given the live values.
/// Each is a consequence, not a scold.
pub fn parameter_watch_points(op: &Operation) -> Vec<String> {
    let mut out = Vec::new();
    match op {
        Operation::Deinterlace {
            output_rate: OutputRate::SendField,
            ..
        } => out.push(
            "One frame per field: the output has twice the frames of the source (25i → 50p). \
             The master will be roughly twice the size and the render twice as long."
                .to_string(),
        ),
        Operation::Deinterlace {
            field_order: FieldOrder::Auto,
            ..
        } => out.push(
            "Auto follows the stream's field-order flag. When a capture's flag is wrong, force \
             the order the preview shows as correct."
                .to_string(),
        ),
        Operation::Stabilize { tripod: true, .. } => out.push(
            "Tripod mode holds every shot still, including deliberate camera moves. \
             It suits locked-off transfers, not handheld footage."
                .to_string(),
        ),
        Operation::Denoise { temporal, .. } if *temporal > 0.0 => out.push(
            "Temporal denoise compares neighbouring frames; fast motion and flashes can smear. \
             Zero it to leave grain alone."
                .to_string(),
        ),
        Operation::Finish {
            normalize_levels: true,
            ..
        } => out.push(
            "Level correction moves values that were out of broadcast range. \
             It clips by definition; a preservation master may prefer the untouched range."
                .to_string(),
        ),
        _ => {}
    }
    out
}

// ---------------------------------------------------------------------------
// Pre-export checks
// ---------------------------------------------------------------------------

/// Outcome of one preflight check. `Unknown` is a first-class state: a
/// fact the shell could not measure is never rendered as a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckLevel {
    Pass,
    Warn,
    Blocked,
    Unknown,
}

impl CheckLevel {
    /// The word the UI prints. Never color alone — the word is the
    /// primary signal, the color is reinforcement.
    pub fn label(self) -> &'static str {
        match self {
            CheckLevel::Pass => "OK",
            CheckLevel::Warn => "CHECK",
            CheckLevel::Blocked => "BLOCKED",
            CheckLevel::Unknown => "UNMEASURED",
        }
    }
}

/// One named check with its evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightCheck {
    /// Stable identifier for tests and for the UI's row keys.
    pub id: &'static str,
    pub label: &'static str,
    pub level: CheckLevel,
    pub detail: String,
}

/// Everything the export panel knows before a render starts. Optional
/// fields are genuinely optional facts — passing `None` yields
/// `Unmeasured`, not a guess.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportFacts {
    pub chain_len: usize,
    /// The planner's verdict on the chain, if it was planned.
    pub chain_error: Option<String>,
    pub previewed: bool,
    pub expert_override: bool,
    pub source_pinned: bool,
    pub output_exists: bool,
    pub sidecar_exists: bool,
    /// Free space on the destination volume, when measured.
    pub free_bytes: Option<u64>,
    /// Estimated output size, when an estimate exists.
    pub estimated_bytes: Option<u64>,
}

/// Run the export panel's checks, in the order the operator reads them:
/// the chain, the preview gate, the destination, the fixity pin, space.
pub fn export_preflight(facts: &ExportFacts) -> Vec<PreflightCheck> {
    let mut checks = Vec::with_capacity(6);

    checks.push(if facts.chain_len == 0 {
        PreflightCheck {
            id: "chain",
            label: "Chain",
            level: CheckLevel::Blocked,
            detail: "The chain is empty. Add at least one operation before exporting.".to_string(),
        }
    } else if let Some(err) = &facts.chain_error {
        PreflightCheck {
            id: "chain",
            label: "Chain plans",
            level: CheckLevel::Blocked,
            detail: format!("The planner refuses this chain: {err}"),
        }
    } else {
        PreflightCheck {
            id: "chain",
            label: "Chain plans",
            level: CheckLevel::Pass,
            detail: format!(
                "{} operation{} planned in order; the exact command is shown in the plan tab.",
                facts.chain_len,
                if facts.chain_len == 1 { "" } else { "s" }
            ),
        }
    });

    checks.push(if facts.previewed {
        PreflightCheck {
            id: "preview",
            label: "Segment preview",
            level: CheckLevel::Pass,
            detail: "A segment of this chain rendered and its proof was written.".to_string(),
        }
    } else if facts.expert_override {
        PreflightCheck {
            id: "preview",
            label: "Segment preview",
            level: CheckLevel::Warn,
            detail: "No preview. An expert override will be recorded in the export log so the \
                     decision is visible later."
                .to_string(),
        }
    } else {
        PreflightCheck {
            id: "preview",
            label: "Segment preview",
            level: CheckLevel::Blocked,
            detail: "Preview this chain on a segment first, or record the expert override in the \
                     export panel."
                .to_string(),
        }
    });

    checks.push(if facts.output_exists || facts.sidecar_exists {
        PreflightCheck {
            id: "destination",
            label: "Destination",
            level: CheckLevel::Blocked,
            detail: if facts.output_exists {
                "A file already exists at the output path. Outputs are always new files — \
                 choose another name."
                    .to_string()
            } else {
                "The proof sidecars for this output name already exist. Choose another output \
                 name; existing proof is never overwritten."
                    .to_string()
            },
        }
    } else {
        PreflightCheck {
            id: "destination",
            label: "Destination",
            level: CheckLevel::Pass,
            detail: "The output path is free; the recipe and export log will be written beside it."
                .to_string(),
        }
    });

    checks.push(if facts.source_pinned {
        PreflightCheck {
            id: "fixity",
            label: "Source fixity",
            level: CheckLevel::Pass,
            detail: "The recipe pins the source's SHA-256; the run refuses if the file no longer \
                     matches."
                .to_string(),
        }
    } else {
        PreflightCheck {
            id: "fixity",
            label: "Source fixity",
            level: CheckLevel::Warn,
            detail: "The recipe carries no source hash. The run will record the hash it measures, \
                     but nothing proves the file is the one you inspected. Pin the hash in Ingest \
                     to close that gap."
                .to_string(),
        }
    });

    checks.push(match (facts.free_bytes, facts.estimated_bytes) {
        (Some(free), Some(est)) => {
            let headroom = (est as f64 * 1.10).ceil() as u64;
            if free >= headroom {
                PreflightCheck {
                    id: "space",
                    label: "Disk space",
                    level: CheckLevel::Pass,
                    detail: format!(
                        "{} free; estimated output {} plus 10% headroom.",
                        crate::format::bytes_human(free),
                        crate::format::bytes_human(est)
                    ),
                }
            } else {
                PreflightCheck {
                    id: "space",
                    label: "Disk space",
                    level: CheckLevel::Blocked,
                    detail: format!(
                        "{} free, but the estimate is {} plus 10% headroom. Free space or choose \
                         another destination.",
                        crate::format::bytes_human(free),
                        crate::format::bytes_human(est)
                    ),
                }
            }
        }
        (Some(free), None) => PreflightCheck {
            id: "space",
            label: "Disk space",
            level: CheckLevel::Unknown,
            detail: format!(
                "{} free. The output size is not estimated for this chain — a lossless master can \
                 be several times the source, so keep generous headroom.",
                crate::format::bytes_human(free)
            ),
        },
        _ => PreflightCheck {
            id: "space",
            label: "Disk space",
            level: CheckLevel::Unknown,
            detail: "Free space was not measured for the destination. Measure it before a long \
                     render."
                .to_string(),
        },
    });

    checks
}

/// True when every check that can block is clear.
pub fn export_is_ready(checks: &[PreflightCheck]) -> bool {
    checks.iter().all(|c| c.level != CheckLevel::Blocked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_core::ExportPreset;

    fn recipe() -> Recipe {
        Recipe::new("capture.mkv", ExportPreset::PreservationMaster)
    }

    #[test]
    fn catalog_is_complete_and_defaults_are_conservative() {
        for id in ModuleId::ALL {
            assert!(!id.label().is_empty());
            assert!(!id.blurb().is_empty());
            assert!(!id.damage_notes().is_empty(), "{id:?} must state its damage");
            assert!(!id.relative_cost().is_empty());
            let op = id.default_operation();
            assert_eq!(ModuleId::of(&op), id);
            // A default operation is valid on its own.
            let mut r = recipe();
            r.operations.push(op);
            assert!(r.validate().is_ok(), "{id:?} default must validate");
        }
        // The quiet defaults: no zoom, no levels change, gentle denoise.
        match ModuleId::Stabilize.default_operation() {
            Operation::Stabilize { zoom_percent, .. } => assert_eq!(zoom_percent, 0.0),
            other => panic!("expected stabilize, got {other:?}"),
        }
        match ModuleId::Denoise.default_operation() {
            Operation::Denoise { temporal, .. } => assert!(temporal <= 3.0),
            other => panic!("expected denoise, got {other:?}"),
        }
        match ModuleId::Finish.default_operation() {
            Operation::Finish {
                normalize_levels,
                deflicker,
                ..
            } => {
                assert!(!normalize_levels);
                assert!(!deflicker);
            }
            other => panic!("expected finish, got {other:?}"),
        }
    }

    #[test]
    fn add_is_unique_and_lands_in_canonical_order() {
        let mut r = recipe();
        // Denoise first, then deinterlace: the deinterlace belongs *before*
        // the denoise, so it lands at index 0 even though it was added later.
        assert_eq!(add_module(&mut r, ModuleId::Denoise), Ok(0));
        assert_eq!(add_module(&mut r, ModuleId::Deinterlace), Ok(0));
        assert_eq!(r.operations.len(), 2);
        assert_eq!(ModuleId::of(&r.operations[0]), ModuleId::Deinterlace);
        assert_eq!(ModuleId::of(&r.operations[1]), ModuleId::Denoise);
        // Finish goes last; stabilize sits between deinterlace and denoise.
        assert_eq!(add_module(&mut r, ModuleId::Finish), Ok(2));
        assert_eq!(add_module(&mut r, ModuleId::Stabilize), Ok(1));
        let order: Vec<ModuleId> = r.operations.iter().map(ModuleId::of).collect();
        assert_eq!(
            order,
            vec![
                ModuleId::Deinterlace,
                ModuleId::Stabilize,
                ModuleId::Denoise,
                ModuleId::Finish
            ]
        );
        let err = add_module(&mut r, ModuleId::Denoise).expect_err("duplicate refused");
        assert_eq!(err, ChainEditError::AlreadyPresent(ModuleId::Denoise));
        assert!(err.message().contains("already in the chain"));
    }

    #[test]
    fn remove_and_insert_round_trip() {
        let mut r = recipe();
        add_module(&mut r, ModuleId::Deinterlace).expect("add");
        add_module(&mut r, ModuleId::Denoise).expect("add");
        let removed = remove_step(&mut r, 0).expect("remove");
        assert_eq!(r.operations.len(), 1);
        insert_step(&mut r, 0, removed).expect("reinsert");
        assert_eq!(ModuleId::of(&r.operations[0]), ModuleId::Deinterlace);
        assert!(matches!(
            remove_step(&mut r, 9),
            Err(ChainEditError::NoSuchStep(9))
        ));
        assert!(matches!(
            insert_step(&mut r, 9, ModuleId::Finish.default_operation()),
            Err(ChainEditError::NoSuchStep(9))
        ));
    }

    #[test]
    fn reordering_is_bounded_and_says_so() {
        let mut r = recipe();
        add_module(&mut r, ModuleId::Deinterlace).expect("add");
        add_module(&mut r, ModuleId::Denoise).expect("add");
        assert!(!move_step_up(&mut r, 0));
        assert_eq!(ModuleId::of(&r.operations[0]), ModuleId::Deinterlace);
        assert!(move_step_down(&mut r, 0));
        assert_eq!(ModuleId::of(&r.operations[0]), ModuleId::Denoise);
        assert!(!move_step_down(&mut r, 1));
        assert!(!move_step_up(&mut r, 5));
        assert!(!move_step_down(&mut r, 99));
    }

    #[test]
    fn invalid_parameters_are_refused_before_they_reach_the_planner() {
        let mut r = recipe();
        add_module(&mut r, ModuleId::Denoise).expect("add");
        let bad = Operation::Denoise {
            engine: DenoiseEngine::Hqdn3d,
            luma: 99.0,
            chroma: 1.0,
            temporal: 1.0,
        };
        let err = replace_step(&mut r, 0, bad).expect_err("out of range refused");
        assert!(matches!(err, ChainEditError::InvalidParameters(_)));
        // The chain is untouched by a refused edit.
        match &r.operations[0] {
            Operation::Denoise { luma, .. } => assert_eq!(*luma, 2.0),
            other => panic!("expected the default denoise, got {other:?}"),
        }
    }

    #[test]
    fn reset_restores_module_defaults() {
        let mut r = recipe();
        let idx = add_module(&mut r, ModuleId::Stabilize).expect("add");
        replace_step(
            &mut r,
            idx,
            Operation::Stabilize {
                smoothing: 60,
                zoom_percent: 20.0,
                tripod: true,
            },
        )
        .expect("replace");
        reset_step(&mut r, idx).expect("reset");
        match &r.operations[idx] {
            Operation::Stabilize {
                smoothing,
                zoom_percent,
                tripod,
            } => {
                assert_eq!(*smoothing, 15);
                assert_eq!(*zoom_percent, 0.0);
                assert!(!*tripod);
            }
            other => panic!("expected stabilize, got {other:?}"),
        }
        assert!(matches!(
            reset_step(&mut r, 4),
            Err(ChainEditError::NoSuchStep(4))
        ));
    }

    #[test]
    fn aggressive_settings_require_a_preview_with_a_reason() {
        let mut r = recipe();
        add_module(&mut r, ModuleId::Denoise).expect("add");
        assert!(preview_required(&r).is_none(), "gentle denoise needs no gate");

        replace_step(
            &mut r,
            0,
            Operation::Denoise {
                engine: DenoiseEngine::Hqdn3d,
                luma: 4.0,
                chroma: 3.0,
                temporal: 12.0,
            },
        )
        .expect("replace");
        let reason = preview_required(&r).expect("strong temporal denoise is gated");
        assert!(reason.contains("12"));

        let mut r = recipe();
        add_module(&mut r, ModuleId::Stabilize).expect("add");
        replace_step(
            &mut r,
            0,
            Operation::Stabilize {
                smoothing: 30,
                zoom_percent: 15.0,
                tripod: false,
            },
        )
        .expect("replace");
        let reason = preview_required(&r).expect("high zoom is gated");
        assert!(reason.contains("15.0%"));
        assert!(reason.contains('%'), "the crop cost is stated");
    }

    #[test]
    fn watch_points_name_consequences() {
        let send_field = Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::Auto,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        };
        let points = parameter_watch_points(&send_field);
        assert!(points.iter().any(|p| p.contains("twice the frames")));
        assert!(points.iter().any(|p| p.contains("field-order flag")));

        let tripod = Operation::Stabilize {
            smoothing: 15,
            zoom_percent: 0.0,
            tripod: true,
        };
        assert!(parameter_watch_points(&tripod)
            .iter()
            .any(|p| p.contains("deliberate camera moves")));

        let quiet = Operation::Finish {
            crop: None,
            scale_width: None,
            scale_height: None,
            deflicker: false,
            normalize_levels: false,
        };
        assert!(parameter_watch_points(&quiet).is_empty());
    }

    #[test]
    fn preflight_blocks_an_empty_or_unpreviewed_chain() {
        let facts = ExportFacts {
            chain_len: 0,
            ..Default::default()
        };
        let checks = export_preflight(&facts);
        assert_eq!(checks[0].level, CheckLevel::Blocked);
        assert!(!export_is_ready(&checks));

        let facts = ExportFacts {
            chain_len: 2,
            previewed: false,
            source_pinned: true,
            free_bytes: None,
            estimated_bytes: None,
            ..Default::default()
        };
        let checks = export_preflight(&facts);
        assert_eq!(checks[1].id, "preview");
        assert_eq!(checks[1].level, CheckLevel::Blocked);
        assert!(!export_is_ready(&checks));
    }

    #[test]
    fn preview_or_a_recorded_override_clears_the_gate() {
        let base = ExportFacts {
            chain_len: 1,
            source_pinned: true,
            ..Default::default()
        };
        let previewed = export_preflight(&ExportFacts {
            previewed: true,
            ..base.clone()
        });
        assert_eq!(previewed[1].level, CheckLevel::Pass);
        assert!(export_is_ready(&previewed));

        let overridden = export_preflight(&ExportFacts {
            expert_override: true,
            ..base
        });
        assert_eq!(overridden[1].level, CheckLevel::Warn);
        assert!(overridden[1].detail.contains("export log"));
        assert!(export_is_ready(&overridden), "a recorded override allows the run");
    }

    #[test]
    fn destination_collisions_block_without_naming_a_file() {
        let checks = export_preflight(&ExportFacts {
            chain_len: 1,
            previewed: true,
            source_pinned: true,
            output_exists: true,
            ..Default::default()
        });
        let destination = checks.iter().find(|c| c.id == "destination").expect("check");
        assert_eq!(destination.level, CheckLevel::Blocked);
        assert!(destination.detail.contains("new files"));

        let sidecar = export_preflight(&ExportFacts {
            chain_len: 1,
            previewed: true,
            source_pinned: true,
            sidecar_exists: true,
            ..Default::default()
        });
        assert_eq!(
            sidecar
                .iter()
                .find(|c| c.id == "destination")
                .expect("check")
                .level,
            CheckLevel::Blocked
        );
    }

    #[test]
    fn unpinned_source_is_a_warning_not_a_silent_pass() {
        let checks = export_preflight(&ExportFacts {
            chain_len: 1,
            previewed: true,
            source_pinned: false,
            ..Default::default()
        });
        let fixity = checks.iter().find(|c| c.id == "fixity").expect("check");
        assert_eq!(fixity.level, CheckLevel::Warn);
        assert!(fixity.detail.contains("Pin the hash"));
    }

    #[test]
    fn space_is_measured_estimated_or_unmeasured() {
        let fits = export_preflight(&ExportFacts {
            chain_len: 1,
            previewed: true,
            source_pinned: true,
            free_bytes: Some(10_000_000_000),
            estimated_bytes: Some(1_000_000_000),
        });
        assert_eq!(fits.iter().find(|c| c.id == "space").unwrap().level, CheckLevel::Pass);

        let short = export_preflight(&ExportFacts {
            chain_len: 1,
            previewed: true,
            source_pinned: true,
            free_bytes: Some(1_050_000_000),
            estimated_bytes: Some(1_000_000_000),
        });
        let space = short.iter().find(|c| c.id == "space").unwrap();
        assert_eq!(space.level, CheckLevel::Blocked);
        assert!(space.detail.contains("headroom"));

        let free_only = export_preflight(&ExportFacts {
            chain_len: 1,
            previewed: true,
            source_pinned: true,
            free_bytes: Some(500_000_000),
        });
        let space = free_only.iter().find(|c| c.id == "space").unwrap();
        assert_eq!(space.level, CheckLevel::Unknown);
        assert!(space.detail.contains("not estimated"));

        let nothing = export_preflight(&ExportFacts {
            chain_len: 1,
            previewed: true,
            source_pinned: true,
            ..Default::default()
        });
        assert_eq!(
            nothing.iter().find(|c| c.id == "space").unwrap().level,
            CheckLevel::Unknown
        );
    }

    #[test]
    fn planner_refusal_is_reported_as_blocked() {
        let checks = export_preflight(&ExportFacts {
            chain_len: 1,
            chain_error: Some("requires the VapourSynth pipeline".into()),
            previewed: true,
            source_pinned: true,
            ..Default::default()
        });
        let chain = checks.iter().find(|c| c.id == "chain").unwrap();
        assert_eq!(chain.level, CheckLevel::Blocked);
        assert!(chain.detail.contains("VapourSynth"));
    }
}
