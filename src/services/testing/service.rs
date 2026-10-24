use std::collections::HashMap;
use std::path::{Path, PathBuf};

use globset::GlobBuilder;

use crate::config::Config;
use crate::services::testing::errors::Error;
use crate::services::testing::model::{
    Document, EvaluationQuestion, EvaluationRequest, PreparedTest, Report, RuleResult, TestRequest,
    Verdict,
};
use crate::services::testing::repositories::decisions::{self, DecisionsRepository};
use crate::services::testing::repositories::files::{
    CollectOptions, FilesRepository, display_path,
};
use crate::services::testing::repositories::rules::{FileSelection, RuleSet, RulesRepository};

/// The application service that owns rule selection, document assembly, and,
/// later, verdicts.
pub struct Service {
    pub(crate) rules: RulesRepository,
    pub(crate) files: FilesRepository,
    pub(crate) config: Config,
}

impl Service {
    /// Creates a service from its repositories and config.
    pub fn new(rules: RulesRepository, files: FilesRepository, config: Config) -> Self {
        Self {
            rules,
            files,
            config,
        }
    }

    /// Loads the rule sets for `request` and selects the ones that apply.
    ///
    /// With `request.rules`, every loaded set is used and `path` matching is
    /// skipped. Without it, the tested directory must match a rule file's
    /// `path` exactly. Duplicate ids across the selected sets and runs with no
    /// enabled rule are errors.
    pub fn resolve_rules(&self, request: &TestRequest) -> Result<Vec<RuleSet>, Error> {
        let target = target_display(&request.root, &request.target)?;
        let source = rules_path(&request.root, request.rules.as_deref());
        let sets = self.rules.load(&source)?;

        let selected = if request.rules.is_some() {
            sets
        } else {
            select_by_path(sets, &target)?
        };

        ensure_unique_ids(&selected)?;

        if !selected
            .iter()
            .flat_map(|set| set.rules.iter())
            .any(|rule| rule.enabled)
        {
            return Err(Error::NoEnabledRules { target });
        }

        Ok(selected)
    }

    /// Collects the target's files and builds the state and questions for one
    /// decisions request.
    ///
    /// The returned rules and questions share one order, contain only enabled
    /// rules, and use keys `r0`, `r1`, ... in that order. An empty file set, a
    /// file over `max_document_bytes`, or a final state over the same limit is
    /// an error.
    pub fn prepare(&self, request: TestRequest) -> Result<PreparedTest, Error> {
        let target = target_display(&request.root, &request.target)?;
        let rule_sets = self.resolve_rules(&request)?;
        let source = rules_path(&request.root, request.rules.as_deref());
        let source_display = rule_source_display(&request.root, &source, request.rules.as_deref());

        let rules: Vec<_> = rule_sets
            .iter()
            .flat_map(|set| set.rules.iter().filter(|rule| rule.enabled).cloned())
            .collect();
        let selection = merge_selections(&rule_sets);

        let mut ignore = self
            .files
            .load_ignore(&request.root.join(".complyignore"))?;
        push_unique(&mut ignore.project, ".git");
        push_unique(&mut ignore.project, ".architecture");
        ignore.target = selection.exclude.clone();

        let collected = self.files.collect(
            &request.root,
            &request.target,
            &CollectOptions {
                selection: &selection,
                ignore: &ignore,
                max_file_bytes: self.config.max_document_bytes,
            },
        )?;

        if collected.files.is_empty() {
            return Err(Error::NoFiles(request.target.clone()));
        }

        let document = Document::new(collected.files);
        let state = format!("{}{}", preamble(&target), document.render());
        if state.len() > self.config.max_document_bytes {
            return Err(Error::DocumentTooLarge {
                bytes: state.len(),
                limit: self.config.max_document_bytes,
            });
        }

        let questions = rules
            .iter()
            .enumerate()
            .map(|(index, rule)| EvaluationQuestion {
                key: format!("r{index}"),
                instruction: format!(
                    "Is the following statement true of the attached code? {}",
                    rule.statement
                ),
            })
            .collect();

        Ok(PreparedTest {
            target: request.target,
            target_display: target,
            rule_source: source,
            rule_source_display: source_display,
            rules,
            document,
            skipped: collected.skipped,
            evaluation: EvaluationRequest {
                model: self.config.model.clone(),
                state,
                questions,
            },
        })
    }

