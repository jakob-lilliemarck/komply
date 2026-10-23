use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use komply::config::Config;
use komply::services::testing::Service;
use komply::services::testing::model::{PreparedTest, Report, TestRequest, Verdict};
use komply::services::testing::repositories::decisions::DecisionsRepository;
use komply::services::testing::repositories::files::FilesRepository;
use komply::services::testing::repositories::rules::RulesRepository;

const USAGE: &str = "Usage:\n  comply test <dir> [--rules <file-or-dir>] [--dry-run] [--model <id>]\n  comply --help\n  comply test --help";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match parse_args(&args) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Test(args)) => match run(args) {
            Ok(code) => ExitCode::from(code),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::from(2)
            }
        },
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Test(Args),
    Help,
}

#[derive(Debug, PartialEq, Eq)]
struct Args {
    target: PathBuf,
    rules: Option<PathBuf>,
    dry_run: bool,
    model: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    let Some(first) = args.first().map(String::as_str) else {
        return Err(usage_error("missing command"));
    };

    if first == "--help" || first == "-h" {
        return Ok(Command::Help);
    }
    if first != "test" {
        return Err(usage_error(&format!("unknown command `{first}`")));
    }

    let mut target = None;
    let mut rules = None;
    let mut dry_run = false;
    let mut model = None;
    let mut index = 1;

    while index < args.len() {
        let argument = args[index].as_str();
        match argument {
            "--help" | "-h" => return Ok(Command::Help),
            "--rules" => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.starts_with('-'))
                    .ok_or_else(|| usage_error("flag `--rules` needs a value"))?;
                rules = Some(PathBuf::from(value));
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--model" => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.starts_with('-'))
                    .ok_or_else(|| usage_error("flag `--model` needs a value"))?;
                model = Some(value.clone());
                index += 2;
            }
            _ if argument.starts_with('-') => {
                return Err(usage_error(&format!("unknown flag `{argument}`")));
            }
            _ => {
                if target.is_some() {
                    return Err(usage_error(&format!("unexpected argument `{argument}`")));
                }
                target = Some(PathBuf::from(argument));
                index += 1;
            }
        }
    }

    let target = target.ok_or_else(|| usage_error("missing target"))?;

    Ok(Command::Test(Args {
        target,
        rules,
        dry_run,
        model,
    }))
}

fn usage_error(message: &str) -> String {
    format!("error: {message}\n\n{USAGE}")
}

fn run(args: Args) -> Result<u8, String> {
    let Args {
        target: target_arg,
        rules,
        dry_run,
        model,
    } = args;

    let cwd = std::env::current_dir()
        .map_err(|error| format!("could not read the working directory: {error}"))?;
    let root = std::fs::canonicalize(&cwd).map_err(|error| {
        format!(
            "could not read the working directory `{}`: {error}",
            cwd.display()
        )
    })?;

    let target_input = if target_arg.is_absolute() {
        target_arg.clone()
    } else {
        cwd.join(&target_arg)
    };
    let target = std::fs::canonicalize(&target_input)
        .map_err(|_| format!("target is not a directory: `{}`", target_input.display()))?;
    if !target.is_dir() {
        return Err(format!(
            "target is not a directory: `{}`",
            target_input.display()
        ));
    }

    let mut config = Config::load(&root).map_err(|error| error.to_string())?;
    if let Some(model) = model {
        config.model = model;
    }
    let timeout = config.request_timeout;

    let service = Service::new(RulesRepository::new(), FilesRepository::new(), config);
    let prepared = service
        .prepare(TestRequest {
            root: root.clone(),
            target,
            rules,
        })
        .map_err(|error| error.to_string())?;

    if dry_run {
        print!("{}", dry_run_text(&prepared)?);
        return Ok(0);
    }

    let decisions = DecisionsRepository::from_env(timeout).map_err(|error| error.to_string())?;

    print_header(&prepared);
    print_skipped(&prepared);
    println!();
    print_pending(&prepared);
    let report = service
        .run(prepared, &decisions)
        .map_err(|error| error.to_string())?;
    print_report(&report);

    Ok(exit_code(&report))
}

fn header_text(prepared: &PreparedTest) -> String {
    format!(
        "Rules:   {}\nTarget:  {}\nModel:   {}\nFiles:   {} ({} bytes)\n",
        prepared.rule_source_display,
        prepared.target_display,
        prepared.evaluation.model,
        prepared.document.files.len(),
        prepared.document.total_bytes
    )
}

fn skipped_text(prepared: &PreparedTest) -> Option<String> {
    if prepared.skipped.is_empty() {
        return None;
    }

    let paths: Vec<String> = prepared
        .skipped
        .iter()
        .map(|file| file.display_path())
        .collect();
    Some(format!(
        "Skipped: {} non-UTF-8 ({})\n",
        paths.len(),
        paths.join(", ")
    ))
}

fn file_summary_text(prepared: &PreparedTest) -> String {
    let width = prepared
        .document
        .files
        .iter()
        .map(|file| file.display_path().len())
        .max()
        .unwrap_or(0);

    let mut output = String::new();
    for file in &prepared.document.files {
        output.push_str(&format!(
            "  {:<width$}  {}\n",
            file.display_path(),
            file.contents.len(),
            width = width
        ));
    }
    output
}

