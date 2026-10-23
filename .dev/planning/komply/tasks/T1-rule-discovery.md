## Skills

`@config`, `@services`, `@repositories`, `@documentation` — the project-local
skills under `.opencode/skills/`.

This task adds no CLI, no filesystem walk, and no network code.

## Goal

Create the komply crate shell and the offline rule half of the tool: load
`.architecture/config.toml`, load and validate rule files, and select the rule
sets whose `path` exactly matches a tested directory.

## Definition of done

- `Cargo.toml`, `src/lib.rs`, `src/config.rs`, and the
  `src/services/testing/` module shell exist as described in `komply.md`.
- `src/services/testing/repositories/rules/` loads one file or a directory of
  `*.toml` files, ignores `config.toml` in a directory, and maps TOML into
  `RuleSet` values.
- `Service::resolve_rules` selects rule sets by exact glob match, supports
  `TestRequest.rules` as a matching bypass, and rejects duplicate rule ids,
  missing `path` fields in default mode, and runs with no enabled rules.
- `cargo test` passes.
- No network, no CLI, no source-file walking, and no writes exist yet.

## Migrations

None.

## Public API

```rust
// src/config.rs — new
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub model: String,
    pub violation_threshold: f64,
    pub clean_threshold: f64,
    pub max_document_bytes: usize,
    pub request_timeout: std::time::Duration,
}

impl Default for Config {
    fn default() -> Self;
}

impl Config {
    pub fn load(root: &std::path::Path) -> Result<Self, ConfigError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read `{path}`: {source}")]
    Read { path: std::path::PathBuf, source: std::io::Error },
    #[error("could not parse `{path}`: {source}")]
    Parse { path: std::path::PathBuf, source: toml::de::Error },
    #[error("invalid config in `{path}`: {message}")]
    Invalid { path: std::path::PathBuf, message: String },
}
```

```rust
// src/services/testing/model.rs — new
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRequest {
    pub root: std::path::PathBuf,
    pub target: std::path::PathBuf,
    pub rules: Option<std::path::PathBuf>,
}
```

```rust
// src/services/testing/repositories/rules/models.rs — new
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    pub path: Option<String>,
    pub files: FileSelection,
    pub rules: Vec<Rule>,
    pub source: std::path::PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileSelection {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub statement: String,
    pub enabled: bool,
}
```

```rust
// src/services/testing/repositories/rules/repository.rs — new
pub struct RulesRepository;

impl RulesRepository {
    pub fn new() -> Self;
    pub fn load(&self, path: &std::path::Path) -> Result<Vec<RuleSet>, Error>;
}
```

```rust
// src/services/testing/repositories/rules/errors.rs — new
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read `{path}`: {source}")]
    Read { path: std::path::PathBuf, source: std::io::Error },
    #[error("could not parse `{path}`: {source}")]
    Parse { path: std::path::PathBuf, source: toml::de::Error },
    #[error("no rule files in `{0}`")]
    NoRuleFiles(std::path::PathBuf),
    #[error("rule `{id}` in `{path}` has an empty id")]
    EmptyId { path: std::path::PathBuf, id: String },
    #[error("rule `{id}` in `{path}` has an empty statement")]
    EmptyStatement { path: std::path::PathBuf, id: String },
    #[error("rule id `{id}` appears more than once in `{path}`")]
    DuplicateId { path: std::path::PathBuf, id: String },
    #[error("rule file `{path}` has an empty `path`")]
    EmptyPath { path: std::path::PathBuf },
}
```

```rust
// src/services/testing/service.rs — new
pub struct Service {
    pub(crate) rules: RulesRepository,
    pub(crate) config: Config,
}

impl Service {
    pub fn new(rules: RulesRepository, config: Config) -> Self;
    pub fn resolve_rules(&self, request: &TestRequest) -> Result<Vec<RuleSet>, Error>;
}
```