    /// Sends the prepared test to the decisions repository and maps every
    /// answer to a rule result.
    ///
    /// The repository validates every requested key before returning. A missing
    /// answer that reaches this method is wrapped as [`Error::Decisions`].
    pub fn run(
        &self,
        prepared: PreparedTest,
        decisions: &DecisionsRepository,
    ) -> Result<Report, Error> {
        let decision = decisions.evaluate(&prepared.evaluation)?;
        let mut results = Vec::with_capacity(prepared.rules.len());

        for (rule, question) in prepared
            .rules
            .iter()
            .zip(prepared.evaluation.questions.iter())
        {
            let noul = decision
                .answers
                .get(&question.key)
                .copied()
                .ok_or_else(|| {
                    Error::Decisions(decisions::Error::MissingAnswer(question.key.clone()))
                })?;
            results.push(RuleResult {
                id: rule.id.clone(),
                statement: rule.statement.clone(),
                noul,
                verdict: verdict(noul, &self.config),
            });
        }

        Ok(Report {
            target: prepared.target,
            target_display: prepared.target_display,
            rule_source: prepared.rule_source,
            rule_source_display: prepared.rule_source_display,
            model: prepared.evaluation.model.clone(),
            file_count: prepared.document.files.len(),
            document_bytes: prepared.document.total_bytes,
            skipped: prepared.skipped,
            results,
            cost_usd: decision.cost_usd,
            input_tokens: decision.input_tokens,
            output_tokens: decision.output_tokens,
        })
    }

    /// Prepares and runs one test in a single call.
    pub fn check(
        &self,
        request: TestRequest,
        decisions: &DecisionsRepository,
    ) -> Result<Report, Error> {
        let prepared = self.prepare(request)?;
        self.run(prepared, decisions)
    }
}

/// Maps a probability to a verdict using the configured thresholds.
///
/// Both boundaries are inclusive: `>= pass_threshold` is a pass and
/// `<= fail_threshold` is a violation.
pub fn verdict(noul: f64, config: &Config) -> Verdict {
    if noul >= config.pass_threshold {
        Verdict::Pass
    } else if noul <= config.fail_threshold {
        Verdict::Violation
    } else {
        Verdict::Unsure
    }
}

fn rules_path(root: &Path, supplied: Option<&Path>) -> PathBuf {
    match supplied {
        Some(path) => root.join(path),
        None => root.join(".architecture"),
    }
}

fn target_display(root: &Path, target: &Path) -> Result<String, Error> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| Error::TargetOutsideRoot {
            root: root.to_path_buf(),
            target: target.to_path_buf(),
        })?;
    Ok(display_path(relative))
}

fn rule_source_display(root: &Path, source: &Path, supplied: Option<&Path>) -> String {
    match source.strip_prefix(root) {
        Ok(relative) => display_path(relative),
        Err(_) => supplied
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| source.display().to_string()),
    }
}

fn preamble(target: &str) -> String {
    format!(
        "The following document contains files from `{target}`.\nEach file is delimited by `<<<FILE relative/path>>>` and `<<<END FILE>>>`.\n\n"
    )
}

fn select_by_path(sets: Vec<RuleSet>, target: &str) -> Result<Vec<RuleSet>, Error> {
    let mut available = Vec::new();
    let mut selected = Vec::new();

    for set in sets {
        if set.paths.is_empty() {
            return Err(Error::MissingRulePath(set.source.clone()));
        }

        let mut matched = false;
        for pattern in &set.paths {
            available.push(pattern.clone());

            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|source| Error::InvalidRulePath {
                    path: set.source.clone(),
                    pattern: pattern.clone(),
                    source,
                })?;

            if glob.compile_matcher().is_match(target) {
                matched = true;
            }
        }

        if matched {
            selected.push(set);
        }
    }

    if selected.is_empty() {
        return Err(Error::NoRulesMatch {
            target: target.to_owned(),
            available,
        });
    }

    Ok(selected)
}