fn dry_run_text(prepared: &PreparedTest) -> Result<String, String> {
    let mut output = header_text(prepared);
    output.push_str(&file_summary_text(prepared));
    if let Some(skipped) = skipped_text(prepared) {
        output.push_str(&skipped);
    }
    output.push('\n');
    output.push_str("Request:\n");

    let placeholder = format!(
        "<<<omitted: {} bytes across {} files>>>",
        prepared.evaluation.state.len(),
        prepared.document.files.len()
    );
    let body = prepared.evaluation.to_json(Some(&placeholder));
    let json = serde_json::to_string_pretty(&body).map_err(|error| error.to_string())?;
    output.push_str(&json);
    output.push('\n');

    Ok(output)
}

fn exit_code(report: &Report) -> u8 {
    if report.violations() > 0 {
        1
    } else if report.unsure() > 0 {
        3
    } else {
        0
    }
}

fn print_header(prepared: &PreparedTest) {
    print!("{}", header_text(prepared));
}

fn print_skipped(prepared: &PreparedTest) {
    if let Some(skipped) = skipped_text(prepared) {
        print!("{skipped}");
    }
}

fn print_pending(prepared: &PreparedTest) {
    print!("{}", pending_text(prepared));
}

fn pending_text(prepared: &PreparedTest) -> String {
    let mut output = String::new();
    for rule in &prepared.rules {
        output.push_str(&format!("  [pending] {}\n", rule.id));
    }
    output
}

fn print_report(report: &Report) {
    let color = std::io::stdout().is_terminal();
    print!("{}", report_text(report, color));
}

fn report_text(report: &Report, color: bool) -> String {
    let id_width = report
        .results
        .iter()
        .map(|result| result.id.len())
        .max()
        .unwrap_or(0);
    let mut output = String::from("\n");

    for result in &report.results {
        let label = match result.verdict {
            Verdict::Pass => "PASS",
            Verdict::Violation => "VIOLATION",
            Verdict::Unsure => "UNSURE",
        };
        let status = format!("[{label}]");
        let padding = " ".repeat(12usize.saturating_sub(status.len()));
        let status = format!("{status}{padding}");
        let status = if color {
            format!("{}{status}{RESET}", colour(result.verdict))
        } else {
            status
        };

        output.push_str(&format!(
            "  {status}{:<id_width$}  noul={:.2}\n",
            result.id,
            result.noul,
            id_width = id_width
        ));
    }

    output.push('\n');
    output.push_str(&summary_text(report));
    output.push('\n');
    output
}

fn summary_text(report: &Report) -> String {
    let violations = report.violations();
    let violation_word = if violations == 1 {
        "violation"
    } else {
        "violations"
    };
    let mut parts = vec![format!(
        "{violations} {violation_word}, {} unsure, {} pass",
        report.unsure(),
        report.passes()
    )];

    if let Some(cost) = report.cost_usd {
        parts.push(format!("${cost:.5}"));
    }
    if let Some(tokens) = report.input_tokens {
        parts.push(format!("{tokens} input tokens"));
    }

    parts.join(" · ")
}

fn colour(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "\x1b[32m",
        Verdict::Violation => "\x1b[31m",
        Verdict::Unsure => "\x1b[33m",
    }
}

