//! ReelDeck command-line entry point.
//!
//! Hand-rolled argument parsing on purpose: the workspace keeps a tiny,
//! stdlib-first dependency set (serde / serde_json / thiserror), and the
//! CLI surface is four subcommands. If that stops being true, revisit.
//!
//! Subcommands:
//!   reeldeck inspect <file> [flags]  Analyze a sampled range of a media
//!                                    file and print a report (text by
//!                                    default; --json / --html <file>)
//!   reeldeck plan <recipe.json>   Print the export plan for a recipe
//!   reeldeck export <recipe.json> Render a recipe end to end; the
//!                                 output ships with its recipe and
//!                                 export-log sidecars
//!   reeldeck preview <recipe.json> --start <s> --duration <s>
//!                                 Render one segment of a recipe
//!   reeldeck verify <output>      Check an export against the proof
//!                                 it shipped with (its sidecars)
//!   reeldeck rerun <log.json>     Re-run a recorded export's recipe
//!                                 and compare the new output's hash
//!                                 with the recorded one
//!   reeldeck batch <manifest.json> Render a manifest's recipes in
//!                                 order; a stopped batch resumes at
//!                                 the entry that did not finish
//!   reeldeck cut <file> --keep <a>-<b>
//!                                 Keep one or more ranges of a file
//!                                 (lossless or frame-accurate)
//!   reeldeck split <file> --at <t> Split a file into parts, each
//!                                 rendered with its own proof
//!   reeldeck join <a> <b> …        Join files in order into one
//!                                 output (lossless or re-encoded)
//!   reeldeck hash <file>          Print the file's SHA-256
//!   reeldeck version              Print the version

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("version") | Some("--version") | Some("-V") => {
            println!("reeldeck {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("hash") => cmd_hash(&args[1..]),
        Some("inspect") => cmd_inspect(&args[1..]),
        Some("plan") => cmd_plan(&args[1..]),
        Some("export") => cmd_export(&args[1..]),
        Some("preview") => cmd_preview(&args[1..]),
        Some("verify") => cmd_verify(&args[1..]),
        Some("rerun") => cmd_rerun(&args[1..]),
        Some("batch") => cmd_batch(&args[1..]),
        Some("cut") => cmd_cut(&args[1..]),
        Some("split") => cmd_split(&args[1..]),
        Some("join") => cmd_join(&args[1..]),
        Some("help") | Some("--help") | Some("-h") | None => {
            print_usage();
            0
        }
        Some(other) => {
            eprintln!("reeldeck: unknown subcommand '{other}'\n");
            print_usage();
            2
        }
    };
    ExitCode::from(code as u8)
}

fn print_usage() {
    println!(
        "ReelDeck — transparent, reproducible video restoration\n\
         \n\
         USAGE:\n\
         \x20 reeldeck inspect <file> [--start <secs>] [--duration <secs>] [--tout <ratio>] [--json] [--html <file>]\n\
         \x20                               Analyze a sampled range: signal stats, anomalies,\n\
         \x20                               interlace suggestion (text report; --json for the\n\
         \x20                               machine-readable report, --html <file> also writes a\n\
         \x20                               standalone HTML report to a NEW file)\n\
         \x20 reeldeck plan <recipe.json>   Print the export plan (argument vector) for a recipe\n\
         \x20 reeldeck export <recipe.json> [--in <file>] [--out <file>]\n\
         \x20                               Render a recipe end to end. The output is a NEW\n\
         \x20                               file and ships with <out>.recipe.json and\n\
         \x20                               <out>.export-log.json beside it\n\
         \x20 reeldeck preview <recipe.json> --start <secs> --duration <secs> [--in <file>] [--out <file>]\n\
         \x20                               Render one segment of a recipe (same sidecars;\n\
         \x20                               the segment is recorded in the recipe notes)\n\
         \x20 reeldeck verify <output-file> [--json]\n\
         \x20                               Check an export against its sidecars: recorded\n\
         \x20                               run, output hash, source hash, preset, schema.\n\
         \x20                               Checks whose evidence is missing are SKIP,\n\
         \x20                               never PASS\n\
         \x20 reeldeck rerun <export-log.json> [--in <file>] [--out <file>]\n\
         \x20                               Re-run a recorded export's recipe beside a\n\
         \x20                               NEW output (<stem>.rerun.<ext> by default) and\n\
         \x20                               report how it compares to the recorded run:\n\
         \x20                               file bytes first, then the elementary\n\
         \x20                               streams, so container metadata is not\n\
         \x20                               mistaken for a changed picture\n\
         \x20 reeldeck batch <manifest.json> [--keep-going] [--status]\n\
         \x20                               Render every recipe in a manifest, in order,\n\
         \x20                               each with its own proof. Progress is saved to\n\
         \x20                               <manifest>.state.json; run it again to resume\n\
         \x20                               at the entry that did not finish. Stops at the\n\
         \x20                               first failure unless --keep-going; --status\n\
         \x20                               prints the saved state without rendering\n\
         \x20 reeldeck cut <file> --keep <start>-<end> [--keep …] (--lossless | --preset <name>) [--out <file>]\n\
         \x20                               Keep ranges of a file (times in seconds or h:mm:ss;\n\
         \x20                               <start>- keeps to the end). --lossless copies the\n\
         \x20                               streams untouched (one range; the start snaps to a\n\
         \x20                               keyframe, recorded in the log). --preset preservation|\n\
         \x20                               mezzanine|access re-encodes and cuts frame-accurately\n\
         \x20 reeldeck split <file> --at <time> [--at …] (--lossless | --preset <name>) [--dir <folder>]\n\
         \x20                               Split into parts at the given times, run as a batch\n\
         \x20                               in <stem>.split/ so each part has its own proof;\n\
         \x20                               the same command again resumes an unfinished split\n\
         \x20 reeldeck join <file> <file> [<file> …] (--lossless | --preset <name>) [--out <file>]\n\
         \x20                               Join files in order into one NEW output, each source\n\
         \x20                               pinned by hash. --lossless copies the streams (all\n\
         \x20                               files must share codecs and picture size); --preset\n\
         \x20                               re-encodes. Different picture sizes, or audio in only\n\
         \x20                               some files, are refused rather than conformed silently\n\
         \x20 reeldeck hash <file>          Print the file's SHA-256\n\
         \x20 reeldeck version              Print the version"
    );
}

fn one_file_arg(args: &[String], usage: &str) -> Result<PathBuf, u8> {
    match args {
        [only] => Ok(PathBuf::from(only)),
        _ => {
            eprintln!("usage: {usage}");
            Err(2)
        }
    }
}

fn cmd_hash(args: &[String]) -> i32 {
    let path = match one_file_arg(args, "reeldeck hash <file>") {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    match rd_core::sha256_file(&path) {
        Ok(hex) => {
            println!("{hex}  {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("reeldeck hash: cannot read {}: {e}", path.display());
            1
        }
    }
}

/// Default temporal-outlier ratio for `inspect` (see
/// `rd_analyze::temporal_outliers`: 0.05..=0.20 suits tape captures).
const DEFAULT_TOUT_THRESHOLD: f64 = 0.05;

struct InspectOptions {
    path: PathBuf,
    start: f64,
    duration: Option<f64>,
    tout: f64,
    json: bool,
    html: Option<PathBuf>,
}

const INSPECT_USAGE: &str = "reeldeck inspect <file> [--start <secs>] [--duration <secs>] \
                            [--tout <ratio>] [--json] [--html <file>]";

fn parse_inspect_args(args: &[String]) -> Result<InspectOptions, u8> {
    let mut opts = InspectOptions {
        path: PathBuf::new(),
        start: 0.0,
        duration: None,
        tout: DEFAULT_TOUT_THRESHOLD,
        json: false,
        html: None,
    };
    let mut have_path = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let value = |i: &mut usize| -> Result<String, u8> {
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| {
                eprintln!("reeldeck inspect: missing value for {arg}");
                eprintln!("usage: {INSPECT_USAGE}");
                2u8
            })
        };
        match arg.as_str() {
            "--start" => {
                let v = value(&mut i)?;
                opts.start = parse_f64_flag("--start", &v)?;
            }
            "--duration" => {
                let v = value(&mut i)?;
                opts.duration = Some(parse_f64_flag("--duration", &v)?);
            }
            "--tout" => {
                let v = value(&mut i)?;
                opts.tout = parse_f64_flag("--tout", &v)?;
            }
            "--json" => opts.json = true,
            "--html" => {
                let v = value(&mut i)?;
                opts.html = Some(PathBuf::from(v));
            }
            flag if flag.starts_with("--") => {
                eprintln!("reeldeck inspect: unknown flag '{flag}'");
                eprintln!("usage: {INSPECT_USAGE}");
                return Err(2);
            }
            _ => {
                if have_path {
                    eprintln!("reeldeck inspect: unexpected extra argument '{arg}'");
                    eprintln!("usage: {INSPECT_USAGE}");
                    return Err(2);
                }
                opts.path = PathBuf::from(arg);
                have_path = true;
            }
        }
        i += 1;
    }
    if !have_path {
        eprintln!("usage: {INSPECT_USAGE}");
        return Err(2);
    }
    // Bad values are usage errors (exit 2), distinct from a failed
    // analysis (exit 1): validate before any tool runs.
    let range = rd_analyze::SampleRange::new(opts.start, opts.duration);
    if let Err(e) = range.validate() {
        eprintln!("reeldeck inspect: {e}");
        return Err(2);
    }
    if !opts.tout.is_finite() || !(0.0..=1.0).contains(&opts.tout) {
        eprintln!(
            "reeldeck inspect: --tout must be a ratio between 0 and 1, got {}",
            opts.tout
        );
        return Err(2);
    }
    Ok(opts)
}

fn parse_f64_flag(flag: &str, raw: &str) -> Result<f64, u8> {
    raw.parse::<f64>().map_err(|_| {
        eprintln!("reeldeck inspect: {flag} expects a number, got '{raw}'");
        eprintln!("usage: {INSPECT_USAGE}");
        2u8
    })
}

