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
    match rd_pipeline::export_plan(&recipe, &recipe.source_name, &output) {
        Ok(plan) => {
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
/// does not.
fn execute_run(
    cmd: &str,
    recipe: &rd_core::Recipe,
    input: &Path,
    output: &Path,
    preview_plan: Option<rd_pipeline::ExportPlan>,
) -> i32 {
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
            0
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
            1
        }
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
