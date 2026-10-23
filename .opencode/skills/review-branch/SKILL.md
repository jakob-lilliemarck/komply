---
name: review-branch
description: |
  Use when reviewing work done on a branch or pull request in komply. Drives the
  review from `git diff main...HEAD`, classifies touched files by layer, and
  checks each layer against the skill that owns its rules.
---

# Reviewing a branch

Start from the diff, not memory.

```bash
git diff main...HEAD
git status
git log --oneline main..HEAD
```

Read every changed file in full before judging it. Cite `file:line`.

## Classify the touched files

| Path | Layer | Skill |
|---|---|---|
| `src/config.rs` | config | `@config` |
| `src/services/testing/service.rs`, `model.rs`, `errors.rs` | service | `@services` |
| `src/services/testing/repositories/**` | repository | `@repositories` |
| `src/bin/cli.rs` | presentation | this skill plus `@services` |
| `.architecture/**`, `README.md`, docs | project data / docs | `@documentation` |
| `Cargo.toml`, `Cargo.lock` | crate | this skill |

A change often spans layers. Review each layer with its own skill, then review
the seams.

## Per-layer checks

### Config

- Values are parsed and validated at load time.
- Defaults are explicit.
- No credential is stored in `Config`.
- The service receives a value; it does not read the file or environment.

### Service

- Business decisions live here: matching, selection, assembly, thresholds.
- No filesystem, network, environment, cwd, stdout, or exit codes.
- Repositories are called through their public methods, not reimplemented.
- Pure helpers are testable without fixtures.
- Errors are typed and carry the failing path or key.

### Repositories

- Pure data access; no domain decisions.
- One resource each; no repository reaches into another's resource.
- Paths, patterns, limits, credentials, and endpoints are injected.
- No `current_dir`, environment read inside `new`, or hardcoded project layout.
- File reads are deterministic and skip non-UTF-8 with a reported reason.
- HTTP responses are validated; retries are limited to the documented cases.
- No writes.

### CLI

- Parsing and presentation only.
- Errors go to stderr as `error: ...` and exit `2`.
- Exit codes: `0` pass, `1` violation, `3` unsure-only, `2` error.
- `--dry-run` is offline and constructs no credentials.
- Output is readable when piped: no cursor movement, no mandatory colour.

## Cross-cutting checks

- `cargo test` passes.
- Tests do not hit the network and do not require `OPENROUTER_API_KEY`.
- No secret appears in source, tests, output, or the request body.
- `.architecture/` changes match the documented schema.
- The document never silently truncates; an oversized document is an error.
- No database, migration, DI, event, job, controller, or view code appears.
- The README still describes the code.

## Report

Order findings by severity:

1. Behaviour that could silently report compliance when the code is not
   compliant.
2. Leaked credentials or files that should not be sent.
3. Broken layering or IO in the wrong layer.
4. Missing or weak tests.
5. Naming, docs, and style.

For each finding give `file:line`, what is wrong, and the smallest change that
fixes it. Do not restate the diff.