fn cmd_inspect(args: &[String]) -> i32 {
    let opts = match parse_inspect_args(args) {
        Ok(o) => o,
        Err(c) => return c as i32,
    };
    // The HTML report is always a NEW file (outputs are never
    // overwritten — the same rule that protects sources). Refuse
    // early, before spending time on analysis.
    if let Some(html_path) = &opts.html {
        if html_path.exists() {
            eprintln!(
                "reeldeck inspect: {} already exists; refusing to overwrite it",
                html_path.display()
            );
            return 1;
        }
    }
    let range = rd_analyze::SampleRange::new(opts.start, opts.duration);
    let report = match rd_analyze::analyze(&opts.path, range, opts.tout) {
        Ok(r) => r,
        // ToolUnavailable and friends are reported plainly, never faked:
        // inspection without the tools is impossible, so say so.
        Err(e) => {
            eprintln!("reeldeck inspect: {e}");
            return 1;
        }
    };
    if opts.json {
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("reeldeck inspect: cannot serialize result: {e}");
                return 1;
            }
        }
    } else {
        print!("{}", render_text(&report));
    }
    if let Some(html_path) = &opts.html {
        if let Err(code) = write_html_report(html_path, &report) {
            return code;
        }
    }
    0
}

fn write_html_report(path: &Path, report: &rd_analyze::AnalysisReport) -> Result<(), i32> {
    use std::io::Write;
    // create_new: fail rather than overwrite, even if the file
    // appeared after the early existence check.
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("reeldeck inspect: cannot create {}: {e}", path.display());
            return Err(1);
        }
    };
    match file.write_all(render_html(report).as_bytes()) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("reeldeck inspect: cannot write {}: {e}", path.display());
            Err(1)
        }
    }
}

fn suggestion_label(s: rd_analyze::InterlaceSuggestion) -> &'static str {
    use rd_analyze::InterlaceSuggestion::*;
    match s {
        TopFieldFirst => "Top field first (TFF)",
        BottomFieldFirst => "Bottom field first (BFF)",
        Progressive => "Progressive",
        Undetermined => "Undetermined",
    }
}

fn fmt_range(range: rd_analyze::SampleRange) -> String {
    match range.duration_secs {
        Some(d) => format!("start {:.2}s, duration {:.2}s", range.start_secs, d),
        None if range.start_secs > 0.0 => {
            format!("start {:.2}s, to end of file", range.start_secs)
        }
        None => "full file".to_string(),
    }
}

fn render_text(report: &rd_analyze::AnalysisReport) -> String {
    use std::fmt::Write as _;
    let probe = &report.probe;
    let mut out = String::new();
    let _ = writeln!(out, "ReelDeck inspect — {}", report.source);
    let _ = writeln!(
        out,
        "Format: {} · Duration: {:.2}s · Size: {} bytes",
        probe.format_name, probe.duration_secs, probe.size_bytes
    );
    if let Some(v) = probe.first_video() {
        let dims = match (v.width, v.height) {
            (Some(w), Some(h)) => format!(" {w}x{h}"),
            _ => String::new(),
        };
        let rate = v
            .avg_frame_rate
            .as_deref()
            .map(|r| format!(" @ {r} fps"))
            .unwrap_or_default();
        let field = v
            .field_order
            .as_deref()
            .map(|f| format!(" · field_order: {f}"))
            .unwrap_or_default();
        let _ = writeln!(out, "Video: {}{}{}{}", v.codec_name, dims, rate, field);
    }
    let _ = writeln!(out, "Range: {}", fmt_range(report.range));
    let _ = writeln!(out, "Frames analyzed: {}", report.frames_analyzed);
    out.push('\n');
    match &report.signal {
        Some(s) => {
            let _ = writeln!(out, "Signal (min / max / mean over analyzed frames):");
            for (name, m) in [
                ("YAVG", s.y_avg),
                ("SATAVG", s.sat_avg),
                ("TOUT", s.tout),
                ("BRNG", s.brng),
            ] {
                let _ = writeln!(
                    out,
                    "  {name:<7} {:.4} / {:.4} / {:.4}",
                    m.min, m.max, m.mean
                );
            }
        }
        None => {
            let _ = writeln!(out, "Signal: no frames measured in this range.");
        }
    }
    out.push('\n');
    let idet = &report.idet;
    let _ = writeln!(
        out,
        "Interlace suggestion: {} (a suggestion — confirm in the viewer)",
        suggestion_label(report.suggestion)
    );
    let _ = writeln!(
        out,
        "  Basis (idet counts): single TFF {} BFF {} Progressive {} Undetermined {}; \
         multi TFF {} BFF {} Progressive {} Undetermined {}; \
         repeated fields Neither {} Top {} Bottom {}",
        idet.single_tff,
        idet.single_bff,
        idet.single_progressive,
        idet.single_undetermined,
        idet.multi_tff,
        idet.multi_bff,
        idet.multi_progressive,
        idet.multi_undetermined,
        idet.neither,
        idet.repeated_top,
        idet.repeated_bottom,
    );
    out.push('\n');
    if report.anomalies.is_empty() {
        let _ = writeln!(out, "Anomalies: none found in this range.");
    } else {
        let _ = writeln!(out, "Anomalies ({}):", report.anomalies.len());
        for a in &report.anomalies {
            match a {
                rd_analyze::Anomaly::BlackSpan {
                    start_secs,
                    end_secs,
                } => {
                    let _ = writeln!(out, "  Black span       {start_secs:.2}s – {end_secs:.2}s");
                }
                rd_analyze::Anomaly::FreezeSpan {
                    start_secs,
                    end_secs,
                } => {
                    let _ = writeln!(out, "  Freeze span      {start_secs:.2}s – {end_secs:.2}s");
                }
                rd_analyze::Anomaly::TemporalOutlier {
                    frame,
                    pts_time,
                    tout,
                } => {
                    let _ = writeln!(
                        out,
                        "  Temporal outlier frame {frame} at {pts_time:.2}s (TOUT {tout:.4})"
                    );
                }
            }
        }
    }
    out.push('\n');
    let _ = writeln!(
        out,
        "Findings are suggestions from signal measurements — confirm them in the viewer before acting on them."
    );
    out
}

/// Escape a string for HTML text/attribute content.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn render_html(report: &rd_analyze::AnalysisReport) -> String {
    use std::fmt::Write as _;
    let probe = &report.probe;
    let source = html_escape(&report.source);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <title>ReelDeck inspect — {source}</title>\n<style>\n\
         body {{ font-family: system-ui, sans-serif; margin: 2em auto; max-width: 60em; padding: 0 1em; }}\n\
         table {{ border-collapse: collapse; margin: 0.5em 0 1.5em; }}\n\
         th, td {{ border: 1px solid #999; padding: 0.25em 0.6em; text-align: left; }}\n\
         th {{ background: #eee; }}\n\
         .note {{ color: #444; }}\n</style>\n</head>\n<body>"
    );
    let _ = writeln!(out, "<h1>ReelDeck inspect</h1>");
    let _ = writeln!(out, "<p>Source: <code>{source}</code></p>");
    let _ = writeln!(out, "<h2>Source</h2>\n<table>");
    let _ = writeln!(
        out,
        "<tr><th>Format</th><td>{}</td></tr>",
        html_escape(&probe.format_name)
    );
    let _ = writeln!(
        out,
        "<tr><th>Duration</th><td>{:.2}s</td></tr>",
        probe.duration_secs
    );
    let _ = writeln!(
        out,
        "<tr><th>Size</th><td>{} bytes</td></tr>",
        probe.size_bytes
    );
    if let Some(v) = probe.first_video() {
        let dims = match (v.width, v.height) {
            (Some(w), Some(h)) => format!(" {w}x{h}"),
            _ => String::new(),
        };
        let rate = v
            .avg_frame_rate
            .as_deref()
            .map(|r| format!(" @ {} fps", html_escape(r)))
            .unwrap_or_default();
        let field = v
            .field_order
            .as_deref()
            .map(|f| format!(" · field_order: {}", html_escape(f)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "<tr><th>Video</th><td>{}{}{}{}</td></tr>",
            html_escape(&v.codec_name),
            dims,
            rate,
            field
        );
    }
    let _ = writeln!(out, "</table>");
    let _ = writeln!(out, "<h2>Range</h2>");
    let _ = writeln!(out, "<p>{}</p>", html_escape(&fmt_range(report.range)));
    let _ = writeln!(out, "<p>Frames analyzed: {}</p>", report.frames_analyzed);
    let _ = writeln!(out, "<h2>Signal</h2>");
    match &report.signal {
        Some(s) => {
            let _ = writeln!(
                out,
                "<table>\n<tr><th>Metric</th><th>Min</th><th>Max</th><th>Mean</th></tr>"
            );
            for (name, m) in [
                ("YAVG", s.y_avg),
                ("SATAVG", s.sat_avg),
                ("TOUT", s.tout),
                ("BRNG", s.brng),
            ] {
                let _ = writeln!(
                    out,
                    "<tr><th>{name}</th><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td></tr>",
                    m.min, m.max, m.mean
                );
            }
            let _ = writeln!(out, "</table>");
        }
        None => {
            let _ = writeln!(out, "<p>No frames measured in this range.</p>");
        }
    }
    let _ = writeln!(out, "<h2>Interlace suggestion</h2>");
    let _ = writeln!(
        out,
        "<p><strong>{}</strong> — a suggestion; confirm in the viewer.</p>",
        suggestion_label(report.suggestion)
    );
    let idet = &report.idet;
    let _ = writeln!(
        out,
        "<table>\n<tr><th>idet</th><th>TFF</th><th>BFF</th><th>Progressive</th><th>Undetermined</th></tr>\n\
         <tr><th>Single frame</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n\
         <tr><th>Multi frame</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n</table>",
        idet.single_tff,
        idet.single_bff,
        idet.single_progressive,
        idet.single_undetermined,
        idet.multi_tff,
        idet.multi_bff,
        idet.multi_progressive,
        idet.multi_undetermined,
    );
    let _ = writeln!(
        out,
        "<p>Repeated fields: Neither {}, Top {}, Bottom {}</p>",
        idet.neither, idet.repeated_top, idet.repeated_bottom
    );
    let _ = writeln!(out, "<h2>Anomalies</h2>");
    if report.anomalies.is_empty() {
        let _ = writeln!(out, "<p>None found in this range.</p>");
    } else {
        let _ = writeln!(
            out,
            "<table>\n<tr><th>Kind</th><th>Start</th><th>End / detail</th></tr>"
        );
        for a in &report.anomalies {
            match a {
                rd_analyze::Anomaly::BlackSpan {
                    start_secs,
                    end_secs,
                } => {
                    let _ = writeln!(
                        out,
                        "<tr><td>Black span</td><td>{start_secs:.2}s</td><td>{end_secs:.2}s</td></tr>"
                    );
                }
                rd_analyze::Anomaly::FreezeSpan {
                    start_secs,
                    end_secs,
                } => {
                    let _ = writeln!(
                        out,
                        "<tr><td>Freeze span</td><td>{start_secs:.2}s</td><td>{end_secs:.2}s</td></tr>"
                    );
                }
                rd_analyze::Anomaly::TemporalOutlier {
                    frame,
                    pts_time,
                    tout,
                } => {
                    let _ = writeln!(
                        out,
                        "<tr><td>Temporal outlier</td><td>{pts_time:.2}s</td><td>frame {frame}, TOUT {tout:.4}</td></tr>"
                    );
                }
            }
        }
        let _ = writeln!(out, "</table>");
    }
    let _ = writeln!(
        out,
        "<p class=\"note\">Findings are suggestions from signal measurements — \
         confirm them in the viewer before acting on them. \
         Temporal-outlier threshold: {:.4}.</p>",
        report.tout_threshold
    );
    let _ = writeln!(out, "</body>\n</html>");
    out
}

fn cmd_plan(args: &[String]) -> i32 {
    let path = match one_file_arg(args, "reeldeck plan <recipe.json>") {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("reeldeck plan: cannot read {}: {e}", path.display());
            return 1;
        }
    };
    let recipe = match rd_core::Recipe::from_json(&text) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "reeldeck plan: {} is not a valid recipe: {e}",
                path.display()
            );
            return 1;
        }
    };
    let output = derived_output_path(&recipe.source_name, recipe.export_preset);
    // Joins and multi-range cuts need facts about their sources; ask
    // the sources when they are reachable.
    let mut facts = Vec::new();
    if !recipe.join.is_empty() || recipe.keep.len() > 1 {
        facts = recipe
            .source_names()
            .iter()
            .map(|name| rd_run::source_facts(Path::new(name)))
            .collect();
    }
    match rd_pipeline::export_plan_with_facts(&recipe, &recipe.source_name, &output, &facts) {
        Ok(plan) => {
            for notice in &plan.notices {
                println!("# note: {notice}");
            }
            if let Some(stab) = &plan.stabilization {
                println!("# pass 1 (stabilization analysis) must run first:");
                println!("{}", stab.pass1_args.join(" "));
                println!("# pass 2 (export):");
            }
            println!("{}", plan.args.join(" "));
            0
        }
        Err(e) => {
            eprintln!("reeldeck plan: {e}");
            1
        }
    }
}

const EXPORT_USAGE: &str = "reeldeck export <recipe.json> [--in <file>] [--out <file>]";
const PREVIEW_USAGE: &str = "reeldeck preview <recipe.json> --start <secs> --duration <secs> \
                             [--in <file>] [--out <file>]";

/// Shared flag surface of `export` and `preview`.
struct RunArgs {
    recipe_path: PathBuf,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    start: Option<f64>,
    duration: Option<f64>,
}

fn parse_run_args(args: &[String], usage: &str, cmd: &str) -> Result<RunArgs, u8> {
    let mut out = RunArgs {
        recipe_path: PathBuf::new(),
        input: None,
        output: None,
        start: None,
        duration: None,
    };
    let mut have_recipe = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let value = |i: &mut usize| -> Result<String, u8> {
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| {
                eprintln!("reeldeck {cmd}: missing value for {arg}");
                eprintln!("usage: {usage}");
                2u8
            })
        };
        match arg.as_str() {
            "--in" => out.input = Some(PathBuf::from(value(&mut i)?)),
            "--out" => out.output = Some(PathBuf::from(value(&mut i)?)),
            "--start" => {
                let v = value(&mut i)?;
                out.start = Some(parse_number_flag(cmd, "--start", &v, usage)?);
            }
            "--duration" => {
                let v = value(&mut i)?;
                out.duration = Some(parse_number_flag(cmd, "--duration", &v, usage)?);
            }
            flag if flag.starts_with("--") => {
                eprintln!("reeldeck {cmd}: unknown flag '{flag}'");
                eprintln!("usage: {usage}");
                return Err(2);
            }
            _ => {
                if have_recipe {
                    eprintln!("reeldeck {cmd}: unexpected extra argument '{arg}'");
                    eprintln!("usage: {usage}");
                    return Err(2);
                }
                out.recipe_path = PathBuf::from(arg);
                have_recipe = true;
            }
        }
        i += 1;
    }
    if !have_recipe {
        eprintln!("usage: {usage}");
        return Err(2);
    }
    Ok(out)
}

