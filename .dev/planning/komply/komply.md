# `[Komply]` — a Jev-backed architecture compliance CLI

Status: settled. Depends on: none. Blocks: none.

## Goal

Build a small, standalone Rust CLI that reads architectural rule statements from
a project's `.architecture/` directory, selects the rules whose `path` matches
the directory being tested, sends that directory's files and the selected
statements to Jev through OpenRouter, and prints a pass / violation / unsure
checklist. It is a manual tool: no database, no agent integration, and no writes
to the project it checks.

## Definition of done

- `cargo build` and `cargo test` pass in `/home/jakob/Projects/komply`.
- `comply test <dir> --dry-run` resolves config and rules, collects the files,
  builds the Jev request, and prints the file summary plus the redacted request
  JSON. It makes no network call and needs no API key.
- `comply test <dir>` sends exactly one Jev request and prints every enabled
  rule as pending, then as pass / violation / unsure.
- A missing `.architecture/`, a path with no matching rule file, an empty file
  set, an oversized document, a missing API key, or an HTTP failure exits `2`
  with a message that names the problem.
- Exit codes: `0` all pass, `1` any violation, `3` unsure with no violation,
  `2` any setup, config, network, or document error.
- The rule format, config format, and `.complyignore` behaviour are documented
  in the project README.
- No database, migrations, DI container, HTTP controllers, views, events, jobs,
  or harness wiring exist in the crate.

## Background

Jev is a decision model, not a chat model. A Decisions request carries one
`state` — a string, object, or array — and a map of typed questions. A `noul`
question answers yes or no with a probability in `[0, 1]`. All questions in one
request are answered in parallel and independently; there is no free-text
reasoning and no line-number output.

That shape determines the design:

- One tested directory becomes one state document.
- Every selected rule becomes one `noul` question in that one request.
- A high probability means the statement is true, which is a violation.
- Jev cannot point at a line, so the report points at the tested directory and
  the rule, never at a line number.
- The tool is language-agnostic: it walks directories and reads UTF-8 files, not
  Rust syntax.

The project was originally called `comply`, but that name is taken on
crates.io. The package is `komply`; the binary is `comply`.

## Locked decisions

- Package `komply`, binary `comply`; standalone crate at
  `/home/jakob/Projects/komply`.
- Rust 2024. No database, migrations, DI container, HTTP controllers, views,
  events, or jobs.
- `.architecture/` is always read from the current working directory. There is
  no upward search. The tested path must be inside the working directory.
- One rule file owns one `path` glob. The glob is matched **exactly** against
  the tested directory, relative to the working directory. A rule for
  `src/services/*/repositories` does not match
  `src/services/foo/repositories/tasks`; that is an error, not a silent skip.
- `--rules <file-or-dir>` loads rules explicitly and bypasses path matching.
  This is the escape hatch for testing a directory no rule targets.
- Rule schema is `path`, optional `[files]` include/exclude, and `[[rules]]`
  entries with `id`, `statement`, and `enabled`. There is no `[scope]`,
  `severity`, `criteria`, drop-in, or override mechanism in v1.
- A rule is a declarative statement. The tool wraps it as
  `Is the following statement true of the attached code? <statement>`.
  Violation is the true answer.
- One document per invocation, one Jev request. The document contains every
  selected file under the tested directory, separated by `<<<FILE>>>` markers.
- No chunking, no per-file requests, no multi-row batching, no line-number
  localisation.
- `.complyignore` is project-root-relative. `[files].exclude` is
  target-relative. Both use glob syntax; neither supports negation in v1.
- Non-UTF-8 files are skipped and reported. There is no extension filter and
  no silent truncation.
- Document size is capped by config. Exceeding the cap is an error; the user
  narrows the target, adds an exclude, or raises the cap.
- Thresholds and the model live in `.architecture/config.toml`. The only value
  the CLI overrides is the model (`--model`).
- Repositories are concrete; there are no ports or traits. The service holds
  `RulesRepository`, `FilesRepository`, and `Config`; the `DecisionsRepository`
  is passed to `run`, so `--dry-run` never constructs credentials.
- Output is a plain pending list followed by a final status list. No cursor
  rewriting. Colour is automatic on a TTY, with no flag.
