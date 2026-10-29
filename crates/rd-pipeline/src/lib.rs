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
    #[error("a lossless cut keeps one range per output, because copied streams can only be cut at keyframes and cannot be joined without re-checking them; use `reeldeck split` for one file per range, or a re-encoding intent to remove parts in one file")]
    LosslessMultiRange,
    #[error("stabilization cannot be combined with kept ranges: its analysis pass measures the whole source, so a cut render would apply the motion of the wrong frames; stabilize first, then cut the stabilized output")]
    StabilizationWithCut,
    #[error("keeping several ranges in one file needs to know whether the source has audio; probe the source first")]
    AudioUnknown,
}

/// Facts about the source that a recipe does not carry and some plans
/// need. Unknown facts stay `None`; a plan that needs one refuses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceFacts {
    pub has_audio: Option<bool>,
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
    // A cut's length depends on the ranges and the source's rate, which
    // the frame count alone cannot give.
    if !recipe.keep.is_empty() {
        return None;
    }
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
        // Every stream, untouched.
        ExportPreset::StreamCopy => &["-map", "0", "-c", "copy"],
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
    /// What the operator must know before this plan runs (for example
    /// that a lossless cut starts on a keyframe). The executor records
    /// each one in the export log.
    #[serde(default)]
    pub notices: Vec<String>,
}

/// Build the export plan for a recipe. The output path is used verbatim;
/// callers pick the extension matching [`ExportPreset::extension`].
///
/// Equivalent to [`export_plan_with_facts`] with no source facts, which
/// is enough for every recipe except one keeping several ranges.
pub fn export_plan(
    recipe: &Recipe,
    input: &str,
    output: &str,
) -> Result<ExportPlan, PipelineError> {
    export_plan_with_facts(recipe, input, output, &SourceFacts::default())
}

/// Build the export plan for a recipe, given facts about its source.
///
/// Kept ranges (recipe schema 2) are planned two ways, never confused:
///
/// - **Re-encoding intents** cut frame-accurately. One range is input
///   seeking plus a duration; several are trimmed from the decoded
///   streams and concatenated in one filter graph (video and the first
///   audio stream, which is why the plan needs to know whether there is
///   audio), and the restoration chain runs on the result.
/// - **Stream copy** copies every stream untouched. One range is cut
///   with input seeking, which can only start on a keyframe — stated in
///   [`ExportPlan::notices`] before the run. Several ranges are refused.
pub fn export_plan_with_facts(
    recipe: &Recipe,
    input: &str,
    output: &str,
    facts: &SourceFacts,
) -> Result<ExportPlan, PipelineError> {
    recipe.validate()?;
    let chain = if recipe.operations.is_empty() {
        String::new()
    } else {
        filter_chain(recipe)?
    };
    let stabilization = recipe.operations.iter().find_map(|op| match op {
        Operation::Stabilize { smoothing, .. } => Some(stabilization_plan(
            input,
            *smoothing,
            DEFAULT_TRANSFORMS_FILE,
        )),
        _ => None,
    });
    if stabilization.is_some() && !recipe.keep.is_empty() {
        return Err(PipelineError::StabilizationWithCut);
    }
    let stream_copy = recipe.export_preset == ExportPreset::StreamCopy;
    if stream_copy && recipe.keep.len() > 1 {
        return Err(PipelineError::LosslessMultiRange);
    }

    let mut notices = Vec::new();
    let mut args: Vec<String> = vec!["ffmpeg".to_string(), "-hide_banner".to_string()];
    match recipe.keep.as_slice() {
        [] => {
            args.extend(["-i".to_string(), input.to_string()]);
            if !chain.is_empty() {
                args.extend(["-vf".to_string(), chain.clone()]);
            }
        }
        [range] => {
            if range.start_secs > 0.0 {
                args.extend(["-ss".to_string(), fmt_secs(range.start_secs)]);
            }
            args.extend(["-i".to_string(), input.to_string()]);
            if let Some(duration) = range.duration_secs() {
                args.extend(["-t".to_string(), fmt_secs(duration)]);
            }
            if !chain.is_empty() {
                args.extend(["-vf".to_string(), chain.clone()]);
            }
            if stream_copy && range.start_secs > 0.0 {
                notices.push(format!(
                    "lossless cut: copied streams can only start on a keyframe, so the output starts at the last keyframe at or before {}s, not exactly there",
                    fmt_secs(range.start_secs)
                ));
            }
        }
        ranges => {
            let has_audio = facts.has_audio.ok_or(PipelineError::AudioUnknown)?;
            args.extend(["-i".to_string(), input.to_string()]);
            let graph = keep_graph(ranges, has_audio, &chain);
            args.extend(["-filter_complex".to_string(), graph]);
            args.extend(["-map".to_string(), "[vout]".to_string()]);
            if has_audio {
                args.extend(["-map".to_string(), "[aout]".to_string()]);
            } else {
                notices
                    .push("the source has no audio stream; the output is picture only".to_string());
            }
            notices.push(format!(
                "{} ranges are joined in one file; only the first video and first audio stream are kept",
                ranges.len()
            ));
        }
    }
    args.extend(preset_args(recipe.export_preset));
    if stream_copy && !recipe.keep.is_empty() {
        // Copied packets keep their original timestamps; shift them so
        // the output starts at zero instead of at the cut point.
        args.extend(["-avoid_negative_ts".to_string(), "make_zero".to_string()]);
    }
    args.push("-y".to_string());
    args.push(output.to_string());
    Ok(ExportPlan {
        input: input.to_string(),
        output: output.to_string(),
        preset: recipe.export_preset,
        filter_chain: chain,
        args,
        stabilization,
        notices,
    })
}

