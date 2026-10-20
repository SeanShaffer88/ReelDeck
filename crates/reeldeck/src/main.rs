//! ReelDeck command-line entry point.
//!
//! Hand-rolled argument parsing on purpose: the workspace keeps a tiny,
//! stdlib-first dependency set (serde / serde_json / thiserror), and the
//! CLI surface is four subcommands. If that stops being true, revisit.
//!
//! Subcommands:
//!   reeldeck inspect <file>       Probe a media file and print the result
//!   reeldeck plan <recipe.json>   Print the export plan for a recipe
//!   reeldeck hash <file>          Print the file's SHA-256
//!   reeldeck version              Print the version

use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
         \x20 reeldeck inspect <file>       Probe a media file (ffprobe) and print the result\n\
         \x20 reeldeck plan <recipe.json>   Print the export plan (argument vector) for a recipe\n\
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

fn cmd_inspect(args: &[String]) -> i32 {
    let path = match one_file_arg(args, "reeldeck inspect <file>") {
        Ok(p) => p,
        Err(c) => return c as i32,
    };
    match rd_analyze::probe(&path) {
        Ok(info) => {
            match serde_json::to_string_pretty(&info) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("reeldeck inspect: cannot serialize result: {e}");
                    return 1;
                }
            }
            0
        }
        // ToolUnavailable and friends are reported plainly, never faked:
        // inspection without the tool is impossible, so say so.
        Err(e) => {
            eprintln!("reeldeck inspect: {e}");
            1
        }
    }
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