fn parse_number_flag(cmd: &str, flag: &str, raw: &str, usage: &str) -> Result<f64, u8> {
    raw.parse::<f64>().map_err(|_| {
        eprintln!("reeldeck {cmd}: {flag} expects a number, got '{raw}'");
        eprintln!("usage: {usage}");
        2u8
    })
}

fn load_recipe(cmd: &str, path: &Path) -> Result<rd_core::Recipe, i32> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("reeldeck {cmd}: cannot read {}: {e}", path.display());
            return Err(1);
        }
    };
    rd_core::Recipe::from_json(&text).map_err(|e| {
        eprintln!(
            "reeldeck {cmd}: {} is not a valid recipe: {e}",
            path.display()
        );
        1
    })
}

/// Run one render (export or preview plan) and report it the same way
/// for both: one line per measured progress block while it runs; the
/// output, its hash, and the proof sidecars when it lands; the plain
/// error — and the export log's path, when one was written — when it
/// does not. Returns the run report on success so callers (rerun)
/// can compare what was produced; the error is the exit code.
fn run_render(
    cmd: &str,
    recipe: &rd_core::Recipe,
    input: &Path,
    output: &Path,
    preview_plan: Option<rd_pipeline::ExportPlan>,
) -> Result<rd_run::RunReport, i32> {
    let opts = rd_run::RunOptions {
        cancel: Arc::new(AtomicBool::new(false)),
        on_progress: Some(Box::new(|p: &rd_run::Progress| {
            println!("frame {}  fps {:.1}  speed {:.2}x", p.frame, p.fps, p.speed);
        })),
    };
    let result = match preview_plan {
        Some(plan) => rd_run::run_plan(plan, recipe, input, output, opts),
        None => rd_run::run_export(recipe, input, output, opts),
    };
    match result {
        Ok(report) => {
            println!("Output: {}", report.output.display());
            println!("SHA-256: {}", report.output_sha256);
            println!("Recipe: {}", report.recipe_path.display());
            println!("Export log: {}", report.log_path.display());
            Ok(report)
        }
        Err(e) => {
            eprintln!("reeldeck {cmd}: {e}");
            // A RenderFailed/Cancelled return means the failure proof
            // was written (a sidecar failure would have replaced the
            // error with SidecarFailed), so its path can be named.
            if matches!(
                e,
                rd_run::RunError::RenderFailed { .. } | rd_run::RunError::Cancelled
            ) {
                let (_, log_path) = rd_run::sidecar_paths(output);
                eprintln!("reeldeck {cmd}: export log: {}", log_path.display());
            }
            Err(1)
        }
    }
}

/// [`run_render`] as an exit code, for the subcommands that need
/// nothing beyond the rendered output (export, preview).
fn execute_run(
    cmd: &str,
    recipe: &rd_core::Recipe,
    input: &Path,
    output: &Path,
    preview_plan: Option<rd_pipeline::ExportPlan>,
) -> i32 {
    match run_render(cmd, recipe, input, output, preview_plan) {
        Ok(_) => 0,
        Err(code) => code,
    }
}

fn cmd_export(args: &[String]) -> i32 {
    let parsed = match parse_run_args(args, EXPORT_USAGE, "export") {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    if parsed.start.is_some() || parsed.duration.is_some() {
        eprintln!("reeldeck export: --start/--duration belong to preview, not export");
        eprintln!("usage: {EXPORT_USAGE}");
        return 2;
    }
    let recipe = match load_recipe("export", &parsed.recipe_path) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let input = parsed
        .input
        .unwrap_or_else(|| PathBuf::from(&recipe.source_name));
    let output = parsed.output.unwrap_or_else(|| {
        PathBuf::from(derived_output_path(
            &recipe.source_name,
            recipe.export_preset,
        ))
    });
    execute_run("export", &recipe, &input, &output, None)
}

fn cmd_preview(args: &[String]) -> i32 {
    let parsed = match parse_run_args(args, PREVIEW_USAGE, "preview") {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    let (start, duration) = match (parsed.start, parsed.duration) {
        (Some(s), Some(d)) if s.is_finite() && s >= 0.0 && d.is_finite() && d > 0.0 => (s, d),
        _ => {
            eprintln!(
                "reeldeck preview: --start (non-negative) and --duration (positive) are both required"
            );
            eprintln!("usage: {PREVIEW_USAGE}");
            return 2;
        }
    };
    let recipe = match load_recipe("preview", &parsed.recipe_path) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let input = parsed
        .input
        .unwrap_or_else(|| PathBuf::from(&recipe.source_name));
    let output = parsed
        .output
        .unwrap_or_else(|| PathBuf::from(preview_output_path(&input, recipe.export_preset)));
    // The recipe the run records is a clone whose notes carry the
    // segment — appended to the operator's notes, never replacing
    // them — so the sidecars show this was a preview, and of what.
    let mut preview_recipe = recipe.clone();
    if !preview_recipe.notes.is_empty() {
        preview_recipe.notes.push('\n');
    }
    preview_recipe.notes.push_str(&format!(
        "preview segment: start={start}s duration={duration}s (from recipe notes appended by reeldeck preview)"
    ));
    let plan = match rd_pipeline::preview_plan(
        &preview_recipe,
        &input.to_string_lossy(),
        &output.to_string_lossy(),
        start,
        duration,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("reeldeck preview: {e}");
            return 1;
        }
    };
    execute_run("preview", &preview_recipe, &input, &output, Some(plan))
}

const VERIFY_USAGE: &str = "reeldeck verify <output-file> [--json]";
const RERUN_USAGE: &str = "reeldeck rerun <export-log.json> [--in <file>] [--out <file>]";

/// Find `name` as given (the caller's working directory), then beside
/// `anchor_dir` (the directory of a related file — the output being
/// verified, or the log being re-run). Sidecars and their media are
/// moved together, so the directory of the file in hand is the
/// natural second place to look. Returns the path found and the
/// paths tried (so a miss can be reported honestly).
fn resolve_beside(name: &str, anchor_dir: Option<&Path>) -> (Option<PathBuf>, Vec<PathBuf>) {
    let direct = PathBuf::from(name);
    let mut tried = vec![direct.clone()];
    if direct.exists() {
        return (Some(direct), tried);
    }
    if let Some(dir) = anchor_dir {
        if !dir.as_os_str().is_empty() {
            let alt = dir.join(name);
            if alt != tried[0] {
                tried.push(alt.clone());
                if alt.exists() {
                    return (Some(alt), tried);
                }
            }
            // The name may itself carry a directory from elsewhere;
            // its bare file name beside the anchor is worth one try.
            if let Some(file_name) = Path::new(name).file_name() {
                let by_name = dir.join(file_name);
                if !tried.contains(&by_name) {
                    tried.push(by_name.clone());
                    if by_name.exists() {
                        return (Some(by_name), tried);
                    }
                }
            }
        }
    }
    (None, tried)
}

/// The directory a path lives in, for [`resolve_beside`] anchors.
fn parent_dir(path: &Path) -> Option<&Path> {
    path.parent().filter(|p| !p.as_os_str().is_empty())
}

fn parse_verify_args(args: &[String]) -> Result<(PathBuf, bool), u8> {
    let mut path: Option<PathBuf> = None;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            flag if flag.starts_with("--") => {
                eprintln!("reeldeck verify: unknown flag '{flag}'");
                eprintln!("usage: {VERIFY_USAGE}");
                return Err(2);
            }
            _ => {
                if path.is_some() {
                    eprintln!("reeldeck verify: unexpected extra argument '{arg}'");
                    eprintln!("usage: {VERIFY_USAGE}");
                    return Err(2);
                }
                path = Some(PathBuf::from(arg));
            }
        }
    }
    match path {
        Some(p) => Ok((p, json)),
        None => {
            eprintln!("usage: {VERIFY_USAGE}");
            Err(2)
        }
    }
}