/// The filter graph keeping several ranges: each range trimmed from the
/// first video (and audio) stream with its timestamps reset, all of them
/// concatenated in order, then the restoration chain on the result.
fn keep_graph(ranges: &[rd_core::KeepRange], has_audio: bool, chain: &str) -> String {
    let bounds = |r: &rd_core::KeepRange| match r.end_secs {
        Some(end) => format!("start={}:end={}", fmt_secs(r.start_secs), fmt_secs(end)),
        None => format!("start={}", fmt_secs(r.start_secs)),
    };
    let mut parts = Vec::new();
    let mut inputs = String::new();
    for (i, range) in ranges.iter().enumerate() {
        parts.push(format!(
            "[0:v:0]trim={},setpts=PTS-STARTPTS[v{i}]",
            bounds(range)
        ));
        inputs.push_str(&format!("[v{i}]"));
        if has_audio {
            parts.push(format!(
                "[0:a:0]atrim={},asetpts=PTS-STARTPTS[a{i}]",
                bounds(range)
            ));
            inputs.push_str(&format!("[a{i}]"));
        }
    }
    let n = ranges.len();
    let (a, outs) = if has_audio {
        (1, "[vcat][aout]")
    } else {
        (0, "[vcat]")
    };
    parts.push(format!("{inputs}concat=n={n}:v=1:a={a}{outs}"));
    if chain.is_empty() {
        parts.push("[vcat]null[vout]".to_string());
    } else {
        parts.push(format!("[vcat]{chain}[vout]"));
    }
    parts.join(";")
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
        notices: Vec::new(),
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

    fn args(plan: &ExportPlan) -> String {
        plan.args.join(" ")
    }

    #[test]
    fn golden_trim_reencodes_frame_accurately() {
        let recipe = Recipe::new("in.mkv", ExportPreset::Access).keep_range(12.5, Some(20.0));
        let plan = export_plan(&recipe, "in.mkv", "out.mp4").expect("plan");
        assert_eq!(
            args(&plan),
            "ffmpeg -hide_banner -ss 12.5 -i in.mkv -t 7.5 -c:v libx264 -crf 18 -preset slow \
             -pix_fmt yuv420p -c:a aac -b:a 192k -y out.mp4"
        );
        assert!(plan.notices.is_empty());
        assert_eq!(plan.filter_chain, "");
    }

    #[test]
    fn golden_trim_carries_the_restoration_chain() {
        let recipe = deinterlace(DeinterlaceEngine::Bwdif, OutputRate::SendField, false)
            .keep_range(0.0, Some(30.0));
        let plan = export_plan(&recipe, "in.mkv", "out.mkv").expect("plan");
        assert!(args(&plan).starts_with(
            "ffmpeg -hide_banner -i in.mkv -t 30 -vf bwdif=mode=send_field:parity=tff:deint=all -c:v ffv1"
        ), "{}", args(&plan));
    }

    #[test]
    fn golden_several_ranges_join_in_one_graph() {
        let recipe = deinterlace(DeinterlaceEngine::Bwdif, OutputRate::SendFrame, false)
            .keep_range(0.0, Some(10.0))
            .keep_range(20.0, None);
        assert!(matches!(
            export_plan(&recipe, "in.mkv", "out.mkv"),
            Err(PipelineError::AudioUnknown)
        ));
        let with_audio = SourceFacts {
            has_audio: Some(true),
        };
        let plan = export_plan_with_facts(&recipe, "in.mkv", "out.mkv", &with_audio).expect("plan");
        let graph = plan.args[plan
            .args
            .iter()
            .position(|a| a == "-filter_complex")
            .unwrap()
            + 1]
        .clone();
        assert_eq!(
            graph,
            "[0:v:0]trim=start=0:end=10,setpts=PTS-STARTPTS[v0];\
             [0:a:0]atrim=start=0:end=10,asetpts=PTS-STARTPTS[a0];\
             [0:v:0]trim=start=20,setpts=PTS-STARTPTS[v1];\
             [0:a:0]atrim=start=20,asetpts=PTS-STARTPTS[a1];\
             [v0][a0][v1][a1]concat=n=2:v=1:a=1[vcat][aout];\
             [vcat]bwdif=mode=send_frame:parity=tff:deint=all[vout]"
        );
        assert!(args(&plan).contains("-map [vout] -map [aout] -c:v ffv1"));

        let silent = SourceFacts {
            has_audio: Some(false),
        };
        let plan = export_plan_with_facts(
            &Recipe::new("in.mkv", ExportPreset::Access)
                .keep_range(1.0, Some(2.0))
                .keep_range(3.0, Some(4.0)),
            "in.mkv",
            "out.mp4",
            &silent,
        )
        .expect("plan");
        let joined = args(&plan);
        assert!(
            joined.contains("[v0][v1]concat=n=2:v=1:a=0[vcat];[vcat]null[vout]"),
            "{joined}"
        );
        assert!(!joined.contains("[aout]"));
        assert!(plan.notices.iter().any(|n| n.contains("no audio")));
    }

    #[test]
    fn golden_stream_copy_remux_and_lossless_cut() {
        let remux = Recipe::new("in.mov", ExportPreset::StreamCopy);
        assert_eq!(
            args(&export_plan(&remux, "in.mov", "out.mkv").unwrap()),
            "ffmpeg -hide_banner -i in.mov -map 0 -c copy -y out.mkv"
        );
        let cut = Recipe::new("in.mov", ExportPreset::StreamCopy).keep_range(60.0, Some(90.0));
        let plan = export_plan(&cut, "in.mov", "out.mkv").unwrap();
        assert_eq!(
            args(&plan),
            "ffmpeg -hide_banner -ss 60 -i in.mov -t 30 -map 0 -c copy -avoid_negative_ts make_zero -y out.mkv"
        );
        assert_eq!(plan.notices.len(), 1);
        assert!(plan.notices[0].contains("keyframe at or before 60s"));
        let several = cut.keep_range(100.0, Some(110.0));
        assert!(matches!(
            export_plan(&several, "in.mov", "out.mkv"),
            Err(PipelineError::LosslessMultiRange)
        ));
    }

    #[test]
    fn stabilization_and_cuts_are_refused_together() {
        let recipe = Recipe::new("in.mp4", ExportPreset::Access)
            .push(Operation::Stabilize {
                smoothing: 15,
                zoom_percent: 0.0,
                tripod: false,
            })
            .keep_range(5.0, Some(9.0));
        assert!(matches!(
            export_plan(&recipe, "in.mp4", "out.mp4"),
            Err(PipelineError::StabilizationWithCut)
        ));
    }

    #[test]
    fn cut_frame_totals_are_unknown() {
        let recipe = deinterlace(DeinterlaceEngine::Bwdif, OutputRate::SendField, false)
            .keep_range(0.0, Some(2.0));
        assert_eq!(expected_output_frames(&recipe, 100), None);
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
