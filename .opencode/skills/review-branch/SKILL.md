---
name: review-branch
description: |
  Use when reviewing work done on a branch or PR. Drives the review from
  `git diff main...HEAD`, classifies touched files by layer, and checks each
  layer against the skill that owns its rules. Covers instrumentation,
  documentation, test coverage, and infrastructure wiring.
---

# Review Branch

A file-driven review workflow. The review is scoped to what changed — only
touched files are inspected. For whole-codebase audits, use `@review-codebase`.

This skill decides **what** to load and in **what order**. The rules themselves
live in the owning skill, `AGENTS.md`, or `docs/`. Never judge a file from memory
of a rule — load its home first.

## Workflow

### Phase 1: Discovery

Run `git diff --name-only main...HEAD` to build the file list. Classify each
file by layer and note the home that owns its rules. The paths below are the
shared layout; a project may name the infrastructure or bootstrap files a little
differently, so classify by layer rather than by an exact path.

| Path pattern | Layer | Rules live in |
|---|---|---|
| `src/services/*/repositories/*/queries/*.rs` | Repository queries | `@repositories` |
| `src/services/*/repositories/*/repository.rs` | Repository structs | `@repositories` |
| `src/services/*/repositories/*/models.rs` | Models | `@repositories` |
| `src/services/*/repositories/*/errors.rs` | Repository errors | `@repositories` |
| `src/services/*/repositories/*/registrations.rs` | Repository DI | `@repositories` |
| `migrations/**/*.sql` | Schema | `@repositories` |
| `src/services/*/service.rs` | Service | `@services` |
| `src/services/*/errors.rs` | Service errors | `@services` |
| `src/services/*/registrations.rs` | Service DI | `@services` |
| `src/services/*/events.rs` | Events | `@events` |
| `src/services/*/jobs.rs` | Jobs | `@jobs` |
| `src/services/boot.rs` | Bootstrap wiring | `AGENTS.md` → Bootstrap Wiring, `@di` |
| the infrastructure module | DI container, event/job infra | `@di`, `@events`, `@jobs` |
| `src/controllers/*.rs` | Controller | `@controllers` |
| `src/views/*.rs` | View | `@views` |
| `ts/*.ts` | TypeScript | `@views` |
| `input.css` | Custom CSS | `@views` |

### Phase 2: Per-layer review

For each layer present in the diff, load its home with the `skill` tool, read
the changed files, and check them against that home. The home is the rule; this
workflow does not restate it. A finding is a file that departs from its home.

When a changed file calls into another layer, check the call against the callee's
home too — a repository call inside a job handler is a `@jobs` question.

### Phase 3: Cross-cutting checks

Apply these across **all** changed files, regardless of layer. Each check is a
pointer to the home; load it rather than recalling it.

- **Instrumentation** — `AGENTS.md` → Logging and Instrumentation. Verify every
  function has `#[instrument]`, at the right level, with the right `skip(...)`.
- **Documentation** — `@documentation`. Verify docstrings describe the contract
  and comments explain the why.
- **Test coverage** — the Tests sections of `@repositories` and `@services`, plus
  `AGENTS.md` → Testing Conventions. Verify every new or changed query function
  and service method is tested as those sections require.

### Phase 4: Final verification

Run the `run_tests` tool — it compiles the crate and runs the full suite
(`SQLX_OFFLINE=true`), including doc tests. Format changed code by hand to
rustfmt style (4-space indent, 100 columns); there is no `cargo fmt` tool. There
is no `cargo check` tool — `run_tests` compiles. Convention compliance is
verified manually during this review.

---

## Related skills

- `@services`, `@repositories`, `@controllers`, `@views` — the rules each layer is checked against
- `@events`, `@jobs` — event and job rules
- `@di`, `AGENTS.md` — wiring, instrumentation, and project invariants
- `@review-codebase` — whole-codebase audit (not diff-driven)
