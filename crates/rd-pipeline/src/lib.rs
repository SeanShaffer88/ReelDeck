//! ReelDeck pipeline planning.
//!
//! Turns a validated [`Recipe`] into the exact strings and argument
//! vectors an executor will run: FFmpeg filter-graph fragments per
//! operation, the two-pass stabilization plan, and a full [`ExportPlan`]
//! per export preset.
//!
//! This crate **plans only** — it never executes a process. Planning is
//! pure string building, golden-tested below, so what the UI shows the
//! operator (the live filter string, the exact command) is byte-for-byte
//! what a future executor will run. That is the reproducibility promise.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

use rd_core::{DeinterlaceEngine, DenoiseEngine, ExportPreset, Operation, Recipe, RecipeError};

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("recipe is not valid: {0}")]
    InvalidRecipe(#[from] RecipeError),
    #[error("operation requires the VapourSynth pipeline (QTGMC); the FFmpeg pipeline will not substitute a different engine")]
    RequiresVapourSynth,
    #[error("recipe produced an empty filter chain")]
    EmptyChain,
    #[error("invalid preview segment: {0}")]
    InvalidSegment(String),
    #[error("preview is not available for a chain with stabilization: the stabilization analysis pass must run over the whole input, so a segment preview cannot show what the full render will do")]
    PreviewWithStabilization,
}

