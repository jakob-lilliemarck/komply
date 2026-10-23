## Skills

`@config`, `@services`, `@repositories`, `@documentation` — the project-local
skills under `.opencode/skills/`.

## Goal

Collect the tested directory's files, honour `.complyignore` and the selected
rule files' `[files]` filters, assemble the single document sent to Jev, and
expose an offline `comply test <dir> --dry-run` that prints the file summary and
the redacted request JSON.

## Definition of done

- `src/services/testing/repositories/files/` exists and implements
  `FilesRepository` with `load_ignore` and `collect`.
- `Service` owns a `FilesRepository` and implements `prepare`.
- `src/bin/cli.rs` parses `test <dir> [--rules <path>] [--dry-run] [--model <id>]`.
- `comply test <dir> --dry-run` prints the header, the included file list with
  byte counts, skipped files, and the request JSON with `state` replaced by a
  placeholder. It makes no network call and needs no API key.
- A live run without `--dry-run` exits `2` with a clear "evaluation is not
  implemented yet; use --dry-run" message.
- `cargo test` passes.

## Migrations

None.

## Public API

```rust
// src/services/testing/repositories/files/models.rs — new
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ignore {
    pub project: Vec<String>,
    pub target: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub relative_path: std::path::PathBuf,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFile {
    pub relative_path: std::path::PathBuf,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    NonUtf8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collected {
    pub files: Vec<SourceFile>,
    pub skipped: Vec<SkippedFile>,
    pub total_bytes: usize,
}

pub struct CollectOptions<'a> {
    pub selection: &'a FileSelection,
    pub ignore: &'a Ignore,
    pub max_file_bytes: usize,
}
```

```rust
// src/services/testing/repositories/files/repository.rs — new
pub struct FilesRepository;

impl FilesRepository {
    pub fn new() -> Self;
    pub fn load_ignore(&self, path: &std::path::Path) -> Result<Ignore, Error>;
    pub fn collect(
        &self,
        root: &std::path::Path,
        target: &std::path::Path,
        options: &CollectOptions<'_>,
    ) -> Result<Collected, Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read `{path}`: {source}")]
    Read { path: std::path::PathBuf, source: std::io::Error },
    #[error("could not walk `{path}`: {source}")]
    Walk { path: std::path::PathBuf, source: walkdir::Error },
    #[error("invalid glob `{pattern}`: {source}")]
    Glob { pattern: String, source: globset::Error },
    #[error("target is not a directory: `{0}`")]
    NotDirectory(std::path::PathBuf),
    #[error("`{path}` is {bytes} bytes, over the {limit}-byte limit")]
    FileTooLarge {
        path: std::path::PathBuf,
        bytes: u64,
        limit: usize,
    },
}
```

```rust
// src/services/testing/model.rs — additions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub files: Vec<SourceFile>,
    pub total_bytes: usize,
}

impl Document {
    pub fn new(files: Vec<SourceFile>) -> Self;
    pub fn render(&self) -> String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationRequest {
    pub model: String,
    pub state: String,
    pub questions: Vec<EvaluationQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationQuestion {
    pub key: String,
    pub instruction: String,
}

impl EvaluationRequest {
    pub fn to_json(&self, state_override: Option<&str>) -> serde_json::Value;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTest {
    pub target: std::path::PathBuf,
    pub target_display: String,
    pub rule_source: std::path::PathBuf,
    pub rule_source_display: String,
    pub rules: Vec<Rule>,
    pub document: Document,
    pub skipped: Vec<SkippedFile>,
    pub evaluation: EvaluationRequest,
}
```

```rust
// src/services/testing/service.rs — additions
pub struct Service {
    pub(crate) rules: RulesRepository,
    pub(crate) files: FilesRepository,
    pub(crate) config: Config,
}

impl Service {
    pub fn new(rules: RulesRepository, files: FilesRepository, config: Config) -> Self;
    pub fn resolve_rules(&self, request: &TestRequest) -> Result<Vec<RuleSet>, Error>;
    pub fn prepare(&self, request: TestRequest) -> Result<PreparedTest, Error>;
}
```

```rust
// src/services/testing/errors.rs — additions
#[error("no files to check under `{0}`")]
NoFiles(std::path::PathBuf),
#[error("document is {bytes} bytes, over the {limit}-byte limit; raise `max_document_bytes` or narrow the target")]
DocumentTooLarge { bytes: usize, limit: usize },
#[error(transparent)]
Files(#[from] crate::services::testing::repositories::files::Error),
```

```rust
// src/bin/cli.rs — new
fn main() -> std::process::ExitCode;

enum Command {
    Test(Args),
    Help,
}

struct Args {
    target: std::path::PathBuf,
    rules: Option<std::path::PathBuf>,
    dry_run: bool,
    model: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Command, String>;
fn run(args: Args) -> Result<u8, String>;
```