- Exit codes are fixed: `0` pass, `1` violation, `3` unsure-only, `2` error.

## Design

### Project layout

```
/home/jakob/Projects/komply/
  Cargo.toml
  src/
    lib.rs
    config.rs
    bin/
      cli.rs
    services/
      mod.rs
      testing/
        mod.rs
        model.rs
        service.rs
        errors.rs
        repositories/
          mod.rs
          rules/
            mod.rs
            repository.rs
            models.rs
          files/
            mod.rs
            repository.rs
            models.rs
          decisions/
            mod.rs
            repository.rs
            models.rs
  .opencode/
    skills/
      services/
      repositories/
      config/
      documentation/
      skill-authoring/
      review-branch/
      brain-dump/
      task-authoring/
  .dev/
    planning/
```

`src/lib.rs` exposes `config` and `services`. `src/bin/cli.rs` uses the library;
it holds no domain logic.

### Crate manifest

```toml
[package]
name = "komply"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "comply"
path = "src/bin/cli.rs"

[lib]
name = "komply"
path = "src/lib.rs"

[dependencies]
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
walkdir = "2"
globset = "0.4"
ureq = { version = "2", features = ["json", "tls"] }

[dev-dependencies]
tempfile = "3"
```

No `clap`: argument parsing is small and hand-written. No colour crate: ANSI
codes and `std::io::IsTerminal` are enough. No `anyhow`: every layer has a typed
`thiserror` error.

### Configuration

Config is a value object loaded from `<root>/.architecture/config.toml` when the
file exists. A missing file means defaults. A malformed file or an invalid value
is an error. The CLI applies `--model` after loading.

```toml
[comply]
model = "typesafe/jev-1.13"
violation_threshold = 0.80   # noul >= this  => violation
clean_threshold = 0.20       # noul <= this  => pass
max_document_bytes = 400000
request_timeout_seconds = 60
```

Defaults:

| Key | Default |
|---|---|
| `model` | `typesafe/jev-1.13` |
| `violation_threshold` | `0.80` |
| `clean_threshold` | `0.20` |
| `max_document_bytes` | `400000` |
| `request_timeout_seconds` | `60` |

Validation:

- `clean_threshold` must be less than `violation_threshold`.
- Both thresholds must be in `[0, 1]`.
- `max_document_bytes` must be greater than zero.
- `request_timeout_seconds` must be greater than zero.

`Config` carries no API key. `OPENROUTER_API_KEY` is read by the decisions
repository only, when a live run constructs it. `COMPLY_DECISIONS_URL`
overrides the endpoint for development and tests; it defaults to
`https://openrouter.ai/api/alpha/decisions`.

### Rule files

Every `*.toml` directly under `.architecture/` is a rule file except
`config.toml`, which is reserved. Subdirectories are not searched. Files are
loaded in lexicographic filename order.

```toml
# .architecture/repositories.toml
path = "src/services/*/repositories/*"

[files]
include = ["**/*"]                 # optional; defaults to ["**/*"]
exclude = ["target/**", "**/*.lock"]

[[rules]]
id = "repo.no-domain-branching"
statement = "There is at least one method implemented on Read, Write or WriteTx that is not a wrapper around search_*, store_* or archive_* functions."
enabled = true

[[rules]]
id = "repo.no-cross-service-writes"
statement = "The code writes to a table owned by another service."
enabled = true
```

Field rules:

- `path` is required in the default load mode. It is a glob relative to the
  working directory. It is optional when the file is loaded through
  `--rules`; there it is ignored.
- `[files].include` and `[files].exclude` are optional. `include` defaults to
  `["**/*"]`. Both are target-relative.
- `[[rules]]` entries require a non-empty `id` and a non-empty `statement`.
  `enabled` defaults to `true`.
- `id` must be unique within one file. It must also be unique across every rule
  set selected for one run.
- A rule file with no rules, or one whose every rule is disabled, is valid but
  contributes nothing.

When several rule files match the same directory, their rules combine. Their
file selections combine too: a file is sent when it matches any selected
`include` and no selected `exclude`. Duplicate rule ids across the combined set
are an error.

`--rules <file-or-dir>` loads exactly that file, or every `*.toml` directly
inside that directory except `config.toml`. Path matching is skipped entirely,
so a rule file loaded this way may omit `path`.

