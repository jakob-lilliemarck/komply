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

`.opencode/skills/` carries komply-specific adaptations of `services`,
`repositories`, `config`, `review-branch`, `brain-dump`, and `task-authoring`.
`documentation` and `skill-authoring` are copied with only the task-path change.
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
