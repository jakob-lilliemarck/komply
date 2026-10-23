## Skills

`@services`, `@repositories`, `@config`, `@documentation` — the project-local
skills under `.opencode/skills/`.

## Goal

Complete the tool: interpret Jev's probabilities as pass / violation / unsure,
build the report, wire the live `comply test <dir>` path, print the pending and
final checklists, and map the result to the agreed exit codes.

## Definition of done

- `Verdict`, `RuleResult`, and `Report` exist and carry every value the CLI
  needs.
- `Service::run` evaluates a prepared test through an injected
  `DecisionsRepository`; `Service::check` prepares and runs.
- `verdict` applies the configured thresholds at exact boundaries.
- `comply test <dir>` loads the API key, prints every enabled rule as pending,
  sends one request, prints the final statuses, the summary, the cost, and the
  input-token count.
- Exit codes are `0` all pass, `1` any violation, `3` unsure with no violation,
  and `2` any error.
- `README.md` is a short introduction with installation and a quick start, and
  links to `docs/index.md`. The docs cover usage, the rule file schema, config
  defaults, `.complyignore`, the security note about sending files to
  OpenRouter, and the exit codes.
- `cargo test` passes, including a service test against a local Decisions stub.

## Migrations

None.

## Public API

```rust
// src/services/testing/model.rs — additions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Violation,
    Unsure,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuleResult {
    pub id: String,
    pub statement: String,
    pub noul: f64,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub target: std::path::PathBuf,
    pub target_display: String,
    pub rule_source: std::path::PathBuf,
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
    pub fn violations(&self) -> usize;
    pub fn unsure(&self) -> usize;
    pub fn passes(&self) -> usize;
}
```

```rust
// src/services/testing/service.rs — additions
pub fn verdict(noul: f64, config: &Config) -> Verdict;

impl Service {
    pub fn run(
        &self,
        prepared: PreparedTest,
        decisions: &DecisionsRepository,
    ) -> Result<Report, Error>;
    pub fn check(
        &self,
        request: TestRequest,
        decisions: &DecisionsRepository,
    ) -> Result<Report, Error>;
}
```

`Service::new`, `resolve_rules`, and `prepare` keep their T2 signatures.

```rust
// src/services/testing/errors.rs — additions
#[error(transparent)]
Decisions(#[from] crate::services::testing::repositories::decisions::Error),
```

```rust
// src/bin/cli.rs — additions and changes
fn main() -> std::process::ExitCode;
fn run(args: Args) -> Result<u8, String>;
fn exit_code(report: &Report) -> u8;
fn print_header(prepared: &PreparedTest);
fn print_pending(prepared: &PreparedTest);
fn print_report(report: &Report);
```

## Behavior

### Verdicts

- `noul >= config.violation_threshold` is `Violation`.
- `noul <= config.clean_threshold` is `Pass`.
- Anything between is `Unsure`.
- The comparisons are inclusive at both boundaries.

### Evaluation

- `Service::run` calls `decisions.evaluate(&prepared.evaluation)` once. The
  repository retries transient failures internally; the service does not.
- It walks the prepared rules and questions in their shared order. For each
  pair it reads `decision.answers[question.key]` and builds a `RuleResult` with
  the rule id, statement, probability, and `verdict`.
- The repository validates every requested key before returning. A missing
  answer reaches the service as `Decisions(decisions::Error::MissingAnswer)`.
- `Report` carries the prepared target and source display paths, model,
  `document.files.len()`, `document.total_bytes`, skipped files, results, and
  the usage values.
- `Service::check` is `run(prepare(request)?, decisions)`.
- The service does not read the API key, construct the client, or print
  anything.

### Live CLI

- `comply test <dir>` builds `Config`, applies `--model`, prepares the test, and
  prints the header and pending list before any network call.
- It constructs `DecisionsRepository::from_env(config.request_timeout)`. A
  missing key or any client error exits `2`.
- It calls `Service::run`, prints the final list, the summary, the cost when
  present, and the input-token count when present.
- `--dry-run` keeps the T2 path: header, file summary, redacted request JSON,
  exit `0`.
- `--rules` and `--help` keep the T2 behavior.

### Output

Header:

```
Rules:   .architecture/repositories.toml
Target:  src/services/planning/repositories/tasks
Model:   typesafe/jev-1.13
Files:   6 (38210 bytes)
Skipped: 1 non-UTF-8 (assets/logo.bin)
```

`Skipped:` is omitted when nothing was skipped. `Rules:` uses
`rule_source_display`; `Target:` uses `target_display`.

Pending:

```
  [pending] repo.no-domain-branching
```

Final:

```
  [PASS]      repo.no-domain-branching          noul=0.04
  [VIOLATION] repo.no-cross-service-writes      noul=0.91
  [UNSURE]    repo.no-inline-sql                noul=0.54
```

Statuses are padded so the rule ids align. Probabilities print with two
decimals. The summary reads
`<n> violation(s), <n> unsure, <n> pass · $<cost> · <n> input tokens`; the cost
and token segments are omitted when absent.

Colour is applied only when `std::io::stdout().is_terminal()`:
green `PASS`, red `VIOLATION`, yellow `UNSURE`. Pending lines, headers, and
summaries are uncoloured. There is no cursor movement and no spinner.

### Exit codes

- `0` when `Report::violations() == 0` and `Report::unsure() == 0`.
- `1` when `Report::violations() > 0`.
- `3` when there are no violations and `Report::unsure() > 0`.
- `2` for every error path.

### Errors

- API key, client construction, transport, status, decode, missing answer, and
  invalid probability errors are printed as `error: <message>` on stderr and
  exit `2`.
- Config, rule, file, size, and no-match errors keep the T1/T2 messages and
  exit `2`.
- Errors never produce a partial green report.

## Tests

- `verdict` returns `Violation` exactly at `violation_threshold`.
- `verdict` returns `Pass` exactly at `clean_threshold`.
- `verdict` returns `Unsure` strictly between the thresholds.
- `Report::violations`, `unsure`, and `passes` count mixed results.
- `Service::run` against a stub returns one `RuleResult` per enabled rule, in
  rule order, with the expected verdicts.
- `Service::run` copies cost and token usage into the report.
- `Service::run` returns `Decisions(MissingAnswer)` when the stub omits a
  requested key.
- `Service::check` prepares and evaluates in one call.
- `exit_code` returns `0`, `1`, and `3` for the three report shapes, and
  violation wins over unsure.
- `print_report` emits `PASS`, `VIOLATION`, and `UNSURE` labels and omits the
  cost segment when cost is absent.
- `print_pending` emits one pending line per enabled rule.
- A live CLI run with the stub and a temporary `COMPLY_DECISIONS_URL` exits with
  the expected code; skip this case if the process environment cannot be set
  safely and rely on the service test instead.

## Notes / out of scope

- No caching, retries outside the decisions repository, line numbers, JSON
  output, `--no-color`, threshold flags, or `--fail-on-unsure`.
- `docs/security.md` must say that every included file is sent to OpenRouter
  and that `.complyignore` / `[files].exclude` are the way to keep secrets out.
