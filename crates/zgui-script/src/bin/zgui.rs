//! `zgui`: developer tools for zgui-script.
//!
//! `zgui check [--nonstrict] <file-or-dir>...` type checks Luau scripts
//! with luau-lsp and the bundled `ui`/`anime` definitions, printing
//! `file:line:col` findings; it exits 1 when there are errors.
//!
//! `zgui lint <file-or-dir>...` reports patterns that cost far more than
//! they look or do not do what they look like (`zgui_script::lint`); it
//! exits 1 when it finds any.
use std::{path::PathBuf, process::ExitCode};
use zgui_script::check::{CheckError, Options, Report, check};

const USAGE: &str = "usage: zgui check [--nonstrict] <file-or-dir>...
       zgui lint [--format text|json] <file-or-dir>...
       zgui lint --rules

check: type checks zgui Luau scripts with luau-lsp, the bundled ui/anime
types, the standard platform and the new type solver. Strict mode applies
unless a .luaurc (languageMode) or a --!nonstrict comment says otherwise.

lint: zgui's best-practice rules (no luau-lsp needed). A finding names the
cheaper way to write it; `-- zgui-lint: allow(rule)` on its line or the
line above accepts a deliberate exception. It exits 1 when it finds any;
`--format json` prints them as one JSON object for tools.";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => {}
        Some("lint") => return lint(args.collect()),
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    let mut options = Options::default();
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--nonstrict" | "--non-strict" => options.strict = false,
            "--strict" => options.strict = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with('-') => {
                eprintln!("zgui check: unknown option {flag}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    if paths.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match check(&paths, &options) {
        Ok(report) => {
            print_report(&report);
            if report.errors() > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error @ CheckError::NotInstalled) => {
            eprintln!("zgui check: {error}");
            ExitCode::from(3)
        }
        Err(error) => {
            eprintln!("zgui check: {error}");
            ExitCode::from(2)
        }
    }
}

fn print_report(report: &Report) {
    for d in &report.diagnostics {
        println!("{d}");
    }
    let plural = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
    eprintln!(
        "{}, {} in {}",
        plural(report.errors(), "error"),
        plural(report.warnings(), "warning"),
        plural(report.files, "file")
    );
}

fn lint(args: Vec<String>) -> ExitCode {
    let mut paths = Vec::new();
    let mut json = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => match args.next().as_deref() {
                Some("json") => json = true,
                Some("text") => json = false,
                other => {
                    eprintln!("zgui lint: --format takes text or json, not {other:?}\n\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--json" => json = true,
            "--rules" => {
                for (rule, summary) in zgui_script::lint::RULES {
                    println!("{rule:18} {summary}");
                }
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with('-') => {
                eprintln!("zgui lint: unknown option {flag}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    if paths.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match zgui_script::lint::lint(&paths) {
        Ok(report) if json => {
            let findings: Vec<_> = report
                .diagnostics
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "file": d.file.display().to_string(),
                        "line": d.line,
                        "column": d.column,
                        "rule": d.code,
                        "message": d.message,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({ "files": report.files, "findings": findings })
            );
            if report.diagnostics.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Ok(report) => {
            print_report(&report);
            if report.diagnostics.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("zgui lint: {error}");
            ExitCode::from(2)
        }
    }
}