```rust
// src/services/testing/errors.rs — new
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("target `{target}` is outside the working directory `{root}`")]
    TargetOutsideRoot { root: std::path::PathBuf, target: std::path::PathBuf },
    #[error("rule file `{0}` has no `path`; `path` is required unless `--rules` selects the file")]
    MissingRulePath(std::path::PathBuf),
    #[error("invalid glob `{pattern}` in `{path}`: {source}")]
    InvalidRulePath {
        path: std::path::PathBuf,
        pattern: String,
        source: globset::Error,
    },
    #[error("no rules match `{target}`; available paths: {available:?}")]
    NoRulesMatch { target: String, available: Vec<String> },
    #[error("no enabled rules match `{target}`")]
    NoEnabledRules { target: String },
    #[error("duplicate rule id `{id}` in `{first}` and `{second}`")]
    DuplicateRuleId {
        id: String,
        first: std::path::PathBuf,
        second: std::path::PathBuf,
    },
    #[error(transparent)]
    Rules(#[from] crate::services::testing::repositories::rules::Error),
}
```

## Behavior

- `Config::load` reads `<root>/.architecture/config.toml` when it exists. A
  missing file returns `Config::default()`. A malformed file returns `Parse`.
  Invalid values return `Invalid` with a message naming the key.
  `clean_threshold` must be less than `violation_threshold`; both must be in
  `[0, 1]`; `max_document_bytes` and `request_timeout_seconds` must be greater
  than zero.
- `RulesRepository::load` on a file loads exactly that file. On a directory it
  loads every `*.toml` directly inside it, in lexicographic filename order,
  skipping `config.toml`. An empty directory returns `NoRuleFiles`.
- TOML is mapped to `RuleSet`. `path` is optional in the domain type because
  `--rules` may omit it; a present-but-blank `path` is `EmptyPath`. `enabled`
  defaults to `true`. A blank rule id is `EmptyId`; a blank statement is
  `EmptyStatement`.
- Duplicate rule ids within one file are `DuplicateId`. Duplicate ids across
  combined files are `DuplicateRuleId`.
- `Service::resolve_rules` loads `request.rules` when set, otherwise
  `<root>/.architecture`.
- In default mode, it computes the target relative to the root with `/`
  separators, using `.` for the root itself. Each rule set without `path` is
  `MissingRulePath`. Each `path` is compiled with
  `globset::GlobBuilder::new(pattern).literal_separator(true)`; a bad pattern is
  `InvalidRulePath`. The tested path must match exactly; ancestor and descendant
  matching are not used.
- No matched rule set is `NoRulesMatch`, carrying every available `path` in
  load order. Matched sets with zero enabled rules are `NoEnabledRules`.
- In `--rules` mode, path matching is skipped. Every loaded rule set is used and
  its `path` is ignored. Duplicate ids and the enabled-rule check still apply.
- A target that cannot be stripped from the root is `TargetOutsideRoot`.
- The service does not touch the filesystem beyond the rules repository and
  does not read environment variables, the current directory, or stdout.

## Tests

- `Config::load` returns defaults when `config.toml` is absent.
- `Config::load` reads every value from a complete file.
- `Config::load` rejects malformed TOML, `clean_threshold >= violation_threshold`,
  a threshold outside `[0, 1]`, zero `max_document_bytes`, and zero
  `request_timeout_seconds`.
- `RulesRepository::load` loads a single file.
- `RulesRepository::load` loads a directory in filename order and ignores
  `config.toml`.
- `RulesRepository::load` rejects an empty directory, malformed TOML, empty id,
  empty statement, empty `path`, and a duplicate id within one file.
- `Service::resolve_rules` selects a rule set whose `path` matches exactly.
- `Service::resolve_rules` rejects a deeper path when only a parent pattern
  exists.
- `Service::resolve_rules` reports the available paths in `NoRulesMatch`.
- `Service::resolve_rules` uses every loaded set with `TestRequest.rules` and
  ignores `path` matching.
- `Service::resolve_rules` rejects a missing `path` in default mode.
- `Service::resolve_rules` rejects a duplicate id across two matched files.
- `Service::resolve_rules` rejects a run with no enabled rules.
- `Service::resolve_rules` rejects a target outside the root.
- `Service::resolve_rules` matches `.` with `path = "**"`.

## Notes / out of scope

- No CLI in this task. T2 adds `src/bin/cli.rs` and the offline dry run.
- No source-file walking, ignores, document rendering, or Jev client.
- `path` matching is exact by design. A deeper target needs its own rule file or
  `--rules`.