struct RerunArgs {
    log_path: PathBuf,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
}

fn parse_rerun_args(args: &[String]) -> Result<RerunArgs, u8> {
    let mut out = RerunArgs {
        log_path: PathBuf::new(),
        input: None,
        output: None,
    };
    let mut have_log = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let value = |i: &mut usize| -> Result<String, u8> {
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| {
                eprintln!("reeldeck rerun: missing value for {arg}");
                eprintln!("usage: {RERUN_USAGE}");
                2u8
            })
        };
        match arg.as_str() {
            "--in" => out.input = Some(PathBuf::from(value(&mut i)?)),
            "--out" => out.output = Some(PathBuf::from(value(&mut i)?)),
            flag if flag.starts_with("--") => {
                eprintln!("reeldeck rerun: unknown flag '{flag}'");
                eprintln!("usage: {RERUN_USAGE}");
                return Err(2);
            }
            _ => {
                if have_log {
                    eprintln!("reeldeck rerun: unexpected extra argument '{arg}'");
                    eprintln!("usage: {RERUN_USAGE}");
                    return Err(2);
                }
                out.log_path = PathBuf::from(arg);
                have_log = true;
            }
        }
        i += 1;
    }
    if !have_log {
        eprintln!("usage: {RERUN_USAGE}");
        return Err(2);
    }
    Ok(out)
}

/// Read and parse one of an export's sidecars. A sidecar that cannot
/// be read or parsed is reported as what it is: a corrupt proof is a
/// failed verification/rerun, never a shrug.
fn load_sidecar<T>(
    cmd: &str,
    kind: &str,
    path: &Path,
    parse: impl Fn(&str) -> Result<T, serde_json::Error>,
) -> Result<T, i32> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("reeldeck {cmd}: cannot read {kind} {}: {e}", path.display());
            return Err(1);
        }
    };
    parse(&text).map_err(|e| {
        eprintln!(
            "reeldeck {cmd}: {} is not a valid {kind}: {e}",
            path.display()
        );
        1
    })
}