const RESET: &str = "\x1b[0m";

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use komply::services::testing::model::{
        Document, EvaluationQuestion, EvaluationRequest, PreparedTest, Report, RuleResult, Verdict,
    };
    use komply::services::testing::repositories::files::{SkipReason, SkippedFile, SourceFile};
    use komply::services::testing::repositories::rules::Rule;

    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn prepared() -> PreparedTest {
        let document = Document::new(vec![
            SourceFile {
                relative_path: PathBuf::from("a.rs"),
                contents: "hello".to_owned(),
            },
            SourceFile {
                relative_path: PathBuf::from("nested/b.rs"),
                contents: "world!".to_owned(),
            },
        ]);
        let skipped = vec![SkippedFile {
            relative_path: PathBuf::from("assets/logo.bin"),
            reason: SkipReason::NonUtf8,
        }];
        let rules = vec![Rule {
            id: "rule.one".to_owned(),
            statement: "The statement.".to_owned(),
            enabled: true,
        }];
        let evaluation = EvaluationRequest {
            model: "typesafe/jev-1.13".to_owned(),
            state: "STATE".to_owned(),
            questions: vec![EvaluationQuestion {
                key: "r0".to_owned(),
                instruction: "Is the following statement true of the attached code? The statement."
                    .to_owned(),
            }],
        };

        PreparedTest {
            target: PathBuf::from("src"),
            target_display: "src".to_owned(),
            rule_source: PathBuf::from(".architecture"),
            rule_source_display: ".architecture".to_owned(),
            rules,
            document,
            skipped,
            evaluation,
        }
    }

    #[test]
    fn it_parses_test_with_every_flag() {
        let command = parse_args(&strings(&[
            "test",
            "src",
            "--rules",
            "rules.toml",
            "--dry-run",
            "--model",
            "test/model",
        ]))
        .unwrap();

        match command {
            Command::Test(args) => {
                assert_eq!(args.target, PathBuf::from("src"));
                assert_eq!(args.rules, Some(PathBuf::from("rules.toml")));
                assert!(args.dry_run);
                assert_eq!(args.model, Some("test/model".to_owned()));
            }
            Command::Help => panic!("unexpected help"),
        }
    }

    #[test]
    fn it_rejects_an_unknown_flag() {
        let error = parse_args(&strings(&["test", "src", "--nope"])).unwrap_err();

        assert!(error.contains("unknown flag"));
    }

    #[test]
    fn it_rejects_a_missing_target() {
        let error = parse_args(&strings(&["test", "--dry-run"])).unwrap_err();

        assert!(error.contains("missing target"));
    }

    #[test]
    fn it_rejects_a_missing_flag_value() {
        let error = parse_args(&strings(&["test", "src", "--rules"])).unwrap_err();

        assert!(error.contains("needs a value"));
    }

    #[test]
    fn it_rejects_a_flag_value_that_is_another_flag() {
        let error = parse_args(&strings(&["test", "src", "--rules", "--dry-run"])).unwrap_err();

        assert!(error.contains("needs a value"));
    }

    #[test]
    fn it_rejects_a_second_target() {
        let error = parse_args(&strings(&["test", "a", "b"])).unwrap_err();

        assert!(error.contains("unexpected argument"));
    }

    #[test]
    fn it_rejects_an_unknown_command() {
        let error = parse_args(&strings(&["build"])).unwrap_err();

        assert!(error.contains("unknown command"));
    }

    #[test]
    fn it_accepts_top_level_help() {
        assert_eq!(parse_args(&strings(&["--help"])).unwrap(), Command::Help);
    }

    #[test]
    fn it_accepts_test_help() {
        assert_eq!(
            parse_args(&strings(&["test", "--help"])).unwrap(),
            Command::Help
        );
    }

    #[test]
    fn it_renders_the_dry_run_summary_and_placeholder() {
        let prepared = prepared();

        let output = dry_run_text(&prepared).unwrap();

        assert!(output.contains("Rules:   .architecture\n"));
        assert!(output.contains("Target:  src\n"));
        assert!(output.contains("Files:   2 (11 bytes)\n"));
        assert!(output.contains("  a.rs"));
        assert!(output.contains("Skipped: 1 non-UTF-8 (assets/logo.bin)\n"));
        let json = output.split_once("Request:\n").unwrap().1;
        let body: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(body["state"], "<<<omitted: 5 bytes across 2 files>>>");
        assert_eq!(body["questions"]["r0"]["type"], "noul");
        assert_eq!(
            body["questions"]["r0"]["instructions"],
            "Is the following statement true of the attached code? The statement."
        );
    }

    fn report(verdicts: &[Verdict], cost_usd: Option<f64>) -> Report {
        Report {
            target: PathBuf::from("src"),
            target_display: "src".to_owned(),
            rule_source: PathBuf::from(".architecture"),
            rule_source_display: ".architecture".to_owned(),
            model: "model".to_owned(),
            file_count: 1,
            document_bytes: 1,
            skipped: Vec::new(),
            results: verdicts
                .iter()
                .enumerate()
                .map(|(index, verdict)| RuleResult {
                    id: format!("rule.{index}"),
                    statement: "statement".to_owned(),
                    noul: 0.5,
                    verdict: *verdict,
                })
                .collect(),
            cost_usd,
            input_tokens: None,
            output_tokens: None,
        }
    }

    #[test]
    fn it_maps_report_shapes_to_exit_codes() {
        assert_eq!(exit_code(&report(&[Verdict::Pass], None)), 0);
        assert_eq!(exit_code(&report(&[Verdict::Violation], None)), 1);
        assert_eq!(exit_code(&report(&[Verdict::Unsure], None)), 3);
    }

    #[test]
    fn it_puts_a_violation_before_unsure() {
        let report = report(&[Verdict::Unsure, Verdict::Violation], None);

        assert_eq!(exit_code(&report), 1);
    }

    #[test]
    fn it_renders_report_labels() {
        let report = report(&[Verdict::Pass, Verdict::Violation, Verdict::Unsure], None);

        let output = report_text(&report, false);

        assert!(output.contains("[PASS]"));
        assert!(output.contains("[VIOLATION]"));
        assert!(output.contains("[UNSURE]"));
        assert!(output.contains("noul=0.50"));
    }

    #[test]
    fn it_omits_the_cost_segment_when_cost_is_absent() {
        let without_cost = report_text(&report(&[Verdict::Pass], None), false);
        let with_cost = report_text(&report(&[Verdict::Pass], Some(0.00012)), false);

        assert!(!without_cost.contains('$'));
        assert!(with_cost.contains("$0.00012"));
    }

    #[test]
    fn it_prints_one_pending_line_per_enabled_rule() {
        let prepared = prepared();

        let output = pending_text(&prepared);

        assert_eq!(output, "  [pending] rule.one\n");
    }
}
