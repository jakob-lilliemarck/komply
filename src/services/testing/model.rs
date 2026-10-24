use std::path::PathBuf;

use crate::services::testing::repositories::files::{SkippedFile, SourceFile};
use crate::services::testing::repositories::rules::Rule;

/// One request to test a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRequest {
    pub root: PathBuf,
    pub target: PathBuf,
    pub rules: Option<PathBuf>,
}

/// The files included in the state document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub files: Vec<SourceFile>,
    pub total_bytes: usize,
}

impl Document {
    /// Builds a document and sums the byte length of its file contents.
    pub fn new(files: Vec<SourceFile>) -> Self {
        let total_bytes = files.iter().map(|file| file.contents.len()).sum::<usize>();
        Self { files, total_bytes }
    }

    /// Renders every file with a marker before and after it.
    ///
    /// A file without a final newline gets one before its end marker, so a
    /// marker never joins the contents.
    pub fn render(&self) -> String {
        let mut output = String::new();
        for file in &self.files {
            output.push_str("<<<FILE ");
            output.push_str(&file.display_path());
            output.push_str(">>>\n");
            output.push_str(&file.contents);
            if !file.contents.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("<<<END FILE>>>\n");
        }
        output
    }
}

/// One request to the decisions model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationRequest {
    pub model: String,
    pub state: String,
    pub questions: Vec<EvaluationQuestion>,
}

/// One `noul` question in a decisions request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationQuestion {
    pub key: String,
    pub instruction: String,
}

impl EvaluationRequest {
    /// Builds the request body. When `state_override` is set, it replaces the
    /// real state in the body.
    pub fn to_json(&self, state_override: Option<&str>) -> serde_json::Value {
        let state = state_override.unwrap_or(&self.state);
        let questions: serde_json::Map<String, serde_json::Value> = self
            .questions
            .iter()
            .map(|question| {
                (
                    question.key.clone(),
                    serde_json::json!({
                        "type": "noul",
                        "instructions": &question.instruction,
                    }),
                )
            })
            .collect();

        serde_json::json!({
            "model": &self.model,
            "state": state,
            "questions": questions,
        })
    }
}

/// A prepared test that is ready to send to the decisions model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTest {
    pub target: PathBuf,
    pub target_display: String,
    pub rule_source: PathBuf,
    pub rule_source_display: String,
    pub rules: Vec<Rule>,
    pub document: Document,
    pub skipped: Vec<SkippedFile>,
    pub evaluation: EvaluationRequest,
}

/// How one probability compares with the configured thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Violation,
    Unsure,
}

/// One rule and the probability that the code satisfies its statement.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleResult {
    pub id: String,
    pub statement: String,
    pub noul: f64,
    pub verdict: Verdict,
}

/// The complete result of one test run.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub target: PathBuf,
    pub target_display: String,
    pub rule_source: PathBuf,
    pub rule_source_display: String,
    pub model: String,
    pub file_count: usize,
    pub document_bytes: usize,
    pub skipped: Vec<SkippedFile>,
    pub results: Vec<RuleResult>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl Report {
    /// Returns how many rules were violations.
    pub fn violations(&self) -> usize {
        self.count(Verdict::Violation)
    }

    /// Returns how many rules were unsure.
    pub fn unsure(&self) -> usize {
        self.count(Verdict::Unsure)
    }

    /// Returns how many rules passed.
    pub fn passes(&self) -> usize {
        self.count(Verdict::Pass)
    }

    fn count(&self, verdict: Verdict) -> usize {
        self.results
            .iter()
            .filter(|result| result.verdict == verdict)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str, contents: &str) -> SourceFile {
        SourceFile {
            relative_path: PathBuf::from(path),
            contents: contents.to_owned(),
        }
    }

    #[test]
    fn it_sums_the_document_bytes() {
        let document = Document::new(vec![source("a.rs", "abc"), source("b.rs", "de")]);

        assert_eq!(document.total_bytes, 5);
    }

    #[test]
    fn it_renders_markers_and_adds_a_missing_final_newline() {
        let document = Document::new(vec![source("a.rs", "abc"), source("nested/b.rs", "de\n")]);

        let rendered = document.render();

        assert_eq!(
            rendered,
            "<<<FILE a.rs>>>\nabc\n<<<END FILE>>>\n<<<FILE nested/b.rs>>>\nde\n<<<END FILE>>>\n"
        );
    }

    #[test]
    fn it_replaces_the_state_when_an_override_is_given() {
        let request = EvaluationRequest {
            model: "ty.p/example".to_owned(),
            state: "REAL STATE".to_owned(),
            questions: vec![EvaluationQuestion {
                key: "r0".to_owned(),
                instruction: "Is it true?".to_owned(),
            }],
        };

        let body = request.to_json(Some("OVERRIDE"));

        assert_eq!(body["state"], "OVERRIDE");
        assert_eq!(body["model"], "ty.p/example");
        assert_eq!(body["questions"]["r0"]["type"], "noul");
        assert_eq!(body["questions"]["r0"]["instructions"], "Is it true?");
    }

    #[test]
    fn it_uses_the_real_state_without_an_override() {
        let request = EvaluationRequest {
            model: "ty.p/example".to_owned(),
            state: "REAL STATE".to_owned(),
            questions: Vec::new(),
        };

        let body = request.to_json(None);

        assert_eq!(body["state"], "REAL STATE");
        assert_eq!(body["questions"], serde_json::json!({}));
    }

    fn report(verdicts: &[Verdict]) -> Report {
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
                    id: format!("r{index}"),
                    statement: "statement".to_owned(),
                    noul: 0.5,
                    verdict: *verdict,
                })
                .collect(),
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
        }
    }

    #[test]
    fn it_counts_mixed_results() {
        let report = report(&[
            Verdict::Pass,
            Verdict::Violation,
            Verdict::Unsure,
            Verdict::Violation,
        ]);

        assert_eq!(report.passes(), 1);
        assert_eq!(report.violations(), 2);
        assert_eq!(report.unsure(), 1);
    }
}