fn cmd_verify(args: &[String]) -> i32 {
    let (output, json) = match parse_verify_args(args) {
        Ok(v) => v,
        Err(c) => return c as i32,
    };
    if !output.exists() {
        eprintln!(
            "reeldeck verify: output file does not exist: {}",
            output.display()
        );
        return 1;
    }
    let (recipe_path, log_path) = rd_run::sidecar_paths(&output);
    for sidecar in [&recipe_path, &log_path] {
        if !sidecar.exists() {
            eprintln!(
                "reeldeck verify: missing sidecar {} — an export without its proof cannot be verified",
                sidecar.display()
            );
            return 1;
        }
    }
    let log = match load_sidecar(
        "verify",
        "export log",
        &log_path,
        rd_core::ExportLog::from_json,
    ) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let recipe = match load_sidecar("verify", "recipe", &recipe_path, rd_core::Recipe::from_json) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let output_hash = match rd_core::sha256_file(&output) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("reeldeck verify: cannot hash {}: {e}", output.display());
            return 1;
        }
    };
    // The source is where the recipe says it was; when the whole
    // set was moved, it sits beside the output. A source that is
    // nowhere to be found makes the source check SKIP (reported by
    // the check itself) — it never fails or passes sight unseen.
    let (source_found, source_tried) = resolve_beside(&recipe.source_name, parent_dir(&output));
    let source_hash = source_found
        .as_ref()
        .and_then(|p| rd_core::sha256_file(p).ok());
    // Joined sources are found the same way as the main source.
    let joined: Vec<(Option<PathBuf>, Option<String>)> = recipe
        .join
        .iter()
        .map(|part| {
            let (found, _) = resolve_beside(&part.source_name, parent_dir(&output));
            let hash = found.as_ref().and_then(|p| rd_core::sha256_file(p).ok());
            (found, hash)
        })
        .collect();
    let joined_hashes: Vec<Option<String>> = joined.iter().map(|(_, h)| h.clone()).collect();
    let report = rd_core::verify_joined_export(
        &log,
        &recipe,
        source_hash.as_deref(),
        Some(output_hash.as_str()),
        &joined_hashes,
    );
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(j) => println!("{j}"),
            Err(e) => {
                eprintln!("reeldeck verify: cannot serialize result: {e}");
                return 1;
            }
        }
    } else {
        println!("ReelDeck verify — {}", output.display());
        match (&source_found, &source_hash) {
            (Some(p), Some(h)) => println!("Source: {} (sha256 {h})", p.display()),
            (Some(p), None) => println!(
                "Source: {} (found but could not be hashed; the source check is skipped)",
                p.display()
            ),
            (None, _) => println!(
                "Source: not found (tried: {})",
                source_tried
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
        for (part, (found, hash)) in recipe.join.iter().zip(&joined) {
            match (found, hash) {
                (Some(p), Some(h)) => println!("Joined source: {} (sha256 {h})", p.display()),
                (Some(p), None) => println!(
                    "Joined source: {} (found but could not be hashed)",
                    p.display()
                ),
                (None, _) => println!("Joined source: {} (not found)", part.source_name),
            }
        }
        for c in &report.checks {
            let label = match c.status {
                rd_core::CheckStatus::Pass => "PASS",
                rd_core::CheckStatus::Fail => "FAIL",
                rd_core::CheckStatus::Skipped => "SKIP",
            };
            println!("{label} {} — {}", c.name, c.detail);
        }
        if report.ok() {
            println!("Verification: OK");
        } else {
            println!(
                "Verification: FAILED ({} failed, {} skipped)",
                report.failed(),
                report.skipped()
            );
        }
    }
    if report.ok() {
        0
    } else {
        1
    }
}

fn cmd_rerun(args: &[String]) -> i32 {
    let parsed = match parse_rerun_args(args) {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    let log = match load_sidecar(
        "rerun",
        "export log",
        &parsed.log_path,
        rd_core::ExportLog::from_json,
    ) {
        Ok(l) => l,
        Err(c) => return c,
    };
    // Rerunning a *failed* run's proof is a category error: there
    // is no recorded success to reproduce. The recipe can still be
    // run directly — say how.
    if !log.success {
        eprintln!(
            "reeldeck rerun: the recorded run did not succeed; \
             rerun its recipe directly with `export` if that is intended"
        );
        return 1;
    }
    // The recipe sidecar: beside the log first (the proof set is
    // moved as a set), then the recorded path as given.
    let (recorded_recipe_path, _) = rd_run::sidecar_paths(Path::new(&log.output_name));
    let recipe_path = match parent_dir(&parsed.log_path) {
        Some(dir) => {
            let beside = dir.join(&recorded_recipe_path);
            if beside.exists() {
                beside
            } else if let Some(file_name) = recorded_recipe_path.file_name() {
                let by_name = dir.join(file_name);
                if by_name.exists() {
                    by_name
                } else if recorded_recipe_path.exists() {
                    recorded_recipe_path.clone()
                } else {
                    beside
                }
            } else if recorded_recipe_path.exists() {
                recorded_recipe_path.clone()
            } else {
                beside
            }
        }
        None => recorded_recipe_path.clone(),
    };
    if !recipe_path.exists() {
        eprintln!(
            "reeldeck rerun: missing recipe sidecar {} — the recorded run's recipe cannot be re-run without it",
            recipe_path.display()
        );
        return 1;
    }
    let recipe = match load_sidecar("rerun", "recipe", &recipe_path, rd_core::Recipe::from_json) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let input = match parsed.input {
        Some(p) => {
            if !p.exists() {
                eprintln!("reeldeck rerun: input file does not exist: {}", p.display());
                return 1;
            }
            p
        }
        None => {
            let (found, tried) = resolve_beside(&recipe.source_name, parent_dir(&parsed.log_path));
            match found {
                Some(p) => p,
                None => {
                    eprintln!(
                        "reeldeck rerun: source file not found (tried: {})",
                        tried
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    return 1;
                }
            }
        }
    };
    let output = parsed.output.unwrap_or_else(|| {
        PathBuf::from(rerun_output_path(&log.output_name, recipe.export_preset))
    });
    let report = match run_render("rerun", &recipe, &input, &output, None) {
        Ok(r) => r,
        Err(c) => return c,
    };
    print_determinism(&log, &report);
    0
}

/// The determinism verdict for a rerun, decided purely from the
/// three facts the comparison has: whether the output files are
/// byte-equal, the recorded run's stream digest, and this run's
/// stream digest (see `ExportLog::output_stream_sha256`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeterminismVerdict {
    /// The output files hash identically.
    BitIdentical,
    /// The files differ, but both stream digests are present and
    /// equal: the elementary streams are identical, and only the
    /// container around them changed.
    StreamsBitIdentical,
    /// Both stream digests are present and they differ — the
    /// streams themselves changed, not only the container.
    StreamsDiffer,
    /// At least one stream digest is missing (a log written before
    /// stream hashing, or a streamhash pass that failed), so no
    /// stream-level comparison can be made.
    StreamComparisonUnavailable,
}

fn determinism_verdict(
    file_hashes_equal: bool,
    recorded_stream: Option<&str>,
    new_stream: Option<&str>,
) -> DeterminismVerdict {
    if file_hashes_equal {
        return DeterminismVerdict::BitIdentical;
    }
    match (recorded_stream, new_stream) {
        (Some(recorded), Some(new)) => {
            if recorded.eq_ignore_ascii_case(new) {
                DeterminismVerdict::StreamsBitIdentical
            } else {
                DeterminismVerdict::StreamsDiffer
            }
        }
        _ => DeterminismVerdict::StreamComparisonUnavailable,
    }
}

/// After a rerun, compare what was produced with what the recorded
/// run produced, at two levels: the output files, and — when both
/// logs carry one — the elementary-stream digests. Facts only:
/// equal hashes are stated as such; a file difference with equal
/// streams is named as container metadata; a stream difference is
/// reported with both hashes and both tool versions, so a changed
/// tool is named as the likely cause when the versions differ —
/// and when they do not, that is said plainly too.
fn print_determinism(recorded: &rd_core::ExportLog, report: &rd_run::RunReport) {
    let Some(recorded_hash) = &recorded.output_sha256 else {
        println!("Determinism: cannot compare — the recorded log carries no output hash");
        return;
    };
    // The rerun's own log is its record of the tool that ran and of
    // the streams it wrote; read it once for both.
    let new_log = std::fs::read_to_string(&report.log_path)
        .ok()
        .and_then(|text| rd_core::ExportLog::from_json(&text).ok());
    let recorded_stream = recorded.output_stream_sha256.as_deref();
    let new_stream = new_log
        .as_ref()
        .and_then(|log| log.output_stream_sha256.as_deref());
    let verdict = determinism_verdict(
        report.output_sha256.eq_ignore_ascii_case(recorded_hash),
        recorded_stream,
        new_stream,
    );
    match verdict {
        DeterminismVerdict::BitIdentical => {
            println!(
                "Determinism: bit-identical to the recorded run (sha256 {})",
                report.output_sha256
            );
        }
        DeterminismVerdict::StreamsBitIdentical => {
            println!(
                "Determinism: streams bit-identical to the recorded run; \
                 file bytes differ — the container carries per-run metadata \
                 (Matroska writes a Segment UUID and DateUTC)."
            );
            println!("  recorded run: sha256 {recorded_hash}");
            println!("  this run:     sha256 {}", report.output_sha256);
            if let Some(stream) = recorded_stream {
                println!("  streams:      sha256 {stream}");
            }
            print_ffmpeg_version_lines(recorded, new_log.as_ref());
        }
        DeterminismVerdict::StreamsDiffer => {
            println!("Determinism: output differs from the recorded run");
            println!("  recorded run: sha256 {recorded_hash}");
            println!("  this run:     sha256 {}", report.output_sha256);
            if let (Some(recorded_s), Some(new_s)) = (recorded_stream, new_stream) {
                println!(
                    "  streams differ too: recorded sha256 {recorded_s}, \
                     this run sha256 {new_s} — not only the container changed."
                );
            }
            print_version_comparison(recorded, new_log.as_ref());
        }
        DeterminismVerdict::StreamComparisonUnavailable => {
            println!("Determinism: output differs from the recorded run");
            println!("  recorded run: sha256 {recorded_hash}");
            println!("  this run:     sha256 {}", report.output_sha256);
            if recorded_stream.is_none() {
                println!(
                    "  Stream-level comparison unavailable: \
                     the recorded run predates stream hashing."
                );
            } else {
                println!(
                    "  Stream-level comparison unavailable: this run's stream \
                     hash could not be computed (its export log records why)."
                );
            }
            print_version_comparison(recorded, new_log.as_ref());
        }
    }
}

/// The recorded and current FFmpeg version lines, without any
/// commentary on causes.
fn print_ffmpeg_version_lines(recorded: &rd_core::ExportLog, new_log: Option<&rd_core::ExportLog>) {
    let recorded_version = recorded
        .ffmpeg_version
        .as_deref()
        .unwrap_or("unknown (not recorded)");
    let current_version = new_log.and_then(|log| log.ffmpeg_version.as_deref());
    println!("  recorded FFmpeg: {recorded_version}");
    println!(
        "  this run's FFmpeg: {}",
        current_version.unwrap_or("unknown")
    );
}

/// The recorded and current FFmpeg version lines, with the cause
/// commentary used when a difference is unexplained: a changed tool
/// version is named as the likely cause when the versions differ —
/// and when they do not, that is said plainly too.
fn print_version_comparison(recorded: &rd_core::ExportLog, new_log: Option<&rd_core::ExportLog>) {
    let recorded_version = recorded
        .ffmpeg_version
        .as_deref()
        .unwrap_or("unknown (not recorded)");
    let current_version = new_log.and_then(|log| log.ffmpeg_version.clone());
    match (&recorded.ffmpeg_version, &current_version) {
        (Some(a), Some(b)) if a != b => {
            println!("  recorded FFmpeg: {a}");
            println!("  this run's FFmpeg: {b}");
            println!(
                "  The tool version changed since the recorded run; \
                 a different build is the likely cause of the different bytes."
            );
        }
        (Some(_), Some(_)) => {
            println!("  recorded FFmpeg: {recorded_version}");
            println!(
                "  this run's FFmpeg: {}",
                current_version.as_deref().unwrap_or("unknown")
            );
            println!(
                "  Same recipe and tool version, different bytes — \
                 this run and the recorded run are both preserved; \
                 investigate before trusting either."
            );
        }
        _ => {
            println!("  recorded FFmpeg: {recorded_version}");
            println!(
                "  this run's FFmpeg: {}",
                current_version
                    .as_deref()
                    .unwrap_or("unknown (the new export log could not be read)")
            );
            println!(
                "  The tool versions could not both be read, so the \
                 cause cannot be narrowed down from the logs."
            );
        }
    }
}

const CUT_USAGE: &str =
    "reeldeck cut <file> --keep <start>-<end> [--keep …] (--lossless | --preset <preservation|mezzanine|access>) [--out <file>]";
const SPLIT_USAGE: &str =
    "reeldeck split <file> --at <time> [--at …] (--lossless | --preset <preservation|mezzanine|access>) [--dir <folder>]";

/// Shared flags of `cut` and `split`.
struct EditArgs {
    input: PathBuf,
    ranges: Vec<rd_core::KeepRange>,
    points: Vec<f64>,
    preset: rd_core::ExportPreset,
    out: Option<PathBuf>,
}

/// `12-30`, `1:00-1:30.5`, or `90-` (to the end of the source).
fn parse_keep_arg(raw: &str) -> Option<rd_core::KeepRange> {
    let (start, end) = raw.split_once('-')?;
    let start = rd_core::parse_time(start)?;
    let end = if end.trim().is_empty() {
        None
    } else {
        Some(rd_core::parse_time(end)?)
    };
    Some(rd_core::KeepRange::new(start, end))
}

fn parse_preset_name(raw: &str) -> Option<rd_core::ExportPreset> {
    match raw {
        "preservation" => Some(rd_core::ExportPreset::PreservationMaster),
        "mezzanine" => Some(rd_core::ExportPreset::Mezzanine),
        "access" => Some(rd_core::ExportPreset::Access),
        _ => None,
    }
}

fn parse_edit_args(args: &[String], cmd: &str, usage: &str) -> Result<EditArgs, u8> {
    let fail = |msg: String| {
        eprintln!("reeldeck {cmd}: {msg}");
        eprintln!("usage: {usage}");
        2u8
    };
    let mut input = None;
    let mut ranges = Vec::new();
    let mut points = Vec::new();
    let mut lossless = false;
    let mut preset = None;
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let mut value = || {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| fail(format!("missing value for {arg}")))
        };
        match arg {
            "--keep" if cmd == "cut" => {
                let v = value()?;
                ranges.push(parse_keep_arg(&v).ok_or_else(|| {
                    fail(format!(
                        "--keep expects <start>-<end> in seconds or h:mm:ss, got '{v}'"
                    ))
                })?);
            }
            "--at" if cmd == "split" => {
                let v = value()?;
                points.push(rd_core::parse_time(&v).ok_or_else(|| {
                    fail(format!(
                        "--at expects a time in seconds or h:mm:ss, got '{v}'"
                    ))
                })?);
            }
            "--lossless" => lossless = true,
            "--preset" => {
                let v = value()?;
                preset = Some(parse_preset_name(&v).ok_or_else(|| {
                    fail(format!(
                        "--preset expects preservation, mezzanine or access, got '{v}'"
                    ))
                })?);
            }
            "--out" if cmd == "cut" => out = Some(PathBuf::from(value()?)),
            "--dir" if cmd == "split" => out = Some(PathBuf::from(value()?)),
            flag if flag.starts_with("--") => return Err(fail(format!("unknown flag '{flag}'"))),
            _ if input.is_some() => return Err(fail(format!("unexpected extra argument '{arg}'"))),
            _ => input = Some(PathBuf::from(arg)),
        }
        i += 1;
    }
    let input = input.ok_or_else(|| fail("name the source file".to_string()))?;
    let preset = match (lossless, preset) {
        (true, None) => rd_core::ExportPreset::StreamCopy,
        (false, Some(p)) => p,
        (true, Some(_)) => {
            return Err(fail(
                "choose one: --lossless copies the streams, --preset re-encodes".to_string(),
            ))
        }
        (false, None) => {
            return Err(fail(
                "choose how to cut: --lossless (streams copied untouched; starts snap to keyframes) or --preset <name> (frame-accurate; re-encoded)".to_string(),
            ))
        }
    };
    if cmd == "cut" && ranges.is_empty() {
        return Err(fail("give at least one --keep range".to_string()));
    }
    if cmd == "split" && points.is_empty() {
        return Err(fail("give at least one --at split point".to_string()));
    }
    Ok(EditArgs {
        input,
        ranges,
        points,
        preset,
        out,
    })
}