### Rule matching

The CLI canonicalises the working directory and the tested path, then checks
that the tested path is a directory inside the working directory. The service
computes the tested path relative to the root and renders it with `/`
separators. Testing the working directory itself yields `.`.

Each rule file's `path` is compiled as a `globset::Glob` with
`literal_separator(true)`. `*` matches one path segment; `**` crosses segments.
The tested directory must match the glob **exactly**. There is no ancestor or
descendant matching.

- `path = "src/services/*"` matches `src/services/planning`, not
  `src/services/planning/repositories`.
- `path = "src/services/*/repositories/*"` matches
  `src/services/planning/repositories/tasks`, not the `repositories` directory
  itself.
- `path = "**"` matches every tested directory, including `.`.

If no rule file matches, the run fails with the tested path and every available
`path` from the loaded files. If every matched rule is disabled, the run fails
with a distinct error. Neither case produces a green report.

### `.complyignore`

`.complyignore` sits at the working-directory root, next to `.architecture/`.
It is read when it exists; a missing file means no extra ignores. Its patterns
are project-root-relative.

- One glob per line.
- Blank lines are ignored.
- Lines starting with `#` are comments.
- A trailing `/` is stripped; the pattern still applies to a directory.
- No negation, no `!` support in v1.
- A matching directory prunes its whole subtree.

The service always adds `.git` and `.architecture` to the project-root ignores.
It does not hardcode `target`, `node_modules`, or any language-specific path;
projects put those in `.complyignore`.

`[files].exclude` is separate: its patterns are relative to the tested
directory. The files repository applies both sets with the correct base.

### File collection

The files repository walks the tested directory. For each entry:

- Directories matching a project ignore or a target exclude are pruned.
- Files must match the selected target-relative `include` patterns.
- Files matching a project ignore or a target exclude are skipped.
- Symbolic links are not followed and are not read as files.
- The file is read as bytes. If it is not valid UTF-8, it is recorded as a
  skipped non-UTF-8 file and omitted from the document.
- If a single file is larger than `max_document_bytes`, the run fails with the
  file path and the limit. This prevents reading an unbounded file before the
  total check.
- Files are sorted by target-relative path for a deterministic document.

The result carries the included files, the skipped files, and the total bytes
of the included contents. An empty included set is an error.

### The document

The document contains every included file, in sorted order, with a marker
before and after each file. The relative path is relative to the tested
directory and uses `/` separators.

```
<<<FILE service.rs>>>
use crate::...
<<<END FILE>>>
<<<FILE errors.rs>>>
...
<<<END FILE>>>
```

The state sent to Jev begins with:

```
The following document contains files from `src/services/planning`.
Each file is delimited by `<<<FILE relative/path>>>` and `<<<END FILE>>>`.

```

If a file does not end in a newline, one is added before its `<<<END FILE>>>`
marker so markers never join content. The size cap applies to the final state
string, including the preamble and markers.

### Jev request and response

The decisions repository posts one JSON body:

```json
{
  "model": "typesafe/jev-1.13",
  "state": "The following document contains files from ...\n<<<FILE service.rs>>>\n...\n<<<END FILE>>>\n",
  "questions": {
    "r0": {
      "type": "noul",
      "instructions": "Is the following statement true of the attached code? There is at least one method ..."
    },
    "r1": {
      "type": "noul",
      "instructions": "Is the following statement true of the attached code? The code writes ..."
    }
  }
}
```

- Headers: `Authorization: Bearer $OPENROUTER_API_KEY` and
  `Content-Type: application/json`.
- Question keys are `r0`, `r1`, ... in rule order. The service keeps the
  key-to-rule mapping; the decisions repository never sees rule ids.
- No `criteria` is sent. A self-contained statement is the contract.
- The response is read as `answers.<key>.noul` plus `usage.cost`,
  `usage.input_tokens`, and `usage.output_tokens`.
- Every requested key must be present, must have `type = "noul"`, and must carry
  a probability in `[0, 1]`. Anything else is a decode error.
- The request timeout is `request_timeout_seconds`.

Transport retries:

- Retry `429`, every `5xx`, and a `402` whose body has
  `error.metadata.limit_source = "openrouter_in_flight_budget"`.
