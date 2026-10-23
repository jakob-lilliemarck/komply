# Komply — planning map

Groomed from the design discussion (2026-10-23). A standalone Rust CLI that
sends a directory's source files and a set of architectural statements to Jev
through OpenRouter, then prints a pass / violation / unsure checklist.

| Epic | File | Status | Tasks |
|---|---|---|---|
| `[Komply]` | `komply.md` | settled | T1–T4 (see `tasks/`) |

## Cross-task notes

- Standalone crate at `/home/jakob/Projects/komply`; package `komply`, binary
  `comply`.
- No database, migrations, DI container, HTTP controllers, views, event bus, or
  job queue.
- The service owns matching, rule selection, document assembly, and verdicts.
  Repositories own filesystem and HTTP access. The CLI owns argument parsing
  and presentation.
- One document per invocation, one Jev request.
- `.architecture/` is read from the current working directory only. There is no
  upward search.
- `--rules <file-or-dir>` bypasses path matching.
- `--dry-run` is fully offline and needs no API key.
- Exit codes: `0` all pass, `1` any violation, `3` unsure with no violation,
  `2` any error.
- Task dependencies: T2 → T1; T3 → T1; T4 → T2 and T3.

## Project skills

`.opencode/skills/` is copied from `~/Projects/relay/.opencode/skills/` for the
eight skills both projects use: `brain-dump`, `config`, `documentation`,
`repositories`, `review-branch`, `services`, `skill-authoring`, and
`task-authoring`. The source skills assume sqlx database repositories and DI.
Their module, error, and test structure applies where it fits; the database,
migration, query, DI, and database-entity model sections do not. Repository
models live in each repository's `models.rs` and are plain read-only value
types. `config` is adapted to komply's TOML project policy and environment
credentials.

`controllers`, `views`, `events`, `jobs`, `di`, `distill`, `epic-authoring`, and
`review-codebase` are deliberately absent: this project has no HTTP layer,
views, event bus, job queue, DI container, or Relay board.

## Task files

| Task | File | Scope |
|---|---|---|
| T1 | `tasks/T1-rule-discovery.md` | project shell, config, rule model, rule loading, path matching |
| T2 | `tasks/T2-document-dry-run.md` | file collection, document assembly, size cap, dry-run CLI |
| T3 | `tasks/T3-decisions-client.md` | OpenRouter/System One decisions client, retries, response mapping |
| T4 | `tasks/T4-evaluation-cli.md` | verdicts, report, live run, output, exit codes, README |