/// The recipe for an edit of `input`, its source pinned by hash.
fn edit_recipe(
    cmd: &str,
    input: &Path,
    source_sha256: &str,
    preset: rd_core::ExportPreset,
    ranges: Vec<rd_core::KeepRange>,
) -> Result<rd_core::Recipe, i32> {
    let mut recipe = rd_core::Recipe::new(input.display().to_string(), preset);
    recipe.source_sha256 = Some(source_sha256.to_string());
    recipe.keep = ranges;
    recipe.notes = format!("written by reeldeck {cmd}");
    recipe.validate().map_err(|e| {
        eprintln!("reeldeck {cmd}: {e}");
        2
    })?;
    Ok(recipe)
}

fn hash_source(cmd: &str, input: &Path) -> Result<String, i32> {
    if !input.exists() {
        eprintln!(
            "reeldeck {cmd}: input file does not exist: {}",
            input.display()
        );
        return Err(1);
    }
    rd_core::sha256_file(input).map_err(|e| {
        eprintln!("reeldeck {cmd}: cannot hash {}: {e}", input.display());
        1
    })
}

fn cmd_cut(args: &[String]) -> i32 {
    let parsed = match parse_edit_args(args, "cut", CUT_USAGE) {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    let sha = match hash_source("cut", &parsed.input) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let recipe = match edit_recipe("cut", &parsed.input, &sha, parsed.preset, parsed.ranges) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let output = parsed.out.unwrap_or_else(|| {
        PathBuf::from(edited_output_path(
            &parsed.input,
            "cut",
            None,
            parsed.preset,
        ))
    });
    execute_run("cut", &recipe, &parsed.input, &output, None)
}

/// `capture.mkv` → `capture.cut.mkv` / `capture.part02.mkv`, with the
/// preset's extension.
fn edited_output_path(
    input: &Path,
    tag: &str,
    part: Option<usize>,
    preset: rd_core::ExportPreset,
) -> String {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    match part {
        Some(n) => format!("{stem}.{tag}{n:02}.{}", preset.extension()),
        None => format!("{stem}.{tag}.{}", preset.extension()),
    }
}

/// Split points into consecutive ranges: `[0, a]`, `[a, b]`, …, `[z, end]`.
fn split_ranges(points: &[f64]) -> Result<Vec<rd_core::KeepRange>, String> {
    let mut ranges = Vec::with_capacity(points.len() + 1);
    let mut start = 0.0;
    for &p in points {
        if p <= start {
            return Err(format!(
                "split points must be greater than 0 and increasing; {p}s does not come after {start}s"
            ));
        }
        ranges.push(rd_core::KeepRange::new(start, Some(p)));
        start = p;
    }
    ranges.push(rd_core::KeepRange::new(start, None));
    Ok(ranges)
}

/// Write `contents` to `path` as a new file, or accept an existing file
/// with exactly these contents (a re-run of the same split). A different
/// existing file is never overwritten.
fn write_or_match(path: &Path, contents: &str) -> Result<(), String> {
    match std::fs::read_to_string(path) {
        Ok(existing) if existing == contents => Ok(()),
        Ok(_) => Err(format!(
            "{} already exists with different contents; it is not overwritten. Choose another --dir",
            path.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
            use std::io::Write;
            file.write_all(contents.as_bytes())
                .map_err(|e| format!("cannot write {}: {e}", path.display()))
        }
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

fn cmd_split(args: &[String]) -> i32 {
    let parsed = match parse_edit_args(args, "split", SPLIT_USAGE) {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    let ranges = match split_ranges(&parsed.points) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("reeldeck split: {e}");
            return 2;
        }
    };
    let sha = match hash_source("split", &parsed.input) {
        Ok(s) => s,
        Err(c) => return c,
    };
    // The manifest lives in the split folder, so the source is named by
    // its absolute path.
    let input = match std::fs::canonicalize(&parsed.input) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "reeldeck split: cannot resolve {}: {e}",
                parsed.input.display()
            );
            return 1;
        }
    };
    let dir = parsed.out.unwrap_or_else(|| {
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".to_string());
        PathBuf::from(format!("{stem}.split"))
    });
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("reeldeck split: cannot create {}: {e}", dir.display());
        return 1;
    }
    let mut entries = Vec::with_capacity(ranges.len());
    for (i, range) in ranges.into_iter().enumerate() {
        let part = i + 1;
        let recipe = match edit_recipe("split", &input, &sha, parsed.preset, vec![range]) {
            Ok(r) => r,
            Err(c) => return c,
        };
        let recipe_name = format!("part{part:02}.recipe.json");
        let json = recipe.to_json().expect("a validated recipe serializes");
        if let Err(e) = write_or_match(&dir.join(&recipe_name), &json) {
            eprintln!("reeldeck split: {e}");
            return 1;
        }
        entries.push(rd_core::BatchEntry {
            recipe: recipe_name,
            input: Some(input.display().to_string()),
            output: Some(edited_output_path(
                &input,
                "part",
                Some(part),
                parsed.preset,
            )),
        });
    }
    let manifest = dir.join("split.json");
    let manifest_json = rd_core::BatchManifest::new(entries)
        .to_json()
        .expect("a manifest serializes");
    if let Err(e) = write_or_match(&manifest, &manifest_json) {
        eprintln!("reeldeck split: {e}");
        return 1;
    }
    println!("Split manifest: {}", manifest.display());
    cmd_batch(&[manifest.display().to_string()])
}

const JOIN_USAGE: &str =
    "reeldeck join <file> <file> [<file> …] (--lossless | --preset <preservation|mezzanine|access>) [--out <file>]";

struct JoinArgs {
    inputs: Vec<PathBuf>,
    preset: rd_core::ExportPreset,
    out: Option<PathBuf>,
}

fn parse_join_args(args: &[String]) -> Result<JoinArgs, u8> {
    let fail = |msg: String| {
        eprintln!("reeldeck join: {msg}");
        eprintln!("usage: {JOIN_USAGE}");
        2u8
    };
    let mut inputs = Vec::new();
    let mut lossless = false;
    let mut preset = None;
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let mut value = || {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| fail(format!("missing value for {arg}")))
        };
        match arg {
            "--lossless" => lossless = true,
            "--preset" => {
                let v = value()?;
                preset = Some(parse_preset_name(&v).ok_or_else(|| {
                    fail(format!(
                        "--preset expects preservation, mezzanine or access, got '{v}'"
                    ))
                })?);
            }
            "--out" => out = Some(PathBuf::from(value()?)),
            flag if flag.starts_with("--") => return Err(fail(format!("unknown flag '{flag}'"))),
            _ => inputs.push(PathBuf::from(arg)),
        }
        i += 1;
    }
    if inputs.len() < 2 {
        return Err(fail(
            "name at least two files to join, in order".to_string(),
        ));
    }
    let preset = match (lossless, preset) {
        (true, None) => rd_core::ExportPreset::StreamCopy,
        (false, Some(p)) => p,
        (true, Some(_)) => {
            return Err(fail(
                "choose one: --lossless copies the streams, --preset re-encodes".to_string(),
            ))
        }
        (false, None) => {
            return Err(fail(
                "choose how to join: --lossless (streams copied untouched; every file must share codecs and picture size) or --preset <name> (re-encoded)".to_string(),
            ))
        }
    };
    Ok(JoinArgs {
        inputs,
        preset,
        out,
    })
}