/// Format an f32 the way filter strings want it: integers without a
/// trailing `.0`, fractions trimmed (`4`, `4.5`, `0.25`).
fn fmt_num(v: f32) -> String {
    if v == v.trunc() {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// The FFmpeg filter fragment for a single operation.
///
/// Stabilize contributes its pass-2 `vidstabtransform` fragment (analysis
/// is planned separately by [`stabilization_plan`]); QTGMC is an error in
/// this pipeline by design.
pub fn filter_for_operation(
    op: &Operation,
    transforms_file: &str,
) -> Result<String, PipelineError> {
    match op {
        Operation::Deinterlace {
            engine,
            field_order,
            output_rate,
            only_flagged,
        } => {
            let mode = output_rate.ffmpeg_mode();
            let parity = field_order.ffmpeg_parity();
            let deint = if *only_flagged { "interlaced" } else { "all" };
            match engine {
                DeinterlaceEngine::Bwdif => {
                    Ok(format!("bwdif=mode={mode}:parity={parity}:deint={deint}"))
                }
                DeinterlaceEngine::Yadif => {
                    Ok(format!("yadif=mode={mode}:parity={parity}:deint={deint}"))
                }
                DeinterlaceEngine::Qtgmc => Err(PipelineError::RequiresVapourSynth),
            }
        }
        Operation::Stabilize {
            smoothing,
            zoom_percent,
            tripod,
        } => Ok(format!(
            // No quotes around the file name: FFmpeg's filter parser
            // does not treat `"` as quoting, so a quoted name would
            // create/read a file literally named `"transforms.trf"`.
            // The transforms name is this workspace's own constant
            // and carries no filter-special characters.
            "vidstabtransform=input={transforms_file}:smoothing={smoothing}:zoom={}:tripod={}",
            fmt_num(*zoom_percent),
            if *tripod { 1 } else { 0 }
        )),
        Operation::Denoise {
            engine,
            luma,
            chroma,
            temporal,
        } => match engine {
            // hqdn3d takes luma_spatial:chroma_spatial:luma_temporal:chroma_temporal.
            DenoiseEngine::Hqdn3d => Ok(format!(
                "hqdn3d={}:{}:{}:{}",
                fmt_num(*luma),
                fmt_num(*chroma),
                fmt_num(*temporal),
                fmt_num(*temporal)
            )),
            DenoiseEngine::Nlmeans => Ok(format!("nlmeans=s={}:p=7:r=15", fmt_num(*luma))),
        },
        Operation::Finish {
            crop,
            scale_width,
            scale_height,
            deflicker,
            normalize_levels,
        } => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(c) = crop {
                parts.push(format!("crop={}:{}:{}:{}", c.width, c.height, c.x, c.y));
            }
            if let (Some(w), Some(h)) = (scale_width, scale_height) {
                parts.push(format!("scale={w}:{h}"));
            }
            if *deflicker {
                parts.push("deflicker".to_string());
            }
            if *normalize_levels {
                parts.push("normalize".to_string());
            }
            if parts.is_empty() {
                // A Finish step with nothing selected contributes nothing;
                // represent it explicitly so the chain stays inspectable.
                parts.push("null".to_string());
            }
            Ok(parts.join(","))
        }
    }
}

/// Format seconds for a seek/duration argument: integers without a
/// trailing `.0`, fractions trimmed — the f64 sibling of [`fmt_num`].
fn fmt_secs(v: f64) -> String {
    if v == v.trunc() {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// How many frames a full render of `recipe` should write, given the
/// source's frame count — the total progress is measured against.
///
/// Deinterlacing one frame per field doubles the count; every other
/// operation keeps it. `None` when the count cannot be stated: the
/// archival tier is not planned here, and deinterlacing only flagged
/// frames at field rate passes unflagged frames through once, so the
/// output count depends on the flags in the file.
pub fn expected_output_frames(recipe: &Recipe, source_frames: u64) -> Option<u64> {
    let mut frames = source_frames;
    for op in &recipe.operations {
        if let Operation::Deinterlace {
            engine,
            output_rate,
            only_flagged,
            ..
        } = op
        {
            if *engine == DeinterlaceEngine::Qtgmc {
                return None;
            }
            if *output_rate == rd_core::OutputRate::SendField {
                if *only_flagged {
                    return None;
                }
                frames = frames.checked_mul(2)?;
            }
        }
    }
    Some(frames)
}

/// The comma-joined FFmpeg filter chain for a whole recipe, in order.
pub fn filter_chain(recipe: &Recipe) -> Result<String, PipelineError> {
    filter_chain_with_transforms(recipe, DEFAULT_TRANSFORMS_FILE)
}

/// [`filter_chain`] with an explicit stabilization transforms file name.
pub fn filter_chain_with_transforms(
    recipe: &Recipe,
    transforms_file: &str,
) -> Result<String, PipelineError> {
    recipe.validate()?;
    let mut parts = Vec::with_capacity(recipe.operations.len());
    for op in &recipe.operations {
        parts.push(filter_for_operation(op, transforms_file)?);
    }
    if parts.is_empty() {
        return Err(PipelineError::EmptyChain);
    }
    Ok(parts.join(","))
}

/// Default transforms file name, stored inside the project so a re-run
/// reuses the same stabilization analysis.
pub const DEFAULT_TRANSFORMS_FILE: &str = "transforms.trf";

/// The two-pass stabilization plan: pass 1 analyzes motion and writes
/// the transforms file; pass 2 (the export render) consumes it via the
/// `vidstabtransform` fragment from [`filter_for_operation`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StabilizationPlan {
    /// Argument vector for pass 1 (no output file; the transforms file
    /// is the product).
    pub pass1_args: Vec<String>,
    /// The pass-2 filter fragment.
    pub pass2_filter: String,
    pub transforms_file: String,
}

pub fn stabilization_plan(input: &str, smoothing: u32, transforms_file: &str) -> StabilizationPlan {
    StabilizationPlan {
        pass1_args: vec![
            "ffmpeg".to_string(),
            "-hide_banner".to_string(),
            "-i".to_string(),
            input.to_string(),
            "-vf".to_string(),
            // vidstabdetect measures motion on one plane and its
            // frame allocation asserts (aborts the process) when the
            // negotiated input format is multi-plane RGB — exactly
            // what an FFV1 preservation source decodes to. Convert
            // for the analysis pass only; the export chain that
            // reaches the output keeps the source format untouched
            // (vidstabtransform handles it directly, verified).
            format!(
                "format=yuv420p,vidstabdetect=shakiness=5:accuracy=15:result={transforms_file}"
            ),
            "-f".to_string(),
            "null".to_string(),
            "-".to_string(),
        ],
        pass2_filter: format!(
            "vidstabtransform=input={transforms_file}:smoothing={smoothing}:zoom=0:tripod=0"
        ),
        transforms_file: transforms_file.to_string(),
    }
}

/// Codec/container arguments for one export preset (no input, filter,
/// or output — [`export_plan`] assembles the full vector).
pub fn preset_args(preset: ExportPreset) -> Vec<String> {
    let args: &[&str] = match preset {
        // FFV1 v3: range coder, large context, one slice set per frame
        // group with per-slice CRCs so corruption localises to a slice.
        ExportPreset::PreservationMaster => &[
            "-c:v",
            "ffv1",
            "-level",
            "3",
            "-coder",
            "1",
            "-context",
            "1",
            "-g",
            "1",
            "-slices",
            "24",
            "-slicecrc",
            "1",
            "-c:a",
            "flac",
        ],
        ExportPreset::Mezzanine => &[
            "-c:v",
            "prores_ks",
            "-profile:v",
            "3",
            "-vendor",
            "apl0",
            "-pix_fmt",
            "yuv422p10le",
            "-c:a",
            "pcm_s16le",
        ],
        ExportPreset::Access => &[
            "-c:v", "libx264", "-crf", "18", "-preset", "slow", "-pix_fmt", "yuv420p", "-c:a",
            "aac", "-b:a", "192k",
        ],
    };
    args.iter().map(|s| s.to_string()).collect()
}

/// A complete, ready-to-run export plan: the exact argument vector, the
/// chain it came from, and the stabilization pass that must run first
/// when the recipe stabilizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPlan {
    pub input: String,
    pub output: String,
    pub preset: ExportPreset,
    pub filter_chain: String,
    /// Full argument vector, program name first.
    pub args: Vec<String>,
    /// Present when the recipe stabilizes: run this before `args`.
    #[serde(default)]
    pub stabilization: Option<StabilizationPlan>,
}

/// Build the export plan for a recipe. The output path is used verbatim;
/// callers pick the extension matching [`ExportPreset::extension`].
pub fn export_plan(
    recipe: &Recipe,
    input: &str,
    output: &str,
) -> Result<ExportPlan, PipelineError> {
    let chain = filter_chain(recipe)?;
    let stabilization = recipe.operations.iter().find_map(|op| match op {
        Operation::Stabilize { smoothing, .. } => Some(stabilization_plan(
            input,
            *smoothing,
            DEFAULT_TRANSFORMS_FILE,
        )),
        _ => None,
    });
    let mut args: Vec<String> = vec![
        "ffmpeg".to_string(),
        "-hide_banner".to_string(),
        "-i".to_string(),
        input.to_string(),
        "-vf".to_string(),
        chain.clone(),
    ];
    args.extend(preset_args(recipe.export_preset));
    args.push("-y".to_string());
    args.push(output.to_string());
    Ok(ExportPlan {
        input: input.to_string(),
        output: output.to_string(),
        preset: recipe.export_preset,
        filter_chain: chain,
        args,
        stabilization,
    })
}

/// Build the plan for a **segment preview**: the recipe's chain and
/// preset, rendered over `[start_secs, start_secs + duration_secs)`
/// of the input instead of the whole file. Seeking is input seeking
/// (`-ss` before `-i`, fast and keyframe-accurate enough for a
/// preview the operator compares by eye); the duration is cut after
/// the input (`-t`), so the frames decoded are exactly the segment.
///
/// A chain with stabilization cannot be previewed this way: its
/// analysis pass must see the whole input, so a segment render would
/// show something the full render will not reproduce. That is
/// refused with [`PipelineError::PreviewWithStabilization`] rather
/// than silently approximated — the plan shown is the plan that can
/// run, or there is no plan.
pub fn preview_plan(
    recipe: &Recipe,
    input: &str,
    output: &str,
    start_secs: f64,
    duration_secs: f64,
) -> Result<ExportPlan, PipelineError> {
    if !start_secs.is_finite() || start_secs < 0.0 {
        return Err(PipelineError::InvalidSegment(format!(
            "start must be a finite, non-negative number of seconds, got {start_secs}"
        )));
    }
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return Err(PipelineError::InvalidSegment(format!(
            "duration must be a finite, positive number of seconds, got {duration_secs}"
        )));
    }
    let chain = filter_chain(recipe)?;
    if recipe.needs_stabilization_pass() {
        return Err(PipelineError::PreviewWithStabilization);
    }
    let mut args: Vec<String> = vec![
        "ffmpeg".to_string(),
        "-hide_banner".to_string(),
        "-ss".to_string(),
        fmt_secs(start_secs),
        "-i".to_string(),
        input.to_string(),
        "-t".to_string(),
        fmt_secs(duration_secs),
        "-vf".to_string(),
        chain.clone(),
    ];
    args.extend(preset_args(recipe.export_preset));
    args.push("-y".to_string());
    args.push(output.to_string());
    Ok(ExportPlan {
        input: input.to_string(),
        output: output.to_string(),
        preset: recipe.export_preset,
        filter_chain: chain,
        args,
        stabilization: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rd_core::{FieldOrder, OutputRate};

    fn deinterlace(
        engine: DeinterlaceEngine,
        output_rate: OutputRate,
        only_flagged: bool,
    ) -> Recipe {
        Recipe::new("in.mkv", ExportPreset::PreservationMaster).push(Operation::Deinterlace {
            engine,
            field_order: FieldOrder::TopFieldFirst,
            output_rate,
            only_flagged,
        })
    }

    #[test]
    fn expected_output_frames_follow_the_output_rate() {
        let field = deinterlace(DeinterlaceEngine::Bwdif, OutputRate::SendField, false);
        assert_eq!(expected_output_frames(&field, 50), Some(100));
        let frame = deinterlace(DeinterlaceEngine::Yadif, OutputRate::SendFrame, false);
        assert_eq!(expected_output_frames(&frame, 50), Some(50));
        let denoise = Recipe::new("in.mkv", ExportPreset::Access).push(Operation::Denoise {
            engine: DenoiseEngine::Hqdn3d,
            luma: 4.0,
            chroma: 3.0,
            temporal: 6.0,
        });
        assert_eq!(expected_output_frames(&denoise, 50), Some(50));
    }

    #[test]
    fn expected_output_frames_are_unknown_when_they_depend_on_the_file() {
        let flagged = deinterlace(DeinterlaceEngine::Bwdif, OutputRate::SendField, true);
        assert_eq!(expected_output_frames(&flagged, 50), None);
        let flagged_frame = deinterlace(DeinterlaceEngine::Bwdif, OutputRate::SendFrame, true);
        assert_eq!(expected_output_frames(&flagged_frame, 50), Some(50));
        let qtgmc = deinterlace(DeinterlaceEngine::Qtgmc, OutputRate::SendField, false);
        assert_eq!(expected_output_frames(&qtgmc, 50), None);
        assert_eq!(
            expected_output_frames(
                &deinterlace(DeinterlaceEngine::Bwdif, OutputRate::SendField, false),
                u64::MAX
            ),
            None
        );
    }

    fn deinterlace_recipe() -> Recipe {
        Recipe::new("in.mkv", ExportPreset::PreservationMaster).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::TopFieldFirst,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        })
    }

    #[test]
    fn golden_bwdif_send_field_tff() {
        let chain = filter_chain(&deinterlace_recipe()).expect("chain");
        assert_eq!(chain, "bwdif=mode=send_field:parity=tff:deint=all");
    }

    #[test]
    fn golden_bwdif_only_flagged_and_bff() {
        let recipe = Recipe::new("in.mkv", ExportPreset::Access).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Bwdif,
            field_order: FieldOrder::BottomFieldFirst,
            output_rate: OutputRate::SendFrame,
            only_flagged: true,
        });
        assert_eq!(
            filter_chain(&recipe).expect("chain"),
            "bwdif=mode=send_frame:parity=bff:deint=interlaced"
        );
    }

    #[test]
    fn golden_yadif_auto() {
        let recipe = Recipe::new("in.mkv", ExportPreset::Access).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Yadif,
            field_order: FieldOrder::Auto,
            output_rate: OutputRate::SendFrame,
            only_flagged: false,
        });
        assert_eq!(
            filter_chain(&recipe).expect("chain"),
            "yadif=mode=send_frame:parity=auto:deint=all"
        );
    }

    #[test]
    fn qtgmc_is_refused_not_substituted() {
        let recipe = Recipe::new("in.mkv", ExportPreset::Access).push(Operation::Deinterlace {
            engine: DeinterlaceEngine::Qtgmc,
            field_order: FieldOrder::Auto,
            output_rate: OutputRate::SendField,
            only_flagged: false,
        });
        assert!(matches!(
            filter_chain(&recipe),
            Err(PipelineError::RequiresVapourSynth)
        ));
    }

    #[test]
    fn golden_hqdn3d_defaults() {
        let recipe = Recipe::new("in.mkv", ExportPreset::Access).push(Operation::Denoise {
            engine: DenoiseEngine::Hqdn3d,
            luma: 4.0,
            chroma: 3.0,
            temporal: 6.0,
        });
        assert_eq!(filter_chain(&recipe).expect("chain"), "hqdn3d=4:3:6:6");
    }

    #[test]
    fn golden_chain_order_is_recipe_order() {
        let recipe = deinterlace_recipe().push(Operation::Denoise {
            engine: DenoiseEngine::Hqdn3d,
            luma: 4.0,
            chroma: 3.0,
            temporal: 6.0,
        });
        assert_eq!(
            filter_chain(&recipe).expect("chain"),
            "bwdif=mode=send_field:parity=tff:deint=all,hqdn3d=4:3:6:6"
        );
    }

    #[test]
    fn golden_preservation_export_args() {
        let plan = export_plan(&deinterlace_recipe(), "in.mkv", "out.mkv").expect("plan");
        assert_eq!(
            plan.args,
            vec![
                "ffmpeg",
                "-hide_banner",
                "-i",
                "in.mkv",
                "-vf",
                "bwdif=mode=send_field:parity=tff:deint=all",
                "-c:v",
                "ffv1",
                "-level",
                "3",
                "-coder",
                "1",
                "-context",
                "1",
                "-g",
                "1",
                "-slices",
                "24",
                "-slicecrc",
                "1",
                "-c:a",
                "flac",
                "-y",
                "out.mkv",
            ]
        );
        assert_eq!(plan.preset, ExportPreset::PreservationMaster);
        assert!(plan.stabilization.is_none());
    }

    #[test]
    fn stabilization_plan_is_two_pass() {
        let recipe = Recipe::new("in.mp4", ExportPreset::Access).push(Operation::Stabilize {
            smoothing: 15,
            zoom_percent: 0.0,
            tripod: false,
        });
        let plan = export_plan(&recipe, "in.mp4", "out.mp4").expect("plan");
        let stab = plan.stabilization.expect("stabilization plan");
        assert_eq!(stab.transforms_file, "transforms.trf");
        assert!(stab
            .pass1_args
            .iter()
            .any(|a| a
                == "format=yuv420p,vidstabdetect=shakiness=5:accuracy=15:result=transforms.trf"));
        assert!(plan
            .filter_chain
            .contains("vidstabtransform=input=transforms.trf:smoothing=15"));
    }

    #[test]
    fn golden_preview_args_seek_and_cut() {
        let plan = preview_plan(&deinterlace_recipe(), "in.mkv", "preview.mkv", 12.5, 10.0)
            .expect("preview plan");
        assert_eq!(
            plan.args,
            vec![
                "ffmpeg",
                "-hide_banner",
                "-ss",
                "12.5",
                "-i",
                "in.mkv",
                "-t",
                "10",
                "-vf",
                "bwdif=mode=send_field:parity=tff:deint=all",
                "-c:v",
                "ffv1",
                "-level",
                "3",
                "-coder",
                "1",
                "-context",
                "1",
                "-g",
                "1",
                "-slices",
                "24",
                "-slicecrc",
                "1",
                "-c:a",
                "flac",
                "-y",
                "preview.mkv",
            ]
        );
        assert!(plan.stabilization.is_none());
        assert_eq!(plan.preset, ExportPreset::PreservationMaster);
    }

    #[test]
    fn preview_rejects_bad_segments() {
        for (start, duration) in [(-1.0, 10.0), (0.0, 0.0), (5.0, -2.0), (f64::NAN, 1.0)] {
            assert!(
                matches!(
                    preview_plan(&deinterlace_recipe(), "in.mkv", "p.mkv", start, duration),
                    Err(PipelineError::InvalidSegment(_))
                ),
                "start={start} duration={duration} must be refused"
            );
        }
    }

    #[test]
    fn preview_refuses_stabilization_chains() {
        let recipe = Recipe::new("in.mp4", ExportPreset::Access).push(Operation::Stabilize {
            smoothing: 15,
            zoom_percent: 0.0,
            tripod: false,
        });
        assert!(matches!(
            preview_plan(&recipe, "in.mp4", "p.mp4", 0.0, 5.0),
            Err(PipelineError::PreviewWithStabilization)
        ));
    }

    #[test]
    fn invalid_recipe_is_rejected_before_planning() {
        let recipe = Recipe::new("in.mkv", ExportPreset::Access);
        assert!(matches!(
            export_plan(&recipe, "in.mkv", "out.mp4"),
            Err(PipelineError::InvalidRecipe(_))
        ));
    }
}