## Behavior

### Rule file selection merge

- Treat an empty `include` in one matched set as `["**/*"]` for that set. Union
  every set's resulting include list, preserving first-seen order.
- Union every set's exclude, preserving first-seen order.
- A file is sent when it matches an include and no exclude.
- The union applies to both default loads and `--rules` loads.

### Ignore loading

- `FilesRepository::load_ignore` reads the supplied path. A missing file returns
  `Ignore::default()`.
- It trims each line, skips blank lines and `#` comments, strips one trailing
  `/`, and keeps every other line as a glob. `!` is not special.
- The service always adds `.git` and `.architecture` to `ignore.project`.
- The service places the merged rule-file excludes in `ignore.target`.

### Collection

- The target must be a directory; otherwise `NotDirectory`.
- Walk the target without following symbolic links. A symlink entry is skipped.
- For each directory, compute its path relative to the root and to the target.
  If it matches a project ignore or a target exclude, prune the subtree.
- For each regular file, compute both relative paths. Skip it unless it matches
  the target-relative include set and matches no project ignore or target
  exclude.
- Read the file's metadata first. If its length exceeds `max_file_bytes`, return
  `FileTooLarge` with the path, length, and limit.
- Read the bytes. `String::from_utf8` failure records `SkippedFile` with
  `SkipReason::NonUtf8` and omits the file from the document.
- Sort included files by `relative_path`. `total_bytes` is the sum of the UTF-8
  contents' byte lengths.

### Document

- `Document::render` emits, for each file in order:

  ```
  <<<FILE relative/path>>>
  <contents>
  <<<END FILE>>>
  ```

- A missing final newline in a file is added before `<<<END FILE>>>`.
- `Service::prepare` builds the state as the preamble followed by the rendered
  document. The preamble names the target display path and explains the
  markers, exactly as shown in `komply.md`.
- The size check runs on the final `state.len()`, not just the file contents.
  Over `config.max_document_bytes` is `DocumentTooLarge`.
- No included files is `NoFiles`.
- Questions are built in rule order from enabled rules only, with keys `r0`,
  `r1`, ... and instructions
  `Is the following statement true of the attached code? <statement>`.
- `target_display` is the target relative to the root, with `/` separators and
  `.` for the root. `rule_source_display` is the rules file or directory
  relative to the root when possible, otherwise the path as supplied.

### Dry-run

- `--dry-run` prints the header, one line per included file with its byte count,
  skipped files, then `Request:` and the pretty JSON from
  `EvaluationRequest::to_json(Some(placeholder))`.
- The placeholder is
  `<<<omitted: <state bytes> bytes across <file count> files>>>`.
- No API key is read and no decisions client is constructed.
- `--model` replaces the config model before `prepare`.
- `--help` prints usage and exits `0`.
- Unknown flags, a missing target, and a missing flag value print usage and exit
  `2`.
- A live run without `--dry-run` prints
  `error: live evaluation is not implemented yet; use --dry-run` and exits `2`.

## Tests

- `load_ignore` returns an empty value for a missing file.
- `load_ignore` removes comments and blank lines, strips a trailing `/`, and
  keeps ordinary patterns.
- `collect` includes all UTF-8 files by default.
- `collect` honours a target-relative include.
- `collect` honours a target-relative exclude and prunes an excluded directory.
- `collect` honours a project-root-relative ignore for a nested path.
- `collect` records a non-UTF-8 file in `skipped` and omits it.
- `collect` returns files in deterministic path order.
- `collect` returns `NotDirectory` for a file target.
- `collect` returns `FileTooLarge` when a file exceeds the passed limit.
- `Document::render` emits the exact markers and adds a missing final newline.
- `Service::prepare` merges include and exclude lists across matched rule sets.
- `Service::prepare` adds `.git` and `.architecture` to project ignores.
- `Service::prepare` builds one question per enabled rule in order.
- `Service::prepare` returns `NoFiles` for an empty selection.
- `Service::prepare` returns `DocumentTooLarge` when the final state exceeds the
  cap.
- `EvaluationRequest::to_json` replaces the state when an override is given and
  uses the real state otherwise.
- `parse_args` accepts `test` with every flag, rejects an unknown flag, and
  rejects a missing target.
- The dry-run renderer prints the file summary and a JSON object whose `state`
  is the placeholder.

## Notes / out of scope

- No live evaluation and no decisions repository; T3 adds the client and T4
  wires the live path.
- No extension filtering, no `#[cfg(test)]` stripping, and no chunking.
- No writes.