fn cmd_join(args: &[String]) -> i32 {
    let parsed = match parse_join_args(args) {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    let mut hashes = Vec::with_capacity(parsed.inputs.len());
    for input in &parsed.inputs {
        match hash_source("join", input) {
            Ok(h) => hashes.push(h),
            Err(c) => return c,
        }
    }
    let first = &parsed.inputs[0];
    let mut recipe = rd_core::Recipe::new(first.display().to_string(), parsed.preset);
    recipe.source_sha256 = Some(hashes[0].clone());
    for (input, hash) in parsed.inputs.iter().zip(&hashes).skip(1) {
        recipe = recipe.join_source(input.display().to_string(), Some(hash.clone()));
    }
    recipe.notes = "written by reeldeck join".to_string();
    if let Err(e) = recipe.validate() {
        eprintln!("reeldeck join: {e}");
        return 2;
    }
    let output = parsed
        .out
        .unwrap_or_else(|| PathBuf::from(edited_output_path(first, "joined", None, parsed.preset)));
    execute_run("join", &recipe, first, &output, None)
}

const BATCH_USAGE: &str = "reeldeck batch <manifest.json> [--keep-going] [--status]";

struct BatchArgs {
    manifest: PathBuf,
    keep_going: bool,
    status_only: bool,
}

fn parse_batch_args(args: &[String]) -> Result<BatchArgs, u8> {
    let mut manifest = None;
    let mut keep_going = false;
    let mut status_only = false;
    for arg in args {
        match arg.as_str() {
            "--keep-going" => keep_going = true,
            "--status" => status_only = true,
            flag if flag.starts_with("--") => {
                eprintln!("reeldeck batch: unknown flag '{flag}'");
                eprintln!("usage: {BATCH_USAGE}");
                return Err(2);
            }
            _ if manifest.is_some() => {
                eprintln!("reeldeck batch: unexpected extra argument '{arg}'");
                eprintln!("usage: {BATCH_USAGE}");
                return Err(2);
            }
            _ => manifest = Some(PathBuf::from(arg)),
        }
    }
    match manifest {
        Some(manifest) => Ok(BatchArgs {
            manifest,
            keep_going,
            status_only,
        }),
        None => {
            eprintln!("usage: {BATCH_USAGE}");
            Err(2)
        }
    }
}

fn cmd_batch(args: &[String]) -> i32 {
    let parsed = match parse_batch_args(args) {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    if parsed.status_only {
        return match rd_run::batch::load_state(&parsed.manifest) {
            Ok(Some(state)) => {
                print_batch_state(&state);
                0
            }
            Ok(None) => {
                println!(
                    "Batch {} has not started (no {}).",
                    parsed.manifest.display(),
                    rd_run::batch::state_path_for(&parsed.manifest).display()
                );
                0
            }
            Err(e) => {
                eprintln!("reeldeck batch: {e}");
                1
            }
        };
    }

    let opts = rd_run::batch::BatchOptions {
        keep_going: parsed.keep_going,
        cancel: Arc::new(AtomicBool::new(false)),
        on_event: Some(Arc::new(std::sync::Mutex::new({
            let mut total_frames = None;
            move |event: &rd_run::batch::BatchEvent| print_batch_event(event, &mut total_frames)
        }))),
        frame_total: None,
    };
    let report = match rd_run::batch::run_batch(&parsed.manifest, opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("reeldeck batch: {e}");
            return 1;
        }
    };
    println!();
    print_batch_state(&report.state);
    println!("State: {}", report.state_path.display());
    if report.is_complete() {
        return 0;
    }
    if let Some(i) = report.state.next_unfinished() {
        let status = report.state.entries[i].status;
        if report.cancelled {
            eprintln!("reeldeck batch: cancelled at entry {}.", i + 1);
        } else if status == rd_core::EntryStatus::Failed {
            eprintln!(
                "reeldeck batch: stopped; entry {} failed. Fix the cause and run the same command again to resume at entry {}.",
                i + 1,
                i + 1
            );
        } else {
            eprintln!(
                "reeldeck batch: unfinished; run the same command again to resume at entry {}.",
                i + 1
            );
        }
    }
    1
}

/// Print one batch event. `total_frames` carries the running entry's
/// frame total from its `Started` event to its progress lines.
fn print_batch_event(event: &rd_run::batch::BatchEvent, total_frames: &mut Option<u64>) {
    use rd_run::batch::BatchEvent;
    match event {
        BatchEvent::Skipped { index, output } => {
            println!(
                "[{index}] skip: already succeeded, output re-verified ({})",
                output.display()
            );
        }
        BatchEvent::Started {
            index,
            attempt,
            input,
            output,
            total_frames: total,
        } => {
            *total_frames = *total;
            let retry = if *attempt > 1 {
                format!(" (attempt {attempt})")
            } else {
                String::new()
            };
            println!(
                "[{index}] start: {} -> {}{retry}",
                input.display(),
                output.display()
            );
        }
        BatchEvent::Progress { index, progress } => {
            println!("[{index}] {}", batch_progress_text(progress, *total_frames));
        }
        BatchEvent::Finished {
            index,
            status,
            detail,
            ..
        } => {
            if *status == rd_core::EntryStatus::Succeeded {
                println!("[{index}] succeeded: SHA-256 {detail}");
            } else {
                println!("[{index}] {}: {detail}", status.label());
            }
        }
    }
}

/// `frame 50/200 (25%)  fps …` with a known total; the bare frame
/// count without one, so no percentage is ever guessed.
fn batch_progress_text(progress: &rd_run::Progress, total_frames: Option<u64>) -> String {
    let frames = match total_frames.filter(|t| *t > 0) {
        Some(total) => format!(
            "frame {}/{total} ({}%)",
            progress.frame,
            (progress.frame.saturating_mul(100) / total).min(100)
        ),
        None => format!("frame {}", progress.frame),
    };
    format!(
        "{frames}  fps {:.1}  speed {:.2}x",
        progress.fps, progress.speed
    )
}

fn print_batch_state(state: &rd_core::BatchState) {
    let counts = state.counts();
    println!(
        "Batch: {} entries: {} succeeded, {} failed, {} cancelled, {} pending, {} interrupted",
        state.entries.len(),
        counts.succeeded,
        counts.failed,
        counts.cancelled,
        counts.pending,
        counts.running
    );
    for (i, entry) in state.entries.iter().enumerate() {
        let attempts = entry.attempts.len();
        let output = entry
            .last_attempt()
            .map(|a| a.output.as_str())
            .unwrap_or(entry.output.as_str());
        println!(
            "  {:>3}. {:<10} {} -> {} ({} attempt{})",
            i + 1,
            entry.status.label(),
            entry.input,
            output,
            attempts,
            if attempts == 1 { "" } else { "s" }
        );
        if let Some(error) = entry.last_attempt().and_then(|a| a.error.as_deref()) {
            if entry.status != rd_core::EntryStatus::Succeeded {
                println!("       {error}");
            }
        }
    }
}

/// `capture.restored.mkv` -> `capture.restored.rerun.mkv`: the rerun
/// is a new file beside the caller's working directory, visibly a
/// rerun of the recorded output.
fn rerun_output_path(output_name: &str, preset: rd_core::ExportPreset) -> String {
    let path = Path::new(output_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    let ext = path
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| preset.extension().to_string());
    format!("{stem}.rerun.{ext}")
}

/// `clip.mkv` + PreservationMaster -> `clip.preview.mkv`.
fn preview_output_path(input: &Path, preset: rd_core::ExportPreset) -> String {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    format!("{stem}.preview.{}", preset.extension())
}

/// `capture.mkv` + PreservationMaster -> `capture.restored.mkv`.
fn derived_output_path(source_name: &str, preset: rd_core::ExportPreset) -> String {
    let stem = Path::new(source_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    format!("{stem}.restored.{}", preset.extension())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> rd_analyze::AnalysisReport {
        use rd_analyze::*;
        AnalysisReport {
            source: "clip<script>.mkv".to_string(),
            range: SampleRange::new(1.0, Some(2.0)),
            probe: ProbeInfo {
                format_name: "matroska,webm".to_string(),
                duration_secs: 3.0,
                size_bytes: 123_456,
                bit_rate: None,
                streams: vec![StreamInfo {
                    index: 0,
                    codec_type: "video".to_string(),
                    codec_name: "ffv1".to_string(),
                    width: Some(720),
                    height: Some(576),
                    avg_frame_rate: Some("25/1".to_string()),
                    field_order: Some("tt".to_string()),
                    sample_rate: None,
                    channels: None,
                }],
            },
            frames_analyzed: 50,
            signal: Some(SignalSummary {
                y_avg: MetricSummary {
                    min: 10.0,
                    max: 100.0,
                    mean: 55.0,
                },
                sat_avg: MetricSummary {
                    min: 0.0,
                    max: 20.0,
                    mean: 5.0,
                },
                tout: MetricSummary {
                    min: 0.0,
                    max: 0.5,
                    mean: 0.01,
                },
                brng: MetricSummary {
                    min: 0.0,
                    max: 0.0,
                    mean: 0.0,
                },
                frames: 50,
            }),
            anomalies: vec![Anomaly::BlackSpan {
                start_secs: 1.5,
                end_secs: 2.0,
            }],
            idet: IdetCounts {
                single_progressive: 4,
                multi_progressive: 51,
                ..Default::default()
            },
            suggestion: InterlaceSuggestion::Progressive,
            tout_threshold: 0.05,
        }
    }

    #[test]
    fn render_text_contains_key_sections() {
        let text = render_text(&sample_report());
        for needle in [
            "ReelDeck inspect",
            "Format: matroska,webm",
            "Video: ffv1 720x576 @ 25/1 fps",
            "field_order: tt",
            "Range: start 1.00s, duration 2.00s",
            "Frames analyzed: 50",
            "YAVG",
            "Interlace suggestion: Progressive",
            "Basis (idet counts)",
            "Anomalies (1)",
            "Black span",
            "confirm them in the viewer",
        ] {
            assert!(
                text.contains(needle),
                "text report missing {needle:?}:\n{text}"
            );
        }
    }

    #[test]
    fn render_html_contains_sections_and_escapes_source() {
        let html = render_html(&sample_report());
        for needle in [
            "<!DOCTYPE html>",
            "<h2>Source</h2>",
            "<h2>Signal</h2>",
            "<h2>Interlace suggestion</h2>",
            "<h2>Anomalies</h2>",
            "Black span",
            "clip&lt;script&gt;.mkv",
        ] {
            assert!(html.contains(needle), "html report missing {needle:?}");
        }
        assert!(
            !html.contains("clip<script>.mkv"),
            "raw user-controlled path leaked into HTML"
        );
        assert!(!html.contains("http"), "no external assets allowed");
    }

    #[test]
    fn parse_inspect_args_defaults_and_flags() {
        let args: Vec<String> = ["clip.mkv"].iter().map(ToString::to_string).collect();
        let opts = parse_inspect_args(&args).expect("defaults parse");
        assert_eq!(opts.path, PathBuf::from("clip.mkv"));
        assert_eq!(opts.start, 0.0);
        assert_eq!(opts.duration, None);
        assert!(!opts.json);
        assert!(opts.html.is_none());

        let args: Vec<String> = [
            "--json",
            "clip.mkv",
            "--start",
            "5.5",
            "--duration",
            "2",
            "--tout",
            "0.2",
            "--html",
            "out.html",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let opts = parse_inspect_args(&args).expect("flags parse");
        assert_eq!(opts.start, 5.5);
        assert_eq!(opts.duration, Some(2.0));
        assert_eq!(opts.tout, 0.2);
        assert!(opts.json);
        assert_eq!(opts.html, Some(PathBuf::from("out.html")));
    }

    #[test]
    fn parse_inspect_args_rejects_bad_values() {
        for bad in [
            vec!["clip.mkv", "--start", "abc"],
            vec!["clip.mkv", "--start", "-1"],
            vec!["clip.mkv", "--duration", "0"],
            vec!["clip.mkv", "--tout", "2"],
            vec!["clip.mkv", "--nope"],
            vec!["clip.mkv", "--start"],
            vec!["a.mkv", "b.mkv"],
            vec![],
        ] {
            let args: Vec<String> = bad.iter().map(ToString::to_string).collect();
            assert!(
                parse_inspect_args(&args).is_err(),
                "{bad:?} must be a usage error"
            );
        }
    }

    #[test]
    fn preview_output_names_follow_preset() {
        assert_eq!(
            preview_output_path(
                Path::new("clip.mkv"),
                rd_core::ExportPreset::PreservationMaster
            ),
            "clip.preview.mkv"
        );
        assert_eq!(
            preview_output_path(Path::new("clip.mp4"), rd_core::ExportPreset::Access),
            "clip.preview.mp4"
        );
    }

    #[test]
    fn parse_verify_args_accepts_path_and_json() {
        let args: Vec<String> = ["out.mkv"].iter().map(ToString::to_string).collect();
        let (path, json) = parse_verify_args(&args).expect("path only");
        assert_eq!(path, PathBuf::from("out.mkv"));
        assert!(!json);
        let args: Vec<String> = ["--json", "out.mkv"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let (path, json) = parse_verify_args(&args).expect("json flag");
        assert_eq!(path, PathBuf::from("out.mkv"));
        assert!(json);
        for bad in [
            vec![],
            vec!["--json"],
            vec!["a.mkv", "b.mkv"],
            vec!["a.mkv", "--nope"],
        ] {
            let args: Vec<String> = bad.iter().map(ToString::to_string).collect();
            assert!(
                parse_verify_args(&args).is_err(),
                "{bad:?} must be a usage error"
            );
        }
    }

    #[test]
    fn parse_rerun_args_accepts_log_and_flags() {
        let args: Vec<String> = ["out.mkv.export-log.json"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let parsed = parse_rerun_args(&args).expect("log only");
        assert_eq!(parsed.log_path, PathBuf::from("out.mkv.export-log.json"));
        assert!(parsed.input.is_none());
        assert!(parsed.output.is_none());
        let args: Vec<String> = ["log.json", "--in", "src.mkv", "--out", "again.mkv"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let parsed = parse_rerun_args(&args).expect("flags");
        assert_eq!(parsed.input, Some(PathBuf::from("src.mkv")));
        assert_eq!(parsed.output, Some(PathBuf::from("again.mkv")));
        for bad in [
            vec![],
            vec!["--in", "x.mkv"],
            vec!["log.json", "--in"],
            vec!["log.json", "--nope"],
            vec!["a.json", "b.json"],
        ] {
            let args: Vec<String> = bad.iter().map(ToString::to_string).collect();
            assert!(
                parse_rerun_args(&args).is_err(),
                "{bad:?} must be a usage error"
            );
        }
    }

    #[test]
    fn determinism_verdict_levels() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        // File equality decides outright; the stream digests are
        // not consulted (either may be missing).
        assert_eq!(
            determinism_verdict(true, None, None),
            DeterminismVerdict::BitIdentical
        );
        assert_eq!(
            determinism_verdict(true, Some(&a), Some(&b)),
            DeterminismVerdict::BitIdentical
        );
        // Files differ, streams equal (case-insensitively).
        assert_eq!(
            determinism_verdict(false, Some(&a), Some(&a)),
            DeterminismVerdict::StreamsBitIdentical
        );
        assert_eq!(
            determinism_verdict(false, Some(&a.to_uppercase()), Some(&a)),
            DeterminismVerdict::StreamsBitIdentical
        );
        // Files and streams differ.
        assert_eq!(
            determinism_verdict(false, Some(&a), Some(&b)),
            DeterminismVerdict::StreamsDiffer
        );
        // Any missing stream digest: no stream-level comparison.
        assert_eq!(
            determinism_verdict(false, None, Some(&a)),
            DeterminismVerdict::StreamComparisonUnavailable
        );
        assert_eq!(
            determinism_verdict(false, Some(&a), None),
            DeterminismVerdict::StreamComparisonUnavailable
        );
        assert_eq!(
            determinism_verdict(false, None, None),
            DeterminismVerdict::StreamComparisonUnavailable
        );
    }

    #[test]
    fn rerun_output_names_mark_the_rerun() {
        assert_eq!(
            rerun_output_path(
                "capture.restored.mkv",
                rd_core::ExportPreset::PreservationMaster
            ),
            "capture.restored.rerun.mkv"
        );
        // Only the file name matters; a recorded directory is not
        // carried into the caller's working directory.
        assert_eq!(
            rerun_output_path("some/dir/clip.restored.mp4", rd_core::ExportPreset::Access),
            "clip.restored.rerun.mp4"
        );
        assert_eq!(
            rerun_output_path("noext", rd_core::ExportPreset::Mezzanine),
            "noext.rerun.mov"
        );
    }

    #[test]
    fn resolve_beside_finds_direct_then_anchor_dir() {
        let dir = std::env::temp_dir().join(format!("reeldeck-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let beside = dir.join("media.mkv");
        std::fs::write(&beside, b"x").expect("write beside file");
        // Missing everywhere: no hit, and the tried list says where
        // it looked.
        let (found, tried) = resolve_beside("absent.mkv", Some(&dir));
        assert!(found.is_none());
        assert!(tried.iter().any(|p| p.ends_with("absent.mkv")));
        // Present beside the anchor.
        let (found, _) = resolve_beside("media.mkv", Some(&dir));
        assert_eq!(found, Some(beside.clone()));
        // A name carrying a foreign directory still resolves by its
        // bare file name beside the anchor.
        let (found, _) = resolve_beside("elsewhere/media.mkv", Some(&dir));
        assert_eq!(found, Some(beside));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keep_args_parse_ranges_and_open_ends() {
        assert_eq!(
            parse_keep_arg("12-30.5"),
            Some(rd_core::KeepRange::new(12.0, Some(30.5)))
        );
        assert_eq!(
            parse_keep_arg("1:00-1:30"),
            Some(rd_core::KeepRange::new(60.0, Some(90.0)))
        );
        assert_eq!(
            parse_keep_arg("90-"),
            Some(rd_core::KeepRange::new(90.0, None))
        );
        for bad in ["", "12", "a-b", "-5", "1:60-2:00"] {
            assert_eq!(parse_keep_arg(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn edit_args_require_an_explicit_cut_mode() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let lossless = parse_edit_args(
            &args(&["in.mp4", "--keep", "1-2", "--lossless"]),
            "cut",
            CUT_USAGE,
        )
        .unwrap();
        assert_eq!(lossless.preset, rd_core::ExportPreset::StreamCopy);
        let accurate = parse_edit_args(
            &args(&["in.mp4", "--keep", "1-2", "--preset", "access"]),
            "cut",
            CUT_USAGE,
        )
        .unwrap();
        assert_eq!(accurate.preset, rd_core::ExportPreset::Access);
        assert_eq!(
            parse_edit_args(&args(&["in.mp4", "--keep", "1-2"]), "cut", CUT_USAGE).err(),
            Some(2)
        );
        assert_eq!(
            parse_edit_args(
                &args(&[
                    "in.mp4",
                    "--keep",
                    "1-2",
                    "--lossless",
                    "--preset",
                    "access"
                ]),
                "cut",
                CUT_USAGE
            )
            .err(),
            Some(2)
        );
        assert_eq!(
            parse_edit_args(&args(&["in.mp4", "--lossless"]), "cut", CUT_USAGE).err(),
            Some(2)
        );
        assert_eq!(
            parse_edit_args(
                &args(&["in.mp4", "--at", "5", "--lossless"]),
                "cut",
                CUT_USAGE
            )
            .err(),
            Some(2),
            "--at belongs to split"
        );
        let split = parse_edit_args(
            &args(&[
                "in.mp4",
                "--at",
                "5",
                "--at",
                "1:00",
                "--preset",
                "mezzanine",
            ]),
            "split",
            SPLIT_USAGE,
        )
        .unwrap();
        assert_eq!(split.points, vec![5.0, 60.0]);
    }

    #[test]
    fn join_args_need_two_files_and_a_mode() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let ok =
            parse_join_args(&args(&["a.mp4", "b.avi", "c.mov", "--preset", "mezzanine"])).unwrap();
        assert_eq!(ok.inputs.len(), 3);
        assert_eq!(ok.preset, rd_core::ExportPreset::Mezzanine);
        assert_eq!(
            parse_join_args(&args(&["a.mp4", "b.mp4", "--lossless"]))
                .unwrap()
                .preset,
            rd_core::ExportPreset::StreamCopy
        );
        assert_eq!(
            parse_join_args(&args(&["a.mp4", "--lossless"])).err(),
            Some(2)
        );
        assert_eq!(parse_join_args(&args(&["a.mp4", "b.mp4"])).err(), Some(2));
        assert_eq!(
            parse_join_args(&args(&["a.mp4", "b.mp4", "--keep", "1-2"])).err(),
            Some(2)
        );
    }

    #[test]
    fn split_points_become_consecutive_ranges() {
        assert_eq!(
            split_ranges(&[5.0, 12.0]).unwrap(),
            vec![
                rd_core::KeepRange::new(0.0, Some(5.0)),
                rd_core::KeepRange::new(5.0, Some(12.0)),
                rd_core::KeepRange::new(12.0, None),
            ]
        );
        assert!(split_ranges(&[0.0]).is_err());
        assert!(split_ranges(&[5.0, 5.0]).is_err());
        assert!(split_ranges(&[9.0, 3.0]).is_err());
    }

    #[test]
    fn edited_output_names() {
        let p = Path::new("dir/tape.mkv");
        assert_eq!(
            edited_output_path(p, "cut", None, rd_core::ExportPreset::Access),
            "tape.cut.mp4"
        );
        assert_eq!(
            edited_output_path(p, "part", Some(3), rd_core::ExportPreset::StreamCopy),
            "tape.part03.mkv"
        );
    }

    #[test]
    fn batch_progress_shows_a_percentage_only_with_a_total() {
        let progress = rd_run::Progress {
            frame: 50,
            fps: 25.0,
            speed: 1.0,
            ..rd_run::Progress::default()
        };
        assert_eq!(
            batch_progress_text(&progress, Some(200)),
            "frame 50/200 (25%)  fps 25.0  speed 1.00x"
        );
        assert_eq!(
            batch_progress_text(&progress, None),
            "frame 50  fps 25.0  speed 1.00x"
        );
        assert_eq!(
            batch_progress_text(&progress, Some(0)),
            "frame 50  fps 25.0  speed 1.00x"
        );
        let over = rd_run::Progress {
            frame: 260,
            ..progress
        };
        assert!(batch_progress_text(&over, Some(200)).starts_with("frame 260/200 (100%)"));
    }

    #[test]
    fn parse_batch_args_accepts_manifest_and_flags() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let parsed = parse_batch_args(&args(&["b.json", "--keep-going"])).expect("parse");
        assert_eq!(parsed.manifest, PathBuf::from("b.json"));
        assert!(parsed.keep_going && !parsed.status_only);
        assert!(
            parse_batch_args(&args(&["--status", "b.json"]))
                .expect("parse")
                .status_only
        );
        assert_eq!(parse_batch_args(&args(&[])).err(), Some(2));
        assert_eq!(
            parse_batch_args(&args(&["a.json", "b.json"])).err(),
            Some(2)
        );
        assert_eq!(
            parse_batch_args(&args(&["b.json", "--bogus"])).err(),
            Some(2)
        );
    }

    #[test]
    fn derived_output_names_follow_preset() {
        assert_eq!(
            derived_output_path(
                "capture_1987.mkv",
                rd_core::ExportPreset::PreservationMaster
            ),
            "capture_1987.restored.mkv"
        );
        assert_eq!(
            derived_output_path("clip.mp4", rd_core::ExportPreset::Access),
            "clip.restored.mp4"
        );
    }
}