- Do not retry any other `4xx`, including an ordinary `402`.
- At most five attempts.
- Honour a numeric `Retry-After` header; ignore an HTTP-date value.
- Otherwise back off from `1s`, doubling to a `30s` cap, with a small jitter.
- On exhaustion, return the status and response body.

### Verdicts, exit codes, and errors

For each rule answer:

- `noul >= violation_threshold` → `Violation`
- `noul <= clean_threshold` → `Pass`
- otherwise → `Unsure`

Exit codes:

| Code | Meaning |
|---|---|
| `0` | every rule passed |
| `1` | at least one violation |
| `3` | no violations, but at least one unsure |
| `2` | any error before or during the run |

Required error cases and messages:

| Case | Message shape |
|---|---|
| `.architecture/` missing | ``could not read rules directory `<root>/.architecture`: ...`` |
| no rule path matches | ``no rules match `<target>`; available paths: ...`` |
| no enabled rules | ``no enabled rules match `<target>` `` |
| `path` missing in default mode | ``rule file `<path>` has no `path`` |
| duplicate rule id | ``duplicate rule id `<id>` in `<path-a>` and `<path-b>`` |
| invalid glob | ``invalid glob `<pattern>` in `<path>`: ...`` |
| target outside root | ``target `<target>` is outside the working directory `<root>` `` |
| target not a directory | ``target is not a directory: `<path>` `` |
| no files | ``no files to check under `<target>` `` |
| file too large | ```<file>` is `<n>` bytes, over the `<limit>`-byte limit`` |
| document too large | ``document is `<n>` bytes, over the `<limit>`-byte limit; raise `max_document_bytes` or narrow the target`` |
| missing API key | ``OPENROUTER_API_KEY is not set`` |
| HTTP status | ``decisions API returned `<status>`: `<body>` `` |
| malformed response | ``could not decode the decisions response: ...`` |

The CLI prints `error: <message>` to stderr and exits `2`. It never prints a
stack trace unless `RUST_BACKTRACE` makes the formatter do so.

### CLI

```
comply test <dir> [--rules <file-or-dir>] [--dry-run] [--model <id>]
comply --help
comply test --help
```

- `<dir>` is resolved from the working directory. It must exist and be a
  directory.
- `--rules` selects a rule file or a directory of rule files, resolved from the
  working directory like `<dir>`, and bypasses path matching.
- `--dry-run` performs resolution and document assembly, then prints the header,
  the file summary, and the request JSON with `state` replaced by a placeholder.
  It does not construct a decisions client.
- `--model` overrides `comply.model` from the config file.
- Any unknown flag or missing argument prints usage and exits `2`.

A real run prints the header and the pending list before calling Jev, then the
final list:

```
Rules:   .architecture/repositories.toml
Target:  src/services/planning/repositories/tasks
Model:   typesafe/jev-1.13
Files:   6 (38210 bytes)
Skipped: 1 non-UTF-8 (assets/logo.bin)

  [pending] repo.no-domain-branching
  [pending] repo.no-cross-service-writes

  [PASS]      repo.no-domain-branching          noul=0.04
  [VIOLATION] repo.no-cross-service-writes      noul=0.91
  [UNSURE]    repo.no-inline-sql                noul=0.54

1 violation, 1 unsure, 1 pass · $0.00012 · 476 input tokens
```

`--dry-run` output:

```
Rules:   .architecture/repositories.toml
Target:  src/services/planning/repositories/tasks
Model:   typesafe/jev-1.13
Files:   6 (38210 bytes)
  repository.rs  4210
  models.rs      2830
  ...
Skipped: 1 non-UTF-8 (assets/logo.bin)

Request:
{
  "model": "typesafe/jev-1.13",
  "state": "<<<omitted: 38210 bytes across 6 files>>>",
  "questions": {
    "r0": {
      "type": "noul",
      "instructions": "Is the following statement true of the attached code? ..."
    }
  }
}
```

The file summary lists target-relative paths and byte counts. The header byte
count is the sum of the included file contents; the placeholder reports the
rendered state length, which also includes the preamble and markers. The
placeholder is a string, so the rest of the JSON is exactly the body that would
be sent.

Colour:

- `PASS` green, `VIOLATION` red, `UNSURE` yellow, when stdout is a TTY.
- No colour flags; piping the output disables colour.
- Pending lines are uncoloured.
- No cursor movement, no clearing, no spinner.

### Testing

- Pure functions are tested directly: config validation, glob matching,
  file-selection merging, document rendering, request construction, verdict
  boundaries, and report counts.
- `rules` is tested against a fixture `.architecture/` directory: all files
  load, `config.toml` is ignored, malformed TOML fails, duplicate ids fail.
- `files` is tested against a fixture tree: includes, target excludes, project
  ignores, directory pruning, deterministic order, and non-UTF-8 skipping.
- `decisions` is tested against a local `TcpListener` stub. No test reaches the
  network and no test needs an API key.
- The service's live path is tested by constructing a `DecisionsRepository`
  pointed at the stub and asserting the exact verdicts and report.
- The CLI's argument parsing and `exit_code` mapping are unit-tested.
- `--dry-run` is tested without a key.

Fixture tree:

```
tests/fixtures/project/
  .architecture/
    config.toml
    services.toml
    repositories.toml
  .complyignore
  src/services/alpha/service.rs
  src/services/alpha/errors.rs
  src/services/alpha/repositories/tasks/repository.rs
  src/services/alpha/repositories/tasks/models.rs
  src/services/alpha/repositories/tasks/queries/search_tasks.rs
  src/services/alpha/assets/logo.bin        # non-UTF-8
  src/services/beta/service.rs
```

## Task breakdown

| # | Layers touched | Scope | Depends on |
|---|---|---|---|
| T1 | crate, config, services, repositories | Project shell; `Config`; rule model; rule loader; exact path matching; duplicate / no-match / no-enabled errors | — |
| T2 | services, repositories, cli | File collection; `.complyignore`; document assembly; size cap; `Service::prepare`; offline `--dry-run` CLI | T1 |
| T3 | repositories | Decisions repository: OpenRouter request/response, auth, timeout, retry policy, response validation | T1 |
| T4 | services, cli, docs | `Service::run`/`check`; verdicts; report; live CLI output; exit codes; README | T2, T3 |

T2 and T3 can proceed in parallel after T1. T4 is the integration task.

## Blast radius

- The entire crate is new; no existing production behaviour changes.
- No migrations, no database, no `.sqlx` cache, no query refresh.
- No changes to Relay or any other repository.
- Runtime network access goes only to the configured Decisions endpoint, and
  only for a live run. `--dry-run` and all tests stay offline.
- The tool reads any UTF-8 file under the tested directory. Projects must use
  `.complyignore` or `[files].exclude` to keep secrets and generated content out
  of the document. The README will say this explicitly.
- The API key is never written to disk, printed, or placed in the request body.

## Out of scope

- Drop-ins or per-directory override files.
- `[scope]`, `severity`, `criteria`, or rule inheritance.
- Caching answers by content hash.
- Line numbers or function-level localization.
- Per-file requests, chunking, or multiple units per invocation.
- Multiple models, fallback models, or model routing preferences.
- JSON output, `--no-color`, threshold flags, `--fail-on-unsure`.
- Language-specific preprocessing such as stripping `#[cfg(test)]` blocks.
- Agent, harness, or Relay integration.
- Writing anything into the tested project.

## Resolved questions

- **Why not a port/trait for the decisions client?** Concrete repositories were
  chosen. The service still takes the decisions repository as a `run` argument,
  so dry-run needs no credentials and tests can point a real repository at a
  stub.
- **Why no `[scope]`?** One `path` per rule file is enough to select a rule set;
  file selection lives in `[files]`.
- **Why no `criteria`?** It can improve precision, but every rule already works
  as a self-contained yes/no statement. It can be added later without breaking
  the schema.
- **Why no drop-ins?** They add merge semantics before the base format has
  proven itself. The rule files are ordinary project files that can be edited.
- **Why store thresholds in TOML instead of flags?** They are project policy,
  not per-invocation choices. `--model` is the only useful per-run override.
- **Why exit `3` for unsure?** It distinguishes "could not confirm" from both
  compliance and a violation while still failing a script.

## Maintainer steps

None. No migrations, no database, no query cache, no deployment change.