fn ensure_unique_ids(sets: &[RuleSet]) -> Result<(), Error> {
    let mut seen: HashMap<&str, &Path> = HashMap::new();
    for set in sets {
        for rule in &set.rules {
            if let Some(first) = seen.insert(rule.id.as_str(), set.source.as_path()) {
                return Err(Error::DuplicateRuleId {
                    id: rule.id.clone(),
                    first: first.to_path_buf(),
                    second: set.source.clone(),
                });
            }
        }
    }
    Ok(())
}

fn merge_selections(sets: &[RuleSet]) -> FileSelection {
    let mut include = Vec::new();
    let mut exclude = Vec::new();

    for set in sets {
        let set_include = if set.files.include.is_empty() {
            vec!["**/*".to_owned()]
        } else {
            set.files.include.clone()
        };
        extend_unique(&mut include, set_include);
        extend_unique(&mut exclude, set.files.exclude.clone());
    }

    FileSelection { include, exclude }
}

fn extend_unique(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn push_unique(target: &mut Vec<String>, value: &str) {
    if !target.iter().any(|item| item == value) {
        target.push(value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;

    fn architecture(root: &Path) -> PathBuf {
        let path = root.join(".architecture");
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_rule(root: &Path, name: &str, contents: &str) {
        let directory = architecture(root);
        std::fs::write(directory.join(name), contents).unwrap();
    }

    fn rule(path: Option<&str>, id: &str, enabled: bool) -> String {
        let path_line = path
            .map(|path| format!("path = \"{path}\"\n"))
            .unwrap_or_default();
        format!(
            "{path_line}\n[[rules]]\nid = \"{id}\"\nstatement = \"The statement.\"\nenabled = {enabled}\n"
        )
    }

    fn request(root: &Path, target: &Path, rules: Option<PathBuf>) -> TestRequest {
        TestRequest {
            root: root.to_path_buf(),
            target: target.to_path_buf(),
            rules,
        }
    }

    fn service() -> Service {
        Service::new(
            RulesRepository::new(),
            FilesRepository::new(),
            Config::default(),
        )
    }

    fn fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project")
    }

    fn document_paths(prepared: &PreparedTest) -> Vec<String> {
        prepared
            .document
            .files
            .iter()
            .map(|file| file.display_path())
            .collect()
    }

    #[test]
    fn it_selects_a_rule_set_whose_path_matches_exactly() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            &rule(Some("src/services/*"), "service.rule", true),
        );
        write_rule(
            root.path(),
            "b.toml",
            &rule(
                Some("src/services/*/repositories/*"),
                "repository.rule",
                true,
            ),
        );
        let target = root.path().join("src/services/alpha/repositories/tasks");

        let sets = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap();

        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].rules[0].id, "repository.rule");
    }

    #[test]
    fn it_selects_a_rule_set_when_any_path_matches() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            "path = [\"src/*\", \"tests/*\"]\n[[rules]]\nid = \"rule.a\"\nstatement = \"A.\"\n",
        );
        let target = root.path().join("tests/alpha");

        let sets = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap();

        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].rules[0].id, "rule.a");
    }

    #[test]
    fn it_reports_every_pattern_from_an_unmatched_array() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            "path = [\"a/*\", \"b/*\"]\n[[rules]]\nid = \"rule.a\"\nstatement = \"A.\"\n",
        );
        let target = root.path().join("c");

        let error = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap_err();

        match error {
            Error::NoRulesMatch { available, .. } => {
                assert_eq!(available, vec!["a/*".to_owned(), "b/*".to_owned()]);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn it_rejects_an_invalid_glob_in_an_array() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            "path = [\"src/*\", \"[\"]\n[[rules]]\nid = \"rule.a\"\nstatement = \"A.\"\n",
        );
        let target = root.path().join("src/alpha");

        let error = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap_err();

        assert!(matches!(error, Error::InvalidRulePath { .. }));
    }

    #[test]
    fn it_rejects_a_deeper_path_when_only_a_parent_pattern_exists() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            &rule(Some("src/services/*"), "service.rule", true),
        );
        let target = root.path().join("src/services/alpha/repositories");

        let error = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap_err();

        assert!(matches!(error, Error::NoRulesMatch { .. }));
    }

    #[test]
    fn it_reports_the_available_paths_in_no_rules_match() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(Some("a/*"), "rule.a", true));
        write_rule(root.path(), "b.toml", &rule(Some("b/*"), "rule.b", true));
        let target = root.path().join("c");

        let error = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap_err();

        match error {
            Error::NoRulesMatch { target, available } => {
                assert_eq!(target, "c");
                assert_eq!(available, vec!["a/*".to_owned(), "b/*".to_owned()]);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn it_uses_every_loaded_set_with_explicit_rules_and_ignores_path_matching() {
        let root = TempDir::new().unwrap();
        let rules = root.path().join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(rules.join("a.toml"), rule(None, "rule.a", true)).unwrap();
        std::fs::write(
            rules.join("b.toml"),
            rule(Some("does/not/match"), "rule.b", true),
        )
        .unwrap();
        let target = root.path().join("anything");

        let sets = service()
            .resolve_rules(&request(root.path(), &target, Some(rules)))
            .unwrap();

        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].rules[0].id, "rule.a");
        assert_eq!(sets[1].rules[0].id, "rule.b");
    }

    #[test]
    fn it_rejects_a_missing_path_in_default_mode() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(None, "rule.a", true));
        let target = root.path().join("src");

        let error = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap_err();

        assert!(matches!(error, Error::MissingRulePath(_)));
    }

    #[test]
    fn it_allows_a_missing_path_in_explicit_mode() {
        let root = TempDir::new().unwrap();
        let file = root.path().join("rules.toml");
        std::fs::write(&file, rule(None, "rule.a", true)).unwrap();
        let target = root.path().join("src");

        let sets = service()
            .resolve_rules(&request(root.path(), &target, Some(file)))
            .unwrap();

        assert_eq!(sets.len(), 1);
    }

    #[test]
    fn it_rejects_a_duplicate_id_across_two_matched_files() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            &rule(Some("src/*"), "rule.same", true),
        );
        write_rule(
            root.path(),
            "b.toml",
            &rule(Some("src/*"), "rule.same", true),
        );
        let target = root.path().join("src/alpha");

        let error = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap_err();

        match error {
            Error::DuplicateRuleId { id, first, second } => {
                assert_eq!(id, "rule.same");
                assert_eq!(first.file_name().unwrap(), "a.toml");
                assert_eq!(second.file_name().unwrap(), "b.toml");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn it_rejects_duplicate_ids_in_explicit_mode() {
        let root = TempDir::new().unwrap();
        let rules = root.path().join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(rules.join("a.toml"), rule(None, "rule.same", true)).unwrap();
        std::fs::write(rules.join("b.toml"), rule(None, "rule.same", true)).unwrap();
        let target = root.path().join("src");

        let error = service()
            .resolve_rules(&request(root.path(), &target, Some(rules)))
            .unwrap_err();

        assert!(matches!(error, Error::DuplicateRuleId { .. }));
    }

    #[test]
    fn it_rejects_a_run_with_no_enabled_rules() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(Some("src/*"), "rule.a", false));
        let target = root.path().join("src/alpha");

        let error = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap_err();

        match error {
            Error::NoEnabledRules { target } => assert_eq!(target, "src/alpha"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn it_keeps_disabled_rules_in_the_selected_set() {
        let root = TempDir::new().unwrap();
        std::fs::write(
            architecture(root.path()).join("a.toml"),
            "path = \"src/*\"\n[[rules]]\nid = \"rule.disabled\"\nstatement = \"Disabled.\"\nenabled = false\n[[rules]]\nid = \"rule.enabled\"\nstatement = \"Enabled.\"\nenabled = true\n",
        )
        .unwrap();
        let target = root.path().join("src/alpha");

        let sets = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap();

        let rules: Vec<_> = sets.iter().flat_map(|set| set.rules.iter()).collect();
        assert_eq!(rules.len(), 2);
        assert!(rules.iter().any(|rule| !rule.enabled));
    }

    #[test]
    fn it_rejects_a_target_outside_the_root() {
        let root = TempDir::new().unwrap();
        let other = TempDir::new().unwrap();

        let error = service()
            .resolve_rules(&request(root.path(), other.path(), None))
            .unwrap_err();

        assert!(matches!(error, Error::TargetOutsideRoot { .. }));
    }

    #[test]
    fn it_matches_the_root_with_a_double_star_path() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(Some("**"), "rule.a", true));

        let sets = service()
            .resolve_rules(&request(root.path(), root.path(), None))
            .unwrap();

        assert_eq!(sets.len(), 1);
    }

    #[test]
    fn it_rejects_an_invalid_glob() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(Some("["), "rule.a", true));
        let target = root.path().join("src");

        let error = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap_err();

        assert!(matches!(error, Error::InvalidRulePath { .. }));
    }

    #[test]
    fn it_returns_matched_sets_in_load_order() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "b.toml", &rule(Some("src/*"), "rule.b", true));
        write_rule(root.path(), "a.toml", &rule(Some("src/*"), "rule.a", true));
        let target = root.path().join("src/alpha");

        let sets = service()
            .resolve_rules(&request(root.path(), &target, None))
            .unwrap();

        assert_eq!(sets[0].rules[0].id, "rule.a");
        assert_eq!(sets[1].rules[0].id, "rule.b");
    }

    #[test]
    fn it_prepares_the_fixture_repository_target() {
        let root = fixture_root();
        let config = Config::load(&root).unwrap();
        let service = Service::new(RulesRepository::new(), FilesRepository::new(), config);
        let target = root.join("src/services/alpha/repositories/tasks");

        let prepared = service.prepare(request(&root, &target, None)).unwrap();

        assert_eq!(
            prepared.target_display,
            "src/services/alpha/repositories/tasks"
        );
        assert_eq!(prepared.rule_source_display, ".architecture");
        assert_eq!(prepared.rules.len(), 1);
        assert_eq!(prepared.rules[0].id, "repo.facade");
        assert_eq!(
            document_paths(&prepared),
            vec![
                "models.rs".to_owned(),
                "queries/search_tasks.rs".to_owned(),
                "repository.rs".to_owned()
            ]
        );
        assert!(prepared.skipped.is_empty());
        assert_eq!(prepared.evaluation.model, "typesafe/jev-1.13");
        assert_eq!(prepared.evaluation.questions.len(), 1);
        assert_eq!(prepared.evaluation.questions[0].key, "r0");
        assert!(
            prepared.evaluation.questions[0]
                .instruction
                .starts_with("Is the following statement true of the attached code? ")
        );
        assert!(prepared.evaluation.state.starts_with(
            "The following document contains files from `src/services/alpha/repositories/tasks`.\n"
        ));
        assert!(prepared.evaluation.state.contains("<<<FILE models.rs>>>"));
        let expected_bytes = ["models.rs", "queries/search_tasks.rs", "repository.rs"]
            .iter()
            .map(|path| std::fs::metadata(target.join(path)).unwrap().len() as usize)
            .sum::<usize>();
        assert_eq!(prepared.document.total_bytes, expected_bytes);
    }

    #[test]
    fn it_prepares_the_fixture_service_target_and_reports_non_utf8() {
        let root = fixture_root();
        let config = Config::load(&root).unwrap();
        let service = Service::new(RulesRepository::new(), FilesRepository::new(), config);
        let target = root.join("src/services/alpha");

        let prepared = service.prepare(request(&root, &target, None)).unwrap();

        assert_eq!(prepared.rules.len(), 1);
        assert_eq!(prepared.rules[0].id, "service.owns-its-tables");
        assert_eq!(prepared.document.files.len(), 5);
        assert_eq!(prepared.skipped.len(), 1);
        assert_eq!(prepared.skipped[0].display_path(), "assets/logo.bin");
    }

    #[test]
    fn it_merges_include_and_exclude_lists_across_matched_rule_sets() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            "path = \"src\"\n[files]\ninclude = [\"**/*.rs\"]\nexclude = [\"skip\"]\n[[rules]]\nid = \"rule.a\"\nstatement = \"A.\"\n",
        );
        write_rule(
            root.path(),
            "b.toml",
            "path = \"src\"\n[files]\ninclude = [\"**/*.toml\"]\nexclude = [\"target\"]\n[[rules]]\nid = \"rule.b\"\nstatement = \"B.\"\n",
        );
        let src = root.path().join("src");
        std::fs::create_dir_all(src.join("skip")).unwrap();
        std::fs::create_dir_all(src.join("target")).unwrap();
        std::fs::write(src.join("a.rs"), "a").unwrap();
        std::fs::write(src.join("b.toml"), "b").unwrap();
        std::fs::write(src.join("notes.txt"), "notes").unwrap();
        std::fs::write(src.join("skip/c.rs"), "c").unwrap();
        std::fs::write(src.join("target/d.rs"), "d").unwrap();

        let prepared = service().prepare(request(root.path(), &src, None)).unwrap();

        assert_eq!(prepared.rules.len(), 2);
        assert_eq!(
            document_paths(&prepared),
            vec!["a.rs".to_owned(), "b.toml".to_owned()]
        );
    }

    #[test]
    fn it_adds_git_and_architecture_to_project_ignores() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(Some("**"), "rule.a", true));
        std::fs::write(root.path().join("a.rs"), "a").unwrap();
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        std::fs::write(root.path().join(".git/config"), "ignored").unwrap();

        let prepared = service()
            .prepare(request(root.path(), root.path(), None))
            .unwrap();

        assert_eq!(document_paths(&prepared), vec!["a.rs".to_owned()]);
    }

    #[test]
    fn it_builds_one_question_per_enabled_rule_in_order() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            "path = \"src\"\n[[rules]]\nid = \"one\"\nstatement = \"One.\"\nenabled = true\n[[rules]]\nid = \"two\"\nstatement = \"Two.\"\nenabled = false\n[[rules]]\nid = \"three\"\nstatement = \"Three.\"\nenabled = true\n",
        );
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), "a").unwrap();

        let prepared = service().prepare(request(root.path(), &src, None)).unwrap();

        assert_eq!(prepared.rules.len(), 2);
        assert_eq!(prepared.rules[0].id, "one");
        assert_eq!(prepared.rules[1].id, "three");
        assert_eq!(prepared.evaluation.questions.len(), 2);
        assert_eq!(prepared.evaluation.questions[0].key, "r0");
        assert_eq!(prepared.evaluation.questions[1].key, "r1");
        assert!(
            prepared.evaluation.questions[0]
                .instruction
                .contains("One.")
        );
        assert!(
            prepared.evaluation.questions[1]
                .instruction
                .contains("Three.")
        );
    }

    #[test]
    fn it_returns_no_files_for_an_empty_selection() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(Some("src"), "rule.a", true));
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();

        let error = service()
            .prepare(request(root.path(), &src, None))
            .unwrap_err();

        assert!(matches!(error, Error::NoFiles(_)));
    }

    #[test]
    fn it_returns_document_too_large_when_the_state_exceeds_the_cap() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(Some("src"), "rule.a", true));
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), "x".repeat(100)).unwrap();
        let config = Config {
            max_document_bytes: 150,
            ..Config::default()
        };
        let service = Service::new(RulesRepository::new(), FilesRepository::new(), config);

        let error = service
            .prepare(request(root.path(), &src, None))
            .unwrap_err();

        assert!(matches!(error, Error::DocumentTooLarge { .. }));
    }

    #[test]
    fn it_displays_a_supplied_rule_source_relative_to_the_root() {
        let root = TempDir::new().unwrap();
        let rules = root.path().join("rules.toml");
        std::fs::write(&rules, rule(None, "rule.a", true)).unwrap();
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), "a").unwrap();

        let prepared = service()
            .prepare(request(
                root.path(),
                &src,
                Some(PathBuf::from("rules.toml")),
            ))
            .unwrap();

        assert_eq!(prepared.rule_source_display, "rules.toml");
    }

    #[test]
    fn it_displays_a_supplied_rule_source_outside_the_root_as_supplied() {
        let root = TempDir::new().unwrap();
        let other = TempDir::new().unwrap();
        let rules = other.path().join("rules.toml");
        std::fs::write(&rules, rule(None, "rule.a", true)).unwrap();
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), "a").unwrap();

        let prepared = service()
            .prepare(request(root.path(), &src, Some(rules.clone())))
            .unwrap();

        assert_eq!(prepared.rule_source_display, rules.display().to_string());
    }

    #[test]
    fn it_maps_probabilities_to_verdicts_at_the_boundaries() {
        let config = Config::default();

        assert_eq!(verdict(0.80, &config), Verdict::Pass);
        assert_eq!(verdict(0.20, &config), Verdict::Violation);
        assert_eq!(verdict(0.50, &config), Verdict::Unsure);
        assert_eq!(verdict(0.79, &config), Verdict::Unsure);
        assert_eq!(verdict(0.21, &config), Verdict::Unsure);
    }

    #[test]
    fn it_runs_a_prepared_test_against_a_stub() {
        let root = TempDir::new().unwrap();
        write_rule(
            root.path(),
            "a.toml",
            "path = \"src\"\n[[rules]]\nid = \"rule.violation\"\nstatement = \"V.\"\n[[rules]]\nid = \"rule.pass\"\nstatement = \"P.\"\n[[rules]]\nid = \"rule.unsure\"\nstatement = \"U.\"\n",
        );
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), "a").unwrap();
        let prepared = service().prepare(request(root.path(), &src, None)).unwrap();
        let url = stub(
            r#"{"answers":{"r0":{"type":"noul","noul":0.04},"r1":{"type":"noul","noul":0.91},"r2":{"type":"noul","noul":0.54}},"usage":{"cost":0.00012,"input_tokens":476,"output_tokens":5}}"#,
        );
        let decisions = DecisionsRepository::new("key", url, Duration::from_secs(5));

        let report = service().run(prepared, &decisions).unwrap();

        assert_eq!(report.results.len(), 3);
        assert_eq!(report.results[0].id, "rule.violation");
        assert_eq!(report.results[0].verdict, Verdict::Violation);
        assert_eq!(report.results[1].id, "rule.pass");
        assert_eq!(report.results[1].verdict, Verdict::Pass);
        assert_eq!(report.results[2].id, "rule.unsure");
        assert_eq!(report.results[2].verdict, Verdict::Unsure);
        assert_eq!(report.violations(), 1);
        assert_eq!(report.unsure(), 1);
        assert_eq!(report.passes(), 1);
        assert_eq!(report.file_count, 1);
        assert_eq!(report.document_bytes, 1);
        assert_eq!(report.cost_usd, Some(0.00012));
        assert_eq!(report.input_tokens, Some(476));
        assert_eq!(report.output_tokens, Some(5));
    }

    #[test]
    fn it_returns_the_repository_missing_answer_error() {
        let root = TempDir::new().unwrap();
        write_rule(root.path(), "a.toml", &rule(Some("src"), "rule.a", true));
        let src = root.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), "a").unwrap();
        let prepared = service().prepare(request(root.path(), &src, None)).unwrap();
        let url = stub(r#"{"answers":{}}"#);
        let decisions = DecisionsRepository::new("key", url, Duration::from_secs(5));

        let error = service().run(prepared, &decisions).unwrap_err();

        match error {
            Error::Decisions(decisions::Error::MissingAnswer(key)) => {
                assert_eq!(key, "r0");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn it_checks_in_one_call() {
        let root = fixture_root();
        let config = Config::load(&root).unwrap();
        let service = Service::new(RulesRepository::new(), FilesRepository::new(), config);
        let target = root.join("src/services/alpha/repositories/tasks");
        let url = stub(r#"{"answers":{"r0":{"type":"noul","noul":0.9}}}"#);
        let decisions = DecisionsRepository::new("key", url, Duration::from_secs(5));

        let report = service
            .check(request(&root, &target, None), &decisions)
            .unwrap();

        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].id, "repo.facade");
        assert_eq!(report.results[0].verdict, Verdict::Pass);
    }

    fn stub(body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("http://{address}/decisions");
        let body = body.to_owned();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            write_response(&mut stream, &body);
        });

        url
    }

    fn read_request(stream: &mut TcpStream) {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];

        loop {
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 {
                return;
            }
            buffer.extend_from_slice(&chunk[..count]);

            if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&buffer[..position]).to_lowercase();
                let content_length = header
                    .lines()
                    .find_map(|line| {
                        line.trim()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let total = position + 4 + content_length;
                while buffer.len() < total {
                    let count = stream.read(&mut chunk).unwrap();
                    if count == 0 {
                        return;
                    }
                    buffer.extend_from_slice(&chunk[..count]);
                }
                return;
            }
        }
    }

    fn write_response(stream: &mut TcpStream, body: &str) {
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(body.as_bytes()).unwrap();
        stream.flush().unwrap();
    }
}
